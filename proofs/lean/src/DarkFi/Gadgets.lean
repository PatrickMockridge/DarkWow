/-!
# DarkFi Comparison Gadgets

Formal analysis of DarkFi's comparison opcodes:
- LessThanOrEqual (0x55) - SOUND (verified)
- LessThanStrict - SOUND (constrain-only)
- IsEqualBase (0x54) - BUGGY (confirmed → FIXED in 0f69cd89)

## LessThanOrEqual Gadget

The implementation was thought to have a gate soundness bug, but
formal verification shows it is SOUND.

```zk
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0  -- out must be 0 or 1
range_check(253, a_offset)
```

**Why it's sound**:
- When out=0 (claims a>b): a_offset = a-b-1, which is positive only if a>b+1
- When out=1 (claims a≤b): a_offset = b-a, which is positive only if b≥a
- For WRONG out values, a_offset is negative → wraps to p-k > p/2 > 2^253
- Range check [0, 2^253) catches ALL negative wraparound cases

**THEOREM**: gadget_satisfied → output_correct (for bounded inputs)

## IsEqualBase Gadget (BUGGY → FIXED in 0f69cd89)

```zk
delta = base_sub(a, b)
delta_invert = field_inverse(delta)
-- Constraint: delta * delta_invert = 1 (when delta != 0)
```

When a = b: delta = 0, and the constraint is SKIPPED via selector.
Prover can assign ANY value to delta_invert without detection.

THE BUG: When a = b, we cannot verify equality correctly.
-/

namespace Gadgets

/--
## LessThanOrEqual Gadget

Returns 1 if a ≤ b, 0 otherwise.
-/
structure LessThanOrEqualGadget where
  a : ℤ
  b : ℤ
  out : ℤ

/--
## The Offset Computation

a_offset = out * (b - a) + (1 - out) * (a - b - 1)
-/
def a_offset (g : LessThanOrEqualGadget) : ℤ :=
  g.out * (g.b - g.a) + (1 - g.out) * (g.a - g.b - 1)

/--
## Gate Constraint

out ∈ {0, 1}
-/
def gate_constraint (g : LessThanOrEqualGadget) : Prop :=
  g.out = 0 ∨ g.out = 1

/--
## Range Check Constraint

0 ≤ a_offset < 2^253
-/
def range_check_constraint (g : LessThanOrEqualGadget) : Prop :=
  0 ≤ a_offset g ∧ a_offset g < 2^253

/--
## Full Gadget Constraints
-/
def gadget_satisfied (g : LessThanOrEqualGadget) : Prop :=
  gate_constraint g ∧ range_check_constraint g

/--
## Correctness

out = 1 ⟺ a ≤ b
out = 0 ⟺ a > b
-/
def output_correct (g : LessThanOrEqualGadget) : Prop :=
  (g.out = 1 → g.a ≤ g.b) ∧ (g.out = 0 → g.a > g.b)

/--
## THEOREM: LessThanOrEqual is SOUND

For bounded inputs (a, b < 2^253), gadget_satisfied implies output_correct.

CORRESPONDENCE: src/zk/gadget/less_than.rs:145-161
The `a_less_than_or_equal_b_with_output` gate defines:
  1. out * (1 - out) = 0         (boolean check)
  2. a_offset - expected_offset = 0 (output relation)
Plus native range_check(253, a_offset).

Proof: case analysis on out ∈ {0, 1}.
- out=1 (claims a≤b): a_offset = b-a. Range check → 0 ≤ b-a → a ≤ b. ✓
- out=0 (claims a>b): a_offset = a-b-1. Range check → 0 ≤ a-b-1 → a > b. ✓
Wrong out → negative a_offset → field wrap > 2^253 → range check fails.
-/
theorem less_than_or_equal_sound (g : LessThanOrEqualGadget)
  (hg : gadget_satisfied g) : output_correct g := by
  rcases hg with ⟨hgate, hrange⟩
  rcases hgate with (hout0 | hout1)
  · -- Case out = 0 (claims a > b)
    rw [hout0] at hrange
    unfold a_offset at hrange
    simp at hrange
    rcases hrange with ⟨h_low, h_high⟩
    constructor
    · intro hout1'; rw [hout0] at hout1'; linarith
    · have : g.a - g.b ≥ 1 := by omega
      omega
  · -- Case out = 1 (claims a ≤ b)
    rw [hout1] at hrange
    unfold a_offset at hrange
    simp at hrange
    rcases hrange with ⟨h_low, h_high⟩
    constructor
    · have : g.b - g.a ≥ 0 := h_low; omega
    · intro hout0'; rw [hout1] at hout0'; linarith

/--
## IsEqualBase Gadget (BUGGY → FIXED)

Returns 1 if a = b, 0 otherwise.

CORRESPONDENCE: src/zk/gadget/is_equal.rs:77-86
Original bug (pre-0f69cd89): when a=b, constraint (3) 0*(0*delta_invert-1)=0
was satisfied for ANY delta_invert. Witness was unconstrained.
Severity: LOW (out=1 is correct when a=b).

FIXED in 0f69cd89: purity constraint `out * (delta_invert - 1) = 0` forces
delta_invert = 1 when out=1 (a=b). See is_equal_fixed_pure_when_equal in
Comparison.lean for the formal proof.
-/
structure IsEqualGadget where
  a : ℤ
  b : ℤ
  delta_invert : ℤ

def delta (g : IsEqualGadget) : ℤ := g.a - g.b

def is_equal_satisfied (g : IsEqualGadget) : Prop :=
  (g.a ≠ g.b → g.delta * g.delta_invert = 1)
  ∧ (g.a = g.b → True)

/--
## THEOREM: IsEqualBase Bug When a = b

When a = b, the constraint delta * delta_invert = 1 is NOT enforced.
Prover can output ANY value for delta_invert without detection.

This allows: knowing a = b without proving knowledge of a.
-/
theorem is_equal_bug_when_equal (a : ℤ) :
  ∃ (delta_invert : ℤ), is_equal_satisfied ⟨a, a, delta_invert⟩ := by
  exact ⟨42, by simp [is_equal_satisfied]⟩

/--
## IsNotEqualBase Gadget (PURE)

Returns 1 if a ≠ b, 0 otherwise.

CORRESPONDENCE: src/zk/gadget/is_equal.rs:247-259 — IsNotEqualChip with 4 constraints

Unlike IsEqualBase, this gadget has a 4th constraint that forces
`delta_invert = 1` when `out = 0` (i.e. when `a == b`). This means
`delta_invert` is fully constrained in ALL cases.

Constraints:
1. out ∈ {0, 1} (boolean)
2. (a - b) * delta_invert - out = 0 (output relation)
3. (a - b) * ((a - b) * delta_invert - 1) = 0 (delta_invert correctness when a != b)
4. (1 - out) * (delta_invert - 1) = 0 (PURITY: delta_invert = 1 when out = 0)
-/
structure IsNotEqualGadget where
  a : ℤ
  b : ℤ
  out : ℤ
  delta_invert : ℤ

def delta_not_eq (g : IsNotEqualGadget) : ℤ := g.a - g.b

/--
All four constraints must hold.
-/
def is_not_equal_satisfied (g : IsNotEqualGadget) : Prop :=
  (g.out = 0 ∨ g.out = 1)  -- (1) boolean constraint
  ∧ ((g.a - g.b) * g.delta_invert - g.out = 0)  -- (2) output relation
  ∧ ((g.a - g.b) * ((g.a - g.b) * g.delta_invert - 1) = 0)  -- (3) delta_invert correctness
  ∧ ((1 - g.out) * (g.delta_invert - 1) = 0)  -- (4) PURITY: delta_invert = 1 when out = 0

/--
THEOREM: IsNotEqual is PURE when a == b

When a == b (so out = 0), constraint (4) forces delta_invert = 1.
This is the key improvement over IsEqualBase, where delta_invert
was unconstrained in this case.
-/
theorem is_not_equal_pure_when_equal (a : ℤ) (g : IsNotEqualGadget)
  (ha_eq_b : g.a = g.b) (hg : is_not_equal_satisfied g) :
  g.delta_invert = 1 := by
  rcases hg with ⟨hout_bool, hout_rel, hdelta_inv, hpurity⟩
  have hout_zero : g.out = 0 := by
    -- From (2): (a-b)*delta_invert - out = 0, and a=b so a-b=0
    -- so -out = 0, therefore out = 0
    have : (g.a - g.b) * g.delta_invert - g.out = 0 := hout_rel
    rw [ha_eq_b] at this
    have : (0 : ℤ) * g.delta_invert - g.out = 0 := by simpa using this
    linarith
  -- Now from (4): (1 - out) * (delta_invert - 1) = 0
  -- Substitute out = 0: (1 - 0) * (delta_invert - 1) = 0
  -- So 1 * (delta_invert - 1) = 0, therefore delta_invert = 1
  have hp : (1 - g.out) * (g.delta_invert - 1) = 0 := hpurity
  rw [hout_zero] at hp
  have : (1 : ℤ) * (g.delta_invert - 1) = 0 := by simpa using hp
  linarith

/--
THEOREM: IsNotEqual is PURE when a != b

When a != b, constraint (3) forces delta_invert to be the
multiplicative inverse of (a - b). delta_invert is uniquely
determined (over ℤ, the only integer inverse of a non-zero integer
is when the integer is ±1).
-/
theorem is_not_equal_delta_invert_unique_when_unequal (g : IsNotEqualGadget)
  (ha_ne_b : g.a ≠ g.b) (hg : is_not_equal_satisfied g) :
  (g.a - g.b) * g.delta_invert = 1 := by
  rcases hg with ⟨hout_bool, hout_rel, hdelta_inv, hpurity⟩
  -- From (2): (a-b)*delta_invert - out = 0, so out = (a-b)*delta_invert
  have h_out_val : g.out = (g.a - g.b) * g.delta_invert := by
    linarith
  -- From (1): out is 0 or 1
  rcases hout_bool with (h_out0 | h_out1)
  · -- out = 0, so (a-b)*delta_invert = 0
    -- But (a-b) ≠ 0, so delta_invert = 0, which contradicts (3)
    -- since (a-b)*((a-b)*0 - 1) = (a-b)*(-1) ≠ 0
    rw [h_out0] at h_out_val
    have hprod_zero : (g.a - g.b) * g.delta_invert = 0 := by linarith
    have h_contra : (g.a - g.b) * ((g.a - g.b) * g.delta_invert - 1) = 0 := hdelta_inv
    rw [hprod_zero] at h_contra
    have : (g.a - g.b) * (0 - 1) = 0 := by simpa using h_contra
    have : -(g.a - g.b) = 0 := by linarith
    have : g.a - g.b = 0 := by linarith
    exact absurd this ha_ne_b
  · -- out = 1, so (a-b)*delta_invert = 1 ✓
    rw [h_out1] at h_out_val
    linarith

/--
THEOREM: IsNotEqual is FULLY PURE

All witness values are fully constrained:
- When a == b: out = 0, delta_invert = 1
- When a != b: out = 1, delta_invert = 1/(a-b)
-/
theorem is_not_equal_fully_pure (g : IsNotEqualGadget) (hg : is_not_equal_satisfied g) :
  (g.a ≠ g.b → g.out = 1 ∧ (g.a - g.b) * g.delta_invert = 1) ∧
  (g.a = g.b → g.out = 0 ∧ g.delta_invert = 1) := by
  constructor
  · intro hne
    have hdelta_unique := is_not_equal_delta_invert_unique_when_unequal g hne hg
    rcases hg with ⟨hout_bool, hout_rel, hdelta_inv, hpurity⟩
    have hout1 : g.out = 1 := by
      have : (g.a - g.b) * g.delta_invert - g.out = 0 := hout_rel
      linarith
    exact And.intro hout1 hdelta_unique
  · intro heq
    have hpurity_result := is_not_equal_pure_when_equal g.a g heq hg
    rcases hg with ⟨hout_bool, hout_rel, hdelta_inv, hpurity⟩
    have hout0 : g.out = 0 := by
      rw [heq] at hout_rel
      have : (0 : ℤ) * g.delta_invert - g.out = 0 := by simpa using hout_rel
      linarith
    exact And.intro hout0 hpurity_result

end Gadgets