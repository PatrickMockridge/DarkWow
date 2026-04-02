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

## Recommended Architecture: Domain-Driven SDK

The recommended pattern separates concerns into distinct crates:

```
my_app/
├── Cargo.toml          # Workspace
├── crates/
│   ├── my-app-domain/   # Pure domain types, no external deps
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── api.rs   # ApiRequest/ApiResponse enums
│   │   │   └── model.rs # Domain models
│   │   └── Cargo.toml
│   ├── my-app-client/   # Transport + SDK implementation
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── transport.rs
│   │   └── Cargo.toml
│   └── my-app-core/     # Business logic (optional)
src/                     # Application binary
```

### Why This Pattern?

1. **Domain independence**: The domain crate has zero external dependencies (only `serde`)
2. **Testability**: Pure domain types are easy to test without infrastructure
3. **Reusability**: Same domain types work across CLI, desktop, web
4. **Type safety**: Compile-time guarantees instead of runtime JSON parsing

## Domain Crate Pattern

The domain crate defines the API contract without any implementation details:

### API Types (`api.rs`)

```rust
use serde::{Deserialize, Serialize};

// Error type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

// Request params - each operation has its own typed struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSwapParams {
    pub wallet_id: Option<String>,
    pub offer_token: [u8; 32],
    pub offer_amount: u64,
    pub request_token: [u8; 32],
    pub request_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSubmitParams {
    pub wallet_id: Option<String>,
    pub draft: SendDraft,
    pub dry_run: bool,
}

// API Request enum - one variant per operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiRequest {
    // Lifecycle
    CreateWallet(CreateWalletParams),
    OpenWallet(OpenWalletParams),
    UnlockWallet(UnlockWalletParams),

    // Swap operations
    CreateSwap(CreateSwapParams),
    AcceptSwap(AcceptSwapParams),
    ExecuteSwap(ExecuteSwapParams),

    // Send operations
    SendSubmit(SendSubmitParams),
}

impl ApiRequest {
    pub fn method_name(&self) -> &'static str {
        match self {
            ApiRequest::CreateWallet(_) => "wallet.create",
            ApiRequest::OpenWallet(_) => "wallet.open",
            ApiRequest::UnlockWallet(_) => "wallet.unlock",
            ApiRequest::CreateSwap(_) => "swap.create",
            ApiRequest::AcceptSwap(_) => "swap.accept",
            ApiRequest::ExecuteSwap(_) => "swap.execute",
            ApiRequest::SendSubmit(_) => "wallet.send_submit",
        }
    }
}

// API Response enum - matching response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiResponse {
    WalletCreated(WalletSummary),
    WalletOpened(WalletSummary),
    SwapCreated(SwapReceipt),
    SendReceipt(SendReceipt),
    // ... other responses
}
```

### Domain Models (`model.rs`)

```rust
use serde::{Deserialize, Serialize};

// Pure domain models with no external dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDraft {
    pub token_id: [u8; 32],
    pub recipients: Vec<Recipient>,
    pub amount: u64,
    pub fee: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipient {
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSummary {
    pub id: String,
    pub name: String,
    pub network: String,
}
```

## Client Crate Pattern

The client crate implements the SDK using the domain types:

### Transport Trait

```rust
use drk_desktop_wallet_domain::{ApiRequest, ApiResponse, ServiceError};

/// Transport trait allows different implementations
pub trait Transport: Send + Sync {
    fn request(&self, request: &ApiRequest) -> Result<ApiResponse, ServiceError>;
}

/// Loopback transport for in-process calls
pub struct LoopbackTransport<H> {
    handler: H,
}

impl<H> Transport for LoopbackTransport<H>
where
    H: HandleRequest,
{
    fn request(&self, request: &ApiRequest) -> Result<ApiResponse, ServiceError> {
        self.handler.handle(request.clone())
    }
}

/// HTTP transport for networked calls
pub struct HttpTransport {
    endpoint: String,
}

impl Transport for HttpTransport {
    fn request(&self, request: &ApiRequest) -> Result<ApiResponse, ServiceError> {
        // JSON-RPC call to endpoint
    }
}
```

### Typed SDK Client

```rust
use drk_desktop_wallet_domain::*;

pub struct WalletClient<T: Transport> {
    transport: T,
}

impl<T: Transport> WalletClient<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn create_wallet(&self, params: CreateWalletParams) -> Result<WalletSummary, ServiceError> {
        match self.transport.request(&ApiRequest::CreateWallet(params))? {
            ApiResponse::WalletCreated(summary) => Ok(summary),
            other => Err(unexpected("WalletCreated", other)),
        }
    }

    pub fn create_swap(&self, params: CreateSwapParams) -> Result<SwapReceipt, ServiceError> {
        match self.transport.request(&ApiRequest::CreateSwap(params))? {
            ApiResponse::SwapCreated(receipt) => Ok(receipt),
            other => Err(unexpected("SwapCreated", other)),
        }
    }

    pub fn submit_send(&self, params: SendSubmitParams) -> Result<SendReceipt, ServiceError> {
        match self.transport.request(&ApiRequest::SendSubmit(params))? {
            ApiResponse::SendReceipt(receipt) => Ok(receipt),
            other => Err(unexpected("SendReceipt", other)),
        }
    }
}

fn unexpected<T>(expected: &str, actual: ApiResponse) -> ServiceError {
    ServiceError::with_detail(
        "unexpected_response",
        format!("Expected `{expected}` response"),
        format!("{actual:?}"),
    )
}
```

## Contract SDK Pattern (for Smart Contracts)

Each DarkFi contract provides a client library in `src/contract/<name>/src/client/mod.rs`:

### Builder Pattern

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

    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    pub fn offer_token(&mut self, token: [u8; 32]) -> &mut Self {
        self.offer_token = Some(token);
        self
    }

    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
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

### Usage

```rust
let call_data = CreateSwapBuilder::new()
    .secret(alice_secret)
    .offer_token(drk_token)
    .offer_amount(1000)
    .build()?;
```

## Key Differences from Generic RPC

**This typed approach is better than generic `contract.invoke()` because:**

| Aspect | Typed API | Generic RPC |
|--------|----------|-------------|
| Type safety | Compile-time checking | Runtime JSON errors |
| Discovery | IDE autocomplete | Documentation lookup |
| Refactoring | Compiler-guided | Manual everywhere |
| Error handling | Exhaustiveness checking | Missed cases |

## Contract Entrypoint Pattern

DarkFi contracts use `define_contract!` with four functions:

```rust
darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);
```

| Function | Purpose | Returns |
|----------|---------|---------|
| `init_contract` | Initialize contract state | `ContractResult` |
| `process_instruction` | Verify state transition | `ContractResult` (update data) |
| `process_update` | Apply state changes | `ContractResult` |
| `get_metadata` | Return ZK public inputs | `ContractResult` |

### Function Selector Pattern

```rust
define_contract_function!(DexFunction {
    InitializeV1 = 0x00,
    CreateSwapV1 = 0x01,
    AcceptSwapV1 = 0x02,
    ExecuteSwapV1 = 0x03,
    CancelSwapV1 = 0x04,
});
```

First byte of call data = function selector.

### WASM APIs

```rust
use darkfi_sdk::wasm;

// Database
let info_db = wasm::db::db_init(cid, "info")?;
let swaps_db = wasm::db::db_lookup(cid, "swaps")?;
wasm::db::db_set(swaps_db, &key, &value)?;
let data = wasm::db::db_get(swaps_db, &key)?;

// Utilities
let call_idx = wasm::util::get_call_index()? as usize;
wasm::util::set_return_data(&metadata)?;
```

### Error Handling

```rust
use darkfi_sdk::error::{ContractError, ContractResult};

// Valid variants:
ContractError::IoError("message".to_string())
ContractError::InvalidFunction
ContractError::Custom(u32)
```

## Best Practices

### 1. Separate Domain from Implementation

```rust
// crates/my-app-domain/src/lib.rs
pub mod api;
pub mod model;

// Re-export for convenience
pub use api::{ApiRequest, ApiResponse};
pub use model::{User, Account};
```

### 2. Use the Transport Trait

```rust
// Allow different transports (loopback, HTTP, WebSocket)
pub fn do_something<T: Transport>(client: &WalletClient<T>) {
    client.create_wallet(params)?;
}
```

### 3. Validate Early in Builders

```rust
pub fn build(&self) -> Result<Vec<u8>, ClientError> {
    let secret = self.secret.ok_or_else(|| {
        ClientError::InvalidInput("secret required".into())
    })?;

    if self.amount == 0 {
        return Err(ClientError::InvalidInput("amount must be > 0".into()));
    }
    // ...
}
```

### 4. Exhaustive Match Checking

```rust
// Compiler ensures all variants handled
match self.transport.request(&ApiRequest::CreateWallet(params))? {
    ApiResponse::WalletCreated(summary) => Ok(summary),
    // Compiler error if we forget a variant
}
```

### 5. Type-Safe IDs

```rust
// Good: Newtype wrappers
pub struct SwapId([u8; 32]);
pub struct TokenId([u8; 32]);

// Avoid: Raw bytes
pub struct CreateSwapParams {
    pub swap_id: [u8; 32],  // Ambiguous
}
```

## File Structure

For a complete DarkFi application:

```
drk-desktop-wallet/           # Workspace
├── Cargo.toml
├── crates/
│   ├── drk-desktop-wallet-domain/   # API types + domain models
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── api.rs               # ApiRequest/ApiResponse
│   │       └── model.rs            # Domain types
│   ├── drk-desktop-wallet-client/  # SDK implementation
│   │   └── src/
│   │       ├── lib.rs              # WalletClient<T>
│   │       └── transport.rs        # Transport trait + impls
│   └── drk-desktop-wallet-core/    # Business logic
├── src/                            # Application binary
│   └── main.rs
└── app_ui/                         # Optional UI code
```

## Testing

Test domain types without infrastructure:

```rust
#[cfg(test)]
mod tests {
    use drk_desktop_wallet_domain::*;

    #[test]
    fn test_api_request_serialization() {
        let request = ApiRequest::CreateWallet(CreateWalletParams {
            name: "test".into(),
            network: "localnet".into(),
            // ...
        });
        let json = serde_json::to_string(&request).unwrap();
        let parsed: ApiRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(request, parsed);
    }
}
```

## References

- [Contract Architecture](../arch/sc/sc.md)
- [Transaction Lifetime](../arch/tx_lifetime.md)
- [ZK VM Primitives](../arch/zkvm_primitives.md)
- Example domain-driven SDK: `crates/drk-desktop-wallet-domain/` in chatty-watty-tinker-token-box
