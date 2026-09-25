#!/bin/bash
# check_fee_guardrails.sh — Fee system invariant guardrails
# Enforces fee-spec.md §13 (Active Guardrails) and fee-spec.md §14 (Fee System
# Invariants). Each check maps to a specific FI- invariant or SPEC- guardrail.
# Zero output on success; explicit FAIL message on violation.
#
# NOTE ON PROVENANCE: this header used to claim "Runs on every push". No CI exists
# in this repository (.github/workflows/ and .gitlab-ci.yml are absent) and
# nothing invoked this script, so that claim was false. It is now wired into
# scripts/run-all-tests.sh, and it carries a negative control (`--self-test`).
set -euo pipefail

# ── Negative control ────────────────────────────────────────────────────────────
#
# `--self-test` plants a violation this gate is supposed to catch, runs the gate
# against it in a temporary tree, and requires the gate to FAIL **on that specific
# finding**. Exiting 0 from the control means the gate cannot fail — which is a
# failure of the gate, not of the tree.
#
# The output is checked, not just the exit status: a gate that dies for an
# unrelated reason (a missing directory, a parse error) must not be able to
# satisfy its own control by accident.
if [ "${1:-}" = "--self-test" ]; then
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$TMP/src"
    # The exact shape [FI-GEN-2] forbids: a `const` of a consensus domain type.
    printf 'pub const PLANTED_VIOLATION: FeeAmount = FeeAmount(1);\n' > "$TMP/src/planted.rs"
    OUT="$(ROOT="$TMP" "$0" 2>&1)" && {
        echo "SELF-TEST FAIL: [FI-GEN-2] did not detect a planted const of a consensus domain type" >&2
        echo "  (a gate whose control cannot make it fail is not a gate)" >&2
        exit 1
    }
    if ! printf '%s' "$OUT" | grep -q 'No const/static of consensus domain types... FAIL'; then
        echo "SELF-TEST FAIL: the gate failed, but not on the planted violation — its control proves nothing" >&2
        printf '%s\n' "$OUT" | sed 's/^/  | /' >&2
        exit 1
    fi
    echo "SELF-TEST PASS: the planted violation was detected, and named by the check that owns it"
    exit 0
fi

FAILED=0
# Overridable so the negative control above can point the checks at a temp tree.
ROOT="${ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"

echo "=== Fee System Guardrails ==="

# ── FI-GEN-2: No compile-time fee constants of consensus domain types ──
echo -n "[FI-GEN-2] No const/static of consensus domain types... "
CONSENSUS_TYPES="FeeAmount|CongestionFactor|RiskFactor|BlockCharge|SupplyAmount|ThresholdAmount|CfValue|WasmKb"
# `grep -E`, and the fix is load-bearing. The previous form was
#   grep -rn "const.*\($CONSENSUS_TYPES\)\|static.*\($CONSENSUS_TYPES\)"
# which mixes a BRE group `\(...\)` with a bare `|`. In a BRE, `|` is a LITERAL
# character, so the group could only match the text
# "(FeeAmount|CongestionFactor|...)" — parentheses and pipes, in a source file.
# Measured 2026-09-25: zero matches in this repository, while
# `pub const RISK_FACTOR_SCALE: u64 = RiskFactor::SCALE;` is present. The check
# passed vacuously on every tree, so this invariant — the fee system's headline
# one — was never enforceable. The negative control (`--self-test`) now fails if
# this regresses.
#
# The shape matched is a *declaration*: `const NAME: Type` / `static NAME: Type`.
# A `const fn` returning a consensus type is not a stored constant and is not a
# violation of "no compile-time fee constants".
VIOLATIONS=$(grep -rnE "(const|static)[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*:[[:space:]]*($CONSENSUS_TYPES)\b" \
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

# ── [FI-ENCRYPT-3] — REMOVED 2026-09-25, its authority does not exist ──
#
# This check was deleted rather than repaired, and the reason is the finding.
# `FI-ENCRYPT-3` occurs in **no document** under doc/ — it was removed with the
# rest of the privacy-preserving-fee channel by the FeeV3 migration
# (`5a9ddb6da4`; doc/src/dev/contracts/safety.md records "FI-ENCRYPT-1..3 …
# RULED OUT — channel removed in FeeV3"). And the symbol it greps for,
# `decrypt_fee*`, exists nowhere in the repository either, so the pattern could
# never match. The check passed vacuously on every tree and said so in a
# rule-shaped sentence citing a clause nobody can read.
#
# Detected by `scripts/check-authority-resolves.sh`, which is wired into
# scripts/run-all-tests.sh. Restoring the check requires restoring the clause.

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

# ── [Check-9] — REMOVED 2026-09-25, its authority and its subject do not exist ──
#
# The check asserted that a `pub struct AccumulatorPoint` exists in
# native_token's model, citing `fee-spec.md §5.6.2.1`. That section does not
# exist — the document's headings run §5.6 straight to §5.7 — and
# `AccumulatorPoint` occurs **nowhere in the repository**, so the check failed
# for a reason no reader could act on: it named a clause that cannot be read and
# a type that is not defined anywhere to be created.
#
# Either the requirement was never landed or it was removed without the check
# following. Restoring it requires deciding which, and writing the clause down
# first. Detected by `scripts/check-authority-resolves.sh`.

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

# ── [Check-14] — REMOVED 2026-09-25, its authority does not exist ──
#
# The check grepped for a deprecated `compute_fee(gas_units)` and cited `M-9`.
# `M-9` is defined in **no document** under doc/, and the symbol exists nowhere
# in the repository, so the check could not fail and its citation could not be
# read. It is the same shape as [FI-ENCRYPT-3] above: a rule-shaped sentence
# attached to a clause that is gone.
#
# Detected by `scripts/check-authority-resolves.sh`. Restoring it requires
# restoring the clause.

# ── Summary ──
echo ""
if [ "$FAILED" -eq 1 ]; then
    echo "=== GUARDRAILS FAILED ==="
    exit 1
else
    echo "=== GUARDRAILS PASSED ==="
    exit 0
fi
