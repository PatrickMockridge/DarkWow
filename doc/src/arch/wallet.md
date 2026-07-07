# Wallet Architecture

## Design Philosophy

The DarkWow wallet is a **full node**. It participates in the P2P network on equal
terms with mining nodes (`dwowd`). It syncs the chain via P2P (GetTip/GetBlocks),
stores blocks in its own `LinearStore`, scans blocks locally with AEAD decryption,
discovers capabilities, builds transactions with ZK proofs, and broadcasts them
via P2P gossip.

See [Wallet vs Daemon Architecture](wallet-vs-daemon.md) for how the wallet's
static CLI runtime model and lightweight P2P client differ from the daemon's
permanent async server architecture.

### The Manifest-First Model — Differentiated from Upstream

DarkWow's wallet architecture makes one fundamental refutation of upstream's design:
**the wallet does not hardcode contract knowledge.** Upstream places the burden on
the wallet client — each client ships with hardcoded contract ABI files, function
discovery logic, and type definitions for the contracts it supports. A wallet that
wants to interact with a new DeFi protocol needs a code update. The inevitable
result is ecosystem fragmentation: different wallets support different subsets of
contracts based on what their maintainers chose to ship.

DarkWow inverts this. Contracts carry their own manifests on-chain — TOML documents
embedded in deployment transactions that describe functions, capabilities, actions,
state trees, and ZK circuits. The wallet reads the manifest and auto-configures
itself. Adding support for a new contract requires zero wallet code changes. The
manifest IS the contract interface.

Nine contracts are deployed at **genesis** to provide the capability primitive
layer that the manifest model depends on. See [Genesis Contracts](genesis.md)
for the complete list with ContractId derivation and bootstrap sequence.

Identity, Oracle, and Attestation power the **contract manifest trust model**.
Without them, the wallet has no way to verify that a manifest accurately describes
its contract. With them, the wallet can mechanically verify WASM exports against
manifest claims (Layer 2) and consult on-chain attestations from trusted issuers
(Layer 3). See [Contract Trust Model](contract-trust-model.md).

### Caveat Emptor: Trust, But Verify

The manifest model creates a trade-off. Upstream's approach — hardcode everything
in the client — is rigid but safe: if the client ships a contract definition, it's
been reviewed. DarkWow's approach — read manifests from the chain — is flexible
but adversarial: **manifests can lie.**

A malicious deployer can ship a manifest that claims a contract implements
`transfer(from, to, amount)` when the actual WASM exports `drain_all_funds(victim)`.
The wallet cannot prevent this — the contract is already on-chain.

DarkWow defends against dodgy manifests with three mechanisms, applied in order:

1. **WASM Verification (mechanical)**: The wallet parses the deployed WASM binary,
   extracts exported functions and circuit data sections, and compares them against
   the manifest's claims. A manifest that declares a function not present in the
   WASM, or omits a circuit that IS present, is flagged. This catches trivial fraud
   but not sophisticated deception.

2. **Genesis Capabilities (social)**: Identity and Attestation are in genesis
   precisely so that trusted issuers can create on-chain attestations for contracts
   they have inspected. "I, Alice, have reviewed contract 0xABCD and confirm it
   does what its manifest claims." The wallet resolves these attestations during
   `contract show` — an attested contract carries the issuer's reputation.

3. **Caveat Emptor Posture (user)**: Trust tiers annotate every contract display
   (`[GENESIS]`, `[OWN]`, `[ATTESTED by Alice]`, `[UNVERIFIED]`). The wallet
   **never blocks interaction** based on trust tier. It warns. The user decides.
   This is the same posture as Bitcoin Core's "this transaction is unconfirmed"
   — information, not policy.

This model shifts the trust burden from the wallet maintainer (who must review
every contract the client ships) to the ecosystem (who can inspect and attest to
contracts on-chain) and the user (who decides their own risk tolerance). It is
adversarial by design: the chain is a hostile environment, and the wallet is a
tool for navigating it, not a guardian that pretends to protect you.

### Generic Capability Engine

The wallet is a generic capability engine — it derives capabilities from on-chain
state rather than authenticating identities against access control lists. Every
contract output discovered via AEAD decryption is a capability. Native Token is the
sole special citizen because it is the consensus asset required for fee payment.
Everything else (PN, BB, escrow, auction, all 25+ contracts) goes through the
generic AEAD + manifest path — no per-contract files, no per-contract methods.

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
│  │ GetBlocks│   │ blocks   │   │ → capabilities        │ │
│  │ TxMessage│   │ by height│   │ → manifests           │ │
│  └──────────┘   └──────────┘   └──────────────────────┘ │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────────┐ │
│  │  Wallet  │   │  Cache   │   │    TX Builder          │ │
│  │  (SQLite)│   │  (sled)  │   │                      │ │
│  │          │   │          │   │ transfer/redeem/burn  │ │
│  │ secrets  │   │ Merkle   │   │ deploy via Deployooor │ │
│  │ held_caps│   │ trees    │   │ invoke via manifest   │ │
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
| `wallet` | SQLite | Held capabilities, generic capabilities, secrets, addresses, tx history, manifests, contract registry |

## P2P Network Connectivity

The wallet connects to the P2P network through a seed node, discovers peers via
hostlist, and syncs the chain using the same linear sync protocol as dwowd.

### P2P Stack — `dwow_core::net::P2p`, Same As Mining Nodes

The wallet uses `dwow_core::net::P2p` — the **same P2P stack as the mining nodes**.
No custom P2P code. `P2p::new()` creates the 6-session orchestrator. `P2p::start()`
activates all sessions (inbound is a no-op for client-only configuration).
`P2p::seed()` connects to seeds, exchanges hostlists via SeedSync, and discovers
mining nodes. The `sync_task` uses P2p channels for GetTip/GetBlocks.

**Feature architecture:**

| Feature | Enabled by | Contains |
|---------|-----------|----------|
| `net-wire` | Wallet (via `net-wallet`) | Message types (`VersionMessage`, `GetAddrsMessage`, etc.) + metering. 2 modules. |
| `net-wallet` | **Wallet** | `net-wire` + `P2p` + sessions + channel + connector + settings + transport (TCP+TLS). All P2P infrastructure. **No transport plugins.** |
| `net-full` | Daemon only | Transport plugins: Tor, I2P, SOCKS5, Unix, QUIC |
| `net` | Daemon | `net-wire` + `net-wallet` + `net-full` (everything) |

**Boundary**: `net-wallet` includes everything in `net-full` EXCEPT:
- `p2p-tor`, `p2p-i2p`, `p2p-socks5`, `p2p-unix`, `p2p-quic` — transport plugins
- `structopt`, `structopt-toml` — CLI config parsing (wallet uses direct TOML deserialization)
- `oxy-upnp-igd` — NAT traversal

The wallet's `Cargo.toml`:
```toml
dwow_core = { features = ["blockchain", "net-wallet", "async-serial"] }
```

The daemon's `Cargo.toml`:
```toml
dwow_core = { features = ["net", ...] }  # = net-wire + net-wallet + net-full
```

**What the wallet gets**: P2p struct, all 6 sessions (Inbound/Outbound/SeedSync/Manual/
Refine/Direct), Channel, Connector, Settings, Hosts, Protocol registry, message types.
**What the wallet does NOT get**: Tor, QUIC, I2P, SOCKS5, Unix transport plugins.

This is the architectural distinction: both binaries share the same P2P code.
The wallet's sessions are configured as client-only (empty `inbound_addrs`,
`outbound_connections=1`). The daemon's sessions handle full node operation.
They speak the same protocol because they use the same code.

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
seeds = [{ url = "tcp+tls://127.0.0.1:31340" }]
localnet = true
```

One config section. One P2P protocol. Same pattern as Bitcoin Core's `addnode`.

### P2P Message Types

The wallet registers these P2P message types (wire-compatible with dwowd):

### Core P2P Messages

Core message types shared with all P2P nodes (seed, mining, wallet):

| Message | Wire name | Direction | Purpose |
|---------|-----------|-----------|---------|
| `VersionMessage` | `version` | bidirectional | Version handshake: app_name, version, features |
| `VerackMessage` | `verack` | bidirectional | Version handshake acknowledgement |
| `PingMessage` | `ping` | bidirectional | Keepalive heartbeat |
| `PongMessage` | `pong` | bidirectional | Keepalive response |
| `GetAddrsMessage` | `getaddr` | bidirectional | Request peer addresses from hostlist |
| `AddrsMessage` | `addr` | bidirectional | Response with peer addresses `Vec<(Url, u64)>` |
| `SeedErrorMessage` | `seederr` | seed → peer | Structured error response (see below) |

### Application Messages

Application-level messages for chain sync and transactions:

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

1. **Query tips**: Iterates connected peer addresses. For each:
   - Connects via `PeerConnection::connect(addr, tls_config, magic_bytes, local_height)`
   - Sends `GetTip`: `conn.send("lineargettip", &GetTip).await`
   - Awaits response with timeout via `smol::future::or(fut, Timer::after(5s))`
   - Updates `highest_peer_tip = max(highest_peer_tip, tip.height)`

2. **Fetch missing blocks**: If any peer tip exceeds local chain height:
   - Connects to a peer via `PeerConnection::connect()`
   - Sends `GetBlocks { start_height: local+1, count: min(peer-local, 20) }`
   - Awaits response with timeout via `smol::future::or(fut, Timer::after(10s))`

3. **Insert only**: For each received block:
   - `insert_synced_block(block)` — writes to wallet's `chain: LinearStore`
   Scanning is NOT done here. A concurrent auto-scan task (spawned by `daemon`
   and by `sync init`) AEAD-decrypts synced blocks and discovers coinbase
   rewards (Native Token) and capabilities (all other contracts).

The loop repeats every 2 seconds. Docker gateway peers (`172.18.0.1`) are filtered
out by address pattern.

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

`sync init` triggers `init_p2p()` and spawns two detached tasks: the insert-only
sync loop (`run_wallet_sync`) and a concurrent auto-scan task. Both die on process
exit — for persistent sync use `daemon`. Subsequent calls are idempotent.

### Daemon Mode

The wallet can run as a persistent daemon — the same pattern as `bitcoind`,
`geth`, and `monero-wallet-rpc`. The daemon does two things:

1. **Initializes P2P** — connects to seed nodes, discovers peers via hostlist,
   establishes TLS connections.
2. **Runs the continuous sync loop** — polls peers every 2 seconds for new
   blocks, inserts them into the local chain store, and scans them for
   wallet-relevant capabilities.

The daemon blocks indefinitely, keeping the async executor alive so the sync
loop persists. While the daemon runs, the wallet stays synced with the network.

The daemon listens on a local Unix socket (`/tmp/dww-{network}.sock`) for
JSON-RPC requests from CLI commands. This is the same IPC pattern used by
`geth attach` and `monero-wallet-rpc`. The daemon owns the sled databases
and P2P connections exclusively — CLI processes never open sled directly.

Commands are classified by database dependency:
- **Needs sled** (transfer, redeem, broadcast, sync, scan): route through
  the daemon's Unix socket RPC.
- **SQLite only** (address, balance, secrets, capabilities): open
  the SQLite wallet directly via `LocalWallet`. No sled access — no lock
  contention with the daemon.
- **Pure** (help, version): no database access.

```
dwow_wallet daemon
```

See the CLI reference for the full command listing.

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
5. Compute `cap_id = poseidon_hash(attributes)`
6. Generate Merkle proof from the native token tree
7. Store in `held_capabilities` table with Merkle proof
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
4. Resolve trust tier: Genesis (9 contracts) → SelfDeployed → Attested (via Identity+Attestation from genesis) → Unverified

### Trust Tiers

| Tier | Criteria | Display |
|------|----------|---------|
| **Genesis** | Contract ID matches one of 9 genesis contracts (NativeToken, Deployooor, PromissoryNote, Identity, Oracle, Attestation, Purse, Box, MultiSig) | `[GENESIS]` |
| **SelfDeployed** | Deployer public key is in wallet's address book | `[OWN]` |
| **Attested** | Verified by on-chain attestation contract | `[ATTESTED]` |
| **Unverified** | Self-reported manifest, no attestation | `[UNVERIFIED]` |

Trust tiers annotate `contract show` and `position` output. They never block
interaction — users decide their own risk tolerance.

## Capability-First Design

### Two Things the Wallet Tracks

1. **Native Token** — the consensus asset. Used for fee payments and coinbase
   rewards. The only asset that requires Merkle proof tracking for fee payment.
   Stored in the `held_capabilities` table.

2. **Capabilities** — everything else. Every contract output discovered via
   AEAD decryption. All 23+ non-genesis contracts. Stored in the
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

Lifecycle: **Discover** (AEAD decrypt) → **Hold** (store in `held_capabilities`) → **Exercise**
(build ZK proof + publish nullifier, `mark_revoked`) → **Detect Revocation** (scan nullifier set).
On reorg, `mark_retained` reverses the revocation. The `revoked` and `revoked_at_height`
fields on `CapRecord` track this lifecycle.

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
secret whose commitment is in the Merkle tree, and the nullifier is fresh."
The verifier learns only that the proof is valid — not which capability,
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

| Method | Contract | Manifest Function | Opcode | What It Does |
|--------|----------|-------------------|--------|--------------|
| `transfer()` | PN | `transfer` | TransferV1 (0x04) | Atomic burn + blind output |
| `redeem()` | PN | `redeem` | RedeemV1 (0x01) | Close lifecycle, create receipt |
| `burn()` | PN | `burn` | BurnV1 (0x03) | Destroy value, publish nullifier |
| `create_token()` | PN | `token_mint` | TokenMintV1 (0x00) | Create token type |
| `mint_tokens()` | PN | `mint` | MintV1 (0x02) | Issue notes with backing proof |
| `deploy_contract()` | Deployooor | — | DeployV1 (0x00) | Deploy WASM contract |
| `lock_contract()` | Deployooor | — | LockV1 (0x01) | Lock contract — mark immutable |
| `invoke_contract()` | Any | Manifest-driven | — | Call any function on any registered contract |

Wallet convenience methods (`transfer`, `redeem`, `burn`, `create_token`, `mint_tokens`)
are thin wrappers: they prepare typed inputs from wallet DB queries, then call
`PromissoryNoteClient` in the contract crate (`src/contract/promissory_note/src/client/`).
ZK binary loading, ProvingKey construction, and builder invocation all live in the
contract crate — the wallet provides only capability selection, Merkle proofs, and secrets.
Every method maps to a manifest function name; the manifest is the canonical source
of opcodes and parameter schemas.

Transaction builders follow a common pattern:
1. Look up inputs from wallet DB (capabilities, secrets, Merkle proofs, leaf position, blinds)
2. Load ZK binary, build circuit, generate proofs
3. Encode params, wrap in ContractCall, create ContractCallLeaf
4. Attach Native Token fee via `build_fee_and_finalize_tx()`
5. Return signed Transaction

After building, `broadcast_tx()` serializes the transaction and calls
`p2p.read().unwrap().broadcast_tx(&tx)` — sending to all connected peers.

## Fee Payment

Fee payment uses Native Token (`FeeV1`, function code 0x00). The fee builder:

1. Selects a retained DRKW native token from the `held_capabilities` table
2. Loads the token's secret, Merkle proof, leaf position, and blinds
3. Loads the fee ZK binary (`NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN`)
4. Builds a ZK circuit with `FeeCallInput` (input token) and `FeeCallOutput` (change token)
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
dwow_wallet wallet balance                    Show balances
dwow_wallet wallet address                    Show default address
dwow_wallet wallet addresses                  List all addresses
dwow_wallet wallet capabilities               List all held capabilities
dwow_wallet wallet secrets                    Show secret keys
dwow_wallet wallet initialize                 Initialize wallet DB (wallet.sql + PN manifest)

dwow_wallet sync init                         Connect to P2P seeds, start sync task
dwow_wallet sync status                       Show sync status (local height + network tip)
dwow_wallet daemon                            P2P sync daemon — init P2P, spawn sync loop, block forever

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
dwow_wallet contract lock <deploy_auth>       Lock deployed contract — makes WASM immutable

```

## Source Files

| File | Purpose |
|------|---------|
| `bin/dww/src/lib.rs` | `Dww` struct, constructor, P2P init, `is_synced()`, `broadcast_tx()`, balance, addresses, secrets, invoke_contract |
| `bin/dww/src/scan.rs` | `ScanCache`, `scan_blocks()`, `scan_block_linear()`, AEAD decryption, coinbase + generic capability scan |
| `bin/dww/src/p2p_wallet.rs` | Thin wrapper: `P2pWalletConfig` + `SeedAddr` types for TOML deserialization. P2P stack is `dwow_core::net::P2p` (same as mining nodes). `init_p2p()` calls `P2p::new()` + `start()` + `seed()` |
| `bin/dww/src/sync_task.rs` | `run_wallet_sync()`, `GetTip`/`Tip`/`GetBlocks`/`Blocks` message types, `HighestPeerTip` |
| `bin/dww/src/dispatch.rs` | Command classification (`classify`, `classify_db_dependency`), `dispatch_sync`, `dispatch_async`, `dispatch_local` |
| `bin/dww/src/rpc_server.rs` | Daemon Unix socket JSON-RPC server, `DwwRpcHandler`, method dispatch |
| `bin/dww/src/wallet_rpc_client.rs` | RPC client — connect-per-call Unix socket, methods for daemon IPC |
| `bin/dww/src/local_wallet.rs` | `LocalWallet` — SQLite-only handle for CLI commands when daemon owns sled |
| `bin/dww/src/sled_checksum.rs` | Blake3 checksum wrapper for sled cache reads/writes — torn page detection |
| `bin/dww/src/config.rs` | `WalletConfig`, `load_config()`, TOML parsing, P2P settings via `P2pWalletConfig` from `[net]` section |
| `bin/dww/src/deploy.rs` | Deployooor `DeployV1` transaction building, `apply_tx_deploy_data()` |
| `bin/dww/src/walletdb.rs` | SQLite schema: `held_capabilities`, `capabilities`, `capability_proofs`, `addresses`, `transactions_history`, `contract_registry`, `contract_metadata`, `deploy_authorities`, `account_manager` |
| `bin/dww/src/capability.rs` | `CapabilityResolver::resolve()` — generic capability resolution from wallet state |
| `bin/dww/src/manifest_resolver.rs` | `ManifestResolver` — answers queries from stored manifests |
| `bin/dww/src/fee_builder.rs` | `build_fee_and_finalize_tx()` — Native Token FeeV1 attachment |
| `bin/dww/src/dispatch.rs` | Transfer/redeem/burn via `invoke_contract("promissory_note", ...)` |
| `bin/dww/src/contract_imports.rs` | Contract ID constants, ZK binary constants, OnceLock registry |
| `bin/dww/src/cache.rs` | Sled cache: Merkle trees, nullifier SMT, scanned block tracker |

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
- Full CLI dispatch classification (24 subcommands, 4 categories)
