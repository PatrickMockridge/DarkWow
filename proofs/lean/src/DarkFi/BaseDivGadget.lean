import Mathlib
import DarkFi.Axioms
import DarkFi.AxiomBudget

/-!
# The division gadget, modelled — and the bound it needs

`base_div` (opcode 0x58) is load-bearing for proving governance while keeping amounts private: the
liquidity and PN-backing ratios behind the PN-issuing contracts (dex, stablecoin, bearer_bond) are
all quotients of two amounts the issuer does not want to publish. This file is the *formal* half of
the campaign that made those circuits sound; `doc/src/arch/verification-hazop.md` carries the other
half and the six circuits it changed.

Two things are modelled here, and they answer different questions.

**Part 1 — what the opcode computes.** `vm.rs:1555-1643` computes `a · b^(p−2) mod p` by
exponentiation by squaring: `p − 2` is 255 bits, 77 of them set, and the loop (`:1608-1638`)
initializes `result = b` — bit 0's contribution — then for `i` in `1..=254` squares a running power
and multiplies it into the accumulator when bit `i` is set, and finally multiplies by `a`: 254
squarings + **76** conditional multiplications + **1** final multiplication = **331** multiplication
gates. The `sqMulGo` definition below is that loop **in value**, transcribed in the shape
`Emission.fixedPowDecayGo` uses, and `sqMul_is_inverse` is the content: for `b ≠ 0` the result times
`b` is `a`, i.e. it really is division.

**One structural difference between the model and the deployed loop, and the count above is the
deployed one's.** `sqMulGo` recurses while its exponent is nonzero, so at `p − 2` it iterates 255
times and squares 255 times, where the deployed loop squares 254 — it consumes bit 0 in the
initialization rather than in an iteration. The *value* is the same and `sqMulGo_eq` is about the
value; but a reader who counted gates from the model would get 332 + the final multiply = 333, not
331. Recorded because it is exactly how a wrong count arises: `Arithmetic.lean`'s cost line and two
documents had "one multiplication per set bit (77)" — corrected 2026-09-24 — and this shape is where
that reading comes from.

There is a reason to prefer the field statement over an `Int`-with-`%` one, beyond brevity: the
`Int` form needs `b % p ≠ 0` as a hypothesis and a bridge lemmas to move between `%` and `ZMod`. The
field form needs the hypothesis too, but it says what the opcode is *for* rather than restating the
same congruence in two languages. And `b = 0` needs no case at all — see `sqMul_zero`.

**Part 2 — the bound the quotient-remainder pattern needs, and what happens without it.** The five
circuits that replaced `base_div` in the RC4 pass (and the ones changed in this campaign) express
integer division as

    q · d ≤ n   and   n < (q + 1) · d

over field elements. `qr_unique` says that pair pins `q = n / d` — *given* the products do not wrap
the field. Whether they wrap is not a property of this pair of inequalities: the comparison chip is
253 bits wide and range-checks its **offset**, the difference of its operands, never the operands
themselves. So each operand must be bounded by a `range_check`, and `qr_needs_bound` is the
counterexample that makes that a requirement rather than a convention — with the real field, the
real chip width, and the real values one of the affected circuits used, a quotient ~10⁷⁶ away from
the true one satisfies both comparisons. The witness is kernel-checked.

## Where this file's statements stop

`qr_unique` is a theorem about the *inequalities*, not about the chip. It assumes the two products
are the integers they look like; establishing that on chain is `range_check`'s job plus the
arithmetic of the specific circuit (every operand `< 2^64` makes every product `< 2^129 < 2^253`).

**The chip half of that is now proved** — Part 0, `less_than_or_equal_integer_reading`: from the
chip's constraints *over the field* together with its range checks, the comparison is the integer
comparison. This note said that proof was "not here and is not claimed"; it is here, and the residual
is one step further out and belongs to the *circuit* rather than to the chip — that the operands fed
to each comparison are in the range the bridge needs. The `2^64` bound makes every product `< 2^129`,
so a difference of two products lies in `(-2^129, 2^129)`, well inside the `2^253` the chip's range
check supplies; but that composition is the quotient-remainder circuits' own argument and is not made
here. So the residue of `OBL-Z12` is that composition, not the chip's reading.

**Corrected 2026-09-24: the composition's two halves are now both theorems, and what remains is a
transcription rather than arithmetic.** The sentence above said the composition "is not made here" —
true of *this file* and false of the layer: the arithmetic is
`Comparison.operand_products_fit_the_offset_window` (64-bit-bounded operands have `a·b - c·d` inside
the `2^253` window, as this note's own paragraph argues) and the bounds themselves are
`Comparison.range_check_64_is_bounded`, which derives `< 2^64` from the **deployed** decomposition —
`NativeRangeCheckChip<10, 64>` at `src/zk/vm.rs:116` — rather than assuming it, which is what the
`a_bits_lt` / `b_bits_lt` fields of `FieldLessThanOrEqual` above carry. **And those fields are obtainable rather than assumed**:
`Comparison.range_check_64_gives_bounded_bits` takes the chunk witness the deployed chip accepts and
returns exactly the `(bits, bits < 2^64, a = ↑bits)` triple this structure's `a_bits`/`a_bits_lt`/
`a_eq` are — so a caller holding a circuit's chunks has the bound, and it is the check's own
decomposition that supplies it. What is *not* proved, and is the honest residue, is the step from a
circuit's `range_check(64, ·)` call to a chunk list of that shape: that is the same
`(r, s) ↦ the circuit's statements` transcription `Circuits/InstanceDerivation.lean` records for
`OBL-T7`, and it is not arithmetic. So `OBL-Z12`'s residue is no longer "the composition is unmade"
but "the composition's inputs come from the check's witness rather than from the circuit source" —
which is a strictly smaller gap, and a different one.
-/

namespace BaseDivGadget

/- ==========================================================================
   Part 0: the bridge the header says was missing — the chip's integer reading

   The header above ends: "A proof that the chip's 253-bit range check implies the integer reading of
   a comparison is not here and is not claimed." It is here now, and this is what it rests on.

   The chip's constraints are imposed over `ZMod PALLAS_MODULUS`, where `2^254 - 1 < 0` is "true" and
   so is `a - b - 1 ≥ 0` for the wrong ordering. The range check is what removes that freedom, and the
   fact is one arithmetic lemma: a field element that is *also* the image of a bounded integer, with
   the bound below the modulus and room to spare, cannot be the image of a second bounded integer.
   `zmod_eq_int_of_bounded` is that lemma, and it is where the number 253 does its work — the bound is
   `2^253 + 2^64 < 2^254 ≤ p`, so an out-of-range difference wraps and is caught.

   `less_than_or_equal_integer_reading` is the chip-level statement, in the shape
   `Gadgets.output_correct` uses: the two cases `out = 1` and `out = 0`, concluding the *integer*
   order rather than the field's.

   Two things about the hypotheses, both deliberate and both the register's `OBL-Z12`:

   * the operand bounds (`a < 2^64`, `b < 2^64`) are **hypotheses**, because they are not properties
     of the chip — they are what the surrounding circuit's `range_check(64)` calls supply, and
     `qr_needs_bound` is the kernel-checked counterexample showing that without them both comparisons
     are satisfied by a quotient ~10^76 away from the true one.
   * the range check is stated as *being the image of a 253-bit integer* rather than as an inequality
     over the field, which is what a range check **is** on chain: it exhibits a bit decomposition. An
     inequality over `ZMod p` would be a weaker hypothesis and a false statement, since `p - 1` is
     "less than 2^253" in no useful sense.
   ========================================================================== -/

/-- **A field element that is the image of a bounded integer is the image of only one.** If `x` lies in
    `[-2^64, 2^64]` and `k` in `[0, 2^253]`, and `x` and `k` are equal in `ZMod PALLAS_MODULUS`, then
    `x = k` in `ℤ`.

    This is the whole content of the 253-bit range check's integer reading. The hypothesis is
    `-2^64 ≤ x` rather than `-2^64 < x` because the lower end is attained: the `out = 0` case of the
    chip hands over `a - b - 1`, which is exactly `-2^64` at `a = 0`, `b = 2^64 - 1`.

    The proof is the bound: `k - x` is a multiple of `p`, and it lies in `(-2^64, 2^253 + 2^64)`, which
    is inside `(-p, p)` because `2^253 + 2^64 < 2^254 ≤ p`. The only multiple of `p` in that interval is
    zero. The modulus is written out as a numeral for that step, so the comparison is arithmetic rather
    than a property of the modulus — no primality is used, and `pallasPrime` is not reached. -/
@[axiom_budget 1]
theorem zmod_eq_int_of_bounded (x k : ℤ) (hx : -(2:ℤ)^64 ≤ x) (hx' : x < (2:ℤ)^64)
    (hk : 0 ≤ k) (hk' : k < (2:ℤ)^253)
    (h : (x : ZMod PALLAS_MODULUS) = (k : ZMod PALLAS_MODULUS)) : x = k := by
  have hp : (PALLAS_MODULUS : ℤ) =
      28948022309329048855892746252171976963363056481941560715954676764349967630337 := by
    rw [PALLAS_MODULUS]; norm_num
  have hdvd : (PALLAS_MODULUS : ℤ) ∣ k - x :=
    Int.modEq_iff_dvd.mp ((ZMod.intCast_eq_intCast_iff x k PALLAS_MODULUS).mp h)
  obtain ⟨c, hc⟩ := hdvd
  rw [hp] at hc
  omega

/-- The chip's constraints as a prover supplies them: over `ZMod PALLAS_MODULUS`, with each range check
    recorded as the integer it decomposes to.

    `Gadgets.LessThanOrEqualGadget.gadget_satisfied` states the equivalent inequalities over `ℤ`, which
    is the chip's *intent*; this structure is the same circuit seen from the field, and the theorem
    below is the bridge between them. The gate and the offset relation are the same two constraints —
    `out ∈ {0,1}` and `out·(b − a) + (1 − out)·(a − b − 1) = offset` — written over the field, which is
    where the prover puts them. -/
structure FieldLessThanOrEqual where
  /-- The chip's output: `1` for `a ≤ b`, `0` for `a > b`. -/
  out : ZMod PALLAS_MODULUS
  /-- The two operands, as the circuit sees them. -/
  a : ZMod PALLAS_MODULUS
  b : ZMod PALLAS_MODULUS
  /-- The range-checked offset. -/
  offset : ZMod PALLAS_MODULUS
  /-- The boolean gate. -/
  out_zero_or_one : out = 0 ∨ out = 1
  /-- The offset relation, over the field. -/
  offset_relation : out * (b - a) + (1 - out) * (a - b - 1) = offset
  /-- `range_check(253, offset)`: the integer the offset decomposes to. -/
  offset_bits : Nat
  offset_bits_lt : offset_bits < 2 ^ 253
  offset_eq : offset = (offset_bits : ZMod PALLAS_MODULUS)
  /-- `range_check(64, a)`: the integer `a` decomposes to. -/
  a_bits : Nat
  a_bits_lt : a_bits < 2 ^ 64
  a_eq : a = (a_bits : ZMod PALLAS_MODULUS)
  /-- `range_check(64, b)`. -/
  b_bits : Nat
  b_bits_lt : b_bits < 2 ^ 64
  b_eq : b = (b_bits : ZMod PALLAS_MODULUS)

/-- **The chip's constraints, read as integers.** This is the statement the header said was missing:
    from the circuit's field-level constraints *plus the range checks*, `out = 1` implies `a ≤ b` in
    `ℕ` and `out = 0` implies `b < a` — the integer reading, not the field's.

    Same shape as `Gadgets.less_than_or_equal_sound` (`output_correct`'s two clauses), one level down:
    that theorem takes the range check as an inequality over `ℤ`, this one as a bit decomposition over
    `ZMod p`, which is what the circuit actually has. -/
@[axiom_budget 1]
theorem less_than_or_equal_integer_reading (g : FieldLessThanOrEqual) :
    (g.out = 1 → g.a_bits ≤ g.b_bits) ∧ (g.out = 0 → g.b_bits < g.a_bits) := by
  have ha := g.a_bits_lt
  have hb := g.b_bits_lt
  have ho := g.offset_bits_lt
  constructor
  · intro hout
    -- `out = 1` collapses the offset relation to `b - a = offset`.
    have hrel : g.b - g.a = g.offset := by
      have := g.offset_relation
      rw [hout] at this
      simpa using this
    have hcast : (g.b_bits : ZMod PALLAS_MODULUS) - (g.a_bits : ZMod PALLAS_MODULUS)
        = (g.offset_bits : ZMod PALLAS_MODULUS) := by
      rw [← g.b_eq, ← g.a_eq, ← g.offset_eq, hrel]
    have hcast' : (((g.b_bits : ℤ) - g.a_bits : ℤ) : ZMod PALLAS_MODULUS)
        = (g.offset_bits : ZMod PALLAS_MODULUS) := by
      push_cast
      exact hcast
    have hint : (g.b_bits : ℤ) - g.a_bits = g.offset_bits :=
      zmod_eq_int_of_bounded _ _ (by omega) (by omega) (by omega) (by omega) hcast'
    omega
  · intro hout
    -- `out = 0` collapses it to `a - b - 1 = offset`, where the `- 1` is what makes the strict case
    -- land strictly: `offset ≥ 0` gives `a ≥ b + 1`.
    have hrel : g.a - g.b - 1 = g.offset := by
      have := g.offset_relation
      rw [hout] at this
      simpa using this
    have hcast : (g.a_bits : ZMod PALLAS_MODULUS) - (g.b_bits : ZMod PALLAS_MODULUS) - 1
        = (g.offset_bits : ZMod PALLAS_MODULUS) := by
      rw [← g.a_eq, ← g.b_eq, ← g.offset_eq, hrel]
    have hcast' : (((g.a_bits : ℤ) - g.b_bits - 1 : ℤ) : ZMod PALLAS_MODULUS)
        = (g.offset_bits : ZMod PALLAS_MODULUS) := by
      push_cast
      exact hcast
    have hint : (g.a_bits : ℤ) - g.b_bits - 1 = g.offset_bits :=
      zmod_eq_int_of_bounded _ _ (by omega) (by omega) (by omega) (by omega) hcast'
    omega

/- ==========================================================================
   Part 1: what `base_div` computes
   ========================================================================== -/

/-- The exponentiation-by-squaring loop of `vm.rs:1570-1640`, transcribed.

    `sqMulGo exp acc sq`: `sq` is the running power, `acc` the accumulator. Each iteration halves
    `exp`, squares `sq`, and multiplies `acc` by `sq` when the low bit of `exp` was set. The
    recursion on `exp / 2` mirrors `Emission.fixedPowDecayGo` — and unlike that function there is
    nothing truncated here, so the odd/even case that obstructs that file's induction closes
    cleanly in a ring. -/
def sqMulGo : Nat → ZMod PALLAS_MODULUS → ZMod PALLAS_MODULUS → ZMod PALLAS_MODULUS
  | 0, acc, _ => acc
  | exp + 1, acc, sq =>
      sqMulGo ((exp + 1) / 2)
        (if (exp + 1) % 2 = 1 then acc * sq else acc)
        (sq * sq)

/-- `b^(p−2)`, computed the way the opcode computes it. -/
def powMod (exp : Nat) (b : ZMod PALLAS_MODULUS) : ZMod PALLAS_MODULUS := sqMulGo exp 1 b

/-- The opcode's output, as a function of its two heap operands: `a · b^(p−2)`. The `a` is the
    single final multiplication the gate count in `vm.rs:1555-1643` accounts for. -/
def sqMul (a b : ZMod PALLAS_MODULUS) : ZMod PALLAS_MODULUS := a * powMod (PALLAS_MODULUS - 2) b

/-- `(sq · sq)^k = sq^(2k)` — the squaring step, as an exponent identity. -/
private theorem sq_mul_sq_pow (sq : ZMod PALLAS_MODULUS) (k : Nat) :
    (sq * sq) ^ k = sq ^ (2 * k) := by
  rw [mul_pow, ← pow_add]
  congr 1
  omega

/-- **The loop computes the power.** `sqMulGo exp acc sq = acc · sq^exp`.

    The *mathematical* content is group algebra in an arbitrary commutative ring: it holds for
    every `n`, needs no primality and no field. That is worth stating, because it is the part of
    the gadget that cannot be wrong in the way the surrounding code was wrong — the RC4 bug in
    `base_div` was a wrong exponent constant (`p − 2` written as something else), not a wrong loop.

    **And the budget is 1 now, which this docstring said was impossible.** It read: "The measured
    budget is 2, not 0 … `pallasPrime` … is not reached by the *proof* — it is reached by the
    *type*: every statement here is about `ZMod PALLAS_MODULUS`, and elaborating that type picks up
    the global `instance : Fact (Nat.Prime PALLAS_MODULUS)`. So 'no assumption beyond Lean's logic'
    is not available for any theorem stated over this field, however elementary the arithmetic."

    The diagnosis was exactly right and the conclusion drawn from it was wrong. The instance is not
    inherent to the type — it was **global**, so it was in scope in every importing file, and
    resolution preferred the `Field` path over the unconditional `ZMod.commRing`. That is why a
    statement needing only a ring was charged a primality proof. `Axioms.lean` declares no such
    instance now, and the one declaration in this file that genuinely needs a field takes it locally.
    Nothing about the arithmetic moved.

    **The measured budget is 1, not the 0 this docstring first claimed after the repair.** Writing "0"
    was a prediction, and the gate refuted it in the same run: `pallasPrime` is gone from the axiom
    set, but the automation (`simp`/`omega` over `Prop` equality) still reaches `Classical.choice`,
    which is charged. So the repair removed the *primality* dependency and left the classical one —
    which is the honest split, and it is measurable rather than arguable. -/
@[axiom_budget 1]
theorem sqMulGo_eq (exp : Nat) :
    ∀ (acc sq : ZMod PALLAS_MODULUS), sqMulGo exp acc sq = acc * sq ^ exp := by
  -- Strong induction, not structural: the recursive call is on `(exp+1)/2`, which is below
  -- `exp+1` but is not its predecessor. Same shape as `Emission.fixedPowDecayGo_le_start`.
  induction exp using Nat.strong_induction_on with
  | _ exp ih =>
      intro acc sq
      cases exp with
      | zero => simp [sqMulGo]
      | succ n =>
          have hlt : (n + 1) / 2 < n + 1 := Nat.div_lt_self (Nat.succ_pos n) (by norm_num)
          -- `2 * ((n+1)/2) + (n+1) % 2 = n+1`: the halving the loop performs, inverted.
          have hsplit : 2 * ((n + 1) / 2) + (n + 1) % 2 = n + 1 := by omega
          simp only [sqMulGo]
          rw [ih _ hlt]
          by_cases hbit : (n + 1) % 2 = 1
          · -- The low bit was set: the accumulator took the extra factor `sq`.
            have hk : 2 * ((n + 1) / 2) + 1 = n + 1 := by
              rw [hbit] at hsplit; exact hsplit
            simp only [hbit, if_true]
            rw [sq_mul_sq_pow, mul_assoc, ← pow_succ', hk]
          · -- The low bit was not set. `(n+1) % 2 < 2` and it is not `1`, so it is `0`, and the
            -- exponent the loop reached is already the one the statement asks for.
            have h0 : (n + 1) % 2 = 0 := by
              rcases Nat.mod_two_eq_zero_or_one (n + 1) with h | h
              · exact h
              · exact absurd h hbit
            have hk : 2 * ((n + 1) / 2) = n + 1 := by
              rw [h0, add_zero] at hsplit; exact hsplit
            simp only [hbit, if_false]
            rw [sq_mul_sq_pow, hk]

/-- **`base_div`'s output is `a · b^(p−2)`.** The loop's power, instantiated at the opcode's
    exponent. Budget 1 by the same scope change as `sqMulGo_eq`: the statement needs a ring, and
    `ZMod` is one unconditionally, so `pallasPrime` is gone; `Classical.choice` from the automation
    is what remains. -/
@[axiom_budget 1]
theorem sqMul_eq (a b : ZMod PALLAS_MODULUS) :
    sqMul a b = a * b ^ (PALLAS_MODULUS - 2) := by
  simp [sqMul, powMod, sqMulGo_eq]

/-- **For `b ≠ 0` the output really is `a / b`.** This is the statement the opcode is *for*, and
    the one an auditor should read: `(a · b^(p−2)) · b = a`.

    Was an axiom in an earlier draft, as `base_div_mul_cancel` in `Int` clothing; the `Int` version
    is `Arithmetic.base_div_mul_cancel` in `BaseDiv.lean`, discharged for the same reason this
    holds — Fermat's little theorem over `ZMod`, resting on `pallasPrime`. Budget 2: `pallasPrime`
    plus the classical choice the `ZMod` field instance reaches. -/
@[axiom_budget 2]
theorem sqMul_is_inverse (a b : ZMod PALLAS_MODULUS) (hb : b ≠ 0) : sqMul a b * b = a := by
  rw [sqMul_eq]
  -- **The one declaration in this file that needs the field.** `ZMod.pow_card_sub_one_eq_one` takes
  -- `[Fact PALLAS_MODULUS.Prime]` as an *instance* argument, so the proof needs one in its local
  -- context — supplied here rather than by a global instance, which is what used to charge every
  -- other declaration in this file for it.
  haveI : Fact (Nat.Prime PALLAS_MODULUS) := ⟨pallasPrime⟩
  have hfermat : b ^ (PALLAS_MODULUS - 1) = 1 := ZMod.pow_card_sub_one_eq_one hb
  have hPge : 2 ≤ PALLAS_MODULUS := by rw [PALLAS_MODULUS]; norm_num
  have hlt : PALLAS_MODULUS - 1 = (PALLAS_MODULUS - 2) + 1 := by omega
  rw [hlt, pow_succ] at hfermat
  calc a * b ^ (PALLAS_MODULUS - 2) * b
      = a * (b ^ (PALLAS_MODULUS - 2) * b) := by ring
    _ = a * 1 := by rw [hfermat]
    _ = a := by ring

/-- **`b = 0` needs no special case.** The implementation returns `0` for a zero divisor with an
    explicit early branch (`vm.rs:1555-1643`), and it is worth recording *why that branch is not a
    convention*: `b^(p−2)` with `b = 0` is `0`, because `p − 2 ≠ 0`. So the branch restates what the
    formula already gives, and no prover freedom hides in it — the "what does division by zero
    mean in a field" question does not arise. Budget 1, `sqMulGo_eq`'s reason as it now reads:
    `pallasPrime` gone with the global instance, `Classical.choice` left from the automation. -/
@[axiom_budget 1]
theorem sqMul_zero (a : ZMod PALLAS_MODULUS) : sqMul a 0 = 0 := by
  rw [sqMul_eq]
  have hne : PALLAS_MODULUS - 2 ≠ 0 := by rw [PALLAS_MODULUS]; norm_num
  rw [zero_pow hne, mul_zero]

/- ==========================================================================
   Part 2: the quotient-remainder pattern
   ========================================================================== -/

/-- The three numbers the pattern relates: numerator `n`, denominator `d`, claimed quotient `q`.
    Integer division, not field division — which is the whole point, since `a · b^(p−2)` is a
    number near `p` and the circuits need a small quotient. -/
structure QuotientRemainder where
  n : Nat
  d : Nat
  q : Nat

/-- The two inequalities the circuits constrain, as a proposition about integers.

    `less_than_or_equal(q*d, n)` is the first, `less_than_strict(n, (q+1)*d)` the second. -/
def qr_constraints (g : QuotientRemainder) : Prop :=
  0 < g.d ∧ g.q * g.d ≤ g.n ∧ g.n < (g.q + 1) * g.d

/-- **The pair pins the quotient.** Under `qr_constraints`, `q` is `n / d` — there is exactly one
    value satisfying both inequalities, so exposing it exposes a *determined* number and not a
    window.

    Budget 0: `Nat.div_eq_of_lt_le` and nothing else. The primality of the modulus is irrelevant
    here, which is the honest accounting — this theorem is about the pattern, and the pattern is
    integer arithmetic. -/
@[axiom_budget 0]
theorem qr_unique (g : QuotientRemainder) (h : qr_constraints g) : g.q = g.n / g.d := by
  rcases h with ⟨_hd, hlo, hhi⟩
  exact (Nat.div_eq_of_lt_le hlo hhi).symm

/-- **The bound is necessary, and the counterexample is kernel-checked.**

    A concrete instance of the wrap the affected circuits were open to: numerator `50000`,
    denominator `2`, and a quotient `q = (p + 49999) / 2 ≈ 1.45 × 10⁷⁶` where the honest quotient is
    `25000`. The two products `q · 2` and `(q + 1) · 2` are reduced modulo `p` — which is what the
    chip's field comparison sees — and land on `49999` and `50001`, i.e. *inside* the accepted
    region on both sides. So `qr_constraints` fails to pin anything at all unless the products are
    known not to wrap, and `range_check(64, ·)` on both operands (every product then `< 2^129 <
    2^253`) is what supplies that. `qr_needs_bound` is the reason the range checks in the six
    repaired circuits are load-bearing rather than defensive.

    `50000` and `2` are not arbitrary: they are the shape `reserve_amount = 5`,
    `total_outstanding = 2` takes in `prove_coverage.zk`, where `reserve * 10000 = 50000`.

    Budget 0, and the proof is `decide` — the kernel evaluates a 77-digit division, two
    multiplications and two moduli. Nothing here is assumed; the counterexample is computed. -/
@[axiom_budget 0]
theorem qr_needs_bound :
    ∃ n d q : Nat,
      0 < d ∧ d ≤ 2 ^ 64 ∧ q ≠ n / d ∧
      q * d % PALLAS_MODULUS ≤ n % PALLAS_MODULUS ∧
      n % PALLAS_MODULUS < ((q + 1) * d) % PALLAS_MODULUS := by
  refine ⟨50000, 2, (PALLAS_MODULUS + 49999) / 2, ?_, ?_, ?_, ?_, ?_⟩
  all_goals decide

end BaseDivGadget
