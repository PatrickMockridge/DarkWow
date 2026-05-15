# DarkWow

A DarkFi fork rebuilt around **four rejections of upstream**:

1. **No governance DAO** — pure PoW, no token-holder voting
2. **No overlay/diff consensus** — deterministic Uncle Merkle with stateless verification
3. **LessThanOrEqual and BaseDiv opcodes built and proven sound in Lean4 on this fork** — additions to upstream's zkVM, not inherited
4. **No premine** — every coin mined

Zero vendor lock-in. Genesis is two contracts — NativeToken and Deployooor. Hard forks are a feature, not a threat. Extended and entirely voluntary smart contract feature set including but not limited to: Darktoshi Dice, DAO (with Escrow and Drain protection), DEX, Stablecoin, Prediction Market, Betting Stake, Identity, Sealed Bidding/Tendering, Labor Market and more.

<!-- TODO: website placeholder — https://darkwow.org -->

Development occurs on the **`linear-master`** branch.

---

## Four Refutations in Detail

### 1. No Governance DAO — Pure PoW

Upstream DarkFi's DAO can freeze the native token through token-holder voting. If governance can restrict minting, miners may not get paid and PoW consensus becomes attackable by wealthy token holders. DarkWow removes all governance control over the native token. There is no DAO that can freeze consensus-critical functions.

### 2. Uncle Merkle Consensus — No Overlay/Diff

Upstream's overlay/diff consensus uses speculative state (checkpoint → commit → rollback), where `diff()` computation depends on sequence history — the same code produces different results depending on timing. This makes deterministic testing effectively impossible.

DarkWow replaces this entirely with **Uncle Merkle consensus**: the canonical chain with the most accumulated work obligates offering uncle chains a one-time option to form a side chain and share the PoW reward. Stateless verification via pure merkle proof — no overlay needed. Same block always produces the same result.

See [Uncle Merkle Consensus](doc/src/arch/consensus/uncle_merkle.md) for the full specification.

### 3. ZK Opcodes — Built and Formally Verified in Lean4

Upstream's zkVM has no `LessThanOrEqual` or `BaseDiv` opcodes. These were built on this fork — `LessThanOrEqual` (0x55) enables conditional logic and O-Cap predicate evaluation in circuits; `BaseDiv` (0x58) enables precise field division for cold-circuit governance operations (stablecoin interest accrual, governance ratio checks). Both have been formally proven sound using the Lean4 proof assistant, with machine-checkable proofs of correctness living in this repository (`proofs/lean/`).

See [Opcodes and Formal Verification](doc/src/arch/zk/opcodes.md) for the full verification analysis.

### 4. No Premine — Every Coin Mined

Upstream DarkFi distributed tokens at genesis to early investors, team members, and SAFT participants — creating concentrated whale dominance. DarkWow has zero pre-mined tokens. Every unit of the native token is earned through RandomX Proof-of-Work mining, following a continuous exponential decay schedule with a permanent 1% tail emission, converging asymptotically toward a 21 million supply cap.

See [Blockchain Rewards](src/sdk/src/blockchain.rs) for the reward schedule constants.

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

### Risk & Gaming
- **Insurance Market**: Underwriting and coverage with O-Cap
- **Prediction Market**: Risk probability pricing
- **Darktoshi Dice**, **Baccarat**, **Roulette**, **Slot**: Privacy-preserving casino games
- **Lottery**: Configurable lottery combining BettingStake and Insurance
- **DarkBet Exchange**: Unified betting with order-book and AMM modes

### Infrastructure
- **Oracle**: Push-model oracle with attestation
- **Auction**: Privacy-preserving auctions
- **Atomic Swap**: Cross-chain swaps

---

## Developer Tooling

DarkWow ships with a **four-level testing infrastructure** that upstream DarkFi
does not have — from fast unit tests to multi-machine Docker devnets. The
extended smart contract suite (28+ contracts) is designed to be forked,
customized, and built on.

| Level | Name | Scope | Command |
|-------|------|-------|---------|
| 1 | Lightweight | Unit/integration tests, no ZK overhead | `cargo test` |
| 2 | Heavyweight | Full ZK proofs, contract execution | `cargo test --release` |
| 3 | Containerized Localnet | Multi-node Docker testnet (seed + miners) | `docker compose up` |
| 4 | Containerized Devnet | LAN/internet shared devnet node | `docker run --network=host` |

Every contract ships with a **test harness** implementing the `ContractHarness`
trait — compile-time ZK circuit loading, typed call builders, and automated
proof verification. No blockchain required for Level 1 and 2 testing.

For a goal-oriented entry point, see the [Developer Quick Start Guide](doc/src/dev/quickstart.md).

### Docker Devnets

- **darkwow-testnet** — 3-container local devnet (lilith seed + 2 mining nodes)
  for P2P, block propagation, and mining tests. `docker compose up` and mine.
  See [darkwow-testnet README](contrib/docker/darkwow-testnet/README.md).
- **dwow-devnet** — Single-container devnet node for multi-machine LAN/internet
  deployment. Turn any Linux machine into a seed or miner.
  See [dwow-devnet README](contrib/docker/dwow-devnet/README.md).
- **testnet-node** — Single-image, dual-mode container for joining the **public
  DarkWow testnet** as a mining node. `docker pull` and mine with native RandomX
  or Monero merge mining via p2pool.
  See [testnet-node README](contrib/docker/testnet-node/README.md).

### Contract Testing

```bash
# Level 1: Fast deployment checks (seconds)
cargo test -p dwowd test_pipeline

# Level 2: Full ZK proof tests (minutes)
RAYON_NUM_THREADS=10 cargo test --release -p dwowd test_heavyweight

# Level 3: Full test pipeline — 5 modes (clean → build → verify)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native

# Level 3: Multi-node Docker testnet with live mining + contract tests
./contrib/docker/darkwow-testnet/test-contracts.sh

# Native (no Docker): mine on public testnet, deploy contracts, send transfers
./contrib/docker/testnet-node/native-workflow.sh
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
# Local development (single node)
cargo run -p dwowd -- --network linear-testnet

# DarkWow public testnet
cargo run -p dwowd -- --network darkwow-testnet
```

---

## Documentation

### Getting Started
- [Developer Quick Start Guide](doc/src/dev/quickstart.md) — Goal-oriented entry point: "I want to do X — what do I run?"
- [Native Mining + Contract Workflow](doc/src/dev/native-workflow.md) — Run a node, mine DRKW, deploy contracts (no Docker)

### Testing Infrastructure
- [Testing Overview](doc/src/dev/testing/overview.md) — Full four-level taxonomy with file map
- [Level 1: Lightweight Tests](doc/src/dev/testing/level-1-lightweight.md) — Unit/integration, no ZK overhead
- [Level 2: Heavyweight Tests](doc/src/dev/testing/level-2-heavyweight.md) — Full ZK proofs, contract execution
- [Level 3: Containerized Localnet](doc/src/dev/testing/level-3-localnet.md) — Docker architecture, wallet setup
- [Level 4: Containerized Devnet](doc/src/dev/testing/level-4-devnet.md) — Multi-machine deployment

### Architecture
- [Architecture Overview](doc/src/arch/overview.md)
- [Uncle Merkle Consensus](doc/src/arch/consensus/consensus.md)
- [O-Cap Authorization](doc/src/arch/ocap.md)
- [Opcodes & Formal Verification](doc/src/arch/zk/opcodes.md)
- [Security Analysis](doc/src/arch/security-analysis.md)

### Contracts
- [Contract Development Guide](doc/src/dev/contracts.md)
- [Contract Standards](doc/src/dev/contracts/standards.md) — ZK circuit rules, token layer architecture
- [ZK Circuit Troubleshooting](doc/src/dev/zk-circuit-troubleshooting.md)

### Operations
- [Contributing & Developer Guide](doc/src/dev/contrib/contrib.md)
- [darkwow-testnet Pipeline](contrib/docker/darkwow-testnet/README.md) — 5-mode test pipeline, Docker images, compose profiles
- [Public Testnet Node](contrib/docker/testnet-node/README.md) — Docker Hub image, native/merge mining, wallet setup
- [dwow-devnet Node](contrib/docker/dwow-devnet/README.md) — Multi-machine shared devnet

---

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**. Known issues are documented in [Security Analysis](doc/src/arch/security-analysis.md).

---

## License

AGPL-3.0-only. See [LICENSE.md](LICENSE.md).

DarkWow is a tool for people and nations to establish sovereignty according to human rights law. See the [UN Declaration on the Rights of Indigenous Peoples](https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf).

---

**Go Dark.**
