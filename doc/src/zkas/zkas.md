zkas
====

zkas is a compiler for the Halo2 zkVM language used in
[DarkFi](https://codeberg.org/darkrenaissance/darkfi).

The current implementation found in the DarkFi repository inside
[`src/zkas`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/zkas)
is the reference compiler and language implementation. It is a
toolchain consisting of a lexer, parser, static and semantic analyzers,
and a binary code compiler.

The
[`main.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/zkas/src/main.rs)
file shows how this toolchain is put together to produce binary code
from source code.

# Architecture

The main part of the compilation happens inside the parser. New opcodes
can be added by extending
[`opcode.rs`](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/zkas/opcode.rs).

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

