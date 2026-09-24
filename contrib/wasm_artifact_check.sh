#!/usr/bin/env bash
#
# The artifact-level invariant: a contract wasm SHALL NOT carry panic machinery, and SHALL NOT embed
# any first-party source path.
#
# WHY THIS IS THE REAL CHECK, AND NOT THE LINTS. Two reasons, both measured:
#
#  1. `#![deny(clippy::unwrap_used)]` is a *clippy* lint. The wasm comes from `cargo build
#     --release`, which never runs clippy. And a site carrying `#[expect(clippy::unwrap_used, ...)]`
#     is silenced while still compiling a panic location into the artifact. So lint cleanliness and
#     artifact purity are related but distinct, and neither implies the other.
#  2. A panic location is `Location { file: &'static str, line: u32 }`. The file is a data-section
#     string and the line an integer; neither is debuginfo, so neither `strip` nor `debug = false`
#     removes them. That is why a *comment-only* edit has moved the genesis hash: it shifts the line
#     numbers of every panic site after it.
#
# What this checks, in the artifact itself:
#
#   - the panic machinery the compiler links in when any panic source survives:
#     `panic_bounds_check` (indexing/slicing), `panic_fmt`, `rust_begin_unwind`, and any
#     `core::panicking` reference. `panic_bounds_check` is the one that survives when the index
#     checks are not folded, and it is the marker this work drives to zero.
#   - embedded first-party source paths, which cannot appear unless a panic location references them.
#
# WHAT THIS IS NARROWER THAN, MEASURED 2026-09-24 — and why there is no second script.
#
# The two halves of the invariant sit in different sections and behave differently under the two build
# levers that were measured that day (`[profile.release] strip = "symbols"` and
# `-Zlocation-detail=none`, together they take all nine genesis contracts to CLEAN):
#
#   * the four **marker** strings live in the wasm **name section**, so `strip` removes them while
#     removing nothing about the code — demonstrated by `wasm-strip`ing a copy of
#     `dwow_multisig_contract.wasm`: DIRTY → CLEAN with the **Code hash unchanged** (`a0783ddd…`) and
#     every panic site still present. Under `strip`, this half is a *metadata* check.
#   * the **first-party paths** live in the **Data** section, and `-Zlocation-detail=none` removes them
#     for real (native_token: `pedersen.rs`, `merkle_node.rs`, `model/mod.rs`, 1 → 0 each, along with
#     five of nine sysroot locations).
#
# So CLEAN after both levers means: no marker names, and **no first-party path string in the binary**.
# The second is the substantive half and it is sufficient, which is why no companion script is needed:
# a panic `Location`'s file field is a `&'static str`, so a first-party path cannot be referenced
# unless its string is in the artifact. Absence of the string therefore implies absence of any location
# that names it — an implication in this direction only, and stated because the converse does not hold:
# a path string can appear for reasons other than a panic site, which is why this script has always
# reported paths as evidence rather than as a panic count.
#
# What CLEAN does **not** mean: that the artifact carries no panic machinery at all. It does not, and
# cannot on this toolchain — `alloc`'s `handle_alloc_error` and `core::fmt` panic by design and are
# linked into all nine, which is exactly why `identity`, `oracle`, `attestation` and `multisig` carried
# markers with **zero** first-party panic sites. `-Zlocation-detail=none` empties a location's file and
# line rather than deleting the record, so the records remain while nothing about them depends on where
# the source sat: measured, a comment inserted into `sdk/src/crypto/merkle_node.rs` changes the artifact
# without the levers (`1c89c9d4…` vs `27ad85fc…`) and does **not** change it with them (`e539f88c…`
# either way).
#
# A companion "inventory the locations" script was written on 2026-09-24 and **deleted the same day**:
# emitting the final crate's LLVM IR cannot see the *dependencies'* locations — the ones that matter
# are generated in `src/sdk` — and an artifact-side parse needs the data segment's memory offset, not
# the file offset. Rather than ship an instrument that reported "no panic locations found" against a
# known-carrying artifact, it was removed, and the sufficiency argument above is why nothing is lost.
#
# Usage:
#   contrib/wasm_artifact_check.sh <wasm> [<wasm> ...]     # check named artifacts
#   contrib/wasm_artifact_check.sh --genesis               # check all nine genesis contracts
#
# Exit status: 0 if every artifact is clean, 1 if any carries panic machinery or an embedded path.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

GENESIS=(
    deployooor native_token promissory_note identity oracle
    attestation purse box multisig
)

PANIC_MARKERS=(
    "panic_bounds_check"
    "panic_index_out_of_bounds"
    "rust_begin_unwind"
    "core::panicking"
)

if [ "${1:-}" = "--genesis" ]; then
    ARTIFACTS=()
    for c in "${GENESIS[@]}"; do
        f="src/contract/$c/dwow_${c}_contract.wasm"
        [ -f "$f" ] && ARTIFACTS+=("$f") || echo "wasm_artifact_check: MISSING $f (not built)" >&2
    done
elif [ $# -gt 0 ]; then
    ARTIFACTS=("$@")
else
    sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit 2
fi

fail=0
for w in "${ARTIFACTS[@]}"; do
    [ -f "$w" ] || { echo "wasm_artifact_check: no such file: $w" >&2; fail=1; continue; }
    [ "$(stat -c%s "$w")" -gt 0 ] || { echo "wasm_artifact_check: empty file: $w" >&2; fail=1; continue; }

    strings="$(strings -a "$w")"

    panic_hits=""
    for m in "${PANIC_MARKERS[@]}"; do
        n="$(printf '%s' "$strings" | grep -c -F "$m" || true)"
        [ "$n" -gt 0 ] && panic_hits="$panic_hits $m($n)"
    done

    paths="$(printf '%s' "$strings" | grep -oE 'src/(contract|sdk|linear)/[A-Za-z0-9_/.-]*\.rs' | sort -u)"

    if [ -n "$panic_hits" ] || [ -n "$paths" ]; then
        echo "DIRTY  $w  ($(stat -c%s "$w") bytes)"
        [ -n "$panic_hits" ] && echo "         panic machinery:$panic_hits"
        [ -n "$paths" ] && echo "         embedded paths:" && printf '%s\n' "$paths" | sed 's/^/           /'
        fail=1
    else
        echo "CLEAN  $w  ($(stat -c%s "$w") bytes)"
    fi
done

exit "$fail"
