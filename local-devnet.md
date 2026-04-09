# Local Devnet Testing - Blockers

This document tracks blockers encountered when running contract tests using darkfid test harnesses.

## Status Update: 2026-04-09

**Test harness compilation errors have been FIXED!**

The `darkfi-contract-test-harness` now compiles successfully. All 43+ compilation errors have been resolved by updating the harness code to match the current contract parameter struct definitions.

---

## Resolved Issues

### Files Modified

| File | Changes |
|------|---------|
| `escrow.rs` | Fixed `ClaimParamsV1` → `ClaimEscrowParamsV1`, field names |
| `oracle.rs` | Fixed `SubmitValueParamsV1`/`ResolveParamsV1` imports, field splits |
| `tender.rs` | Fixed field splits (`pub` → `pub_x`/`pub_y`), removed obsolete fields |
| `attestation.rs` | Added missing `proof`, `attestation_id`, `metadata`, `claim_id`, `revealed_result` |
| `block_height_prediction.rs` | Fixed `creator_pub` → `creator`, `target_timestamp` → `target_time` |
| `labor_market.rs` | Fixed field splits and renames |
| `pool_stake.rs` | Fixed `creator_pub` → `owner_pub`, `fee_bp` → `operator_fee_bp` |
| `relayer_endowment.rs` | Fixed `InitializeParamsV1` and `DeployCapitalParamsV1` fields |
| `subscription.rs` | Added missing fields, fixed field splits |
| `bridge.rs` | Fixed `.inner().to_bytes()` → `.to_repr()` + PrimeField import |
| `insurance_market.rs` | Fixed `.to_bytes()` → `.to_repr()` + PrimeField import |

### Root Causes Identified and Fixed

1. **Coordinate Field Splitting**: Fields like `creator_pub` were split into `pub_x` and `pub_y`
2. **Renamed Params Structs**: `ClaimParamsV1` → `ClaimEscrowParamsV1`
3. **Field Renames**: `budget` → `payment_amount`, `deadline` → `timeout`/`bid_deadline`
4. **Missing Required Fields**: `proof`, `attestation_id`, `metadata`, etc.
5. **Type Method Errors**: `Fp` type uses `.to_repr()` not `.inner().to_bytes()`

---

## Remaining Issue: Linker Bus Error on Debug Builds

When building integration tests in debug mode, the linker (`rust-lld`) crashes.

### Error
```
collect2: fatal error: ld terminated with signal 7 [Bus error], core dumped
```

### Workaround
Use `--release` flag:
```bash
cargo test --release -p darkfi_money_contract --test integration
```

---

## Current Status

| Component | Status |
|-----------|--------|
| Test harness compiles | ✅ Fixed |
| Integration tests run | ✅ Compiles and starts running |
| Debug build works | ❌ Linker Bus error (use release) |

### Runtime Issue Discovered

When running `money_integration` test, it fails with:
```
Found unhandled zkas namespace OpenPositionV1
```

This is a runtime issue in the VKS (Verification Key System) - the test harness doesn't know about all ZK proof namespaces used by the contracts.

---

## Next Steps

1. Investigate the VKS issue - `OpenPositionV1` namespace not handled
2. Add missing zkas namespaces to test harness
3. Re-run integration tests