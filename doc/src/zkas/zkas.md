zkas
====

> **Note:** The zkas compiler and toolchain are inherited from upstream DarkWow.
> The core compiler infrastructure, language syntax, and binary format are shared
> with upstream and track upstream changes.

zkas is a compiler for the Halo2 zkVM language used in DarkWow.

The current implementation found in the repository inside
[`src/zkas`](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/zkas)
is the reference compiler and language implementation. It is a
toolchain consisting of a lexer, parser, static and semantic analyzers,
and a binary code compiler.

The
[`main.rs`](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/bin/zkas/src/main.rs)
file shows how this toolchain is put together to produce binary code
from source code.

# Architecture

The main part of the compilation happens inside the parser. New opcodes
can be added by extending
[`opcode.rs`](https://codeberg.org/PatrickM123/darkwow/src/branch/linear-master/src/zkas/opcode.rs).

```rust
{{#include ../../../bin/zkas/src/main.rs:zkas}}
```

# Security: Public Key Derivation

When adding circuits that derive public keys from secrets using `ec_mul_base` + `ec_get_x/y`, you **must** bind the derived coordinates to public inputs using `constrain_equal_base`. A pre-commit hook (`hooks/pre-commit`) automatically rejects circuits missing this binding.

**Required Pattern**:

```zk
# Derive and bind
pub = ec_mul_base(secret, NULLIFIER_K);
derived_x = ec_get_x(pub);
derived_y = ec_get_y(pub);
constrain_equal_base(derived_x, witness_x);  # BIND
constrain_equal_base(derived_y, witness_y);  # BIND

# Then expose
constrain_instance(witness_x);
constrain_instance(witness_y);
```

**Without binding**: A malicious prover can claim any public key without knowing the secret.

**See Also**: [Public Key Constraint Hook](../arch/pubkey-constraint-hook.md)

# ZK Language Syntax Specification

This section documents the zkas circuit language syntax based on actual usage patterns and compiler implementation.

## File Structure

A `.zk` file has four sections:

```zk
k = 11;
field = "pallas";

constant "Namespace" {
    # constants here (optional, can be empty)
}

witness "Namespace" {
    # witness declarations here
}

circuit "Namespace" {
    # circuit logic here
}
```

## Comments

Only `#` style comments are supported. C-style `//` comments cause parse errors:

```zk
# This is valid
# But this is NOT: // invalid comment
```

## Metadata Directives

### `k = N`

Specifies circuit rows as $2^N$. Typical values: 11-14. Must be present.

### `field = "pallas"`

Specifies the base field. Currently only `"pallas"` is supported.

## Constant Section

Declares elliptic curve generator constants for Pedersen commitments:

```zk
constant "CommitBet_V1" {
    EcFixedPointShort VALUE_COMMIT_VALUE,
    EcFixedPoint VALUE_COMMIT_RANDOM,
}
```

**Allowed constant types and names:**

| Type | Allowed Names |
|------|---------------|
| `EcFixedPoint` | `VALUE_COMMIT_RANDOM` |
| `EcFixedPointShort` | `VALUE_COMMIT_VALUE` |
| `EcFixedPointBase` | `VALUE_COMMIT_RANDOM_BASE`, `NULLIFIER_K` |

Empty constant sections are allowed but generate a warning:

```zk
constant "SettleBet_V1" {
    # Empty - no constants needed
}
```

## Witness Section

Declares circuit inputs (private and public). Variables declared here can be used in the circuit:

```zk
witness "CommitBet_V1" {
    Base player_pub_x,
    Base player_pub_y,
    Base bet_value,
    Base secret_nonce,
    Scalar value_blind,
}
```

**Allowed types:**
- `Base` - Pallas base field element
- `Scalar` - Pallas scalar field element
- `EcPoint` - Elliptic curve point (x, y coordinates)
- `Uint64` - Unsigned 64-bit integer (for literals only)

## Circuit Section

Contains the proof logic. Statements end with semicolons.

### Variable Assignment

```zk
result = opcode(arg1, arg2);
```

### Poseidon Hash

```zk
# Comma-separated arguments (NOT array syntax)
bet_id = poseidon_hash(player_pub_x, player_pub_y, bet_value);
```

**Incorrect** (causes parse error):
```zk
# WRONG - array syntax not supported
bet_id = poseidon_hash([player_pub_x, player_pub_y, bet_value]);
```

### EC Operations

```zk
# Pedersen commitment
vcv = ec_mul_short(bet_value, VALUE_COMMIT_VALUE);
vcr = ec_mul(value_blind, VALUE_COMMIT_RANDOM);
value_commit = ec_add(vcv, vcr);

# Public key derivation
pub = ec_mul_base(secret, NULLIFIER_K);
pub_x = ec_get_x(pub);
pub_y = ec_get_y(pub);
```

### Constraint Opcodes

Three constraint opcodes enforce circuit relationships:

```zk
# Bind derived values to witnesses (REQUIRED for security)
constrain_equal_base(derived_pub_x, witness_pub_x);
constrain_equal_base(derived_pub_y, witness_pub_y);

# Expose as public input
constrain_instance(pub_x);
constrain_instance(pub_y);
```

**List of constraint opcodes:**

| Opcode | Arguments | Purpose |
|--------|-----------|---------|
| `constrain_equal_base(a, b)` | (Base, Base) | Enforce a == b |
| `constrain_equal_point(a, b)` | (EcPoint, EcPoint) | Enforce point equality |
| `constrain_instance(x)` | (Base) | Bind x to public input |

### Nested Function Calls

Opcodes can be nested - the inner result is pushed to the heap and consumed by the outer call:

```zk
# Valid nested call
constrain_instance(ec_get_x(token_commit));

# Equivalent expanded form
coord = ec_get_x(token_commit);
constrain_instance(coord);
```

## Complete Example

```zk
# Commit Bet Circuit
k = 11;
field = "pallas";

constant "CommitBet_V1" {
    EcFixedPointShort VALUE_COMMIT_VALUE,
    EcFixedPoint VALUE_COMMIT_RANDOM,
}

witness "CommitBet_V1" {
    Base player_pub_x,
    Base player_pub_y,
    Base bet_value,
    Base secret_nonce,
    Base blind,
    Base token_id,
    Scalar value_blind,
}

circuit "CommitBet_V1" {
    # Derive bet ID from parameters
    bet_id = poseidon_hash(player_pub_x, player_pub_y, bet_value, secret_nonce, blind, token_id);
    constrain_instance(bet_id);

    # Verify value commitment
    vcv = ec_mul_short(bet_value, VALUE_COMMIT_VALUE);
    vcr = ec_mul(value_blind, VALUE_COMMIT_RANDOM);
    value_commit = ec_add(vcv, vcr);
    constrain_instance(ec_get_x(value_commit));
    constrain_instance(ec_get_y(value_commit));
}
```

## Common Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `Invalid token '/'` | C-style `//` comments | Use `#` comments |
| `Duplicate constant section` | Multiple `constant` blocks | Merge into single block |
| `Missing constant section` | No constant block at all | Add empty `constant "Namespace" {}` |
| `TABLE_ID_POINTS is not valid` | Wrong constant name | Use only allowed names above |
| `Character is illegal` at `[,]` | Array syntax in poseidon_hash | Use comma-separated args |
| `constrain_only(a < b)` | Comparison operators not supported | Use `less_than_strict` or remove |

## Security Pattern

When deriving public keys from secrets, **you must** bind the derived coordinates:

```zk
# WRONG - vulnerable!
pub = ec_mul_base(secret, NULLIFIER_K);
pub_x = ec_get_x(pub);
constrain_instance(pub_x);  # No binding!

# CORRECT - bound to witness
pub = ec_mul_base(secret, NULLIFIER_K);
derived_x = ec_get_x(pub);
derived_y = ec_get_y(pub);
constrain_equal_base(derived_x, witness_pub_x);  # BIND
constrain_equal_base(derived_y, witness_pub_y);  # BIND
constrain_instance(witness_pub_x);
constrain_instance(witness_pub_y);
```

