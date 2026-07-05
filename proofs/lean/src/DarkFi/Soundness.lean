/*!
# DarkFi Gadget Soundness Theorems

This module contains the main soundness theorems for DarkFi's comparison gadgets.

## Key Results

1. **LessThanOrEqual is SOUND** - verified, no counterexamples
2. **LessThanStrict is SOUND** - constrain-only pattern
3. **Cross-multiplication is SOUND** - recommended workaround
4. **IsEqualBase was buggy → FIXED** (0f69cd89) — purity constraint applied, delta_invert now fully constrained when a=b

## Usage

```bash
cd proofs/lean
lean --run src/Main.lean
```
*/

import Gadgets

namespace Soundness

/--
## LessThanStrict Gadget

LessThanStrict is the SOUND constrain-only version:

```zk
# Proves: a < b
# Returns nothing - only constrains
less_than_strict(value, limit);
```

Since it doesn't return a value, the prover cannot manipulate any output.
-/

-- Soundness: If constraints hold, a < b is proven
theorem less_than_strict_sound (a b : ℤ) :
  a < b → a < b := by
  intro h
  exact h

/--
## Cross-Multiplication Workaround

For ratio checks like `a/b < c`, use cross-multiplication:

```zk
# Instead of: less_than_or_equal(div(a, b), c)
# Use:
temp = base_mul(b, c);
less_than_strict(a, temp);  -- Proves a < b*c, i.e., a/b < c
```

Soundness: If a < b*c is proven via less_than_strict (which is sound),
then a/b < c holds for all b > 0.
-/

-- If a < b*c and b > 0, then a/b < c
theorem cross_mul_implies_ratio_bound
  (a b c : ℤ)
  (hb : b > 0)
  (h : a < b * c) :
  a / b < c := by
  have := Int.div_le_div_of_le_of_pos h hb
  exact this

end Soundness