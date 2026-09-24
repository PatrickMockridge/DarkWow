/-
# The uncle rules — the depth window, the derived pin, and what the proof does not bind

`validation.rs::check_uncles` decides whether a block's uncle set is admissible, and it is the one
piece of `validation.rs` whose rules are arithmetic rather than plumbing: a depth window with both ends
bounded, a pin **re-derived** from the depth rather than trusted, a per-uncle target, and a cross-block
dedup key. This module models the first two, and the reason they are rules at all.

## The depth window, and the two refusals one clause makes

The depth is the Rust's `current_height.saturating_sub(uncle.header.height)` — a **saturating**
subtraction, which is why the rules are one clause and not two. A sibling at the same height lands on
depth 0 and is refused; so does a block claiming a height *above* the referencing block, because the
saturation maps it onto 0 as well. The code documents the coincidence ("`sat_sub` maps a block claiming
a height ABOVE the referencing block onto depth 0, so future-dated uncles are rejected by the same
rule"); `one_clause_refuses_both` is that sentence as a theorem, and it is worth mechanizing because
the *error message* names only the sibling case, so a reader who trusts the message would expect a
future-dated uncle to be rejected somewhere else.

**And the depth-0 refusal is quantitative, which is the part the comment leaves implicit.** The code
says depth 0 "would pay 100% of the base reward"; `admitted_pin_le_half` is the general form —
admitting only depth ≥ 1 caps *every* uncle's pin at half the base reward, for every depth in the
window. So the rule's two ends do different jobs: the lower end bounds each pin, and the upper end
bounds how far back a payable uncle can be.

## The pin is derived because the proof does not bind it

The code's argument is stated carefully and it is exact:

> The pin is DERIVED, not trusted: `pin_confirmed_i = base_reward / 2^depth_i`. `pin_confirmed` is a
> wire field the producer controls AND it sits outside `uncle_merkle_root` (which commits only to the
> header), so without this check a relaying producer could rewrite the split for any included uncle.

`derivation_is_load_bearing` is that argument as a theorem, and it is stated so that it cannot be a
tautology: the merkle proof is modelled as a relation on the **header alone** — which is what "commits
only to the header" means — so there exist two uncles that share a header, are *both* proof-valid, and
are separated only by the pin rule. The proof cannot substitute for the derivation, and that is a
statement about the proof's domain rather than about the pin arithmetic.

A rejected pin (`pin_accepted == false`) pays nothing, so its field is deliberately unconstrained
(`rejected_pin_is_unconstrained`), and the rule admits the derived value and nothing else
(`rewritten_pin_is_refused`).

## The bounds are necessary and not sufficient, and the model says which check is which

`MAX_UNCLE_COUNT` bounds the *number* of uncles and `MAX_UNCLE_DEPTH` bounds each one's depth value —
and neither requires the depths to be **distinct**. The code's own comment gives the count bound its
reason ("One uncle per depth level is the natural bound"), and `three_uncles_at_one_depth_satisfy_every_
rule` is the measurement that nothing enforces it: three uncles at depth 1 are within the count bound and
inside the window, and their pins exceed the base reward. Whether that is permitted is not this rule's
question — it is `Consensus/CoinbaseSplit.lean`'s, where `Σ pin` is bounded by the note-level split. So
the two modules model two checks of the same reward, and this is the half that does *not* bound the sum.

## The alignment guard, and an asymmetry that was closed

`check_uncles` indexes two slices by the same index: `uncle_targets[i]` and `proofs[i]`. It guarded the
**first** — `uncle_targets.len() != uncles.len()` is an error, with the reason stated ("a mismatch would
silently skew every PoW verdict, so fail closed on it") — and did **not** guard the second, which the
loop reads for every uncle.
`guard_buys_every_index` and `unaligned_has_an_uncovered_index` are any guard's purpose as a theorem:
alignment is exactly what makes every in-range index of one slice in range of the other, and without it
there is an index covered by one list and not the other.

**The asymmetry's reachability was measured, 2026-09-24, and it is not reachable from untrusted input.**
`UncleProof` appears nowhere outside `src/linear/src/` — it is not serialized, not decoded from the wire,
and its only constructor is `build_uncle_merkle`, which emits exactly `uncles.len()` proofs; the single
production caller (`bin/dwowd/src/block_acceptor.rs`) passes that same vector. So it was an API contract
on a `pub` function rather than a live defect, which is why it was recorded as its own register row
rather than presented as either.

**Closed 2026-09-24 (`OBL-C114`): the second guard is now beside the first**, and the measurement above
is what the severity rested on rather than a reason to leave it. The theorem pair needed no
restatement for it — both are stated over `{α β : Type}`, any two lists, so the same two lemmas cover
the slice that was already guarded and the one that was not. The paragraph this replaces said the
asymmetry was "**not** modelled here — a model would be modelling a hazard rather than a rule"; that
was the right call while the hazard stood, and the lemmas were the rule's statement either way.

## What this does not model

* **The RandomX verification.** `verify_uncle_proof` re-hashes the uncle's header with the uncle's own
  key and enforces the supplied target. The *per-uncle target* is modelled only as the alignment rule
  above; the PoW itself is not, for the reason every module in this directory gives — the hash is not a
  function this layer has.
* **The dedup key's blake3 form.** Register row `OBL-C27` records the key as correct "only while every
  key is a 32-byte blake3 hash, which is a fragile invariant rather than an enforced one". The model
  therefore treats dedup as *distinctness of headers* and uses it only where a statement needs two
  uncles that do not collide, rather than pretending to model the encoding.
* **The merkle root's recomputation.** The caller builds the root from the block's uncles and compares it
  to the header; that is a caller contract (the docstring calls it `P2-3`) and it is not a rule inside
  this function.
* **Nothing here is a claim about the Rust.** The rules are transcribed from `check_uncles` and from
  `contrib/model/chain_validation_model.py`'s `check_uncles`, which models the same depth window and the
  same derivation. -/

import DarkFi.Consensus.CoinbaseSplit
import DarkFi.AxiomBudget

namespace Consensus.UncleRules

open Consensus.CoinbaseSplit (splitForUncle)

/-- The code's per-block uncle limit. -/
def MAX_UNCLE_COUNT : Nat := 6

/-- The code's depth window. -/
def MAX_UNCLE_DEPTH : Nat := 6

/-! ===== Part 1 — the depth window, both ends ===== -/

/-- **The depth a referencing block assigns an uncle at height `h`**: the Rust's
    `current.saturating_sub(h)`. Saturation is the whole of why the rules are one clause: a height at or
    above `current` lands on 0 rather than wrapping. -/
def depthOf (current h : Nat) : Nat := if h ≤ current then current - h else 0

/-- **The depth rule**: an admitted uncle is at depth `1 .. MAX_UNCLE_DEPTH`. The lower end is why a
    sibling is not an uncle; the upper end is how far back payable work reaches. -/
def depthAdmits (current h : Nat) : Prop :=
  1 ≤ depthOf current h ∧ depthOf current h ≤ MAX_UNCLE_DEPTH

/-- A sibling — a block at the referencing height — is at depth 0. -/
@[axiom_budget 0]
theorem sibling_maps_to_zero (current : Nat) : depthOf current current = 0 := by
  unfold depthOf
  simp

/-- And so is a block claiming a height **above** the referencing block: the saturating subtraction has
    no other value to give it. -/
@[axiom_budget 0]
theorem future_maps_to_zero (current h : Nat) (hge : current ≤ h) : depthOf current h = 0 := by
  unfold depthOf
  split
  · omega
  · rfl

/-- **One clause, two refusals.** The depth rule refuses a sibling and a future-dated uncle alike, which
    is what the code's comment says and what its error message does not: the message names only the
    sibling case. -/
@[axiom_budget 0]
theorem one_clause_refuses_both (current : Nat) :
    ¬ depthAdmits current current ∧ ∀ h, current ≤ h → ¬ depthAdmits current h := by
  constructor
  · intro h
    rw [depthAdmits, sibling_maps_to_zero current] at h
    omega
  · intro h hge had
    rw [depthAdmits, future_maps_to_zero current h hge] at had
    omega

/-- Inside the window the depth is the ordinary difference, so the rule is stated without subtraction
    for the reason the neighbouring modules record — the truncated form needs a case split at every
    step. -/
@[axiom_budget 0]
theorem admitted_iff (current h : Nat) (hlt : h < current) :
    depthAdmits current h ↔ 1 ≤ current - h ∧ current - h ≤ MAX_UNCLE_DEPTH := by
  unfold depthAdmits depthOf
  rw [if_pos (by omega)]

/-- The depth never exceeds the referencing height, so the window is empty below genesis. -/
@[axiom_budget 0]
theorem depth_le_current (current h : Nat) : depthOf current h ≤ current := by
  unfold depthOf
  split
  · exact Nat.sub_le _ _
  · exact Nat.zero_le _

/-- **Admitting only depth ≥ 1 caps every uncle's pin at half the base reward.** This is the depth-0
    rule's quantitative content — the code states the case ("Admitting depth 0 would pay 100% of the
    base reward"), and this is the general form, for every depth the window allows. -/
@[axiom_budget 1]
theorem admitted_pin_le_half (base current h : Nat) (hok : depthAdmits current h) :
    splitForUncle base (depthOf current h) ≤ base / 2 := by
  obtain ⟨h1, -⟩ := hok
  unfold splitForUncle
  split
  · exact Nat.zero_le _
  · have hpow : (2 : Nat) ≤ 2 ^ depthOf current h := Nat.pow_le_pow_right (n := 2) (by norm_num) h1
    exact Nat.div_le_div_left (a := base) (b := 2 ^ depthOf current h) (c := 2) hpow (by norm_num)

/-! ===== Part 2 — the alignment guard, and what it buys =====

`check_uncles` indexes `uncle_targets[i]` and `proofs[i]` by one index, and now guards both slices'
lengths. These two theorems are what a length guard is *for*, stated as a property rather than as
prose — and because they are stated over `{α β : Type}` rather than over these two slices, they were
already the statement of the guard `OBL-C114` added, and needed no restatement when it landed. -/

/-- Alignment, as the guard checks it. -/
def aligned {α β : Type} (xs : List α) (ys : List β) : Prop := xs.length = ys.length

/-- **What the guard buys**: every index covered by the first slice is covered by the second, so
    `ys[i]` is safe for every `xs[i]` the loop reaches. The code states the reason — a mismatch "would
    silently skew every PoW verdict" — and this is the mechanical half of it. -/
@[axiom_budget 0]
theorem guard_buys_every_index {α β : Type} (xs : List α) (ys : List β)
    (h : aligned xs ys) : ∀ i, i < xs.length → i < ys.length := by
  intro i hi
  rw [aligned] at h
  omega

/-- **And without it there is an index that is covered by one slice and not the other** — the case the
    guard exists to make impossible. -/
@[axiom_budget 0]
theorem unaligned_has_an_uncovered_index {α β : Type} (xs : List α) (ys : List β)
    (h : ys.length < xs.length) : ∃ i, i < xs.length ∧ ¬ i < ys.length :=
  ⟨ys.length, h, by omega⟩

/-! ===== Part 3 — the derived pin, and the proof's blindness ===== -/

/-- An uncle as the rules see it: the **header** the merkle proof binds — its height and its miner —
    and the two pin fields that sit outside it. -/
structure Uncle where
  /-- The uncle's own height, which fixes its depth. -/
  height : Nat
  /-- The uncle's declared miner, which is what the dedup key is computed over. -/
  miner : Nat
  /-- Whether the uncle accepted the pin. A rejected pin pays nothing. -/
  pinAccepted : Bool
  /-- The declared pin. A wire field the producer controls, outside the merkle root. -/
  pinConfirmed : Nat

/-- **The merkle proof, as a relation on the header alone.** This is not a simplification: the code's
    own statement is that `uncle_merkle_root` "commits only to the header", so whatever the proof
    establishes, it establishes about `(height, miner)` and nothing else. -/
def proofOk (ok : Nat → Nat → Prop) (u : Uncle) : Prop := ok u.height u.miner

/-- **The pin rule**: a rejected pin pays nothing and is deliberately unconstrained; an accepted pin
    must equal the value re-derived from the depth. -/
def pinAdmits (base current : Nat) (u : Uncle) : Prop :=
  u.pinAccepted = false ∨ u.pinConfirmed = splitForUncle base (depthOf current u.height)

/-- The uncle the derivation admits at a header: accepted, carrying the re-derived pin. Named because
    `derivation_is_load_bearing`'s witnesses are this uncle and a rewrite of it — which is the whole
    point of the theorem. -/
def derivedUncle (base current h m : Nat) : Uncle :=
  { height := h, miner := m, pinAccepted := true,
    pinConfirmed := splitForUncle base (depthOf current h) }

/-- A rejected pin is unconstrained — the code says so ("A rejected pin … pays nothing, so its field is
    unused and is deliberately not constrained"), and this is that. -/
@[axiom_budget 0]
theorem rejected_pin_is_unconstrained (base current : Nat) (u : Uncle)
    (h : u.pinAccepted = false) : pinAdmits base current u := Or.inl h

/-- The derived value is admitted. -/
@[axiom_budget 0]
theorem derived_pin_is_admitted (base current : Nat) (u : Uncle) :
    pinAdmits base current { u with pinConfirmed := splitForUncle base (depthOf current u.height) } :=
  Or.inr rfl

/-- **A rewritten pin is refused**, which is the check a relaying producer meets. -/
@[axiom_budget 0]
theorem rewritten_pin_is_refused (base current p : Nat) (u : Uncle)
    (hacc : u.pinAccepted = true) (hne : p ≠ splitForUncle base (depthOf current u.height)) :
    ¬ pinAdmits base current { u with pinConfirmed := p } := by
  intro h
  rw [pinAdmits] at h
  rcases h with h | h
  · simp [hacc] at h
  · exact hne h

/-- **The derivation is load-bearing, and the proof cannot substitute for it.** Two uncles that share a
    header are indistinguishable to the merkle proof — so it is valid of both — and the pin rule is the
    only thing that separates them. This is the code's argument ("it sits outside `uncle_merkle_root` …
    so without this check a relaying producer could rewrite the split") as a theorem, and the reason it
    is not a tautology is that the proof's domain is the header: the statement is about what the proof
    *cannot* see, and both witnesses are exhibited rather than asserted to exist. -/
@[axiom_budget 0]
theorem derivation_is_load_bearing (ok : Nat → Nat → Prop) (base current h m : Nat)
    (hok : ok h m) (hdepth : depthAdmits current h) :
    ∃ u u' : Uncle,
      u.height = h ∧ u'.height = h ∧ u.miner = m ∧ u'.miner = m ∧
      proofOk ok u ∧ proofOk ok u' ∧
      pinAdmits base current u ∧ ¬ pinAdmits base current u' ∧
      depthAdmits current u.height := by
  let p := splitForUncle base (depthOf current h)
  refine ⟨derivedUncle base current h m,
          { derivedUncle base current h m with pinConfirmed := p + 1 },
          ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · simp [derivedUncle]
  · simp [derivedUncle]
  · simp [derivedUncle]
  · simp [derivedUncle]
  · simpa [proofOk, derivedUncle] using hok
  · simpa [proofOk, derivedUncle] using hok
  · exact derived_pin_is_admitted base current (derivedUncle base current h m)
  · intro hcon
    refine rewritten_pin_is_refused base current _ _ ?_ ?_ hcon
    · simp [derivedUncle]
    · simp only [derivedUncle]; omega
  · simpa [derivedUncle] using hdepth

/-! ===== Part 4 — the bounds are necessary and not sufficient =====

The window bounds each depth value and the count bounds how many uncles there are. Neither requires the
depths to be distinct — so a set can satisfy every rule in this module while paying out more than the
base reward. Bounding the sum is the split check's job, one module over. -/

/-- The count rule, as the code states it. -/
def countAdmits (uncles : List Uncle) : Prop := uncles.length ≤ MAX_UNCLE_COUNT

/-- **Three uncles at one depth satisfy every rule in this module** — within `MAX_UNCLE_COUNT`, inside
    the depth window, each at depth 1 — and their pins exceed the base reward. That is the measurement
    the code's own comment needs: it gives the count bound its reason as "One uncle per depth level is
    the natural bound", and one-per-depth is enforced nowhere. So this module's rules are necessary and
    not sufficient, and the sum is bounded by `Consensus/CoinbaseSplit.lean`'s split check rather than
    by anything here. The three miners differ, so the dedup key — computed over the header — does not
    collide and does not refuse the second or third. -/
@[axiom_budget 1]
theorem three_uncles_at_one_depth_satisfy_every_rule :
    countAdmits [⟨99, 1, true, 0⟩, ⟨99, 2, true, 0⟩, ⟨99, 3, true, 0⟩] ∧
    (∀ h, h = 99 → depthAdmits 100 h) ∧
    splitForUncle 100 (depthOf 100 99) * 3 > 100 := by
  refine ⟨?_, ?_, ?_⟩
  · unfold countAdmits MAX_UNCLE_COUNT
    decide
  · rintro h rfl
    norm_num [depthAdmits, depthOf, MAX_UNCLE_DEPTH]
  · norm_num [depthOf, splitForUncle]

/-! ===== Part 5 — non-vacuity, at concrete heights =====

At `current = 10` and `MAX_UNCLE_DEPTH = 6` the admitted heights are exactly `4..9`, and the refusals are
the three the rules are for: a sibling, a future-dated block, and one past the window. Stated together
so neither direction is asserted of nothing — a predicate that refused everything would satisfy the
refusals alone. -/

/-- **The window's refusals and its admitted band, exercised rather than asserted.** -/
@[axiom_budget 1]
theorem window_witness :
    (∀ h, 4 ≤ h ∧ h ≤ 9 → depthAdmits 10 h) ∧
    ¬ depthAdmits 10 10 ∧ ¬ depthAdmits 10 11 ∧ ¬ depthAdmits 10 3 := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · intro h hh
    unfold depthAdmits depthOf MAX_UNCLE_DEPTH
    split <;> omega
  · norm_num [depthAdmits, depthOf, MAX_UNCLE_DEPTH]
  · norm_num [depthAdmits, depthOf, MAX_UNCLE_DEPTH]
  · norm_num [depthAdmits, depthOf, MAX_UNCLE_DEPTH]

/-- **And a legal uncle is admitted with its derived pin, while a rewritten one is not** — the two sides
    of the pin rule at one concrete height, so the rule is neither always-true nor always-false. -/
@[axiom_budget 1]
theorem pin_witness :
    pinAdmits 100 10 ⟨8, 7, true, splitForUncle 100 (depthOf 10 8)⟩ ∧
    ¬ pinAdmits 100 10 ⟨8, 7, true, 0⟩ ∧
    pinAdmits 100 10 ⟨8, 7, false, 0⟩ := by
  refine ⟨?_, ?_, ?_⟩ <;> (unfold pinAdmits depthOf splitForUncle) <;> norm_num

end Consensus.UncleRules
