# Promissory Note — Intermediary Contract Guide

## Overview

This document describes how contracts interact with the Promissory Note (PN) contract.
It is written for **contract authors** who need to integrate PN tokens — sending, receiving,
redeeming, or swapping them — from their own contracts.

PN is the standard DeFi token contract. It implements a complete bearer-instrument lifecycle:
**Token creation → Mint → Transfer → Burn → Redeem → OTC swap**. Every interaction
goes through a child call from the intermediary contract to PN.

**22 of 29 contracts** in the ecosystem interact with PN. This guide covers the
validation requirements, patterns, and reference implementations for each role.

For the PN contract's internal design (circuits, entrypoints, data model), see
[promissory_note.md](promissory_note.md). For the spend hook callback mechanism, see
[Spend Hooks](../arch/zk/spend_hook.md).

---

## Ecosystem Architecture

```
                        ┌──────────────────────┐
                        │  Promissory Note (PN) │
                        │  Opcodes: 0x00–0x05   │
                        └──────┬───────────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                 │
         ┌────▼─────┐   ┌─────▼──────┐   ┌─────▼──────────┐
         │  Issuer   │   │    DEX     │   │  20× Token     │
         │(stablecoin)│   │(OTC swaps) │   │    Movers      │
         └──────────┘   └────────────┘   └────────────────┘
```

### Role Taxonomy

| Role | Description | PN Opcodes Used |
|------|-------------|-----------------|
| **Issuer** | Creates tokens, manages supply, redeems | TransferV1 (0x04), RedeemV1 (0x01), BurnV1 (0x03, via spend hook) |
| **OTC** | Atomic peer-to-peer swaps | TransferV1 (0x04), OtcSwapV1 (0x05) |
| **Token Mover** | Moves existing tokens between participants | TransferV1 (0x04) only |

### PN Opcode Reference

| Opcode | Name | Purpose | Consumers |
|--------|------|---------|-----------|
| 0x00 | TokenMintV1 | Create a new token type | None (anyone can call) |
| 0x01 | RedeemV1 | Redeem coin → zero-value receipt | stablecoin (RedeemStableV1) |
| 0x02 | MintV1 | Mint tokens of existing type | None (mint authority required) |
| 0x03 | BurnV1 | Destroy tokens, publish nullifiers | stablecoin (via spend hook callback) |
| 0x04 | TransferV1 | Private token transfer | **22 contracts** (94% of PN child calls) |
| 0x05 | OtcSwapV1 | Atomic OTC swap | dex (via swap execution) |

### Contracts Not Interacting with PN

Seven contracts have no PN dependency: attestation, deployooor, identity, native_token,
oracle, tender, tau. These are outside the scope of this guide.

---

## Validation Requirements by Role

Every contract that calls PN as a child **must** validate the child call to prevent
cross-contract routing attacks and value manipulation. The minimum validation
varies by role.

### All Roles: Contract ID Validation

**Requirement:** Verify the child call targets the expected PN contract.

```rust
use dwow_promissory_note_contract::validation::validate_child_contract_id;

// Prevent routing attacks: child call must target OUR known PN instance
validate_child_contract_id(&child_call.contract_id, &stored_pn_contract_id)?;
```

The PN contract ID must be stored in the intermediary's config DB at initialization.
Do not hardcode it — different deployments may use different PN instances.

Without this check, a transaction builder could route a child call to a malicious
contract that shares the same opcode (e.g., `0x04` is used by multiple contracts).

### Token Movers: Value Commit Validation

**Requirement:** Verify the child TransferV1 moves the expected amount of tokens.

```rust
use dwow_promissory_note_contract::validation::validate_child_value_commit;

// Derive a deterministic blind seed from this contract's unique state
let blind_seed = poseidon_hash([
    pallas::Base::from(expected_amount),
    contract_specific_state,
]);

// Verify: child call's output value_commit matches expected_amount
validate_child_value_commit(&child_call.data, expected_amount, blind_seed)?;
```

This is **privacy-preserving**: it uses Pedersen commitment comparison rather than
requiring the child to reveal plaintext values. The blind seed is derived deterministically
from the parent contract's state, so the child TransferV1 can compute the same blind
and produce a matching Pedersen commitment.

**Contracts that validate both contract_id AND value_commit:**
stablecoin, bridge. All other token movers validate contract_id only, which is
acceptable when the parent contract controls the amount through its own logic
(e.g., the DEX determines the swap rate in its own entrypoint).

### Issuer: Full Lifecycle Validation

The stablecoin is the sole issuer contract. It implements the complete lifecycle:

| Operation | PN Child Call | Validation |
|-----------|--------------|------------|
| MintStableV1 | TransferV1 (0x04) | contract_id + value_commit |
| RepayStableV1 | TransferV1 (0x04) | contract_id + value_commit |
| RedeemStableV1 | RedeemV1 (0x01) | contract_id + receipt validation |
| Burn (spend hook) | BurnV1 (0x03) callback | caller PN contract_id + nullifier replay |

For RedeemV1 specifically, the issuer calls `validate_child_redeem_v1` to extract
the receipt coin's value_commit and token_commit:

```rust
use dwow_promissory_note_contract::validation::validate_child_redeem_v1;

let (receipt_value_commit, receipt_token_commit) =
    validate_child_redeem_v1(&child_call.data)?;
```

### spend_hook Policy

**Token movers must set `spend_hook = pallas::Base::zero()`** on all output coins.
A non-zero spend_hook means "this coin can only be burned through the specified
contract." Token movers are not issuers — they don't restrict how tokens are used.

**Issuers set spend_hook to their own contract ID** on minted coins. The
spend_hook callback mechanism (see below) allows the issuer to track burns
and redemptions without requiring every token mover to call BurnV1 directly.

---

## Reference Implementations

### Stablecoin — Full Lifecycle Issuer

**Source:** [`src/contract/stablecoin/src/`](../../src/contract/stablecoin/src/)

The stablecoin is the **reference implementation** for an issuer contract. It demonstrates:

- **Minting** via `MintStableV1 (0x04)`: calls PN::TransferV1 as a child, sets `spend_hook = self` on output coins. Collateral is held in the stablecoin contract.
- **Repayment** via `RepayStableV1 (0x04)`: calls PN::TransferV1 to return stablecoins to the contract.
- **Redemption** via `RedeemStableV1 (0x0A)`: calls PN::RedeemV1 (0x01) as a child to destroy stablecoins and create a zero-value receipt. Releases proportional collateral. Updates `total_redeemed`.
- **Burn tracking** via spend hook callback (`SpendHookCallback 0x0B`): receives callbacks from PN::BurnV1, records nullifiers, increments `total_redeemed`.

The entrypoint uses `define_contract_with_spend_hook!` to export the `__spend_hook`
WASM function. `SpendHookCallback` is an internal opcode reachable **only** via
`__spend_hook` — calling it through `exec()` returns an error.

**Balance sheet tracking** (all on-chain, in the config DB):

| Key | Purpose |
|-----|---------|
| `total_debt` | Total stablecoins minted |
| `total_collateral` | Total collateral deposited |
| `total_redeemed` | Total stablecoins redeemed (incremented by both RedeemStableV1 and spend hook callbacks) |

The `GovernanceReportV1` cold path reads all three counters from on-chain state,
computes `outstanding = total_debt - total_redeemed`, and enforces
`total_collateral >= outstanding` — preventing fractional reserve. Reports are
persisted in the `governance_reports` tree for public auditability.

**Key files:**
- [`entrypoint.rs:103`](../../src/contract/stablecoin/src/entrypoint.rs) — `define_contract_with_spend_hook!`
- [`entrypoint.rs:492–543`](../../src/contract/stablecoin/src/entrypoint.rs) — `process_spend_hook()`
- [`entrypoint.rs:630–652`](../../src/contract/stablecoin/src/entrypoint.rs) — `apply_spend_hook_callback()`
- [`entrypoint.rs:1362`](../../src/contract/stablecoin/src/entrypoint.rs) — `process_redeem_stable_instruction()`
- [`lib.rs:156`](../../src/contract/stablecoin/src/lib.rs) — `STABLECOIN_CONTRACT_TOTAL_REDEEMED`

### Bridge — Cross-Chain Token Mover

**Source:** [`src/contract/bridge/src/`](../../src/contract/bridge/src/)

The bridge is a token mover with special cross-chain semantics. It uses
TransferV1 (0x04) exclusively for PN interaction:

- **Deposit:** Mints wrapped tokens via PN::TransferV1 child call. The actual
  deposit exists on the external chain (BTC, XMR, ZEC, etc.).
- **Withdrawal:** Burns wrapped tokens via PN::TransferV1 child call, transferring
  them to the bridge contract. The actual release happens on the external chain.

The bridge validates **both** `validate_child_contract_id` and `validate_child_value_commit`.

Unlike the stablecoin, the bridge cannot verify collateral coverage on-chain
since the collateral lives on external chains. The `GovernanceReportV1` proves
internal accounting consistency only: `outstanding = total_deposited - total_withdrawn`.

**The bridge does not use BurnV1 or RedeemV1.** Its "burns" are simulated by
transferring tokens to the bridge contract. The external chain is the source of
truth for the actual release. This is architecturally correct for a bridge.

### DEX — OTC Swaps

**Source:** [`src/contract/dex/src/`](../../src/contract/dex/src/)

The DEX uses TransferV1 (0x04) for swap execution and OtcSwapV1 (0x05) for the
actual atomic swap. It validates `contract_id` indirectly through the otc_swap
mechanism. No value_commit validation is performed on individual transfers —
the swap rate is determined by the DEX's own order matching logic.

---

## Validation Helpers

**Source:** [`src/contract/promissory_note/src/validation.rs`](../../src/contract/promissory_note/src/validation.rs)

PN exports three validation helpers for parent contracts. They are always
compiled (not behind `no-entrypoint`) so caller contracts can import them
regardless of feature flags.

### `validate_child_contract_id`

```rust
pub fn validate_child_contract_id(
    child_contract_id: &ContractId,
    expected_contract_id: &ContractId,
) -> Result<(), crate::ContractError>
```

Prevents cross-contract routing attacks. Call this after verifying the child
call's opcode byte.

### `validate_child_value_commit`

```rust
pub fn validate_child_value_commit(
    child_call_data: &[u8],
    expected_value: u64,
    blind_seed: pallas::Base,
) -> Result<(), crate::ContractError>
```

Privacy-preserving amount verification. Parses `TransferParamsV1` from the
child call data and compares the Pedersen value commitment against the
expected amount. The `blind_seed` must be derived deterministically from the
parent contract's unique state — the child TransferV1 must use the same derivation
when generating its BlindOutput_V1 ZK proof.

Only works for **TransferV1 (0x04)** child calls. Call after verifying
`child_call.data[0] == 0x04`.

### `validate_child_redeem_v1`

```rust
pub fn validate_child_redeem_v1(
    child_call_data: &[u8],
) -> Result<(pallas::Point, pallas::Base), crate::ContractError>
```

Parses `RedeemParamsV1` from the child call data and returns the receipt coin's
`(value_commit, token_commit)` tuple. The parent contract can inspect these to
verify the receipt coin's properties.

The ZK circuit (`redeem_v1.zk`) constrains `coin_value = 0` as a public input
and exposes `coin_spend_hook` — the host verifies the ZK proof, so the parent
does not need to independently verify the zero-value property.

Only works for **RedeemV1 (0x01)** child calls. Call after verifying
`child_call.data[0] == 0x01`.

---

## Spend Hook Mechanism

The spend hook mechanism allows **issuer contracts** to receive notifications
when their tokens are burned. It is a composability primitive — PN does not
know or care what the target contract does with the notification.

### How It Works

1. **Issuer sets spend_hook on minted coins** — when the stablecoin mints tokens
   via PN::TransferV1, it sets `spend_hook = stablecoin_contract_id` on the output coin.
   This is exposed as a public input in all output-creating ZK circuits.

2. **Token mover burns tokens** — any holder calls PN::BurnV1. PN checks that all
   inputs share the same `spend_hook` (rejects with `SpendHookMismatch` if they differ).

3. **PN dispatches callback** — when `spend_hook != 0`, PN builds a
   `BurnSpendHookPayload` containing the nullifiers, token_commits, value_commits,
   and user_data_encs, then calls `emit_spend_hook(target_cid, payload)`.

4. **Host executes callback** — the blockchain pipeline (see `execution.rs:232-239`)
   detects the `spend_hook_request` on the WASM runtime context and calls
   `__spend_hook` → `apply()` on the target contract **in the same overlay**.
   If the callback fails, the entire transaction reverts — the burn and callback
   are atomic.

5. **Issuer records the burn** — the target contract's `process_spend_hook()`
   validates the caller (must be the PN contract), checks nullifiers for replay,
   and produces update data. `apply_spend_hook_callback()` persists the nullifiers
   and updates accounting (e.g., increments `total_redeemed`).

### Who Should Implement Spend Hooks

- **Issuers** — to track burns/redemptions and maintain balance sheet integrity.
- **No one else** — token movers should set `spend_hook = 0` on all outputs.
  Non-issuer contracts should never receive spend hook callbacks.

### Reference Implementation

The stablecoin's `SpendHookCallback (0x0B)` is the canonical example:

- Exported via `define_contract_with_spend_hook!` at [`entrypoint.rs:103`](../../src/contract/stablecoin/src/entrypoint.rs)
- `process_spend_hook()` at line 492 validates caller, checks nullifier replay
- `apply_spend_hook_callback()` at line 630 records nullifiers, increments `total_redeemed`
- Guarded: calling `SpendHookCallback` via `exec()` returns `InvalidProof`

---

## Redemption Lifecycle

The redemption lifecycle is the complete path from token creation to destruction:

```
TokenMintV1 (0x00)     MintV1 (0x02)        TransferV1 (0x04)    RedeemV1 (0x01)
─────────────────►  ───────────────►  ...  ──────────────────►  ───────────────►
Create token type    Mint tokens            Transfer tokens       Redeem → receipt
(anyone)             (mint authority)       (coin holder)         (issuer)
```

### Redemption via RedeemV1

`RedeemV1` burns a coin and creates a **zero-value receipt coin** as proof of
redemption. The ZK circuit (`redeem_v1.zk`) constrains `coin_value = 0` using
the `is_notequal` gate — this proves the receipt has no monetary value without
revealing the original coin's value.

The receipt coin's `spend_hook` is set to the issuer contract ID, making it
**non-transferable** — only the issuer can interact with receipt coins.

The stablecoin's `RedeemStableV1` is the sole consumer of RedeemV1:

1. User requests redemption of N stablecoins
2. `RedeemStableV1` calls `PN::RedeemV1` as a child call
3. PN burns the stablecoin, creates a receipt coin
4. Stablecoin verifies the receipt via `validate_child_redeem_v1`
5. Stablecoin releases proportional collateral: `collateral_return = (redeem_amount * total_collateral) / total_debt`
6. Stablecoin updates `total_debt` and `total_redeemed`

### Redemption via Spend Hook (Burn)

When a token holder directly calls `PN::BurnV1` (0x03), the spend hook mechanism
notifies the issuer. The issuer increments `total_redeemed` in the callback.
This path does not release collateral — it is a pure burn. Only `RedeemStableV1`
releases collateral.

### Receipt Coins

Receipt coins are **capability tokens** (capability type `CAP_RECEIPT = 0x02`).
They are non-consumable and non-transferable. Their purpose is to serve as
on-chain proof that redemption occurred, for governance audit trails.

---

## Contract Inventory

All 22 PN-interacting contracts, with their current validation status.

### Issuer

| Contract | PN Opcodes | Validates Contract ID | Validates Value Commit | spend_hook | Redemption |
|----------|------------|----------------------|------------------------|------------|------------|
| **stablecoin** | 0x04, 0x01, 0x03 (callback) | Yes | Yes | Implemented | RedeemStableV1 |

### OTC

| Contract | PN Opcodes | Validates Contract ID | Validates Value Commit | spend_hook |
|----------|------------|----------------------|------------------------|------------|
| **dex** | 0x04, 0x05 | Indirect (via otc_swap) | No | None |

### Token Movers (20 contracts)

| # | Contract | PN Opcodes | Validates Contract ID | Validates Value Commit |
|---|---|-----------|----------------------|------------------------|
| 1 | **bridge** | 0x04 | Yes (gated on config) | Yes |
| 2 | **darkbet_exchange** | 0x04 | Yes | No |
| 3 | **labor_market** | 0x04, 0x07, 0x0b | Yes | No |
| 4 | **lottery** | 0x04 | Yes | No |
| 5 | **baccarat** | 0x04 | Yes | No |
| 6 | **darktoshi_dice** | 0x04 | Yes | No |
| 7 | **roulette** | 0x04 | Yes | No |
| 8 | **slot** | 0x04 | Yes | No |
| 9 | **betting_stake** | 0x04 | Yes | No |
| 10 | **pool_stake** | 0x04 | Yes | No |
| 11 | **game_room** | 0x04 | Yes | No |
| 12 | **escrow** | 0x04 | Yes | No |
| 13 | **auction** | 0x04 | Yes | No |
| 14 | **dao_escrow** | 0x04, 0x0b | Yes | No |
| 15 | **drain_protection** | 0x04 | Yes | No |
| 16 | **insurance_market** | 0x04 | Yes | No |
| 17 | **otc_swap** | 0x04 | Yes | No |
| 18 | **relayer_endowment** | 0x04 | Yes | No |
| 19 | **subscription** | 0x04 | Yes | No |
| 20 | **dex** | 0x04, 0x05 | Indirect | No |

**Universal pattern:** All token movers use TransferV1 (0x04) exclusively for PN
interaction. All validate `validate_child_contract_id`. Some validate value_commit.
None set `spend_hook` on outputs. None participate in mint/redeem — they only move
existing tokens between participants. This is correct: TransferV1 is the only opcode
they need.

### Ecosystem Metrics

| Metric | Count |
|--------|-------|
| Total contracts | 29 |
| PN-interacting | 22 |
| Issuers | 1 (stablecoin) |
| Token movers | 20 (+ dex as OTC) |
| Independent (no PN) | 7 |
| Using TokenMintV1 (0x00) | 0 |
| Using RedeemV1 (0x01) | 1 (stablecoin) |
| Using MintV1 (0x02) | 0 |
| Using BurnV1 (0x03) | 1 (stablecoin, via spend hook) |
| Using TransferV1 (0x04) | 22 |
| Using OtcSwapV1 (0x05) | 1 (dex) |
| Contracts with redemption support | 1 (stablecoin) |
| Contracts with balance sheet tracking | 2 (stablecoin, bridge) |

---

## Security Model

### Threats Mitigated by Design

**Cross-contract routing attacks.** Every intermediary validates `child_contract_id`
to ensure the child call targets the expected PN instance. Without this, a
transaction builder could route a child call to a malicious contract sharing
the same opcode.

**Value inflation.** TransferV1 and OtcSwapV1 enforce cross-proof value conservation
via Pedersen additive homomorphism: `sum(input value_commits) == sum(output value_commits)`
per token_commit group. This prevents creating tokens out of thin air within a transfer.

**Unauthorized minting.** MintV1 requires a ZK proof demonstrating knowledge of the
mint backing secret (`mint_public = poseidon_hash(backing_secret)` constrained in-circuit
at `mint_v1.zk:41-43`). The entrypoint verifies `mint_public == stored_token_auth_parent`
against the on-chain token registry.

**Double-spending.** Nullifiers are tracked in a Sparse Merkle Tree. Every burn/transfer
checks that the nullifier is not already spent before accepting the transaction.

**Signature separation.** The burn circuit derives `signature_secret = poseidon_hash(coin_secret, nullifier)`
in-circuit (`burn_v1.zk:83-84`), cryptographically binding the transaction signer to the
coin owner. Each burn produces a unique `signature_public`, preserving privacy across burns.

**Spend hook replay.** The stablecoin's `process_spend_hook()` records nullifiers and
rejects replays. The callback runs in the same overlay as the burn for atomicity —
a failed callback reverts the entire transaction.

**Fractional reserve.** The stablecoin's `GovernanceReportV1` reads all three balance
sheet counters from on-chain config DB, computes outstanding, and enforces
`total_collateral >= outstanding`. Reports are persisted on-chain for public audit.

### Residual Risks

**TokenMintV1 has no authority check.** Anyone can call TokenMintV1 to create a new
token type. This is by design — token creation is permissionless. The security boundary
is at MintV1 (minting tokens of an existing type requires the mint authority).

**Same-block double-spend.** Execution overlays are cloned per-call (`execution.rs:147`),
so two transactions spending the same nullifier in one block would both pass exec-phase
checks. The merge phase (`execution.rs:296`) silently uses the last writer. A proper
fix requires either nullifier-aware mempool deduplication or key-conflict detection
in the overlay merge phase. This is a consensus-layer issue tracked in the execution
engine, not a PN contract issue.

**Bridge deposit verification.** The bridge's WithdrawV1 ZK circuit
(`withdraw_v1.zk:46`) constrains `merkle_root_val` as a public input, proving the
deposit leaf exists in a Merkle tree. However, the on-chain entrypoint
(`entrypoint.rs:797-799`) assigns `_deposits_db` (underscore = unused) — deposit
existence in the bridge's on-chain tree is not independently verified. The comment
acknowledges this: "In production, we would verify the merkle proof here."
Additionally, the metadata function (`withdraw_get_metadata`, line 304-306) provides
4 public inputs (nullifier, deposit_leaf, derived_recipient, token_minimum) while the
ZK circuit exposes 5 (`constrain_instance` calls at lines 28, 39, 46, 50, 57) —
the `merkle_root_val` from circuit line 46 may not be wired through host verification.
For a future upgrade, the bridge should: (a) ensure `merkle_root_val` is part of the
metadata public inputs, and (b) verify it against the stored deposit tree root on-chain.

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
