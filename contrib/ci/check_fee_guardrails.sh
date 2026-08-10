#!/bin/bash
# check_fee_guardrails.sh — Fee system invariant guardrails
# Runs on every push. Enforces fee-spec.md §13 (Active Guardrails)
# and fee-spec.md §14 (Fee System Invariants).
#
# Each check maps to a specific FI- invariant or SPEC- guardrail.
# Zero output on success; explicit FAIL message on violation.
set -euo pipefail

FAILED=0
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

echo "=== Fee System Guardrails ==="

# ── FI-GEN-2: No compile-time fee constants of consensus domain types ──
echo -n "[FI-GEN-2] No const/static of consensus domain types... "
CONSENSUS_TYPES="FeeAmount|CongestionFactor|RiskFactor|BlockCharge|SupplyAmount|ThresholdAmount|CfValue|WasmKb"
VIOLATIONS=$(grep -rn "const.*\($CONSENSUS_TYPES\)\|static.*\($CONSENSUS_TYPES\)" \
    "$ROOT"/src/ "$ROOT"/bin/ "$ROOT"/crates/ --include="*.rs" 2>/dev/null | \
    grep -v "SCALE\|RISK_FACTOR_SCALE\|ZERO\|IDENTITY\|BASELINE_STORAGE\|//\|///" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md FI-GEN-2: No compile-time constants of consensus domain types."
    FAILED=1
else
    echo "PASS"
fi

# ── SPEC-3: No bare magic fee numbers in production paths ──
echo -n "[FI-GEN-2] No bare magic fee constants (1_001_000, 42_000_000)... "
MAGIC_FEES="1_001_000\|42_000_000"
# Collect file:line matches, then filter: keep only files that do NOT
# contain #[cfg(test)] or mod tests (test-only files are exempt).
VIOLATIONS=""
for match in $(grep -rln "$MAGIC_FEES" \
    "$ROOT"/bin/dwowd/src/ "$ROOT"/bin/dww/src/ "$ROOT"/src/linear/src/ \
    --include="*.rs" 2>/dev/null); do
    # Skip test files: in */tests/ directory or contain #[cfg(test)]
    if echo "$match" | grep -q '/tests/' 2>/dev/null; then
        continue
    fi
    if grep -q '#\[cfg(test)\]' "$match" 2>/dev/null; then
        continue
    fi
    # In non-test files, find lines with magic numbers,
    # excluding comments, deprecations, and named constants
    VIOLATIONS="$VIOLATIONS$(grep -n "$MAGIC_FEES" "$match" 2>/dev/null | \
        grep -v "//\|///\|\*\|BUG\|FIXME\|TODO\|#[deprecated]\|DECLARATIVE_CHARGE\|MIN_FEE_ESTIMATE" || true)"
done
VIOLATIONS=$(echo "$VIOLATIONS" | grep -v '^$' || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md SPEC-1: All fee values SHALL be genesis-initialized and window-updated."
    FAILED=1
else
    echo "PASS"
fi

# ── FI-ENCRYPT-3: No silent fallback to estimate on decrypt failure ──
echo -n "[FI-ENCRYPT-3] No .unwrap_or() on fee decrypt paths... "
VIOLATIONS=$(grep -rn "decrypt_fee.*unwrap_or\|decrypt_fee.*unwrap_or_else" \
    "$ROOT"/bin/ "$ROOT"/src/ --include="*.rs" 2>/dev/null | \
    grep -v "test\|cfg(test)\|//\|///" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md FI-ENCRYPT-3: No silent decryption fallback."
    FAILED=1
else
    echo "PASS"
fi

# ── FI-RISK-6: No static RISK_FACTOR_* constants or risk_factor() function ──
echo -n "[FI-RISK-6] No static risk factor classification... "
VIOLATIONS=$(grep -rn "RISK_FACTOR_GENESIS\|RISK_FACTOR_ATTESTED\|RISK_FACTOR_SELF\|RISK_FACTOR_UNKNOWN\|fn risk_factor(" \
    "$ROOT"/src/sdk/src/manifest.rs 2>/dev/null | \
    grep -v "//\|///\|FI-RISK-6\|#[deprecated]" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md FI-RISK-6: Manifest SHALL NOT declare risk factors."
    FAILED=1
else
    echo "PASS"
fi

# ── SPEC-4: No feature gate on consensus-critical fee paths ──
echo -n "[SPEC-4] No #[cfg(feature = \"fee-window\")] in consensus paths... "
VIOLATIONS=$(grep -rn '#\[cfg.*feature.*fee.window' \
    "$ROOT"/src/ "$ROOT"/bin/ --include="*.rs" 2>/dev/null | \
    grep -v "test\|cfg(test)" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md SPEC-4: No feature gates on consensus-critical fee code."
    FAILED=1
else
    echo "PASS"
fi

# ── SPEC-6: No try_lock().unwrap_or(0) in congestion measurement ──
echo -n "[SPEC-6] No try_lock congestion measurement... "
VIOLATIONS=$(grep -rn "try_lock.*unwrap_or(0)" \
    "$ROOT"/crates/dwow-mempool/src/lib.rs 2>/dev/null | \
    grep -v "test\|cfg(test)\|//\|///\|TODO\|FIXME" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md SPEC-6: Congestion measurement SHALL be accurate under load."
    FAILED=1
else
    echo "PASS"
fi

# ── Check 7: No unwrap_or on accumulator reads (§A.3.4) ──
echo -n "[Check-7] No unwrap_or(Identity) on accumulator reads... "
VIOLATIONS=$(grep -rn "unwrap_or.*identity\|unwrap_or.*Identity" \
    "$ROOT"/src/contract/native_token/src/entrypoint/ 2>/dev/null | \
    grep -v "test\|cfg(test)\|//\|///\|\.inner()" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  contract-wasm-type-system.md §A.3.4: No unwrap_or on sled reads."
    FAILED=1
else
    echo "PASS"
fi

# ── Check 8: No raw [0u8; 32] written to accumulator key (FI-COLLECT-5) ──
echo -n "[Check-8] No raw [0u8; 32] at accumulator key... "
VIOLATIONS=$(grep -rn "FEE_COMMIT_ACCUMULATOR.*\[0u8; 32\]" \
    "$ROOT"/src/contract/native_token/src/ 2>/dev/null | \
    grep -v "test\|cfg(test)\|//\|///" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  fee-spec.md FI-COLLECT-5: Use write_accumulator(), not raw [0u8; 32]."
    FAILED=1
else
    echo "PASS"
fi

# ── Check 9: AccumulatorPoint::decode is the only decoder ──
echo -n "[Check-9] AccumulatorPoint type exists... "
if grep -q "pub struct AccumulatorPoint" \
    "$ROOT"/src/contract/native_token/src/model/mod.rs 2>/dev/null; then
    echo "PASS"
else
    echo "FAIL"
    echo "  fee-spec.md §5.6.2.1: AccumulatorPoint nominal type must exist."
    FAILED=1
fi

# ── Check 10: No raw data[0] FeeV2 routing in wallet (C-1/C-2) ──
echo -n "[Check-10] No vec![0x08u8] fee construction... "
VIOLATIONS=$(grep -rn "vec!\[0x08u8\]" \
    "$ROOT"/bin/dww/src/ 2>/dev/null | grep -v "test\|//\|///\|TODO\|FIXME\|C-1" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  type-system.md §8.4: Use MassBalanceFeeV2CallData::encode(), not raw bytes."
    FAILED=1
else
    echo "PASS"
fi

# ── Check 11: No raw data[0]==0x08 dispatch in production paths ──
echo -n "[Check-11] No raw data[0] FeeV2 dispatch... "
VIOLATIONS=$(grep -rn "data.first().*0x08\|data\[0\].*0x08" \
    "$ROOT"/bin/ "$ROOT"/src/linear/ 2>/dev/null | \
    grep -v "test\|cfg(test)\|//\|///\|TODO\|C-1\|C-2\|C-3\|MassBalanceFeeV2Selector\|SELECTOR" || true)
if [ -n "$VIOLATIONS" ]; then
    echo "FAIL"
    echo "$VIOLATIONS"
    echo "  type-system.md §10.5: Use MassBalanceFeeV2CallData::from_bytes(), not data[0]."
    FAILED=1
else
    echo "PASS"
fi

# ── Check 12: RiskFactor, WasmKb, CfValue, ThresholdAmount, EstimatedFee exist ──
echo -n "[Check-12] Nominal fee types exist (RiskFactor, WasmKb, ThresholdAmount, EstimatedFee, CfValue)... "
MISSING=""
grep -q "pub struct RiskFactor" "$ROOT"/src/sdk/src/blockchain.rs 2>/dev/null || MISSING="$MISSING RiskFactor"
grep -q "pub struct WasmKb" "$ROOT"/src/sdk/src/blockchain.rs 2>/dev/null || MISSING="$MISSING WasmKb"
grep -q "pub struct ThresholdAmount" "$ROOT"/src/sdk/src/blockchain.rs 2>/dev/null || MISSING="$MISSING ThresholdAmount"
grep -q "pub struct EstimatedFee" "$ROOT"/src/sdk/src/blockchain.rs 2>/dev/null || MISSING="$MISSING EstimatedFee"
grep -q "pub struct CfValue" "$ROOT"/src/linear/src/fee_window.rs 2>/dev/null || MISSING="$MISSING CfValue"
if [ -n "$MISSING" ]; then
    echo "FAIL — missing:$MISSING"
    echo "  type-system.md §2.3.1: All consensus numeric domains SHALL be nominal types."
    FAILED=1
else
    echo "PASS"
fi

# ── Check 13: decrypt_fee_for_miner returns Result<FeeAmount, _> not Result<u64, _> ──
echo -n "[Check-13] decrypt_fee_for_miner returns FeeAmount, not u64... "
if grep -q "Result<FeeAmount" "$ROOT"/bin/dwowd/src/lib.rs 2>/dev/null; then
    echo "PASS"
else
    echo "FAIL"
    echo "  fee-spec.md H-3: decrypt_fee_for_miner SHALL return Result<FeeAmount, FeeDecryptError>."
    FAILED=1
fi

# ── Check 14: No deprecated compute_fee(gas) in SDK ──
echo -n "[Check-14] No deprecated compute_fee(gas_units) in SDK... "
if grep -q "pub fn compute_fee(gas_units" "$ROOT"/src/sdk/src/blockchain.rs 2>/dev/null; then
    echo "FAIL"
    echo "  M-9: Deprecated compute_fee(gas_units) SHALL be removed."
    FAILED=1
else
    echo "PASS"
fi

# ── Summary ──
echo ""
if [ "$FAILED" -eq 1 ]; then
    echo "=== GUARDRAILS FAILED ==="
    exit 1
else
    echo "=== GUARDRAILS PASSED ==="
    exit 0
fi
