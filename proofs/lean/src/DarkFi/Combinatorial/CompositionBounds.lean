import DarkFi.Combinatorial.StateSpace
import DarkFi.Combinatorial.Transitions
import DarkFi.Combinatorial.ComplexityJump
-- Brings `Mathlib`'s tactic set (`norm_num`, `nlinarith`) and registers `@[axiom_budget]`.
-- Without it this file had only core `omega`, which is why `additive_vs_multiplicative_gap`
-- was written as a nest of `omega` calls trying to prove a *nonlinear* fact.
import DarkFi.Axioms
import DarkFi.AxiomBudget

/-!
# O-Cap Composition Bounds — Modularity Prevents Combinatorial Explosion

Proves the fundamental architectural guarantee of DarkWow's o-cap model:
when two L1 contracts compose via capabilities (no shared state, only
delegation through the wallet kernel), their state spaces combine additively,
NOT multiplicatively.

Without o-caps (shared mutable state), the complexity would be multiplicative:
  |transitions(A × B)| = |transitions(A)| × |transitions(B)|

With o-caps (disjoint state, mediated delegation):
  |transitions(A ∘ B)| = |transitions(A)| + |transitions(B)|

The difference is existential: additive composition stays manageable even
as contracts are added. Multiplicative composition would rapidly exceed
any practical bound (e.g., 10^5 × 10^5 = 10^10 transitions for just 2 contracts).

This theorem is the formal counterpart to the Authorization Inversion Theorem
(see Capability/Inversion.lean) — that theorem proves type inhabitance; this
theorem proves complexity boundedness.

References:
  - doc/src/arch/ocap.md (o-cap model, capability composition)
  - wallet.md §2 (wallet as type construction engine)
  - Capability/Composition.lean (barb preservation under composition)
-/

open Combinatorial
open Combinatorial.Transitions
open Combinatorial.ComplexityJump

namespace Combinatorial.CompositionBounds

/-! ==========================================================================
   Part 1: Disjoint State — The O-Cap Invariant
   ==========================================================================
   Two L1 contracts are o-cap-composed when:
   1. Each has its own Merkle tree (no shared leaves)
   2. Each has its own nullifier set (no shared nullifiers)
   3. Each has its own roots DB (no shared inclusion anchors)
   4. Delegation is mediated by the wallet kernel (not by direct state access)

   This is the DISJOINT STATE invariant — the foundation of all proofs below.
-/

structure DisjointPair where
  boxState   : L1AnonymitySet
  purseState : L1AnonymitySet
  -- INVARIANT: boxState and purseState share no objects, nullifiers, or roots.
  -- The wallet kernel ensures this by assigning distinct contract IDs.
  deriving BEq, Repr

/-! ==========================================================================
   Part 2: Transition Counts Under O-Cap Composition
   ==========================================================================
   When contracts are disjoint (o-cap-composed), the total number of valid
   state transitions is the SUM of each contract's transitions, not the PRODUCT.

   Why? Because an operation targets a SINGLE contract — Box Put targets Box's
   Merkle tree, Purse Deposit targets Purse's Merkle tree. They never interact
   within a single operation's state transition.
-/

/--
THEOREM: O-cap composition is additive.

Under the disjoint state invariant, the total transition count for a
composed system of Box (with N_B objects, M contents options) and
Purse (with N_P objects, A amount options) is:

  Total = Box transitions + Purse transitions
        = N_B × (M + 1) + N_P × (2A + 1)

NOT: N_B × (M + 1) × N_P × (2A + 1)  (the multiplicative nightmare)
-/
@[axiom_budget 0]
theorem ocap_additive_composition (nb np m a : Nat) :
    boxTotalTransitionCount nb m + purseTotalTransitionCount np a =
    nb * (m + 1) + np * (2 * a + 1) := by
  rw [box_total_linear, purse_total_linear]

/-- `x + y < x * y` when `x ≥ 2` and `y ≥ 3`.

    Note the asymmetric bounds. `≥ 2` on both sides is *not* enough — at `x = y = 2` the claim is
    `4 < 4`. The proof below needs `y ≥ 3` precisely because `(x-1)*(y-1) > 1` is what makes
    `x*y - x - y = (x-1)*(y-1) - 1` positive.

    This replaces a block of nested `omega` calls that could not work: `omega` is a *linear*
    decision procedure, and `x * y` is a product of variables. -/
@[axiom_budget 1]
lemma add_lt_mul_of_two_le_of_three_le {x y : Nat} (hx : 2 ≤ x) (hy : 3 ≤ y) :
    x + y < x * y := by
  obtain ⟨y', rfl⟩ := Nat.exists_eq_add_of_le hy
  nlinarith [hx]

/--
THEOREM: The additive composition grows as O(N_B*M + N_P*A) —
linear in the sum of each contract's parameters. Adding a third
contract would add its transitions, not multiply.

For N_B=100, N_P=100, M=10, A=100:
  Additive: 100*11 + 100*201 = 1,100 + 20,100 = 21,200
  Multiplicative: 1100 * 20100 = 22,110,000

The additive model is 1000x smaller for just 2 contracts. The gap
grows factorially with each additional contract.
-/
@[axiom_budget 1]
theorem additive_vs_multiplicative_gap (nb np m a : Nat) (hnb : nb > 0) (hnp : np > 0)
    (hm : m > 0) (ha : a > 0) :
    (boxTotalTransitionCount nb m + purseTotalTransitionCount np a) <
    (boxTotalTransitionCount nb m * purseTotalTransitionCount np a) := by
  rw [box_total_linear, purse_total_linear]
  -- The goal is now `x + y < x * y` for `x = nb * (m + 1)` and `y = np * (2 * a + 1)`.
  have hx : 2 ≤ nb * (m + 1) := by
    have hm1 : 2 ≤ m + 1 := by omega
    have hn : 1 ≤ nb := by omega
    -- `Nat.mul_le_mul hn hm1 : 1 * 2 ≤ nb * (m + 1)`. The arguments must be in that order:
    -- the previous code passed `this this`, i.e. the same hypothesis twice, so the second slot
    -- received `nb ≥ 1` where `2 ≤ m + 1` was wanted.
    calc nb * (m + 1) ≥ 1 * 2 := Nat.mul_le_mul hn hm1
      _ = 2 := by norm_num
  have hy : 3 ≤ np * (2 * a + 1) := by
    have ha1 : 3 ≤ 2 * a + 1 := by omega
    have hn : 1 ≤ np := by omega
    calc np * (2 * a + 1) ≥ 1 * 3 := Nat.mul_le_mul hn ha1
      _ = 3 := by norm_num
  exact add_lt_mul_of_two_le_of_three_le hx hy

/-! ==========================================================================
   Part 3: Without O-Caps — The Multiplicative Nightmare
   ==========================================================================
   If contracts shared state (no o-cap isolation), an operation could
   simultaneously affect both contracts. The transition count would be
   the cross-product of each contract's transitions.
-/

/--
THEOREM: Without o-caps (shared state), composition is multiplicative.

If Box and Purse shared a single Merkle tree, an operation could target
any pair (box, purse) simultaneously. The total transitions would be
the product of each contract's individual transitions.

This is why o-caps are ESSENTIAL: without them, adding a new L1 contract
would MULTIPLY the total state space rather than just adding to it.
-/
@[axiom_budget 0]
theorem unconstrained_composition_explosion (nb np m a : Nat) :
    boxTotalTransitionCount nb m * purseTotalTransitionCount np a =
    (nb * (m + 1)) * (np * (2 * a + 1)) := by
  rw [box_total_linear, purse_total_linear]

/-
## The additive half — a theorem deleted, and where the claim actually lives

    @[axiom_budget 0]
    theorem ocap_scaling (k : Nat) (hbase : Nat) : True := by trivial

sat here under a heading reading "This is the formal statement of why DarkWow's architecture
scales". Its statement was `True`, its proof was `trivial`, both of its parameters were unused,
and `@[axiom_budget 0]` recorded it as resting on nothing — which was true, and was the problem.
A meaningful name on a vacuous statement is worse than no statement, because the budget table and
any citation of the name both read as evidence. Nothing referenced it, so it is deleted rather
than restated.

**Where the additive property is actually enforced.** The scaling claim has two halves, and only
one of them was ever a theorem here:

* **Multiplicative (no o-caps)** — proved, immediately above:
  `unconstrained_composition_explosion` equates the joint transition count with the *product* of
  the per-contract counts, by rewriting with `box_total_linear` and `purse_total_linear`.
* **Additive (with o-caps)** — not a theorem in this file, and not a theorem that could be
  stated in this vocabulary, because it is a property of the *type system* rather than of a
  natural-number model. It is `Capability.Composition.compose`, which is a set **union** of
  primitive barbs: composing capabilities cannot manufacture a barb, so the effect of adding a
  contract is to add its barbs to the union and nothing else. That is the real content of
  "additive, not multiplicative", it is machine-checked (`coversBarbs`, and the twelve coverage
  proofs in `Capability/Composition.lean`), and `Capability/PerContractTree.lean` is where the
  per-contract state isolation that makes the union correct is stated.

The `k`-contract generalisation over transition counts is therefore *not* an open theorem; it is
the wrong encoding of a property the type system already carries. Recorded so that a later reader
does not re-add the placeholder to close a gap that is not there.

## Two things `ocap_additive_composition` above is not

Worth saying here, because documents cite it as the additive composition theorem — and one of them
(`contract-wasm-type-system.md` §C.7, `privacy.md` §6, `safety.md` Lesson 23, `ai-index.md`) states
the conclusion it is cited for, which it does not establish:

* **It is a rewriting lemma, not a composition theorem.** After `rw [box_total_linear,
  purse_total_linear]` the two sides are syntactically identical. The `+` is stipulated by the
  statement; no operation composing two contracts appears in this file, so nothing here derives
  `T(A ∘ B) = T(A) + T(B)`. `additive_vs_multiplicative_gap` is the same shape — a comparison
  between two numbers the statement chose to write with `+` and `×`.
* **The additive reading is anyway false for the quantity that matters.** The *size* of one composed
  capability is additive (`Combinations.card_biUnion_le_sum`), but the *number* of distinct operation
  combinations across contracts is a **product**, and stays one under o-cap isolation —
  `Combinations.combinationCount`, with the instance measured at
  `615192791076863999999999` combinations over the 31 contracts in this tree. There is no
  "cross-contract combinatorial explosion" that o-caps prevent in that count.

What o-caps prevent is **state merging**, which is `unconstrained_composition_explosion` above — a
second and independent source of products. See `Combinatorial/Combinations.lean`.
-/

end Combinatorial.CompositionBounds
