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
exponentiation by squaring: `p − 2` is 255 bits, 77 of them set, and the loop squares a running
power and multiplies it into an accumulator for each set bit — 254 squarings + 76 conditional
multiplications + 1 final multiplication = **331** multiplication gates. The `sqMulGo` definition
below is that loop, transcribed in the shape `Emission.fixedPowDecayGo` uses, and `sqMul_is_inverse`
is the content: for `b ≠ 0` the result times `b` is `a`, i.e. it really is division.

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
A proof that the chip's 253-bit range check implies the integer reading of a comparison is not here
and is not claimed. That gap is recorded in the register as OBL-Z12.
-/

namespace BaseDivGadget

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

    **The measured budget is 2, not 0**, and the gap is worth recording rather than explaining
    away: `Classical.choice` from the automation, and `pallasPrime`. The second is not reached by
    the *proof* — it is reached by the *type*: every statement here is about
    `ZMod PALLAS_MODULUS`, and elaborating that type picks up the global
    `instance : Fact (Nat.Prime PALLAS_MODULUS)`. So "no assumption beyond Lean's logic" is not
    available for any theorem stated over this field, however elementary the arithmetic. The
    `Nat`-level statements at the foot of this file read 0 for exactly that reason. -/
@[axiom_budget 2]
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
    exponent. Budget 2 for the reason given on `sqMulGo_eq`. -/
@[axiom_budget 2]
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
    mean in a field" question does not arise. Budget 2, `sqMulGo_eq`'s reason. -/
@[axiom_budget 2]
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
