# Wallet Architecture

## Design Philosophy

The DarkWow wallet is a **full node**. It participates in the P2P network on equal
terms with mining nodes (`dwowd`). It syncs the chain via P2P (GetTip/GetBlocks),
stores blocks in its own `LinearStore`, scans blocks locally with AEAD decryption,
discovers capabilities, builds transactions with ZK proofs, and broadcasts them
via P2P gossip. **Zero RPC.**

The wallet is a generic capability engine — it derives capabilities from on-chain
state rather than authenticating identities against access control lists. Every
contract output discovered via AEAD decryption is a capability. Native Token is the
sole special citizen because it is the consensus asset required for fee payment.
Deployooor is hardcoded because it is genesis infrastructure for contract deployment.
Every other contract (PN, escrow, auction, all 25+) goes through the generic AEAD +
manifest path — no per-contract files, no per-contract methods.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                     Wallet (dwow_wallet)                 │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────────┐ │
│  │ P2P Layer │   │  Chain   │   │      Scan Engine      │ │
│  │          │   │          │   │                      │ │
│  │ seeds    │──▶│ LinearStore│◀──│ scan_blocks()        │ │
│  │ hostlist │   │ (sled)   │   │ scan_block_linear()  │ │
│  │ GetTip   │   │          │   │ AEAD decrypt          │ │
│  │ GetBlocks│   │ blocks   │   │ → coins + capabilities│ │
│  │ TxMessage│   │ by height│   │ → manifests           │ │
│  └──────────┘   └──────────┘   └──────────────────────┘ │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────────┐ │
│  │  Wallet  │   │  Cache   │   │    TX Builder          │ │
│  │  (SQLite)│   │  (sled)  │   │                      │ │
│  │          │   │          │   │ transfer/redeem/burn  │ │
│  │ secrets  │   │ Merkle   │   │ deploy via Deployooor │ │
│  │ coins    │   │ trees    │   │ invoke via manifest   │ │
│  │ caps     │   │ nullifier│   │ + fee attachment      │ │
│  │ addrs    │   │ SMT      │   │ + ZK proof generation │ │
│  └──────────┘   └──────────┘   └──────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Data Stores

| Store | Type | Contents |
|-------|------|----------|
| `chain` | sled (LinearStore) | Synced blocks by height. Same sled DB dwowd writes to. |
| `cache` | sled | Merkle tree checkpoints, nullifier SMT, scanned block tracker |
| `wallet` | SQLite | Native token coins, capabilities, secrets, addresses, tx history, manifests, contract registry |

## P2P Network Connectivity

The wallet connects to the P2P network through a seed node, discovers peers via
hostlist, and syncs the chain using the same linear sync protocol as dwowd.

### Seed Connection

```
Wallet ──seed──▶ lilith (seed node at 127.0.0.1:31340)
  │                 │
  │    GetAddrs     │
  │◀───────────────│  returns hostlist
  │                 │
  │  connect ──────▶ node0, node1, ...
  │                 │
  │    GetTip ─────▶ each peer
  │◀─── Tip ───────│  peer height + hash
  │                 │
  │  GetBlocks ────▶ ahead peers
  │◀── Blocks ─────│  batch of 20 blocks
```

### Environment Configuration

| Environment | Seed address | How it works |
|------------|-------------|---------------|
| Docker container | `tcp+tls://lilith:31340` | Docker DNS resolves `lilith` inside bridge network |
| Host ↔ Docker devnet | `tcp+tls://127.0.0.1:31340` | Lilith P2P port published to host loopback |
| Public testnet | `tcp+tls://<seed IP>:31340` | Public IP of a lilith seed node |

```toml
# ~/.config/dwow/dww_config.toml — wallet config for host ↔ Docker devnet
[network_config."darkwow-testnet".net]
seeds = ["tcp+tls://127.0.0.1:31340"]
inbound = false
```

One config section. One P2P protocol. Same pattern as Bitcoin Core's `addnode`.

### P2P Message Types

The wallet registers these P2P message types (wire-compatible with dwowd):

| Message | Wire name | Direction | Purpose |
|---------|-----------|-----------|---------|
| `GetTip` | `lineargettip` | wallet → peer | Request peer's chain tip height + hash |
| `Tip` | `lineartip` | peer → wallet | Response: `{ height: u64, hash: String }` |
| `GetBlocks` | `lineargetblocks` | wallet → peer | Request blocks from `start_height`, up to `count` |
| `Blocks` | `linearblocks` | peer → wallet | Response: `{ blocks: Vec<Block> }` (batch of 20) |
| `TxMessage` | `lineartx` | wallet → peers | Broadcast transaction (256 KB max) |

## Chain Sync

### Sync Task (`sync_task.rs`)

After `init_p2p()` connects to seeds and discovers peers, a background sync task
(`run_wallet_sync`) runs in a detached smol task. It:

1. **Query tips**: Iterates all connected peer channels. For each:
   - Subscribes to `Tip` messages: `channel.subscribe_msg::<Tip>().await`
   - Sends `GetTip`: `channel.send(&GetTip).await`
   - Awaits response: `tip_sub.receive_with_timeout(5).await`
   - Updates `highest_peer_tip = max(highest_peer_tip, tip.height)`

2. **Fetch missing blocks**: If any peer tip exceeds local chain height:
   - Subscribes to `Blocks` messages on the peer channel
   - Sends `GetBlocks { start_height: local+1, count: min(peer-local, 20) }`
   - Awaits response: `blocks_sub.receive_with_timeout(10).await`

3. **Insert + scan**: For each received block:
   - `insert_synced_block(block)` — writes to wallet's `chain: LinearStore`
   - `scan_block_linear(scan_cache, block)` — AEAD decrypts outputs, discovers
     coinbase rewards (Native Token) and capabilities (all other contracts)

The loop repeats every 10 seconds. Docker gateway peers (`172.18.0.1`) are filtered
out — only full-node peers with `SESSION_DEFAULT` are queried.

### Sync Status

`is_synced()` returns true when the wallet has caught up to the network:

```
is_synced():
    local = chain.get_height()
    if local == 0: return false
    if p2p is connected and highest_peer_tip > 0:
        return local >= highest_peer_tip
    return local > 0   // fallback: any blocks = synced (no peer info yet)
```

`sync status` CLI shows:
```
Sync status: SYNCED
  Local chain height: 42
  P2P connected: yes
```

`sync init` triggers `init_p2p()` and spawns the background sync task. Subsequent
calls are idempotent.

## Local Scan

### Scan Architecture

The wallet scans its own `chain: LinearStore` — the same sled database dwowd
writes to. No RPC. No network call. Pure local iteration.

```
for height in last_scanned+1 .. chain_height():
    block = chain_block(height)          // local sled read
    scan_block_linear(scan_cache, block) // pure local processing
```

### scan_block_linear — Two Paths

**Path 1: Coinbase (Native Token consensus reward)**

If a transaction has a `coinbase` field:
1. Decode `AeadEncryptedNote` from the coinbase bytes
2. For each wallet secret, attempt AEAD decryption (ChaCha20Poly1305 + Sapling DH)
3. If decryption succeeds: the coinbase reward belongs to this wallet
4. Derive `CoinAttributes` (public_key, value, token_id, spend_hook, user_data, blind)
5. Compute `coin = poseidon_hash(attributes)` → coin_id
6. Generate Merkle proof from the native token tree
7. Store in `coins` table with Merkle proof
8. Store in `capabilities` table (structured: NativeToken)

**Path 2: Generic AEAD (every other contract — PN, BB, escrow, all 25+)**

For each contract call in each transaction:
1. Skip function code byte, scan remaining `call.data` byte-by-byte
2. At each offset, attempt `AeadEncryptedNote::decode()`
3. For each decoded note, for each wallet secret, attempt `note.decrypt::<T>(secret)`
4. If decryption succeeds: the output capability belongs to this wallet
5. Try known decoders first (`NativeToken::decode`), then fall through to generic
6. Store opaque record in `capabilities` table with nullifier, contract_id, height

Path 2 is a single generic function. No contract ID lookup. No per-contract branch.
The AEAD authentication tag IS the discriminator. **New contracts work without
wallet code changes.**

### Manifest Discovery

During `scan_block_linear`, when a Deployooor `DeployV1` call (function code 0x00)
is detected:
1. Decode `DeployParamsV1` from call data
2. Check for contract manifest: `ContractManifest::from_deploy_ix(&params.ix)`
   (detected via 0x4D magic byte prefix in the ix field)
3. If found: parse TOML, store as JSON in SQLite `contract_metadata` table
4. Resolve trust tier: Genesis → SelfDeployed → Attested → Unverified

### Trust Tiers

| Tier | Criteria | Display |
|------|----------|---------|
| **Genesis** | Contract ID matches NativeToken, Deployooor, or PromissoryNote | `[GENESIS]` |
| **SelfDeployed** | Deployer public key is in wallet's address book | `[OWN]` |
| **Attested** | Verified by on-chain attestation contract | `[ATTESTED]` |
| **Unverified** | Self-reported manifest, no attestation | `[UNVERIFIED]` |

Trust tiers annotate `contract show` and `position` output. They never block
interaction — users decide their own risk tolerance.

## Capability-First Design

### Two Things the Wallet Tracks

1. **Native Token** — the consensus asset. Used for fee payments and coinbase
   rewards. The only asset that requires Merkle proof tracking for fee spending.
   Stored in the `coins` table.

2. **Capabilities** — everything else. Every contract output discovered via
   AEAD decryption. PN, BB, escrow, auction, all 25+ contracts. Stored in the
   `capabilities` table.

There is no third category. No contract gets its own table, its own tree, or its
own dedicated methods. Native Token is the sole special citizen — and only
because it is the consensus asset needed for fee payment.

### The Four-Part Capability Pattern

Every output the wallet discovers follows the same cryptographic pattern:

| Component | Function | Wallet Role |
|-----------|----------|-------------|
| **Commitment** | `H(secret, params)` | Stored on-chain. The wallet decrypts it to discover ownership. |
| **Nullifier** | `H(secret, commitment)` | Published when exercising the capability. The wallet detects spends by scanning for nullifiers. |
| **Proof** | ZK proof of secret knowledge | Built by the wallet when the user wants to exercise a capability. |
| **Revocation** | Issuer invalidates | Optional. Checked during capability resolution. |

Lifecycle: **Discover** (AEAD decrypt) → **Hold** (store in DB) → **Exercise**
(build ZK proof + publish nullifier) → **Detect Spend** (scan nullifier set).

### Object Capabilities and the Wallet

The wallet is an **object-capability (o-cap) native architecture** — it derives
capabilities from on-chain state rather than authenticating identities against
access control lists. This is Mark Miller's model of authorization: a capability
is an **unforgeable reference** that combines **designation** (what the object is)
with **authority** (what the holder can do with it).

The wallet never asks "who is this user?" It asks "what can these secrets do?"
The secrets are the capability. The proof IS the authority.

### Capability Resolution

The wallet resolves capabilities by scanning on-chain state and matching
against the user's secrets. This is a pure local computation — no network
requests, no identity queries:

1. **Collect user's public keys** from the wallet database
2. **Scan block outputs** via generic AEAD decryption — every decrypted output
   is a capability (regardless of which contract produced it)
3. **Match against on-chain state** — for native token, check Merkle proofs;
   for everything else, the AEAD tag IS the proof of ownership
4. **Derive available actions** — which contract functions the user can call
   given their current capabilities

### ZK + Conditional Circuits: The Mathematical Foundation

The Authorization Inversion Theorem ([ocap.md](ocap.md)) proves that
capability-based authorization is mathematically equivalent to having a ZK
proof system for the predicate defined by the capability:

**A'(π, r, s) = ∃ w : P_{r,s}(w) = 1**

Where P_{r,s} is a predicate over the witness w, and the proof π reveals only
the predicate result — not w, not the holder's identity. The witness is known
only to the prover and is cryptographically unlinkable to any principal.

DarkWow's ZK circuits implement this directly. A transfer proves: "I know a
secret whose commitment is in the Merkle tree, and the nullifier hasn't been
spent." The verifier learns only that the proof is valid — not which coin,
not the value, not the token type, not the holder. This is privacy-by-construction:
the circuit's public inputs are exactly what must be revealed for consensus
validation; everything else stays in the witness.

Conditional circuits (`LessThanOrEqual`, `IsNotEqual`) extend this to
predicate evaluation: "I have balance ≥ threshold" or "my credential has not
expired." The return value is a single bit — authorized or not. The attribute
values never leave the witness.

### Why O-Caps, Not ACLs

Access control lists answer "WHO has access to X?" O-caps answer "Can you
PROVE you have access to X?" The difference is fundamental:

- ACLs require storing and authenticating identities — a privacy catastrophe
  on a public ledger
- O-caps require only that the holder can produce a valid proof against a
  public commitment — identity is never involved

The wallet is the user's capability browser. It shows what the user can DO,
not who the user IS. Every contract interaction — transfer, redeem, vote,
bid, stake — is a ZK predicate evaluation. The wallet builds the proof.
The system verifies it. Identity is never on the chain.

See [O-Cap & Composable Privacy](ocap.md) for the full mathematical foundation
including the Authorization Inversion Theorem.

## Transaction Building

The wallet builds transactions for specific contract functions. These are
legitimate operations — they call named opcodes on known contracts:

| Method | Contract | Opcode | What It Does |
|--------|----------|--------|--------------|
| `transfer()` | PN | TransferV1 (0x04) | Atomic burn + blind output |
| `redeem()` | PN | RedeemV1 (0x01) | Close lifecycle, create receipt |
| `burn()` | PN | BurnV1 (0x03) | Destroy value, publish nullifier |
| `create_token()` | PN | TokenMintV1 (0x00) | Create token type |
| `mint_tokens()` | PN | MintV1 (0x02) | Mint coins with backing proof |
| `deploy_contract()` | Deployooor | DeployV1 (0x00) | Deploy WASM contract |
| `invoke_contract()` | Any | Manifest-driven | Call any function on any registered contract |

Transaction builders follow a common pattern:
1. Look up inputs from wallet DB (coins, secrets, Merkle proofs, leaf position, blinds)
2. Load ZK binary, build circuit, generate proofs
3. Encode params, wrap in ContractCall, create ContractCallLeaf
4. Attach Native Token fee via `build_fee_and_finalize_tx()`
5. Return signed Transaction

After building, `broadcast_tx()` serializes the transaction, wraps it in a
`TxMessage`, and calls `p2p.broadcast(&tx_msg)` — sending to all connected peers.

## Fee Payment

Fee payment uses Native Token (`FeeV1`, function code 0x00). The fee builder:

1. Selects an unspent DRKW coin from the `coins` table
2. Loads the coin's secret, Merkle proof, leaf position, and blinds
3. Loads the fee ZK binary (`NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN`)
4. Builds a ZK circuit with `FeeCallInput` (input coin) and `FeeCallOutput` (change coin)
5. Generates a ZK proof
6. Encodes params as `FeeV1` call data
7. Appends the fee call as a root-level call in the transaction (no parent, no children)

Default fee: 42,000,000 base units. The change output returns the remainder to
the same public key with a fresh blind.

## Contract Deployment

Deployment uses the Deployooor contract (genesis infrastructure):

1. **Build**: `Dww::deploy_contract(deploy_keypair, wasm_bincode, deploy_ix)`
   - Creates `DeployParamsV1` with public_key, WASM bytes, deploy_ix
   - Optionally embeds a `ContractManifest` in the deploy_ix field (0x4D magic byte)
   - Builds DeployV1 (function code 0x00) contract call
   - Attaches fee via `build_fee_and_finalize_tx()`

2. **Broadcast**: `broadcast_tx(tx)` serializes to bytes, wraps in `TxMessage`,
   calls `p2p.broadcast(&tx_msg)` — gossips to all connected peers.

3. **Confirmation**: The sync task will eventually receive the block containing
   the deployed transaction. During `scan_block_linear`, the Deployooor call is
   detected, the manifest is stored, and the contract becomes available for
   `contract show` and `contract invoke`.

The deploy CLI path:
```
dwow_wallet contract deploy <auth_key> <wasm_path> --manifest manifest.toml
```
1. Parse deploy authority key
2. Read WASM bytes
3. Build deploy_ix from manifest TOML (if `--manifest` provided)
4. `deploy_contract()` → build DeployV1 tx with fee
5. `broadcast_tx()` → P2P gossip
6. Print txid

## Contract Discovery and Interaction

The wallet discovers contract interfaces through **contract manifests** — TOML
documents embedded in deployment transactions. Manifests describe functions,
capability types, actions, state trees, and ZK circuits.

### Manifest Data Pipeline

```
DeployV1 tx on chain
  → scan_block_linear() detects 0x4D magic byte             [scan.rs]
  → ContractManifest::from_deploy_ix() parses TOML           [sdk/src/manifest.rs]
  → wallet.store_manifest() stores as JSON in SQLite         [walletdb.rs]
  → CapabilityResolver::resolve() reads manifest             [capability.rs]
  → ManifestResolver answers queries (describe, validate)    [manifest_resolver.rs]
  → CLI: contract show / contract invoke --params            [dispatch.rs]
```

Each stage is a single function call — synchronous local operations, no network.

## CLI

```
dwow_wallet wallet keygen                     Generate new keypair
dwow_wallet wallet balance                    Show balances
dwow_wallet wallet address                    Show default address
dwow_wallet wallet addresses                  List all addresses
dwow_wallet wallet coins                      List all coins
dwow_wallet wallet secrets                    Show secret keys
dwow_wallet wallet initialize                 Initialize wallet DB (wallet.sql + PN manifest)

dwow_wallet sync init                         Connect to P2P seeds, start sync task
dwow_wallet sync status                       Show sync status (local height + network tip)

dwow_wallet scan                              Scan local chain for wallet outputs
dwow_wallet scan --reset <height>             Re-scan from specific height

dwow_wallet transfer <amt> <token> <rcpt>     Build + broadcast TransferV1
dwow_wallet redeem <coin_id>                  Build + broadcast RedeemV1
dwow_wallet burn <coin_ids>                   Build + broadcast BurnV1

dwow_wallet broadcast                         Read tx from stdin, broadcast via P2P

dwow_wallet position                          Show capabilities and available actions
dwow_wallet position --json                   JSON output

dwow_wallet contract deploy <auth> <wasm>     Deploy a WASM contract
    --manifest manifest.toml                  Embed manifest in deploy tx
    --deploy-ix <hex>                         Legacy deploy init params
dwow_wallet contract show <contract_id>       Display interface from stored manifest
    [GENESIS] / [OWN] / [UNVERIFIED] trust
dwow_wallet contract invoke <id> <fn>         Call a contract function (manifest-driven)
    --params params.json                      With parameter validation

dwow_wallet contract register <name> <id>     Register contract name→ID mapping
dwow_wallet contract list                     List deploy authorities
dwow_wallet contract generate-deploy          Generate new deploy authority keypair

dwow_wallet mine                              Mine blocks (LOCALNET ONLY — stratum TCP)
```

## Source Files

| File | Purpose |
|------|---------|
| `bin/drk/src/lib.rs` | `Dww` struct, constructor, P2P init, `is_synced()`, `broadcast_tx()`, keygen, balance, addresses, secrets, redeem, burn, transfer, invoke_contract |
| `bin/drk/src/scan.rs` | `ScanCache`, `scan_blocks()`, `scan_block_linear()`, AEAD decryption, coinbase + generic capability scan, `miner_mine()` |
| `bin/drk/src/sync_task.rs` | `run_wallet_sync()`, `GetTip`/`Tip`/`GetBlocks`/`Blocks`/`TxMessage` P2P types, `HighestPeerTip` |
| `bin/drk/src/dispatch.rs` | Command classification, `dispatch_sync`, `dispatch_async`, `requires_sync()` gate, deploy handler |
| `bin/drk/src/config.rs` | `WalletConfig`, `load_config()`, TOML parsing, P2P settings from `[net]` section |
| `bin/drk/src/deploy.rs` | Deployooor `DeployV1` transaction building, `apply_tx_deploy_data()` |
| `bin/drk/src/walletdb.rs` | SQLite schema: `coins`, `capabilities`, `secrets`, `addresses`, `transactions_history`, `contract_registry`, `contract_metadata`, `deploy_authorities` |
| `bin/drk/src/capability.rs` | `CapabilityResolver::resolve()` — generic capability resolution from wallet state |
| `bin/drk/src/manifest_resolver.rs` | `ManifestResolver` — answers queries from stored manifests |
| `bin/drk/src/fee_builder.rs` | `build_fee_and_finalize_tx()` — Native Token FeeV1 attachment |
| `bin/drk/src/transfer.rs` | TransferV1 transaction builder |
| `bin/drk/src/contract_imports.rs` | Contract ID constants, ZK binary constants, OnceLock registry |
| `bin/drk/src/cache.rs` | Sled cache: Merkle trees, nullifier SMT, scanned block tracker |

## Python Model

The canonical specification is `contrib/model/wallet_model.py`. Python leads,
Rust follows. The model covers:

- Full cryptographic primitives (Pallas curve, Poseidon hash, AEAD encryption)
- Complete database schema (SQLite, 15 tables)
- Scan engine (coinbase + generic AEAD paths)
- Capability resolution (18+ contracts, generic resolver)
- Transaction building (TransferV1, RedeemV1, BurnV1, DeployV1, FeeV1)
- ZK proof model (circuit, witness, proof generation)
- P2P sync flow (GetTip/GetBlocks, `is_synced` vs peer tip)
- Manifest parsing and trust tiers
- WASM verification (zero-trust, mechanical checking)
- Full CLI dispatch classification (51 subcommands, 4 categories)
