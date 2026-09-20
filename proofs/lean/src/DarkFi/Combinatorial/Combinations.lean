import Mathlib
import DarkFi.AxiomBudget

/-!
# Combinations of L1 operations — growth in the number of contracts

## The axis this file adds

`Combinatorial/Transitions.lean` counts **within one contract**: `l1TrajectoryCount N K = N ^ K` is
the number of K-step trajectories over N anonymous objects, and `boxTotalTransitionCount N M = N * (M + 1)`
is the branching per operation. Neither mentions how many contracts there are, so the layer had no
vocabulary in which a statement about *adding contracts* could be made — which is why
`CompositionBounds.lean` came to carry

    @[axiom_budget 0] theorem ocap_scaling (k : Nat) (hbase : Nat) : True := by trivial

under the heading "This is the formal statement of why DarkWow's architecture scales". That theorem
was deleted; this file is the vocabulary.

## The quantity

`C` contracts, contract `i` offering `nᵢ` state-transition operations. A *combination* is a choice,
per contract, of either "do nothing" or one of that contract's operations — the all-nothing choice
excluded:

    combinationCount [n₁, …, n_C] = ∏(nᵢ + 1) − 1

It is a **product**, because the choices are *simultaneous*: choosing operation `a` from contract 1
and `b` from contract 2 is one combination, and the number of such pairs is the product of the
option counts. For `nᵢ ≥ 1` each factor is at least 2, so the count is at least `2^C − 1` —
exponential in the number of contracts, and in particular superlinear.

The `− 1` is carried separately, as `combinationsIncludingIdle`, because every product law is clean
on the idle-inclusive quantity and `Nat` subtraction is not. `combinationCount` is then one `sub`.

## What this corrects

`CompositionBounds.ocap_additive_composition` and the documents that cite it as the additive
composition theorem (`privacy.md`, `safety.md`, `contract-wasm-type-system.md` C.7) state that
o-cap composition is *additive*: `T(A ∘ B) = T(A) + T(B)`, "the state spaces add, not multiply",
and that this "prevents cross-contract combinatorial explosion". That conflates two quantities, and
`sum_succ_lt_combinationsIncludingIdle` below is the counterexample:

* **containment** — the *size* of one composed capability is additive. The barbs a composition
  exhibits are a union, so `|⋃_{c∈S} B c| ≤ Σ_{c∈S} |B c|` (`card_biUnion_le_sum`). This is what
  bounds blast radius, and it really is additive.
* **combination count** — the *number* of distinct combinations is a **product**, and remains one
  under o-cap isolation, because you choose one operation per involved contract *simultaneously*.
  Isolating contract *state* does not divide the number of ways to combine contracts.

Both are true. Only the first is additive. The layer previously stated the second in the vocabulary
of the first, which is how a `True` acquired a heading about scaling.
-/

namespace Combinatorial.Combinations

/-! ==========================================================================
   Part 1: The model
   ==========================================================================
-/

/-- An L1 contract, as the combination analysis sees it: it offers `ops` distinct
    state-transition operations. Nothing else about it enters — whether those are
    `put`/`take`, `deposit`/`withdraw`/`balance`, or anything else, reaches the counting only
    through the number. -/
structure L1Contract where
  ops : Nat
deriving BEq, Repr

/-- Every way to choose, per contract, either no action or one of its operations — *including* the
    choose-nothing-everywhere combination. This is the primitive quantity: it is `List.prod` of the
    option counts, so it satisfies `List.prod`'s laws with no subtraction in the way. -/
def combinationsIncludingIdle (ops : List Nat) : Nat :=
  (ops.map (· + 1)).prod

/-- The number of *actual* combinations: the idle-inclusive count less the one combination in which
    nothing happens. A **product** because the choices are simultaneous. Contrast `List.sum`, which
    is what the additive claim would give and which `combinationCount_gt_sum` refutes. -/
def combinationCount (ops : List Nat) : Nat :=
  combinationsIncludingIdle ops - 1

/-- The same count, reading a contract list rather than operation counts. -/
def contractCombinationCount (cs : List L1Contract) : Nat :=
  combinationCount (cs.map L1Contract.ops)

/-- Every contract offers at least one operation. Stated as a hypothesis rather than assumed, so
    that a contract with no operations makes the exponential lower bound *inapplicable* rather than
    false — the same discipline as `l1_exceeds_l2`'s `N ≥ 2`. -/
def AllPositive (ops : List Nat) : Prop := ∀ n ∈ ops, 1 ≤ n

/-! ==========================================================================
   Part 2: Product laws
   ==========================================================================
-/

/-- `∏ (nᵢ + 1) ≥ 1` — a product of factors each at least 1. -/
@[axiom_budget 0]
theorem one_le_combinationsIncludingIdle (ops : List Nat) :
    1 ≤ combinationsIncludingIdle ops := by
  unfold combinationsIncludingIdle
  induction ops with
  | nil => simp
  | cons a t ih =>
      simp only [List.map_cons, List.prod_cons]
      exact Nat.mul_le_mul (by omega : 1 ≤ a + 1) ih

/-- **Appending a contract multiplies the idle-inclusive count by its option count.**

    `n + 1` new selections of the appended contract against each existing combination, and no
    correction — the idleness of the new contract is one of those `n + 1` options rather than a
    special case. This is why the idle-inclusive formulation is the right primitive: the law is
    `List.prod_append` and nothing else. -/
@[axiom_budget 0]
theorem combinationsIncludingIdle_append (ops : List Nat) (n : Nat) :
    combinationsIncludingIdle (ops ++ [n]) = (n + 1) * combinationsIncludingIdle ops := by
  unfold combinationsIncludingIdle
  simp [List.prod_append, Nat.mul_comm]

/-- **Each added contract with at least one operation at least doubles the count.**

    The sharp form of superlinearity. "Superlinear" for a nondecreasing integer sequence is easy to
    satisfy by accident; *at least doubling at every step* is not, and it is what makes the growth
    exponential rather than merely unbounded. -/
@[axiom_budget 0]
theorem combinationsIncludingIdle_append_doubles (ops : List Nat) (n : Nat) (hn : 1 ≤ n) :
    2 * combinationsIncludingIdle ops ≤ combinationsIncludingIdle (ops ++ [n]) := by
  rw [combinationsIncludingIdle_append]
  exact Nat.mul_le_mul_right _ (by omega : 2 ≤ n + 1)

/-- **The increment is at least the whole count so far** — equivalently `f (C+1) ≥ 2 · f C`.

    Superlinearity with no arithmetic left over. A function with *constant* increment `c` — that is,
    `f C = c · C` — has `f (C+1) − f C = c` at every step, the same number each time. Here the
    increment is at least the accumulated value, so the increment itself grows without bound and no
    constant can be its increment. Stated as a comparison of `Nat`s rather than as `f (C+1) ≥ 2 * f C`
    so that the subtraction is the ordinary one and division never appears. -/
@[axiom_budget 0]
theorem increment_ge_value (ops : List Nat) (n : Nat) (hn : 1 ≤ n) :
    combinationsIncludingIdle ops ≤
      combinationsIncludingIdle (ops ++ [n]) - combinationsIncludingIdle ops := by
  rw [combinationsIncludingIdle_append, Nat.add_mul, Nat.one_mul, Nat.add_sub_cancel]
  exact Nat.le_mul_of_pos_left _ hn

/-! ==========================================================================
   Part 3: The exponential lower bound
   ==========================================================================
-/

/-- **The exponential lower bound.** If every contract offers at least one operation, each factor of
    the product is at least 2, so the idle-inclusive count is at least `2^C`.

    This is the whole content of "combinations of L1 o-caps grow exponentially with the number of
    circuits", and it holds for *every* family with `nᵢ ≥ 1` — it does not depend on the contracts
    being disjoint, on their barbs being distinguishable, or on anything else about them. -/
@[axiom_budget 0]
theorem two_pow_le_combinationsIncludingIdle (ops : List Nat) (h : AllPositive ops) :
    2 ^ ops.length ≤ combinationsIncludingIdle ops := by
  unfold combinationsIncludingIdle
  induction ops with
  | nil => simp
  | cons a t ih =>
      have ha : 2 ≤ a + 1 := by
        have := h a (by simp)
        omega
      have ht : AllPositive t := fun n hn => h n (by simp [hn])
      have ih' := ih ht
      simp only [List.map_cons, List.prod_cons, List.length_cons, Nat.pow_succ]
      calc 2 ^ t.length * 2 ≤ (t.map (· + 1)).prod * (a + 1) := Nat.mul_le_mul ih' ha
        _ = (a + 1) * (t.map (· + 1)).prod := Nat.mul_comm _ _

/-- **The headline, in terms of actual combinations**: at least `2^C − 1`. -/
@[axiom_budget 0]
theorem two_pow_sub_one_le_combinationCount (ops : List Nat) (h : AllPositive ops) :
    2 ^ ops.length - 1 ≤ combinationCount ops := by
  unfold combinationCount
  exact Nat.sub_le_sub_right (two_pow_le_combinationsIncludingIdle ops h) 1

/-- **Unconditional upper bound.** At most one combination per selection, so the count is at most
    the idle-inclusive product. True of every family, including degenerate ones; it is the bound
    that o-cap isolation does *not* improve. -/
@[axiom_budget 0]
theorem combinationCount_le_prod (ops : List Nat) :
    combinationCount ops ≤ combinationsIncludingIdle ops :=
  Nat.sub_le _ _

/-! ==========================================================================
   Part 4: The correction — the count is a product, not a sum
   ==========================================================================
   This refutes the additive claim rather than only disagreeing with it in prose.
-/

/-- `Σ nᵢ + 1 ≤ ∏(nᵢ + 1)` — for *every* list, with no positivity hypothesis at all.

    The reason is that the product already contains every summand plus the `+1`: expanding
    `∏(nᵢ+1)` gives `∏nᵢ + … + Σnᵢ + 1`, and `∏nᵢ ≥ 0`. At length 0 and 1 it is an equality, which
    is exactly why the *strict* version below needs two contracts. -/
@[axiom_budget 1]
theorem sum_succ_le_combinationsIncludingIdle (l : List Nat) :
    l.sum + 1 ≤ combinationsIncludingIdle l := by
  unfold combinationsIncludingIdle
  induction l with
  | nil => simp
  | cons a t ih =>
      simp only [List.sum_cons, List.map_cons, List.prod_cons]
      have hP : 1 ≤ (t.map (· + 1)).prod := one_le_combinationsIncludingIdle t
      -- `a + (t.sum + 1) ≤ a + P ≤ (a + 1) * P`; the second step is `a ≤ a * P` from `P ≥ 1`.
      calc a + t.sum + 1 = a + (t.sum + 1) := by ring
        _ ≤ a + (t.map (· + 1)).prod := Nat.add_le_add_left ih a
        _ ≤ (a + 1) * (t.map (· + 1)).prod := by nlinarith

/-- **The count strictly exceeds `Σ nᵢ + 1`**, for any family of at least two contracts each
    offering at least one operation.

    `Σ nᵢ` is what "the state spaces add, not multiply" would give for this quantity. It is lower,
    always: two contracts with 2 operations each compose into `3 · 3 − 1 = 8` combinations where
    addition gives `2 + 2 = 4`.

    Stated on two fixed leading elements and a tail, rather than on `n :: rest` with a length
    hypothesis, because the claim genuinely **fails** below length 2 — for `[n]` the count is `n`
    and the sum is `n`, so a single contract does not distinguish the two readings, and for `[]`
    both are `0`. Two contracts do, and that is the sharpest place to say it. -/
@[axiom_budget 1]
theorem sum_succ_lt_combinationsIncludingIdle (a b : Nat) (t : List Nat)
    (ha : 1 ≤ a) (hb : 1 ≤ b) :
    (a :: b :: t).sum + 1 < combinationsIncludingIdle (a :: b :: t) := by
  simp only [combinationsIncludingIdle, List.sum_cons, List.map_cons, List.prod_cons]
  -- `(b :: t).sum + 1 ≤ P` where `P` is the tail's product, and `2 ≤ P` because `b ≥ 1`.
  have hP1 : b + t.sum + 1 ≤ (b + 1) * (List.map (fun x => x + 1) t).prod := by
    simpa only [combinationsIncludingIdle, List.sum_cons, List.map_cons, List.prod_cons] using
      sum_succ_le_combinationsIncludingIdle (b :: t)
  have hP2 : 2 ≤ (b + 1) * (List.map (fun x => x + 1) t).prod := by
    have hb2 : 2 ≤ b + 1 := by omega
    have h1 : 1 ≤ (List.map (fun x => x + 1) t).prod := one_le_combinationsIncludingIdle t
    calc 2 = 2 * 1 := by ring
      _ ≤ (b + 1) * (List.map (fun x => x + 1) t).prod := Nat.mul_le_mul hb2 h1
  -- Goal: `a + (b + t.sum) + 1 < (a + 1) * P`. From `hP1`, the left is at most `a + P`, and
  -- `a + P < (a + 1) * P` because `a ≥ 1` and `P ≥ 2`.
  nlinarith [hP1, hP2, ha]

/-- **The count is not the sum** — the refutation of the additive claim as a statement about
    `combinationCount` rather than about the idle-inclusive product.

    `combinationCount = idle − 1` and `Nat.lt_sub_iff_add_lt`, so this is the previous theorem
    restated. It is the one the prose citations should point at. -/
@[axiom_budget 1]
theorem combinationCount_gt_sum (a b : Nat) (t : List Nat) (ha : 1 ≤ a) (hb : 1 ≤ b) :
    (a :: b :: t).sum < combinationCount (a :: b :: t) := by
  unfold combinationCount
  rw [Nat.lt_sub_iff_add_lt]
  exact sum_succ_lt_combinationsIncludingIdle a b t ha hb

/-! ==========================================================================
   Part 5: What *is* additive — the sizes, not the counts
   ==========================================================================
   The reconciliation. `compose` (`Capability/Composition.lean`) is a union, and a union is at most
   as large as the sum of its parts. That is the additive law, it is the one that bounds the blast
   radius of a composed capability, and it is a different quantity from the count above.
-/

/-- **The size of one composed capability is additive.** The barbs a selection of capabilities
    exhibits are the union of theirs, and a union is at most as large as the sum of the parts.

    This is the precise content of containment in `ocap.md` §5.1: a composed capability cannot
    exhibit a barb outside the union, so its blast radius is bounded by the sum of its primitives' —
    *regardless* of how many combinations of those primitives exist. The two facts are independent,
    and `ocap.md`'s "blast radius is bounded" is this one, not a bound on the count. -/
@[axiom_budget 1]
theorem card_biUnion_le_sum {ι α : Type*} [DecidableEq α] (s : Finset ι) (B : ι → Finset α) :
    (s.biUnion B).card ≤ ∑ c ∈ s, (B c).card :=
  Finset.card_biUnion_le

/-! ==========================================================================
   Part 6: The instance, machine-checked
   ==========================================================================
   The numbers below are the ones the documents quote. They are `norm_num`-checked from the
   per-contract counts rather than transcribed, so a count that drifts fails the build instead of
   silently invalidating a sentence in `privacy.md`.

   **Where the counts come from, and what they are.** Each `.zk` file under `src/contract/*/proof/`
   contains exactly one `circuit "…"` declaration; `contractOps` is the count of those per contract,
   measured over the tree at 31 contracts / 166 circuits. That count is a *proxy* for an operation
   count and is an **upper** bound on it — `init`, `register_type` and `update_config` are circuits
   that are not state-transition operations. Nothing below depends on the proxy being tight: the
   exponential claim in Part 3 needs only `nᵢ ≥ 1`, and every contract here has at least 2.
-/

/-- Circuit counts per contract, `src/contract/*/proof/`, in sorted directory order. Each entry is
    the number of `circuit "…"` declarations in that contract's `proof/` directory, and **every
    `.zk` file in the tree contains exactly one**, so this is also the file count. The names are
    listed here because a bare `List Nat` is not auditable on its own:

        attestation 10, auction 6, baccarat 4, bearer_bond 4, betting_stake 5, box 2, bridge 2,
        dao_escrow 7, darkbet_exchange 10, darktoshi_dice 4, dex 8, drain_protection 9, escrow 5,
        game_room 12, identity 2, insurance_market 4, labor_market 9, lottery 6, multisig 3,
        native_token 3, oracle 5, otc_swap 4, pool_stake 4, promissory_note 5, purse 3,
        relayer_endowment 3, roulette 4, slot 3, stablecoin 10, subscription 5, tender 5

    Re-derive with `grep -c 'circuit "'` per directory; the list was checked against that
    inventory rather than transcribed from it. -/
def contractOps : List Nat :=
  [10, 6, 4, 4, 5, 2, 2, 7, 10, 4, 8, 9, 5, 12, 2, 4, 9, 6, 3, 3, 5, 4, 4, 5, 3, 3, 4, 3, 10, 5, 5]

/-- **The headline instance.** 31 contracts, 166 circuits, and
    `∏(nᵢ + 1) − 1 = 615 192 791 076 863 999 999 999` distinct operation combinations — 24 digits,
    against a `2³¹ − 1 ≈ 2.1 × 10⁹` lower bound and a `Σ nᵢ = 166` additive reading.

    The number is checked, not typed: `norm_num` reduces the product in the kernel. -/
@[axiom_budget 0]
theorem contractOps_combinationCount :
    combinationCount contractOps = 615192791076863999999999 := by
  norm_num [combinationCount, combinationsIncludingIdle, contractOps]

/-- **The genesis subset.** The six genesis contracts present in the inventory — box (2 circuits),
    bridge (2), dao_escrow (7), native_token (3), promissory_note (5), purse (3) — give
    `∏(nᵢ + 1) − 1 = 6911` combinations, against `2⁶ − 1 = 63` for the lower bound. -/
def genesisOps : List Nat := [2, 2, 7, 3, 5, 3]

@[axiom_budget 0]
theorem genesisOps_combinationCount : combinationCount genesisOps = 6911 := by
  norm_num [combinationCount, combinationsIncludingIdle, genesisOps]

/-- **The positivity hypothesis is necessary, not decorative.** Two contracts offering no
    operations give `2² − 1 = 3 > 0 = combinationCount [0, 0]`, so the exponential lower bound
    genuinely fails without `AllPositive`. Recorded as a proof rather than as a note because a
    hypothesis that no model can violate is indistinguishable from decoration, and this one can be
    violated — by a contract with no operations, which is the case the hypothesis names.

    The *upper* bound is unconditional and survives this instance:
    `combinationCount [0, 0] = 0 ≤ 1 = combinationsIncludingIdle [0, 0]`. -/
@[axiom_budget 0]
theorem lower_bound_needs_positivity :
    ¬ (2 ^ ([0, 0] : List Nat).length - 1 ≤ combinationCount [0, 0]) := by decide

/-- The lower bound applies to the instance, and is far below the actual count — the exponential
    bound is a floor that the real circuit counts clear by fifteen orders of magnitude. Both facts
    are checked. -/
@[axiom_budget 1]
theorem instance_lower_bound_is_loose :
    2 ^ contractOps.length - 1 ≤ combinationCount contractOps
    ∧ 2 ^ contractOps.length - 1 < combinationCount contractOps := by
  refine ⟨two_pow_sub_one_le_combinationCount contractOps ?_, ?_⟩
  · intro n hn
    simp only [contractOps, List.mem_cons, List.not_mem_nil, or_false] at hn
    omega
  · norm_num [combinationCount, combinationsIncludingIdle, contractOps]

end Combinatorial.Combinations
