#!/usr/bin/env bash
#
# Per-file counts of the test-robustness lints, across the genesis-critical crates.
#
# This is the acceptance instrument for the test-conversion work: it turns "the lint
# is clean now" from an assertion into a number, per file, which can be compared
# before and after. It is also the gate — a non-zero total exits non-zero.
#
# WHAT THE THREE NUMBERS MEAN — they do not count the same thing, and reading them as
# if they did would overstate or understate the work:
#
#   unwrap_used, expect_used   one hit per call site  (the sites to replace)
#   panic_in_result_fn         one hit per FUNCTION   (verified: its primary span is the
#                              `fn` signature, and the offending `panic!`/assert sites are
#                              secondary spans). So "0" for this lint means no
#                              Result-returning function contains a panic or an assertion
#                              — not that no assertion remains anywhere. A `()`-returning
#                              test with only assertions is untouched by it, which is
#                              exactly the scope rule the plan states.
#
# The `spans` in the JSON carry the offending sites as secondary spans; the counts below
# deliberately use the primary span so the per-file numbers stay comparable run to run.
#
# WHY NOT `make clippy`: that target lints `--workspace`, and `src/transport` carries
# 21 pre-existing production violations from a divergence that is being resolved after
# genesis and consensus are green. This script lints only the crates named below.
#
# WHY ONE INVOCATION PER CRATE: `--no-deps` is what keeps clippy off each crate's path
# dependencies, so that transport is never linted. But a crate that is *both* selected
# and a dependency of another selected crate can be skipped by `--no-deps` — which
# would silently under-report, the worst kind of instrument bug. One invocation per
# crate makes every crate a top-level selection, so nothing in scope can hide.
#
# WHY THE LINTS ARE PASSED ON THE COMMAND LINE: they are the lints this work is
# measured against, and the crate roots do not deny all of them yet. Passing `-W`
# here means the baseline can be taken *before* the deny lands, so the before/after
# numbers are comparable.
#
# Usage:
#   contrib/clippy_critical_counts.sh [output-file]
#
# Exit status: 0 if the in-scope total is zero, 1 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v jq >/dev/null 2>&1; then
    echo "clippy-critical: jq is required to count lints" >&2
    exit 2
fi

TARGET="$(rustc -Vv | grep '^host: ' | cut -d' ' -f2)"
OUT="${1:-/tmp/clippy-critical-counts.txt}"

# The genesis-critical path. Kept in one place so the gate and the plan cannot drift.
CRATES=(
    dwowd
    dwow_chain
    dwow-sdk
    dwow_deployooor_contract
    dwow_native_token_contract
    dwow_promissory_note_contract
    dwow_identity_contract
    dwow_oracle_contract
    dwow_attestation_contract
    dwow_purse_contract
    dwow_box_contract
    dwow_multisig_contract
)

LINT_ARGS=(
    -W clippy::unwrap_used
    -W clippy::expect_used
    -W clippy::panic_in_result_fn
)

RAW="$(mktemp)"
ERRLOG="${OUT}.stderr.log"
trap 'rm -f "$RAW"' EXIT

# A crate that fails to lint contributes no messages, so an unchecked failure would
# read exactly like a clean crate. `set -o pipefail` makes the pipeline carry cargo's
# status, and the crate is recorded so the run reports an instrument failure (exit 2)
# rather than a pass.
FAILED=()

for crate in "${CRATES[@]}"; do
    echo "clippy-critical: linting $crate" >&2
    if ! RAYON_NUM_THREADS=10 cargo clippy \
        --target="$TARGET" --release --all-features \
        -p "$crate" --tests --no-deps \
        --message-format=json -- "${LINT_ARGS[@]}" 2>>"$ERRLOG" \
    | jq -r '
        select(.reason == "compiler-message")
        | .message
        | select(.code != null)
        | select(.code.code | test("^(clippy::)?(unwrap_used|expect_used|panic_in_result_fn)$"))
        | .code.code as $lint
        | (.spans[] | select(.is_primary) | .file_name) as $file
        | "\($lint)\t\($file)"
      ' >> "$RAW"
    then
        FAILED+=("$crate")
    fi
done

if [ "${#FAILED[@]}" -gt 0 ]; then
    printf '%s\n' "${FAILED[@]}" > "${OUT}.lint-failed"
fi

# count <TAB> lint <TAB> file, worst first
sort "$RAW" | uniq -c | sort -rn \
| awk 'BEGIN{OFS="\t"} {n=$1; $1=""; sub(/^[ \t]+/,""); print n, $0}' > "$OUT"

TOTAL="$(awk -F'\t' '{s+=$1} END{print s+0}' "$OUT")"

echo
echo "clippy-critical: $TOTAL lint hits across ${#CRATES[@]} crates in scope"
echo "  written to $OUT"
echo
awk -F'\t' '{printf "  %5s  %-28s %s\n", $1, $2, $3}' "$OUT" | head -25

# A crate that could not be linted is unmeasured, NOT clean — so the counts above are
# partial, and saying so is the whole point of taking the exit code seriously.
if [ "${#FAILED[@]}" -gt 0 ]; then
    echo >&2
    echo "clippy-critical: FAILED to lint: ${FAILED[*]}" >&2
    echo "  list: ${OUT}.lint-failed   cargo output: $ERRLOG" >&2
    echo "  THE COUNTS ABOVE ARE PARTIAL — those crates are unmeasured, not clean." >&2
    exit 2
fi

if [ "$TOTAL" -ne 0 ]; then
    exit 1
fi
