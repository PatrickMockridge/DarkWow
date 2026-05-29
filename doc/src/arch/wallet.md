# Wallet Architecture

The DarkWow wallet (`dwow_wallet`) is a **full node** — it holds the complete
blockchain on local disk and derives all state from local data. There is no SPV,
no light client, no network fetches for position resolution. Every query is pure
local computation over sled trees and SQLite tables.

The wallet works with two coin contracts:

- **Promissory Note** — the rich bearer-instrument contract supporting the full
  lifecycle: create, mint, transfer, redeem, burn, and OTC swap. Most user-issued
  tokens are PN-based.
- **Native Token** — a rock-dumb token contract that only does basic transfers
  and fee payments. Its sole function is `FeeV1 (0x00)`, used by every transaction
  to pay network fees in the native token.

The wallet's primary integration is with Promissory Note, which exposes six
lifecycle functions through the `Drk` struct.

## Data Stores

The `Drk` struct holds two databases:

| Store | Type | Contents |
|---|---|---|
| `cache: Cache` | sled (embedded KV) | Block pointers, Merkle tree checkpoints, per-contract state trees (escrows, nullifiers, spent flags), PN nullifier SMT, PN coin Merkle tree |
| `wallet: WalletPtr` | SQLite | User coins, secrets, addresses, deploy authorities, contract registry, contract metadata, contract interactions, transaction history, token registry |

Both are at paths configured in `dww_config.toml` (`cache_path`, `wallet_path`).

## Promissory Note Integration

Promissory Note is the primary contract for user-issued tokens. The wallet
exposes the complete PN bearer-instrument lifecycle through six functions:

| # | Function | Opcode | Wallet Method | Description |
|---|----------|--------|---------------|-------------|
| 1 | TokenMintV1 | `0x00` | `create_token()` | Create a new token type with mint authority |
| 2 | RedeemV1 | `0x01` | `redeem()` | Redeem a coin with the issuer — destroys value, creates receipt |
| 3 | MintV1 | `0x02` | `mint_tokens()` | Mint more coins of an existing token type |
| 4 | BurnV1 | `0x03` | `burn()` | Destroy coins, publish nullifiers |
| 5 | TransferV1 | `0x04` | `transfer()` | Transfer coins to a recipient (atomic burn + blind output) |
| 6 | OtcSwapV1 | `0x05` | `init_swap()` / `join_swap()` | Atomic peer-to-peer token swap |

### Transaction Flow

PN transactions follow a common pattern:

```
1. Look up coin(s) from wallet DB (by token_id or coin_id)
2. Fetch secrets, Merkle proof, leaf position, coin blind
3. Build client-side call input with ZK witness data
4. Load ZK binary, create proving key, generate proofs
5. Encode params, wrap in ContractCall → ContractCallLeaf
6. Attach fee call (NativeToken FeeV1) via TransactionBuilder
7. Return signed Transaction
```

This pattern is implemented in `transfer()`, `redeem()`, `burn()`, and
`join_swap()`. Each reuses the same ZK circuits — `Burn_V1` and
`BlindOutput_V1` — for both PN and NativeToken fee operations.

### CLI Commands

```
dwow_wallet transfer <amount> <token> <recipient> [--spend-hook <cid>] [--user-data <data>] [--half-split]
dwow_wallet redeem <coin_id> [--spend-hook <contract_id>]
dwow_wallet burn <coin_id> [<coin_id>...]
dwow_wallet otc init <amount> <token> <receive_amount> <receive_token>
dwow_wallet otc join          # reads both swap halves from stdin
dwow_wallet otc inspect       # reads swap JSON from stdin
dwow_wallet otc sign <coin_id> <value> <token> <receive_value> <receive_token>
dwow_wallet token create <name> <supply> <decimals>
dwow_wallet token mint <token_id> <amount>
```

## Block Scanning

At startup (or via `dwow_wallet scan`), the wallet fetches blocks from its
local peer via JSON-RPC (`blockchain.get_block_linear`). For each transaction
in each block, contract-specific handlers decrypt notes to discover coins
belonging to the wallet:

```
for each unseen block:
    for each transaction:
        for each contract call:
            route to handler by contract_id + function opcode
            decrypt notes with wallet secrets
            if ours → insert CoinRecord + MerkleProof
    update trees, flush database
```

PN function handlers recognize all six opcodes (0x00 through 0x05). Discovered
coins are stored in the `coins` table with their value, token, spend hook,
user data, leaf position in the Merkle tree, secrets, blinds, spent status, and
creation height.

## Capability-Based Position Resolution

Above the coin layer, the wallet interprets on-chain state as **capabilities**
— things the user can do. This is the position resolution system.

### Core Idea

Everything that authorizes an action is a capability:
- **Coins** — "I can spend this coin" (PN `CAP_COIN`)
- **Mint authorities** — "I can mint tokens of this type" (PN `CAP_MINT_AUTHORITY`)
- **Receipt coins** — "I redeemed this coin" (PN `CAP_RECEIPT`, non-consumable)
- **Contract roles** — "I am the Creator of escrow X in Funded state"
- **ZK credentials** — "I hold an unrevoked identity credential"
- **DAO memberships** — "I paid the premium and it hasn't expired"

Actions (contract function calls) **require** capabilities, **consume** some
(via nullifiers), and **produce** new ones.

### Types (defined in `dwow_sdk::capability`)

**`CapabilityId`** — 32-byte identifier derived deterministically from
`(contract_id, capability_type, instance_id)` via Poseidon hash. This means
capability instances can be matched without storing them — re-derive and compare.

**`CapabilitySource`** — how the resolver derives this capability from
on-chain facts:
- `Coin { coin_id }` — spendable coin
- `Role { state, role, instance_id }` — contract role in a specific state
- `ZkCredential { credential_id, nullifier, revoked }` — identity credential
- `Membership { membership_id, expiry }` — DAO membership

**`Capability`** — a capability the user holds:
- `id: CapabilityId` — unique identifier
- `contract_id: ContractId` — owning contract
- `description: String` — human-readable label
- `source: CapabilitySource` — derivation method
- `consumable: bool` — true if exercising it consumes it (nullifier)
- `expires_at: Option<u64>` — block height when it expires

**`CapabilityExpression`** — boolean expression over capabilities required to
authorize an action:
- `Any(Vec<CapabilityId>)` — any one is sufficient (OR)
- `All(Vec<CapabilityId>)` — all required (AND)
- `Not(Box<CapabilityExpression>)` — must NOT hold
- `Threshold { capabilities, count, total }` — voting quorum

**`Action`** — a contract function the user is authorized to call:
- `function_id: u8` — opcode byte
- `name: String` — "TransferV1", "RedeemV1", etc.
- `contract_id: ContractId` — target contract
- `description: String` — human-readable
- `requires: CapabilityExpression` — what must be held
- `consumes: Vec<CapabilityId>` — nullified on execution
- `produces: Vec<CapabilityOutput>` — gained on execution

**`CapabilityDescriptor`** — a contract's declaration of what actions exist and
what capabilities they require, consume, and produce. Each contract provides one
descriptor (e.g. `src/contract/promissory_note/src/capability.rs`,
`src/contract/escrow/src/capability.rs`).

### Resolution Algorithm

`CapabilityResolver::resolve(&self, wallet, cache)` runs locally:

```
1. Collect user's public keys from wallet.get_addresses()

2. Derive coin capabilities:
   for each unspent coin in wallet.get_coins(false):
       is_receipt = value == 0 && spend_hook is set
       type = is_receipt ? CAP_RECEIPT(0x02) : CAP_COIN(0x00)
       Capability { source: Coin { coin_id }, consumable: !is_receipt }

3. Per-contract resolution (single pass per sled tree or token registry):
   for each registered descriptor:
       match desc.name:
           "promissory_note" → scan token registry for mint authorities
           "escrow" → scan sled tree for escrow instances
           ...

4. Return PositionResult { capabilities, available_actions }
```

Each contract gets **one resolver method** that scans its data **once** and
produces both capabilities and actions. There is no second pass, no template
substitution, no dead code.

### Promissory Note Capabilities

Promissory Note is both the coin contract AND the primary capability source.
Its descriptor (`src/contract/promissory_note/src/capability.rs`) defines
three capability types and six actions:

| Capability | Discriminant | Source | Consumable |
|------------|-------------|--------|------------|
| Spendable Coin | `0x00` | Unspent coin in wallet | Yes |
| Mint Authority | `0x01` | Knows mint_secret for token_id | No |
| Receipt Coin | `0x02` | Unspent coin with value=0 | No |

**Actions declared:**

| Action | Function ID | Requires | Consumes | Produces |
|--------|------------|----------|----------|----------|
| TransferV1 | `0x04` | Any coin | Coin | New coin |
| BurnV1 | `0x03` | Any coin | Coin | — |
| RedeemV1 | `0x01` | Any coin | Coin | Receipt coin |
| MintV1 | `0x02` | Mint authority | — | New coin |
| TokenMintV1 | `0x00` | Mint authority | — | Initial coin |
| OtcSwapV1 | `0x05` | Any coin | Coin | Swapped coin |

**Coin capability derivation** (`derive_coin_capabilities`):
- Each unspent coin with `value > 0` → `CAP_COIN (0x00)`, consumable, "Coin worth X"
- Each unspent coin with `value == 0` and `spend_hook` set → `CAP_RECEIPT (0x02)`, non-consumable, "Receipt for token"

**Mint authority derivation** (`resolve_promissory_note`):
- Scans `wallet.get_all_tokens()` for tokens with `mint_authority` set
- For each token → `CAP_MINT_AUTHORITY (0x01)`, derives `MintV1` and `TokenMintV1` actions
- Token symbol used in descriptions (e.g. "Mint authority for TOKEN")
- If token is frozen, `expires_at` is set to the freeze height

### Escrow Reference Implementation

The escrow contract (`src/contract/escrow/`) is the reference for role-based
resolution. Its state machine:

```
Created ──[Fund]──> Funded ──[Claim]──> Claimed
                  │                │
                  │                └──[Refund]──> Refunded
                  │
                  └──[Cancel]──> Cancelled
```

**Capability type discriminants** (defined in `escrow/src/capability.rs`):
- `0x00` — Creator in Created state
- `0x01` — Counterparty in Created state
- `0x02` — Creator in Funded state
- `0x03` — Counterparty in Funded state

**Resolution** (`bin/drk/src/capability.rs::resolve_escrow`):
- Opens `cache.db.open_tree(escrow_cid.hash_state_id("escrows"))`
- Deserializes each `Escrow` entry
- Matches `buyer_pubkey` / `seller_pubkey` against user's pubkeys
- For each match, derives capabilities and actions based on (role, state):

| State | Role | Capability | Available Actions |
|---|---|---|---|
| Created | Buyer (Creator) | `0x00` Creator+Created | CancelEscrow (0x05) |
| Created | Seller (Counterparty) | `0x01` Counterparty+Created | FundEscrow (0x02) |
| Funded | Buyer (Creator) | `0x02` Creator+Funded | RefundEscrow (0x04) |
| Funded | Seller (Counterparty) | `0x03` Counterparty+Funded | ClaimEscrow (0x03) |
| Claimed/Refunded/Cancelled | either | *(terminal — none)* | *(none)* |

## CLI: `dwow_wallet position`

The `Position` subcommand loads descriptors, instantiates the resolver, and
prints the user's current position:

```
$ dwow_wallet position

=== Held Capabilities ===
  6S2nMh1... — Coin worth 1000 [consumable]
  4Rt9xB2... — Mint authority for TOKEN
  7Yp3kL5... — Receipt for token f3a8b1c0...
  3Kj9xP7... — Creator of escrow 8Rm4wQ2... (Funded) [consumable]

=== Available Actions ===
  promissory_note::TransferV1 (0x04) — Transfer coins to a recipient
  promissory_note::BurnV1 (0x03) — Burn coins — destroy value, publish nullifiers
  promissory_note::MintV1 (0x02) — Mint new coins of TOKEN
  escrow::RefundEscrow (0x04) — Refund escrow 8Rm4wQ2...
  escrow::ClaimEscrow (0x03) — Claim escrow 8Rm4wQ2...
```

## Spend Hook Callback Awareness

When a coin has a `spend_hook` set (non-zero), the PN contract dispatches a
child call to the hook contract during transfer or burn. The wallet surfaces
this through:

- **`Drk::check_spend_hook(contract_id)`** — looks up the hook contract in the
  contract registry and returns its name and category. If the contract is
  unknown, the caller should warn the user before sending coins to it.
- **Configurable function code** — `create_spend_hook_call()` accepts an
  optional function code (defaults to `0x00` for generic callback dispatch).
  Contracts that expect a specific dispatch code (e.g. stablecoin uses `0x0B`
  for `SpendHookCallback`) can specify it.
- **Transaction builder child tree** — the spend_hook call is attached as a
  child of the transfer/burn call leaf, ensuring atomic execution in the same
  overlay.

For details see [Spend Hook Callback](../arch/zk/spend_hook.md).

## Adding a New Contract Resolver

When adding capability resolution for a new contract:

1. **Create a descriptor** in the contract crate (e.g. `src/contract/<name>/src/capability.rs`):
   - Define capability type discriminants (u8 constants)
   - Implement `pub fn descriptor(contract_id) -> CapabilityDescriptor`
   - Declare actions with their require/consume/produce expressions

2. **Add a resolver method** in `bin/drk/src/capability.rs`:
   - Single method: `resolve_<name>(&self, cid, cache/wallet, ..., &mut capabilities, &mut actions)`
   - One pass over the contract's sled tree or database table
   - Match user pubkeys against stored participant keys
   - Derive capabilities and actions for each instance

3. **Register** in `resolve()`:
   ```rust
   if desc.name == "my_contract" {
       if let Some(cid) = crate::contract_imports::MY_CONTRACT_ID.get() {
           self.resolve_my_contract(*cid, cache, ..., &mut capabilities, &mut actions);
       }
   }
   ```

4. **Register the descriptor** in `main.rs` (Position handler):
   ```rust
   if let Some(cid) = dwow_wallet::contract_imports::MY_CONTRACT_ID.get() {
       resolver.register_descriptor(dwow_my_contract::capability::descriptor(*cid));
   }
   ```

### Bearer Bond Resolver

Bearer Bond is a profit-share staking contract where stake coins are tradeable
capital positions. The resolver scans the contract's `coins` tree (sled) to
discover BondCoin instances owned by the wallet. Five capability types:

| Capability | Discriminant | Derivation |
|---|---|---|
| `CAP_STAKE` | `0x00` | BondCoin owned by wallet (consumable — transfer burns old coin) |
| `CAP_PROFIT_RIGHT` | `0x01` | Unclaimed profit declarations in `bonds_info` tree since `last_claim_block` |
| `CAP_UNSTAKE_RIGHT` | `0x02` | Always derived (contract enforces maturity on-chain) |
| `CAP_RECEIPT` | `0x03` | Receipt coin after unstaking (non-transferable) |
| `CAP_COVERAGE_REPORT` | `0x04` | Governance — issuer proved reserves >= outstanding stake |

**Ownership check:** `poseidon_hash([secret.inner()]) == BondCoin.signature_public`.
Unlike contracts that store a raw `PublicKey`, bearer bond uses hashed pubkeys
matching Promissory Note's ZK privacy model.

**Profit right detection:** The resolver performs a secondary scan of the
`bonds_info` tree. For each `ProfitDeclaration` record matching the coin's
`token_commit` where `end_block > last_claim_block`, a `CAP_PROFIT_RIGHT`
capability is derived.

**Per-coin actions:**
- `TransferStakeV1` (0x01) — always available while holding `CAP_STAKE`
- `ClaimProfitsV1` (0x03) — requires `CAP_STAKE` + `CAP_PROFIT_RIGHT`
- `UnstakeV1` (0x04) — requires `CAP_STAKE` + `CAP_UNSTAKE_RIGHT`

## Database Schema

The wallet SQLite database (`wallet_path`) schema is defined in
`bin/drk/wallet.sql`. Key tables:

| Table | Purpose |
|---|---|
| `addresses` | User public keys, secrets, default flag |
| `coins` | All discovered coins with value, token, secrets, blinds, spent status |
| `coin_secrets` | Spend-authorizing secrets keyed by coin_id |
| `coin_merkle_proofs` | Merkle proofs for coin inclusion |
| `tokens` | Token metadata (name, symbol, decimals, mint authority, freeze status) |
| `transactions_history` | Sent transactions with status |
| `deploy_authorities` | Deploy keys for contracts the user deployed |
| `contract_registry` | Known contract name → contract_id mappings |
| `contract_metadata` | On-chain metadata (name, symbol, category, deployer, attestations) |
| `contract_interactions` | Record of every contract function the user called |
| `scanned_blocks` | Last scanned block height (for resume) |
| `aliases` | Human-readable token aliases |

## Testing

Wallet capability resolution is tested across three levels, mirroring the
four-level contract testing taxonomy:

### Level 1 — Bash CLI integration

[`bin/drk/test_capability_lightweight.sh`](../../../bin/drk/test_capability_lightweight.sh)
tests the `dwow_wallet position` subcommand end-to-end:

- Subcommand registration and help text
- Error handling: missing config, corrupt config, no running node
- End-to-end: start `dwowd` in devnet mode, mine blocks, scan, verify position output format

No ZK proofs, no Docker. Runtime: ~30 seconds.

### Level 2 — Rust resolver logic (in-process)

`#[cfg(test)] mod tests` at the bottom of
[`bin/drk/src/capability.rs`](../../../bin/drk/src/capability.rs) — 20+ tests
covering the full phase space:

| Category | Tests | Scenarios |
|---|---|---|
| Empty / null state | 2 | Empty wallet, no descriptors registered |
| Coin capabilities | 2 | Multiple coins, invalid bs58 coin_id skipped |
| Escrow Created state | 2 | Buyer (CancelEscrow) + Seller (FundEscrow) |
| Escrow Funded state | 3 | Buyer (RefundEscrow), Seller (ClaimEscrow), timeout on capability |
| Terminal states | 3 | Claimed, Refunded, Cancelled — all produce zero caps/actions |
| Multi-instance / multi-role | 4 | Mixed states, same user across roles, both roles same instance, multiple wallet addresses |
| Null safety | 4 | Empty sled tree, corrupt entry, missing contract ID, unknown descriptor skipped |

Each test constructs a temporary `sled::Db` and in-memory `WalletDb`, inserts
serialized escrow entries, registers contract IDs, and asserts on the resolved
capabilities and actions. No ZK proofs, no network, pure in-process.
Runtime: <2 seconds.

```bash
cargo test -p dwow_wallet --lib -- capability::tests
```

### Null-safety coverage

Every fallible point in the resolver is exercised by at least one test:

| # | Fallible point | Test |
|---|---|---|
| 1 | `wallet.get_addresses()` → Err | `warn!` log + empty set (manually verified) |
| 2 | `wallet.get_coins(false)` → Err | `warn!` log + return (manually verified) |
| 3 | `PROMISSORY_NOTE_CONTRACT_ID.get()` → None | `test_null_missing_contract_id` |
| 4 | `ESCROW_CONTRACT_ID.get()` → None | `test_null_missing_contract_id` |
| 5 | `cache.db.open_tree(...)` → Err | `test_null_empty_sled_tree` |
| 6 | `tree.iter()` entry → Err | `test_null_corrupt_entry` |
| 7 | `deserialize::<Escrow>(...)` → Err | `test_null_corrupt_entry` |
| 8 | `bs58::decode(&coin.coin_id)` → Err / wrong len | `test_null_coin_id_decode_failure` |
| 9 | Empty descriptors map | `test_no_descriptors_registered` |
| 10 | Unknown descriptor name | `test_unknown_descriptor_skipped` |

### Level 3 — Docker container

The [`darkwow-testnet`](../../../contrib/docker/darkwow-testnet/) Docker
environment provides a dedicated wallet container
([`Dockerfile.wallet`](../../../contrib/docker/darkwow-testnet/Dockerfile.wallet),
[`entrypoint-wallet.sh`](../../../contrib/docker/darkwow-testnet/entrypoint-wallet.sh))
that builds only `dwow_wallet` (no WASM contracts, no `dwowd`, no `lilith`).
It runs in a live multi-node Docker testnet and verifies the full scan-to-position
cycle:

- `test_pipeline.sh --with-wallet` adds wallet container build, start, and verify steps to the pipeline
- [`test-wallet.sh`](../../../contrib/docker/darkwow-testnet/test-wallet.sh) runs the container in test mode: auto-init, scan, position, assert, exit
- `docker compose --profile wallet up -d` starts interactive mode for `docker exec` access

Test mode assertions verify:

- Coin capabilities appear from mining rewards
- Descriptors count is reported
- Capabilities section and wallet address appear in output

## Related Documents

- [Promissory Note Contract](../contract/promissory_note.md) — full PN lifecycle specification
- [Spend Hook Callback](../arch/zk/spend_hook.md) — callback mechanism for programmatic coins
- [Wallet Scanning](wallet_scanning.md) — how blocks are fetched and coins discovered
- [Wallet Contract Tracking](wallet_contract_tracking.md) — contract matching during scanning
- [DEP 0004](../dep/0004.md) — WASM modules for wallet extensibility (proposal)
- [dwowd JSON-RPC](../clients/dwowd_jsonrpc.md) — RPC endpoints the wallet consumes
