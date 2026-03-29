# DarkFi Safemath: Safe ZK Arithmetic Templates

[darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) provides audited ZK template circuits for bounded integer relations in DarkFi circuits.

## Why Safemath?

DarkFi's zkVM operates in the Pallas field where field arithmetic wraps at `p`. Direct comparisons and divisions don't work the same as integer arithmetic. Safemath provides:

1. **Bounded range checks** — prevent field wraparound attacks
2. **Safe comparison gadgets** — avoid experimental opcode soundness issues
3. **Cross-multiplication patterns** — ratio checks without division

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
| Assert `a <= b` passes | ✅ `assert_lte_u64_v1.zk` | ⚠️ Experimental |
| Return 0/1 Boolean for public output | ❌ Not possible | ✅ Works but experimental |

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

## Source

- **Repository**: https://codeberg.org/rusticml/darkfi-safemath
- **Local copy**: `src/contract/safemath/`

## See Also

- [Experimental Opcodes](experimental-opcodes.md) — LessThanOrEqual soundness issues
- [Field Arithmetic Constraints](field_arithmetic.md) — Why field math differs from integer math
- [zkVM Primitive Layer](zkvm_primitives.md) — Opcode implementation details
- [Contract README](../../src/contract/README.md) — Circuit safety summary
