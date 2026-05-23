# Lean4 Formal Verification — DarkWow Opcodes

Formal verification of DarkWow zkVM opcodes using the Lean 4 proof assistant.
All opcodes verified here are **DarkWow fork additions** — they do not exist in
upstream DarkFi's zkVM.

## Why This Fork Exists (Opcode Justification)

### The Problem: Upstream's zkVM Is Too Limited

Upstream DarkFi's zkVM provides basic arithmetic (add, mul, sub) and comparison
constraints (less_than_strict, less_than_loose, bool_check, constrain_equal_base).
These are sufficient for assertion-only constraints but **cannot express Boolean
logic** — you can enforce that a value is in range, but you cannot return a
conditional result and use it in subsequent computations.

Boolean-returning opcodes enable:
- **Conditional execution**: "if X, then Y, else Z" inside a circuit
- **O-Cap predicates**: "prove you have credential X without revealing identity"
- **Multi-branch logic**: ZK circuits that behave differently based on inputs

Without Boolean-returning opcodes, every ZK contract must be a single straight-line
computation with no branching. This severely limits what contracts can express.

### What This Fork Built

| Opcode | Code | Returns | Fork Status |
|--------|------|---------|-------------|
| `less_than_or_equal` | 0x55 | 1 if a ≤ b, else 0 | Built here |
| `base_lt_strict` | 0x57 | 1 if a < b, else 0 | Built here |
| `is_not_equal` | 0x62 | 1 if a ≠ b, else 0 | Built here |
| `base_div` | 0x58 | a / b mod p | Built here |

### The IsEqualBase Bug and the IsNotEqual Solution

Upstream's `is_equal_base` (0x54) has a known soundness issue: when `a == b`,
the `delta_invert` witness is completely unconstrained — the prover can assign
any value without detection. While this doesn't enable false proofs (the output
is still correct), it's mathematically impure.

**`IsNotEqual` (0x62) is the first fully constrained pure Boolean operator in
the zkVM.** It adds a 4th constraint:

```
(1 - out) * (delta_invert - 1) = 0
```

When `a == b` (so `out = 0`), this forces `delta_invert = 1`. No unconstrained
witnesses. The same pattern can fix `IsEqualBase`: add `out * (delta_invert - 1) = 0`.

### Lean4 Proves Purity

```lean4
theorem is_not_equal_fully_pure (g : IsNotEqualGadget) (hg : is_not_equal_satisfied g) :
  (g.a ≠ g.b → g.out = 1 ∧ (g.a - g.b) * g.delta_invert = 1) ∧
  (g.a = g.b → g.out = 0 ∧ g.delta_invert = 1) := ...
```

This theorem proves that **all witness values are fully determined** in all cases.
There is no assignment where `delta_invert` can take an arbitrary value while
satisfying all constraints.

## Running the Verification

```bash
# Install Lean 4
curl -L https://github.com/leanprover/elan/releases/download/v4.2.1/elan-x86_64-unknown-linux-gnu.tar.gz | tar xz
./elan-init -y --default-toolchain 4.12.0
source ~/.elan/env

# Build and run
cd proofs/lean
lake build
lean --run src/Main.lean
```

## Project Structure

```
proofs/lean/
├── lean-toolchain          # Lean 4.12.0
├── lakefile.lean           # Build configuration
├── README.md               # This file
└── src/
    ├── Main.lean           # Executable verification tests
    │                         Run with: lean --run src/Main.lean
    ├── Examples.lean       # Additional verification examples
    └── DarkFi/
        ├── Field.lean      # Pallas field arithmetic formalization
        ├── Gadgets.lean    # Gadget specifications and purity theorems
        └── Soundness.lean  # Cross-multiplication equivalence theorems
```

## Verification Results

| Opcode | Method | Result |
|--------|--------|--------|
| `LessThanOrEqual` (0x55) | Exhaustive search (1000×1000) | 0 counterexamples |
| `IsEqualBase` (0x54) | Constraint analysis | **Bug found**: delta_invert unconstrained |
| `IsNotEqual` (0x62) | Exhaustive search + purity theorem | **0 bugs, fully pure** |
| `NotBase` (0x56) | Constraint analysis | Sound |
| `BaseLtStrict` (0x57) | Exhaustive search (1000×1000) | 0 counterexamples |
| `BaseDiv` (0x58) | Theorem proving (Fermat) | Mathematically verified |

## License

AGPL-3.0-only.
