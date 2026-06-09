# Wallet Architecture

The DarkWow wallet (`dwow_wallet`) is a **full node** — it holds the complete
blockchain on local disk and derives all state from local data. There is no SPV,
no light client, no network fetches for position resolution. Every query is pure
local computation over sled trees and SQLite tables.

The wallet is a **capability-first OS kernel** — it tracks cryptographic
capabilities regardless of which contract produced them. All 25+ contracts
use the same AEAD encryption primitive (ChaCha20Poly1305 + Sapling DH).
The wallet discovers capabilities by attempting AEAD decryption on every
output; the AEAD authentication tag IS the discriminator. There is no
contract_id filter, no token_id filter, no opcode matching.

Three contracts have optimized scan handlers:

- **Promissory Note** — the rich bearer-instrument contract supporting the full
  lifecycle: create, mint, transfer, redeem, burn, and OTC swap. Most user-issued
  tokens are PN-based.
- **Native Token** — a rock-dumb token contract that only does basic transfers
  and fee payments. Its sole function is `FeeV1 (0x00)`, used by every transaction
  to pay network fees in the native token.
- **Bearer Bond** — a fixed-interest staking contract for capital formation.
  Stake coins earn deterministic interest. Maturity is ZK-committed. Interest
  claims use a two-step request→pay flow where holders prove ownership before
  the issuer pays.

All other contracts (identity, dao_escrow, DEX, slot_auction, bidding,
tendering, labour_market, lottery, darkbet, game_room, roulette, baccarat,
darktoshi_dice, 15+ more) are handled by the **generic AEAD decryption
fallback** — no wallet code changes needed.

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
local peer via JSON-RPC (`blockchain.get_block_linear`). Scanning uses
**two independent paths** — no shared fallback logic, defense in depth.

### Path 1: Native Token Scanner (consensus-aligned)

Native token is the only token-based capability — a cryptocoin in the
Bitcoin sense, the mining reward, the consensus incentive. It gets a
**dedicated, first-class** scan path that handles ONLY coinbase outputs:

```
for each coinbase in block:
    deserialize AeadEncryptedNote from coinbase.encrypted_note
    for each wallet secret:
        if decrypt::<NativeNote>(secret) succeeds:
            build CoinAttributes -> CoinRecord -> insert_coin()
```

This path knows exactly the native_token note format. No guessing.
No fallback to other decoders. If decrypt fails, the coinbase is
not ours.

### Path 2: Generic Capability Scanner (Mark Miller capabilities)

Every other contract produces capabilities in the Mark Miller sense —
bearer instruments, authorization proofs, permissions. The scanner
discovers these via AEAD decryption with no decoder guessing:

```
for each contract call in transaction:
    if contract_id is known:
        use optimized handler (route by opcode)
    else:
        deserialize AeadEncryptedNote from call data
        for each wallet secret:
            if decrypt::<Vec<u8>>(secret) succeeds:
                capability found (AEAD tag = discriminator)
```

The AEAD authentication tag IS the discriminator. If decryption
succeeds, the capability IS ours regardless of whether we recognize
the note type. **New contracts work without any wallet code changes.**

### Why two paths?

- **Defense in depth**: If Path 2 breaks, Path 1 still finds coins.
- **Consensus alignment**: Native token is special — it's the blockchain
  reward mechanism. It deserves first-class treatment.
- **No decoder guessing**: Each path knows exactly what it's looking for.

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

## Wallet-to-Contract Integration Pattern

Every contract that the wallet integrates with follows the same six-layer
pattern. Promissory Note is the reference implementation; Bearer Bond was
built by following it. When adding a new contract, implement each layer in
order — each builds on the previous.

### Layer 1: Contract Metadata

**File:** `bin/drk/src/contract_metadata.rs`

Register every function the contract exposes, mapping opcode bytes to
human-readable names and proof requirements. This is what `dwow_wallet
contract metadata <name>` displays.

```rust
FunctionSignature { name: "fn_name", code: 0xNN, requires_proof: bool, proof_circuit: Some("circuit_name") },
```

The function names here must match the contract's `BearerBondFunction` enum
exactly — stale names (e.g. `declare_profits` when the contract says
`RequestInterestV1`) make the wallet's position output misleading.

### Layer 2: Wallet SQL Schema

**File:** `bin/drk/wallet.sql`

Add tables for coin records, secrets, and Merkle proofs. Mirror the pattern
from PN's `coins` / `coin_secrets` / `coin_merkle_proofs` tables, but with
contract-specific fields. For bearer bond this means `bond_coins` (adding
`maturity_block`, `last_claim_block`, `issuer_contract`, `interest_rate_bps`)
and `bond_coin_secrets`.

The table schema is what the block scanner writes into when it discovers a
coin belonging to the wallet. The capability resolver reads from these tables
at position-resolution time.

### Layer 3: Block Scanning — AEAD Note Decryption

**File:** `bin/drk/src/rpc.rs`

**Generic path (no code needed):** For most contracts, the generic AEAD
decryption fallback handles discovery automatically. If a contract
encrypts output notes using the standard `AeadEncryptedNote` format,
the wallet will find them with zero integration work.

**Contract-specific handler (when needed):** Three things are needed:

1. **Add contract state to `ScanCache`** — Merkle tree, SMT for nullifiers,
   note decryption secrets.
2. **Wire into `scan_block_linear()`** — a branch that checks
   `CONTRACT_ID` and dispatches to a handler.
3. **Write the handler function** — `apply_tx_<contract>_data_linear()`.
   This dispatches on the function opcode byte, decrypts AEAD-encrypted
   output notes with trial decryption (each secret × each output), and
   inserts discovered coins into the wallet database.

A handler is only needed when the contract requires typed coin storage,
Merkle tree tracking, or structured transaction history beyond what the
generic path provides.

### Layer 4: Wallet Operations Module

**File:** `bin/drk/src/<contract>.rs` (new)

Transaction builders and wallet queries. Each contract function that the user
can call gets a builder function:

- `keygen()` — generate a keypair, store in `addresses` + `coin_secrets`
- `get_coins()` — query the wallet database for owned coins
- `build_<function>()` — construct a transaction with ZK proofs

Builders follow the PN pattern:
```
1. Look up coin(s) from wallet DB
2. Fetch secrets, Merkle proof, leaf position, blinds
3. Build client-side call input with ZK witness data
4. Load ZK binary, create proving key, generate proofs
5. Encode params, wrap in ContractCall → TransactionBuilder
6. Attach fee call (NativeToken FeeV1)
7. Return signed Transaction
```

Register the module in `lib.rs` and add `initialize_<contract>()` to set up
trees and secrets.

### Layer 5: CLI Commands

**File:** `bin/drk/src/main.rs`

Add a subcommand enum variant and dispatch arm. Each subcommand wraps a
wallet operation from Layer 4:

```
BearerBond { List, ShowSeries, IssueStake, TransferStake, RequestInterest,
             Unstake, EmergencyUnstake, PayInterest, ProveCoverage }
```

The `List` subcommand is the quickest path to user feedback — it queries
the wallet database and prints owned coins. Other subcommands are stubs
until their ZK proof builders are wired.

### Layer 6: Capability Resolution

**File:** `bin/drk/src/capability.rs`

This layer interprets on-chain state as things the user can do. Two files
are involved:

1. **Contract-side descriptor** (`src/contract/<name>/src/capability.rs`):
   - Define capability type discriminants (u8 constants)
   - Implement `pub fn descriptor(contract_id) -> CapabilityDescriptor`
   - Declare actions with their require/consume/produce expressions

2. **Wallet-side resolver** (`bin/drk/src/capability.rs`):
   - Single method: `resolve_<name>(cid, cache, ..., &mut capabilities, &mut actions)`
   - One pass over the contract's sled tree
   - Match user pubkeys/keys against stored participant identifiers
   - Derive capabilities and actions for each instance
   - If the contract has two-party flows (like bearer bond's request→pay),
     add a second pass for the counterparty side

3. **Register** the resolver method in `resolve()` and the descriptor in
   `main.rs` (Position handler):
   ```rust
   if desc.name == "my_contract" {
       if let Some(cid) = crate::contract_imports::MY_CONTRACT_ID.get() {
           self.resolve_my_contract(*cid, cache, ..., &mut capabilities, &mut actions);
       }
   }
   ```

### Bearer Bond as Worked Example

Bearer Bond was built by following this pattern end-to-end. The commit
history shows the layers being added in order. For a developer adding a
new contract, bearer bond is the most complete reference after PN itself:

| Layer | Bearer Bond Implementation |
|---|---|
| Metadata | `contract_metadata.rs` — 9 function signatures (0x00-0x08), correct names matching the contract enum |
| SQL | `wallet.sql` — `bond_coins` + `bond_coin_secrets` tables with BB-specific fields |
| Scanning | `rpc.rs` — `apply_tx_bearer_bond_data_linear()` dispatches on IssueStakeV1/TransferStakeV1/PayInterestV1, `ScanCache` holds `bearer_bond_tree` + `bb_smt` |
| Operations | `bearer_bond.rs` — keygen, `get_bond_coins()`, 7 transaction builder stubs |
| CLI | `main.rs` — `BearerBond` subcommand with 9 subcommands |
| Capabilities | `capability.rs` — holder-side (6 capability types) + issuer-side (pending claim scanning for PayInterestV1) |

### Bearer Bond Resolver

Bearer Bond is a fixed-interest staking contract where stake coins are tradeable
capital positions with ZK-committed maturity and deterministic interest. The
interest claim flow is two-step: holders request payment (proving bond ownership
via Burn_V1 proof), then issuers pay against validated claims.

The resolver scans the contract's `coins` tree (sled) and `bonds_info` tree
to discover BondCoin instances and pending claims. Six capability types:

| Capability | Discriminant | Derivation |
|---|---|---|
| `CAP_STAKE` | `0x00` | BondCoin owned by wallet (consumable) |
| `CAP_INTEREST_RIGHT` | `0x01` | Always derived — interest is deterministic, no issuer reporting needed |
| `CAP_UNSTAKE_RIGHT` | `0x02` | Derived when `current_block >= maturity_block` |
| `CAP_RECEIPT` | `0x03` | Receipt coin after unstaking |
| `CAP_COVERAGE_REPORT` | `0x04` | Coverage report in `bonds_info` tree |
| `CAP_EMERGENCY_UNSTAKE` | `0x05` | Coverage < 100% — exit before maturity |

**Ownership check:** `poseidon_hash([secret.inner()]) == BondCoin.signature_public`,
matching Promissory Note's ZK privacy model.

**Interest right:** Since interest is computed deterministically from on-chain
state (`principal × rate × blocks_elapsed / (10000 × BLOCKS_PER_YEAR)`), the
right to claim is always derivable — no issuer declaration scanning needed.

**Coverage and emergency unstake:** The resolver scans the `bonds_info` tree for
`CoverageReport` entries. If `coverage_ratio_bps < 10000`, `CAP_EMERGENCY_UNSTAKE`
is derived, and `EmergencyUnstakeV1` (0x03) becomes available.

**Issuer-side scanning:** After processing holder-side capabilities, the resolver
scans `bonds_info` for `RequestedClaim` entries with `status == Pending`.
For each pending claim, a `PayInterestV1` (0x08) action is derived, requiring
`CAP_COVERAGE_REPORT` — the issuer must have filed a coverage proof before paying.

**Per-coin actions (holder):**
- `TransferStakeV1` (0x01) — always available while holding `CAP_STAKE`
- `RequestInterestV1` (0x02) — requires `CAP_STAKE` + `CAP_INTEREST_RIGHT`
- `EmergencyUnstakeV1` (0x03) — requires `CAP_STAKE` + `CAP_EMERGENCY_UNSTAKE`
- `UnstakeV1` (0x04) — requires `CAP_STAKE` + `CAP_UNSTAKE_RIGHT`

**Per-series actions (issuer):**
- `PayInterestV1` (0x08) — requires `CAP_COVERAGE_REPORT` for each pending claim

## Database Schema

The wallet SQLite database (`wallet_path`) schema is defined in
`bin/drk/wallet.sql`. Key tables:

| Table | Purpose |
|---|---|
| `addresses` | User public keys, secrets, default flag |
| `coins` | PN coins — value, token, secrets, blinds, spent status |
| `coin_secrets` | PN spend-authorizing secrets keyed by coin_id |
| `coin_merkle_proofs` | Merkle proofs for coin inclusion (shared by PN and BB) |
| `bond_coins` | Bearer bond coins — Pedersen value commit, maturity_block, last_claim_block, issuer_contract, interest_rate_bps |
| `bond_coin_secrets` | Bearer bond note secrets — principal, blinds, maturity, issuer |
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

## Reference Model

The Python model at `contrib/model/capability_scan_model.py` is the
**canonical specification** for the capability scan architecture, providing
a 1:1 mapping of the Rust implementation:

- Pallas curve arithmetic (NullifierK generator from `nullifier_k.rs`)
- Sapling DH key agreement (`sapling_ka_agree`)
- BLAKE2b KDF with DarkFiSaplingKDF personalization (`kdf_sapling`)
- ChaCha20Poly1305 AEAD encrypt/decrypt with fixed zero nonce
- NativeNote Encodable serialization (8 fields, 201 bytes minimum)
- Generic multi-contract scan (decrypt everything, AEAD tag = discriminator)

Run: `python3 contrib/model/capability_scan_model.py`
All 3 tests pass: serialization round-trip, coinbase mining → scan,
generic multi-contract scan.
