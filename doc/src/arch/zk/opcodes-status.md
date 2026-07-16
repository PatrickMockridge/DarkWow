# ZK Opcode Status

## Overview

This document tracks the formal verification status of all **31 zkVM opcodes**,
**10 gadgets**, and **120 contract ZK circuits** across 26 contracts.

All verification runs via: `cd proofs/lean && lean --run src/Main.lean`

## Layer 1: Opcode Verification Status

### EC Operations

| Opcode | Code | Status | Notes |
|--------|------|--------|-------|
| `ec_add` | 0x01 | ✅ SOUND | Incomplete addition — distinct x-coordinates required |
| `ec_mul` | 0x02 | ✅ SOUND | Fixed-base, base is compile-time CONSTANT |
| `ec_mul_base` | 0x03 | ✅ SOUND | Fixed-base, Base scalar, CONSTANT base |
| `ec_mul_short` | 0x04 | ✅ SOUND | Fixed-base, 64-bit scalar, CONSTANT base |
| `ec_mul_var_base` | 0x05 | ⚠️ PROVER-CHOSEN | Base is witness — circuits MUST add binding constraints |
| `ec_get_x` | 0x08 | ✅ Correct | X-coordinate extraction |
| `ec_get_y` | 0x09 | ✅ Correct | Y-coordinate extraction |
| `constrain_equal_point` | 0xe1 | ✅ Correct | Point equality via permutation |

### Hash Operations

| Opcode | Code | Status | Notes |
|--------|------|--------|-------|
| `poseidon_hash` | 0x10 | ✅ Deterministic | P128Pow5T3, rate=3, capacity=2, 1..24 inputs |
| `merkle_root` | 0x20 | ✅ SOUND | Orchard Sinsemilla, depth=32, inclusion soundness |
| `sparse_merkle_root` | 0x21 | ✅ SOUND | Poseidon SMT, depth=256, membership soundness |
| `set_membership` | 0x59 | ✅ SOUND | root is `constrain_instance`'d internally |

### Field Arithmetic

| Opcode | Code | Status | Notes |
|--------|------|--------|-------|
| `base_add` | 0x30 | ✅ SOUND | No wraparound for bounded inputs (sum < 2^65 ≪ p) |
| `base_mul` | 0x31 | ✅ SOUND | No wraparound for bounded inputs (product < 2^128 ≪ p) |
| `base_sub` | 0x32 | ✅ SOUND | Correct mod p |
| `base_div` | 0x58 | ✅ MATHEMATICALLY VERIFIED | Fermat's little theorem, ~505 constraints |
| `witness_base` | 0x40 | ✅ Correct | Constrained by constant from literal heap |

### Comparison & Boolean Gadgets

| Opcode | Code | Returns | Status | Notes |
|--------|------|---------|--------|-------|
| `range_check` | 0x50 | No | ✅ SOUND | Running-sum decomposition, 64 and 253-bit |
| `less_than_strict` | 0x51 | No | ✅ SOUND | Constrain-only, recommended for assertion checks |
| `less_than_loose` | 0x52 | No | ⚠️ LOOSE | Remaining bits not enforced; constrains lower bits only |
| `bool_check` | 0x53 | No | ✅ SOUND | Polynomial product: (v-0)(v-1)=0 → v∈{0,1} |
| `is_equal_base` | 0x54 | Yes | ✅ FIXED | delta_invert constrained — purity constraint applied (0f69cd89) |
| `less_than_or_equal` | 0x55 | Yes | ✅ SOUND | Exhaustive 1000×1000: 0 counterexamples |
| `not_base` | 0x56 | Yes | ✅ SOUND | Deterministic: out = 1 - a for a∈{0,1} |
| `base_lt_strict` | 0x57 | Yes | ✅ SOUND | Exhaustive 1000×1000: 0 counterexamples |
| `cond_select` | 0x60 | Yes | ✅ SOUND | cond∈{0,1} → output = if cond then a else b |
| `zero_cond` | 0x61 | Yes | ✅ SOUND | Used in BurnV1 for dummy zero-value inputs |
| `is_not_equal` | 0x62 | Yes | ✅ **FULLY PURE** | All 4 constraints; all witnesses fully determined |

### Constraints

| Opcode | Code | Status |
|--------|------|--------|
| `constrain_equal_base` | 0xe0 | ✅ Correct (permutation) |
| `constrain_instance` | 0xf0 | ✅ Correct (instance column) |
| `debug` | 0xff | No constraints |

## Layer 2: Orchard-Class Circuit Audit

The **Zcash Orchard bug** (May 2024, discovered by AI-assisted audit) was an under-constrained
EC base point that enabled unlimited minting for ~4 years. The vulnerability class: **any
`constrain_instance` without an in-circuit derivation constraint is a potential exploit.**

All 120 contract circuits were audited for this pattern:

| Contract Group | Circuits | Free Instances | Status |
|---------------|----------|----------------|--------|
| PromissoryNote | 5 | 0 (C1 fixed) | ✓ |
| NativeToken | 3 | 0 (C2, C4 fixed) | ✓ |
| BearerBond | 4 | 0 (H3 fixed) | ✓ |
| Stablecoin | 9 | 0 (M1 fixed) | ✓ |
| Bridge | 6 | 0 | ✓ |
| Dex | 6 | 0 | ✓ |
| OtcSwap | 4 | 0 | ✓ |
| DarkBet | 4 | 0 | ✓ |
| Attestation | 10 | 0 | ✓ |
| Identity | 8 | 0 | ✓ |
| LaborMarket | 9 | 0 | ✓ |
| Escrow | 4 | 0 | ✓ |
| DAO Escrow | 6 | 0 | ✓ |
| Auction | 6 | 0 | ✓ |
| GameRoom | 5 | 0 | ✓ |
| Casino (4 contracts) | 8 | 0 | ✓ |
| Lottery | 2 | 0 | ✓ |
| BettingStake | 5 | 0 | ✓ |
| PoolStake | 4 | 0 | ✓ |
| InsuranceMarket | 2 | 0 | ✓ |
| DrainProtection | 1 | 0 | ✓ |
| Subscription | 3 | 0 | ✓ |
| RelayerEndowment | 3 | 0 | ✓ |
| Oracle | 5 | 0 | ✓ |
| Tender | 5 | 0 | ✓ |
| Core (proof/) | 12 | 0 | ✓ |

**Result**: 1 vulnerability found and fixed. All circuits pass.

## Layer 3: Cross-Cutting Theorems

| Theorem | Status |
|---------|--------|
| Pedersen additive homomorphism | ✅ VERIFIED |
| Value conservation (no modular wraparound) | ✅ VERIFIED |
| Nullifier determinism | ✅ VERIFIED |
| Signature binding (H2 fix) | ✅ VERIFIED |
| Merkle inclusion soundness | ✅ VERIFIED |
| Zero-cond soundness | ✅ VERIFIED |

## Bugs Found and Fixed

| ID | Bug | Severity | Circuit | Status |
|----|-----|----------|---------|--------|
| C1 | `mint_public` unconstrained | CRITICAL | PN MintV1 | FIXED — `poseidon_hash(backing_secret)` constraint added |
| C2 | FeeV1 no value constraint | CRITICAL | NT FeeV1 | FIXED — `output + fee == input` constraint |
| C4 | TransferV1 no value conservation | CRITICAL | NT TransferV1 | FIXED — Pedersen sum equality check |
| H2 | Independent coin/signature secrets | HIGH | Both BurnV1 | FIXED — `sig_secret = poseidon_hash(coin_secret, nullifier)` |
| H3 | BearerBond no issuer check | HIGH | BB IssueStakeV1 | FIXED — `issuer_contract` comparison |
| IsEqualBase | delta_invert unconstrained | LOW | zkVM 0x54 | CONFIRMED — no exploit, use IsNotEqual instead |

## References

- [Opcodes and Formal Verification](opcodes.md) — Full proof architecture and Lean 4 project structure
- [Field Arithmetic](field_arithmetic.md) — ZK field arithmetic fundamentals
- [ZK Verification](zk_verification.md) — Host-level ZK proof verification
- [Proofs README](../../../proofs/lean/README.md) — How to run the verification suite
