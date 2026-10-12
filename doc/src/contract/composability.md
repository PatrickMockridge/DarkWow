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
│  child[1] (identity::VerifyCapabilityV1 0x06)                    │
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

**Notable detail**: A parent checks the child's **function code** (`child_call.data[0]`) **and** its
**contract id** (`validate_child_contract_id`). Both are needed, because two contracts can share an
opcode — `PromissoryNote::TransferV1` and `Attestation::VerifyClaimV1` are both `0x04` — so the
function byte alone does not say which contract was called. This paragraph claimed only the
function byte is checked, and that the contract is left to the transaction builder, until
2026-10-12; `escrow`, `labor_market`, `betting_stake` and `pool_stake` each validate the child's
contract id against the id they hold in their own info tree.

### Promissory Note Child Call Amount Validation

When a `promissory_note::TransferV1` call is a child of another contract, the parent verifies the
transfer amount by **recomputing the Pedersen commitment it expects** and matching it against the
child's outputs. There are no `public_value`/`public_asset_id` fields on `Output` and no
`TransferOutput_V1` circuit — this section described both, and named a
`validate_child_transfer_value` helper, until 2026-10-12.

The real helper is `validate_child_value_commit(child_call_data, expected_value, blind_seed)`:

```rust
use dwow_promissory_note_contract::validation::validate_child_value_commit;

let this_call = &calls[call_idx];

// 1. Validate the child call exists and is a TransferV1
if this_call.children_indexes.len() != 1 {
    return Err(ContractError::InvalidChildrenIndexes.into())
}
let child_idx = this_call.children_indexes[0];
let child_call = &calls[child_idx].data;
if child_call.data[0] != 0x04 {
    return Err(ContractError::InvalidChildCall.into())
}

// 2. Validate the transfer amount against the value the parent expects.
//    `blind_seed` is derived deterministically from the parent's own state
//    (e.g. poseidon_hash([value, nullifier])), so the child's BlindOutput_V1
//    proof commits to the same blind this function recomputes.
validate_child_value_commit(&child_call.data, params.amount, blind_seed)?;
```

It decodes `TransferParamsV1` from `child_call.data[1..]`, converts `blind_seed` to a scalar, and
returns `Ok` if **any** output's `value_commit` equals `pedersen_commitment_u64(expected_value,
value_blind)` — otherwise `PromissoryNoteError::ValueMismatch`. Because the comparison is against a
commitment, the amount stays private: the parent proves it knows the value without publishing it.

For contracts that don't want to depend on `dwow_promissory_note_contract` directly, the same check
can be performed inline by deserializing `TransferParamsV1` and comparing `output.value_commit`
against a locally computed commitment.

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
| `AcceptJobWithCapabilityV1` | 0x0d | `child.data[0] != 0x06` | Identity::VerifyCapabilityV1 |

### DAO-Escrow → Other Contracts

| Source Function | Opcode | Child Validates | Target Function |
|---|---|---|---|
| `PayPremiumV1` | 0x02 | `child.data[0] != 0x04` | promissory_note::TransferV1 |
| `ProposeClaimV1` | 0x07 | `child.data[0] != 0x03` at slot 0 | MultiSig::FinalizeV1 (the governance approval for *this* claim) |

Three rows stood here until 2026-10-12: two described **retired selectors** —
`VerifyMemberCapabilityV1` and `ResolveDisputeV1` do not exist in dao_escrow, whose opcode enum
runs `0x09` straight to `0x0d` (`0x0b`/`0x0c` are unassigned), and it makes no identity child
call at all (see [identity.md](identity.md)) — and the third attributed a promissory-note
transfer to `ProposeClaimV1`, whose single child is a MultiSig approval.

### Function Code Constants

| Contract | Function | Code |
|---|---|---|
| Identity | `VerifyCapabilityV1` | 0x06 |
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

A transaction for accepting a capability-gated job:

```
Transaction { calls: Vec<DarkLeaf<ContractCall>> }
│
├── [0] DarkLeaf<ContractCall> {
│       data: ContractCall { contract_id: identity_cid, data: [0x06, ...VerifyCapabilityParams] }
│       children_indexes: []
│   }
│
└── [1] DarkLeaf<ContractCall> {
        data: ContractCall { contract_id: labor_market_cid, data: [0x0d, ...AcceptJobParams] }
        children_indexes: [0]
    }
```

Execution order (DFS post-order): identity call → labor market call. `accept_job_with_capability_v1`
requires **exactly one** child (`children_indexes.len() != 1`), checks `calls[0].data[0] == 0x06`,
and validates that the child's `contract_id` equals the identity id stored in labor_market's own
info tree. Both must succeed or the transaction is rejected.

This example showed a **two-child** root until 2026-10-12 — identity at `[0]` *and* a
promissory-note transfer at `[1]`, with the root's `children_indexes: [0, 1]` and the identity
call publishing `0x0b` instead of `0x06`. No labor_market entrypoint takes two children: the
capability-gated ones (`AcceptJobWithCapabilityV1`, `CreateJobWithCapabilityV1`) each take one,
and `AcceptJobWithCapabilityV1`'s is the identity call, not a payment. A client built from that
shape is refused with `InvalidChildrenIndexes`.

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [Recruitment Pipeline Case Study](recruitment_pipeline.md) — full walkthrough of 4 contracts composing to automate hiring
- [Attestation Contract](attestation.md) — consumer of composability for claim verification
- [Labor Market Contract](labor_market.md) — primary cross-contract caller
- [DAO-Escrow Contract](dao_escrow.md) — governance composability with Identity and Attestation
- [Contract Invocation API](../arch/contract_invoke_api.md) — runtime-level details
