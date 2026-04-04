# Contract Plain Deprecation: Resolution of Dual-Layer Architecture

*This document describes the resolution of DarkFi's dual-layer contract architecture. The ZK opcode limitations that necessitated `contract_plain/` have been resolved.*

## Background

The `src/contract_plain/` directory was created to workaround ZK circuit limitations:

| Opcode | Workaround Required | Status Then | Status Now |
|--------|-------------------|-------------|------------|
| `base_div` (0x58) | Native Rust division | Not implemented | ✅ **IMPLEMENTED** |
| `less_than_or_equal` (0x55) | Cross-multiplication | Unverified | ✅ **VERIFIED SOUND** |

The dual-layer architecture was a pragmatic solution:
- ZK contracts for privacy-preserving operations
- Plain contracts when ZK circuits couldn't express required logic

## Resolution

**COMPLETED**: All ZK opcode limitations have been resolved.

### Opcode Soundness Verification

Lean 4 formal verification in `proofs/lean/src/Main.lean` confirms:

```
LessThanOrEqual (0x55): SOUND ✅ (verified)
BaseDiv (0x58): MATHEMATICALLY VERIFIED ✅ (implemented)
IsEqualBase (0x54): Minor issue (doesn't enable false proofs)
```

### Migration Path Completed

| Plain Contract | ZK Replacement | Status |
|---------------|----------------|--------|
| `subscription_plain` | `../../contract/subscription/` | ✅ Migrated |
| `labor_market_plain` | `../../contract/labor_market/` | ✅ Migrated |
| `insurance_plain` | `../../contract/insurance_market/` | ✅ Migrated |
| `oracle_plain` | `../../contract/oracle/` | ✅ Migrated |
| `attestation_plain` | `../../contract/attestation/` | ✅ Migrated |

## Deprecation Notice

All contracts in `src/contract_plain/` are now **DEPRECATED**.

**Migration complete**: Use ZK contracts in `src/contract/` instead.

### Why ZK Over Plain?

| Aspect | Plain Contract | ZK Contract |
|--------|---------------|-------------|
| Privacy | Partial (amounts public) | **Full** (commitments hidden) |
| Expressivity | Full Rust | **Equivalent** (opcodes now sound) |
| Auditability | Visible on-chain | Proof verification |
| Correctness | Bug visible | **Cryptographically enforced** |

The ZK contracts now have equivalent functionality with **full privacy**.

## File Changes

### Source Code

- `src/contract_plain/*/src/lib.rs` - Added deprecation notices
- `src/contract_plain/*/README.md` - Added deprecation notices
- `src/contract_plain/README.md` - Updated with migration status

### Documentation

- `doc/src/arch/contract_plain_deprecation.md` - This document
- `doc/src/arch/opcodes.md` - Updated opcode soundness status

## Lessons Learned

### What Worked

1. **Dual-layer architecture** was a valid pragmatic solution
2. **Plain contracts** enabled real-economy applications during ZK limitations
3. **Visibility preference** principle was sound: "plain over unsound ZK"

### What Changed

1. `LessThanOrEqual` gate soundness was **verified** via Lean 4 exhaustive testing
2. `BaseDiv` was **implemented** using binary exponentiation (~254 field muls)
3. ZK contracts now have **feature parity** with plain contracts

### Design Decision

The original architecture document argued:

> "A malicious proof is more dangerous than a public bug."

This principle was correct for unsound opcodes. Now that opcodes are sound, ZK contracts provide **both** privacy **and** correctness - the best of both worlds.

## See Also

- [ZK Contract Architecture](./contract_architecture.md) - Current ZK contract design
- [Opcodes Reference](./opcodes.md) - Opcode soundness verification
- [Composability](./composability.md) - Cross-contract composition
- [Lean 4 Proofs](../../proofs/lean/src/Main.lean) - Formal verification