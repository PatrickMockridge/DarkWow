#!/usr/bin/env bash
#
# Per-contract counts of bare `as` casts that narrow a length or a count.
#
# WHY A SEPARATE INSTRUMENT. Two specs already forbid this, and neither is enforced:
#
#   contract-wasm-type-system.md §A.4.5
#     "try_from SHALL be used at every width conversion. Bare `as` casts SHALL NOT
#      appear at the FFI boundary. A value that does not fit in the target type SHALL
#      be a ContractError, not a silent truncation."
#   type-system.md §2.3
#     "A bare `as` cast on any consensus quantity (height, amount, supply) SHALL NOT
#      pass review."
#
# The only mechanical guard is Cargo.toml's `cast_possible_truncation = "warn"`, which
# is why 207 `len()/count as u8` sites accumulated across the 32 contracts. This script
# is the missing gate, and §A.9.3 I12 is its home in the invariant checklist.
#
# WHAT IT COUNTS. A `as` whose source is a `.len()` or a `*_count`/`count` binding —
# i.e. a length or a count, the quantities §A.3.1.1 requires to be the nominal
# `SerializedLen` at a fixed `u32`. It does NOT count enum/bool discriminant casts
# (`Function::X as u8`, `x.is_some() as u8`), which are mechanically safe and number
# in the hundreds; counting them would make the number meaningless, which is the same
# reason `clippy_totality_counts.sh` exists separately from `clippy_critical_counts.sh`.
#
# WHY IT MATTERS. A proof is kilobytes. A `u8` prefix truncates, every field after it
# mis-reads, and the failure surfaces only as `ContractError::IoError("Unknown")` with
# the cause irretrievably lost — the failure mode §A.3.1.2 already records.
#
# Usage:
#   contrib/length_cast_counts.sh [--detail]     # default: per-contract counts
#
# Exit status: 1 if any counted site remains in the scoped files, 0 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DETAIL=false
[[ "${1:-}" == "--detail" ]] && DETAIL=true

# A length or a count being NARROWED. `as usize`/`as u64` from a `len()` are widening
# (identity on 64-bit) and are deliberately not counted — a gate that fires on harmless
# lines gets switched off, which is the failure mode this instrument exists to avoid.
#
# Two detectable shapes:
#   1. `<expr>.len() as <narrow int>`
#   2. a local named `*_count`/`count` bound from a `.len()` and then cast
#
# NOT detectable by grep, and therefore covered by review under §A.9.3 I12: a local bound
# from a length under another name (`let n = v.len(); .. n as u8`). promissory_note
# model/mod.rs:1026 is the known instance.
PATTERN='(\.len\(\)|_count|count) as (u8|u16|u32)'

TOTAL=0
declare -a FAILING=()

for dir in src/contract/*/; do
    name="$(basename "$dir")"
    [[ -d "$dir/src" ]] || continue

    hits="$(grep -rInE "$PATTERN" "$dir/src" --include='*.rs' 2>/dev/null || true)"
    [[ -z "$hits" ]] && continue

    count="$(printf '%s\n' "$hits" | grep -c . )"
    TOTAL=$((TOTAL + count))
    FAILING+=("$name")

    if $DETAIL; then
        printf '\n%s (%s)\n' "$name" "$count"
        printf '%s\n' "$hits" | sed 's/^/    /'
    else
        printf '%-20s %s\n' "$name" "$count"
    fi
done

echo
echo "length/count `as` casts remaining: $TOTAL across ${#FAILING[@]} contracts"

if [[ $TOTAL -gt 0 ]]; then
    cat <<'EOF'

These SHALL be the nominal `SerializedLen` (contract-wasm-type-system.md §A.3.1.1),
constructed with `SerializedLen::try_from_len(len)?` — never `as`. The width is fixed
at `u32` for the whole system. See §A.9.3 I12 and type-system.md §2.3.

Re-run with --detail for file:line and the expression.
EOF
    exit 1
fi

echo "no length/count narrowing remains"
exit 0
