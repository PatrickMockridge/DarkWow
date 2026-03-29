# Public Key Constraint Pre-commit Hook

*Automated prevention of the missing `constrain_equal_base` vulnerability in ZK circuits*

---

## Overview

DarkFi uses a git pre-commit hook to prevent the introduction of circuits with missing public key binding constraints. This vulnerability occurs when a circuit derives a public key from a secret using `ec_mul_base` but fails to bind the derived coordinates to public inputs before exposing them.

## The Vulnerability

### Incorrect Pattern (Vulnerable)

```zk
circuit "Vulnerable" {
    pub = ec_mul_base(secret, NULLIFIER_K);
    pub_x = ec_get_x(pub);
    pub_y = ec_get_y(pub);

    # WRONG: Only exposes, no binding constraint
    constrain_instance(pub_x);
    constrain_instance(pub_y);
}
```

**Why this is vulnerable**: The circuit only proves knowledge of `secret`, but does NOT prove that the derived public key matches the public inputs `pub_x` and `pub_y`. A malicious prover can claim any public key without knowing the corresponding secret.

### Correct Pattern (Sound)

```zk
witness "Example" {
    Base secret,
    Base pub_x,  # Public key coordinate - constrained to witness
    Base pub_y,  # Public key coordinate - constrained to witness
}

circuit "Example" {
    pub = ec_mul_base(secret, NULLIFIER_K);
    derived_pub_x = ec_get_x(pub);
    derived_pub_y = ec_get_y(pub);

    # CRITICAL: Bind derived public key to public inputs
    constrain_equal_base(derived_pub_x, pub_x);
    constrain_equal_base(derived_pub_y, pub_y);

    # Now expose as public inputs
    constrain_instance(pub_x);
    constrain_instance(pub_y);
}
```

## The Hook

**Location**: `hooks/pre-commit`

**Detection Logic**: The hook scans `.zk` files being committed for the pattern where `ec_get_x` or `ec_get_y` results are used in `constrain_instance` without a preceding `constrain_equal_base` binding.

### Detection Algorithm

1. Track all `ec_get_x` and `ec_get_y` assignments within circuit blocks
2. When `constrain_equal_base` is seen referencing those variables, clear the "pending" flag (this is correct)
3. When `constrain_instance` is seen referencing pending variables, flag as vulnerable
4. Clear pending flags on new `ec_mul_base` or `ec_mul_var_base` assignments (variables are overwritten)

### Error Output

When the hook detects the vulnerability:

```
ERROR: Vulnerable pubkey derivation pattern detected!
dao/proof/mint.zk:30 VULNERABLE: notes_public_x used in constrain_instance without constrain_equal_base

FIX: Add constrain_equal_base to bind the derived public key to its witness.
Example:
  BEFORE (vulnerable):
    pub = ec_mul_base(secret, NULLIFIER_K);
    pub_x = ec_get_x(pub);
    constrain_instance(pub_x);

  AFTER (sound):
    pub = ec_mul_base(secret, NULLIFIER_K);
    pub_x = ec_get_x(pub);
    constrain_equal_base(pub_x, witness_pub_x);  // BIND
    constrain_instance(witness_pub_x);

Commit rejected: vulnerable constrain_equal_base pattern detected.
```

## Setup

The hook is located in the `hooks/` directory and is configured via git's `core.hooksPath`:

```bash
# The repository is configured to use hooks/ as the hooks directory
git config core.hooksPath hooks
```

When cloning a fresh copy of the repository, ensure the hook is executable:

```bash
chmod +x hooks/pre-commit
```

## Exceptions

The hook correctly handles cases where `constrain_instance` is used on `ec_get_x/y` results from sources that do NOT derive from secrets:

```zk
# ec_mul_var_base with a known public key - no binding needed
coin_public_key = ec_mul_var_base(ONE, coin_public_key);
pub_x = ec_get_x(coin_public_key);
constrain_instance(pub_x);  # OK - coin_public_key was not derived from a secret
```

The hook only flags cases where `ec_get_x/y` follows `ec_mul_base` with a secret-derived multiplication, as these require binding.

## Future: zkas Builtin

The recurring nature of this vulnerability suggests a compiler-level solution would provide better protection. A proposed `derive_pubkey` builtin would enforce the constraint atomically:

```zk
# Proposed future syntax
derive_pubkey secret, NULLIFIER_K, pub_x, pub_y;
constrain_instance(pub_x);
constrain_instance(pub_y);
```

This would make the vulnerable pattern impossible to write.

## See Also

- [Security Analysis: Issue 19](../arch/security-analysis.md#issue-19-missing-public-key-constraint-majormd--fixed)
- [Writing ZK Proofs](../zkas/writing-zk-proofs.md)
- [ZKas Compiler](../zkas/zkas.md)
