# zkVM Primitive Layer: Opcode Reasoning

> **Prerequisite reading**: Before this document, read [Field Arithmetic Constraints](field_arithmetic.md). It explains why every operation in a ZK circuit must be re-expressed in finite field arithmetic — and why that re-expression is the primary difficulty in ZK circuit design. The examples in this document assume you understand field vs. integer ordering and modular arithmetic.

The opcode layer is not an implementation detail — it is the **primitive layer** that
determines the entire expressiveness surface of DarkFi's smart contract system.

DarkFi's zkVM executes ZK circuits compiled from `.zk` source files. Every contract
— identity credentials, DEX atomic swaps, bridge deposits, stablecoin positions —
ultimately reduces to a sequence of opcodes. The available opcodes define the
mathematical and logical operations that contract authors can assume exist.

**What this means**: If an opcode is missing, every contract that would logically
need it must either work around it with complex compositions, leave the proof
incomplete, or simply not exist.

This is why reasoning about the opcode layer is a **core architectural discussion**,
not a peripheral one. The roadmap of what DarkFi's contracts can express is
fundamentally constrained by — and derivable from — the opcode set.

## Why Opcode Reasoning Belongs on the Roadmap

DarkFi's core team is correctly focused on core consensus, protocol security, and
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

Consider features explicitly discussed in DarkFi's public communications:

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

DarkFi's zkVM operates in the Pallas field — the scalar field of bn254, with prime order:

```
p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1
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

### How `LessThanOrEqual` Works: Combining Equality and Ordering

`LessThanOrEqual(a, b)` returns `1` if `a ≤ b`, `0` otherwise. This can be decomposed as:

```
LessThanOrEqual(a, b) = IsEqual(a, b) OR LessThanLoose(a, b)
```

In field arithmetic:
```
IsEqual(a, b) = 1 - less_than_loose(a, b) - less_than_loose(b, a)
```

The gadget computes both `a - b` and `b - a`, runs the range check on each, and combines the results. The output is a `Base` value (0 or 1) usable in subsequent computation — unlike the `constrain`-only versions.

---

### `IsEqualBase`: The Delta-Invert Approach

`IsEqualBase(a, b)` returns `1` if `a == b`, `0` otherwise. The gadget computes `delta = a - b` and its field inverse `delta_invert = delta^(-1)`:

```
delta = base_sub(a, b)
delta_invert = field_inverse(delta)

# Constraint: delta * delta_invert == 1  (when delta != 0)
# When a == b: delta = 0, and delta_invert is set to 1 by convention
```

**The soundness issue**: When `a == b`, `delta = 0`. The constraint `delta * inv == 1` becomes `0 * 1 == 1`, which is unsatisfiable in a field. However, a selector gate makes this constraint only active when `a != b`. When `a == b`, only the second constraint `delta * inv + (out - 1) == 0` is evaluated, which becomes `0 + 0 == 0` — always satisfied, regardless of what `delta_invert` is assigned.

**Consequence**: A malicious prover can assign any value to `delta_invert` when `a == b` without detection. The fix would require an explicit `is_zero` gadget that correctly constrains `delta_invert` when `delta = 0`, rather than relying on the selector gate to disable the problematic constraint.

---

### Gate Soundness: LessThanOrEqual with Output

The `LessThanOrEqual` gate encodes the result as an offset value:

```zk
# Gate constraint:
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0  # out must be 0 or 1

# Where:
# - out = 1 means a <= b
# - out = 0 means a > b
```

`a_offset` is then range-checked to `[0, 2^253)`. The concern: a malicious prover could assign `out = 0` and `a_offset = a - b - 1` (where `a > b`), producing an `a_offset` value that satisfies both the gate constraint and the range check. This would incorrectly pass verification even though the comparison result was flipped.

The range check limits feasible incorrect assignments, but the interaction between the gate constraint and range check has not been formally analyzed. For production use, this deserves a proper security reduction.

---

### Why This Is Hard: Summary of Challenges

| Challenge | Why It's Hard |
|-----------|--------------|
| **Field vs integer ordering** | Near `p`, field wraparound inverts integer ordering; requires range constraints on inputs |
| **Delta-invert for IsEqualBase** | Cannot invert zero; convention-based fix creates a prover-controlled assignment with no constraint |
| **Gate soundness for LessThanOrEqual** | Prover controls witness assignment; range check limits but doesn't eliminate incorrect assignments |
| **No upgrade path** | Once deployed, buggy comparison results cannot be corrected without a hard fork |
| **Composability untested** | Chaining comparison outputs into `CondSelect` or other opcodes has not been tested |

---

### What This Means for Contract Authors

For most use cases — threshold predicates, collateralization bounds, reward limits — the current implementation is sufficient. The gadgets work correctly for honest provers and the range constraints keep inputs in the safe zone.

However, before using these opcodes in a production contract that guards significant value, consider:

1. **Input range validation**: Add explicit `range_check(253, a)` and `range_check(253, b)` before comparison to eliminate boundary cases
2. **Redundant checks**: For high-value operations, add a redundant `LessThanStrict` constraint alongside `LessThanOrEqual` as a sanity check
3. **Formal audit**: The delta-invert issue in `IsEqualBase` should be fixed with an explicit `is_zero` gadget before production deployment

**See also**: `src/zk/gadget/less_than.rs` for the actual Halo2 implementation, and `dao/exec.zk` for a working example of cross-multiplication ratio checks that avoid these comparison issues entirely.

---

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

**Comparison** (available, constrain-only):
- `LessThanStrict`, `LessThanLoose` — constrain but don't return
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

`LessThanOrEqual` (0x55), `IsEqualBase` (0x54), `NotBase` (0x56), `BaseLtStrict` (0x57), and `BaseDiv` (0x58) were initially developed on a [separate experimental branch](https://codeberg.org/rusticml/darkfi/commits/branch/less-than-or-equal-experiment) (by rusticml) and integrated into this repository at commit `41b0629e0`. `BaseModExp` remains unimplemented.

**Formal Verification Results** (Lean 4, see [Opcodes and Formal Verification](opcodes.md)):

| Opcode | Status | Notes |
|--------|--------|-------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Lean 4 exhaustive testing, no counterexamples |
| `IsEqualBase` (0x54) | ❌ Bug | delta_invert unconstrained when `a == b` |
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
NotBase ──────────┘    (used to compose boolean logic)
      │
      └──► Combined comparisons: a < b OR a == c
                      = LessThanLoose(a,b) + IsEqualBase(a,c)
```

## Safemath: Workaround vs Ideal Opcode

This section explains the relationship between **safemath templates** (external ZK circuit libraries) and **native VM opcodes** (built into the zkVM). Understanding this distinction is essential for contract authors making architectural decisions.

### The Core Tension: Opcode vs Template

When a circuit needs `LessThanOrEqual`, there are two approaches:

| Approach | What It Is | Example |
|----------|-----------|---------|
| **Native Opcode** | Single implementation in VM, all circuits reference it | `LessThanOrEqual(a, b)` built into zkVM |
| **Safemath Template** | ZK circuit gadget copy-pasted into every circuit that needs it | `assert_lte_u64_v1.zk` template from darkfi-safemath |

### Why Native Opcodes Are Ideal

A native opcode is **the right approach** for comparison operations:

1. **No circuit bloat**: One implementation in the VM, used by all circuits. The gadget constraints are in the verification key once, not replicated in every circuit.

2. **Proper composability**: The opcode output is a `Base` value that can feed into other opcodes (`CondSelect`, `BoolCheck`, `ConstrainEqualBase`). This is how ZK circuits compose — through values flowing between operations.

3. **Single audit point**: The opcode implementation is audited once. Every circuit that uses it benefits automatically.

4. **Efficient verification**: The verifier's work is proportional to the number of opcodes executed, not the size of copied gadget code.

```
Native opcode (ideal):
┌─────────────────────────────────────────────────────┐
│  zkVM                                                  │
│  ┌─────────────────────────────────────────────┐     │
│  │  LessThanOrEqual gate (ONE implementation)    │     │
│  └─────────────────────────────────────────────┘     │
│                         ▲                           │
│    ┌────────────────────┼────────────────────┐       │
│    │                    │                    │       │
│ Circuit A ──────────────│───────────── Circuit B      │
│ (references opcode)     │    (references opcode)     │
└─────────────────────────────────────────────────────┘

Safemath template (workaround):
┌─────────────────────────────────────────────────────┐
│  Circuit A                                           │
│  ┌─────────────────────────────────────────────┐     │
│  │  assert_lte_u64_v1.zk (COPY of gadget)      │     │
│  └─────────────────────────────────────────────┘     │
├─────────────────────────────────────────────────────┤
│  Circuit B                                           │
│  ┌─────────────────────────────────────────────┐     │
│  │  assert_lte_u64_v1.zk (ANOTHER COPY)        │     │
│  └─────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────┘
```

### Why Safemath Is the Current Workaround

When an opcode isn't available or isn't production-ready, safemath templates are the workaround:

1. **External template library**: [darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) provides pre-built ZK circuit templates for common operations

2. **Assertion gadgets**: Safemath templates only **constrain** — they don't return values. This is sufficient when you only need to assert a relation passes, not when you need the result for downstream logic.

3. **Bounded inputs required**: Safemath assumes inputs are within safe ranges (e.g., u64). Range checks must be explicit.

**The safemath pattern** (`assert_lte_u64_v1.zk`):
```zk
# Prove: lhs <= rhs  (assertion, no return value)
range_check(64, lhs);
range_check(64, rhs);
rhs_plus_one = base_add(rhs, witness_base(1));
less_than_strict(lhs, rhs_plus_one);  # Proves lhs < rhs + 1
```

### Benefits and Drawbacks: Full Analysis

#### Native Opcode: `LessThanOrEqual(a, b) → Base`

| Aspect | Status |
|--------|--------|
| Returns 0/1 Boolean | ✅ Yes — can feed into other logic |
| Composability | ✅ Full — output is Base, usable everywhere |
| Circuit bloat | ✅ None — single implementation in VM |
| Audit burden | ✅ Single audit point |
| Soundness risk | ⚠️ Gate soundness concern (unverified) |
| Production status | ⚠️ Grey-market (experimental) |

**Drawback**: Requires implementing and auditing a native opcode in the VM. If the implementation has a bug, all circuits using it are affected.

#### Safemath Template: `assert_lte_u64_v1.zk`

| Aspect | Status |
|--------|--------|
| Returns 0/1 Boolean | ❌ No — constrain-only |
| Composability | ❌ Limited — cannot feed result into other logic |
| Circuit bloat | ❌ Each circuit copies the gadget |
| Audit burden | ❌ Each circuit must be reviewed |
| Soundness risk | ✅ Low — uses sound `less_than_strict` |
| Production status | ✅ Production-ready (uses existing opcodes) |

**Drawback**: Cannot be used when a Boolean return value is needed for downstream logic (e.g., `constrain_equal_base(result, public_input)`).

### When to Use Which

| Use Case | Use Safemath? | Why |
|----------|--------------|-----|
| `assert x <= y` (internal constraint) | ✅ Yes | No return value needed |
| `if x <= y then A else B` | ❌ No | Need Boolean for `CondSelect` |
| Public output of comparison result | ❌ No | Safemath can't produce public value |
| Bounded ratio check `a/b <= c/d` | ✅ Yes | Cross-multiplication works |
| Collateralization `debt <= collateral/ratio` | ✅ Yes | Assertion only |

### The Identity Contract: Why Safemath Required a Semantic Change

The identity contract's `create_claim_v1.zk` originally used `LessThanOrEqual` to return a Boolean constrained to a public input `predicate_result`:

```zk
# BEFORE (Level 1 selective disclosure):
is_authorized = less_than_or_equal(threshold, attribute_value);
constrain_equal_base(is_authorized, predicate_result);  # Public!
# Verifier learns: is_authorized == predicate_result
```

Using safemath (assertion only, no return value) required removing the public output:

```zk
# AFTER (Level 0 zk_only):
attribute_plus_one = base_add(attribute_value, witness_base(1));
less_than_strict(threshold, attribute_plus_one);  # Assert only
# Verifier learns: proof is valid or invalid
```

**This is a semantic change**: Level 1 reveals the predicate result publicly; Level 0 reveals only proof validity. The privacy properties changed.

**The lesson**: Safemath can only replace `LessThanOrEqual` when you don't need a Boolean return value. If your circuit's logic requires the comparison result as a value (not just as an assertion that passes), you need the native opcode.

### The Path Forward: Ideal Opcodes as Foundation

The proper long-term approach is to implement comparison opcodes **correctly and formally verified** in the zkVM:

1. **Fix `IsEqualBase` delta-invert**: Replace the selector-gate workaround with an explicit `is_zero` gadget

Note: `LessThanOrEqual` soundness has been **verified** via Lean 4 exhaustive testing.

3. **Single audit, universal benefit**: Once the opcodes are correct, every circuit using them is automatically secure

4. **Composability unlocked**: With return-value opcodes, circuits can compose comparison results into complex logic:
   ```zk
   # With native LessThanOrEqual:
   authorized = less_than_or_equal(balance, minimum);
   time_locked = less_than_or_equal(current_time, unlock_time);
   can_withdraw = and(authorized, not(time_locked));
   result = cond_select(can_withdraw, withdrawal_amount, 0);
   constrain_instance(result);
   ```

### Summary: Workaround vs Ideal

| | Safemath (Workaround) | Native Opcode (Ideal) |
|---|---|---|
| **Bloat** | Each circuit copies gadget | Single VM implementation |
| **Returns value** | ❌ No | ✅ Yes |
| **Composability** | ❌ Limited to assertions | ✅ Full |
| **Soundness** | ✅ Uses sound `less_than_strict` | ⚠️ Needs formal verification |
| **Audit** | Per-circuit | Single opcode audit |
| **Use when** | Assertion only needed | Boolean return needed |

**Recommendation**: Use safemath for assertions (collateralization checks, bounds checks). Implement native opcodes when you need return values for composability. The goal should always be native opcodes as the foundation — safemath is the workaround for when they're not available.

**See also**:
- [Safemath](../safemath.md) — integration guide for darkfi-safemath templates
- [Opcodes Reference](opcodes.md) — LessThanOrEqual and BaseDiv verification

---

## Cross-Contract ZK Composition: The Trusted Setup Workaround

This section documents a critical workaround that appears in contracts requiring
cross-contract state verification, specifically the DEX contract's integration with
the Money contract.

### The Problem: Verifying State Across Contract Boundaries

When the DEX contract needs to verify that a user has locked funds in the Money
contract, the ideal solution is **cross-contract ZK composition**:

```
Ideal (not yet possible):
┌─────────────────────────────────────────────────────────────────┐
│  DEX Contract                                                     │
│                                                                    │
│  1. Call Money contract's ZK circuit                            │
│  2. Verify lock_proof is valid                                   │
│  3. Money contract's state is verified without duplication       │
│                                                                    │
│  Problem: No cross-contract ZK composition opcodes exist         │
└─────────────────────────────────────────────────────────────────┘
```

The DEX needs to verify that `lock_proof` (a Merkle proof showing funds are locked
in the Money contract) is valid. Without cross-contract ZK composition, the DEX
cannot directly verify the Money contract's state.

### The Trusted Setup Workaround

The current implementation uses a **trusted setup pattern**:

```rust
// During DEX initialization:
pub struct InitializeParams {
    /// Trusted Merkle root of the money contract's coin tree
    pub trusted_money_merkle_root: [u8; 32],
}

// When creating/accepting a swap:
fn verify_lock_proof(
    config_db: u32,
    lock_commitment: &[u8; 32],
    lock_proof: &[[u8; 32]],
) -> Result<(), ContractError> {
    // Get trusted Merkle root from config
    let trusted_root = get_trusted_root(config_db)?;

    // Recompute Merkle root from lock_proof
    let computed_root = compute_merkle_root(lock_commitment, lock_proof)?;

    // Compare against trusted root
    if computed_root != trusted_root {
        return Err(DexError::InvalidMerkleProof.into());
    }
    Ok(())
}
```

### Security Trade-offs

This trusted setup is a **significant security trade-off**:

| Aspect | Status | Implication |
|--------|--------|-------------|
| **Trusted root correctness** | User-provided at initialization | If wrong, invalid lock_proofs accepted |
| **Root synchronization** | Manual update required | If money contract updates its Merkle root, DEX becomes stale |
| **No on-chain verification** | Root comparison only | Cannot detect if money contract state changed |
| **Double-spend prevention** | Nullifiers still enforced | Protects against double-spending within DEX |

### Why This Is Temporary

The trusted setup is explicitly a **workaround** until proper cross-contract ZK
composition opcodes exist. The proper solution requires:

| Requirement | Description | Status |
|-------------|-------------|--------|
| **Cross-contract ZK composition opcodes** | VM can call and verify proofs from other contracts | Not implemented |
| **On-chain Merkle root verification** | Contract can verify Merkle root against consensus state | Not implemented |
| **Event-based state synchronization** | Contracts receive notifications when state changes | Not implemented |

### Trusted Setup in the DEX

The DEX contract uses this pattern in two places:

1. **CreateSwapV1**: Verifies proposer's lock_proof against trusted Merkle root
2. **AcceptSwapV1**: Verifies acceptor's lock_proof against trusted Merkle root

```rust
// In create_swap_v1.rs and accept_swap_v1.rs:
// WARNING: This is a TRUSTED SETUP workaround
// Proper implementation requires cross-contract ZK composition
let config_db = wasm::db::db_lookup(cid, DEX_CONTRACT_CONFIG_TREE)?;
verify_lock_proof(config_db, &params.lock_commitment, &params.lock_proof)?;
```

### DEX-Money Contract Integration Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    DEX + Money Contract Integration                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. Initialize DEX contract                                                 │
│     └─► Set trusted_money_merkle_root from current Money contract state     │
│                                                                              │
│  2. User locks funds in Money contract                                      │
│     └─► Money contract updates its Merkle tree                             │
│     └─► User receives lock_commitment + lock_proof                         │
│                                                                              │
│  3. User creates swap in DEX                                               │
│     └─► DEX verifies lock_proof against trusted root                       │
│     └─► WARNING: If Money's Merkle root changed, verification may fail     │
│                                                                              │
│  4. User accepts swap in DEX                                               │
│     └─► Same verification against trusted root                             │
│                                                                              │
│  5. Execute swap                                                            │
│     └─► Both parties' locks verified via nullifier tracking                │
│     └─► Funds transferred via Money contract                               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Security Notes for Contract Authors

If you are building a contract that requires cross-contract state verification:

1. **Document the trust assumption**: Explicitly state that a trusted Merkle root
   is used and why

2. **Warn about staleness**: If the money contract updates its Merkle root, the
   DEX's trusted root becomes stale and swaps may fail

3. **Prefer nullifiers over commitments**: Store nullifiers (not commitments) in
   participants_db for double-spend prevention

4. **Plan for migration**: When cross-contract ZK composition opcodes are available,
   remove the trusted setup and implement proper verification

### Long-Term Solution: Cross-Contract ZK Composition Opcodes

The proper solution requires new opcodes that enable circuits to call other circuits:

```zk
# Hypothetical cross-contract ZK composition:
cross_contract_call(
    target_contract: Money,
    function: "verify_lock",
    inputs: [lock_commitment, lock_proof],
    proof: zk_proof
) -> Base  # Returns 1 if verified
```

This would allow the DEX to:
1. Call the Money contract's verification circuit
2. Pass the lock_proof as input
3. Receive verification result without duplicating the circuit

### Related Documentation

- [DEX Contract](../../src/contract/dex/) — Implementation of trusted setup
- [Money Contract](../../src/contract/money/) — Source of truth for locked funds
- [O-Cap & Composable Privacy](ocap.md) — Cross-contract patterns and authorization primitives

---

## Adding Custom Opcodes

The zkVM opcode system is designed to be extensible. To add a new opcode:

### Step 1: Define in `src/zkas/opcode.rs`

```rust
define_opcodes! {
    Noop = 0x00, "noop", (), ();

    // ... existing opcodes ...

    // Add new opcode at first available slot after 0x52
    LessThanOrEqual = 0x53, "less_than_or_equal",
        (VarType::Base), (VarType::Base, VarType::Base);
}
```

### Step 2: Implement in `src/zk/vm.rs`

```rust
Opcode::LessThanOrEqual => {
    let a = stack.pop_base()?;
    let b = stack.pop_base()?;
    // result = is_equal(a, b) + less_than_loose(a, b)
    let is_eq = if a == b { F::one() } else { F::zero() };
    let is_lt = // Halo2 less_than gadget
    stack.push(is_eq + is_lt);
}
```

### Step 3: Use in circuits

```zk
circuit "MyContract" {
    # After LessThanOrEqual is implemented:
    authorized = less_than_or_equal(minimum_balance, user_balance);
    constrain_equal_base(authorized, claimed_authorization);
}
```

## References

- [Field Arithmetic Constraints](field_arithmetic.md) — foundational reading for understanding why ZK circuit arithmetic is hard
- [O-Cap & Composable Privacy](ocap.md) — the authorization pattern these opcodes enable and how they compose across contracts
- [intent-amm fork (rusticml)](https://codeberg.org/rusticml/darkfi-intent-amm-proposal) — experimentation with intent-based AMM logic
- [zkas bincode](../zkas/bincode.md) — existing opcode specifications
- [Smart Contracts architecture](sc/sc.md) — contract layer built on zkVM
