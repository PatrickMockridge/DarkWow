/-
# The assumption boundary

This is the **only** file in `proofs/lean/` permitted to contain an `axiom` — or a
value-less `opaque`, which is the same thing under a different keyword (`opaque f : T`
with no `:=` declares a constant with no value; `Lean/Elab/MutualDef.lean` documents the
form). Every assumption the proofs rest on is declared here, and nowhere else.

## Why it is shaped this way

An assumption can only be stated here if the types its signature mentions are in scope
here. Two consequences:

* This file **imports** the modules that define those types — `Arithmetic` for
  `PALLAS_PRIME`, `ECOps` for `ECMulGadget`, and `Capability.Composition` for
  `Resource`/`Action`.
* For one module it is the other way round. `SupplyChain` contains *proofs* that consume this
  file's assumptions — its theorems unfold `apply_block`, whose body calls `reward`,
  `pedersen_commit` and `coinbase_blind` — so it imports this one, and the definitions those
  assumptions are stated in (`PedersenPoint`, `PedersenPoint.add`, the `Add PedersenPoint`
  instance) are declared here.

`Capability.Purse` used to be the second such module and no longer is: its assumption
`purseNullifier_nonce_injective` was the only one in this tree with a live consumer, and it is
now a theorem in `Capability/Purse.lean` (see that file). Nothing here is consumed by the purse
write path.

Declarations keep the namespace they had in their original module, so no fully-qualified
name changes and no reference site needed editing: `ECOps.fixed_base_mul_uses_constant` is
still spelled that way, and `SupplyChain`'s assumptions are still at top level (`reward`,
`pedersen_commit`, …). This matters most for `pedersen_additive_homomorphism`, which was
declared twice. Only the top-level one survives; the `ECOps` one was a `: Prop` stub that
asserted nothing and has been deleted. They were never duplicates.

## What is absent, and why

Assumptions that were **deleted** rather than moved, because they were not assumptions:

* seven `: Prop`-valued axioms — uninterpreted predicates, which name a claim without stating
  one and therefore cannot be consumed by any proof
  (`CrossCutting.{nullifier_determinism, signature_binding_h2_fix, merkle_inclusion_foundation}`,
  `HashOps.{smt_membership_sound, smt_membership_privacy}`,
  `ECOps.{pedersen_additive_homomorphism, variable_base_without_binding_is_orchard_class}`);
* two duplicates — `Field.pallas_div_mul_cancel` (identical to `Arithmetic.base_div_mul_cancel`)
  and `ECOps.pedersen_additive_homomorphism`;
* `ECOps.pedersen_commitment_binding`, whose conclusion was `P ∨ ¬P`;
* `MAX_SUPPLY` and `total_reward_bounded`, which were **false about the system**: there is no
  supply cap (`src/sdk/src/blockchain.rs:56-60`, `consensus-coinbase.md:830`), and nothing in
  `src/` declares a `MAX_SUPPLY` constant.

Each deletion is recorded in `DarkFi.HAZOP.Elevated` and in the section notes below.

Assumptions **discharged** into theorems, in the modules that own the model:
`commitment_binding` and `nullifier_binding` (`HashOps.lean`),
`merkle_root_change_detection` (`HashOps.lean`), `purseNullifier_nonce_injective`
(`Capability/Purse.lean`), and `Soundness.cross_mul_implies_ratio_bound` (`Field.cross_mul_lt`).

## The contract

Every assumption carries four fields:

    ASSUMES:            the mathematical content, in words
    NOT PROVED BECAUSE: why it is an assumption rather than a theorem
    DISCHARGED BY:      what would discharge it — a mathlib lemma, a curve model, an audit
    IF FALSE:           what breaks, and whether it breaks loudly or silently

`IF FALSE:` must name either a concrete declaration in this tree whose proof would fail
(loud), or the literal token `NOTHING` plus the HAZOP entry that records the silence.
`script/check_lean_axioms.py` enforces both forms, so an assumption cannot be silent by
omission — silence has to be written down.

`@[axiom_budget N]` on a theorem declares how many of these assumptions its proof depends
on. The budget counts *our* assumptions plus `Classical.choice`, `Lean.ofReduceBool`,
`Lean.trustCompiler` and `sorryAx`; it does not count `propext` and `Quot.sound`, which are
Lean's own logic rather than ours. A budget of 0 therefore means the theorem is proved, and
a proof by `native_decide` reads 2 rather than masquerading as 0.
-/

import Mathlib
import DarkFi.Arithmetic
import DarkFi.ECOps
import DarkFi.Capability.Composition
import DarkFi.AxiomBudget
import DarkFi.Emission

open DarkFi.Capability.Composition

-- `@[axiom_budget N]` is registered in `DarkFi/AxiomBudget.lean`, not here. It cannot live in
-- this file: `Axioms.lean` imports `Arithmetic`, `ECOps` and `Capability.Composition`, so any of
-- those importing `Axioms` for the attribute would be importing itself. See that file for what
-- the budget counts.

/-! ===== Arithmetic: the Pallas field modulus =====

    `Arithmetic.PALLAS_PRIME` stays in `Arithmetic.lean` — it is a definition, and
    `Arithmetic.lean` must not import this file. `base_div_mul_cancel` is stated inside
    `namespace Arithmetic` so that it keeps its original fully-qualified name and resolves
    `PALLAS_PRIME` through the import above. -/

namespace Arithmetic

/-- ASSUMES: For `b` not divisible by `PALLAS_PRIME`, `(a * b^(p-2)) * b ≡ a (mod p)` —
    i.e. `b^(p-2)` is the multiplicative inverse of `b`.
    NOT PROVED BECAUSE: the statement needs `Nat.Prime PALLAS_PRIME` (a 254-bit Pratt
    certificate), not merely Fermat's little theorem, which mathlib does have. The comment
    this replaces claimed the blocker was that the project "depends on core Lean 4 without
    Mathlib". That was false — this file and `lakefile.lean` both require mathlib. The real
    blocker is the un-mechanised primality certificate.
    DISCHARGED BY: a proof of `Nat.Prime PALLAS_PRIME` from a Pratt certificate over the
    Pallas modulus, after which `ZMod` arithmetic closes the goal.
    IF FALSE: NOTHING. No theorem in this tree consumes this assumption; it is recorded as
    SILENT in `DarkFi.HAZOP.Elevated` ELEV-7.

    NOTE ON THE EXPONENT: this is `PALLAS_PRIME.toNat - 2`, a `Nat`. The original statement
    wrote `b ^ (PALLAS_PRIME - 2)` with `PALLAS_PRIME : Int`, which needs `HPow Int Int _` —
    a typeclass instance that does not exist, so the declaration could never elaborate. That
    error (`failed to synthesize HPow ℤ ℤ`) is in the baseline build log, which means this
    axiom had never existed as an elaborated term. -/
axiom base_div_mul_cancel (a b : Int) (hb : b % PALLAS_PRIME ≠ 0) :
  ((a * (b ^ (PALLAS_PRIME.toNat - 2))) % PALLAS_PRIME * b) % PALLAS_PRIME = a % PALLAS_PRIME

end Arithmetic

/-! ===== Hash operations =====

`poseidon_hash_output` is declared here rather than in `HashOps.lean` because a value-less
`opaque` is an assumption and assumptions belong on this side of the boundary —
`script/check_lean_axioms.py` enforces that. `HashOps.lean` imports this file for it;
`SMTMembershipGadget`, `MerklePath` and `PoseidonHashGadget` stay there.

Three `HashOps` names used to be here and are **deleted**:
`smt_membership_sound (g) (h_out : g.output = 1) : Prop` and
`smt_membership_privacy (g) (h_out : g.output = 1) : Prop` — both `: Prop`-valued axioms, so
both name an SMT soundness and privacy claim without stating one, and no proof could consume
them; `doc/src/arch/zk/opcodes.md` reported SMT membership as "SOUND ✓" on the strength of their
existence. And `compute_merkle_root` plus `axiom merkle_root_change_detection`, which moved to
`HashOps.lean` where the fold is now shaped like the implementation and the theorem is proved.

`commitment_binding` and `nullifier_binding` were here too; both are **theorems** in
`HashOps.lean` now, being `poseidon_collision_resistance` applied to two fixed-arity lists. -/

namespace HashOps

/-- Opaque: Poseidon permutation P128Pow5T3 over the Pallas base field, as a sponge applied to
    a list of field elements.

    Making this opaque rather than a stub is deliberate: an opaque function cannot be reduced
    equationally, so the collision-resistance assumption below is a real assumption rather
    than a contradiction with a trivial definition. The earlier note here claimed the
    alternative was `inputs.head?.getOrElse 0 + 1`; `README.md`'s honest-scope section still
    describes the placeholder that way. It has not been that for some time.

    ASSUMES: that the sponge produces a value at all. Carries no content of its own — it is
    the signature the assumptions below are stated in.
    NOT PROVED BECAUSE: the MDS matrix, the S-box and the 128 rounds are a separate
    formalisation project.
    DISCHARGED BY: a formalisation of the P128Pow5T3 permutation, at which point this becomes
    a `def`.
    IF FALSE: NOTHING — a missing function is a type error, not a false claim. See
    `DarkFi.HAZOP.Elevated` ELEV-8. -/
opaque poseidon_hash_output (inputs : List Int) : Int

/-- ASSUMES: Poseidon (P128Pow5T3 over the Pallas base field) is collision-resistant — no
    two distinct input lists share an output; equivalently `poseidon_hash_output` is
    injective.
    NOT PROVED BECAUSE: `poseidon_hash_output` is a value-less `opaque`, not the sponge.
    Formalising the MDS matrix, the S-box and 128 rounds is a separate project.
    DISCHARGED BY: a formalisation of the Poseidon permutation, or a reduction to a
    standard-model collision-resistance assumption for P128Pow5T3.
    IF FALSE: `HashOps.commitment_binding`, `HashOps.nullifier_binding`,
    `HashOps.smtCrh_injective` and `HashOps.merkle_root_change_detection` — every binding and
    the Merkle change-detection theorem would lose their proofs. **Loud**: those four are proved
    *from* this assumption, so their proof terms cite it. Recorded as LOUD in
    `DarkFi.HAZOP.High`. -/
axiom poseidon_collision_resistance :
  ∀ (x y : List Int), x ≠ y → poseidon_hash_output x ≠ poseidon_hash_output y

end HashOps

/-! ===== EC operations =====

`ECMulGadget`, `ECMulKind` and `FixedGenerator` stay in `ECOps.lean` (this file imports it).
Three `ECOps` names were **deleted**, not moved:

* `pedersen_additive_homomorphism (values blinds : List Int) : Prop` — a `: Prop` stub. The real
  Pedersen homomorphism is the top-level `pedersen_additive_homomorphism` below; the stub shared
  nothing with it but its name.
* `variable_base_without_binding_is_orchard_class (g) (hkind : …) : Prop` — another `: Prop`
  stub. Its content is already carried by `variable_base_mul_is_prover_chosen` below, which the
  Orchard-class claim is an immediate corollary of. `doc/src/arch/` is restated to cite that
  assumption rather than the deleted name.
* `pedersen_commitment_binding (v1 r1 v2 r2 : Int) …` — its conclusion was
  `(v1 = v2 ∧ r1 = r2) ∨ (v1 ≠ v2 ∨ r1 ≠ r2)`, a tautology by `em`, and its binding content lived
  entirely in a hypothesis it never used. Nothing depended on it, so it is removed rather than
  re-proved.

Also on record: `ECOps.detect_orchard_class_vulnerability` is a `def` returning `True` for the
`var_base` case — the detection rule is vacuous for exactly the case its name denotes. It is a
definition, not an assumption, so it has no entry here; it is recorded in
`DarkFi.HAZOP.Elevated`. -/

namespace ECOps

/-- ASSUMES: for `ec_mul` (0x02), `ec_mul_base` (0x03) and `ec_mul_short` (0x04), the base
    point is a compile-time constant — never a witness, never prover-chosen.
    NOT PROVED BECAUSE: `ECMulGadget.base_is_constant` is a `Bool` *field* of a Lean record;
    nothing in Lean ties it to what the zkas VM does with the `constant` block of a `.zk`
    file. The assumption is the model-to-implementation correspondence, not a mathematical
    fact.
    DISCHARGED BY: a model of the zkas VM's opcode dispatch (`.zk` `constant` vs `witness`
    blocks) plus a machine-checked audit that every `ec_mul`/`ec_mul_base`/`ec_mul_short`
    call site passes a declared constant. That audit is manual today.
    IF FALSE: NOTHING. No theorem consumes it. Recorded as SILENT in
    `DarkFi.HAZOP.Elevated` ELEV-13. -/
axiom fixed_base_mul_uses_constant (g : ECMulGadget)
  (hkind : g.kind ≠ ECMulKind.var_base) :
  g.base_is_constant

/-- ASSUMES: for `ec_mul_var_base` (0x05), the base is prover-chosen, so no circuit may
    assume a specific base without an additional binding constraint.
    NOT PROVED BECAUSE: as `fixed_base_mul_uses_constant` — a property of the VM's dispatch
    of opcode 0x05, which Lean does not model.
    DISCHARGED BY: the same zkas VM model; the claim then follows from the opcode's operand
    shape (an `EcNiPoint` witness).
    IF FALSE: NOTHING. No theorem consumes it. Recorded as SILENT in
    `DarkFi.HAZOP.Elevated` ELEV-14. -/
axiom variable_base_mul_is_prover_chosen (g : ECMulGadget)
  (hkind : g.kind = ECMulKind.var_base) :
  ¬ g.base_is_constant

end ECOps

/-! ===== Cross-cutting =====

Three `CrossCutting` names were **deleted**, not moved:

* `nullifier_determinism (secret commitment : Int) : Prop`
* `signature_binding_h2_fix (commitment_secret nullifier : Int) : Prop`
* `merkle_inclusion_foundation (leaf pos root : Int) (path : List Int) : Prop`

Each is a `: Prop`-valued axiom — an uninterpreted predicate, so it states no claim and no
proof can consume it. `CrossCutting.lean`'s own "VERIFIED" table cited them as evidence for
nullifier determinism, the signature H2 fix and Merkle inclusion; that table was reporting
the existence of three placeholders as verification. Recorded in `DarkFi.HAZOP.Elevated`, and
the corresponding rows in `doc/src/arch/zk/opcodes.md` and `doc/src/arch/security-analysis.md`
no longer read "VERIFIED ✓".

`CrossCutting.PedersenCommitment` and `sum_pedersen` stay in `CrossCutting.lean`. -/

/-! ===== Soundness =====

Two names were **deleted** here, not moved:

* `Soundness.cross_mul_implies_ratio_bound` asserted
  `(a b c : ℤ) (hb : b > 0) (h : a < b * c) : a / b < c`, and its own comment said it was
  "marked as axiom until completed". The proof was completed elsewhere in the tree:
  `Field.cross_mul_lt` proves exactly this statement from the division algorithm, and
  `Soundness.lean` now cites it.
* `Field.pallas_div_mul_cancel` had a byte-identical signature to
  `Arithmetic.base_div_mul_cancel` under a different name; `proofs/lean/README.md` itself
  admitted "(duplicated)". One assumption stated twice is a defect in the boundary, not a
  second assumption. The surviving copy is `Arithmetic.base_div_mul_cancel` above. -/

/-! ===== Supply chain =====

`SupplyChain.lean`'s theorems unfold `apply_block` and `cumulative_commit_sum`, whose bodies
call `reward`, `pedersen_commit`, `coinbase_blind` and `PedersenIdentity`, so those
declarations are genuinely consumed. `PedersenPoint`, its opaque `add` and the
`Add PedersenPoint` instance are declared here because the assumptions below are stated in
terms of them, and `SupplyChain.lean` imports this file.

`reward_nonneg` is *not* here: `axiom reward : Nat → Nat` makes `reward h ≥ 0` a consequence
of `Nat.zero_le`, so `SupplyChain.lean` states it as a theorem.

`MAX_SUPPLY` and `total_reward_bounded` were here and are **deleted**: the system has no supply
cap (`src/sdk/src/blockchain.rs:56-60`: "continues permanently — there is no supply cap";
`doc/src/arch/consensus-coinbase.md:830`: "NOT a hard cap — perpetual tail emission"), and no
`MAX_SUPPLY` constant exists in `src/`. So `total_reward_bounded` did not state an unproved truth
about the system, it stated the opposite of one — and it was ill-typed besides (`List.sum`
applied to a function). The property the code does guarantee is a *floor*, not a ceiling.
Proving that needs `reward` to be a definition rather than an uninterpreted function; see
OBL-C5 in `doc/src/arch/verification-hazop.md`. -/

/-- ASSUMES: the coinbase blind in block `height` is a `Nat`, used as the Pedersen blinding
    factor.
    NOT PROVED BECAUSE: the real blind is `f(prev_commitment, H)` for a deterministic `f`,
    and the choice of `f` is an implementation detail this model deliberately does not
    commit to. This declares a free parameter, not a claim about one.
    DISCHARGED BY: defining `coinbase_blind` from `prev_commitment` and `height`.
    IF FALSE: NOTHING — "there is no such function" is a type error. Recorded as SILENT in
    `DarkFi.HAZOP.Elevated` ELEV-20. -/
axiom coinbase_blind (height : Nat) : Nat

/-- ASSUMES: `reward` is monotone non-increasing.

    `reward` itself is no longer an assumption — it is a **definition** in `DarkFi/Emission.lean`,
    transcribed from `src/sdk/src/blockchain.rs:1032-1069`. What remains assumed is only this
    property of it.

    NOT PROVED BECAUSE: it needs monotonicity of `fixedPowDecay`'s exponentiation-by-squaring loop
    in `exp`, and that loop truncates at *every* squaring, so its value is not the closed form
    `DECAY_FP^e / 2^(32e)` and the argument has to go through the loop's per-bit product. The
    pieces are in `Emission.lean` (`fpMul_le_left`, `fixedPowDecay_le_one`); the parity analysis is
    not.
    DISCHARGED BY: monotonicity of `fixedPowDecay` in `exp`, by parity case analysis on `exp` with
    `fpMul_le_left` as the contraction.
    IF FALSE: NOTHING. No theorem consumes it. `total_supply_theorem` and
    `cumulative_commit_theorem` are structural inductions that hold for *any* `reward`, so they
    are proved without it — and the schedule's own Rust test asserts non-increase over a range,
    which is the only thing checking this claim today.
    Silence recorded as `DarkFi.HAZOP.Elevated` ELEV-22. -/
axiom reward_monotone (h₁ h₂ : Nat) (hle : h₁ ≤ h₂) : reward h₂ ≤ reward h₁

/-! ===== The Pallas curve: one arithmetic fact replaces seven structural ones

Seven assumptions used to be declared here — `PedersenPoint.add` (a value-less `opaque`),
`PedersenIdentity`, `pedersen_add_identity`, `pedersen_add_comm`, `pedersen_add_assoc`,
`pedersen_commit` and `pedersen_additive_homomorphism` — because the group was *modelled* as an
abstract type with an opaque operation, and its laws had to be postulated.

They are all gone. `DarkFi/Pedersen.lean` defines Pallas for real —
`WeierstrassCurve.Affine.Point` for `y² = x³ + 5` over `ZMod PALLAS_MODULUS` — and mathlib's
`WeierstrassCurve.Affine.Point.instAddCommGroup` supplies the *complete* group law through the
coordinate ring, so it handles the point at infinity and the doubling case that the opaque `add`
sidestepped. The four group laws are then instances of `add_comm`/`add_assoc`/`add_zero`/`zero_add`,
and the homomorphism is `add_nsmul` bookkeeping.

What survives is the one fact mathlib cannot supply: that the modulus is prime — needed for
`ZMod PALLAS_MODULUS` to be a `Field` at all. -/

/-- The Pallas base field modulus as a `Nat`, for `ZMod`. `Arithmetic.PALLAS_PRIME` is the same
    number as an `Int`; `ZMod` takes a `Nat`, so both spellings exist and must agree. -/
def PALLAS_MODULUS : Nat := 2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1

/-- ASSUMES: `PALLAS_MODULUS` is prime.

    NOT PROVED BECAUSE: primality of a 254-bit number needs a Pratt certificate, which means
    factoring `p - 1 = 2^32 · (2^222 - 2^7 - 2^4 - 2 - 2)` — a 222-bit cofactor. `norm_num` and
    `decide` cannot decide it in the kernel at acceptable cost. This is *the* arithmetic
    assumption now: `Arithmetic.base_div_mul_cancel` needs exactly this fact too, and the seven
    Pedersen assumptions above were all consequences of it plus the curve being nonsingular.
    DISCHARGED BY: a proof of `Nat.Prime PALLAS_MODULUS` from a Pratt certificate over the Pallas
    modulus; every consumer then becomes unconditional.
    IF FALSE: `ZMod PALLAS_MODULUS` is not a field, so `Pedersen.pallasCurve.Point` is not an
    additive group and `Pedersen.pedersen_add_comm`, `pedersen_add_assoc`, `pedersen_add_identity`
    and `pedersen_additive_homomorphism` all lose their proofs — as does
    `Arithmetic.base_div_mul_cancel`'s statement. **Loud**: those five are proved *from* this
    assumption. Recorded as LOUD in `DarkFi.HAZOP.High`. -/
axiom pallasPrime : Nat.Prime PALLAS_MODULUS

instance : Fact (Nat.Prime PALLAS_MODULUS) := ⟨pallasPrime⟩


/-! ===== Capability: Purse — DISCHARGED =====

Three declarations used to be here: `PurseWitness`, the value-less `opaque purseNullifier`, and

    axiom purseNullifier_nonce_injective (s p n₁ n₂) :
      purseNullifier ⟨s, p, n₁⟩ = purseNullifier ⟨s, p, n₂⟩ → n₁ = n₂

All three are now in `Capability/Purse.lean`, where `purseNullifier` is a **definition** over
`poseidon_hash_output` and the injectivity claim is a **theorem** derived from
`poseidon_collision_resistance`.

This was the only assumption in the tree with a live consumer — `purse_chained_nullifiers_distinct`
cited it by name — so discharging it is what removes the last `Axioms.lean` dependency from a
proved theorem's *proof term* rather than from its statement. Nothing here is consumed by the
purse write path any more. -/

/-! ===== The ZK-to-type bridge ===== -/

/-- The ZK layer's obligation to the type layer.

    ASSUMES: for a resource/action pair, every public input of the corresponding Halo2
    circuit is `constrain_instance`-derived from a witness — no free instances.
    NOT PROVED BECAUSE: Halo2 constraint-system semantics, the polynomial commitment scheme
    and the Fiat–Shamir transform are not modelled in Lean. This is an *uninterpreted
    predicate*: unlike the axiom it replaces (`circuitSoundnessBridge`), it asserts nothing
    about any particular resource or action — it names the premise so callers can supply it.
    DISCHARGED BY: the manual circuit audit over `src/contract/*/proof/*.zk`, mechanised;
    `doc/src/arch/verification-hazop.md` OBL-Z1 is that obligation, and it is currently
    unchecked.
    IF FALSE: NOTHING today. `Capability.capabilityType_of_circuitDerivable` is proved from
    `CircuitDerivable.coversBarbs` alone and never touches this predicate, because type
    *existence* is purely combinatorial. This predicate is what a soundness theorem about
    capabilities would need, and no such theorem exists. Recorded as SILENT in
    `DarkFi.HAZOP.Elevated` ELEV-26. -/
axiom NoFreeInstances (r : Resource) (s : Action) : Prop
