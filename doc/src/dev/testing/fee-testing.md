# Fee System Testing

The fee signalling system is the universal coordination mechanism across the
DarkWow stack. It has no isolated components. Testing must be integrative by
nature — verifying invariants across their full scope, not functions in isolation.

## Testing Philosophy

The fee system cuts across wallet, mempool, miner, and contract. A test that
verifies `compute_fee()` in isolation tells you nothing about whether the wallet
and miner agree on the fee value. A test that verifies `FeeCollectV1` without
FeeV2 accumulation tells you nothing about whether the miner sums the right
plaintext pot.

**Every test SHALL verify one or more invariants from fee-spec.md §14 across
the invariant's declared scope.** The invariant's scope determines the test level:

| Invariant Scope | Minimum Test Level |
|----------------|-------------------|
| Single function (pure arithmetic) | L1 (unit) |
| Single component (e.g., fee_window.rs) | L1 (unit) |
| Two components (e.g., wallet → mempool) | L1.5 (bridge) |
| Multi-component with chain state | L1.5 or L2 |
| Multi-window with block progression | L2 (heavyweight) |
| Multi-node with real P2P | L3 (Docker pipeline) |

## WYSIWYG Principles

Every fee system test SHALL follow these principles:

1. **Unique assertion tags:** Every assertion carries a unique tag referencing
   the invariant it verifies. Example: `[FI-COLLECT-2] total_fees == fees_db[height]`
2. **State logged before checks:** `log!("[FI-RISK-2] contract_risk[contract_A] = {}",
   tracker.get_risk_factor("contract_A"))` before asserting escalation.
3. **Log files for reproducibility:** `create_log_file("fee_integration_test")` —
   if a test fails in CI, the log reproduces the exact state sequence.
4. **No shared mutable state:** Fresh `HeavyweightPipeline` per test.

## Integration Test Architecture

Fee integration tests use the existing `HeavyweightPipeline` infrastructure
(`bin/dwowd/src/tests/blockchain.rs`) extended with:

- **Wallet:** `NativeTokenHarness` for FeeV2 construction with plaintext `FeeParamsV3`
- **Mempool:** `Mempool` with `NativeTokenFeeSignallingExtractor` (plain fee comparison)
- **Miner:** `prepare_block()` summing plaintext fees
- **Chain:** `accept_block()` with FeeCollectV1 verification

## Invariant Coverage Matrix

| Invariant | Current Tests | Gaps |
|-----------|-------------|------|
| FI-GEN-1,2 | 0 | No genesis parameter initialization test |
| FI-WINDOW-1,2,3 | 33 tests (fee_window.rs) | No cross-window L2 test in Rust |
| FI-FLAG-1,2,3 | 10 tests (fee_window.rs) | No wallet flag roundtrip test |
| FI-ENCRYPT-1,2,3 | RULED OUT | encrypted-fee channel removed (fee-spec.md §14.4) |
| FI-ADMIT-1,2,3 | 10 tests (mempool_tests.rs) | No cross-window tier-price update test |
| FI-COLLECT-1,2 | 6 tests (heavyweight_pipeline.rs) | No intermediate pot state assertion |
| FI-RISK-1 through FI-RISK-6 | 0 in Rust (Python only) | Entire risk pipeline unimplemented |
| FI-WASM-1,2 | 1 test (mempool, stubbed) | DeployV1 detection not implemented |
| FI-TIME-1 | RULED OUT | no threshold proofs in FeeV3 |

## False Positive Categories

Tests in these categories pass but the invariant is actually violated:

1. **Dead-code:** Tests of `compute_total_fee()` and `risk_factor()` pass but
   the functions have zero production call sites. FI-RISK-1 is violated.
2. **Isolation-pass:** A test of component A passes, a test of component B passes,
   but no test verifies A and B together produce the same result. FI-FLAG-1,
   FI-RISK-5 fall into this category.

## CI Guardrails

The script `contrib/ci/check_fee_guardrails.sh` enforces mechanical invariants:

- **FI-GEN-2:** No `const` or `static` of `FeeAmount`, `CongestionFactor`,
  `RiskFactor`, or `BlockCharge`
- **FI-RISK-6:** No `RISK_FACTOR_*` constants or `risk_factor(status)` function
  in `manifest.rs`
- **FI-COLLECT-2:** No silent fallback in the plaintext fee sum — a malformed
  `FeeParamsV3` SHALL reject, not default
- Magic numbers `1_001_000`, `42_000_000` do not appear in production code
