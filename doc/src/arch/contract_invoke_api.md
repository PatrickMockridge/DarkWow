# Generalized Contract Invocation API

> **Implementation Status:** Partially implemented (commit `7a7424579`). The `contract.invoke` RPC endpoint is functional for dry-run mode. Full ZK proof generation and transaction broadcasting is in progress.

## Overview

A generalized RPC API for invoking any DarkWow smart contract without requiring a new API endpoint per contract function.

A generalized RPC API for invoking any DarkWow smart contract function without requiring a new API endpoint per contract.

## Motivation

Traditional contract invocation requires a new RPC method for each contract function:
- `dao_escrow.initialize` → new endpoint
- `dao_escrow.pay_premium` → new endpoint
- `insurance_market.purchase` → new endpoint
- etc.

This creates API proliferation as new contracts are added. The generalized `contract.invoke` API provides a single endpoint that can invoke any contract function.

## Architecture

### RPC Endpoint

**Method:** `contract.invoke`

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "contract.invoke",
  "params": {
    "wallet_id": "optional_wallet_id",
    "contract_id": "dao_escrow",
    "function": "InitializeV1",
    "params": {
      "enable_drain_protection": true
    },
    "dry_run": false
  },
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "contract_id": "dao_escrow",
    "function": "InitializeV1",
    "result": {
      "selector": 0,
      "calldata_len": 42,
      "message": "Transaction building not yet implemented - ZK proof generation required"
    },
    "transaction_id": null,
    "status": "dry_run"
  },
  "id": 1
}
```

## Request Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `wallet_id` | string | No | Wallet ID to use. If omitted, uses default wallet. |
| `contract_id` | string | Yes | Contract identifier (e.g., "dao_escrow", "money", "insurance_market") |
| `function` | string | Yes | Function name to invoke (e.g., "InitializeV1", "PayPremiumV1") |
| `params` | object | Yes | JSON object with function-specific parameters |
| `dry_run` | boolean | No | If true, only simulate without broadcasting. Default: false |

## Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `contract_id` | string | Contract that was invoked |
| `function` | string | Function that was called |
| `result` | object | Function-specific result data |
| `transaction_id` | string? | Transaction ID if broadcast, null if dry_run |
| `status` | string | "simulated", "broadcast", or "failed" |

## Supported Contracts

### DAO-Escrow (`dao_escrow`)

| Function | Selector | Description |
|----------|----------|-------------|
| `InitializeV1` | `0x00` | Create new DAO-Escrow instance |
| `UpdateV1` | `0x01` | Update DAO-Escrow parameters |
| `PayPremiumV1` | `0x02` | Pay premium as member |
| `WithdrawV1` | `0x03` | Owner withdrawal |
| `EndowmentWithdrawV1` | `0x04` | Endowment withdrawal (insurance) |
| `TreasurySpendV1` | `0x05` | Treasury spending |
| `EnableDrainProtectionV1` | `0x06` | Enable DrainProtection |

### Money (`money`)

| Function | Selector | Description |
|----------|----------|-------------|
| `TransferV1` | `0x03` | Transfer funds |
| `TokenMintV1` | `0x07` | Mint tokens |
| `BurnV1` | `0x08` | Burn tokens |

### Native Contracts

Native contracts (`money`, `dao`, `deployooor`) use hardcoded ContractIds. WASM contracts must be deployed first.

## Contract Handler Interface

New contracts implement the `ContractHandler` trait:

```rust
pub trait ContractHandler: Send + Sync {
    /// Returns the contract identifier string
    fn contract_id(&self) -> &'static str;

    /// Returns the function selector (first byte of calldata) for a function name
    fn function_selector(&self, function: &str) -> Option<u8>;

    /// Build calldata bytes from JSON params
    fn build_params(&self, function: &str, params: JsonValue) -> HandlerResult<Vec<u8>>;

    /// List supported functions
    fn supported_functions(&self) -> Vec<&'static str>;
}
```

## Files

| File | Description |
|------|-------------|
| `bin/darkfid/src/rpc/contract.rs` | RPC request/response types and handler |
| `bin/darkfid/src/contract_registry.rs` | Registry trait and contract mapping |
| `bin/darkfid/src/contract_handler/mod.rs` | Handler module |
| `bin/darkfid/src/contract_handler/dao_escrow.rs` | DAO-Escrow handler implementation |

## Implementation Status

- [x] `ContractInvokeRequest/Response` types
- [x] `ContractHandler` trait
- [x] `ContractRegistry` with handler lookup
- [x] `DaoEscrowContractHandler` with function selectors
- [x] `contract.invoke` RPC endpoint
- [ ] ZK proof generation integration
- [ ] Wallet key/coin access for proof generation
- [ ] Transaction building and broadcasting
- [ ] Additional contract handlers (money, insurance_market, etc.)

## Example Usage

### Initialize DAO-Escrow with DrainProtection

```javascript
const response = await fetch('http://localhost:8332', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    jsonrpc: '2.0',
    method: 'contract.invoke',
    params: {
      contract_id: 'dao_escrow',
      function: 'InitializeV1',
      params: {
        owner_pubkey: '...',
        endowment_token_id: '...',
        enable_drain_protection: true
      },
      dry_run: true
    },
    id: 1
  })
});
const result = await response.json();
console.log(result.result.status); // "dry_run" or "broadcast"
```

## Halloy Frontend Integration

The halloy-extend wallet uses this API:

```rust
// In halloy-extend wallet.rs
pub async fn contract_invoke(
    mode: RuntimeMode,
    contract_id: String,
    function: String,
    params: serde_json::Value,
    dry_run: bool,
) -> ContractInvokeResult {
    // ... calls darkfid contract.invoke
}

// WhistleblowerSubmit uses it:
let params = serde_json::json!({
    "dao_name": selected_dao_name,
    "title": self.title,
    "escrow_amount": self.escrow_amount,
    "drain_protection_enabled": self.drain_protection_enabled,
});
wallet::contract_invoke(mode, "dao_escrow".into(), "InitializeV1".into(), params, false).await;
```

## See Also

- [DAO-Escrow Contract](../contract/dao_escrow.md)
- [DrainProtection](../contract/drain_protection.md)
- [Insurance Market](../contract/insurance_market.md)
