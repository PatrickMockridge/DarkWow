# Spend Hooks

Cross-contract callback mechanism for token burns. When a Promissory Note coin is
burned with a non-zero `spend_hook`, the PN contract dispatches a callback to the
target contract, enabling atomic cross-contract composition.

## Overview

The `spend_hook` field is a `pallas::Base` value embedded in every PN coin commitment:

```
Coin = poseidon_hash(owner_pub, value, token_id, spend_hook, user_data, blind)
```

It is typically set to a `ContractId` (truncated to 32 bytes and interpreted as a
field element). When a coin is burned via BurnV1 with `spend_hook != 0`, the PN
contract calls the target contract's `__spend_hook` export. When `spend_hook == 0`,
no callback fires — the burn is a plain destruction.

The field is also present on `Output` and `TokenMintParamsV1`/`MintParamsV1`, so
issuing contracts can set the spend_hook on newly created coins.

## ZK Circuit Coverage

All 5 PN ZK circuits expose `coin_spend_hook` as a public input:

| Circuit | Role | spend_hook public? |
|---------|------|--------------------|
| `burn_v1.zk` | Input/Spend | YES |
| `mint_v1.zk` | Mint new coins | YES |
| `token_mint_v1.zk` | Token create + initial mint | YES |
| `blind_output_v1.zk` | Transfer/OTC outputs | YES |
| `redeem_v1.zk` | Redeem receipt coin | YES |

This means parent contracts can verify the `spend_hook` of any coin — input,
output, or receipt — by inspecting the ZK proof's public inputs. The value is
cryptographically bound to the proof; a prover cannot change it without breaking
the proof.

## Callback Mechanism

When a BurnV1 transaction includes inputs with non-zero `spend_hook`, the full
dispatch chain is:

```
User calls PN::BurnV1
  ├── PN verifies all inputs share the same spend_hook (SpendHookMismatch if not)
  ├── PN verifies nullifiers (no double-spend)
  ├── PN builds BurnSpendHookPayload {
  │     caller_contract_id,  // PN's own ContractId
  │     nullifiers,          // all nullifiers from this burn
  │     token_commits,       // per-input token commitments
  │     value_commits,       // per-input value commitments (Pedersen)
  │     user_data_encs,      // per-input encrypted user data
  │   }
  ├── PN calls emit_spend_hook(target_cid, &serialize(&payload))
  │     └── Host writes (target_cid_bytes, payload) to Env.spend_hook_request
  │
  └── After PN.exec() returns, blockchain pipeline (dwowd):
        ├── Reads env.spend_hook_request.take()
        ├── Loads target contract WASM from store
        ├── Creates target Runtime in same overlay
        ├── target_runtime.metadata(&payload)  // validates callback structure
        ├── target_runtime.spend_hook(&payload) // __spend_hook export
        └── target_runtime.apply(&update_data)  // commit state changes
              │
              └── If any step fails → revert_to_checkpoint()
                   (burn + callback are atomic)
```

### Atomicity

The spend_hook callback runs in the **same overlay** as the parent burn. If any
step of the callback fails (metadata, spend_hook, or apply), the checkpoint is
reverted — the burn does not take effect and no nullifiers are published. This
provides strong atomicity: either both the burn and the callback succeed, or
neither does.

### Backward Compatibility

When `spend_hook == pallas::Base::zero()`, no callback is dispatched. This is the
default for all existing coins and contracts — the mechanism is opt-in per coin.

## BurnSpendHookPayload

The payload delivered to the target contract's `__spend_hook`:

```rust
pub struct BurnSpendHookPayload {
    pub caller_contract_id: ContractId,    // PN contract that initiated the burn
    pub nullifiers: Vec<pallas::Base>,     // nullifiers being published
    pub token_commits: Vec<pallas::Base>,  // per-input token commitments
    pub value_commits: Vec<pallas::Point>, // per-input Pedersen value commitments
    pub user_data_encs: Vec<pallas::Base>, // per-input encrypted user data
}
```

The target contract receives this as the `instruction_data` argument to its
`__spend_hook` export. It should:
1. Verify `caller_contract_id` is the expected PN contract
2. Verify nullifiers are not duplicated (replay protection)
3. Extract any application-specific data from `user_data_encs`

## Implementing a Spend Hook Receiver

### 1. Use `define_contract_with_spend_hook!`

Replace `define_contract!` with `define_contract_with_spend_hook!` in your
entrypoint, providing a 5th function for spend_hook callbacks:

```rust
define_contract_with_spend_hook! {
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata,
    spend_hook: process_spend_hook
}
```

This generates the `__spend_hook` WASM export alongside the standard 4 exports
(`__initialize`, `__entrypoint`, `__update`, `__metadata`).

Contracts using the standard `define_contract!` macro do NOT export `__spend_hook`
and cannot receive callbacks.

### 2. Implement the spend_hook handler

```rust
fn process_spend_hook(
    contract_id: ContractId,
    instruction_data: &[u8],
) -> ContractResult {
    // Deserialize the BurnSpendHookPayload
    let payload: BurnSpendHookPayload = deserialize(instruction_data)?;

    // Verify the caller is the expected PN contract
    let expected_pn_cid = get_expected_pn_contract_id()?;
    if payload.caller_contract_id != expected_pn_cid {
        return Err(ContractError::InvalidCaller);
    }

    // Check nullifiers for replay
    for nullifier in &payload.nullifiers {
        if nullifier_already_processed(nullifier)? {
            return Err(ContractError::ReplayDetected);
        }
    }

    // Build update data for the apply phase
    let update = SpendHookCallbackUpdateV1 {
        nullifiers: payload.nullifiers.iter().map(|n| n.to_repr()).collect(),
        value_commits: payload.value_commits.iter().map(|vc| vc.to_bytes()).collect(),
    };

    set_return_data(&serialize(&update))
}
```

### 3. Handle the apply phase

The return data from `__spend_hook` is passed to `apply()`. Add a corresponding
variant to your update enum and process it in `process_update`.

## Security Considerations

- **Caller verification**: Always verify `payload.caller_contract_id` matches the
  expected PN contract. Without this, any contract could forge callbacks.
- **Nullifier replay**: Track processed nullifiers in the database. A malicious
  transaction could attempt to re-burn the same coins.
- **Atomicity**: The callback runs in the same overlay as the burn. If your
  spend_hook handler panics or returns an error, the burn is reverted. Ensure
  handlers are fallible and don't make assumptions about external state.
- **No reentrancy**: Callbacks go through `__spend_hook`, not `__entrypoint`
  (exec). The target contract's normal exec() is never called during a callback,
  preventing reentrancy into the contract's main logic.
- **Single callback per burn**: The `emit_spend_hook` host function enforces one
  callback per exec() call. Burns with multiple spend_hook values are rejected
  with `SpendHookMismatch`.
- **Deployooor compatibility**: The `__spend_hook` export is optional — contracts
  without it deploy normally. The Deployooor validation loop skips `__spend_hook`
  if present, and does not reject contracts that lack it.

## Usage in Contracts

### Stablecoin (SpendHookCallback)

The stablecoin receives spend_hook callbacks when users burn stablecoins:

```
User burns stablecoin via PN::BurnV1 (spend_hook = stablecoin_cid)
  → PN dispatches callback to stablecoin
    → stablecoin.process_spend_hook() verifies:
        - Caller is the expected PN contract
        - Nullifiers are not duplicate
    → stablecoin.apply() records nullifiers for replay protection
```

### Pattern for Issuing Contracts

1. At mint time, set `spend_hook = your_contract_id` on the coin
2. Switch to `define_contract_with_spend_hook!` in your entrypoint
3. Implement `process_spend_hook` with caller verification and nullifier tracking
4. All burns of your token automatically route through your callback

### Pattern for Intermediary Contracts

Intermediaries (DEX, game_room, insurance_market) that receive coins should
verify the `spend_hook` on incoming coins matches their expectations. Since all
5 ZK circuits now expose `spend_hook` as a public input, the value is available
in the proof metadata.

## Files

```
src/sdk/src/wasm/
├── entrypoint.rs        # define_contract! and define_contract_with_spend_hook! macros
└── util.rs              # emit_spend_hook WASM wrapper, host function declarations

src/runtime/
├── import/util.rs       # emit_spend_hook host function (writes to Env.spend_hook_request)
└── vm_runtime.rs        # ContractSection::SpendHook, Runtime::spend_hook(), Env field

src/contract/promissory_note/
├── src/model/mod.rs     # BurnSpendHookPayload struct
├── src/error.rs         # SpendHookMismatch error
└── src/entrypoint/mod.rs # burn_v1() spend_hook dispatch logic

src/contract/stablecoin/
├── src/lib.rs           # SpendHookCallback = 0x0B
├── src/model/mod.rs     # SpendHookCallbackUpdateV1
└── src/entrypoint.rs    # process_spend_hook(), apply_spend_hook_callback()

bin/dwowd/src/execution.rs  # Pipeline: read spend_hook_request, dispatch callback

src/contract/deployooor/src/entrypoint/deploy_v1.rs  # __spend_hook optional export
```

## References

- [Promissory Note](../../contract/promissory_note.md) — Bearer instrument contract
- [Safety Patterns](../../dev/contracts/safety.md) — spend_hook safety checklist
- [Stablecoin](../../contract/stablecoin.md) — Reference spend_hook receiver implementation
- [Intermediary Contracts](../../contract/promissory_note_intermediaries.md) — Ecosystem audit
