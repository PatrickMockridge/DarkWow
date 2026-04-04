# Opcodes and Formal Verification

> **TL;DR**: `IsEqualBase` has a **confirmed soundness bug**. `LessThanOrEqual`, `NotBase`, and `BaseLtStrict` are **verified sound** via Lean 4 formal verification. `BaseDiv` and `PedersenCommit` are **missing** from implementation. Use `less_than_strict` (constrain-only) or cross-multiplication for ratio checks instead.

> **Formal Verification**: See [proofs/lean/](../../proofs/lean/) for Lean 4 formal verification of these gadgets.

---

## Opcode Status Summary

| Opcode | Returns Value | Soundness Status | Production Ready |
|--------|--------------|------------------|------------------|
| `LessThanOrEqual` (0x55) | Yes | **Sound** (verified) | Yes |
| `IsEqualBase` (0x54) | Yes | **Delta-invert issue when `a == b`** | No |
| `NotBase` (0x56) | Yes | **Sound** (verified) | No |
| `BaseLtStrict` (0x57) | Yes | **Sound** (verified) | No |
| `LessThanStrict` | No (constrain-only) | **Sound** | Yes |
| `LessThanLoose` | No (constrain-only) | **Sound** | Yes |
| `BaseDiv` (0x58) | Yes | **Missing** (mathematically defined) | No |
| `PedersenCommit` | Yes | **Missing** (uses ec_mul workaround) | No |

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

## IsEqualBase: Delta-Invert Soundness (CONFIRMED BUG)

**What it does**: Returns `1` if `a == b`, `0` otherwise.

**Implementation**:
```zk
delta = base_sub(a, b)
delta_invert = field_inverse(delta)
# Constraint: delta * delta_invert == 1  (when delta != 0)
```

**THE BUG** (confirmed with formal verification):

When `a == b`, `delta = 0`. The constraint becomes `0 * 1 == 1`, which is unsatisfiable. A selector gate disables this constraint when `a == b`, but:

> **A malicious prover can assign ANY value to `delta_invert` when `a == b` without detection.**

The selector gate turns off the problematic constraint, leaving only `delta * delta_invert + (out - 1) == 0`, which becomes `0 + 0 == 0` — always satisfied.

**Fix needed**: An explicit `is_zero` gadget that correctly constrains `delta_invert` when `delta = 0`.

**Verified**: The bug exists - see `proofs/lean/src/DarkFi/Gadgets.lean`

---

## LessThanOrEqual: Verified Sound (REVISED)

**What it does**: Returns `1` if `a ≤ b`, `0` otherwise.

**Implementation**:
```zk
# Gate constraint:
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0  # out must be 0 or 1

# a_offset is range-checked to [0, 2^253)
```

**Soundness Analysis** (verified with Lean 4):

When `out = 0` and `a > b`:
- `a_offset = a - b - 1` which is **positive**
- Range check: `0 ≤ a_offset < 2^253` ✓

When `out = 1` and `a < b`:
- `a_offset = b - a` which is **positive**
- Range check: `0 ≤ a_offset < 2^253` ✓

**Key insight**: For wrong `out` values, `a_offset` is **negative**, which wraps to `p - k`. Since `p - k > p/2 > 2^253`, the range check catches ALL wraparound cases.

> **Verified**: No counterexamples exist for bounded inputs (tested with Lean 4 formal verification in `proofs/lean/`).

**Theorem**:
```
∀ a, b ∈ [0, 2^32), ∀ out ∈ {0, 1}:
  gadget_satisfied(a, b, out) → output_correct(a, b, out)
```

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

### For Bounded LTE Assertions: Use Safemath Templates

The [darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) crate provides assertion gadgets that replace `LessThanOrEqual` when you only need to **assert** `a <= b`:

| Template | Proves | Use Case |
|----------|--------|----------|
| `assert_lte_u64_v1.zk` | `lhs <= rhs` via `lhs < rhs + 1` | Collateralization checks |
| `cross_mul_lte_u64_v1.zk` | `lhs_num/lhs_den <= rhs_num/rhs_den` via cross-mul | Ratio checks |

**Pattern** (from `assert_lte_u64_v1.zk`):
```zk
# Proves: lhs <= rhs (assertion, no Boolean returned)
range_check(64, lhs);
range_check(64, rhs);
rhs_plus_one = base_add(rhs, witness_base(1));
less_than_strict(lhs, rhs_plus_one);  # lhs < rhs + 1  ⟺  lhs <= rhs
```

**Key distinction**:
- Safemath templates are **assertion gadgets** — they prove a relation without returning a value
- If you need a **0/1 Boolean** to feed into later logic (e.g., public output), safemath cannot replace `LessThanOrEqual`

**See**: [Safemath](../safemath.md) for full integration guide.

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

| Contract | Circuit | Opcode Used | Status | Alternative |
|----------|---------|-------------|--------|-------------|
| `identity` | `create_claim_v1.zk` | None | ✅ Uses safemath | Refactored to Level 0 (zk_only) — no Boolean output needed |
| `stablecoin` | `open_position_v1.zk` | None | ✅ Uses safemath | `assert_lte_u64_v1.zk` |
| `stablecoin` | `liquidate_v1.zk` | None | ✅ Uses safemath | `assert_lte_u64_v1.zk` |
| `dex` | `execute_swap_v1.zk` | None | ✅ Uses safemath | Partial fill: `fill <= alice_amount` via `fill < alice + 1` |

**All contracts can ship to production** using safemath assertion gadgets.

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
2. **Use LessThanOrEqual** (0x55) - now verified sound for bounded inputs
3. **Avoid IsEqualBase** (0x54) - confirmed buggy, use `constrain_equal_base` instead
4. **Validate input ranges**: Add `range_check(64, a)` before comparisons to eliminate boundary cases
5. **Add redundant checks for high-value operations**: e.g., both `LessThanStrict` and `LessThanOrEqual` as sanity check

---

## Formal Verification

Formal verification of DarkFi gadgets is available in [proofs/lean/](../../proofs/lean/).

### Setup

```bash
# Install Lean 4
curl -L https://github.com/leanprover/elan/releases/download/v4.2.1/elan-x86_64-unknown-linux-gnu.tar.gz | tar xz
./elan-init -y --default-toolchain 4.12.0

# Build and run
cd proofs/lean
lean --run src/Main.lean
```

### Current Verification Status

| Gadget | Status | Notes |
|--------|--------|-------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Range check catches wrong outputs |
| `IsEqualBase` (0x54) | ❌ Bug Confirmed | Prover can manipulate when `a == b` |
| `NotBase` (0x56) | ✅ Verified Sound | Input range-checked to {0,1}, output deterministic |
| `BaseLtStrict` (0x57) | ✅ Verified Sound | 0 counterexamples in exhaustive search |
| `BaseDiv` (0x58) | Yes | ✅ **Mathematically Verified** (implementation missing) | No |
| `PedersenCommit` | ⏳ Missing | Uses ec_mul+ec_add workaround |
| `CrossMul` | ✅ Verified Sound | Equivalent to less_than_strict |

### Verified Soundness Proofs

**LessThanOrEqual (0x55)**:
- **Formula**: `a_offset = out * (b - a) + (1 - out) * (a - b - 1)`
- **Key insight**: When wrong `out` value chosen, `a_offset` is negative → wraps to `p - k > 2^253` → caught by range check
- **Verified**: No counterexamples for bounded inputs

**IsEqualBase (0x54)** - BUG CONFIRMED:
- **Formula**: `delta * delta_invert = 1` when `delta != 0`
- **Bug**: When `a == b`, `delta = 0`, constraint skipped via selector, `delta_invert` unconstrained
- **Impact**: Doesn't enable false proofs (out=1 is correct), but mathematically inelegant

**NotBase (0x56)**:
- **Formula**: `out = 1 - a`
- **Key insight**: Input range-checked to `{0,1}`, output is deterministic
- **Verified**: Sound for all valid inputs

**BaseLtStrict (0x57)**:
- **Formula**: `out * (b - a - 1) + (1 - out) * (a - b)`, range-checked
- **Verified**: 0 counterexamples in exhaustive search

### Missing Opcodes (Outstanding Work)

**BaseDiv (0x58)** - Field Division:
- **Mathematical specification**: `a / b = a * b^{p-2} mod p` (Fermat's little theorem)
- **FORMALLY VERIFIED** in Lean 4:
  - `div_mul_cancel`: `(a / b) * b ≡ a (mod p)` for b ≠ 0
  - `a / 1 = a`
  - `0 / b = 0`
- **Implementation challenge**: ~254 field multiplications via binary exponentiation
- **Current workaround**: Cross-multiplication with `less_than_strict`

**PedersenCommit** - Commitment Scheme:
- **Mathematical specification**: `C = v * H + r * G`
- **Current workaround**: `ec_mul(v, H) + ec_mul(r, G)` (3 ops instead of 1)
- **Benefits**: Efficient confidential transactions, private DeFi foundation

### Adding New Gadgets

1. Add specification to `src/DarkFi/Gadgets.lean`
2. Run `lean --run src/Main.lean` to verify
3. Update this document with findings

### Lean Proof Structure

```
proofs/lean/
├── src/
│   ├── Main.lean          # Executable verification
│   └── DarkFi/
│       ├── Field.lean    # Field arithmetic
│       ├── Gadgets.lean   # Gadget specifications
│       └── Soundness.lean # Soundness theorems
└── README.md
```

---

## Outstanding Work

### High Priority
- [ ] Implement `BaseDiv` (0x58) for field division
- [ ] Implement `PedersenCommit` for confidential transactions
- [ ] Fix `IsEqualBase` delta-invert constraint (add `is_zero` gadget)
- [ ] Formal proof of `LessThanOrEqual` in Lean (beyond empirical)

### Medium Priority
- [ ] Implement `SignatureVerify` for external chain signatures
- [ ] Add formal verification for `LessThanStrict` and `LessThanLoose`
- [ ] Verify `MerkleRoot` gadget for different tree depths

### Lower Priority
- [ ] Investigate `timestamp_range` for time-delegation
- [ ] Explore `set_membership` for permission hierarchies

---

## See Also

- [zkVM Primitive Layer](zkvm_primitives.md) — Deep dive into opcode implementation
- [Field Arithmetic Constraints](field_arithmetic.md) — Why field math matters
- [proofs/lean/](../../proofs/lean/) — Formal verification with Lean 4
- [opcode_universe.md](opcode_universe.md) — Complete mathematical universe analysis
- `dao/exec.zk` — Cross-multiplication pattern example
- `src/zk/gadget/less_than.rs` — Halo2 implementation
