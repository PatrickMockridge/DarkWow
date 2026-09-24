/-
# DarkFi Gadget Soundness Theorems

This module contains the main soundness theorems for DarkFi's comparison gadgets.

## Key Results

This file holds **no** proof of `less_than_strict` soundness. It used to claim four results;
what remains is `cross_mul_implies_ratio_bound`, which is now a theorem rather than an axiom.

1. ~~**LessThanStrict is SOUND** — constrain-only pattern~~ — the theorem that claimed this was
   `a < b → a < b`. The real result is `Comparison.less_than_strict_sound`.
2. **Cross-multiplication** — `cross_mul_implies_ratio_bound` below, discharged from
   `Field.cross_mul_lt` rather than assumed.
3. **IsEqualBase was buggy → FIXED** (0f69cd89) — purity constraint applied, delta_invert now
   fully constrained when a=b. Not proven in this file; see `Comparison.lean`.

## Usage

```bash
cd proofs/lean
lake build DarkFi                        # type-checks the proofs — this is the gate
python3 ../../script/check_lean_axioms.py --require-collector
```

This block used to say `lean --run src/Main.lean`. It should not: `src/Main.lean` does not
compile (21 errors, measured 2026-09-24, on its version at HEAD as well — see its header) and no
gate invokes it, so running it verifies nothing.
-/

import DarkFi.Gadgets
import DarkFi.Field
import DarkFi.Axioms
import DarkFi.AxiomBudget

namespace Soundness

/-
## LessThanStrict Gadget — no proof of it here

LessThanStrict is the constrain-only version:

```zk
# Proves: a < b
# Returns nothing - only constrains
less_than_strict(value, limit);
```

Since it doesn't return a value, the prover cannot manipulate any output.

A theorem `less_than_strict_sound (a b : ℤ) : a < b → a < b` used to sit here, proved by
`intro h; exact h`. That is the identity function: the conclusion *is* the hypothesis, and the
statement never mentions the `less_than_strict` gadget. The name claimed a soundness result
about the gadget; the theorem was `id`.

The real result is `Comparison.less_than_strict_sound` (`Comparison.lean`), which takes the
gadget's actual parameters — `(a b m offset : Int)` — rather than restating `a < b`.
-/

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

-- This used to be `axiom cross_mul_implies_ratio_bound`, with the comment "Marked as axiom
-- until completed". The proof was completed elsewhere in the tree: `Field.cross_mul_lt`
-- proves exactly this statement from `Int.div_lt_iff_lt_mul`. Keeping an axiom here would
-- have been an unproved second copy of a theorem already present, so the axiom is gone and
-- the theorem below discharges it. `#print axioms` for it is empty.
--
-- The old comment claimed the available lemma returned `a/b ≤ c` rather than `a/b < c`. The
-- strict form is `Int.div_lt_iff_lt_mul`, which is what `Field.cross_mul_lt` uses.
@[axiom_budget 1]
theorem cross_mul_implies_ratio_bound
  (a b c : ℤ)
  (hb : b > 0)
  (h : a < b * c) :
  a / b < c :=
  cross_mul_lt hb h

end Soundness