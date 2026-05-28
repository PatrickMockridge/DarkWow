# Spend Hooks

Cross-contract call authorization pattern for token transfers.

## Overview

A spend hook allows Contract A to call a function in Contract B, authorizing the transfer of tokens. This pattern enables atomic bilateral operations where tokens move between contracts in a single transaction.

## Pattern

In promissory_note operations that spend coins (`BurnV1`, `TransferV1`, `OtcSwapV1`), the `spend_hook` and `user_data` fields are carried on `Input` structs (`src/contract/promissory_note/src/model/mod.rs:184-185`). These fields are also present in `CoinAttributes` (lines 141-142) and `Output` (passed via AEAD-encrypted note).

The `spend_hook` field is a `pallas::Base` value — typically a contract function ID — included as a public input in the ZK proof metadata. The `user_data` field carries opaque data for the receiving contract.

There is no standalone `SpendHook` struct; the two fields live inline on `Input`, `Output`, and `CoinAttributes`.

When a promissory_note function executes with a non-zero `spend_hook`:
1. Token transfer is verified (ZK proof checked)
2. The `spend_hook` value is exposed in the proof's public inputs
3. The calling contract (e.g. DEX, Stablecoin) validates the `spend_hook` value matches expected function IDs

Note: the actual child-call dispatch is a host-layer mechanism — the promissory_note entrypoint exposes `spend_hook` in metadata but does not itself execute the target contract.

## Usage in Contracts

### DEX (ExecuteSwapV1)

The DEX uses `otc_swap_v1` (0x05) for atomic bilateral token swaps:

```
DEX::ExecuteSwapV1
├── promissory_note::OtcSwapV1 (alice's tokens to bob)
│   └── spend_hook: DEX::ExecuteSwapV1 (callback)
└── promissory_note::OtcSwapV1 (bob's tokens to alice)
    └── spend_hook: DEX::ExecuteSwapV1 (callback)
```

### Stablecoin (MintStableV1)

Stablecoin uses `transfer_v1` (0x04) to move minted stablecoins to users:

```
Stablecoin::MintStableV1
└── promissory_note::TransferV1
    └── spend_hook: Stablecoin::MintStableV1 (confirmation)
```

### DarkbetExchange

DarkbetExchange uses multiple `transfer_v1` calls for position management:

```
DarkbetExchange::BuyPositionV1
├── promissory_note::TransferV1 (position mint)
└── promissory_note::TransferV1 (bet placement)

DarkbetExchange::SettleBetV1
├── promissory_note::TransferV1 (payout)
└── promissory_note::TransferV1 (house fee)
```

## Child Call Validation

Contracts validate child calls by checking:

1. **Function ID**: `data[0]` must match the expected function (e.g., `0x05` for OtcSwapV1)
2. **Call count**: Typically exactly one child call per endpoint
3. **Authorization**: The child call's proof must be valid

Example from DEX `execute_swap_v1` (atomic swap requires 2 child calls):

```rust
// Validate exactly 2 child calls for atomic swap
ensure!(call_data.children_indexes.len() == 2);

// Validate both are OtcSwap calls
for child_idx in call_data.children_indexes {
    let child_call = &calls[child_idx];
    let func_id = child_call.data[0];
    ensure!(func_id == 0x05); // OtcSwapV1
}
```

## Security Considerations

- **Atomicity**: If child call fails, parent transaction fails (atomic rollback)
- **Authorization**: The proof in the child call must be valid for the transfer to succeed
- **No reentrancy**: Spend hooks cannot recursively call back to the originating contract
- **FuncId Binding**: DEX circuits verify FuncIds as public inputs (prover-provided). The contract computes `FuncId = poseidon_hash([contract_id, func_code])` from child calls and includes it in metadata for ZK verification. Tests must deploy promissory_note FIRST to compute real FuncIds before proof generation.