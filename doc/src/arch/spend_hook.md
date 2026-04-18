# Spend Hooks

Cross-contract call authorization pattern for token transfers.

## Overview

A spend hook allows Contract A to call a function in Contract B, authorizing the transfer of tokens. This pattern enables atomic bilateral operations where tokens move between contracts in a single transaction.

## Pattern

In money_v3 operations (`MintV1`, `TransferV1`, `OtcSwapV1`), the `spend_hook` and `user_data` fields specify what to call after tokens are transferred:

```rust
pub struct SpendHook {
    pub spend_hook: pallas::Base,   // Function ID to invoke
    pub user_data: pallas::Base,    // Data passed to the hook
}
```

When a money_v3 function executes with a non-zero `spend_hook`:
1. Token transfer is verified (proof checked)
2. Child call is made to `spend_hook` with `user_data`
3. Child call result determines if the main transaction succeeds

## Usage in Contracts

### DEX (ExecuteSwapV1)

The DEX uses `otc_swap_v1` (0x05) for atomic bilateral token swaps:

```
DEX::ExecuteSwapV1
├── money_v3::OtcSwapV1 (alice's tokens to bob)
│   └── spend_hook: DEX::ExecuteSwapV1 (callback)
└── money_v3::OtcSwapV1 (bob's tokens to alice)
    └── spend_hook: DEX::ExecuteSwapV1 (callback)
```

### Stablecoin (MintStableV1)

Stablecoin uses `transfer_v1` (0x04) to move minted stablecoins to users:

```
Stablecoin::MintStableV1
└── money_v3::TransferV1
    └── spend_hook: Stablecoin::MintStableV1 (confirmation)
```

### DarkbetExchange

DarkbetExchange uses multiple `transfer_v1` calls for position management:

```
DarkbetExchange::BuyPositionV1
├── money_v3::TransferV1 (position mint)
└── money_v3::TransferV1 (bet placement)

DarkbetExchange::SettleBetV1
├── money_v3::TransferV1 (payout)
└── money_v3::TransferV1 (house fee)
```

## Child Call Validation

Contracts validate child calls by checking:

1. **Function ID**: `data[0]` must match the expected function (e.g., `0x05` for OtcSwapV1)
2. **Call count**: Typically exactly one child call per endpoint
3. **Authorization**: The child call's proof must be valid

Example from DEX `execute_swap_v1`:

```rust
// Validate exactly one child call
ensure!(call_data.children_indexes.len() == 1);

// Validate it's an OtcSwap call
let func_id = call_data.data[0];
ensure!(func_id == DexFunction::OtcSwapV1 as u8);
```

## Security Considerations

- **Atomicity**: If child call fails, parent transaction fails (atomic rollback)
- **Authorization**: The proof in the child call must be valid for the transfer to succeed
- **No reentrancy**: Spend hooks cannot recursively call back to the originating contract