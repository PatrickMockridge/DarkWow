# zkVM Primitive Layer: Opcode Reasoning

> **Note:** The core zkVM opcode layer (constraint system, bincode format, WASM execution model) is inherited from upstream DarkFi. DarkWow-specific additions (LessThanOrEqual, BaseDiv, IsNotEqual) are documented in [What's Different from Upstream](../about/differences_from_upstream.md).

> **Prerequisite reading**: Before this document, read [Field Arithmetic Constraints](field_arithmetic.md). It explains why every operation in a ZK circuit must be re-expressed in finite field arithmetic — and why that re-expression is the primary difficulty in ZK circuit design. The examples in this document assume you understand field vs. integer ordering and modular arithmetic.

The opcode layer is not an implementation detail — it is the **primitive layer** that
determines the entire expressiveness surface of DarkWow's smart contract system.

DarkWow's zkVM executes ZK circuits compiled from `.zk` source files. Every contract
— identity credentials, DEX atomic swaps, bridge deposits, stablecoin positions —
ultimately reduces to a sequence of opcodes. The available opcodes define the
mathematical and logical operations that contract authors can assume exist.

**What this means**: If an opcode is missing, every contract that would logically
need it must either work around it with complex compositions, leave the proof
incomplete, or simply not exist.

This is why reasoning about the opcode layer is a **core architectural discussion**,
not a peripheral one. The roadmap of what DarkWow's contracts can express is
fundamentally constrained by — and derivable from — the opcode set.

## Why Opcode Reasoning Belongs on the Roadmap

DarkWow's core team is correctly focused on core consensus, protocol security, and
mainnet stability. The opcode primitives discussed here represent the **contract
expressiveness layer** that builds on that foundation.

Features currently on the "dusty shelf" — advanced identity predicates, intent-based
AMMs, sophisticated stablecoin logic, time-locked conditions — are not blocked by
consensus or cryptography. They are blocked by a small number of missing opcodes
in the VM that maps circuit logic into proofs.

This is a tractable engineering problem: each opcode is a self-contained gadget
implementation in Halo2, testable in isolation, and additive (no existing opcode
is changed or removed). The reasoning here makes visible the dependency between
promised features and the primitive layer that enables them.

Consider features explicitly discussed in DarkWow's public communications:

| Promised Feature | Required Primitives | Notes |
|-----------------|---------------------|-------|
| "Prove you meet criteria without revealing data" (identity) | `LessThanOrEqual`, `IsEqualBase` | ✅ `LessThanOrEqual` implemented |
| "Collateral must exceed debt" (stablecoin) | `LessThanOrEqual` | ✅ `LessThanOrEqual` implemented |
| "Order matching at or above price" | `LessThanOrEqual`, `IsEqualBase` | Partial; atomic swap MVP works without |
| "Partial fills where amount <= requested" | `LessThanOrEqual` | Partial; atomic swap works without |
| "Liquidation when collateral/debt ratio < threshold" | `LessThanOrEqual`, `BaseMul` | ✅ Uses cross-multiplication (see `dao/exec.zk`), no `BaseDiv` needed |
| "Generic intent fill conditions" (intent-amm fork) | `IsEqualBase`, `LessThanOrEqual`, `NotBase` | ✅ All implemented |
| "Time-locked reveal with bypass conditions" | `IsEqualBase`, `NotBase` | ✅ All implemented |
| "ZK-computed AMM pricing" | `BaseMul` | TWAP supplied as oracle input; no in-circuit division needed |

## The Math Behind Comparison Opcodes

This section provides a technical deep-dive into how the comparison opcodes work, why they're challenging to implement correctly, and what can go wrong. Understanding this is essential before using these opcodes in production circuits.

### The Fundamental Problem: Field Elements Are Not Integers

DarkWow's zkVM operates in the Pallas field — the scalar field of bn254, with prime order:

```
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
```

All `Base` values are elements of this field. The critical insight is that **field arithmetic and integer arithmetic are not the same** near the modulus boundary.

Consider comparing two integers `a` and `b` where `a < b`. In a ZK circuit, we manipulate field elements `a_f` and `b_f` (their field representations). The relation between integer ordering and field ordering breaks down near `p`:

```
Integer: 0 < 1 < 2 < ... < p-2 < p-1
Field:   0 ≡ p < 1 < 2 < ... < p-2 < p-1 ≡ -1  (mod p)
```

For most values (well below `p`), integer ordering and field ordering agree. But for values in the range `[p - 2^32, p)`, field wraparound causes `a_f > b_f` even when `a < b` as integers.

**Practical implication**: A gadget that compares field elements as integers must either:
1. Constrain all inputs to a safe range (e.g., bottom 253 bits), or
2. Handle wraparound explicitly

The current implementation uses approach (1): the range check constrains values to `[0, 2^253)`, safely below the problematic boundary.

---

### How `LessThanLoose` Works: Range Check Gadget

The core technique is to compute the difference `d = a - b` and check which range `d` falls into:

```
If a < b (integer):  d = a - b is in [-(p), 0)    → in field: d ∈ (p - 2^253, p)
If a ≥ b (integer):  d = a - b is in [0, p)        → in field: d ∈ [0, 2^253)
```

The gadget computes `d = a - b` using `BaseSub`, then constrains `d` to the range `[0, 2^253)` via a range check. If the range check passes, `a ≥ b` as integers. If it fails, `a < b`.

`LessThanStrict` is the same check but with an offset that accounts for the boundary case when `a == b`:

```zk
# LessThanStrict: returns 1 if a < b
d = base_sub(a, b);
less_than_range_check(d + 2^253);  # Shifted so equality fails the check
```

---

### LessThanOrEqual

`LessThanOrEqual(a, b)` returns `1` if `a ≤ b`, `0` otherwise. It combines range checks on `a - b` and `b - a` with a gate constraint that produces a `Base` output usable in subsequent computation.

`IsEqualBase` uses a delta-invert approach with a known soundness issue (`delta_invert` unconstrained when `a == b`). For the full formal verification of these comparison gates — including the delta-invert attack, gate soundness analysis, Lean4 machine-checkable proofs, and the `IsNotEqual` pure Boolean gate — see [Opcodes and Formal Verification](opcodes.md).


## Case Study: Reasoning About a Missing Opcode

This section demonstrates how to reason about a missing opcode by working through a real example from contract development. The goal is to model the thought process, not just the conclusion.

### The Problem: "I Need to Check if a Value Is Within a Range"

During stablecoin development, a circuit needed to verify that a liquidator reward did not exceed the collateral. The circuit author wrote:

```zk
# Want: reward <= collateral
# If true, the position can be liquidated
constrain_equal_base(reward, collateral);  # WRONG: this checks equality
```

This fails because equality is not the right relation. The author needed a comparison — not just an assertion, but a **value** that could be used in subsequent logic.

### Step 1: Identify What You Actually Need

Before reaching for an opcode, ask: what is the circuit actually trying to enforce?

The goal was to enforce: `liquidator_reward ≤ collateral_amount`

This is a **less-than-or-equal** check. But the existing zkVM only had `LessThanStrict` which *constrains* the circuit to fail if `a ≥ b` — it doesn't return a value. You cannot do:

```zk
x = less_than_strict(a, b);  # ERROR: returns ()
```

### Step 2: Analyze the Existing Tools

What does the VM actually provide?

| What you have | What it does | Limitation |
|--------------|-------------|-----------|
| `LessThanStrict(a, b)` | Fails the circuit if `a ≥ b` | Returns nothing |
| `LessThanLoose(a, b)` | Same — fails if `a ≥ b` | Returns nothing |
| `BoolCheck(x)` | Enforces `x ∈ {0, 1}` | Needs a value to check |
| `ConstrainEqualBase(a, b)` | Enforces `a = b` | Returns nothing |

All existing comparison is **constrain-only**: it enforces a relation without producing a value. This is fine for hard constraints ("this must be true or the proof fails") but useless for soft logic ("if this is true, do A, else do B").

### Step 3: Ask the Right Question

The right question is not "what opcode am I missing?" — it is: **what relation do I need, and what inputs does it consume?**

For `reward ≤ collateral`, the circuit needed to:
1. Compute a comparison result `r ∈ {0, 1}`
2. Use `r` in a subsequent constraint: `constrain_equal_base(r, 1)` (asserting the check passed)

This requires an opcode with signature `(Base, Base) → Base`, not `(Base, Base) → ()`.

### Step 4: Derive the Solution

The derivation: `a ≤ b` if and only if `a < b OR a == b`.

```
LessThanOrEqual(a, b) = IsEqual(a, b) OR LessThanLoose(a, b)
```

Each of these primitives either existed or could be constructed:
- `LessThanLoose` already existed (constrain-only)
- `IsEqual` can be built from field arithmetic: `1 - (a - b) - (b - a)` then `BoolCheck`-ed

The result is a **return-value opcode** that composes with the rest of the VM.

### Step 5: Identify the Constraints

Not every comparison can be added to a ZK VM. The constraints that matter:

**Input domain**: Comparisons over field elements behave like integers only within safe ranges. The gadget must constrain inputs to `[0, 2^253)` to avoid field wraparound near the modulus `p`.

**Composability**: A return-value opcode must produce a `Base` that can be consumed by other opcodes (`CondSelect`, `BoolCheck`, etc.). If the VM's type system doesn't allow this, the opcode is useless.

**Soundness**: The gadget must be hard for a malicious prover to bypass. A comparison gadget that can be fooled is worse than no gadget — it creates false security.

### Step 6: Recognize Workarounds

Before implementing a new opcode, check whether existing opcodes can enforce the same constraint differently.

For ratio checks like `collateral/debt < threshold`, a circuit can avoid comparison entirely using cross-multiplication:

```zk
# Instead of: collateral/debt < threshold
# Do:         collateral < threshold * debt
lhs = base_mul(collateral, 1);
rhs = base_mul(threshold, debt);
less_than_strict(lhs, rhs);  # Existing opcode, no new gadget needed
```

This is exactly what `dao/exec.zk` does for approval ratio checks. The lesson: **ZK circuits enforce constraints, they don't compute values**. Express your invariant as a constraint, not as an algorithm.

### The General Pattern

When reasoning about a missing opcode, follow this checklist:

1. **What relation do I need?** (ordering, equality, membership, etc.)
2. **Does an existing opcode enforce this relation?** (even if constrain-only)
3. **Can I express my requirement as a constraint using existing opcodes?** (cross-multiplication for ratios, etc.)
4. **If I need a return value, what opcode consumes it?** (compose all the way to the output)
5. **What are the domain constraints on the inputs?** (range, bit-width, field membership)
6. **What's the soundness story?** (can a malicious prover bypass this?)

---

## Existing Opcode Inventory

**Elliptic Curve** (available):
- `EcAdd`, `EcMul`, `EcMulBase`, `EcMulShort`, `EcMulVarBase`
- `EcGetX`, `EcGetY`
- Used for: Pedersen commitments, public key derivation, hashing to points

**Hashing** (available):
- `PoseidonHash`, `MerkleRoot`, `SparseMerkleRoot`
- Used for: commitments, nullifiers, Merkle membership proofs

**Field Arithmetic** (available):
- `BaseAdd`, `BaseSub`, `BaseMul`
- Used for: amount arithmetic, scaling values

**Comparison** (available):
- `LessThanStrict`, `LessThanLoose` — constrain but don't return
- `LessThanOrEqual` (0x55) — return-value comparison, Lean 4 verified
- `IsNotEqual` (0x62) — pure Boolean inequality gate, Lean 4 verified
- `IsEqualBase` (0x54) — return-value equality (known bug, see opcodes.md)
- `NotBase` (0x56) — logical negation
- `BaseLtStrict` (0x57) — return-value strict less-than
- `BoolCheck` — enforce 0 or 1

**Control Flow** (available):
- `CondSelect`, `ZeroCondSelect` — mux-style conditional selection

**Constraints** (available):
- `ConstrainEqualBase`, `ConstrainEqualPoint`, `ConstrainInstance`
- `RangeCheck`, `DebugPrint`

## Reasoned Opcodes

These opcodes have been reasoned about through contract development, external fork
experimentation, and feature roadmapping. They are not speculative — they are
needed to deliver functionality already discussed publicly.

### Verification Status

`LessThanOrEqual` (0x55), `IsEqualBase` (0x54), `NotBase` (0x56), `BaseLtStrict` (0x57), and `BaseDiv` (0x58) are additions beyond what upstream currently provides in the zkVM. The initial Rust implementation was prototyped on a [separate experimental branch](https://codeberg.org/rusticml/darkfi/commits/branch/less-than-or-equal-experiment) (by rusticml) and integrated into this repository at commit `41b0629e0`. Formal Lean4 verification was completed on this fork. `BaseModExp` remains unimplemented.

**Formal Verification Results** (Lean 4, see [Opcodes and Formal Verification](opcodes.md)):

| Opcode | Status | Notes |
|--------|--------|-------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Lean 4 exhaustive testing, no counterexamples |
| `IsEqualBase` (0x54) | ❌ Bug | delta_invert unconstrained when `a == b` |
| `IsNotEqual` (0x62) | ✅ **Fully Pure** | All witnesses constrained in all cases; purity theorem |
| `NotBase` (0x56) | ✅ Verified Sound | Input range-checked to `{0,1}` |
| `BaseLtStrict` (0x57) | ✅ Verified Sound | Lean 4 exhaustive testing |
| `BaseDiv` (0x58) | ✅ **Implemented** | Binary exponentiation (~254 field muls) |

#### `IsEqualBase` Bug Detail

When `a == b`:
- `delta = base_sub(a, b) = 0`
- `delta_invert = field_inverse(delta)` is **completely unconstrained**
- The constraint `delta * delta_invert = 1 - out` is bypassed when `delta = 0`
- **Does not enable false proofs** (out=1 is correct when a==b), but is mathematically inelegant

**Fix**: Add an `is_zero` gadget to properly constrain `delta_invert`. Use `ConstrainEqualBase(a, b)` for assertion-only cases.

#### `BaseDiv` Status

**IMPLEMENTED** ✅

Mathematically verified via Fermat's little theorem:
- `div_mul_cancel`: `(a / b) * b ≡ a (mod p)` for b ≠ 0
- `a / 1 = a`
- `0 / b = 0`

Implementation uses binary exponentiation (~254 field multiplications).

#### Remaining Work

- **Fix IsEqualBase** — Add `is_zero` gadget to constrain delta_invert
- **Implement BaseModExp** — For RSA verification, hash-based commitments

### `LessThanOrEqual(a, b)` → Base

**Signature**: `(Base a, Base b) → Base` (returns 0 or 1)

**Purpose**: Returns 1 if `a <= b`, 0 otherwise. Usable as a value in
subsequent computation.

**What it unlocks**:

```zk
# Predicate verification (identity credentials)
is_authorized = less_than_or_equal(threshold, attribute_value);
constrain_equal_base(is_authorized, claimed_result);

# Collateralization check (stablecoin)
is_solvent = less_than_or_equal(debt_times_ratio, collateral);
constrain_instance(is_solvent);

# Partial fill logic (DEX/intent-amm)
fill_ok = less_than_or_equal(fill_amount, requested_amount);
```

**Implementation**: Returns `1` if `a <= b`, 0 otherwise. Can be built as:
`IsEqualBase(a, b) | LessThanLoose(a, b)`. Requires Halo2 range check gadget
similar to `LessThanLoose`.

---

### `IsEqualBase(a, b)` → Base

**Signature**: `(Base a, Base b) → Base` (returns 0 or 1)

**Purpose**: Returns 1 if `a == b`, 0 otherwise. Unlike `ConstrainEqualBase`
(which constrains but returns nothing), this produces a usable value.

**What it unlocks**:

```zk
# Intent fill conditions (from intent-amm fork)
require_on_fill = is_equal_base(intent_action, ACTION_FILL);
bypass_on_cancel = is_equal_base(intent_action, ACTION_CANCEL);
final_condition = cond_select(bypass_on_cancel, 1, require_on_fill);

# State machine transitions
next_state = is_equal_base(current_state, STATE_OPEN) + ...;
```

**Implementation**: Returns 1 if `a == b`, 0 otherwise. Can be derived from:
`1 - (a - b) - (b - a)` using field arithmetic, then `BoolCheck`-ed to ensure
result is 0 or 1.

---

### `NotBase(a)` → Base

**Signature**: `(Base a) → Base` where `a` must be 0 or 1

**Purpose**: Returns logical negation: `1 - a`. Enables composing complex
boolean logic from simpler operations.

**What it unlocks**:

```zk
# Complement of a range
outside_range = not_base(in_range);

# Negated predicates
not_expired = not_base(is_expired);
```

**Implementation**: Trivially `1 - a` using `BaseSub(1, a)`. Requires `BoolCheck`
on input to ensure `a` is 0 or 1.

---

### `BaseDiv(a, b)` → Base (IMPLEMENTED)

**Signature**: `(Base a, Base b) → Base`

**Purpose**: Field division `a / b` (modular multiplicative inverse via Fermat's little theorem).

**Implementation**: Binary exponentiation `a * b^{p-2} mod p`
- Cost: ~500 field multiplications (253 squarings + up to 249 multiplications)
- Opcode: `0x58`

**Usage**:
```zk
quotient = base_div(a, b);  // a / b mod p
```

**For ratio checks**: Cross-multiplication with `less_than_strict` is still preferred when appropriate (cheaper):

```
# Prove: a/b < c  ⟺  a < b*c (no BaseDiv needed)
temp = base_mul(b, c);
less_than_strict(a, temp);
```

**Legitimate uses for `BaseDiv`**:
- Verifying externally-computed quotients
- General circuit ergonomics where cross-multiplication is impractical

**Note**: TWAP prices are expected to be supplied as oracle inputs, not computed in-circuit.

---

### `BaseModExp(base, exp, mod)` → Base (not implemented)

**Signature**: `(Base base, Base exp, Base mod) → Base`

**Purpose**: Modular exponentiation `base^exp mod mod`. Essential for
RSA verification, hash-based commitments, and certain cryptographic protocols.

**What it unlocks**:
- ZK verification of RSA signatures in credentials
- Hash-based accumulators
- Time-lock puzzles and commitment schemes

---

### `IsNotEqual(a, b)` → Base (0x62)

**Signature**: `(Base a, Base b) → Base` (returns 0 or 1)

**Purpose**: Returns 1 if `a != b`, 0 if `a == b`. Unlike `IsEqualBase` (0x54)
which has a known `delta_invert` bug when `a == b`, `IsNotEqual` is a fully
constrained symmetric Boolean gate — both branches (equal and not-equal) are
fully constrained, leaving no unconstrained witness variables for a malicious
prover to exploit.

**Formal Verification**: ✅ **Fully Pure** — Lean 4 exhaustive search + purity
theorem. This is the first fully constrained Boolean operator in the zkVM.
See [Opcodes and Formal Verification](opcodes.md) for the proof.

**What it unlocks**:

```zk
# Identity predicates — prove an attribute differs from a known value
is_different = is_not_equal(attribute_value, expected_value);

# Combined with LessThanOrEqual for range exclusion
outside_range = is_not_equal(in_range, 0);

# Circuit branch selection
take_branch_b = is_not_equal(condition, 1);  # negate the condition
```

**Why `IsNotEqual` instead of `IsEqualBase`**: `IsEqualBase` (0x54) has a
`delta_invert` vulnerability — when `a == b`, the witness `delta_invert` is
completely unconstrained, letting the prover set it arbitrarily. By contrast,
`IsNotEqual` treats both `a == b` and `a != b` symmetrically, with every witness
variable fully constrained in both cases. The fix pattern proven in `IsNotEqual`
(`out * (delta_invert - 1) = 0`) can be backported to `IsEqualBase` when needed.

---
### `BaseLtStrict(a, b)` → Base (returns value)

**Signature**: `(Base a, Base b) → Base` (returns 0 or 1)

**Purpose**: Like `LessThanStrict` but returns the result as a value
instead of just constraining. Having both this and `LessThanOrEqual`
makes arithmetic expressions cleaner than negating.

## Opcode Interaction Graph

These opcodes compose into higher-level constructions:
```
LessThanOrEqual ──┬──► Predicate verification (identity)
          │          └──► Collateralization checks (stablecoin)
          │          └──► AMM price bounds (DEX)
          │
IsEqualBase ──────┬──► Intent fill conditions (intent-amm)
          │          └──► State machine transitions
          │          └──► Schema validation
          │
IsNotEqual ───────┬──► Identity predicates (attribute != expected)
          │          └──► Range exclusion
          │          └──► Branch negation
          │
NotBase ──────────┘    (used to compose boolean logic)
          │
          └──► Combined comparisons: a < b OR a != c
                          = LessThanLoose(a,b) + IsNotEqual(a,c)
```

---

## Cross-Contract ZK Composition: Trusted Setup Workaround

When a contract needs to verify state from another contract (e.g., DEX verifying
PromissoryNote contract's state), the ideal solution is cross-contract ZK composition
— calling one contract's ZK circuit from another. This is **not yet implemented**.

**Current workaround**: The DEX stores a **trusted Merkle root** of the PromissoryNote
contract's coin tree at initialization. Swaps verify proofs against this
trusted root via `verify_lock_proof()`. This is a security trade-off: the trusted
root is user-provided, requires manual updates when PromissoryNote's state changes, and
relies on nullifiers for double-spend prevention within DEX.

**Long-term solution**: Cross-contract ZK composition opcodes enabling circuits
to call other circuits directly. See [O-Cap](ocap.md) for cross-contract patterns.

---

## Adding Custom Opcodes

1. **Define** in `src/zkas/opcode.rs` using `define_opcodes!` macro — assign opcode hex ID, name, and type signature
2. **Implement** in `src/zk/vm.rs` — add handler for the new opcode, pop arguments from stack, push result
3. **Use** in circuits — reference the opcode in `.zk` source files

## References

- [Field Arithmetic Constraints](field_arithmetic.md) — foundational reading for understanding why ZK circuit arithmetic is hard
- [O-Cap & Composable Privacy](ocap.md) — the authorization pattern these opcodes enable and how they compose across contracts
- [intent-amm fork (rusticml)](https://codeberg.org/rusticml/darkfi-intent-amm-proposal) — experimentation with intent-based AMM logic
- [zkas bincode](../zkas/bincode.md) — existing opcode specifications
- [Smart Contracts architecture](sc/sc.md) — contract layer built on zkVM
