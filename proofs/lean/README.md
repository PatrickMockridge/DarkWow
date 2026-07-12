# Lean4 Formal Verification — DarkWow Type System & ZK Gadgets

Formal specification and verification of the DarkWow cryptographic type system
and zkVM opcode gadgets using Lean 4 (v4.12.0). Zero Mathlib dependencies —
all proofs use core Lean 4 with `native_decide` for computation.

**To verify everything:** `cd proofs/lean && lake build`

## What Is Proved

### Part 1: Capability Type System (`Capability/`)

The type system formalizes the ρ-calculus barb model from `type-system.md`.
Every cryptographic primitive is a distinct behavioral type with a fixed barb
set. Capabilities are existential proofs that a list of primitives covers the
barbs required by a resource.

| Module | Content | Key Theorems |
|--------|---------|-------------|
| `Types.lean` | 17 primitive types with barb sets, 3 raw byte containers (for distinction proofs) | — (definitions) |
| `Pareto.lean` | All primitive pairs have distinct barb sets | `primitiveTypesAreParetoEfficient` (by `native_decide`), 15 pairwise lemmas, `barbEqualityImpliesTypeEquality` |
| `Distinction.lean` | 10 non-unifiable type pairs (e.g. nullifier ≠ `[u8;32]`) | All 10 proved by `native_decide`, `allUnifiablePairsProved` |
| `Composition.lean` | 12 concrete capability types (native token transfer, DAO vote, tender bid, coinbase claim, purse balance/withdraw, identity credential, box take, multisig approval, attestation, bridge deposit/withdraw) | `barbPreservation` (induction over primitives list), `coversBarbs` for each type |
| `Wallet.lean` | Wallet capability construction function | `walletConstruct_sound`, `_complete`, `_preservesPrimitives`, `_deterministic`, `_idempotent` |
| `Inversion.lean` | Authorization Inversion Theorem | `authorizationInversion_TypeLevel` (bidirectional: type exists iff barbs covered), `verifierLearnsOnlyRequiredBarbs` |

**Capability types defined (12):** `nativeTokenTransferType`, `nativeTokenCoinbaseType`,
`daoVoteType`, `tenderBidType`, `purseBalanceType`, `purseWithdrawType`,
`identityCredentialType`, `boxCapType`, `multisigApprovalType`, `attestationType`,
`bridgeDepositType`, `bridgeWithdrawType`.

### Part 2: ZK Opcode Gadgets (`Gadgets.lean`, `Comparison.lean`, `Arithmetic.lean`)

Soundness theorems for zkVM opcode constraint systems. Each theorem proves: if the
constraint equations are satisfied, the output equals the mathematical function.

| Theorem | Opcode | Property |
|---------|--------|----------|
| `less_than_or_equal_sound` | 0x55 | out=1 iff a≤b for bounded inputs |
| `less_than_strict_sound` | 0x51 | Range-check offset constraint → a\<b |
| `is_not_equal_fully_pure` | 0x62 | All witnesses fully constrained (no degrees of freedom) |
| `is_not_equal_pure_when_equal` | 0x62 | delta_invert forced to 1 when a=b |
| `is_not_equal_delta_invert_unique_when_unequal` | 0x62 | (a-b)\*delta_invert=1 unique when a≠b |
| `is_equal_bug_when_equal` | 0x54 | **Bug found:** delta_invert unconstrained when a=b |
| `is_equal_fixed_pure_when_equal` | 0x54 | Fix pattern: purity constraint forces delta_invert=1 |
| `boolcheck_sound` | 0x53 | value\*(value-1)=0 → value∈{0,1} |
| `cond_select_correct` | 0x60 | Correct conditional selection |
| `zero_cond_correct` | 0x61 | a=0 → output=0 |
| `zero_cond_nonzero` | 0x61 | a≠0 → output=b |
| `base_add_correctness` | 0x30 | No wraparound for 64-bit inputs |
| `base_mul_correctness_bounded` | 0x31 | No wraparound for 64-bit inputs |
| `base_sub_ge_case` | 0x32 | No wraparound when a≥b≥0 |
| `base_div_by_zero` | 0x58 | Division by zero returns 0 |

### Part 3: Cross-Cutting & Arithmetic

| Module | Key Theorems |
|--------|-------------|
| `Field.lean` | `cross_mul_lt` (integer cross-multiplication soundness), `wraparound_safe` (bounded inputs preserve ordering) |
| `CrossCutting.lean` | `pedersen_value_conservation`, `value_conservation_no_wraparound` (16×64-bit values fit in Pallas field) |
| `HashOps.lean` | `merkle_root_deterministic`, `merkle_inclusion_soundness`, `merkle_root_change_detection` (induction on path length) |
| `ECOps.lean` | `fixed_base_mul_uses_constant`, `pedersen_commitment_binding`, `ec_add_inputs_must_be_distinct` |
| `Soundness.lean` | `cross_mul_implies_ratio_bound` |

### Part 4: Supply Chain Invariants (`SupplyChain.lean`)

Multi-block induction over cumulative supply commitments:

| Theorem | Property |
|---------|----------|
| `total_supply_theorem` | After H blocks, total_supply = expected_cumulative_supply(H) |
| `cumulative_commit_theorem` | Cumulative Pedersen commitment = sum of per-block commitments |
| `supply_chain_invariant` | Conjunction of both |
| `no_hidden_inflation` | Total supply exactly matches expected cumulative supply |

## Axioms: What Is Assumed

Every formal verification has axioms at the boundary between the model and the
world. These are ours. None have computational content — they are `Prop` statements
that the proofs depend on but do not compute.

### Cryptographic Assumptions (6 axioms)

| Axiom | Justification | Module |
|-------|---------------|--------|
| `poseidon_collision_resistance` | Standard cryptographic assumption; no known breaks of Poseidon-128 | `HashOps.lean` |
| `base_div_mul_cancel` | Fermat's little theorem: a\*b^(p-2)\*b = a (mod p). Mathlib-provable, deferred | `Arithmetic.lean` |
| `div_mul_cancel` | Same as above (duplicated) | `Field.lean` |
| `pedersen_additive_homomorphism` | EC group homomorphism: C(v1+v2, b1+b2) = C(v1,b1)+C(v2,b2) | `ECOps.lean` |
| `variable_base_without_binding_is_orchard_class` | EC discrete log: variable-base multiplication without base-binding constraint is the Orchard vulnerability class | `ECOps.lean` |
| `circuitSoundnessBridge` | If a ZK circuit exists verifying a resource/action pair, the CapabilityType has an inhabitant | `Inversion.lean` |

### Circuit Audit Axioms (11 axioms)

These document that 120 contract ZK circuits have been manually audited for the
`constrain_instance` pattern (the Orchard-class vulnerability). They carry no
computational content — they are documentation of human review, not machine proofs.

| Module | Axioms |
|--------|--------|
| `Circuits/Token.lean` | `burn_v1_signature_binding`, `burn_v1_nullifier_determinism`, `mint_v1_c1_fix`, `mint_v1_no_free_instances`, `token_mint_v1_auth_parent_free_by_design`, `blind_output_v1_no_free_instances`, `redeem_v1_coin_value_enforced_by_host` |
| `Circuits/Bridge.lean` | `bridge_withdraw_v1_instance_derivation`, `bridge_circuits_orchard_safe` |
| `Circuits/Exchange.lean` | `exchange_circuits_orchard_safe` |
| `Circuits/All.lean` | `all_contracts_orchard_safe` (98 additional circuits) |

### Hash & Merkle Properties (3 axioms)

| Axiom | Justification |
|-------|---------------|
| `nullifier_determinism` | Deterministic by Poseidon construction |
| `signature_binding_h2_fix` | Deterministic by Poseidon construction |
| `merkle_inclusion_foundation` | Merkle proof soundness reduces to Poseidon collision resistance |

### Supply Chain Axioms (8 axioms)

| Axiom | Justification |
|-------|---------------|
| `reward`, `reward_nonneg`, `reward_monotone` | Reward schedule parameters (economic policy, not crypto) |
| `MAX_SUPPLY`, `total_reward_bounded` | Supply cap (economic policy) |
| `PedersenIdentity`, `pedersen_commit`, `coinbase_blind` | Abstractions over Pedersen commitment implementation |

## What Is NOT Proved (Honest Scope)

- **Halo2 constraint system semantics are not modeled.** We prove properties of the
  mathematical functions the opcodes implement, not that the Halo2 gate/region/
  copy-constraint system correctly implements those functions.
- **Circuit-to-Lean correspondence is not mechanized.** The `Circuits/` directory
  documents a manual audit of 120 `.zk` files, not machine-verified extraction.
- **Poseidon is a placeholder.** `poseidon_hash_output` in `HashOps.lean` is
  defined as `inputs.head?.getOrElse 0 + 1` — a trivial function, not the actual
  sponge construction. The theorems prove *structure* (determinism, collision
  resistance implies change detection), not the actual Poseidon permutation.
- **Pedersen point addition is abstract.** `PedersenPoint.add` is implemented as
  `Nat` addition, not EC point arithmetic. The supply chain induction proves the
  *algebraic structure*, not the curve implementation.
- **Fermat's Little Theorem is assumed**, not proved. Adding Mathlib as a
  dependency would make this provable.
- **No verified compiler from `.zk` files.** The ZKAS compiler produces Halo2
  circuits; there is no formal semantics for the ZKAS language in Lean4.

## Verification

```bash
cd proofs/lean

# Verify all proofs (type-checks every theorem):
lake build

# Run the verification suite (IO-based checks + honest summary):
lean --run src/Main.lean

# Expected output:
#   Native Token Transfer: {spend, nullify, commit, dispatch, gate, denominate} ⊆ {...} = true
#   DAO Vote: ... ⊆ ... = true
#   ... (all 12 capability types)
#   Bridge Deposit: ... ⊆ ... = true
#   Bridge Withdrawal: ... ⊆ ... = true
#   All capability types: coversBarbs verified.
#   HONEST VERIFICATION SUMMARY:
#     Proved theorems:   ~40 (with non-trivial proofs)
#     Computational:     ~25 (by native_decide)
#     Axioms:            ~43
#     IO tests:          4
#     HAZOP findings:    15
#     Bugs found:        2
```

## Project Structure

```
proofs/lean/
├── lean-toolchain              # Lean 4.12.0
├── lakefile.lean               # Build configuration (core Lean only, no Mathlib)
├── lake-manifest.json          # Dependency manifest (empty — zero external deps)
├── README.md                   # This file
└── src/
    ├── Main.lean               # Verification suite entry point
    ├── Examples.lean           # Interactive examples (lean --run via lake run)
    └── DarkFi/
        ├── Field.lean          # Pallas field arithmetic foundations
        ├── Gadgets.lean        # LessThanOrEqual, IsEqual, IsNotEqual soundness
        ├── Soundness.lean      # Cross-multiplication theorems
        ├── Arithmetic.lean     # base_add, base_mul, base_sub, base_div correctness
        ├── Comparison.lean     # BoolCheck, CondSelect, ZeroCond, LessThanStrict
        ├── CrossCutting.lean   # Value conservation, nullifier determinism, signature binding
        ├── HashOps.lean        # Merkle root, Poseidon hash, SMT membership
        ├── ECOps.lean          # EC operations, Orchard-class vulnerability detection
        ├── SupplyChain.lean    # Multi-block cumulative supply induction
        ├── HAZOP.lean          # HAZOP risk matrix and cross-cutting patterns
        ├── Capability/         # ρ-calculus type system
        │   ├── Types.lean      # 17 primitive types with barb sets
        │   ├── Pareto.lean     # Pareto-efficiency (all types pairwise distinct)
        │   ├── Distinction.lean # 10 non-unifiable pairs
        │   ├── Composition.lean # 12 capability type constructions
        │   ├── Wallet.lean     # walletConstruct soundness/completeness
        │   └── Inversion.lean  # Authorization Inversion Theorem
        ├── Circuits/           # constrain_instance manual audit (axioms)
        │   ├── Token.lean
        │   ├── Bridge.lean
        │   ├── Exchange.lean
        │   └── All.lean
        └── HAZOP/              # Structured audit findings
            ├── Critical.lean   # Risk ≥ 60
            ├── High.lean       # Risk 40-59
            └── Elevated.lean   # Risk 30-39
```

## Contributing

To add a new primitive type:
1. Define it in `Capability/Types.lean` with its barb set
2. Add it to `allPrimitiveTypes`
3. `lake build` — Pareto-efficiency is automatically checked for all new pairs

To add a new capability type:
1. Define `Resource` (required barbs) and `Action` in `Capability/Composition.lean`
2. Construct `CapabilityType` with `primitives` list and `coversBarbs` proof
3. Add an `IO.println` check to the `#eval do` block
4. `lake build` — the `coversBarbs` proof is checked by the Lean4 kernel

To add a new ZK opcode proof:
1. Model the constraint equations in `Gadgets.lean` or `Comparison.lean`
2. State and prove the soundness theorem
3. `lake build`

All Lean4 sources are AGPL-3.0-only.
