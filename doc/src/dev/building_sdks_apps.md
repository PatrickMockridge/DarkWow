# Building SDKs and Applications for DarkFi

This guide covers the optimal patterns for building SDKs and applications that interact with DarkFi smart contracts.

## Architecture Overview

DarkFi applications typically follow a layered architecture:

```
┌─────────────────────────────────────────────────────────────┐
│                     Application Layer                          │
│  (CLI tools like drk, desktop wallets, web frontends)        │
├─────────────────────────────────────────────────────────────┤
│                       SDK Layer                              │
│  (Client transaction builders, RPC clients, wallet APIs)     │
├─────────────────────────────────────────────────────────────┤
│                   Contract Layer                              │
│  (WASM smart contracts + ZK circuits)                        │
├─────────────────────────────────────────────────────────────┤
│                     Node Layer                               │
│  (darkfid validator, P2P network, consensus)                 │
└─────────────────────────────────────────────────────────────┘
```

## Two Types of SDKs

### 1. Contract Client SDKs (Transaction Builders)

Each DarkFi contract provides a client library for building transactions. These live in `src/contract/<name>/src/client/mod.rs`.

**Pattern**: Builder API with method chaining

```rust
/// Builder for creating an atomic swap proposal
pub struct CreateSwapBuilder {
    secret: Option<[u8; 32]>,
    offer_token: Option<[u8; 32]>,
    offer_amount: Option<u64>,
    // ... more fields
}

impl CreateSwapBuilder {
    pub fn new() -> Self { /* ... */ }

    /// Set the secret for the lock
    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Set the token being offered
    pub fn offer_token(&mut self, token: [u8; 32]) -> &mut Self {
        self.offer_token = Some(token);
        self
    }

    /// Build the create swap transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        // Validation
        let secret = self.secret.ok_or_else(|| {
            ClientError::InvalidInput("secret required".into())
        })?;

        // Encode call data (first byte = function selector)
        let mut call_data = Vec::new();
        call_data.push(0x01); // CreateSwapV1 = 0x01
        call_data.extend_from_slice(&compute_swap_id(...));

        Ok(call_data)
    }
}
```

**Usage**:
```rust
let call_data = CreateSwapBuilder::new()
    .secret(alice_secret)
    .offer_token(drk_token)
    .offer_amount(1000)
    .request_token(eth_token)
    .request_amount(1)
    .build()?;
```

### 2. Integration SDKs (Wallet/Frontend)

Integration SDKs expose high-level APIs for applications. They combine:
- RPC client for node communication
- Wallet management
- Transaction signing
- State tracking

## Contract SDK Pattern

Every DarkFi contract follows this structure:

### Entry Point: `define_contract!`

```rust
darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);
```

**Four functions**:

| Function | Purpose | Returns |
|----------|---------|---------|
| `init_contract` | Initialize contract state | `ContractResult` |
| `process_instruction` | Verify state transition | `ContractResult` (update data) |
| `process_update` | Apply state changes | `ContractResult` |
| `get_metadata` | Return ZK public inputs | `ContractResult` |

### Function Selector Pattern

Contracts use a function enum with `#[repr(u8)]`:

```rust
define_contract_function!(DexFunction {
    InitializeV1 = 0x00,
    CreateSwapV1 = 0x01,
    AcceptSwapV1 = 0x02,
    ExecuteSwapV1 = 0x03,
    CancelSwapV1 = 0x04,
});
```

**Important**: The first byte of call data is the function selector.

### Data Encoding

All parameters use `darkfi_serial` for CBOR encoding:

```rust
use darkfi_serial::{SerialEncodable, SerialDecodable};

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateSwapParams {
    pub swap_id: [u8; 32],
    pub offer_token: [u8; 32],
    pub offer_amount: u64,
    // ...
}
```

### Error Handling

Use `ContractError` variants from the SDK:

```rust
use darkfi_sdk::error::{ContractError, ContractResult};

// Valid variants:
// ContractError::IoError("message".to_string())
// ContractError::InvalidFunction
// ContractError::Custom(u32)
```

**Do NOT use** non-existent variants like:
- `ContractError::NotYetImplemented`
- `ContractError::FailedToDeserialize`
- `ContractError::DecodeError`
- `ContractError::DbError`

## SDK APIs for Contracts

### WASM Database API

```rust
use darkfi_sdk::wasm;

// Initialize a tree
let info_db = wasm::db::db_init(cid, "info")?;

// Lookup a tree handle
let swaps_db = wasm::db::db_lookup(cid, "swaps")?;

// Get value
let data = wasm::db::db_get(swaps_db, &key)?;

// Check existence
if wasm::db::db_contains_key(swaps_db, &key)? {
    // ...
}

// Set value
wasm::db::db_set(swaps_db, &key, &value)?;

// Delete value
wasm::db::db_del(swaps_db, &key)?;
```

### WASM Utilities

```rust
use darkfi_sdk::wasm::util;

// Get current call index
let call_idx = wasm::util::get_call_index()? as usize;

// Set return data
wasm::util::set_return_data(&metadata)?;
```

### Serialization

```rust
use darkfi_serial::{deserialize, serialize, Encodable, Decodable};

// Serialize
let encoded = serialize(&data);

// Deserialize
let decoded: MyType = deserialize(&encoded)?;
```

### Field Operations

```rust
use darkfi_sdk::pasta::pallas;
use darkfi_sdk::crypto::pasta_prelude::PrimeField;

// Convert bytes to field element
let element = match pallas::Base::from_repr(params.some_bytes).into_option() {
    Some(v) => v,
    None => return Err(ContractError::IoError("Invalid".to_string()).into()),
};

// Convert field element to bytes
let bytes: [u8; 32] = element.to_repr();
```

### Intent Types

```rust
use darkfi_sdk::crypto::{IntentCommitment, IntentNullifier};

// Create from bytes
let commitment = IntentCommitment::from_bytes([0u8; 32]).unwrap();
let nullifier = IntentNullifier::from_bytes([0u8; 32]).unwrap();

// Get inner bytes
let inner = commitment.inner(); // returns pallas::Base
let bytes = commitment.to_bytes(); // returns [u8; 32]
```

## Application SDK Pattern

### RPC Client

Connect to `darkfid` via JSON-RPC:

```rust
use jsonrpc_client::{Client, JsonRpcError};

pub struct DarkfiClient {
    rpc: Client,
}

impl DarkfiClient {
    pub async fn new(url: &str) -> Result<Self, Error> {
        let rpc = Client::new(url);
        Ok(Self { rpc })
    }

    pub async fn contract_invoke(
        &self,
        contract_id: &str,
        function: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        self.rpc.call("contract.invoke", (contract_id, function, params)).await
    }
}
```

### Wallet Integration

```rust
pub struct Wallet {
    secret: SecretKey,
    rpc: DarkfiClient,
}

impl Wallet {
    pub async fn create_swap(
        &self,
        offer_token: [u8; 32],
        offer_amount: u64,
    ) -> Result<Transaction, Error> {
        // 1. Build contract call
        let call_data = CreateSwapBuilder::new()
            .secret(self.secret.to_bytes())
            .offer_token(offer_token)
            .offer_amount(offer_amount)
            .build()?;

        // 2. Create transaction with proof
        let proof = self.generate_zk_proof(&call_data)?;

        // 3. Submit via RPC
        let tx = self.submit_transaction(call_data, proof).await?;

        Ok(tx)
    }
}
```

## Transaction Lifecycle

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Builder    │────▶│  RPC Submit  │────▶│   darkfid    │
│  (creates)   │     │              │     │  (validates) │
└──────────────┘     └──────────────┘     └──────────────┘
                                                  │
                                                  ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   Receipt    │◀────│  Broadcast   │◀────│  ZK Verify   │
│  (result)    │     │              │     │              │
└──────────────┘     └──────────────┘     └──────────────┘
```

## Building a CLI Tool (drk pattern)

The `drk` tool demonstrates the recommended CLI pattern:

```
drk/
├── Cargo.toml
├── src/
│   ├── main.rs        # CLI entry point
│   ├── lib.rs         # Library exports
│   ├── error.rs      # Error types
│   ├── money.rs      # Money contract commands
│   ├── dao.rs        # DAO contract commands
│   ├── swap.rs       # Swap contract commands
│   ├── rpc.rs        # RPC client
│   └── walletdb.rs   # Wallet database
└── Makefile
```

### CLI Structure

```rust
// main.rs
#[tokio::main]
async fn main() -> Result<(), Error> {
    let matches = Command::new("drk")
        .subcommand(money::cmd())
        .subcommand(dao::cmd())
        .run_async()
        .await?;

    match matches.subcommand() {
        Some(("transfer", m)) => money::transfer(m).await?,
        Some(("swap", m)) => swap::execute(m).await?,
        // ...
    }
    Ok(())
}
```

## Best Practices

### 1. Use Builder Pattern for Transactions

```rust
// Good
let tx = TransactionBuilder::new()
    .contract("dao_escrow")
    .function("InitializeV1")
    .params(params)
    .fee(fee)
    .build()?;

// Avoid
let tx = build_transaction("dao_escrow", "InitializeV1", &params, fee);
```

### 2. Validate Early

```rust
pub fn build(&self) -> Result<Vec<u8>, ClientError> {
    // Validate all fields at the start
    let secret = self.secret.ok_or_else(|| {
        ClientError::InvalidInput("secret is required".into())
    })?;

    if self.amount == 0 {
        return Err(ClientError::InvalidInput("amount must be > 0".into()));
    }

    // Proceed with encoding
    // ...
}
```

### 3. Use Type-Safe IDs

```rust
// Good: Newtype wrappers
pub struct SwapId([u8; 32]);
pub struct TokenId([u8; 32]);

// Avoid: Raw bytes everywhere
pub struct CreateSwapParams {
    pub swap_id: [u8; 32],  // Ambiguous
}
```

### 4. Handle Errors Explicitly

```rust
// Good
impl From<ContractError> for MyError {
    fn from(e: ContractError) -> Self {
        match e {
            ContractError::IoError(msg) => MyError::Io(msg),
            ContractError::Custom(code) => MyError::Contract(code),
            _ => MyError::Unknown,
        }
    }
}

// Avoid
impl From<ContractError> for MyError {
    fn from(e: ContractError) -> Self {
        MyError::Other(Box::new(e)) // Loses type safety
    }
}
```

### 5. Separate ZK Proof Generation

```rust
// In client module
pub mod proof {
    use halo2_proofs::plonk;

    pub fn create_swap_proof(params: &CreateSwapParams) -> Result<Proof, ClientError> {
        // ZK proof generation is expensive
        // Keep separate from transaction building
    }
}
```

## File Structure

For a complete DarkFi application:

```
my_app/
├── Cargo.toml              # Workspace member or standalone
├── src/
│   ├── main.rs            # CLI entry
│   ├── lib.rs             # Library exports
│   ├── error.rs          # Error types
│   ├── rpc.rs            # RPC client
│   ├── wallet.rs         # Wallet management
│   ├── commands/         # CLI subcommands
│   │   ├── mod.rs
│   │   ├── transfer.rs
│   │   └── swap.rs
│   └── clients/          # Contract client SDKs
│       ├── mod.rs
│       ├── money.rs
│       └── dao.rs
└── Makefile
```

## Testing

Test contract interactions with the test harness:

```rust
#[cfg(test)]
mod tests {
    use darkfi_contract_test_harness::*;

    #[tokio::test]
    async fn test_create_swap() -> Result<(), TestError> {
        let harness = TestHarness::new().await?;
        let tx = harness.execute_create_swap().await?;
        assert!(harness.verify_tx(&tx).await?);
        Ok(())
    }
}
```

## References

- [Contract Architecture](../arch/sc/sc.md)
- [Transaction Lifetime](../arch/tx_lifetime.md)
- [ZK VM Primitives](../arch/zkvm_primitives.md)
- [Rust/WASM Interaction](rust-wasm-interaction.md)
- Example contracts: `src/contract/dex/`, `src/contract/money/`
