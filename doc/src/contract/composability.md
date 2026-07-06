# Smart Contract Composability

DarkWow contracts compose through **cross-contract child calls** within a single DarkTree transaction. This is the mechanism that lets four independent contracts (identity, labor_market, attestation, dao_escrow) form a complete hiring pipeline — each doing one thing well, combined through child calls into something greater than the sum of its parts.

## How It Works

### DarkTree / DarkForest Transaction Structure

Every transaction is a flat vector of contract calls (`Vec<DarkLeaf<ContractCall>>`), organized as a forest of trees in DFS post-order. Each call specifies its own `contract_id`, so child calls can target different contracts than their parent:

```
┌─────────────────────────────────────────────────────────────────┐
│                    DarkTree Transaction                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  root_call (labor_market::AcceptJobWithCapabilityV1 0x0d)        │
│  ├── children_indexes: [1]                                       │
│  │                                                               │
│  child[1] (identity::VerifyCapabilityV1 0x0b)                    │
│      └── children_indexes: []                                    │
│                                                                   │
│  Flattened (DFS post-order): [identity_call, root]                │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

Key properties:
- **MAX_TX_CALLS = 20**: Up to 20 contract calls per transaction.
- **Per-call contract_id**: Each `ContractCall` carries its own target contract, so children can call different contracts.
- **Atomic execution**: If any child call fails, the entire transaction reverts. No partial state mutations.

### Child Call Validation Pattern

Every contract that accepts child calls follows the same validation pattern in its `instruction` phase (before state is mutated):

```rust
// Validate child call exists
let this_call = &calls[call_idx];
if this_call.children_indexes.len() != 1 {
    return Err(ContractError::InvalidChildrenIndexes.into())
}

// Validate child call targets the expected function
let child_idx = this_call.children_indexes[0];
let child_call = &calls[child_idx].data;
if child_call.data[0] != EXPECTED_FUNCTION_CODE {
    return Err(ContractError::InvalidChildCall.into())
}
```

This check happens **before apply** — if the child call has the wrong function code, the transaction is rejected atomically before any state is mutated.

**Notable detail**: Contracts validate `child_call.data[0]` (the function code byte of the child), not `child_call.contract_id`. This means the calling contract specifies *what function* must be called, but the *which contract* is determined by the transaction builder and enforced by the WASM runtime routing the call.

### Promissory Note Child Call Amount Validation

When a promissory_note::TransferV1 call is a child of another contract, the parent needs to verify the transfer amount. PromissoryNote's `Output` struct supports optional `public_value` and `public_token_id` fields for this purpose, backed by a TransferOutput_V1 ZK proof that constrains these public values equal the encrypted coin attributes.

```rust
// Full validation pattern for promissory_note child transfers:
let this_call = &calls[call_idx];

// 1. Validate child call exists and targets TransferV1
if this_call.children_indexes.len() != 1 {
    return Err(ContractError::InvalidChildrenIndexes.into())
}
let child_idx = this_call.children_indexes[0];
let child_call = &calls[child_idx].data;
if child_call.data[0] != 0x04 {
    return Err(ContractError::InvalidChildCall.into())
}

// 2. Validate the transfer amount matches expected value
dwow_promissory_note_contract::entrypoint::validate_child_transfer_value(
    &child_call.data,
    params.amount,       // expected amount from parent's params
    None,                // optional token_id check
)?;
```

The `validate_child_transfer_value` helper deserializes the child call's `TransferParamsV1`, verifies each output's `public_value` matches the expected amount, and checks `public_token_id` if provided. This closes the cross-contract composition gap: parent contracts can now verify that child promissory_note transfers actually move the expected value, not just that a TransferV1 call exists.

For contracts that don't want to depend on `dwow_promissory_note_contract` directly, the same validation can be performed inline by deserializing `TransferParamsV1` and checking `output.public_value` manually.

## Current Cross-Contract Call Map

### Labor Market → Other Contracts

| Source Function | Opcode | Child Validates | Target Function |
|---|---|---|---|
| `CreateJobV1` | 0x00 | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `AcceptJobV1` | 0x01 | *none (ZK-only, no child calls)* | — |
| `SubmitDeliverableV1` | 0x02 | `child.data[0] != 0x04` | Attestation::VerifyClaimV1 |
| `SubmitGitDeliverableV1` | 0x03 | `child.data[0] != 0x04` | Attestation::VerifyClaimV1 |
| `ConfirmDeliveryV1` | 0x04 | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `DisputeV1` | 0x05 | `child.data[0] != 0x07` | DAO-Escrow::ProposeClaimV1 |
| `RefundV1` | 0x06 | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `CancelV1` | 0x07 | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `CreateJobWithMilestonesV1` | 0x08 | *none (ZK-only, no child calls)* | — |
| `InitiateDisputeV1` | 0x0b | `child.data[0] != 0x07` | DAO-Escrow::ProposeClaimV1 |
| `CreateJobWithCapabilityV1` | 0x0c | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `AcceptJobWithCapabilityV1` | 0x0d | `child.data[0] != 0x0b` | Identity::VerifyCapabilityV1 |

### DAO-Escrow → Other Contracts

| Source Function | Opcode | Child Validates | Target Function |
|---|---|---|---|
| `PayPremiumV1` | 0x02 | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `VerifyMemberCapabilityV1` | 0x0b | `child.data[0] != 0x0b` | Identity::VerifyCapabilityV1 |
| `ResolveDisputeV1` | 0x0c | `!children_indexes.is_empty()` | Attestation + promissory_note (multi-call) |

### Function Code Constants

| Contract | Function | Code |
|---|---|---|
| Identity | `VerifyCapabilityV1` | 0x0b |
| DAO-Escrow | `ProposeClaimV1` | 0x07 |
| Attestation | `VerifyClaimV1` | 0x04 |
| promissory_note | `TransferV1` | 0x04 |

Note: Attestation::VerifyClaimV1 and promissory_note::TransferV1 both use opcode 0x04. Contracts distinguish them by `contract_id` in the child call's `ContractCall` struct, not by opcode alone. The calling contract only validates the function code byte; the transaction builder sets the correct `contract_id` for each child.

## Security Properties

1. **Atomic execution**: All calls in the DarkTree succeed or the entire transaction reverts. No partial state.

2. **Pre-apply validation**: Child call checks occur in the `instruction` phase. State mutation only happens in the `apply` phase. A bad child call never leaves the contract in an inconsistent state.

3. **Function code gating**: Each contract lists exactly which child function codes it accepts. A malformed transaction with a wrong child function code is rejected before execution.

4. **Anti-replay**: nullifiers in ZK circuits prevent double-submission. DB key checks (e.g. `db_contains_key(disputes_db, dispute_id)`) add a second layer for non-ZK paths.

5. **Capability chaining**: A child call can itself require child calls. For example, `labor_market::AcceptJobWithCapabilityV1 → identity::VerifyCapabilityV1` means the identity contract verifies the capability, and labor_market verifies the identity call was present.

## Example: Composite Transaction

A transaction for accepting a capability-gated job with payment escrow:

```
Transaction { calls: Vec<DarkLeaf<ContractCall>> }
│
├── [0] DarkLeaf<ContractCall> {
│       data: ContractCall { contract_id: identity_cid, data: [0x0b, ...VerifyCapabilityParams] }
│       children_indexes: []
│   }
│
├── [1] DarkLeaf<ContractCall> {
│       data: ContractCall { contract_id: promissory_note_cid, data: [0x04, ...TransferParams] }
│       children_indexes: []
│   }
│
└── [2] DarkLeaf<ContractCall> {
        data: ContractCall { contract_id: labor_market_cid, data: [0x0d, ...AcceptJobParams] }
        children_indexes: [0, 1]
    }
```

Execution order (DFS post-order): identity call → promissory_note transfer → labor market call. The labor market call validates that `calls[0].data[0] == 0x0b` (identity VerifyCapabilityV1) and `calls[1].data[0] == 0x04` (promissory_note TransferV1). All three must succeed or the entire transaction is rejected.

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Recruitment Pipeline Case Study](recruitment_pipeline.md) — full walkthrough of 4 contracts composing to automate hiring
- [Attestation Contract](attestation.md) — consumer of composability for claim verification
- [Labor Market Contract](labor_market.md) — primary cross-contract caller
- [DAO-Escrow Contract](dao_escrow.md) — governance composability with Identity and Attestation
- [Contract Invocation API](../arch/contract_invoke_api.md) — runtime-level details
