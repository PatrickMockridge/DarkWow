# ZK Opcode Status

## Overview

This document tracks the formal verification status of ZK circuit opcodes used in DarkWow contracts. Most comparison opcodes are now formally verified or implemented.

## Opcode Status Table

| Opcode | Status | Notes |
|--------|--------|-------|
| `LessThanOrEqual` (0x55) | ✅ **Verified Sound** | Lean 4 exhaustive testing |
| `LessThanStrict` (0x51) | ✅ Sound | Constrain-only, inherently safe |
| `LessThanLoose` (0x52) | ✅ Sound | Constrain-only |
| `NotBase` (0x56) | ✅ Verified | Production-ready |
| `BaseLtStrict` (0x57) | ✅ Verified | Production-ready |
| `BaseDiv` (0x58) | ✅ **Implemented** | Binary exponentiation (Fermat's theorem) |
| `IsEqualBase` (0x54) | ⚠️ Bug | Delta-invert unconstrained when `a == b` - use `ConstrainEqualBase` |

## Known Issue: IsEqualBase

**IsEqualBase (0x54)** has a bug: when `a == b`, `delta_invert` is unconstrained.

**Workaround**: Use `ConstrainEqualBase` for assertion-only equality checks.

## Contract Compatibility

| Contract | Feature | Status |
|----------|---------|--------|
| stablecoin | Collateralization checks | ✅ LessThanOrEqual or Safemath |
| identity | Threshold predicates | ✅ LessThanOrEqual verified |
| dex | Partial fills | ✅ LessThanOrEqual verified |
| **bridge** | All deposit/withdraw operations | ✅ No workarounds needed! |

### Bridge = Opcode-Independent

The bridge is **NOT held up by missing opcodes**.

The bridge uses **atomic swap semantics** which only need:
- Hash constraints (poseidon_hash)
- Merkle proofs (merkle_root)
- Range checks (range_check)

No division, no Boolean returns, no complex arithmetic. The bridge "just works" because atomic operations don't need the advanced opcodes.

See [Bridge Architecture](bridge.md) for details.

## Migration Status

Plain contracts have been **deprecated** in favor of ZK contracts since:
- `LessThanOrEqual` is formally verified sound
- `BaseDiv` is implemented

## References

- [Opcodes and Formal Verification](opcodes.md) — Full opcode analysis with Lean 4 proofs
- [Safemath](safemath.md) — ZK arithmetic templates for assertion-only comparisons
- [Field Arithmetic](field_arithmetic.md) — zkVM primitive analysis
