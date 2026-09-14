#!/usr/bin/env bash
#
# Per-file counts of the panic sources that reach the genesis VALUE path.
#
# WHY A SEPARATE INSTRUMENT. `clippy_critical_counts.sh` counts the lints that gate the test-conversion
# work. This counts the ones that gate totality, and mixing them would make both numbers meaningless:
# `arithmetic_side_effects` fires on every integer operation in the workspace, so its count is only
# informative when it is scoped to the files that determine the genesis block's bytes.
#
# Totality is the second half of purity: a pure function cannot panic, because a panic is an effect.
# Rust's panic sources are a closed list, and a *complete* obligation names all of them:
#
#   unwrap_used, expect_used        `Option`/`Result` unwrapping
#   panic, unreachable, todo, unimplemented
#   indexing_slicing                slice/array indexing — the one nothing in this repo guards
#   arithmetic_side_effects         integer overflow and division by zero — also unguarded
#
# NOT covered, and therefore audited by hand because no lint sees them:
#   `subtle::CtOption` unwrap/expect   clippy lints `Option`/`Result` only
#   `unsafe` blocks                    a separate obligation entirely
#
# Usage:
#   contrib/clippy_totality_counts.sh [file-list]        # default: the measured value-path set
#   contrib/clippy_totality_counts.sh --all              # every file in the scoped crates
#
# Exit status: 1 if any counted site remains in the scoped files, 0 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# The crates that contain the value path. Linting is per crate; the file filter below scopes the
# *report* to the path, since clippy has no per-file mode.
CRATES=(dwowd dwow_chain dwow-sdk dwow_native_token_contract)

# The value path, as measured by the genesis totality survey: everything between the coinbase build
# and the block hash, plus the primitives they call. Derived from the call graph, not guessed.
VALUE_PATH=(
    "bin/dwowd/src/registry/model.rs"
    "bin/dwowd/src/lib.rs"
    "src/linear/src/block.rs"
    "src/linear/src/chain_state.rs"
    "src/linear/src/miner.rs"
    "src/linear/src/transaction.rs"
    "src/linear/src/supply_chain.rs"
    "src/sdk/src/blockchain.rs"
    "src/sdk/src/crypto/pedersen.rs"
    "src/sdk/src/crypto/nullifier.rs"
    "src/sdk/src/crypto/util.rs"
    "src/sdk/src/crypto/keypair.rs"
    "src/contract/native_token/src/client/pow_reward.rs"
    "src/contract/native_token/src/model/mod.rs"
)

LINT_ARGS=(
    -W clippy::unwrap_used
    -W clippy::expect_used
    -W clippy::panic
    -W clippy::unreachable
    -W clippy::todo
    -W clippy::unimplemented
    -W clippy::indexing_slicing
    -W clippy::arithmetic_side_effects
)

scope="${1:-path}"
OUT="/tmp/clippy-totality-counts.txt"
RAW="$(mktemp)"
ERRLOG="$OUT.stderr.log"
trap 'rm -f "$RAW"' EXIT

TARGET="$(rustc -Vv | grep '^host: ' | cut -d' ' -f2)"
FAILED=()

for crate in "${CRATES[@]}"; do
    echo "clippy-totality: linting $crate" >&2
    if ! RAYON_NUM_THREADS=10 cargo clippy \
        --target="$TARGET" --release --all-features \
        -p "$crate" --lib --no-deps \
        --message-format=json -- "${LINT_ARGS[@]}" 2>>"$ERRLOG" \
    | jq -r '
        select(.reason == "compiler-message")
        | .message
        | select(.code != null)
        | select(.code.code | test("^(clippy::)?(unwrap_used|expect_used|panic|unreachable|todo|unimplemented|indexing_slicing|arithmetic_side_effects)$"))
        | .code.code as $lint
        | (.spans[] | select(.is_primary) | .file_name) as $file
        | "\($lint)\t\($file)"
      ' >> "$RAW"
    then
        FAILED+=("$crate")
    fi
done

if [ "${#FAILED[@]}" -gt 0 ]; then
    echo "clippy-totality: FAILED to lint: ${FAILED[*]} — see $ERRLOG" >&2
    echo "  counts are PARTIAL; those crates are unmeasured, not clean" >&2
    exit 2
fi

# Report per file, and — in the default mode — only for the value path.
if [ "$scope" = "--all" ]; then
    sort "$RAW" | uniq -c | sort -rn \
    | awk 'BEGIN{OFS="\t"} {n=$1; $1=""; sub(/^[ \t]+/,""); print n, $0}' > "$OUT"
else
    FILTER="$(printf '%s\n' "${VALUE_PATH[@]}" | sed 's/[].[^$*\\/]/\\&/g' | paste -sd'|' -)"
    sort "$RAW" | uniq -c | sort -rn \
    | awk 'BEGIN{OFS="\t"} {n=$1; $1=""; sub(/^[ \t]+/,""); print n, $0}' \
    | grep -E "	(${FILTER})$" > "$OUT" || true
fi

TOTAL="$(awk -F'\t' '{s+=$1} END{print s+0}' "$OUT")"

echo
echo "clippy-totality: $TOTAL panic-source hit(s) in scope ($scope) across ${#CRATES[@]} crates"
echo "  written to $OUT"
echo
awk -F'\t' '{printf "  %5s  %-28s %s\n", $1, $2, $3}' "$OUT" | head -30

if [ "$TOTAL" -ne 0 ]; then
    exit 1
fi
