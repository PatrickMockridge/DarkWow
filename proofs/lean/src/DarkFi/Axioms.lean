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

/-
## `base_div_mul_cancel` — DISCHARGED, 2026-09-20

    axiom base_div_mul_cancel (a b : Int) (hb : b % PALLAS_PRIME ≠ 0) :
      ((a * (b ^ (PALLAS_PRIME.toNat - 2))) % PALLAS_PRIME * b) % PALLAS_PRIME = a % PALLAS_PRIME

It is a **theorem** now, in `DarkFi/BaseDiv.lean`, and the diagnosis its own four-field block
carried turned out to be exactly right: the blocker was never Fermat's little theorem, which
mathlib has, but the un-mechanised primality certificate — `Nat.Prime PALLAS_PRIME`.

So this was not an arithmetic assumption wearing a different hat. It was the *same* assumption as
`pallasPrime`, restated over `Int`. With `Fact (Nat.Prime PALLAS_MODULUS)` in scope, `ZMod
PALLAS_MODULUS` is a field, `b^(p−1) = 1` for `b ≠ 0`, and the `Int` statement follows from the
`%`-to-`ZMod` bridge (`ZMod.intCast_eq_intCast_iff'`). The budget says 2 and cites `pallasPrime`,
which is the honest reading: one assumption, not two.

The two document claims this makes true are worth noting for where they sit — see the header of
`BaseDiv.lean`. `security-analysis.md:517` had it right when it listed this among the *assumptions*;
`philosophy.md:39` and `quantum-os.md:64` called `BaseDiv` "Lean4-verified", which was premature and
now is not.

No theorem consumed it, so no budget in the table moved. The boundary is one smaller.
-/

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

/-- ASSUMES: `poseidon_hash_output` is **injective** — no two distinct input lists share an
    output.

    **This is not collision-resistance, and the equivalence is not a stylistic slip.** The
    previous text here read "Poseidon ... is collision-resistant — no two distinct input lists
    share an output; equivalently `poseidon_hash_output` is injective", and the two are not
    equivalent in either direction that matters:

    * injectivity is **strictly stronger** than collision-resistance — CR permits collisions that
      are merely hard to find, injectivity forbids them outright;
    * and injectivity is **false of the real Poseidon**, by pigeonhole. `P128Pow5T3` over the
      Pallas base field takes up to 24 field elements — 24 × 254 ≈ 6096 bits of input — and
      returns 254 bits. Distinct inputs sharing an output exist necessarily.

    So this assumption is not a hypothesis about Poseidon. It is satisfiable, and only because the
    *model* has none of the real function's structure: `poseidon_hash_output` is opaque with
    domain `List Int`, and `List Int` and `Int` are both countable, so an injection between them
    exists and the axiom set stays consistent. What the model thereby describes is an injective
    function of `List Int → Int`, which no sponge of this shape can be.

    **What follows for the consumers.** `HashOps.commitment_binding`, `nullifier_binding`,
    `smtCrh_injective` and `merkle_root_change_detection` are proved *from* this, and each derives
    a hash *inequality* from an input inequality — the injectivity direction. They are sound as
    statements about an injective `h`, and they say **nothing about the deployed Poseidon**, which
    is not one. The proof terms cite the assumption, so the budget records the dependency honestly;
    what the budget cannot record is that the hypothesis is false of the object the theorem names.
    A faithful statement needs the standard-model form — "no efficient adversary finds a collision"
    — which is a statement about adversaries and not about a function, and which Lean here has no
    computational model to make.

    NOT PROVED BECAUSE: `poseidon_hash_output` is a value-less `opaque`, not the sponge, and the
    property asserted is the wrong one to prove of a sponge.
    DISCHARGED BY: for the injectivity as stated, nothing — it is false of Poseidon. What would
    make the four consumers meaningful is a formalisation of collision-resistance in the
    standard model, or a restatement of those four against a property Poseidon does have.
    IF FALSE: `HashOps.commitment_binding`, `HashOps.nullifier_binding`, `HashOps.smtCrh_injective`
    and `HashOps.merkle_root_change_detection` lose their proofs. They are proved *from* this
    assumption, so their proof terms cite it. Recorded as LOUD in `DarkFi.HAZOP.High`, and as
    a fidelity gap rather than a dependency in `doc/src/arch/verification-hazop.md`. -/
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

Also on record: `ECOps.detect_orchard_class_vulnerability` is a `def` over a *modelled* gadget, so
it establishes nothing about a real circuit and has no entry here; it is recorded in
`DarkFi.HAZOP.Elevated`. Its `var_base` branch used to return `True`, making the branch vacuous
for exactly the case its name denotes; it now returns `varBaseObligation g`, which is
`¬ g.base_is_constant` and can be false. That is the branch no longer being empty — it is not the
rule being mechanized. The rule over the 180 `.zk` sources is
`script/circuit_instance_derivation.py`. -/

namespace ECOps

/-! ===== Two assumptions removed — they were false, and made the theory inconsistent =====

`fixed_base_mul_uses_constant` and `variable_base_mul_is_prover_chosen` used to be *axioms* here,
stated over a `ECMulGadget` that carried a free `base_is_constant : Bool` field:

    axiom fixed_base_mul_uses_constant (g : ECMulGadget)
      (hkind : g.kind ≠ ECMulKind.var_base) : g.base_is_constant
    axiom variable_base_mul_is_prover_chosen (g : ECMulGadget)
      (hkind : g.kind = ECMulKind.var_base) : ¬ g.base_is_constant

**Both were false, and their falsity was derivable as `False`.** `ECMulGadget` is freely
constructible, so `⟨ECMulKind.fixed_short, 0, false, FixedGenerator.value_commit_value, 0, 0⟩` is a
gadget whose kind is fixed and whose `base_is_constant` is `false`; the first axiom applied to it
gives `false = true`. From that, `False`, and from `False` every theorem in `proofs/lean/` — not
conditionally, but outright. The axiom set was **inconsistent**, which is a strictly worse condition
than any of the three this file distinguishes (`unproved`, `false`, `unconsumed`): it makes the
budget table meaningless rather than merely incomplete. `1 = 2` was derivable.

The fix is in `ECOps.lean`: `base_is_constant` is no longer a field. `ECMulKind.baseIsConstant`
derives constancy from the kind, and both statements are now **theorems** there, proved by case
split with `@[axiom_budget 0]`. With no free field there is no counterexample to construct, and
consistency does not depend on a convention holding.

What those axioms were reaching for is still not proved, and it is not expressible over these
types: that the model's kind-to-constancy mapping is the one the zkas VM implements. That is the
model-to-implementation correspondence — a claim about the Rust dispatch and the `.zk` `constant`
block — and it is the same class of gap as `NoFreeInstances` below, not a declaration about
`ECMulGadget`.

Recorded as `DarkFi.HAZOP.Elevated` ELEV-13 and ELEV-14, and in
`doc/src/arch/verification-hazop.md` under "The axioms that were inconsistent". -/

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
    NOT PROVED BECAUSE: the real blind is `poseidon_hash([sk_H, H, DOMAIN_COMMITMENT_BLIND])` with
    `sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H)` (`consensus-coinbase.md`
    §2.2–§2.3, and §2.7's "no random keys"): deterministic, but a function of the **miner's owner
    secret** and the height. **Corrected 2026-09-24: this field claimed the real blind is
    `f(prev_commitment, H)`, and nothing in the tree derives a blind from a previous commitment**
    — measured both ways, against the specification and by grep: the tree's pattern is
    `poseidon_hash([secret, …])`, and `src/linear/src/supply_chain.rs:104`'s chain *sums* per-block
    blinds (`blind_H = blind_{H-1} + coinbase_blind_H`) rather than deriving them. So the model's
    `Nat → Nat` typing is **stronger than the specification**: it makes the blind a function of the
    height alone, where two miners at one height carry different blinds. That over-determination is
    the deliberate simplification this axiom is — a free parameter, not a claim about the code.
    DISCHARGED BY: a model of the cycled key derivation (`derive_instance`, whose own route is the
    Poseidon sponge that `poseidon_hash_output` waits on) and of the commitment hash. **The route
    this field named — "defining `coinbase_blind` from `prev_commitment` and `height`" — does not
    exist**: the blind is not a function of those two arguments, so no such definition can be
    written. A discharge would *re-type* this to take the owner secret, and the `Nat → Nat` shape is
    then the part the specification contradicts.
    IF FALSE: NOTHING — "there is no such function" is a type error. Recorded as SILENT in
    `DarkFi.HAZOP.Elevated` ELEV-20. -/
axiom coinbase_blind (height : Nat) : Nat

/-- ASSUMES: `reward` is non-increasing **from genesis on**: `1 ≤ h₁ ≤ h₂ → reward h₂ ≤ reward h₁`.

    `reward` itself is no longer an assumption — it is a **definition** in `DarkFi/Emission.lean`,
    transcribed from `src/sdk/src/blockchain.rs:1032-1069`. What remains assumed is only this
    property of it.

    **The statement was false until 2026-09-20**, because it lacked the `1 ≤ h₁` hypothesis. It read
    `∀ h₁ h₂, h₁ ≤ h₂ → reward h₂ ≤ reward h₁`, which at `(h₁, h₂) = (0, 1)` says
    `reward 1 ≤ reward 0`, i.e. `1383764049 ≤ 0`. `reward 0 = 0` is the *pre-genesis sentinel*
    rather than a schedule value, so the schedule jumps at height 1 — and `reward_tail_floor`, the
    theorem immediately below, already carried exactly the `1 ≤ h` hypothesis this axiom needed.
    The refutation is machine-checked in `Emission.reward_monotone_unbounded_is_false`, and the
    corrected statement is `Emission.RewardNonIncreasing`.

    This is the second assumption in this file found **false** rather than merely unproved (the
    other is `pallasPrime`, via a composite modulus). Both failed the same way: a property that
    holds on the range the system actually uses was stated without the range hypothesis, and
    nothing in the tree could tell a false assumption from an unproved one.

    NOT PROVED BECAUSE: it needs monotonicity of `fixedPowDecay`'s exponentiation-by-squaring loop
    in `exp`, and **the analysis as of 2026-09-20 has narrowed to one case.** Writing `G` for
    `fixedPowDecayGo` and `b'` for `fpMul b b`, monotonicity follows from the single-step lemma
    `G (n+1) r b ≤ G n r b`, which splits by the parity of `n`:

    * **`n = 2k` — done.** The sides are `G k (fpMul r b) b'` and `G k r b'`, so it is
      `fpMul r b ≤ r` fed to `Emission.fixedPowDecayGo_mono_acc`, which is now a **theorem**.
    * **`n = 2k+1` — the obstruction.** The sides are `G (k+1) r b'` and `G k (fpMul r b) b'`.
      The inequality is true — **measured 2026-09-24**, not merely believed: it holds over 300 steps
      at the state the schedule reaches (`r = FP_ONE`, `b = DECAY_FP`) and over grids of `(k, r, b)`.
      But the induction hypothesis at `k` gives `G (k+1) r b' ≤ G k r b'`, and `G k (fpMul r b) b'` is
      *smaller* than `G k r b'` because `fpMul r b ≤ r` — so the hypothesis lands on the wrong side of
      the target. What is needed is a statement about the *size of the factors* each loop multiplies
      by: the extra step the `k+1` loop performs multiplies by some power of `b'` (hence at most `b`),
      while the target's pre-multiplication is by exactly `b`. Truncation at every squaring is what
      stops that from being bookkeeping. **And the missing ingredient is quantitative**, which the
      analysis above did not say: the cumulative error in the decay passes the *local* gap between
      successive ideal values once the exponent exceeds about 5.5·10⁴, so no absolutely-bounded
      sandwich survives the middle range, and a proof has to compare the truncation errors of
      *adjacent* exponents, which nearly cancel because they differ by one carry. Estimated 30–60
      lemmas.

    **Routes already checked and ruled out, so they are not retried:**

    * the closed form is **not** equal — `fixedPowDecay 34 = 4294871042` against
      `FP_ONE · DECAY_FP^34 / FP_ONE^34 = 4294871043`, off by one and compounding;
    * `G (n+1) r b ≤ fpMul (G n r b) b` is **false** (fails at `n = 6, b = 2147482232`, again by
      one), so the contraction cannot be applied at the outer step;
    * the step bound `G (k+1) r b ≤ G k (fpMul r b) b` is **false** — machine-checked now, as
      `Emission.fixedPowDecayGo_step_bound_is_false`, at `k = 1, r = 95872739, b = 1363349908`
      (`9660288` against `9660287`);
    * `fpMul`'s nested bound `fpMul r (fpMul b b) ≤ fpMul (fpMul r b) b` is **false** —
      machine-checked as `Emission.fpMul_nested_bound_is_false` at `r = 346803675, b = 4229726225`
      (`336347717` against `336347716`). This one needs no loop at all: truncating fixed-point
      multiplication is not associative, and that is what makes the "fold the extra step into the
      accumulator" rescue fail;
    * a kernel-checked *range* bound by reflection is blocked, and **the fix this entry proposed for it
      was tried and does not pay** — measured 2026-09-24. Making the loop structural does let the
      kernel reduce it (both refutations above are kernel-checked that way), but a range check over
      `reward` costs about 1.8s at 500 blocks and **aborts with a kernel stack overflow** somewhere
      between 500 and 2000, against the 3·10⁵ exponents a plain scan covers. The instrument was
      strictly weaker than the measurement it would have replaced, so it was not landed: a definition
      nothing consumes is what this tree deletes.

    The pieces are in `Emission.lean`: `fpMul_le_left`, `fixedPowDecayGo_mono_acc`,
    `fixedPowDecay_le_one`, `decayedReward_le_initial`, `reward_nonincreasing_first_step`, and the two
    refutations above.
    DISCHARGED BY: a quantitative error-propagation argument — comparing the truncation errors of
    *adjacent* exponents, rather than bounding the factor the extra step multiplies by. That
    distinction is the correction of 2026-09-24: this field named the factor bound as the route, and
    the two machine-checked refutations above are the two shapes that route takes. Plus a domain
    bound, which is free: `fixedPowDecay e = 0` for `e ≥ 2²⁵+1` and `decayedReward` is below
    `TAIL_REWARD` from `e ≈ 4.32·10⁶`, so the range that matters is finite.
    IF FALSE: NOTHING. No theorem consumes it. `total_supply_theorem` and
    `cumulative_commit_theorem` are structural inductions that hold for *any* `reward`, so they
    are proved without it. What checks this claim today is `Emission.reward_nonincreasing_first_step`
    (the first step, kernel-checked) and the schedule's own Rust test over a range.
    Silence recorded as `DarkFi.HAZOP.Elevated` ELEV-22. -/
axiom reward_monotone (h₁ h₂ : Nat) (h₁_ge : 1 ≤ h₁) (hle : h₁ ≤ h₂) : reward h₂ ≤ reward h₁

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
    number as an `Int`; `ZMod` takes a `Nat`, so both spellings exist and must agree.

    **This was wrong until 2026-09-20.** It read `2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1`, which
    is `0x3fff…ffed…6d` — and is divisible by 3 (also by 7 and 109), so it is composite. The
    real Pallas modulus is `2 ^ 254 + 45560315531419706090280762371685220353`, whose high 128 bits
    are `0x40000000000000000000000000000000` and whose low 128 are
    `0x224698fc094cf91b992d30ed00000001` (`pasta_curves-0.5.2`, the generator's base field).
    Because the old value was composite, `pallasPrime` below asserted something **false**, and the
    `Fact` instance derived from it made `ZMod PALLAS_MODULUS` a `Field` on the strength of a
    falsehood — so every theorem proved through that structure was vacuous. The modulus is now
    the real one, and `Axioms.pallasPrime` is a true statement that remains assumed.

    That `Fact` instance was **global** until 2026-09-24 and is declared nowhere now: each
    declaration that genuinely needs the field takes it locally, which is what stopped a primality
    fact from being charged to statements that need only a ring. The note under the axiom has the
    measurement. -/
def PALLAS_MODULUS : Nat := 2 ^ 254 + 45560315531419706090280762371685220353

/-- ASSUMES: `PALLAS_MODULUS` is prime.

    NOT PROVED BECAUSE: primality of a 254-bit number needs a Pratt certificate, which means
    factoring `p - 1`. **The attempt, recorded verbatim as of 2026-09-20:**

    * The route is Mathlib's Lucas test, `Mathlib/NumberTheory/LucasPrimality.lean:38`:

          theorem lucas_primality (p : ℕ) (a : ZMod p) (ha : a ^ (p - 1) = 1)
              (hd : ∀ q : ℕ, q.Prime → q ∣ p - 1 → a ^ ((p - 1) / q) ≠ 1) : p.Prime

      `hd` quantifies over **every** prime divisor of `p − 1`, so it needs the *complete*
      factorisation, not a partial one. That is where this dies.
    * `p − 1 = 2^32 · 3 · 463 · q` with `q` a **64-digit composite**. (The *previous* text here gave
      `2^32 · (2^222 - 2^7 - 2^4 - 2 - 2)`, which factorises the composite modulus this file used to
      define — a wrong factorisation of a wrong number.) Trial division removes `2^32 · 3 · 463` and
      stops; Pollard's rho did not factor `q` within five minutes.
    * **`norm_num` is not a route and must not be reached for.** Mathlib's own header
      (`Mathlib/Tactic/NormNum/Prime.lean:17-21`) says: *"For numbers larger than 25 bits, the
      primality proof produced by `norm_num` is an expression that is thousands of levels deep, and
      the Lean kernel seems to raise a stack overflow when type-checking that proof."* Pallas is 254
      bits, and `norm_num` decides primality by trial division (`minFac`) rather than by Lucas.
    * `decide` is worse still — it would trial-divide to `√p ≈ 2^127`.

    So the obstruction is a **factorisation**, not a proof: everything else the certificate needs is
    already in Mathlib. This is *the* arithmetic assumption: the seven Pedersen postulates and
    `Arithmetic.base_div_mul_cancel` (now a theorem in `DarkFi/BaseDiv.lean`) all reduce to it.

    **The factorisation was found on 2026-09-24, and it was necessary but not sufficient.** The `q`
    above is two primes rather than a composite, and `p − 1` factors completely:

        p − 1 = 2³² · 3 · 463 · 539204044132271846773 · 8999194758858563409123804352480028797519453
        q1 − 1 = 2² · 3⁵ · 89 · 14923 · 417677162933
        q2 − 1 = 2² · 3⁴ · 11 · 2531 · 115603 · 1197907 · 22160661629 · 325086459374267

    obtained with `/usr/bin/gp` — `default(parisizemax, 2^32)` and then `factor(p-1)` — in under five
    minutes. `isprime(p)` returns **1**, so the statement is **true**, and a Lucas witness exists at
    `a = 5`. But the factorisation was the *stated* obstruction and not the only one: each large factor
    needs its own recursive certificate (68 and 143 bits), and `hd` needs `a ^ ((p-1)/q) ≠ 1` in
    `ZMod PALLAS_MODULUS` for each of the five divisors — exponents of about 2²⁵⁰ bits, which kernel
    `decide` cannot reduce (Mathlib's `Monoid.npow` is unary) and `native_decide` is unavailable for,
    `OBL-T10` being a closed row whose whole content is that no proof rests on
    `Lean.ofReduceBool`/`trustCompiler`.

    DISCHARGED BY: a recursive Pratt certificate over the factorisation above, plus those five
    exponentiation facts. Estimated 500–1500 lines. The factorisation, its tooling and the witness are
    recorded here so the next attempt re-runs rather than re-derives them.
    IF FALSE: `ZMod PALLAS_MODULUS` is not a field, so `Pedersen.pallasCurve.Point` is not an
    additive group and `Pedersen.pedersen_add_comm`, `pedersen_add_assoc`, `pedersen_add_identity`
    and `pedersen_additive_homomorphism` all lose their proofs — as does
    `Arithmetic.base_div_mul_cancel`'s statement. **Loud**: the collector counts **14** declarations
    reaching this assumption, and they are proved *from* it. Recorded as LOUD in `DarkFi.HAZOP.High`.

    **This assumption was false until 2026-09-20**, because `PALLAS_MODULUS` was the composite
    `2^254 - 2^32 - 2^7 - 2^4 - 2 - 1` = `3 × 9649340769776349618630915417390658987772498722136713669954798667324662480847`.
    The distinction matters and is not a technicality: an axiom that is *false* makes every theorem
    derived through it vacuous, whereas an axiom that is *unproved* leaves them conditional — and
    the budget table cannot tell the two apart. That is the failure mode this whole file exists to
    make visible, and it happened here. -/
axiom pallasPrime : Nat.Prime PALLAS_MODULUS

/- **There is deliberately no `instance : Fact (Nat.Prime PALLAS_MODULUS)` here**, and its absence is a
   repair rather than an omission. It used to be a global instance, and a global instance is reached by
   *every* file that imports this one — so `NatPow`/`MonoidWithZero` resolution on `ZMod PALLAS_MODULUS`
   preferred the `Field` path over `ZMod.commRing` (which is unconditional) at every site, whether or not
   the site needed a field. The effect was that a declaration's *measured* budget depended on what
   happened to be in scope rather than on what its proof used: `theorem (5 : ZMod PALLAS_MODULUS) = 5 :=
   rfl` read budget 2, because the `Field` structure is built *from* `pallasPrime`.

   The split is made by measurement rather than by reading: removing the instance and letting the build
   name the sites that break is the same experiment, and each site that needs one carries a
   `local instance` in the section wrapping it. What that did to the budget distribution is in this
   module's commit and in the theorem annotations themselves, which the gate checks. -/


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
    unchecked. **Corrected 2026-09-24: that is not the route, and the measurements are these.**
    The audit **is** mechanised (`script/circuit_instance_derivation.py`, the register's only
    MECHANIZED row) **and it is a gate** — `scripts/check-circuit-instance-derivation.sh`, wired at
    `scripts/run-all-tests.sh:101` as `run_gate "circuit instance derivation"`. This paragraph said
    the opposite for an hour, from a grep that searched for the *Python* file's name while the
    wrapper is *hyphenated*: one side of a two-sided pair read alone, and the correction is recorded
    here rather than quietly edited. What is true is that the gate **exits 1** today — 11 instances
    across 7 circuits are neither derived, nor bound, nor redundant, nor declared free (all eleven
    named in `doc/src/arch/verification-hazop.md` `OBL-Z16`). But **even a passing audit would not
    discharge this predicate**: the audit is a checker over circuit *sources*, while this is
    `(r, s)`-indexed, and what sits between them — `(r, s) ↦ the circuit source` — is a
    *transcription*, not a check. A Lean term cannot read a `.zk` file, so the discharge needs the
    transcription to be *supplied* (a module generated from the sources and freshness-gated, in the
    style of this tree's `check-artifact-freshness.sh`). What the mechanisation does give is on the
    other side of the bridge: `Circuits/InstanceDerivation.lean` defines the property over a
    statement list, proves the soundness direction and refutes its converse, and checks one worked
    circuit — so the audit's mechanisation yields the *definition* the axiom is a stand-in for, not
    the premise.
    IF FALSE: NOTHING — an uninterpreted `Prop` is a name, not a claim, so there is no falsity to
    detect; that is ELEV-26's point. **The sentence that stood here was wrong in a way worth
    keeping**: it said `Capability.capabilityType_of_circuitDerivable` "is proved from
    `CircuitDerivable.coversBarbs` alone and never touches this predicate, because type
    *existence* is purely combinatorial". The first half is true of the *proof term*, which is
    `CapabilityType.mk (CircuitDerivable.primitives h) (CircuitDerivable.coversBarbs h)` — and the
    second half is false of the *budget*: that theorem's `@[axiom_budget 1]` is correct, and
    `#print axioms capabilityType_of_circuitDerivable` reports `NoFreeInstances`. The reason is a
    rule about the model rather than about the premise being load-bearing — **a structure with a
    `Prop` field whose type names an axiom carries that axiom in every projection, including the
    projections of its data fields** (`CircuitDerivable.primitives` and `.coversBarbs` each read 1
    on their own) — measured minimally in `Capability/Inversion.lean`'s header. Recorded as
    SILENT in `DarkFi.HAZOP.Elevated` ELEV-26, and as a budget-mechanism hazard in
    `DarkFi.HAZOP.High` HIGH-16. -/
axiom NoFreeInstances (r : Resource) (s : Action) : Prop

/-! ===== AEAD: the note-opening primitive and its key-committing property =====

    The receive path's `↓discover` barb rests on one fact about AEAD: a ciphertext
    authenticates under the key it was sealed to and under no other. That fact was
    **assumed by a definition** before this pair existed — `Net/Receive.lean`'s `decrypt`
    was `if k = n.recipient then some … else none`, so its `decrypt_sound` was true of the
    `if` rather than of any ciphertext, and `doc/src/arch/wallet.md` §2.1 cited it as
    evidence about the deployed receive path. The primitive is now opaque and the property
    is an assumption, so the dependency is visible in a budget rather than built into a
    branch. -/

/-- The AEAD opening primitive: `aead_open ciphertext key` is the note plaintext if the
    ciphertext authenticates under `key`, and `none` otherwise. Opaque for the same reason
    `HashOps.poseidon_hash_output` is: the deployed construction is bytes (ChaCha20Poly1305
    over a Sapling-DH-derived key, `src/sdk/src/crypto/note.rs:113-135`) and this tree has no
    byte-level model of it.

    ASSUMES: that an opening is a total function of a ciphertext and a key. Carries no
    content of its own — it is the signature the assumption below is stated in.
    NOT PROVED BECAUSE: there is no byte-level AEAD in this tree; the nonce derivation
    (`Self::derive_nonce`, the M7 fix), the Sapling KDF and `ChaCha20Poly1305` are unmodelled.
    DISCHARGED BY: a byte-level model of `AeadEncryptedNote` — at which point this becomes a
    `def` and the property below becomes a theorem about it.
    IF FALSE: NOTHING — a missing function is a type error, not a false claim. See
    `DarkFi.HAZOP.Elevated` ELEV-31. -/
opaque aead_open (ciphertext : Int) (key : Int) : Option Int

/-- ASSUMES: a ciphertext authenticates under **at most one** key — `aead_open c k₁ = some p₁`
    and `aead_open c k₂ = some p₂` force `k₁ = k₂`. This is the standard **key-committing**
    property of an AEAD, and it is exactly what makes a wrong-key wallet discover nothing:
    `Net.decrypt_sound` and `Net.decrypt_wrong_key_none` are proved *from* it.

    **The statement is the unconditional strengthening of a computational property, and the
    difference is the one `poseidon_collision_resistance` already carries.** The deployment is
    Sapling DH → `kdf_sapling` → `ChaCha20Poly1305` with the nonce derived from `ephem_public`
    (`src/sdk/src/crypto/note.rs:113-135`), whose real guarantee is that a ciphertext made to
    authenticate under a second key is *found with probability about 2⁻¹²⁸ per attempt* — a
    statement about adversaries. This axiom asserts that no such pair **exists**, which is
    false of the real construction by counting: Poly1305 tags are 128 bits and the domain is
    larger, so unbounded searching finds a collision. It is satisfiable only because `aead_open`
    is opaque and `Int` is countable, so the model has none of the real function's structure.
    What the model describes is a function injective under opening, which no tag-based AEAD can
    be. Honest scope: `proofs/lean/README.md`'s honest-scope section states this beside the
    Poseidon entry rather than leaving it to the reader.

    NOT PROVED BECAUSE: neither the cipher nor the MAC is modelled, and the property in the
    form a standard-model proof gives it — "no efficient adversary finds a ciphertext that
    authenticates under two keys" — is a statement about adversaries, for which this tree has
    no computational model.
    DISCHARGED BY: a byte-level `ChaCha20Poly1305` model with its authentication game, which
    turns the equality into `Pr[collision] ≤ 2⁻¹²⁸`; or, short of that, a restatement of the two
    consumers against a property the deployment does have unconditionally.
    IF FALSE: `Net.decrypt_sound` and `Net.decrypt_wrong_key_none` lose their proofs — both are
    proved from this assumption, so their proof terms cite it and the build fails loudly. The
    direction that would be silent is the opposite one: an assumption *stronger* than the
    deployment is consistent, so nothing fails — it only over-states what the receive path
    proves. Recorded as LOUD in `DarkFi.HAZOP.High` HIGH-15. -/
axiom aead_key_committing (c k₁ k₂ p₁ p₂ : Int) :
  aead_open c k₁ = some p₁ → aead_open c k₂ = some p₂ → k₁ = k₂
