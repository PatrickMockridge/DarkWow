# shellcheck shell=bash
# Gate runner for the CI lanes.
#
# The umbrella (`scripts/run-all-tests.sh`) remains the single inventory of checks; this library is
# the CI executor. It mirrors `run_gate`'s semantics — pass = exit 0, fail = any non-zero — and
# records the same measured cost the umbrella records (`reports/gate-times.tsv`). A label runs at
# most once per lane even when several selected groups name it.
#
# Sourced by `contrib/ci/run-ci-lane.sh` and the group scripts under `contrib/ci/gates/`. It must
# not be executed directly.

CI_REPO_ROOT="${CI_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
CI_LANE="${CI_LANE:-local}"
CI_DRY_RUN="${CI_DRY_RUN:-0}"
CI_COMMIT="${CI_COMMIT:-$(git -C "$CI_REPO_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)}"
CI_TIMING_FILE="$CI_REPO_ROOT/reports/ci/gate-times-$CI_LANE.tsv"
CI_METRICS_FILE="${CI_METRICS_FILE:-/var/cache/darkwow/metrics/gate-times.tsv}"

GATE_PASSED=0
GATE_FAILED=0
GATE_SKIPPED=0
GATE_FAILED_LABELS=()
GATE_RESULTS=()
declare -A GATE_SEEN=()

_ci_now() { date -Is; }

_ci_record() {
    local label="$1" elapsed="$2" status="$3"
    mkdir -p "$(dirname "$CI_TIMING_FILE")"
    printf '%s\t%s\t%s\t%s\n' "$(_ci_now)" "$label" "$elapsed" "$status" >> "$CI_TIMING_FILE"
    if [ -n "$CI_METRICS_FILE" ] && mkdir -p "$(dirname "$CI_METRICS_FILE")" 2>/dev/null; then
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$(_ci_now)" "$CI_COMMIT" "$CI_LANE" "$label" \
            "$elapsed" "$status" >> "$CI_METRICS_FILE" 2>/dev/null || true
    fi
}

# run_gate LABEL COMMAND...
run_gate() {
    local label="$1"; shift
    if [ -n "${GATE_SEEN[$label]:-}" ]; then
        GATE_SKIPPED=$((GATE_SKIPPED + 1))
        return 0
    fi
    GATE_SEEN[$label]=1

    if [ "$CI_DRY_RUN" = "1" ]; then
        printf '[dry-run] %s :: %s\n' "$label" "$*"
        return 0
    fi

    echo ""
    echo "=== ${label} ==="
    local started=$SECONDS status=0
    "$@" || status=$?
    local elapsed=$((SECONDS - started))
    _ci_record "$label" "$elapsed" "$status"
    if [ "$status" -eq 0 ]; then
        echo "PASS: ${label}"
        GATE_PASSED=$((GATE_PASSED + 1))
        GATE_RESULTS+=("PASS  ${label}")
    else
        echo "FAIL: ${label} (exit ${status})"
        GATE_FAILED=$((GATE_FAILED + 1))
        GATE_FAILED_LABELS+=("$label")
        GATE_RESULTS+=("FAIL  ${label} (exit ${status})")
    fi
    return 0
}

# `gate` is the name the lane entrypoint and the catalog use; `run_gate` mirrors the umbrella's
# own function name. One implementation, two spellings.
gate() { run_gate "$@"; }

# run_advisory LABEL COMMAND... — run and report, but never fail the lane. Used for the tracker
# probe on the fast lane: the tracker is a verification-surface gate, not an instrument, and while
# a row is undecided the fast lane is explicitly not a verdict.
run_advisory() {
    local label="$1"; shift
    if [ "$CI_DRY_RUN" = "1" ]; then
        printf '[dry-run] advisory %s :: %s\n' "$label" "$*"
        return 0
    fi
    echo ""
    echo "=== ${label} (advisory) ==="
    local started=$SECONDS status=0
    "$@" || status=$?
    local elapsed=$((SECONDS - started))
    _ci_record "$label" "$elapsed" "$status"
    if [ "$status" -eq 0 ]; then
        echo "ADVISORY: ${label} — GREEN"
        GATE_RESULTS+=("ADVISORY  ${label} — GREEN")
    else
        echo "ADVISORY: ${label} — RED (no verdict)"
        GATE_RESULTS+=("ADVISORY  ${label} — RED (no verdict)")
    fi
    return 0
}

# skip LABEL REASON — record an explicit skip so a lane's summary cannot read as coverage.
skip_gate() {
    local label="$1" reason="${2:-not selected}"
    GATE_SKIPPED=$((GATE_SKIPPED + 1))
    GATE_RESULTS+=("SKIP  ${label} (${reason})")
}

# record_failure LABEL STATUS — fold a command that manages its own execution (the umbrella, a
# multi-gate lane step) into the same summary without wrapping it in another run.
record_failure() {
    local label="$1" status="${2:-1}"
    GATE_FAILED=$((GATE_FAILED + 1))
    GATE_FAILED_LABELS+=("$label")
    GATE_RESULTS+=("FAIL  ${label} (exit ${status})")
}

ci_summary() {
    echo ""
    echo "========================================"
    echo "CI lane ${CI_LANE}: ${GATE_PASSED} passed, ${GATE_FAILED} failed, ${GATE_SKIPPED} skipped"
    echo ""
    local result
    for result in "${GATE_RESULTS[@]}"; do
        echo "  ${result}"
    done
    if [ "$GATE_FAILED" -gt 0 ]; then
        echo ""
        echo "Failed gates: ${GATE_FAILED_LABELS[*]}"
    fi
    echo "========================================"
    [ "$GATE_FAILED" -eq 0 ]
}
