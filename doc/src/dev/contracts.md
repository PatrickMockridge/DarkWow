# Smart Contracts

DarkFi implements several privacy-preserving smart contracts. Each contract follows the same architecture: WASM execution, ZK proof verification, and object-capability security.

## Contract Overview

| Contract | Purpose | Key Features |
|----------|---------|--------------|
| [Bridge](dev/contracts/bridge.md) | Cross-chain asset transfers | Object Capability Security, no VSS |
| [DEX](dev/contracts/dex.md) | Atomic swap DAO | Minimal Viable Information, ZK proofs |
| [Identity](dev/contracts/identity.md) | Credential proofs | Selective disclosure, competency DAGs |
| [Stablecoin](dev/contracts/stablecoin.md) | Collateralized debt positions | Monero backing, liquidation mechanism |
| [Escrow](../src/contract/escrow/README.md) | Hashed Timelock escrow | Conditional payments, timeout refund |
| [DAO-Escrow](../src/contract/dao_escrow/README.md) | Community insurance | DAO-governed endowment, premium collection |

## Common Patterns

All DarkFi contracts follow consistent patterns:

### Function IDs

```rust
#[repr(u8)]
enum ContractFunction {
    InitializeV1 = 0x00,
    // ... function variants
}
```

### Data Structures

```rust
// Call parameters are CBOR-encoded
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SomeParams {
    pub field1: [u8; 32],
    pub field2: u64,
    // ...
}
```

### ZK Circuits

ZK proofs verify contract logic without revealing secrets:

```zk
circuit some_proof_v1(prover: Witness) {
    // Public inputs (verified on-chain)
    commitment: Scalar = prover.pub_input("commitment");

    // Private inputs (known only to prover)
    secret: Scalar = prover.witness("secret");

    // Verification
    computed: Scalar = poseidon_hash(secret);
    assert_equal(computed, commitment);
}
```

### Error Handling

Errors map to numeric codes:

```rust
impl From<ContractError> for MyError {
    fn from(e: MyError) -> Self {
        match e {
            MyError::NotFound => Self::Custom(1),
            MyError::InvalidState => Self::Custom(2),
            // ...
        }
    }
}
```

## Building Contracts

```bash
# Build WASM contract
cd src/contract/<name>
make

# Compile ZK circuits
make proof

# Run tests
cargo test
```

## Contract Structure

```
src/contract/<name>/
├── proof/              # ZK proof circuits (.zk files)
├── src/
│   ├── client/        # Client-side transaction builders
│   ├── entrypoint.rs  # WASM contract implementation
│   ├── error.rs      # Error definitions
│   ├── lib.rs        # Contract enum and constants
│   └── model/        # Data structures
├── tests/             # Integration tests
├── Cargo.toml
└── Makefile
```

## Security Principles

1. **Object Capability**: Instead of VSS/threshold signing, users hold secrets directly
2. **Minimal Viable Information**: Only reveal what's necessary
3. **ZK-First**: Private data stays private, only proofs verified on-chain
4. **Deterministic**: Address derivation and commitments use cryptographic hashing
