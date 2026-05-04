# DarkFi Safemath: Safe ZK Arithmetic Templates

> **DEPRECATED**: `LessThanOrEqual` (0x55) is now **verified sound** and `BaseDiv` (0x58) is **implemented**.
> Safemath workarounds are no longer necessary for sound ZK operations.
> This document is retained for historical reference and for the assertion-only pattern which remains useful.
>
> See [Opcodes Reference](opcodes.md).

---

[darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) provides audited ZK template circuits for bounded integer relations in DarkFi circuits.

## Why Safemath? (Legacy)

> **Note**: With `LessThanOrEqual` now verified sound, safemath is **optional**. It remains useful for:
> - Assertion-only patterns (no Boolean return needed)
> - Bounded comparisons where you don't need the return value

DarkFi's zkVM operates in the Pallas field where field arithmetic wraps at `p`. Direct comparisons and divisions don't work the same as integer arithmetic. Safemath provides:

1. **Bounded range checks** — prevent field wraparound attacks
2. **Safe comparison gadgets** — useful for assertion-only comparisons
3. **Cross-multiplication patterns** — ratio checks without division (still useful even with BaseDiv available)

## Quick Reference

| Template | Proves | Bounded Inputs |
|----------|--------|----------------|
| `assert_lte_u64_v1.zk` | `lhs <= rhs` | Both u64 |
| `assert_lt_u64_v1.zk` | `lhs < rhs` | Both u64 |
| `assert_u64_v1.zk` | Value is u64 | Single value |
| `cross_mul_lte_u64_v1.zk` | `lhs_num/lhs_den <= rhs_num/rhs_den` | All u64 |
| `cross_mul_gte_u64_v1.zk` | `lhs_num/lhs_den >= rhs_num/rhs_den` | All u64 |
| `div_floor_u128_by_u64_to_u64_v1.zk` | `floor(a/b) = result` | a: u128, b: u64 |
| `sqrt_floor_u128_v1.zk` | `floor(sqrt(x)) = result` | x: u128 |

## The Core Pattern: LTE via Addition

The key insight is that `a <= b` can be expressed as `a < b + 1`:

```zk
# From assert_lte_u64_v1.zk
range_check(64, lhs);
range_check(64, rhs);
rhs_plus_one = base_add(rhs, witness_base(1));
less_than_strict(lhs, rhs_plus_one);  # Passes if lhs < rhs + 1
```

**Why this works**:
- `less_than_strict` is constrain-only (no return value) — provers can't manipulate
- Bounded inputs (u64) prevent field wraparound
- Adding 1 converts strict `<` to non-strict `<=`

## Integration Guide

### 1. Replace LessThanOrEqual with Assertion

**Before** (experimental opcode):
```zk
is_collateralized = less_than_or_equal(two_times_debt, collateral_amount);
constrain_equal_base(is_collateralized, witness_base(1));
```

**After** (safemath):
```zk
range_check(64, two_times_debt);
range_check(64, collateral_amount);
collateral_plus_one = base_add(collateral_amount, witness_base(1));
less_than_strict(two_times_debt, collateral_plus_one);
```

### 2. Use Cross-Multiplication for Ratios

**Proving**: `collateral/debt >= min_ratio` (200% collateralization)

```zk
# Proving: lhs_num/lhs_den >= rhs_num/rhs_den
# I.e., lhs_num * rhs_den >= rhs_num * lhs_den

lhs_cross = base_mul(lhs_num, rhs_den);
rhs_cross = base_mul(rhs_num, lhs_den);
rhs_cross_plus_one = base_add(rhs_cross, witness_base(1));
less_than_strict(rhs_cross_plus_one, lhs_cross);
```

## Key Distinction: Assertion vs Boolean

Safemath templates are **assertion gadgets** — they constrain but don't return values:

| Use Case | Safemath Template | LessThanOrEqual Opcode |
|----------|------------------|----------------------|
| Assert `a <= b` passes | ✅ `assert_lte_u64_v1.zk` | ✅ Verified Sound |
| Return 0/1 Boolean for public output | ❌ Not possible | ✅ Verified Sound |

**Why the distinction matters**:

```zk
# Identity contract (needs Boolean output):
is_authorized = less_than_or_equal(threshold, attribute_value);
constrain_equal_base(is_authorized, predicate_result);  # Public output!
# This REVEALS authorization decision — Level 1 selective disclosure
# Cannot use safemath without changing semantics

# Stablecoin (assertion only):
less_than_strict(two_times_debt, collateral_plus_one);  # Pass/fail only
# No Boolean returned — internal constraint only
# Can use safemath safely
```

### Bounded Equation Construction (Alternative Pattern)

There's an alternative approach that **returns a public 0/1 bit without LessThanOrEqual**:

```zk
# Prove: threshold <= attribute_value, return public bit
#
# Construction:
#   threshold + delta = attribute_value + (1 - result) * 2^64
#
# With constraints:
range_check(64, threshold);
range_check(64, attribute_value);
range_check(64, delta);
bool_check(predicate_result);
base_mul_small((1 - predicate_result), 2^64);  # Large constant

# Interpretation:
# - If predicate_result = 1: equation becomes threshold + delta = attribute_value
#   This is solvable iff threshold <= attribute_value
# - If predicate_result = 0: equation becomes threshold + delta = attribute_value + 2^64
#   This is solvable iff threshold > attribute_value
```

**Why this works**:
- Both values are bounded to u64, so arithmetic is well-defined
- The equation collapses to solvable/unsolvable depending on the bit
- The prover MUST set predicate_result correctly to satisfy constraints
- No experimental opcode needed — uses only: `range_check`, `bool_check`, `base_add`, `base_mul`, `constrain_equal_base`

**This preserves Level 1 semantics** — predicate_result is a public output bit.

**Key limitation**: Only works for u64-bounded comparisons, not arbitrary field ordering.

## Safemath in the Stack

```
┌─────────────────────────────────────────────────────────┐
│                    DarkFi Contracts                     │
│  (stablecoin, dao, escrow, money, bridge, etc.)           │
└─────────────────────┬───────────────────────────────────┘
                      │ Uses
                      ▼
┌─────────────────────────────────────────────────────────┐
│              darkfi-safemath-zk (this crate)            │
│  src/contract/safemath/templates/safemath/              │
│  - assert_lte_u64_v1.zk                                  │
│  - cross_mul_lte_u64_v1.zk                               │
│  - div_floor, sqrt_floor, etc.                          │
└─────────────────────┬───────────────────────────────────┘
                      │ Compiled by
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     zkVM / zkas                         │
│  - less_than_strict (sound, constrain-only)              │
│  - base_add, base_mul (field arithmetic)                  │
│  - range_check (bounded inputs)                          │
└─────────────────────────────────────────────────────────┘
```

## When You Don't Need Safemath

- **DAO ratio checks**: Already use cross-multiplication in `exec.zk`
- **Merkle proofs**: Use `merkle_root` opcode directly
- **Range checks**: Use `range_check(64, value)` directly
- **Boolean constraints**: Use `bool_check(value)` + `constrain_equal_base`

## Real-World Use Cases

### DEX Partial Fills

The DEX contract uses safemath for partial fill checks:

```zk
# From src/contract/dex/proof/execute_swap_v1.zk
#
# Partial fill: prove fill_amount <= alice_amount
# This prevents the filler from exceeding Alice's offered amount

range_check(64, alice_amount);
range_check(64, bob_amount);
range_check(64, fill_amount);

# Safemath: fill < alice + 1  ⟺  fill <= alice
ONE = witness_base(1);
alice_amount_plus_one = base_add(alice_amount, ONE);
less_than_strict(fill_amount, alice_amount_plus_one);

# Also verify: bob_amount >= fill_amount (Bob receives the fill)
less_than_strict(fill_amount, bob_amount);
```

**Why safemath for DEX**:
- Partial fill only needs assertion (fill must not exceed Alice's offer)
- No Boolean return needed for further constraints
- Safemath pattern is still valid for assertion-only use cases

**LessThanOrEqual is now verified sound**, so either approach works:
- LessThanOrEqual: Returns Boolean (useful for composability)
- Safemath: Assertion-only (sufficient for DEX partial fill checks)

### Stablecoin Collateralization

```zk
# From src/contract/stablecoin/proof/open_position_v1.zk
#
# Prove: 2 * debt <= collateral (200% collateralization)

range_check(64, two_times_debt);
range_check(64, collateral_amount);
ONE = witness_base(1);
collateral_plus_one = base_add(collateral_amount, ONE);
less_than_strict(two_times_debt, collateral_plus_one);
```

### Identity Threshold (Assertion-Only)

```zk
# From src/contract/identity/proof/create_claim_v1.zk
#
# Prove: threshold <= attribute_value (credential threshold met)
# Uses safemath assertion (no public bit returned)

ONE = witness_base(1);
attribute_plus_one = base_add(attribute_value, ONE);
less_than_strict(threshold, attribute_plus_one);
```

### Identity Threshold (Bounded Equation - Returns Public Bit)

For Level 1 selective disclosure where a public predicate_result bit is needed:

```zk
# Bounded equation construction returning public bit:
#   threshold + delta = attribute_value + (1 - result) * 2^64

range_check(64, threshold);
range_check(64, attribute_value);
range_check(64, delta);
bool_check(predicate_result);
# Prover sets predicate_result to 1 if threshold <= attribute_value
```

This preserves Level 1 semantics using an alternative construction (LessThanOrEqual is now verified sound and can also be used).

## Source

- **Repository**: https://codeberg.org/rusticml/darkfi-safemath
- **Local copy**: `src/contract/safemath/`

## See Also

- [Opcodes Reference](opcodes.md) — LessThanOrEqual and BaseDiv verification
- [Field Arithmetic Constraints](field_arithmetic.md) — Why field math differs from integer math
- [zkVM Primitive Layer](zkvm_primitives.md) — Opcode implementation details
- [Contract README](../../src/contract/README.md) — Circuit safety summary
