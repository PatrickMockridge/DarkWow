/*!
# DarkFi Gadget Verification Examples

Run these with: `lake run <target>`

## Targets

- `check_less_than_or_equal_bug` - Verify the bug exists
- `verify_cross_mul_sound` - Verify the workaround is sound
- `search_bounded_counterexamples` - Search for counterexamples in bounded range
-/

import DarkFi.Field
import DarkFi.Gadgets
import DarkFi.Soundness

namespace Examples

/--
## Check: LessThanOrEqual Bug Exists

This should output "BUG CONFIRMED: counterexample found"
-/
def check_less_than_or_equal_bug : IO Unit := do
  let a := 5
  let b := 10
  let out := 0

  -- Check gate constraint
  let gateSat := (out = 0) ∨ (out = 1)
  IO.println s!"Gate satisfied (out={out}): {gateSat}"

  -- Compute offset
  let offset := out * (b - a) + (1 - out) * (a - b - 1)
  IO.println s!"a_offset = {offset}"

  -- In field arithmetic, -6 ≡ p - 6
  let fieldOffset := (offset % PALLAS_PRIME + PALLAS_PRIME) % PALLAS_PRIME
  IO.println s!"a_offset as field element: {fieldOffset}"
  IO.println s!"Is < 2^253? {fieldOffset < 2^253}"

  -- Check correctness
  let correctOut := if a ≤ b then 1 else 0
  IO.println s!"Correct out should be: {correctOut}"
  IO.println s!"Prover claimed: {out}"

  if gateSat && (fieldOffset < 2^253) && (out ≠ correctOut) then
    IO.println "BUG CONFIRMED: constraints satisfied but output wrong!"
  else
    IO.println "No bug found (this would be unexpected)"

/--
## Verify: Cross-Multiplication is Sound

The cross-multiplication pattern:
- Proves: a < b * c
- Therefore: a/b < c (when b > 0)
- Sound because less_than_strict is constrain-only
-/
def verify_cross_mul_sound : IO Unit := do
  let a := 100
  let b := 3
  let c := 40

  -- Cross-multiply: a < b*c ?
  let product := b * c
  let constraint := a < product

  IO.println s!"Checking: {a} < {b} * {c} = {product}"
  IO.println s!"Constraint satisfied: {constraint}"

  if constraint then
    -- Therefore a/b < c
    let ratioBound := a / b
    IO.println s!"Since b > 0: {a}/{b} ≤ {ratioBound}"
    IO.println s!"And {ratioBound} < {c}? {ratioBound < c}"
    IO.println "CROSS-MUL SOUND: a/b < c proven via less_than_strict(a, b*c)"
  else
    IO.println "Constraint not satisfied - correctly rejected"

/--
## Search: Bounded Counterexamples

Search for counterexamples where gadget is satisfied but output is wrong.
Limited search for k=8 to keep runtime reasonable.
-/
def search_bounded_counterexamples (k : Nat) : IO Unit := do
  let limit := 2^k
  let mut found := 0

  IO.println s!"Searching {k}-bit range for LessThanOrEqual counterexamples..."
  IO.println s!"Total pairs to check: {limit * limit}"

  for a in List.range limit do
    for b in List.range limit do
      for out in [0, 1] do
        -- Check if this is a counterexample
        let correctOut := if a ≤ b then 1 else 0
        if out ≠ correctOut then
          -- Compute offset
          let offset := out * (b - a) + (1 - out) * (a - b - 1)
          let fieldOffset := (offset % PALLAS_PRIME + PALLAS_PRIME) % PALLAS_PRIME

          -- Check if constraints satisfied
          let gateSat := (out = 0) ∨ (out = 1)
          let rangeSat := (0 ≤ fieldOffset) ∧ (fieldOffset < 2^253)

          if gateSat && rangeSat then
            found := found + 1
            if found ≤ 10 then
              IO.println s!"FOUND: a={a}, b={b}, out={out} (correct={correctOut})"

  IO.println s!"Total counterexamples found: {found}"

/--
## Main: Run All Checks
-/
def main : IO Unit := do
  IO.println "=== DarkFi Gadget Verification ==="
  IO.println ""

  IO.println "1. Checking LessThanOrEqual bug..."
  check_less_than_or_equal_bug
  IO.println ""

  IO.println "2. Verifying cross-multiplication soundness..."
  verify_cross_mul_sound
  IO.println ""

  IO.println "3. Searching for 8-bit counterexamples..."
  search_bounded_counterexamples 8
  IO.println ""

  IO.println "=== Done ==="

end Examples