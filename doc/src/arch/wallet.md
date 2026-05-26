# Wallet Architecture

The DarkWow wallet (`dww`) is a **full node** — it holds the complete blockchain
on local disk and derives all state from local data. There is no SPV, no light
client, no network fetches for position resolution. Every query is pure local
computation over sled trees and SQLite tables.

## Data Stores

The `Drk` struct holds two databases:

| Store | Type | Contents |
|---|---|---|
| `cache: Cache` | sled (embedded KV) | Block pointers, Merkle tree checkpoints, per-contract state trees (escrows, nullifiers, spent flags), money nullifier SMT |
| `wallet: WalletPtr` | SQLite | User coins, secrets, addresses, deploy authorities, contract registry, contract metadata, contract interactions, transaction history |

Both are at paths configured in `dww_config.toml` (`cache_path`, `wallet_path`).

## Block Scanning

At startup (or via `dww scan`), the wallet fetches blocks from its local peer via
JSON-RPC (`blockchain.get_block_linear`). For each transaction in each block,
contract-specific handlers decrypt notes to discover coins belonging to the
wallet:

```
for each unseen block:
    for each transaction:
        for each contract call:
            route to handler by contract_id + function opcode
            decrypt notes with wallet secrets
            if ours → insert CoinRecord + MerkleProof
    update trees, flush database
```

Discovered coins are stored in the `coins` table with their value, token,
spend hook, user data, leaf position in the Merkle tree, secrets, blinds,
spent status, and creation height.

## Capability-Based Position Resolution

Above the coin layer, the wallet interprets on-chain state as **capabilities**
— things the user can do. This is the position resolution system.

### Core Idea

Everything that authorizes an action is a capability:
- **Coins** — "I can spend this coin"
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
- `name: String` — "FundEscrow", "ClaimEscrow", etc.
- `contract_id: ContractId` — target contract
- `description: String` — human-readable
- `requires: CapabilityExpression` — what must be held
- `consumes: Vec<CapabilityId>` — nullified on execution
- `produces: Vec<CapabilityOutput>` — gained on execution

**`CapabilityDescriptor`** — a contract's declaration of what actions exist
and what capabilities they require, consume, and produce. Each contract
provides one descriptor (e.g. `src/contract/escrow/src/capability.rs`).

### Resolution Algorithm

`CapabilityResolver::resolve(&self, wallet, cache)` runs locally:

```
1. Collect user's public keys from wallet.get_addresses()

2. Derive coin capabilities:
   for each unspent coin in wallet.get_coins(false):
       Capability { source: Coin { coin_id }, consumable: true }

3. Per-contract resolution (single pass per sled tree):
   for each registered descriptor:
       open contract sled tree via cache.db.open_tree(hash_state_id(...))
       for each contract instance:
           if user's pubkey matches a participant pubkey:
               derive capabilities (role × state)
               build available actions (role × state → Action)

4. Return PositionResult { capabilities, available_actions }
```

Each contract gets **one resolver method** that scans its sled tree
**once** and produces both capabilities and actions from each instance.
There is no second pass, no template substitution, no dead code.

### Coin Capabilities

Every unspent coin in the wallet becomes a `Capability` with:
- `contract_id` = Money V3 contract
- `source` = `CapabilitySource::Coin { coin_id }`
- `consumable` = true (spending the coin nullifies it)

These represent the fundamental "I can spend money" capability.

### Escrow Reference Implementation

The escrow contract (`src/contract/escrow/`) is the first contract with
capability resolution. Its state machine:

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

Each action's `requires`, `consumes`, and `produces` fields use
instance-specific `CapabilityId`s derived with the actual
`escrow_id_bytes`, so held capabilities correctly match action requirements.

## CLI: `dww position`

The `Position` subcommand loads descriptors, instantiates the resolver, and
prints the user's current position:

```
$ dww position

=== Held Capabilities ===
  6S2nMh1... — Coin worth 1000 [consumable]
  3Kj9xP7... — Creator of escrow 8Rm4wQ2... (Funded) [consumable] [expires: block 50000]

=== Available Actions ===
  escrow::RefundEscrow (0x04) — Refund escrow 8Rm4wQ2...
  escrow::ClaimEscrow (0x03) — Claim escrow 8Rm4wQ2...
```

## Adding a New Contract Resolver

When adding capability resolution for a new contract:

1. **Create a descriptor** in the contract crate (e.g. `src/contract/<name>/src/capability.rs`):
   - Define capability type discriminants (u8 constants)
   - Implement `pub fn descriptor(contract_id) -> CapabilityDescriptor`
   - Declare actions with their require/consume/produce expressions

2. **Add a resolver method** in `bin/drk/src/capability.rs`:
   - Single method: `resolve_<name>(&self, cid, cache, user_pubkeys, &mut capabilities, &mut actions)`
   - One pass over the contract's sled tree
   - Match user pubkeys against stored participant keys
   - Derive capabilities and actions for each instance

3. **Register** in `resolve()`:
   ```rust
   if desc.name == "my_contract" {
       if let Some(cid) = crate::contract_imports::MY_CONTRACT_ID.get() {
           self.resolve_my_contract(*cid, cache, &user_pubkeys, &mut capabilities, &mut actions);
       }
   }
   ```

4. **Wire in the Position handler** (`main.rs`): register the descriptor
   after wallet creation.

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
tests the `dww position` subcommand end-to-end:

- Subcommand registration and help text
- Error handling: missing config, corrupt config, no running node
- End-to-end: start `dwowd` in devnet mode, mine blocks, scan, verify position output format

No ZK proofs, no Docker. Runtime: ~30 seconds.

### Level 2 — Rust resolver logic (in-process)

`#[cfg(test)] mod tests` at the bottom of
[`bin/drk/src/capability.rs`](../../../bin/drk/src/capability.rs) — 20 tests
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
serialized escrow entries, registers contract IDs, and asserts on the
resolved capabilities and actions. No ZK proofs, no network, pure in-process.
Runtime: <2 seconds.

```bash
cargo test -p dww --lib -- capability::tests
```

### Null-safety coverage

Every fallible point in the resolver is exercised by at least one test:

| # | Fallible point | Test |
|---|---|---|
| 1 | `wallet.get_addresses()` → Err | `warn!` log + empty set (manually verified) |
| 2 | `wallet.get_coins(false)` → Err | `warn!` log + return (manually verified) |
| 3 | `MONEY_V3_CONTRACT_ID.get()` → None | `test_null_missing_contract_id` |
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
that builds only `dww` (no WASM contracts, no `dwowd`, no `lilith`). It runs
in a live multi-node Docker testnet and verifies the full scan-to-position cycle:

- `test_pipeline.sh --with-wallet` adds wallet container build, start, and verify steps to the pipeline
- [`test-wallet.sh`](../../../contrib/docker/darkwow-testnet/test-wallet.sh) runs the container in test mode: auto-init, scan, position, assert, exit
- `docker compose --profile wallet up -d` starts interactive mode for `docker exec` access

Test mode assertions verify:

- Coin capabilities appear from mining rewards
- Descriptors count is reported
- Capabilities section and wallet address appear in output

## Related Documents

- [Wallet Scanning](wallet_scanning.md) — how blocks are fetched and coins discovered
- [Wallet Contract Tracking](wallet_contract_tracking.md) — contract matching during scanning
- [DEP 0004](../dep/0004.md) — WASM modules for wallet extensibility (proposal)
- [dwowd JSON-RPC](../clients/dwowd_jsonrpc.md) — RPC endpoints the wallet consumes
