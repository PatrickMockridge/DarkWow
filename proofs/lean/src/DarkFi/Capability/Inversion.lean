/-
DarkWow Capability Type System — the circuit bridge, and what it does not prove

This file used to open by restating, as the theorem it formalizes, the claim from
type-system.md §6 and ocap.md §3:

  THEOREM (Authorization Inversion). An ACL-based authorization system
  A(p, r, s) can be inverted to a privacy-preserving O-Cap scheme
  A'(π, r, s) if and only if there exists a ZK proof system for the
  language L_{r,s} = { w : P_{r,s}(w) = 1 } with proofs simulatable
  without knowledge of w.

Two things were wrong with that as a statement of what this file contains.

**The `iff` is false, and only one direction of it is claimed here.** Necessity fails:
privacy-preserving authorization does not require a ZK proof system. Anonymous credentials
(Chaum 1985; Camenisch–Lysyanskaya 2001), blind signatures (Chaum 1982) and MAC-based tokens
all authorize without revealing a principal, and none of them is a proof system for a language
of the shape above. What is claimed is the sufficiency direction, under its hypotheses: *if* a
ZK proof system exists for L_{r,s}, *then* an ACL-based system can be inverted to a
privacy-preserving capability scheme. Prior art: proof-carrying authorization (Bauer, Appel,
Felten, CSF 2016) for the proof-carrying form, Camenisch–Lysyanskaya for the credential form.
The docs are corrected in the same commit; see Part 1 below.

**The ZK half was never proved.** This file does not model a ZK proof system. It contains:

  - Part 1: the ZK premise as an explicit *hypothesis* (`CircuitDerivable`), replacing an
    axiom whose antecedent was vacuous;
  - Part 2: existence of three concrete capability types, constructively;
  - Part 3: `Nonempty (CapabilityType r s) ↔ ∃ primitives, r.requiredBarbs ⊆ compose
    primitives` — a real iff, but about barb coverage, not about proof systems;
  - Parts 4-5: consequences of `CapabilityType.coversBarbs`.

Everything ZK-shaped in this file is conditional on a premise no theorem here discharges.
`Axioms.NoFreeInstances` names that premise; it does not assert it.

The full ZK model would need Halo2 constraint-system semantics, a polynomial commitment
scheme, and Fiat–Shamir. That is future work, and until it exists `#print axioms` on any
theorem in this file should be read as "conditional on the circuits being what the audit says
they are".
-/

import DarkFi.Capability.Types
import DarkFi.Capability.Composition
import DarkFi.Axioms
import DarkFi.AxiomBudget

open DarkFi.Capability.Types
open DarkFi.Capability.Composition

/- ==========================================================================
   Part 1: The circuit bridge — a hypothesis, not a bridge
   ==========================================================================
   An axiom used to stand here:

     axiom circuitSoundnessBridge (r : Resource) (s : Action) :
       (∃ (circuit : String), True) → Nonempty (CapabilityType r s)

   Its antecedent is true for every `r` and `s` — the witness is `""` — so the "if" carried no
   information, and the axiom asserted that *every* capability type exists, for every resource
   and every action. That erased exactly the distinction it claimed to bridge: a resource with
   an unsatisfiable barb set would have had a capability type just the same.

   What replaces it is the premise written down as a premise:

     * `Axioms.NoFreeInstances r s` is an uninterpreted predicate. It names the ZK layer's
       obligation — that every public input of the circuit for `(r, s)` is
       `constrain_instance`-derived from a witness — without asserting it. Its entry in
       `Axioms.lean` records what would discharge it.
     * `CircuitDerivable r s` is a *structure*, so a caller supplies a proof term rather than
       receiving one for free, and `capabilityType_of_circuitDerivable` is a genuine
       implication whose hypothesis must be inhabited by whoever invokes it.

   `#print axioms capabilityType_of_circuitDerivable` is **not** empty, and this file said it was
   until 2026-09-24. Measured:

       'capabilityType_of_circuitDerivable' depends on axioms: [NoFreeInstances, propext, Quot.sound]

   so the `@[axiom_budget 1]` below is right and the sentence was wrong. Type existence is still
   purely combinatorial — the proof term is
   `CapabilityType.mk (CircuitDerivable.primitives h) (CircuitDerivable.coversBarbs h)` and never
   mentions `noFreeInstances` — but the *axiom set of that term* contains it anyway, and the
   reason is a rule about the model rather than about this proof: **a structure with a `Prop`
   field whose type names an axiom carries that axiom in every one of its projections**, including
   the projections of its data fields. Measured minimally:

       axiom Foo : Prop
       structure S where a : Nat; p : Foo
       def f (s : S) : Nat := s.a     -- '#print axioms f' → [Foo]
       #print axioms S.a              -- → [Foo]

   (`structure T where a : Nat; def g (t : T) : Nat := t.a` is clean.) So `CircuitDerivable.primitives`
   and `CircuitDerivable.coversBarbs` each read budget 1 on their own, and any theorem that
   projects them inherits it. The ZK premise is therefore charged to this theorem's budget while
   not being used by its proof — the over-statement is by exactly one, and it is a fact about
   `structure` rather than about the premise being load-bearing. Saying so is the point: the ZK
   premise *is* what a soundness theorem about capabilities would need, and no such theorem exists
   yet. Dropping the field would have hidden that — and would have made the budget read 0, which
   is the other half of why it is kept.

   The bridge is **one-directional**, and the converse is false in general:
   `Nonempty (CapabilityType r s)` does not imply `CircuitDerivable r s`, because
   privacy-preserving authorization does not need a proof system at all (see the file header).
   ========================================================================== -/

/-- The premise the ZK layer owes the type layer. Inhabiting it is the caller's job: the
    `noFreeInstances` field has no proof in this tree, because Halo2 constraint-system
    semantics are not modelled. -/
structure CircuitDerivable (r : Resource) (s : Action) where
  /-- The primitives the circuit realises. -/
  primitives : List PrimitiveType
  /-- The circuit's constraint set covers the resource's required barbs. -/
  coversBarbs : r.requiredBarbs ⊆ compose primitives
  /-- Every public input is `constrain_instance`-derived from a witness. Supplied by the
      circuit audit over `src/contract/*/proof/*.zk`; not provable in Lean today. -/
  noFreeInstances : NoFreeInstances r s

/-- **Conditional, and one-directional.** Given that a circuit is derivable for `(r, s)`, the
    capability type exists. The proof uses only `coversBarbs`: barb coverage is a
    combinatorial fact about `compose`, and the ZK premise is not needed for *existence* —
    though it is charged to the budget, because the projections carry it (see the file header).
    The converse does not hold; see the file header. -/
@[axiom_budget 1]
theorem capabilityType_of_circuitDerivable (r : Resource) (s : Action)
    (h : CircuitDerivable r s) : Nonempty (CapabilityType r s) :=
  ⟨{ primitives := h.primitives, coversBarbs := h.coversBarbs }⟩

/- ==========================================================================
   Part 2: Capability Type Existence
   ==========================================================================
   For resources with known circuit proofs (native_token transfer, DAO vote,
   tender bid), we prove existence constructively by providing the
   CapabilityType value.

   Note what this does and does not establish. It establishes that the composed
   primitives cover the required barbs — i.e. that the capability type is
   inhabited, which is a statement about `compose`. It does NOT establish that a
   circuit for the pair exists, or that any such circuit is sound. Those are the
   `CircuitDerivable` obligations of Part 1, and they are supplied by the audit.
   ========================================================================== -/

@[axiom_budget 0]
theorem nativeTokenTransferExists : Nonempty (CapabilityType nativeTokenResource transferAction) := by
  apply Nonempty.intro
  exact nativeTokenTransferType

@[axiom_budget 0]
theorem daoVoteExists : Nonempty (CapabilityType daoResource voteAction) := by
  apply Nonempty.intro
  exact daoVoteType

@[axiom_budget 0]
theorem tenderBidExists : Nonempty (CapabilityType tenderResource bidAction) := by
  apply Nonempty.intro
  exact tenderBidType

/- ==========================================================================
   Part 3: Capability Type Existence ↔ Barb Coverage
   ==========================================================================
   For every resource-action pair, the capability type exists (the composition
   covers the required barbs) if and only if the barbs required by the resource
   are a subset of what the composed primitives can exhibit.

   This is a real `iff` and both directions are proved below. It is a statement
   about `compose` and `CapabilityType`, and nothing more.

   It is NOT the Authorization Inversion Theorem, and the docs used to conflate
   the two: this iff is about barb coverage, while the inversion theorem is about
   ZK proof systems, and only its sufficiency direction holds (Part 1).
   ========================================================================== -/

@[axiom_budget 0]
theorem authorizationInversion_TypeLevel (r : Resource) (s : Action) :
  (Nonempty (CapabilityType r s)) ↔
  (∃ (primitives : List PrimitiveType), r.requiredBarbs ⊆ compose primitives) := by
  constructor
  · intro h
    rcases h with ⟨ct⟩
    exact ⟨ct.primitives, ct.coversBarbs⟩
  · intro h
    rcases h with ⟨primitives, h_covers⟩
    apply Nonempty.intro
    exact { primitives := primitives, coversBarbs := h_covers }

/- ==========================================================================
   Part 4: HAZOP Pattern 4 — Capability Predicate Bypass Prevention
   ==========================================================================
   HAZOP.lean documents pattern4_capability_bypass: "capability_predicate_result
   = 1 is free witness; provenance unverified." This is the vulnerability
   class that Authorization Inversion prevents.

   The prevention is: if capability_predicate_result is constrained to
   equal the output of a verified predicate evaluation (LTE gate, IsNotEqual
   gate), then a prover cannot set it arbitrarily. The existing circuit
   proofs (Gadgets.lean: less_than_or_equal_sound, is_not_equal_fully_pure)
   provide the ZK soundness. The type system provides the barbs that the
   circuit must cover.

   THEOREM: if ↓prove ∈ requiredBarbs, then the composition contains a
   primitive that carries ↓prove — so the predicate cannot be free: it comes
   from a declared primitive, and `proveCarriersAreOnlyDleqProof` below says
   which one.

   **What this theorem was, and what it is now.** It proved
   `Barb.prove ∈ compose ct.primitives` by applying `ct.coversBarbs`, i.e. it
   re-exposed the `CapabilityType` field its hypothesis came from — a
   restatement of the type's own definition under a name about bypass
   prevention. It now concludes the *carrier* form via
   `Composition.exists_carrier_of_barb_mem`, which implies the old statement
   (`barbPreservation`) and says strictly more.

   **And the paragraph that stood here was false.** It read "the primitive
   type system has no type with ↓prove as its sole barb — this is intentional:
   ↓prove is a COMPOSITE barb". `Types.lean:232-236` defines `dleqProof` with
   `barbs := {Barb.prove}` and nothing else, and `oracleOperatorType`
   (`Composition.lean:445`) is constructed by relying on exactly that. So
   ↓prove has a sole carrier, it is `DLEqProof`, and it is not composite.

   **What remains unmodelled, stated rather than implied.** The claim's second
   half — that the carrier's predicate is *circuit-verified* — is about Halo2
   constraint systems, which this tree does not model. The carrier lemma
   establishes that the barb is exhibited by a declared primitive; it does not
   establish that the primitive's circuit constrains the result. That is
   `Axioms.NoFreeInstances`'s territory (`OBL-T7`), and it is not discharged.
   ========================================================================== -/

@[axiom_budget 0]
theorem capabilityPredicateBypass_prevention (r : Resource) (s : Action)
    (ct : CapabilityType r s) (h_prove : Barb.prove ∈ r.requiredBarbs) :
    ∃ p ∈ ct.primitives, Barb.prove ∈ p.barbs :=
  exists_carrier_of_barb_mem ct.primitives Barb.prove (ct.coversBarbs h_prove)

/-- The `Bool` computation behind the theorem below, in the shape `Capability/Pareto.lean` uses
    for its table-wide `decide` (`pairsDistinctCheck`): a fold rather than a quantifier, so the
    kernel can reduce it. -/
def proveCarriersOnlyCheck : Bool :=
  allPrimitiveTypes.all fun p =>
    !decide (Barb.prove ∈ p.barbs) || decide (p.barbs = dleqProof.barbs)

/-- Budget 1, and it is the same 1 `Capability/Pareto.lean`'s `pairsDistinctCheck_eq_true` carries:
    `decide` over `Finset Barb` equality reaches `Classical.choice` in this mathlib even though the
    sets are finite, so a `decide`-checked table costs one. Annotated as measured — this gate
    caught the `0` I first wrote, which is the whole point of it. -/
@[axiom_budget 1]
theorem proveCarriersOnlyCheck_eq_true : proveCarriersOnlyCheck = true := by decide

/-- Every primitive that carries `↓prove` has `dleqProof`'s barb set — the fact the comment above
    this theorem denied. With `dleqProof_carries_prove` below, that makes `dleqProof` the **sole**
    carrier: `Types.lean:232-236` gives it `barbs := {Barb.prove}` and nothing else, and
    `chainDepositProof` — the other proof-flavoured primitive — carries `↓prove-inclusion` and
    `↓verify`, not this one. So `↓prove` is not composite. -/
@[axiom_budget 1]
theorem carriers_of_prove_are_dleqProof_barbs {p : PrimitiveType} (hp : p ∈ allPrimitiveTypes)
    (h : Barb.prove ∈ p.barbs) : p.barbs = dleqProof.barbs := by
  have hall : (!decide (Barb.prove ∈ p.barbs) || decide (p.barbs = dleqProof.barbs)) = true :=
    List.all_eq_true.mp proveCarriersOnlyCheck_eq_true p hp
  rw [decide_eq_true h, Bool.not_true, Bool.false_or] at hall
  exact of_decide_eq_true hall

/-- And `dleqProof` is such a carrier, so the set is not empty. -/
@[axiom_budget 0]
theorem dleqProof_carries_prove : Barb.prove ∈ dleqProof.barbs := by decide

/- ==========================================================================
   Part 5: Barb Observability — What the Verifier Learns
   ==========================================================================
   Per ocap.md §3: the verifier observes only the predicate result (1/0),
   the nullifier (ensuring single-use), and the commitment's inclusion
   proof. Everything else is hidden.

   In the type system: the composed barb set of a capability type
   determines what the verifier CAN observe. The hidden witness w is
   NOT a barb — it is the proof term that inhabits the type.
   ========================================================================== -/

structure VerifierObservation (r : Resource) where
  observedBarbs : Finset Barb
  observedBarbs_subset : observedBarbs ⊆ r.requiredBarbs
  hiddenWitness : Bool := true
-- No `deriving Repr`: `observedBarbs : Finset Barb` makes the derived instance depend on
-- `Finset.instRepr`, which is `unsafe`, and Lean rejects the declaration outright —
-- "(kernel) invalid declaration, it uses unsafe declaration 'Finset.instRepr'". Nothing
-- needs `Repr` here; the structure is only the domain of `verifierLearnsOnlyRequiredBarbs`.

/-- **Note on the statement.** This used to fix the action to `transferAction` while `r` stayed
    universally quantified, so it was about `(r, transfer)` for every `r` — a pair the
    hypothesis says nothing about. It is stated over the same `s` the capability type is
    indexed by. The conclusion is barb-set inclusion and nothing more: it does NOT say the
    verifier learns nothing else, because this file does not model what a verifier sees. -/
@[axiom_budget 0]
theorem verifierLearnsOnlyRequiredBarbs (r : Resource) (s : Action) (obs : VerifierObservation r)
    (ct : CapabilityType r s) :
    obs.observedBarbs ⊆ compose ct.primitives := by
  have h_sub : obs.observedBarbs ⊆ r.requiredBarbs := obs.observedBarbs_subset
  have h_covers : r.requiredBarbs ⊆ compose ct.primitives := ct.coversBarbs
  exact Finset.Subset.trans h_sub h_covers
