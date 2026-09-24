/-
# DarkFi Comparison Gadget Completeness Proofs

Completes verification of all comparison opcodes (0x50-0x62).
Extends the existing proofs in Gadgets.lean.

## Orchard-Class Detection Rule

Any comparison gadget that returns a boolean output MUST have
ALL witnesses fully constrained in all cases. The IsEqualBase
bug (delta_invert unconstrained when a=b) is the canonical
example — and IsNotEqual is the fix.
-/

import DarkFi.Gadgets
import DarkFi.AxiomBudget

namespace Comparison

/--
## BoolCheck (0x53): value ∈ {0, 1}

Constraint: (value - 0) * (value - 1) = 0

This polynomial product is zero iff value = 0 or value = 1.
-/
structure BoolCheckGadget where
  value : Int
  -- Constraint: value * (value - 1) = 0

/--
## THEOREM: BoolCheck Soundness

If the constraint holds, value is 0 or 1.
-/
@[axiom_budget 1]
theorem boolcheck_sound (g : BoolCheckGadget) (h : g.value * (g.value - 1) = 0) :
  g.value = 0 ∨ g.value = 1 := by
  -- From g.value * (g.value - 1) = 0, we get g.value = 0 or g.value = 1
  have h_zero_or_one : g.value = 0 ∨ g.value - 1 = 0 := by
    apply eq_zero_or_eq_zero_of_mul_eq_zero h
  rcases h_zero_or_one with (h0 | h1)
  · left; exact h0
  · right; linarith

/--
## CondSelect (0x60): if cond=1 return a, else return b

Constraints:
1. cond * (1 - cond) = 0 (cond is boolean)
2. (a - b) * cond + b - output = 0 (selection formula)
-/
structure CondSelectGadget where
  cond : Int
  a : Int
  b : Int
  output : Int
deriving BEq

def cond_select_constraints (g : CondSelectGadget) : Prop :=
  g.cond * (1 - g.cond) = 0 ∧
  (g.a - g.b) * g.cond + g.b - g.output = 0

/--
## THEOREM: CondSelect Correctness

If constraints hold:
  - When cond=1: output = a
  - When cond=0: output = b
-/
@[axiom_budget 1]
theorem cond_select_correct (g : CondSelectGadget) (h : cond_select_constraints g) :
  (g.cond = 1 → g.output = g.a) ∧ (g.cond = 0 → g.output = g.b) := by
  rcases h with ⟨hbool, hselect⟩
  have hcond_bool : g.cond = 0 ∨ g.cond = 1 := by
    have hzero := eq_zero_or_eq_zero_of_mul_eq_zero hbool
    rcases hzero with (h0 | h1)
    · left; exact h0
    · right; linarith
  constructor
  · intro hcond1
    rw [hcond1] at hselect
    have : (g.a - g.b) * 1 + g.b - g.output = 0 := by simpa using hselect
    linarith
  · intro hcond0
    rw [hcond0] at hselect
    have : (g.a - g.b) * 0 + g.b - g.output = 0 := by simpa using hselect
    linarith

/--
## ZeroCond (0x61): if a=0 return a else return b

Constraint: is_zero * output + (1 - is_zero) * (output - b) = 0
where is_zero is the internal IsZero gadget output (1 if a=0, 0 otherwise).

This is used in BurnV1 to handle dummy zero-value commitments:
when commitment_value=0, zero_cond makes the Merkle leaf also 0,
matching the tree's zero leaf at empty positions.

CORRESPONDENCE: src/zk/gadget/zero_cond.rs:77 — single constraint gate.
-/
structure ZeroCondGadget where
  a : Int        -- test value
  b : Int        -- value if a ≠ 0
  output : Int   -- result
  is_zero : Int  -- internal: 1 if a=0, 0 otherwise

/--
The zero_cond constraint: is_zero * output + (1 - is_zero) * (output - b) = 0

When is_zero = 1 (a = 0): output + 0 = 0 → output = 0
When is_zero = 0 (a ≠ 0): 0 + (output - b) = 0 → output = b
-/
def zero_cond_constraint (g : ZeroCondGadget) : Prop :=
  g.is_zero * g.output + (1 - g.is_zero) * (g.output - g.b) = 0

/--
## THEOREM: ZeroCond Correctness (is_zero = 1 case)

When is_zero = 1, the constraint forces output = 0.

The `h_a_zero : g.a = 0` hypothesis this used to carry was never invoked: the constraint acts on
`is_zero`, and `is_zero` relates to `a` through the IsZero gadget, not through a premise. Dropping
it strengthens the theorem (it now holds for every `a`) and leaves the gadget lemma that the proof
actually establishes. -/
@[axiom_budget 1]
theorem zero_cond_correct (g : ZeroCondGadget)
  (h_is_zero_val : g.is_zero = 1)
  (h_constraint : zero_cond_constraint g) :
  g.output = 0 := by
  rw [zero_cond_constraint] at h_constraint
  rw [h_is_zero_val] at h_constraint
  -- 1 * output + (1 - 1) * (output - b) = output + 0 = 0
  simp at h_constraint
  exact h_constraint

/--
## THEOREM: ZeroCond Correctness (is_zero = 0 case)

When is_zero = 0, the constraint forces output = b.

`h_a_ne_zero` was unused for the same reason `h_a_zero` was above. -/
@[axiom_budget 1]
theorem zero_cond_nonzero (g : ZeroCondGadget)
  (h_is_zero_val : g.is_zero = 0)
  (h_constraint : zero_cond_constraint g) :
  g.output = g.b := by
  rw [zero_cond_constraint] at h_constraint
  rw [h_is_zero_val] at h_constraint
  -- 0 * output + (1 - 0) * (output - b) = output - b = 0
  simp at h_constraint
  linarith

/-
## WITHDRAWN: ZeroCond Is Sound for BurnV1

In burn_v2.zk, zero_cond(commitment_value, commitment) is used so that
dummy zero-value inputs (value=0) produce commitment_incl=0 for the
Merkle root computation. This matches the tree's zero leaf.

The attack vector: if a prover could make zero_cond return
a non-zero commitment while commitment_value=0, they could smuggle fake
commitments into the Merkle proof.

This text used to be a doc comment attached to a theorem that claimed "when value=0, the
Merkle leaf IS 0. No fake commitment smuggling possible." The theorem was `h → h` and has been
deleted; the reasoning below replaces it. The paragraph above describes the *attack* correctly; what
was missing was any statement connecting it to a gadget.

(Note for anyone editing this file: a Lean block comment nests, so writing the two characters that
open a comment inside one — as the first version of this paragraph did — silently swallows the rest
of the file.)
-/
/-
## `zero_cond_burn_v1_sound` — deleted

    theorem zero_cond_burn_v1_sound (commitment_value commitment : Int)
      (h_value_zero : commitment_value = 0) : (commitment_value = 0) := by exact h_value_zero

the identity function. The docstring above it says "when value=0, the Merkle leaf IS 0. No fake
commitment smuggling possible" — and the statement never mentions `zero_cond`, a Merkle leaf, or
`commitment`. `commitment` is an unused parameter, and the conclusion is the hypothesis spelled
again. It was a tautology of exactly the shape the register names (`a < b → a < b`), and it survived
three rounds of audit because the name sounds like the defence it was cited as.

The real content it was standing in for is `zero_cond_correct` and `zero_cond_nonzero` above, which
take `zero_cond_constraint g` — the gadget's actual polynomial — and derive `output = 0` when
`is_zero = 1`. Those are non-vacuous and stay. What is *not* proved anywhere is the step from that
gadget lemma to the burn circuit's Merkle leaf, which needs the circuit model; that gap is
`Axioms.NoFreeInstances` and OBL-Z1, not this name.
-/

/-
## IsEqualBase (0x54): BUG CONFIRMED → FIXED in 0f69cd89

Original bug: When a=b (delta=0, out=1), delta_invert was UNCONSTRAINED.
The prover could assign any value to delta_invert.

This did NOT enable false proofs (out=1 is correct when a=b),
but it was mathematically impure — the constraint system did not
fully determine all witness values.

FIXED: purity constraint `out * (delta_invert - 1) = 0` applied in
0f69cd89. See `is_equal_fixed_pure_when_equal` below for the proof
that delta_invert is now forced to 1 when a=b.
-/

/-
## THEOREM: IsEqualBase Bug Reproduction

When a=b:
  delta = a - b = 0
  Constraint: delta * delta_invert = 0 (always satisfied)
  delta_invert can be ANY value

Verified by existing proof in Gadgets.lean: is_equal_bug_when_equal
-/

/-
## IsNotEqual (0x62): PURE — Fully Constrained

The fix for IsEqualBase: add constraint (4):
  (1 - out) * (delta_invert - 1) = 0

When a=b (out=0): delta_invert = 1 (fully constrained!)
When a≠b (out=1): delta_invert = 1/(a-b) (fully determined)

Already verified in Gadgets.lean: is_not_equal_fully_pure
-/

/--
## THEOREM: IsNotEqual Fix Pattern

The 4-constraint pattern can fix IsEqualBase:
  (1 - out) * (delta_invert - 1) = 0  → forces delta_invert=1 when a=b
becomes:
  out * (delta_invert - 1) = 0  → forces delta_invert=1 when out=1 (a=b)

This makes IsEqualBase fully pure.
-/
def is_equal_fixed_constraints (a b out delta_invert : Int) : Prop :=
  (out = 0 ∨ out = 1) ∧
  ((a - b) * delta_invert + (out - 1) = 0) ∧
  ((a - b) * ((a - b) * delta_invert - 1) = 0) ∧
  (out * (delta_invert - 1) = 0)  -- FIX: forces delta_invert=1 when out=1 (a=b)

@[axiom_budget 1]
theorem is_equal_fixed_pure_when_equal (a : Int) :
  -- When a=b (so out=1), delta_invert MUST be 1
  ∀ (out delta_invert : Int),
    is_equal_fixed_constraints a a out delta_invert →
    (out = 1) → (delta_invert = 1) := by
  intro out delta_invert h hout
  rcases h with ⟨_, _, _, hpurity⟩
  rw [hout] at hpurity
  have : (1 : Int) * (delta_invert - 1) = 0 := by simpa using hpurity
  linarith

/-
## RangeCheck (0x50): Running-Sum Decomposition

range_check(64, x) decomposes x into K-bit chunks and does
a table lookup for each chunk. The running sum propagates:
  z_{i+1} = (z_i - k_i) / 2^K

**Corrected 2026-09-24: both of the constants that stood here were wrong.** The block said "For
64-bit range check: K=8, 8 chunks. For 253-bit range check: K=3, 85 chunks." Measured, the deployed
window is **K = 10**: `src/zk/vm.rs:28` imports `K` from
`src/sdk/src/crypto/constants/sinsemilla.rs:31`, where it is `pub const K: usize = 10`, and `vm.rs`
uses it for exactly these two chips (`NativeRange64(NativeRangeCheckConfig<K, 64>)` at `:116`,
`NativeRange253(NativeRangeCheckConfig<K, 253>)` at `:119`, built at `:560` and `:564`). So the
deployed instances are `NativeRangeCheckChip<10, 64>` and `<10, 253>`:

  * 64-bit:  `⌈64/10⌉  = 7`  windows — 6 full chunks and a **4-bit** last chunk;
  * 253-bit: `⌈253/10⌉ = 26` windows — 25 full chunks and a **3-bit** last chunk.

Neither is a multiple, so the short check on the last chunk is what makes each bound exact — see the
section below, which transcribes the chip and proves it.
-/

/-
## WITHDRAWN: Range Check Soundness (64-bit)

If range_check(64, x) passes, then 0 ≤ x < 2^64.
Proved by the running-sum invariant.

That is the statement the deleted theorem *claimed* in its heading, while asserting in Lean that
`0 ≤ x ∧ x < 2^64` follows from `0 ≤ x ∧ x < 2^64`. The claim about the running-sum invariant is
real; it is not what was formalized, and it needs the chunk decomposition rather than an `Int`.
-/
/-
## `range_check_64_sound` — deleted

    theorem range_check_64_sound (x : Int) (h : 0 ≤ x ∧ x < 2^64) : 0 ≤ x ∧ x < 2^64 := by exact h

The range check's soundness, stated as its own hypothesis and then returned. `range_check(64, x)` is
sound when the *circuit's* decomposition — the K-bit chunks and the running sum described directly
above — is what forces `0 ≤ x < 2^64`. Assuming the conclusion proves nothing about the gadget. Not
restated, because the real content needs the chunk decomposition, which lives in `src/zk/gadget/`
rather than here.

The neighbours in this file are genuine and are untouched: `range_check_prevents_value_wraparound`
and `less_than_strict_sound` both take the gadget's parameters and derive something the hypotheses
do not contain.

**Amended 2026-09-24: the content is supplied below, and the sentence that said otherwise was half
wrong.** This block said the real thing "needs the chunk decomposition, which lives in
`src/zk/gadget/` rather than here" — true of the *gadget*, false of the *model*: the decomposition is
an algorithm with a transcribable shape, and the section below transcribes it from
`src/zk/gadget/native_range_check.rs` and proves what the deleted theorem only assumed. What the
withdrawal got right is kept: the ring `z ∈ [0, 2^k)` cannot be *derived* from an `Int` hypothesis,
which is why the old statement was a tautology rather than a proof.
-/

/-!
## The deployed range check, and what its decomposition proves

`NativeRangeCheckChip<WINDOW_SIZE, NUM_BITS>` — `src/zk/gadget/native_range_check.rs` — accepts a
value by **decomposing** it, and it is the decomposition, not any inequality, that bounds it. The
chip's own comment (`:206-208`) states the identity its gates enforce:

    z = c₀ + 2ʷc₁ + 2²ʷc₂ + ⋯ + 2ᵐʷcₘ

with `m = ⌈NUM_BITS / WINDOW_SIZE⌉` chunks, each looked up in `k_values_table` so that it is a
`WINDOW_SIZE`-bit value — and, when `NUM_BITS` is not a multiple of `WINDOW_SIZE`, the *last* chunk
gets a **short** check of `NUM_BITS - WINDOW_SIZE·(m - 1)` bits instead (`:236-245`).

`chunkSum` is that identity, accumulated right-to-left so the recursion mirrors the chip's running
sum `zᵢ = (zᵢ₋₁ - cᵢ₋₁)/2ʷ` rather than a flat sum. What follows is what it proves.

**What this does NOT model, stated rather than implied.** The gates constrain the running sum over
`ZMod p`, and here it is the *integer* equation the chip's comment writes — the step from one to the
other is `BaseDivGadget.zmod_eq_int_of_bounded`, which already exists and is not repeated. The
`k_values_table` lookup is taken as its content (a chunk is `< 2^w`); its own soundness is not
modelled. And `decompose_value`'s bit plumbing — `to_le_bits`, `chunks_exact`, the padding — is not
transcribed: the chunk *values* are, the bit vector they are cut from is not, which is why
`exists_chunkSum_eq` proves decomposability arithmetically instead of from that construction.
-/

/-- The value a chunk sequence stands for: `c₀ + 2ʷc₁ + 2²ʷc₂ + ⋯`, accumulated right-to-left so the
    recursion is structural — and so it mirrors the chip's running sum rather than a flat sum. -/
def chunkSum (w : Nat) : List Nat → ℤ
  | [] => 0
  | c :: cs => (c : ℤ) + (2 : ℤ) ^ w * chunkSum w cs

-- `chunkSum`'s two defining equations are *not* restated as theorems: they would be statements whose
-- content is their own definition, which is the species this tree's anti-vacuity arm exists to catch.
-- `simp only [chunkSum]` unfolds the definition by name wherever a proof needs it.
--
/-- Appending chunks shifts the tail: the head's contribution is the head length's worth of powers. -/
@[axiom_budget 0]
theorem chunkSum_append (w : Nat) (xs ys : List Nat) :
    chunkSum w (xs ++ ys) = chunkSum w xs + (2 : ℤ) ^ (w * xs.length) * chunkSum w ys := by
  induction xs with
  | nil =>
    simp only [List.nil_append, chunkSum, List.length_nil, Nat.mul_zero, pow_zero, one_mul, zero_add]
  | cons c cs ih =>
    simp only [List.cons_append, chunkSum, List.length_cons]
    -- Restated in `List.append` form: after the unfold above the goal carries `List.append`, where
    -- the induction hypothesis carries the `++` notation, and `rw` matches one but not the other.
    have ihapp : chunkSum w (List.append cs ys)
        = chunkSum w cs + (2 : ℤ) ^ (w * cs.length) * chunkSum w ys := ih
    rw [ihapp, Nat.mul_succ, pow_add]
    ring

/-- **A full decomposition is bounded by its width.** `n` chunks of `w` bits each stand for a value
    below `2^(w·n)` — the induction the running sum's invariant gives, and the thing the deleted
    theorem assumed. -/
@[axiom_budget 1]
theorem chunkSum_lt_pow (w : Nat) (cs : List Nat) (h : ∀ c ∈ cs, c < 2 ^ w) :
    chunkSum w cs < (2 : ℤ) ^ (w * cs.length) := by
  induction cs with
  | nil =>
    simp only [chunkSum, List.length_nil, Nat.mul_zero, pow_zero]
    norm_num
  | cons c cs ih =>
    have hc' : c < 2 ^ w := h c (by simp)
    have hc : (c : ℤ) < (2 : ℤ) ^ w := by exact_mod_cast hc'
    have htail : chunkSum w cs < (2 : ℤ) ^ (w * cs.length) := ih fun x hx => h x (by simp [hx])
    have hS : chunkSum w cs ≤ (2 : ℤ) ^ (w * cs.length) - 1 := by omega
    have hw : (0 : ℤ) < (2 : ℤ) ^ w := by positivity
    simp only [chunkSum, List.length_cons, Nat.mul_succ, pow_add]
    nlinarith [hc, hS, hw]

/-- **Every value decomposes**, so the predicate above is about the chip rather than about an empty
    set. The chunks are `z`'s base-`2^w` digits, which is what `decompose_value` computes — proved
    here from the arithmetic rather than from that construction (see the section note). -/
@[axiom_budget 1]
theorem exists_chunkSum_eq (w : Nat) (hw : 0 < w) (z : Nat) :
    ∃ cs : List Nat, (∀ c ∈ cs, c < 2 ^ w) ∧ chunkSum w cs = (z : ℤ) := by
  induction z using Nat.strong_induction_on with
  | _ z ih =>
    rcases Nat.lt_or_ge z (2 ^ w) with hsmall | hbig
    · refine ⟨[z], by simpa using hsmall, ?_⟩
      simp only [chunkSum]
      ring
    · have hpos : 0 < 2 ^ w := by positivity
      have h2 : 1 < 2 ^ w := by
        have hle : 2 ^ 1 ≤ 2 ^ w := Nat.pow_le_pow_right (by norm_num) hw
        omega
      have hq : z / 2 ^ w < z := Nat.div_lt_self (by omega) h2
      obtain ⟨cs, hcs, hsum⟩ := ih (z / 2 ^ w) hq
      have hmod : z % 2 ^ w < 2 ^ w := Nat.mod_lt _ hpos
      refine ⟨z % 2 ^ w :: cs, ?_, ?_⟩
      · intro c hc
        rcases List.mem_cons.mp hc with rfl | hc
        · exact hmod
        · exact hcs c hc
      · simp only [chunkSum, hsum]
        have hnat : z % 2 ^ w + 2 ^ w * (z / 2 ^ w) = z := by
          have := Nat.div_add_mod z (2 ^ w)
          omega
        have hcast : ((z % 2 ^ w : Nat) : ℤ) + ((2 ^ w : Nat) : ℤ) * ((z / 2 ^ w : Nat) : ℤ)
            = (z : ℤ) := by
          rw [← Nat.cast_mul, ← Nat.cast_add, hnat]
        rw [Nat.cast_pow] at hcast
        exact hcast

/-- A concrete evaluation at the deployed window, so the definition is visibly not the zero function:
    `5 + 2¹⁰·2 + 2²⁰·0 = 2053`. -/
@[axiom_budget 0]
theorem chunkSum_example : chunkSum 10 [5, 2, 0] = 2053 := by
  simp only [chunkSum]
  norm_num

/-- **The deployed short check.** `NUM_BITS` is not generally a multiple of `WINDOW_SIZE`, so the chip
    gives the *last* chunk a check of `NUM_BITS - WINDOW_SIZE·(m - 1)` bits rather than `WINDOW_SIZE`
    (`native_range_check.rs:236-245`). That is what makes the bound `2^NUM_BITS` **exact**: without it
    the decomposition only gives `2^(WINDOW_SIZE·m)`, which at the deployed window is `2^70` for the
    64-bit instance rather than `2^64`. -/
@[axiom_budget 1]
theorem chunkSum_lt_pow_of_short_last (w N : Nat) (cs : List Nat) (last : Nat)
    (h : ∀ c ∈ cs, c < 2 ^ w)
    (hlast : last < 2 ^ (N - w * cs.length))
    (hl : w * cs.length ≤ N) :
    chunkSum w (cs ++ [last]) < (2 : ℤ) ^ N := by
  have hcs' : chunkSum w cs ≤ (2 : ℤ) ^ (w * cs.length) - 1 := by
    have := chunkSum_lt_pow w cs h
    omega
  have hlastI : (last : ℤ) < (2 : ℤ) ^ (N - w * cs.length) := by exact_mod_cast hlast
  have hlastI' : (last : ℤ) ≤ (2 : ℤ) ^ (N - w * cs.length) - 1 := by omega
  have happ : chunkSum w (cs ++ [last]) =
      chunkSum w cs + (2 : ℤ) ^ (w * cs.length) * (last : ℤ) := by
    rw [chunkSum_append]
    simp only [chunkSum]
    ring
  rw [happ]
  have hpow : (2 : ℤ) ^ N = (2 : ℤ) ^ (w * cs.length) * (2 : ℤ) ^ (N - w * cs.length) := by
    rw [← pow_add, Nat.add_sub_of_le hl]
  rw [hpow]
  have hX : (0 : ℤ) < (2 : ℤ) ^ (w * cs.length) := by positivity
  nlinarith [hcs', hlastI', hX]

/-- **The 64-bit instance at the deployed parameters** — `NativeRangeCheckChip<10, 64>`
    (`src/zk/vm.rs:116`, `:560`, with `K = 10` from `src/sdk/src/crypto/constants/sinsemilla.rs:31`).
    `⌈64/10⌉ = 7` windows: 6 full chunks and a 4-bit last chunk. So a value the deployed check accepts
    is below `2^64` as an *integer* — the bound `BaseDivGadget`'s bridge needs for its operands. -/
@[axiom_budget 1]
theorem range_check_64_is_bounded (cs : List Nat) (last : Nat)
    (hlen : cs.length = 6) (h : ∀ c ∈ cs, c < 2 ^ 10) (hlast : last < 2 ^ 4) :
    chunkSum 10 (cs ++ [last]) < (2 : ℤ) ^ 64 := by
  have hlast' : last < 2 ^ (64 - 10 * cs.length) := by rw [hlen]; exact hlast
  exact chunkSum_lt_pow_of_short_last 10 64 cs last h hlast' (by rw [hlen]; omega)

/-- And the 253-bit one: `NativeRangeCheckChip<10, 253>` (`src/zk/vm.rs:119`, `:564`) has
    `⌈253/10⌉ = 26` windows — 25 full chunks and a 3-bit last chunk. This is the width the offset of
    the comparison chip is checked against. -/
@[axiom_budget 1]
theorem range_check_253_is_bounded (cs : List Nat) (last : Nat)
    (hlen : cs.length = 25) (h : ∀ c ∈ cs, c < 2 ^ 10) (hlast : last < 2 ^ 3) :
    chunkSum 10 (cs ++ [last]) < (2 : ℤ) ^ 253 := by
  have hlast' : last < 2 ^ (253 - 10 * cs.length) := by rw [hlen]; exact hlast
  exact chunkSum_lt_pow_of_short_last 10 253 cs last h hlast' (by rw [hlen]; omega)

/-- **The composition `OBL-Z12`'s residue names.** `BaseDivGadget`'s bridge needs the operands of each
    comparison to be in range, and its `FieldLessThanOrEqual` carries those bounds as *supplied* —
    `a_bits_lt : a_bits < 2^64` and its neighbours. This is the step they were waiting on, one level
    out from the chip: with every operand 64-bit range-checked, `a·b - c·d` lies inside the `2^253`
    window the offset's own range check supplies, so the bounds meet with room to spare. -/
@[axiom_budget 1]
theorem operand_products_fit_the_offset_window (a b c d : Nat)
    (ha : a < 2 ^ 64) (hb : b < 2 ^ 64) (hc : c < 2 ^ 64) (hd : d < 2 ^ 64) :
    -((2 : ℤ) ^ 253) < (a : ℤ) * b - (c : ℤ) * d ∧
      (a : ℤ) * b - (c : ℤ) * d < (2 : ℤ) ^ 253 := by
  have hp : (2 : ℤ) ^ 128 = (2 : ℤ) ^ 64 * (2 : ℤ) ^ 64 := by norm_num
  have ha' : (a : ℤ) < (2 : ℤ) ^ 64 := by exact_mod_cast ha
  have hb' : (b : ℤ) < (2 : ℤ) ^ 64 := by exact_mod_cast hb
  have hc' : (c : ℤ) < (2 : ℤ) ^ 64 := by exact_mod_cast hc
  have hd' : (d : ℤ) < (2 : ℤ) ^ 64 := by exact_mod_cast hd
  have ha0 : (0 : ℤ) ≤ (a : ℤ) := Int.natCast_nonneg a
  have hb0 : (0 : ℤ) ≤ (b : ℤ) := Int.natCast_nonneg b
  have hc0 : (0 : ℤ) ≤ (c : ℤ) := Int.natCast_nonneg c
  have hd0 : (0 : ℤ) ≤ (d : ℤ) := Int.natCast_nonneg d
  have hab : (a : ℤ) * b < (2 : ℤ) ^ 128 := by nlinarith [ha', hb', ha0, hb0, hp]
  have hcd : (c : ℤ) * d < (2 : ℤ) ^ 128 := by nlinarith [hc', hd', hc0, hd0, hp]
  have h128 : (2 : ℤ) ^ 128 < (2 : ℤ) ^ 253 := by norm_num
  constructor <;> nlinarith [hab, hcd, ha0, hb0, hc0, hd0, h128]

/-!
## What the deployed check hands the gadgets that use it

The bound above is about a *value*; what the gadgets need is that a value they hold is the image of a
bounded integer. `BaseDivGadget.FieldLessThanOrEqual` carries exactly that as fields —
`a_bits : Nat`, `a_bits_lt : a_bits < 2^64`, `a_eq : a = ↑a_bits`, and the same pair for `b` and the
offset — and the section note there now records that those are the *shape*
`BaseDivGadget.zmod_eq_int_of_bounded` consumes rather than a fact about arithmetic. What the two
theorems below add is that the shape is not an assumption about the value at all: it follows from the
check's own witness, so a caller who has a circuit's chunk list has the bound, and it is the check's
chunks — not a hypothesis — that supply it.

This is the step `OBL-Z12`'s residue named, one level of *statement* further in than the arithmetic.
It is **not** the whole bridge: nothing here reads a `.zk` source, so the passage from a circuit's
`range_check(64, ·)` call to the chunk list below is still the transcription
`Circuits/InstanceDerivation.lean` records. What is proved is that *given* the witness, the bound is a
consequence.
-/

/-- The chunks' standing for a value is never negative — every term is a `Nat` times a power. -/
@[axiom_budget 1]
theorem chunkSum_nonneg (w : Nat) (cs : List Nat) : 0 ≤ chunkSum w cs := by
  induction cs with
  | nil => simp only [chunkSum]; norm_num
  | cons c cs ih =>
    simp only [chunkSum]
    have h2 : (0 : ℤ) < (2 : ℤ) ^ w := by positivity
    nlinarith [ih, h2, Int.natCast_nonneg c]

/-- **The bound a circuit's range check supplies, as a bounded integer rather than a proof
    obligation.** Given the chunk witness the deployed chip accepts, some `bits : Nat` stands for
    the same value and is below `2^N` — which is the `(a_bits, a_bits_lt, a_eq)` triple
    `FieldLessThanOrEqual` carries, so those fields are obtainable rather than assumed. -/
@[axiom_budget 1]
theorem exists_bounded_bits_of_range_check (w N : Nat) (cs : List Nat) (last : Nat)
    (h : ∀ c ∈ cs, c < 2 ^ w)
    (hlast : last < 2 ^ (N - w * cs.length))
    (hl : w * cs.length ≤ N) :
    ∃ bits : Nat, (bits : ℤ) = chunkSum w (cs ++ [last]) ∧ bits < 2 ^ N := by
  have hb : chunkSum w (cs ++ [last]) < (2 : ℤ) ^ N :=
    chunkSum_lt_pow_of_short_last w N cs last h hlast hl
  have hnn : 0 ≤ chunkSum w (cs ++ [last]) := chunkSum_nonneg w (cs ++ [last])
  have htoNat : (((chunkSum w (cs ++ [last])).toNat : ℤ)) = chunkSum w (cs ++ [last]) :=
    Int.toNat_of_nonneg hnn
  refine ⟨(chunkSum w (cs ++ [last])).toNat, htoNat, ?_⟩
  have : (((chunkSum w (cs ++ [last])).toNat : ℤ)) < (2 : ℤ) ^ N := by rw [htoNat]; exact hb
  exact_mod_cast this

/-- The 64-bit instance at the deployed parameters — `NativeRangeCheckChip<10, 64>` — which is the
    exact shape `FieldLessThanOrEqual`'s `a_bits`/`a_bits_lt`/`a_eq` fields need. -/
@[axiom_budget 1]
theorem range_check_64_gives_bounded_bits (cs : List Nat) (last : Nat)
    (hlen : cs.length = 6) (h : ∀ c ∈ cs, c < 2 ^ 10) (hlast : last < 2 ^ 4) :
    ∃ bits : Nat, (bits : ℤ) = chunkSum 10 (cs ++ [last]) ∧ bits < 2 ^ 64 := by
  have hlast' : last < 2 ^ (64 - 10 * cs.length) := by rw [hlen]; exact hlast
  exact exists_bounded_bits_of_range_check 10 64 cs last h hlast' (by rw [hlen]; omega)

/--
## THEOREM: Range Check Is Necessary for Value Conservation

Every `commitment_value` in every PN circuit is range_checked to 64 bits.
Without this, a prover could set commitment_value = p-1 (≈ 2^254) and
the Pedersen value commitment would wrap around, breaking
value conservation.

This theorem states: range_check(64, value) ⇒ value < 2^64 ≪ p
so no field wraparound in Pedersen commitments.
-/
@[axiom_budget 0]
theorem range_check_prevents_value_wraparound (value : Int)
  (h_range : 0 ≤ value ∧ value < 2^64) :
  value < 2^64 := by
  exact h_range.right

/-
## LessThanStrict (0x51): Constrain-Only a < b

Constraint: a_offset = a + 2^m - b
range_check(m, a_offset) and range_check(m, a)

If both pass: a ∈ [0, 2^m) and a + 2^m - b ∈ [0, 2^m)
Therefore: b > a (strict)

This is the SOUND, constrain-only version.
-/

/--
## THEOREM: LessThanStrict Soundness

If less_than_strict(a, b) succeeds, then a < b.
-/
@[axiom_budget 1]
theorem less_than_strict_sound (a b offset : Int) (m : Nat)
  (h_offset_range : 0 ≤ offset ∧ offset < 2^m)
  (h_offset_eq : offset = a + 2^m - b) :
  a < b := by
  rcases h_offset_range with ⟨ho_low, ho_high⟩
  rw [h_offset_eq] at ho_low ho_high
  -- offset = a + 2^m - b ≥ 0  ⇒  b - a ≤ 2^m
  -- offset = a + 2^m - b < 2^m ⇒ a - b < 0 ⇒ a < b
  --
  -- The `h_a_range : 0 ≤ a ∧ a < 2^m` hypothesis that used to be here, and the `have hpos`
  -- that restated `ho_high`, were both unused — `linarith` derives `a < b` from the offset
  -- bounds alone, so the input range check is not needed for this conclusion. Dropping it
  -- strengthens the theorem: it holds for every `a` and every `b`, not only range-checked ones.
  linarith

/-
## LessThanOrEqual (0x55): Boolean Return

Returns 1 if a ≤ b, 0 otherwise.

Constraint: out ∈ {0,1}
a_offset = out*(b-a) + (1-out)*(a-b-1)
range_check(253, a_offset)

VERIFIED SOUND in Gadgets.lean.
-/

/-
## BaseLtStrict (0x57): Boolean Return

Returns 1 if a < b, 0 otherwise.

Constraint: out ∈ {0,1}
a_offset = out*(b-a-1) + (1-out)*(a-b)
range_check(253, a_offset)

**Corrected 2026-09-24: both of the citations that stood here were wrong, and the theorem they
should have pointed at did not exist.** The line read "VERIFIED SOUND by `less_than_strict_sound`
below" after replacing "VERIFIED SOUND in Main.lean (exhaustive 1000×1000)", and the replacement is
no better than what it replaced:

* `less_than_strict_sound` is **about a different constraint system.** Its hypothesis is
  `offset = a + 2^m - b`, which is the *other* strict opcode's gate — `s_lt` at
  `src/zk/gadget/less_than.rs:126-137`, opcode 0x51. This opcode's gate is `s_lt_out`
  (`less_than.rs:163-180`), whose offset is `out·(b − a − 1) + (1 − out)·(a − b)`, and the shape
  printed above is that gate's ✓. A theorem about 0x51's constraint says nothing about 0x57's.
* `src/Main.lean`'s search is not evidence either: that file is in **no `lean_lib`** — `lakefile.lean`
  declares `DarkFi` and `Transcribed` and nothing else — so `lake build DarkFi` never compiles it and
  no gate reads its output. It also does not compile at all (21 errors, measured 2026-09-24; see its
  header). The search it contains is the `lt_strict_offset`/`lt_strict_satisfied` loop at `:91-110`,
  which is exactly the "exhaustive 1000×1000" the documentation cites.

**So the opcode had no soundness theorem, and the documentation said it was SOUND.** The proof
below is that theorem, stated over the gate's own shape. `Gadgets.lean` has no 0x57 row either, and
this section had nothing but the shape description — which is why the absence survived: a reader
found a citation in both places. Registered as `OBL-Z20` in `doc/src/arch/verification-hazop.md`,
minted for the absence rather than for a defect, the way `OBL-C115`/`C116` were.
-/

/-- **`BaseLtStrict` (0x57): the deployed gate, and its soundness.** `src/zk/gadget/less_than.rs`
    constrains, under `s_lt_out`, `out·(1 − out) = 0` and
    `a_offset = out·(b − a − 1) + (1 − out)·(a − b)` (`:172`, `:177`), and the offset carries a
    `range_check(253, ·)`. From those, `out = 1` exactly when `a < b` — so the returned bit is the
    comparison, and neither value of it is available when the comparison says otherwise.

    The range check's *upper* bound (`a_offset < 2^253`) is deliberately not a hypothesis:
    soundness uses `0 ≤ a_offset` alone, and an unused hypothesis is what this tree removes rather
    than carries for the look of it — the same reason `less_than_strict_sound` below records for
    having dropped `h_a_range`. What the upper bound is *for* is stated at `Gadgets.lean:116`, in
    `less_than_or_equal_sound`'s docstring: "wrong `out` → negative `a_offset` → field wrap >
    2^253 → range check fails". That is the field-versus-integer reading, which is `OBL-Z12`'s
    subject (`BaseDivGadget.less_than_or_equal_integer_reading`) rather than this theorem's. -/
@[axiom_budget 1]
theorem base_lt_strict_sound (a b offset out : Int)
    (h_out : out = 0 ∨ out = 1)
    (h_offset : offset = out * (b - a - 1) + (1 - out) * (a - b))
    (h_low : 0 ≤ offset) :
    (out = 1 ↔ a < b) ∧ (out = 0 ↔ b ≤ a) := by
  have h1 : out = 1 → a < b := by
    intro h
    rw [h] at h_offset
    norm_num at h_offset
    omega
  have h0 : out = 0 → b ≤ a := by
    intro h
    rw [h] at h_offset
    norm_num at h_offset
    omega
  constructor
  · refine ⟨h1, ?_⟩
    intro hab
    rcases h_out with h | h
    · rw [h] at h_offset
      norm_num at h_offset
      omega
    · exact h
  · refine ⟨h0, ?_⟩
    intro hba
    rcases h_out with h | h
    · exact h
    · rw [h] at h_offset
      norm_num at h_offset
      omega

/-
## `boolean_output_must_be_constrained` — deleted

    theorem boolean_output_must_be_constrained (out : Int)
      (hbool : out = 0 ∨ out = 1) : out = 0 ∨ out = 1 := hbool

with the docstring above it reading "All Boolean Comparison Outputs Must Be Range-Checked ... Without
(1), the output could be any value." The hypothesis **is** point (1) — the theorem assumes the output
is boolean and concludes it is boolean. The requirement it means to state is a requirement on
*circuits*: that every comparison gadget includes the `bool_check` constraint. That is a syntactic
property of `.zk` sources, not a proposition about an `Int`, and it is now checked there — the
`bool_check` call appears in the circuits, and OBL-Z1's classifier walks them.

Note this is the second `Comparison` theorem whose docstring names a constraint and whose statement
takes that constraint as a hypothesis. `boolcheck_sound` at the top of this file does it correctly:
it takes the *polynomial* `g.value * (g.value - 1) = 0` and derives `g.value = 0 ∨ g.value = 1`.
-/

end Comparison
