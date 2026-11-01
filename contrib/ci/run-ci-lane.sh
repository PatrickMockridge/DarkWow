#!/bin/bash
# DarkWow CI lane entrypoint.
#
# One repo-owned executor for the GitHub workflow. The umbrella
# (`scripts/run-all-tests.sh`) remains the gate inventory; its `--ci-lane/--groups` selector reads
# `contrib/ci/gate-groups.tsv`, so this script does not duplicate gate commands. Only the checks the
# umbrella does not carry (the fast proof control, the L1.5 control, the heavyweight/zk-audit/Docker
# nightly jobs, and the CI self-tests) are defined here; the coverage ratchet knows them as
# `fast-only` rows.
#
# Lanes:
#   fast-static --groups G,...   instruments selected by path; tracker is advisory, no verdict
#   fast-compile                 pre-build guard for dwowd/wallet/32 contracts on wasm32
#   fast-models                  the Python models (conditional on path)
#   fast-lean                    Lean freshness/build (conditional on path)
#   fast-proof                   one exact production-path proof test
#   merge                        tracker-gated lightweight Tier 1 + L1.5
#   nightly-heavyweight          nine genesis heavyweight tests
#   nightly-zk                   zk_audit
#   nightly-docker               Docker E2E pipeline
#   nightly-controls             the cheap safety set + all controls
#   cost                         the nightly cost report
#
# --dry-run prints the commands without executing cargo, Lean or Docker.
# --list-gates prints the umbrella + fast-only inventories.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=lib/gate.sh
source "$SCRIPT_DIR/lib/gate.sh"
# shellcheck source=lib/heavy.sh
source "$SCRIPT_DIR/lib/heavy.sh"

export RAYON_NUM_THREADS="${RAYON_NUM_THREADS:-10}"
export RUST_MIN_STACK="${RUST_MIN_STACK:-67108864}"

CI_TEST_SKIP="-- --skip test_all_harnesses_zk_coverage --skip test_heavyweight_ --skip test_relayer_lifecycle_heavyweight"

case "${1:-}" in
    --list-gates) exec python3 -B "$SCRIPT_DIR/gate_catalog.py" --list ;;
    --self-test)
        python3 -B "$SCRIPT_DIR/select_groups.py" --self-test || exit 1
        python3 -B "$SCRIPT_DIR/check-gate-coverage.py" --self-test || exit 1
        for f in "$SCRIPT_DIR"/*.sh "$SCRIPT_DIR"/lib/*.sh; do bash -n "$f" || exit 1; done
        echo "SELF-TEST PASS: lane entrypoint, selector and ratchet controls"
        exit 0 ;;
esac
CI_GROUPS_ARG=""
CI_DRY_RUN=0
CI_ALL=0
LANE="${1:-}"
[ $# -gt 0 ] && shift

while [ $# -gt 0 ]; do
    case "$1" in
        --groups) CI_GROUPS_ARG="${2:-}"; shift 2 ;;
        --groups=*) CI_GROUPS_ARG="${1#*=}"; shift ;;
        --all) CI_ALL=1; shift ;;
        --dry-run) CI_DRY_RUN=1; shift ;;
        --list-gates)
            exec python3 -B "$SCRIPT_DIR/gate_catalog.py" --list
            ;;
        --self-test)
            "$0" --list-gates >/dev/null || exit 1
            python3 -B "$SCRIPT_DIR/select_groups.py" --self-test || exit 1
            python3 -B "$SCRIPT_DIR/check-gate-coverage.py" --self-test || exit 1
            echo "SELF-TEST PASS: lane entrypoint, selector and ratchet controls"
            exit 0
            ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done

export CI_LANE="${LANE:-local}"
export CI_DRY_RUN
mkdir -p reports/ci

if [ "$CI_ALL" = "1" ] && [ -z "$CI_GROUPS_ARG" ]; then
    CI_GROUPS_ARG="docs,contracts,shared-sdk,genesis,consensus,wallet,lean,models,ci,docker"
fi

_fast_static_groups() {
    # Models, Lean and Docker have their own jobs; running their umbrella gates here would duplicate
    # them. `always` is unconditional.
    local selected="always"
    local group
    for group in $(printf '%s' "$CI_GROUPS_ARG" | tr ',' ' '); do
        case "$group" in
            ""|models|lean|docker) continue ;;
            *) selected="$selected,$group" ;;
        esac
    done
    printf '%s' "$selected"
}

umbrella() {
    # umbrella LANE GROUPS [extra args...]
    local lane="$1" groups="$2"; shift 2
    local args=(--ci-lane "$lane" --groups "$groups" --tier 1)
    [ "$CI_DRY_RUN" = "1" ] && args+=(--dry-run)
    bash "$REPO_ROOT/scripts/run-all-tests.sh" "${args[@]}" "$@"
}

run_umbrella() {
    # The umbrella's internal gates are its own summary; fold its single exit status into this
    # lane's summary so a run of 100 pass lines cannot read as "4 passed" at the lane level.
    local lane="$1" groups="$2"; shift 2
    local status=0
    umbrella "$lane" "$groups" "$@" || status=$?
    [ "$CI_DRY_RUN" = "1" ] && return 0
    if [ "$status" -eq 0 ]; then
        GATE_PASSED=$((GATE_PASSED + 1))
        GATE_RESULTS+=("PASS  umbrella selected gates (${lane}: ${groups:-all})")
    else
        record_failure "umbrella selected gates (${lane}: ${groups:-all})" "$status"
    fi
}

tracker_probe() {
    local output status
    output="$(bash "$REPO_ROOT/scripts/check-tracker-settled.sh" --require-clean 2>&1)"
    status=$?
    printf '%s\n' "$output"
    return "$status"
}

blocked_summary() {
    local lane="$1"
    echo ""
    echo "BLOCKED: ${lane} lane not run."
    echo "This is not a test failure; it is the verification-surface rule:"
    echo "OPEN/BLOCKED/DEFERRED are undecided; CLOSED and RESIDUAL are the only exits."
    echo "::error title=Verification blocked::tracker has an undecided finding"
}

preflight_or_block() {
    local lane="$1" output status
    output="$(tracker_probe)" && return 0
    status=$?
    if [ "$CI_DRY_RUN" = "1" ]; then
        printf '%s\n' "$output"
        echo "[dry-run] the ${lane} lane would be BLOCKED by the tracker preflight"
        return 0
    fi
    printf '%s\n' "$output"
    blocked_summary "$lane"
    exit 78
}

ci_bash_syntax() {
    local ok=0 f
    for f in "$SCRIPT_DIR"/*.sh "$SCRIPT_DIR"/lib/*.sh; do
        [ -f "$f" ] || continue
        bash -n "$f" || ok=1
    done
    return "$ok"
}

fast_static() {
    local groups; groups="$(_fast_static_groups)"
    run_umbrella fast "$groups" --tier 1

    # CI-only checks, cheap and unconditional.
    gate "fast-only: gate coverage ratchet" python3 -B "$SCRIPT_DIR/check-gate-coverage.py"
    gate "fast-only: path selector self-test" python3 -B "$SCRIPT_DIR/select_groups.py" --self-test
    gate "fast-only: CI scripts parse (bash -n)" ci_bash_syntax
    if printf '%s' "$CI_GROUPS_ARG" | grep -qE '(^|,)docs(,|$)'; then
        gate "fast-only: repo doc links resolve" bash scripts/check-repo-doc-links.sh --check
    fi

    local tracker_out tracker_status
    tracker_out="$(tracker_probe)"; tracker_status=$?
    if [ "$tracker_status" -ne 0 ]; then
        echo ""
        echo "TRACKER STATE: RED (advisory, no verdict)"
        printf '%s\n' "$tracker_out" | sed 's/^/  /'
        echo "HEAVY LANES: BLOCKED until the tracker is settled."
        echo "::warning title=Tracker advisory::instruments only; no verification verdict was issued"
    else
        echo ""
        echo "TRACKER STATE: GREEN"
    fi
    ci_summary
}

proof_control() {
    # A fresh checkout has no zkas binary and no .zk.bin (.zk.bin is untracked); the test embeds
    # them via include_bytes!, so build both first. The test itself is one exact case: the whole
    # binary contains known-red create_tender cases.
    ( cd "$REPO_ROOT" && make zkas ) || return 1
    ./scripts/build-contract-zk.sh || return 1
    cargo test --release -p dwow-contract-test-harness \
        --test client_proof_self_verification -- \
        slot_commit_bet_proof_verifies_against_its_own_circuit --exact --test-threads=1
}

fast_proof() {
    [ "$CI_DRY_RUN" = "1" ] || ci_heavy_lock || return 78
    gate "fast-only: proof control: slot commit_bet self-verification" proof_control
    ci_summary
}

merge_lane() {
    preflight_or_block merge
    [ "$CI_DRY_RUN" = "1" ] || ci_heavy_lock || exit 78

    # A direct push has no PR fast lane, so the classification ratchet runs here too: a new
    # unclassified gate must not be silently skipped in the merge lane.
    gate "fast-only: gate coverage ratchet" python3 -B "$SCRIPT_DIR/check-gate-coverage.py"

    # make test builds the contract artifacts every downstream gate needs. It runs under this
    # lane's scope; the umbrella is then told to skip it so Lean is not wrapped in a second scope.
    gate "fast-only: merge lightweight make test" \
        ci_heavy_scope "${CI_MERGE_MEMORY_MAX:-20G}" \
        bash -c "cd '$REPO_ROOT' && env TEST_ARGS='$CI_TEST_SKIP' make test"

    CI_SKIP_MAKE_TEST=1 run_umbrella merge "" --tier 1

    gate "fast-only: repo doc links resolve" bash scripts/check-repo-doc-links.sh --check
    gate "fast-only: merge L1.5 production-path control" \
        ci_heavy_scope "${CI_L15_MEMORY_MAX:-20G}" \
        cargo test --release -p dwowd --lib -- \
        test_wallet_coinbase_scan_only test_canonical_call_failure_rejects_block \
        test_merge_mined_block_acceptance test_merge_mined_block_deterministic \
        test_wallet_capability_scan
    ci_summary
}

nightly_common() {
    preflight_or_block "$1"
    [ "$CI_DRY_RUN" = "1" ] || ci_heavy_lock || exit 78
}

nightly_heavyweight() {
    nightly_common nightly-heavyweight
    gate "fast-only: nightly nine-genesis heavyweight" \
        ci_heavy_scope "${CI_HEAVYWEIGHT_MEMORY_MAX:-18G}" \
        env DWOW_TEST_LOGS=1 bash bin/dwowd/src/tests/heavyweight.sh \
        --deployooor --native-token --promissory-note --identity --oracle \
        --attestation --purse --box --multisig --nocapture
    ci_summary
}

nightly_zk() {
    nightly_common nightly-zk
    gate "fast-only: nightly zk_audit" \
        ci_heavy_scope "${CI_ZK_MEMORY_MAX:-24G}" \
        cargo test --release -p dwow-contract-test-harness --test zk_audit \
        -- --nocapture --test-threads=1
    ci_summary
}

nightly_docker() {
    nightly_common nightly-docker
    gate "Docker test pipeline (native, 2 wallets)" \
        ci_heavy_scope "${CI_DOCKER_MEMORY_MAX:-8G}" \
        bash contrib/docker/darkwow-testnet/test_pipeline.sh --mode native --with-wallet 2
    ci_summary
}

nightly_controls() {
    run_umbrella nightly always --tier 1
    ci_summary
}

cost_report() {
    python3 -B "$SCRIPT_DIR/cost-report.py"
}

case "$LANE" in
    fast-static) fast_static ;;
    fast-compile) run_umbrella fast shared-sdk --tier 1; ci_summary ;;
    fast-models) run_umbrella fast models --tier 1; ci_summary ;;
    fast-lean) run_umbrella fast lean --tier 1; ci_summary ;;
    fast-proof) fast_proof ;;
    merge) merge_lane ;;
    nightly-heavyweight) nightly_heavyweight ;;
    nightly-zk) nightly_zk ;;
    nightly-docker) nightly_docker ;;
    nightly-controls) nightly_controls ;;
    cost) cost_report ;;
    *)
        echo "Usage: $0 {fast-static|fast-compile|fast-models|fast-lean|fast-proof|merge|nightly-heavyweight|nightly-zk|nightly-docker|nightly-controls|cost} [--groups G,...] [--all] [--dry-run] [--list-gates] [--self-test]" >&2
        exit 2 ;;
esac
