# zkVM Primitive Layer: Opcode Reasoning

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

| Promised Feature | Required Primitives |
|-----------------|---------------------|
| "Prove you meet criteria without revealing data" (identity) | `LessThanOrEqual`, `IsEqualBase` |
| "Collateral must exceed debt" (stablecoin) | `LessThanOrEqual` |
| "Order matching at or above price" | `LessThanOrEqual`, `IsEqualBase` |
| "Partial fills where amount <= requested" | `LessThanOrEqual` |
| "Liquidation when collateral/debt ratio < threshold" | `LessThanOrEqual`, `BaseMul`, `BaseDiv` |
| "Generic intent fill conditions" (intent-amm fork) | `IsEqualBase`, `LessThanOrEqual`, `NotBase` |
| "Time-locked reveal with bypass conditions" | `IsEqualBase`, `NotBase` |
| "ZK-computed AMM pricing" | `BaseMul`, `BaseDiv`, `LessThanOrEqual` |

## The Core Gap: Opcodes That Return vs. Opcodes That Constrain

The existing zkVM has two kinds of comparison:

| Opcode | Signature | Behavior |
|--------|-----------|----------|
| `LessThanStrict` | `(Base, Base) → ()` | **Constrains**: fails if `a >= b` |
| `LessThanLoose` | `(Base, Base) → ()` | **Constrains**: fails if `a >= b` |

Both *constrain* the circuit but **do not return a value**. You cannot use
their results in subsequent computation:

```zk
x = less_than_strict(a, b);  // ERROR: returns ()
y = x + 1;                   // Cannot use x as a value
```

This matters for constructions like:

```zk
# Want: if (a <= b) then c else d
# But we need the comparison result as a value to select
result = cond_select(less_than_or_equal(a, b), c, d);  // needs return value
```

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

### `BaseDiv(a, b)` → Base

**Signature**: `(Base a, Base b) → Base`

**Purpose**: Field division `a / b`. Currently no division opcode exists.

**What it unlocks**:

```zk
# Price computation (AMM, stablecoin)
exchange_rate = base_div(output_amount, input_amount);

# Collateralization ratios
collateral_ratio = base_div(collateral_value, debt_value);
```

**Note**: Division in a prime field requires computing the modular multiplicative
inverse. Expensive in circuit form but feasible.

---

### `BaseModExp(base, exp, mod)` → Base

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

- [Private Authorization Layer](privauth.md) — the authorization pattern these opcodes enable
- [Composability & General Primitives](composability.md) — how these opcodes compose across contracts
- [Contract MVP Status](mvp_status.md) — blockers for each contract and the single highest-leverage primitive to implement
- [intent-amm fork (rusticml)](https://codeberg.org/rusticml/darkfi-intent-amm-proposal) — experimentation with intent-based AMM logic
- [zkas bincode](../zkas/bincode.md) — existing opcode specifications
- [Smart Contracts architecture](sc/sc.md) — contract layer built on zkVM
