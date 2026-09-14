#!/usr/bin/env bash
#
# The Lean genesis model against the Rust it models: does the model still describe the code?
#
# WHY THIS EXISTS. `proofs/lean/src/DarkFi/Genesis/Ceremony.lean` proves that the genesis ceremony is
# total, single-valued and pure — but only about *the model*. A proof about a model that has drifted
# from the code is worse than no proof, because it reads like assurance. The model cannot recompute
# the hash (core Lean has no blake3 or Poseidon), so the properties are proved in Lean and the
# *correspondence* is checked here, mechanically, against the source.
#
# WHAT IT CHECKS, in both directions where possible:
#   - the emission constants: height 1 at genesis, INITIAL_REWARD, TAIL_REWARD, DECAY_FP, FP_SHIFT
#   - the target: `BlockTarget::MAX` in Rust is `u32::MAX`, and the model's literal is that value
#   - the deployment table: the nine names in order, and which of them carry a manifest
#   - the clock claim: the Rust's `timestamp` in the genesis header literal is a constant, since the
#     model proves `timestamp = 0` for every input
#
# WHAT IT DOES NOT DO. It does not check the hashes, the merkle root, or the wasm bytes — those are
# data, not structure, and the model deliberately does not carry them (see the module header). It also
# does not check that the Lean *proofs* are valid: `lake build` does that, and a model with a `sorry`
# would pass this script while proving nothing, which is why the script fails on `sorry` too.
#
# Usage: contrib/genesis_model_conformance.sh
# Exit status: 1 on any mismatch, or on a `sorry`/`axiom` in the model.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODEL=proofs/lean/src/DarkFi/Genesis/Ceremony.lean
BLOCKCHAIN=src/sdk/src/blockchain.rs
GENESIS=bin/dwowd/src/lib.rs

for f in "$MODEL" "$BLOCKCHAIN" "$GENESIS"; do
    [ -f "$f" ] || { echo "genesis_model_conformance: missing $f" >&2; exit 2; }
done

FAILED=0
note() { printf '  %-46s %s\n' "$1" "$2"; }
check() { # check <description> <expected> <actual>
    if [ "$2" = "$3" ]; then note "$1" "agree (${2})"; else note "$1" "MISMATCH: model=${2} rust=${3}"; FAILED=1; fi
}

echo "genesis_model_conformance: model vs Rust"
echo

# ---- proof hygiene first: a model that proves nothing must not report agreement -------------------
if grep -nE '(^|[^a-zA-Z_])(sorry|admit)([^a-zA-Z_]|$)' "$MODEL" | grep -v '^\s*--' | grep -qv 'admits'; then
    echo "  model contains 'sorry': the proofs are incomplete, so agreement below is meaningless" >&2
    FAILED=1
fi
if grep -nE '^\s*(axiom|native_decide)' "$MODEL" > /dev/null; then
    echo "  model contains 'axiom' or 'native_decide': the claims are not proved" >&2
    FAILED=1
fi

# ---- the model's constants -----------------------------------------------------------------------
lean_num() { grep -m1 -oE "^def $1 : Nat := [0-9_]+" "$MODEL" | grep -oE '[0-9_]+$' | tr -d '_'; }
# the Rust's, with underscore separators stripped so both sides compare as plain digits
# Anchored on the declaration, so a constant named in a doc comment above it does not match.
rs_num() { grep -m1 -oE "const $1(: [A-Za-z0-9_]+)? = [A-Za-z_]*\(?[0-9_]+" "$BLOCKCHAIN" | grep -oE '[0-9_]+$' | tr -d '_'; }

check "genesis height" "$(lean_num genesisHeight)" "$(grep -m1 -oE 'GENESIS: Self = Self\([0-9]+\)' "$BLOCKCHAIN" | grep -oE '[0-9]+')"
check "INITIAL_REWARD" "$(lean_num initialReward)" "$(rs_num 'INITIAL_REWARD')"
check "TAIL_REWARD" "$(lean_num tailReward)" "$(rs_num 'TAIL_REWARD')"
check "DECAY_FP" "$(lean_num decayFp)" "$(rs_num 'DECAY_FP')"
check "FP_SHIFT" "$(lean_num fpShift)" "$(rs_num 'FP_SHIFT')"

# BlockTarget::MAX is *written* as `u32::MAX` in the Rust, so compare the model against that value.
U32MAX=4294967295
check "maxTarget (u32::MAX)" "$(lean_num maxTarget)" "$(grep -q 'pub const MAX: Self = Self(u32::MAX)' "$BLOCKCHAIN" && echo "$U32MAX" || echo "NOT-u32::MAX")"

echo

# ---- the deployment table -------------------------------------------------------------------------
# The Rust side: (wasm_bytes, manifest_bytes_or_empty, "Name") in genesis_contracts() order.
# The name is the last quoted string before the closing paren of each table entry.
rs_table=$(awk '/fn build_genesis_deployment_txs/,/^    \];/' "$GENESIS" \
    | grep -oE ', "[A-Za-z]+"\)' | sed 's/^, "//; s/")$//')
rs_names=$(printf '%s\n' "$rs_table" | paste -sd, -)
lean_names=$(grep -oE '⟨"[A-Za-z]+", (true|false)⟩' "$MODEL" | sed -E 's/⟨"([A-Za-z]+)".*/\1/' | paste -sd, -)
check "deployment names, in order" "$lean_names" "$rs_names"

rs_count=$(printf '%s\n' "$rs_table" | grep -c .)
check "deployment count" "9" "$rs_count"

# Manifest presence: the Rust entry has a non-empty second element iff the manifest bytes are named.
rs_manifests=$(awk '/fn build_genesis_deployment_txs/,/^    \];/' "$GENESIS" \
    | grep -cE ', include_bytes!\("[^"]*manifest')
lean_manifests=$(grep -cE '⟨"[A-Za-z]+", true⟩' "$MODEL")
check "contracts with a manifest" "$lean_manifests" "$rs_manifests"

echo

# ---- the clock claim ------------------------------------------------------------------------------
# The model proves `timestamp = 0` for every input. That is only a statement about the Rust if the
# Rust's genesis header literal has a constant timestamp: a call there would make the theorem a
# statement about the model alone.
# The header field is `BlockTimestamp::new(timestamp)`; the constancy is in the local. Both halves
# are checked, because a literal here and a computation there would look the same to a single grep.
ts_local=$(grep -m1 -oE 'let timestamp = [0-9]+u64;' "$GENESIS" || echo "NOT-CONSTANT")
ts_field=$(grep -m1 -oE 'timestamp: BlockTimestamp::new\(timestamp\)' "$GENESIS" || echo "NOT-A-LOCAL")
check "genesis timestamp local is a literal" "let timestamp = 0u64;" "$ts_local"
check "genesis header uses that local" "timestamp: BlockTimestamp::new(timestamp)" "$ts_field"

# The two encoding-dependent fields: the model claims they are *fixed*, so what must hold in the Rust
# is that they are literals or defaults — not which bytes they encode to.
ps=$(grep -m1 -oE 'pow_source: [A-Za-z:]+,' "$GENESIS" | sed 's/,$//')
check "genesis pow_source is a constant" "pow_source: PowSource::Native" "$ps"
fw=$(grep -m1 -oE 'fee_window_flags: [A-Za-z:_()]+,?' "$GENESIS" | sed 's/,$//')
check "genesis fee_window_flags is a constant" "fee_window_flags: FeeWindowFlags::default()" "$fw"

# The RandomX key: the model carries only "a function of height alone", so the Rust must not consult
# anything else — a clock, an RNG, or the miner identity.
rk=$(grep -m1 -oE 'randomx_key: [A-Za-z:_()]+' "$GENESIS")
check "genesis randomx_key is derived from height" "randomx_key: Miner::derive_key_from_height" "$(echo "$rk" | grep -o 'randomx_key: Miner::derive_key_from_height' || echo "$rk")"

echo
if [ "$FAILED" -eq 0 ]; then
    echo "genesis_model_conformance: model and Rust agree"
else
    echo "genesis_model_conformance: DRIFT — the model no longer describes the code" >&2
fi
exit "$FAILED"
