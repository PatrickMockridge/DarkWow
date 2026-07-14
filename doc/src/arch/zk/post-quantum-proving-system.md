# Post-Quantum Proving System Requirements

This document defines the functional requirements a post-Halo2 proving
system SHALL satisfy for like-for-like replacement in DarkWow's zkVM.
It is the "swap-out spec" — WHAT a replacement must do, not HOW to build it.

The proving system is the computational realization of the `↓prove` and
`↓verify` barbs in DarkWow's ρ-calculus type system (see
[Type System §0-§2](../type-system.md)). Under the Authorization Inversion
Theorem (§6): `CapabilityType(r, s) = L_{r,s}` where `L_{r,s}` is the ZK
proof language. The replacement SHALL support the same predicate language
expressiveness as Halo2 — the 39 zkVM opcodes SHALL have equivalent gadgets.

**RFC 2119**: SHALL, MUST, SHALL NOT, MUST NOT are used throughout.

## Functional Requirements

Each requirement maps a Halo2 property the zkVM depends on to a
requirement a replacement proving system SHALL satisfy.

| # | Halo2 Property | Requirement | Must Match |
|---|---|---|---|
| FR-1 | Transparent setup | SHALL be transparent — no trusted ceremony, no toxic waste, no MPC. `Params::new(k)` equivalent SHALL be deterministic from public randomness. | `src/zk/proof.rs:44` |
| FR-2 | Pallas field arithmetic | SHALL support arithmetic in a prime field ≥ 255 bits with a large 2-adic subgroup for FFT (≥ 2^20). | All 39 opcodes in `src/zk/vm.rs` |
| FR-3 | Vesta commitment scheme | The commitment scheme SHALL be binding and hiding. SHALL support additive homomorphism for value conservation proofs. | `src/zk/proof.rs` — `VerifyingKey`, `Proof::verify()` |
| FR-4 | PLONK arithmetization | SHALL support custom gates with degree ≥ 5 (Poseidon S-box x^5). SHALL support copy constraints (permutation argument). SHALL support lookup arguments for range checks. | `configure_with_params()` in `vm.rs:444-605` |
| FR-5 | Poseidon hash | SHALL provide a circuit-friendly sponge hash with S-box exponent 5 over the field from FR-2. For PQ: SHALL support double-width (P256Pow5T3 for 128-bit PQ security). | `src/zk/vm.rs:501-507` |
| FR-6 | Sinsemilla hash | SHALL provide Merkle tree hashing in-circuit. If Sinsemilla cannot be efficiently emulated, Poseidon-based Merkle (already supported via SparseMerkleRoot) is acceptable. | `src/zk/vm.rs:516-542` |
| FR-7 | ECC fixed-base scalar mul | SHALL support fixed-base scalar multiplication for Pedersen-style commitments. If STARK-based: hash-based commitments (Poseidon) SHALL replace Pedersen. | `src/zk/vm.rs:493-498` |
| FR-8 | Merkle inclusion proofs | SHALL support Merkle tree membership proofs of depth ≥ 32. Proof verification SHALL be expressible as a constraint. SHALL support Sparse Merkle Trees (Poseidon-based). | `src/zk/vm.rs:526-542` |
| FR-9 | Fiat-Shamir transcript | SHALL use a collision-resistant hash with ≥ 256-bit output for non-interactive proofs. Blake2b-512 is current; replacement SHALL be at least as strong. | `src/zk/proof.rs:192-199` |
| FR-10 | Range checks | SHALL support range checks (64-bit, 253-bit) for integer comparison in the field. SHALL support Boolean range checks (0 or 1). | `src/zk/vm.rs:551-557` |
| FR-11 | Floor planner | SHALL provide region-based circuit synthesis with row allocation. API: assign region, copy constraint, lookup, equality constraint. | `src/zk/vm.rs:607` |
| FR-12 | Recursive composition | SHOULD support recursive proof verification. Pallas/Vesta cycle-of-curves is Halo2's mechanism; STARK-to-STARK recursion or Circle STARKs are acceptable alternatives. | `src/zk/proof.rs` |

## API Surface

The replacement SHALL implement a conceptual interface matching the Halo2
API surface at `src/zk/proof.rs`. This is not a Rust trait specification —
it defines the functional signatures a replacement must provide.

```
trait ProvingBackend {
    type Field;              // FR-2: ≥ 255-bit prime field
    type CommitmentParams;   // FR-1: transparent setup output
    type ProvingKey;         // FR-1: derived from circuit + params
    type VerifyingKey;       // FR-3: derived from circuit + params
    type Proof;              // FR-4,9: non-interactive proof
    type Circuit;            // FR-4: circuit with constraint system
    type ConstraintSystem;   // FR-4: gates, lookups, wiring
    type Transcript;         // FR-9: Fiat-Shamir transcript

    fn setup(k: u32) -> Self::CommitmentParams;                    // FR-1
    fn build_vk(params, circuit) -> Self::VerifyingKey;            // FR-4
    fn build_pk(params, circuit) -> Self::ProvingKey;              // FR-4
    fn prove(pk, circuit, instances, rng) -> Self::Proof;          // FR-4,9
    fn verify(vk, proof, instances) -> Result<(), ProofError>;     // FR-3,9
    fn serialize_proof(proof) -> Vec<u8>;                          // FR-13
    fn deserialize_proof(bytes) -> Result<Self::Proof, ProofError>;// FR-13
    fn serialize_vk(vk) -> Vec<u8>;                                // FR-13
    fn deserialize_vk(bytes) -> Result<Self::VerifyingKey, ProofError>;// FR-13
}
```

## Constraint System Interface

The zkVM at `src/zk/vm.rs` configures 10 advice columns plus per-gadget
columns (Fixed, Instance, Lookup). The replacement SHALL support:

- **Columns**: ≥ 10 advice, ≥ 5 fixed, ≥ 2 instance, ≥ 3 lookup table columns
- **Gates**: Custom gate degree ≥ 5. Poseidon's S-box requires degree-5
  polynomial constraints (x^5 permutation step)
- **Lookups**: Range check lookups (64-bit, 253-bit). Table lookups for
  Sinsemilla generator constants
- **Copy constraints**: Permutation argument for wiring advice columns
- **Regions**: `assign_region`, `namespace`, `constrain_equal`,
  `constrain_instance` — equivalent abstractions SHALL exist

## Arithmetization Requirements

| Primitive | Current (Halo2) | Requirement | Notes |
|---|---|---|---|
| Field | Pallas::Base (255-bit) | ≥ 255-bit prime with 2-adic FFT subgroup | All 39 opcodes affected |
| Hash (commitments) | Poseidon P128Pow5T3 | Poseidon or equivalent sponge. P256Pow5T3 for PQ | S-box exponent 5 |
| Hash (Merkle) | Sinsemilla (Orchard) | Poseidon-based acceptable (SparseMerkleRoot exists) | FR-6 migration path |
| Commitments | Pedersen (ECC) | Hash-based (Poseidon) for STARKs | FR-7: zero-knowledge property |
| Fiat-Shamir | Blake2b-512 | Any ≥ 256-bit output hash | SHA3-256 or Poseidon acceptable |
| Range checks | Lookup table (64/253-bit) | Any lookup or permutation-based range check | Standard technique |

## Migration Compatibility

**FR-13 (ZkBinEntry)**: The replacement SHALL define a migration cutover
height. Historical blocks before cutover remain verifiable via the Halo2
code path. New blocks after cutover use the post-quantum proving system.
Version byte in the ZkBinary format identifies the proving backend.

**FR-14 (Circuit binary)**: The replacement SHALL define a versioned
circuit binary format. First byte SHALL indicate proving system:
`0x01` = Halo2, `0x02` = STARK, etc.

**FR-15 (Proof serialization)**: The `Proof(Vec<u8>)` newtype SHALL be
versioned. First byte SHALL identify the proving backend. Enables single
verification dispatch path across proving systems.

**FR-16 (Gadget abstraction)**: The zkVM opcode implementations SHALL be
refactored to use a `GadgetBackend` trait rather than calling Halo2 chips
directly. Tracked as a separate issue — this is a prerequisite refactor,
not part of this specification.

## Quantum Resistance Requirements

| Assumption | Acceptable? | Reason |
|---|---|---|
| ECDLP (any curve) | NO | Broken by Shor |
| Discrete log in pairings | NO | Broken by Shor |
| RSA/factoring | NO | Broken by Shor |
| Hash collision resistance (≥ 256-bit output) | YES | Grover: O(2^128) acceptable |
| Hash preimage resistance (≥ 256-bit output) | YES | Grover: O(2^128) acceptable |
| LWE (Learning With Errors) | YES | No known quantum break. NIST PQC standard |
| SIS (Short Integer Solution) | YES | Related to LWE. No known quantum break |
| Code-based (McEliece) | YES | No known quantum break. NIST PQC standard |
| Multivariate quadratic equations | YES | No known quantum break |

**FR-17**: The replacement SHALL NOT rely on ECDLP, discrete log in
pairings, or integer factorization for soundness.

**FR-18**: The replacement SHALL achieve ≥ 128-bit post-quantum security
level (NIST PQC Category 5 equivalent for long-term protection).

## Relationship to the Type System

The proving system is the execution substrate for ρ-calculus processes
(see [Type System §9](../type-system.md#9-concurrent-execution-model)).
Under the Authorization Inversion Theorem: `CapabilityType(r, s) = L_{r,s}`
— the ZK proof language is the behavioral type of a capability.

The barb preservation theorem guarantees that composing capability types
does not erase barbs. The replacement SHALL preserve this: a circuit that
proves `↓spend` in Halo2 SHALL prove `↓spend` in the replacement.

The type distinctions between `ProvingKey`, `VerifyingKey`, and `Proof`
(each with distinct barbs) SHALL be preserved in the replacement's type
hierarchy.

## STARK Gap Analysis

| Halo2 Component | STARK Equivalent | Status | Notes |
|---|---|---|---|
| PLONK arithmetization | AIR (Algebraic Intermediate Representation) | Redesign required | Opcodes become AIR constraints |
| Pallas field | Any 255+ bit field (Goldilocks-64, BabyBear, Mersenne-31) | Choice affects proof size | |
| Poseidon chip | Native Poseidon in AIR | Direct translation | Same permutation, different constraint form |
| Sinsemilla chip | Poseidon-based Merkle (exists) | Migration path ready | SparseMerkleRoot opcode |
| ECC fixed-base mul | Not needed (hash commitments) | Simplification | STARK-native |
| Merkle chip | Merkle in AIR | Straightforward | Hashes are STARK-native |
| Blake2b transcript | Same or SHA3 | Straightforward | |
| Range checks | AIR-native (LogUp or cached quotients) | More efficient than PLONK lookups | |
| Floor planner | AIR degree bounds | Different constraint model | |
| Recursion | STARK-to-STARK or Circle STARKs | Active research (2025-2026) | |
| Proof size | 50-200 KB (vs 1-3 KB Halo2) | Acceptable trade-off | On-chain storage impact |

## Compliance Checklist

| # | Requirement | Priority |
|---|---|---|
| FR-1 | Transparent setup | MUST |
| FR-2 | ≥ 255-bit prime field | MUST |
| FR-3 | Binding + hiding commitment scheme | MUST |
| FR-4 | Custom gates + copy constraints + lookups | MUST |
| FR-5 | Circuit-friendly hash (Poseidon or equivalent) | MUST |
| FR-6 | Merkle tree hashing in-circuit | MUST |
| FR-7 | Commitment scheme (hash-based for PQ) | MUST |
| FR-8 | Merkle inclusion proofs (depth ≥ 32) | MUST |
| FR-9 | Fiat-Shamir transcript | MUST |
| FR-10 | Range checks (64-bit, 253-bit, Boolean) | MUST |
| FR-11 | Region-based synthesis | MUST |
| FR-12 | Recursive proof composition | SHOULD |
| FR-13 | ZkBinEntry compatibility (cutover model) | MUST |
| FR-14 | Versioned circuit binary format | MUST |
| FR-15 | Versioned proof serialization | MUST (defer exact format to impl) |
| FR-16 | Gadget abstraction (prerequisite refactor) | SHOULD (separate tracking issue) |
| FR-17 | No ECDLP/pairing/factoring assumptions | MUST |
| FR-18 | ≥ 128-bit post-quantum security | MUST |

## See Also

- [Quantum Threat Model](../quantum-threat.md) — threat specification, triggers, circuit inventory
- [Type System Specification](../type-system.md) — ρ-calculus formalism
- [ZK Verification Architecture](zk_verification.md) — current Halo2 verification flow
- [zkVM Primitives](zkvm_primitives.md) — all 39 opcodes
- [NIST PQC Standards](https://csrc.nist.gov/projects/post-quantum-cryptography)
