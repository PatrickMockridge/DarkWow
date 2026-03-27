# Experimental Opcodes: Reference for Contract Authors

> **TL;DR**: `LessThanOrEqual`, `IsEqualBase`, `NotBase`, and `BaseLtStrict` are implemented but **not production-ready**. Use `less_than_strict` (constrain-only) or cross-multiplication for ratio checks instead.

---

## Opcode Status Summary

| Opcode | Returns Value | Soundness Status | Production Ready |
|--------|--------------|------------------|------------------|
| `LessThanOrEqual` (0x55) | Yes | **Gate soundness issue** | No |
| `IsEqualBase` (0x54) | Yes | **Delta-invert issue when `a == b`** | No |
| `NotBase` (0x56) | Yes | Unused, unverified | No |
| `BaseLtStrict` (0x57) | Yes | Unused, unverified | No |
| `LessThanStrict` | No (constrain-only) | **Sound** | Yes |
| `LessThanLoose` | No (constrain-only) | **Sound** | Yes |

---

## The Core Problem: Field Elements vs Integers

DarkFi's zkVM operates in the Pallas field (prime order `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`).

**Field arithmetic and integer arithmetic are NOT the same** near `p`:

```
Integer:  0 < 1 < 2 < ... < p-2 < p-1
Field:    0 ≡ p < 1 < 2 < ... < p-2 < p-1 ≡ -1  (mod p)
```

For most values (well below `p`), ordering agrees. But for values in `[p - 2^32, p)`, field wraparound causes `a_f > b_f` even when `a < b` as integers.

---

## IsEqualBase: Delta-Invert Soundness (CRITICAL)

**What it does**: Returns `1` if `a == b`, `0` otherwise.

**Implementation**:
```zk
delta = base_sub(a, b)
delta_invert = field_inverse(delta)
# Constraint: delta * delta_invert == 1  (when delta != 0)
```

**The bug**: When `a == b`, `delta = 0`. The constraint becomes `0 * 1 == 1`, which is unsatisfiable. A selector gate disables this constraint when `a == b`, but:

> **A malicious prover can assign ANY value to `delta_invert` when `a == b` without detection.**

The selector gate turns off the problematic constraint, leaving only `delta * delta_invert + (out - 1) == 0`, which becomes `0 + 0 == 0` — always satisfied.

**Fix needed**: An explicit `is_zero` gadget that correctly constrains `delta_invert` when `delta = 0`.

---

## LessThanOrEqual: Gate Soundness (CRITICAL)

**What it does**: Returns `1` if `a ≤ b`, `0` otherwise.

**Implementation**:
```zk
# Gate constraint:
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0  # out must be 0 or 1

# Where:
# - out = 1 means a <= b
# - out = 0 means a > b

# a_offset is then range-checked to [0, 2^253)
```

**The bug**: A malicious prover could assign `out = 0` and `a_offset = a - b - 1` (where `a > b`), producing an `a_offset` value that satisfies both the gate constraint and the range check. This would incorrectly pass verification.

> **The range check limits feasible incorrect assignments, but the interaction between gate constraint and range check has not been formally analyzed.**

**Fix needed**: Formal security reduction or a redesigned gadget.

---

## Safe Alternatives

### For Constrain-Only Comparisons: Use `less_than_strict`

```zk
# Proves: a < b (fails circuit if a >= b)
# Returns nothing - just constrains
less_than_strict(value, limit);
```

This is **sound** because it only enforces a constraint, returning no value a prover could manipulate.

### For Ratio Checks: Use Cross-Multiplication

```zk
# To prove: collateral/debt < threshold
# WITHOUT division - use cross-multiplication:

lhs = base_mul(collateral_value, 1);
rhs = base_mul(debt_value, threshold);
# If we want <= , add 1 to rhs to convert strict < to <=
rhs_1 = base_add(rhs, 1);
less_than_strict(lhs, rhs_1);  # Passes if lhs < rhs + 1
```

See `dao/exec.zk` lines 118-126 for the exact pattern.

---

## When You CAN Use Experimental Opcodes

For **experimental/skeleton code** where:

1. The circuit guards no significant value
2. You explicitly document the soundness concern
3. An honest prover assumption is acceptable for now
4. You have a path to replacement with a sound opcode

**Always document**:
```zk
# NOTE: LessThanOrEqual is experimental (gate soundness issue)
# See doc/src/arch/experimental-opcodes.md
# DO NOT use in production without formal verification
```

---

## Contracts Using Experimental Opcodes

| Contract | Circuit | Opcode Used | Status |
|----------|---------|-------------|--------|
| `identity` | `create_claim_v1.zk` | `LessThanOrEqual` | ⚠️ Experimental |
| `stablecoin` | `open_position_v1.zk` | `LessThanOrEqual` | ⚠️ Experimental |
| `stablecoin` | `liquidate_v1.zk` | `LessThanOrEqual` | ⚠️ Experimental |

**These cannot ship to production** until LessThanOrEqual is formally verified or replaced.

---

## Contracts Using ONLY Proven Opcodes

| Contract | Circuit | Status |
|----------|---------|--------|
| `money` | `burn_v1.zk` | ✅ Production-ready |
| `money` | `fee_v1.zk` | ✅ Production-ready |
| `dao` | `exec.zk` | ✅ Production-ready |
| `dao` | `propose-main.zk` | ✅ Production-ready |
| `escrow` | `refund_v1.zk` | ✅ Production-ready |
| `dao_escrow` | `init_v1.zk` | ✅ Production-ready |
| `dao_escrow` | `pay_premium_v1.zk` | ✅ Production-ready |
| `bridge` | `deposit_v1.zk` | ✅ Production-ready |
| `bridge` | `withdraw_v1.zk` | ✅ Production-ready |

---

## Recommendations for Contract Authors

1. **Default to safe opcodes**: `less_than_strict`, `constrain_equal_base`, cross-multiplication
2. **Document experimental opcode usage**: Include a note explaining the concern
3. **Plan for replacement**: Have a path to sound alternatives before production
4. **Validate input ranges**: Add `range_check(253, a)` before comparisons to eliminate boundary cases
5. **Add redundant checks for high-value operations**: e.g., both `LessThanStrict` and `LessThanOrEqual` as sanity check

---

## See Also

- [zkVM Primitive Layer](zkvm_primitives.md) — Deep dive into opcode implementation
- [Field Arithmetic Constraints](field_arithmetic.md) — Why field math matters
- `dao/exec.zk` — Cross-multiplication pattern example
- `src/zk/gadget/less_than.rs` — Halo2 implementation
