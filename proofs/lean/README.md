# Lean4 Formal Specification — DarkWow ZK Circuit Gadgets

Formal specification of DarkWow zkVM opcode gadgets using the Lean 4
proof assistant (v4.12.0). This is a **parallel formal specification**,
not a verified extraction from Rust code.

## What Is Proved

### Genuine Prop-Based Theorems (~20)

These theorems have non-trivial Lean4 proofs establishing soundness
or purity properties of the gadget constraint equations.

| # | Theorem | Opcode | Property | File |
|---|---------|--------|----------|------|
| 1 | `is_not_equal_fully_pure` | 0x62 | All witnesses fully constrained | Gadgets.lean |
| 2 | `is_not_equal_pure_when_equal` | 0x62 | delta_invert=1 when a=b | Gadgets.lean |
| 3 | `is_not_equal_delta_invert_unique_when_unequal` | 0x62 | (a-b)*delta_invert=1 when a≠b | Gadgets.lean |
| 4 | `is_equal_bug_when_equal` | 0x54 | delta_invert unconstrained when a=b | Gadgets.lean |
| 5 | `is_equal_fixed_pure_when_equal` | 0x54 | Fix pattern forces delta_invert=1 | Comparison.lean |
| 6 | `less_than_or_equal_sound` | 0x55 | out=1 iff a≤b (range-check offset) | Gadgets.lean |
| 7 | `less_than_strict_sound` | 0x51 | Range-check offset → a<b | Comparison.lean |
| 8 | `boolcheck_sound` | 0x53 | value*(value-1)=0 → value∈{0,1} | Comparison.lean |
| 9 | `cond_select_correct` | 0x60 | Correct conditional selection | Comparison.lean |
| 10 | `zero_cond_correct` | 0x61 | When a=0: output=0 | Comparison.lean |
| 11 | `zero_cond_nonzero` | 0x61 | When a≠0: output=b | Comparison.lean |
| 12 | `base_add_correctness` | 0x30 | No wraparound for 64-bit inputs | Arithmetic.lean |
| 13 | `base_mul_correctness_bounded` | 0x31 | No wraparound for 64-bit inputs | Arithmetic.lean |
| 14 | `base_sub_ge_case` | 0x32 | No wraparound when a≥b≥0 | Arithmetic.lean |
| 15 | `cross_mul_lt` | — | Int cross-multiplication soundness | Field.lean |
| 16 | `cross_mul_implies_ratio_bound` | — | Int ratio bound soundness | Soundness.lean |
| 17 | `value_conservation_no_wraparound` | — | 16×64-bit values fit in field | CrossCutting.lean |
| 18 | `merkle_root_change_detection` | 0x20 | Collision resistance → change detection | HashOps.lean |
| 19 | `wraparound_safe` | — | Bounded inputs preserve int ordering | Field.lean |
| 20 | `base_div_by_zero` | 0x58 | Division by zero returns 0 | Arithmetic.lean |

### Cryptographic Axioms (~10)

These are stated as axioms with mathematical justification. They capture
properties that are computationally assumed (hash collision resistance)
or mathematically true but require Mathlib for proof (Fermat's theorem).

| Axiom | Justification |
|-------|---------------|
| `poseidon_collision_resistance` | Standard cryptographic assumption |
| `base_div_mul_cancel` | Fermat's little theorem (Mathlib-provable) |
| `div_mul_cancel` (Field.lean) | Fermat's little theorem (Mathlib-provable) |
| `pedersen_additive_homomorphism` | EC point addition associativity |
| `nullifier_determinism` | Poseidon collision resistance |
| `signature_binding_h2_fix` | Poseidon collision resistance |
| `merkle_inclusion_foundation` | Merkle proof soundness (Poseidon-dependent) |
| `smt_membership_sound` | SMT proof soundness (Poseidon-dependent) |
| `smt_membership_privacy` | ZK property of Halo2 proof system |
| `variable_base_without_binding_is_orchard_class` | EC discrete log assumption |

### Bugs Found

| # | Bug | Severity | Status |
|---|-----|----------|--------|
| IsEqualBase | delta_invert unconstrained when a=b (constraint 3 is 0=0) | LOW | CONFIRMED, formally characterized in Gadgets.lean |
| EcAdd | Incomplete addition formula; doubling case (x1=x2) not rejected | MEDIUM | Documented in ECOps.lean |

## What This Is NOT

- **NOT** a full Halo2 ConstraintSystem formal model (no gate/region/copy-constraint modeling)
- **NOT** a verified compiler from `.zk` circuits to Lean theorems
- **NOT** 187 verified theorems (the original claim was inflated; we now report honestly)
- **NOT** 120 Orchard-verified contract circuits (the `Circuits/` directory contains a manual audit of `constrain_instance` patterns, not formal circuit-level proofs)
- **NOT** a replacement for Halo2 MockProver testing or integration tests

## HAZOP Audit

The `HAZOP/` directory contains structured audit findings (not formal proofs):
- **CRITICAL** (4 findings, risk ≥ 60): governance_report free instances, liquidate no collateralization check, withdraw recipient front-running, aggregate bound checks NO-OP
- **HIGH** (5 findings, risk 40–59): burn zero_cond bypass, labor refund check skip, labor nullifier collision, governance zero-division
- **ELEVATED** (6 findings, risk 30–39): deposit zero_cond, swap_id mismatch, exit incomplete circuit, redeem no zero check, slippage field division, DEX bool_check amounts

These are `def`-based findings with descriptions and fix suggestions. They are NOT formal theorems — they document constraint gaps discovered during circuit review.

## Project Structure

```
proofs/lean/
├── lean-toolchain              # Lean 4.12.0
├── lakefile.lean               # Build (core Lean only, no Mathlib)
├── README.md                   # This file
└── src/
    ├── Main.lean               # Verification suite with honest summary
    └── DarkFi/
        ├── Field.lean          # Pallas field, cross_mul, wraparound
        ├── Gadgets.lean        # LessThanOrEqual, IsEqual, IsNotEqual
        ├── Soundness.lean      # Cross-multiplication theorems
        ├── Arithmetic.lean     # base_add, base_mul, base_sub, base_div
        ├── Comparison.lean     # BoolCheck, CondSelect, ZeroCond, LessThanStrict
        ├── CrossCutting.lean   # Value conservation, sum bounds
        ├── HashOps.lean        # Merkle root, Poseidon, SMT
        ├── ECOps.lean          # EC operations, Orchard-class defense
        ├── HAZOP/              # Structured audit findings (defs, not theorems)
        │   ├── Critical.lean   # Risk ≥ 60
        │   ├── High.lean       # Risk 40-59
        │   └── Elevated.lean   # Risk 30-39
        └── Circuits/           # constrain_instance audit (axioms, not theorems)
            ├── Token.lean
            ├── Bridge.lean
            ├── Exchange.lean
            └── All.lean
```

## Running

```bash
cd proofs/lean
lake build                     # Compiles all modules
lean --run src/Main.lean       # Runs verification suite
```

## Future Work

1. Full Halo2 gate/region/copy-constraint model in Lean4
2. Mechanized correspondence between Rust constraint code and Lean specifications
3. Integration with Mathlib for Fermat's theorem (replacing BaseDiv axioms)
4. Property-based test generation from Lean theorems (QuickCheck-style)
5. Formal proof of the 253-iteration BaseDiv exponentiation algorithm

## License

AGPL-3.0-only.
