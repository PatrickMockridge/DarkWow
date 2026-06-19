# DarkWow

A DarkFi fork rebuilt around **four rejections of upstream**:

1. **No governance DAO** — pure PoW, no token-holder voting
2. **No overlay/diff consensus** — deterministic Uncle Merkle with stateless verification
3. **LessThanOrEqual, IsNotEqual, and BaseDiv opcodes built and proven sound in Lean4 on this fork** — additions to upstream's zkVM, not inherited
4. **No premine** — every coin mined

Zero vendor lock-in. Genesis is eight contracts — NativeToken (consensus),
Deployooor (deployment), Promissory Note (universal DeFi primitive),
Identity (credentials), Oracle (data feeds), Attestation (trust verification),
**Purse** (ZK fungible asset container), and **Box** (ZK capability delegation).
Purse and Box are DarkWow's O-Cap composition primitives — they replace
hand-rolled balance tracking and capability proofs with modular, composable
child calls. DAOs hold Purses for treasuries. Contracts delegate authority via
Boxes. The manifest trust model verifies that every contract's on-chain
interface matches reality. Hard forks are a feature, not a threat.
Extended and entirely voluntary smart contract feature set including but not
limited to: Darktoshi Dice, DAO (with Escrow and Drain protection), DEX,
Stablecoin, Prediction Market, Betting Stake, Sealed Bidding/Tendering, Labor
Market and more.

These four refutations are the technical expression of a political-economic fork.
DarkFi upstream follows the Dark Enlightenment trajectory of Nick Land — governance-DAO
plutocracy, premine extraction, SAFT financialization. DarkWow follows the
left-accelerationist trajectory of Mark Fisher — disintermediation, temporal
sovereignty, architecture as desire-engineering. Same technical base (zkVM, Halo2,
WASM runtime, P2P stack); opposite political conclusion. See the
[Philosophy](doc/src/philosophy/philosophy.md) page for the full articulation.

<!-- TODO: website placeholder — https://darkwow.org -->

Development occurs on the **`linear-master`** branch.

---

## Four Refutations in Detail

See [What's Different from Upstream DarkFi](doc/src/about/differences_from_upstream.md) for the complete comparison table and detailed explanations of the four design divergences.

### 1. No Governance DAO — Pure PoW

DarkWow removes all governance control over the native token.

---

## Privacy Architecture: O-Cap Authorization

**Traditional blockchain asks**: "WHO has access?" — public keys link transactions to identities.

**DarkWow asks**: "Can you PROVE you have access?" — identity never revealed, only capabilities proven. O-Cap (Object Capability) = authorization without revelation.

```
Alice proves: "I am a verified smart contract auditor"
Verifier learns: ✓ Alice can audit
Verifier DOES NOT learn: Alice's public key, balance, or identity
```

---

## Finality Architecture: Two Independent Security Layers

DarkWow provides **two orthogonal finality mechanisms** that protect against
51% attacks and chain reorganization — both operating as constraint overlays
on top of PoW fork choice:

### Monero Anchoring (p2pool merge mining)

DarkWow blocks can be merged-mined with Monero via the p2pool protocol.
An `xmrig` miner hashes the Monero block (RandomX); when a share meets
DarkWow's difficulty, p2pool submits the solution to dwowd's `mm_rpc`
JSON-RPC endpoint alongside three cryptographic proofs:

1. **Merge mining tag** — `extract_aux_merkle_root_from_block()` verifies the
   Monero coinbase `tx_extra` contains a merge mining tag with an aux merkle root
2. **Aux merkle proof** — `MerkleProof::calculate_root()` verifies the solution's
   `aux_hash` is a leaf in the merkle tree rooted at that tag
3. **Coinbase merkle proof** — `is_coinbase_valid_merkle_root()` verifies the
   coinbase transaction is in the Monero block's transaction tree

All three proofs are packaged into `PowSource::Monero(MoneroPowData)` and
stored on-chain. Native PoW verification is skipped for merge-mined blocks
(xmrig hashes the Monero block, not the DarkWow header).

**Architecture:** `xmrig → p2pool → dwowd (mm_rpc HTTP JSON-RPC)` — no adaptor.

**Mining blob format:** 228 bytes (227-byte DarkWow header + 1-byte
`pow_source` discriminator: `0x00` = Native, `0x01` = Monero).

- Requires: p2pool + monerod (testnet, synced)
- Settlement: ~6 min (3 Monero blocks)
- Protects: merge miners only

**E2E test:**
```bash
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
  bash contrib/docker/darkwow-testnet/test_merge_mining_p2pool.sh
```

Full specification: [Monero Merge Mining](doc/src/arch/monero-merge-mining.md)

### Caribina (Arweave proof-of-storage)

Blocks are anchored to Arweave via ArDrive Turbo — free, no AR tokens required.
Each block gets a verifiable timestamp on Arweave's proof-of-storage chain.
An attacker who controls RandomX hashpower cannot forge Arweave timestamps.

- Requires: nothing (HTTP POST to ArDrive Turbo)
- Settlement: ~2 min (1 DarkWow block)
- Protects: **all** miners — native and merge

| Property | Monero Anchor | Caribina (Arweave) |
|----------|--------------|---------------------|
| Requires p2pool | Yes | **No** |
| Protects native miners | No | **Yes** |
| Settlement time | ~6 min | **~2 min** |
| Consensus basis | PoW (RandomX) | Proof-of-Storage |
| Under 51% attack | 4/5 blocks protected | **5/5 blocks protected** |

See [Caribina — Arweave-Anchored Finality](doc/src/arch/caribina.md) for the
full specification. See [Mining Tokenomics](doc/src/arch/mining-tokenomics.md#anchoring-finality-gadget)
for the Monero anchoring gadget. Both mechanisms are modeled in the
[merge mining model](contrib/model/merge_mining_model.py) (ALL VERIFIED).

Python consensus models provide 1:1 executable specifications for the Rust
implementation — see [contrib/model/](contrib/model/) for chain validation (34/34),
VM state machine, and merge mining models. Python leads, Rust follows.

---

## Smart Contracts

### Identity & Authorization
- **Identity**: O-Cap primitives, credentials, DAG-based competency claims

### Finance
- **Native Token**: Consensus-first native token (block rewards, fees, transfers)
- **Stablecoin**: Synthetix-style pooled debt model
- **DEX**: Privacy-preserving decentralized exchange
- **Escrow**: Conditional value escrow
- **DAO-Escrow**: Voluntary DAO-governed endowment (opt-in, ZK predicate-based)
- **Subscription**: Recurring payment streams
- **Bridge**: Cross-chain transfers

### Labor & Tendering
- **Labor Market**: Job posting and acceptance with O-Cap
- **Tender**: Sealed-bid procurement with O-Cap
- **Attestation**: Generalized claims and evidence verification
- **OTC Swap**: Peer-to-peer atomic token swaps with two-phase commit

### Risk & Gaming
- **Insurance Market**: Underwriting and coverage with O-Cap
- **Prediction Market**: Risk probability pricing
- **Darktoshi Dice**, **Baccarat**, **Roulette**, **Slot**: Privacy-preserving casino games
- **Lottery**: Configurable lottery combining BettingStake and Insurance
- **DarkBet Exchange**: Unified betting with order-book and AMM modes

### Infrastructure
- **Oracle**: Push-model oracle with attestation
- **Auction**: Privacy-preserving auctions

### Wallet — Manifest-First Architecture

Every DarkWow wallet (`dwow_wallet`) is a **full node** — it holds the complete
blockchain on local disk and derives all state from local computation over
sled trees and SQLite tables. There is no SPV, no light client, no network
fetches for position resolution.

The wallet is **manifest-first**: contracts carry their own interfaces on-chain
via TOML manifests embedded in deployment transactions. The wallet reads the
manifest and auto-configures — no hardcoded contract ABIs, no per-contract code.
Adding support for a new contract requires zero wallet code changes. This is a
fundamental break from upstream's client-side config model where each wallet
ships its own contract definitions and ecosystem fragments as different clients
support different subsets of contracts.

The wallet interprets on-chain state through a **capability-based model**:
coins, contract roles, ZK credentials, and DAO memberships are all capabilities.
Actions (contract function calls) require capabilities, consume some via
nullifiers, and produce new ones. A single-pass resolver scans each contract's
sled tree and derives both held capabilities and available actions in one
traversal.

Six genesis contracts provide the capability primitive layer: NativeToken
(consensus asset), Deployooor (deployment infrastructure), PromissoryNote
(tokens), Identity (credentials), Oracle (data feeds), and Attestation (trust
verification). Identity and Attestation power the manifest trust model — the
wallet mechanically verifies WASM exports against manifest claims and consults
on-chain attestations for reputation. The posture is **caveat emptor**: the
wallet warns, the user decides. See [Wallet Architecture](doc/src/arch/wallet.md).

The `dwow_wallet position` CLI command displays the user's current position —
what they hold and what they can do — with no network round-trips. See
[Wallet Architecture](doc/src/arch/wallet.md) for the full design.

---

## Developer Tooling

DarkWow ships with a **four-level testing infrastructure** that upstream DarkFi
does not have — from fast unit tests to multi-machine Docker devnets. The
extended smart contract suite (28+ contracts) is designed to be forked,
customized, and built on.

| Level | Name | Scope | Command |
|-------|------|-------|---------|
| 1 | Lightweight | Deployooor-based deployment (real production path, no ZK) | `cargo test` |
| 2 | Heavyweight | Contract functions, ZK proofs, uncle-merkle block execution | `cargo test --release` |
| 3 | Containerized Localnet | Multi-node Docker testnet (seed + miners) | `docker compose up` |
| 4 | Containerized Devnet | LAN/internet shared devnet node | `docker run --network=host` |
| Wallet | Capability resolution | L1: Bash CLI. L2: In-process Rust (20 tests). L3: Docker container. | `./test-wallet.sh`, `cargo test -p dwow_wallet --lib` |

Every contract ships with a **test harness** implementing the `ContractHarness`
trait — compile-time ZK circuit loading, typed call builders, and automated
proof verification. No blockchain required for Level 1 and 2 testing.

**Clear demarcation:** Level 1 tests deployment through the Deployooor contract
(the real production flow — validates WASM exports, checks lock status,
derives ContractId from deploy keypair). Level 2 tests contract functions,
state transitions, ZK proof verification, and uncle-merkle block execution
(multi-uncle, depth, mixed canonical/uncle, gas limits). Both are required —
deployment correctness is not tested in Level 2, and function behavior is not
tested in Level 1.

For a goal-oriented entry point, see the [Developer Quick Start Guide](doc/src/dev/quickstart.md).

### AI-Friendly Design: Vibe-Code-Safe Architecture

DarkWow is designed to make AI-assisted development safe by construction:

- **O-Cap containment**: AI-generated contract code holds only the capabilities
  explicitly passed to it. No ambient authority — a vibe-coded contract cannot
  access tokens, state, or operations it wasn't granted. Bugs are contained.

- **Deterministic consensus**: Uncle Merkle produces identical results for
  identical inputs. No timing-dependent state, no speculative rollbacks.
  AI-generated code is tested under reproducible conditions — same code, same
  block, same result, every time.

- **Four-level safety net**: The test pipeline (Level 1-4) catches compilation,
  ZK proof, networking, and deployment errors before mainnet. When used
  completely with no gaps, contracts achieve a basic level of audit superior
  to most of the industry — not because any single test is magic, but because
  the pipeline verifies failure modes that traditional audits never reach.

**The compact**: The architecture cannot save you from not using it. Run the
full pipeline. Skip nothing. Your responsibility is to leave no gaps — the
infrastructure will catch what you feed it.

This is left-accelerationist praxis in code: embrace AI to accelerate
development beyond the control of centralized gatekeepers, with the O-Cap
boundary ensuring that acceleration does not reproduce the hierarchies of
capitalist control. Accelerate the build. Contain the blast radius.

See [AI-Assisted Development](doc/src/dev/ai-assisted-development.md) for
the full workflow and philosophy.

### Docker Devnets

- **darkwow-testnet** — 3-container local devnet (lilith seed + 2 mining nodes)
  for P2P, block propagation, and mining tests. `docker compose up` and mine.
  Also serves as the public testnet node image — `docker pull` + `docker run`
  and join the live DarkWow testnet with native RandomX or Monero merge mining.
  See [darkwow-testnet README](contrib/docker/darkwow-testnet/README.md).
- **dwow-devnet** — Single-container devnet node for multi-machine LAN/internet
  deployment. Turn any Linux machine into a seed or miner.
  See [dwow-devnet README](contrib/docker/dwow-devnet/README.md).
- **bridge-node** — Single-image container for running a cross-chain bridge
  relayer with capital endowment. Combines dwowd, bridge/endowment contracts,
  and universal_relayer. Three modes: full, relayer-only, lilith.
  See [bridge-node README](contrib/docker/bridge-node/README.md).

### Contract Testing

```bash
# Level 1: Deployooor-based deployment (real production path, no ZK — seconds)
cargo test -p dwowd test_pipeline

# Level 1: Batch deploy all 21 contracts through Deployooor
cargo test -p dwowd test_all_contracts_deploy

# Level 2: Contract functions, ZK proofs, uncle-merkle block execution (minutes)
RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd test_heavyweight

# Level 3: Containerized localnet — multi-node Docker mining + contract tests
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native

# Level 3: Multi-node Docker testnet with live mining + contract tests
./contrib/docker/darkwow-testnet/test-contracts.sh

# Native (no Docker): join public testnet, mine, deploy contracts, send transfers
./contrib/docker/darkwow-testnet/join-testnet.sh --mode native

# Wallet capability resolution tests
RAYON_NUM_THREADS=10 bash bin/drk/test_capability_lightweight.sh  # Level 1: CLI integration
cargo test -p dwow_wallet --lib -- capability::tests                # Level 2: 20 in-process resolver tests
./contrib/docker/darkwow-testnet/test-wallet.sh             # Level 3: Docker container integration test
```

**Fork, build, customize.** Every contract in `src/contract/<name>/` is
self-contained with its own ZK circuits, tests, and harness — use any contract
as a template for your own.

---

## Build

```shell
git clone https://codeberg.org/PatrickM123/darkwow
cd darkwow
rustup target add wasm32-unknown-unknown
make
```

Minimum Rust version: **1.87.0**.

### Running a Node

```bash
# Local development (single node, skip sync, 28xxx ports)
cargo run -p dwowd -- --network darkwow-devnet

# DarkWow public testnet (DRKW magic, 31xxx ports, P2P seeds)
cargo run -p dwowd -- --network darkwow-testnet
```

| Network | Magic | Ports | Sync | Difficulty | Purpose |
|---------|-------|-------|------|------------|---------|
| `darkwow-devnet` | (none) | 28xxx | skip_sync, localnet | Fixed | Local dev iteration |
| `darkwow-testnet` | DRKW | 31xxx | Full P2P sync | Variable | Public coordination |
| `mainnet` | (TBD) | (TBD) | Full P2P sync | Variable | Production |

### Wallet Network Connectivity

The wallet is a **full node** — it connects to P2P seeds, discovers peers via
hostlist, syncs the chain, and scans for coins using the same P2P protocol as
every other node. It does **not** use RPC to sync the chain. RPC is for dwowd
management queries only (`blockchain.get_height`, etc.).

The only thing that changes between environments is the **seed address**:

| Environment | Seed address | How it works |
|------------|-------------|---------------|
| Docker container | `tcp+tls://lilith:31340` | Docker DNS resolves `lilith` inside the bridge network |
| Host ↔ Docker devnet | `tcp+tls://127.0.0.1:31340` | Lilith P2P port (31340) published to host loopback |
| Public testnet | `tcp+tls://<seed IP>:31340` | Public IP of a lilith seed node |

```toml
# ~/.config/dwow/drk.toml — wallet config override for host ↔ Docker devnet
[network_config."darkwow-testnet".net]
seeds = ["tcp+tls://127.0.0.1:31340"]
```

This is the same pattern as Bitcoin Core's `addnode`, Geth's `bootnodes`, and
every other P2P node. One config field. One protocol. No special sync paths.

---

## Documentation

### Getting Started
- [Developer Quick Start Guide](doc/src/dev/quickstart.md) — Goal-oriented entry point: "I want to do X — what do I run?"
- [Native Mining + Contract Workflow](doc/src/dev/native-workflow.md) — Run a node, mine DRKW, deploy contracts (no Docker)

### Testing Infrastructure
- [AI-Assisted Development](doc/src/dev/ai-assisted-development.md) — Vibe-coding guide, AI safety architecture, pipeline audit philosophy
- [Testing Overview](doc/src/dev/testing/overview.md) — Full four-level taxonomy with file map
- [Level 1: Lightweight Tests](doc/src/dev/testing/level-1-lightweight.md) — Unit/integration, no ZK overhead
- [Level 2: Heavyweight Tests](doc/src/dev/testing/level-2-heavyweight.md) — Full ZK proofs, contract execution
- [Level 3: Containerized Localnet](doc/src/dev/testing/level-3-localnet.md) — Docker architecture, wallet setup
- [Level 4: Containerized Devnet](doc/src/dev/testing/level-4-devnet.md) — Multi-machine deployment

### Architecture
- [Architecture Overview](doc/src/arch/overview.md)
- [Uncle Merkle Consensus](doc/src/arch/consensus/consensus.md)
- [Caribina — Arweave-Anchored Finality](doc/src/arch/caribina.md)
- [Monero Merge Mining](doc/src/arch/monero-merge-mining.md) — Full p2pool protocol specification
- [Network Types](doc/src/arch/network-types.md) — darkwow-devnet vs darkwow-testnet vs mainnet
- [Monero Anchoring Finality](doc/src/arch/mining-tokenomics.md#anchoring-finality-gadget)
- [Contract Metadata](doc/src/arch/contract-metadata.md) — On-chain metadata, attestations, future verification pipeline
- [O-Cap Authorization](doc/src/arch/ocap.md)
- [Opcodes & Formal Verification](doc/src/arch/zk/opcodes.md)
- [Security Analysis](doc/src/arch/security-analysis.md)

### Contracts
- [Contract Development Guide](doc/src/dev/contracts.md)
- [Contract Standards](doc/src/dev/contracts/standards.md) — ZK circuit rules, token layer architecture
- [ZK Circuit Troubleshooting](doc/src/dev/zk-circuit-troubleshooting.md)

### Operations
- [Contributing & Developer Guide](doc/src/dev/contrib/contrib.md)
- [DarkWow Devnets](contrib/docker/darkwow-testnet/README.md) — Range of containerized devnet options (local, LAN, public testnet)
- [Public Testnet Node](contrib/docker/darkwow-testnet/README.md) — Docker Hub image, native/merge mining, wallet setup
- [Bridge Node](contrib/docker/bridge-node/README.md) — Cross-chain bridge relayer with capital endowment

---

## Build & Test Status

| Level | Scope | Command |
|-------|-------|---------|
| 1 — Lightweight | Deployooor-based deployment (21 contracts, no ZK) | `cargo test -p dwowd test_all_contracts_deploy` |
| 2 — Heavyweight | Contract functions, ZK proofs, uncle-merkle stress (36 tests) | `RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd test_heavyweight` |
| 3 — Localnet | Multi-node Docker mining + contracts | `./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native` |
| Wallet Spec | Python canonical model (14 tests, 1:1 Rust mapping) | `python3 contrib/model/wallet_model.py` |
| Wallet Sim | Chain→wallet bridge simulation (6 tests, mining + reorg) | `python3 contrib/model/wallet_simulation.py` |
| Wallet L1 | CLI integration (bash, 8 assertions) | `RAYON_NUM_THREADS=10 bash bin/drk/test_capability_lightweight.sh` |
| Wallet L2 | Unit tests (43 in-process tests) | `RAYON_NUM_THREADS=10 cargo test -p dwow_wallet --lib` |
| Wallet L3 | Docker container integration test | `RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/test-wallet.sh` |
| Wallet Multi | Cross-wallet tx mesh (deploy, mint, OTC swap) | `RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/test-wallet-transactions.sh` |
| Wallet L4 | Per-contract wallet verification (17 contracts) | `RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/contract-tests/run-all.sh` |

> **USE AT YOUR OWN RISK.** No third-party audit. For current audit status see
> [Smart Contract Safety](doc/src/dev/contracts/safety.md).

### Proof of Token Balance

DarkWow provides a **cryptographic proof of total supply** enforced as an active
consensus rule — a direct response to the Zcash Orchard exploit (May 2026):

**Per-block Pedersen mass balance**: Every block must satisfy
`Σ outputs + Σ burns + Σ fees == Σ inputs` for the native darkw token. This proves
that non-coinbase transactions do not secretly mint new supply. The check uses
Pedersen commitment additive homomorphism — all `Input.value_commit` and
`Output.value_commit` points are summed and verified equal, without revealing any
plaintext values. The coinbase reward is excluded from the sums and verified
separately against the emission schedule.

**Cumulative supply commitment chain**: Each coinbase ZK proof constrains
`S_H = S_{H-1} + C_H` via `ec_add` in the circuit. Any node can independently
walk the chain and verify that `S_H` matches the expected cumulative supply using
pure Pedersen arithmetic — without verifying a single ZK proof.

These two properties rely on independent cryptographic assumptions (Halo2
soundness vs. Pedersen binding). Breaking one doesn't break the other.

The proof of token balance is enforced at **every block acceptance path** in
`dwowd` — P2P broadcast, built-in miner, RPC miner, stratum, merge mining, and
consensus sync. A block that fails the mass balance check is **rejected** before
it can be applied to the chain.

Implementation: `bin/dwowd/src/proof_of_token_balance.rs`. Python model:
`contrib/model/proof_of_token_balance.py`. See
[Consensus: Supply Audit](doc/src/arch/consensus/consensus.md#supply-audit-capability).

---

## License

AGPL-3.0-only. See [LICENSE.md](LICENSE.md).

DarkWow is a tool for people and nations to establish sovereignty according to human rights law. See the [UN Declaration on the Rights of Indigenous Peoples](https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf).

---

**Go Dark.**
