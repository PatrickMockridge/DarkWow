# DarkWow

A privacy-preserving blockchain rebuilt around **six design commitments**:

1. **O-Cap governance primitives instead of a monolithic DAO** —
   Purse, Box, Identity, MultiSig, Oracle, and Attestation provide modular
   self-governance, private voting, treasury management, and trust. Every
   user builds their own organisations from composable pieces.
   [Read more →](doc/src/arch/wallet.md)

2. **Uncle Merkle consensus with stateless verification** —
   No overlay/diff. Deterministic fork resolution. Competing blocks earn
   partial rewards via uncle-merkle inclusion. [Read more →](doc/src/arch/consensus/uncle_merkle.md)

3. **Sovereign keys, deterministic wallet** —
   Keys are owned by the user, not the daemon. Never delegated. The wallet
   derives its identity on boot and scans locally — the only async operation
   is P2P chain sync. Uncle Merkle provides forward-only state: same keys +
   same chain = identical wallet state, every time.
   [Read more →](doc/src/arch/wallet.md)

4. **ZKVM opcodes proven sound in Lean4** —
   `LessThanOrEqual`, `IsNotEqual`, and `BaseDiv` opcodes formally verified
   on this fork — not inherited from upstream. [Read more →](doc/src/arch/zk/opcodes.md)

5. **No premine** — Every coin mined. No SAFT, no insider allocation.

6. **Per-block Pedersen mass balance** — A direct response to the Zcash
   Orchard exploit (May 2026). Every block must satisfy `Σ outputs + Σ burns +
   Σ fees == Σ inputs` via additive homomorphism. Cumulative supply commitment
   chain verifiable without ZK proofs. Enforced at every block acceptance path.
   [Read more →](doc/src/arch/consensus/consensus.md#supply-audit-capability)

Zero vendor lock-in. 32 contracts (9 genesis + 23 works in progress). Hard forks are a feature, not a threat.

See [What's Different from Upstream](doc/src/about/differences_from_upstream.md)
for the full comparison table, and [Philosophy](doc/src/philosophy/philosophy.md)
for the design rationale behind the fork.

Development occurs on the **`linear-master`** branch.

---

## Architecture

**[O-Cap Authorization](doc/src/arch/ocap.md)** —
Capabilities are cryptographic secrets, not object references. Proving
knowledge of the secret IS the authority. Identity never revealed.
[Read more →](doc/src/arch/ocap.md)

**[Uncle Merkle Consensus](doc/src/arch/consensus/uncle_merkle.md)** —
Competing blocks at the same height become uncles. The canonical block's
miner shares the reward with uncle miners via merkle proofs. No overlay/diff.
[Read more →](doc/src/arch/consensus/consensus.md)

**[Dual Finality](doc/src/arch/caribina.md)** —
Two independent security layers protect against 51% attacks:
Caribina (Arweave proof-of-storage, ~2min settlement, protects all miners)
and Monero merge mining (p2pool, ~6min settlement, protects merge miners).
[Caribina →](doc/src/arch/caribina.md)
[Monero →](doc/src/arch/monero-merge-mining.md)

**[Genesis & Manifests](doc/src/arch/genesis.md)** —
Nine contracts deploy to an empty chain at block 1: NativeToken, Deployooor,
PromissoryNote, Identity, Oracle, Attestation, Purse, Box, and MultiSig. Each
carries a TOML manifest on-chain declaring its interface — WASM exports, ZK
circuits, state schema, capability requirements. The wallet reads these
manifests and auto-configures. Adding a new contract requires zero wallet
code changes. Both miner and wallet verify WASM exports against manifest
claims at startup. [Read more →](doc/src/arch/manifest.md)

**[Supply Audit](doc/src/arch/consensus/consensus.md#supply-audit-capability)** —
Per-block Pedersen mass balance proves no hidden inflation. Cumulative
supply commitment chain verifiable without ZK proofs. Two independent
cryptographic assumptions (Halo2 + Pedersen binding).
[Read more →](doc/src/arch/consensus/consensus.md#supply-audit-capability)

**[Wallet as Full Node](doc/src/arch/wallet.md)** —
Every wallet holds the complete blockchain on local disk. Manifest-first
architecture: contracts carry their own interfaces on-chain. Auto-configures —
zero wallet code changes for new contracts.
[Read more →](doc/src/arch/wallet.md)

### I want to...

- **Build contracts** → [Contract Developer Guide](doc/src/for-contract-developers.md) — dao_escrow case study, O-Cap primitives, testing pipeline
- **Run a node or mine** → [Node Operator Guide](doc/src/for-node-operators.md) — Uncle Merkle consensus, merge mining, monetary policy
- **Research the cryptography** → [Researcher Guide](doc/src/for-researchers.md) — ZK circuits, ρ-calculus type system, Lean4 verification

---

32 contracts (9 genesis + 23 works in progress) covering identity, DeFi, DAO, gaming, infrastructure, and markets. See [Contract Status](doc/src/contracts.md) for per-contract details.
All self-contained with their own ZK circuits, tests, and harnesses.

| Category | Contracts |
|----------|-----------|
| **Genesis** | NativeToken, Deployooor, PromissoryNote, Identity, Oracle, Attestation, Purse, Box, MultiSig |
| **Finance** | Stablecoin, DEX, Escrow, DAO-Escrow, Subscription, Bridge, OTC Swap, Bearer Bond |
| **Labour** | Labor Market, Tender |
| **Gaming** | Darktoshi Dice, Baccarat, Roulette, Slot, Lottery, Betting Stake, DarkBet Exchange, Game Room |
| **Risk** | Insurance Market, Drain Protection, Pool Stake |
| **Infrastructure** | Auction, Relayer Endowment |

[Full contract docs →](doc/src/contracts.md)

---

## Quick Start

```bash
git clone https://codeberg.org/PatrickM123/darkwow
# Mirror: git clone https://github.com/PatrickMockridge/DarkWow
cd darkwow
rustup target add wasm32-unknown-unknown
make

# Local dev node
cargo run -p dwowd -- --network darkwow-devnet

# Public testnet
cargo run -p dwowd -- --network darkwow-testnet
```

Rust: **stable** (see `rust-toolchain.toml`).

| Network | Magic | Ports | Purpose |
|---------|-------|-------|---------|
| `darkwow-devnet` | (none) | 28xxx | Local dev iteration |
| `darkwow-testnet` | DRKW | 31xxx | Public coordination |
| `mainnet` | (TBD) | (TBD) | Production |

[Docker devnets →](contrib/docker/darkwow-testnet/README.md)
[Docker build guide →](contrib/docker/darkwow-testnet/README.md)
[Network types →](doc/src/arch/network-types.md)

---

## Documentation

### Getting Started
- [Developer Quick Start Guide](doc/src/dev/quickstart.md)
- [Native Mining + Contract Workflow](doc/src/dev/native-workflow.md)
- [AI-Assisted Development](doc/src/dev/ai-assisted-development.md)

### Architecture
- [Overview](doc/src/arch/overview.md)
- [Uncle Merkle Consensus](doc/src/arch/consensus/consensus.md)
- [Caribina Finality](doc/src/arch/caribina.md)
- [Monero Merge Mining](doc/src/arch/monero-merge-mining.md)
- [O-Cap Authorization](doc/src/arch/ocap.md)
- [Wallet Architecture](doc/src/arch/wallet.md)
- [Observer (Relay Node)](doc/src/arch/observer.md)
- [Key Management](doc/src/arch/key-management.md)
- [Security Analysis](doc/src/arch/security-analysis.md)
- [Opcodes & Formal Verification](doc/src/arch/zk/opcodes.md)

### Contracts
- [Contract Index](doc/src/contracts.md)
- [Contract Development Guide](doc/src/dev/contracts.md)
- [Contract Standards](doc/src/dev/contracts/standards.md)

### Testing
- [Testing Overview](doc/src/dev/testing/overview.md)
- [Level 1: Lightweight](doc/src/dev/testing/level-1-lightweight.md)
- [Level 2: Heavyweight](doc/src/dev/testing/level-2-heavyweight.md)
- [Level 3: Localnet](doc/src/dev/testing/level-3-localnet.md)
- [Level 4: Devnet](doc/src/dev/testing/level-4-devnet.md)

### Operations
- [Public Testnet Node](contrib/docker/darkwow-testnet/README.md)
- [Bridge Node](contrib/docker/bridge-node/README.md)
- [Contributing](doc/src/dev/contrib/contrib.md)

---

## Test Status

| Scope | Command |
|-------|---------|
| Pre-production bridge | `RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd --lib` |
| Lightweight (deployment) | `cargo test -p dwowd test_all_contracts_deploy` |
| Heavyweight (ZK proofs) | `./bin/dwowd/src/tests/heavyweight.sh --all` |
| Localnet (Docker) | `./contrib/docker/darkwow-testnet/test_pipeline.sh --mode native` |
| Wallet spec (Python) | `python3 contrib/model/wallet_model.py` |
| Wallet unit | `RAYON_NUM_THREADS=10 cargo test -p dwow_wallet --lib` |
| Wallet integration (Docker) | `RAYON_NUM_THREADS=10 bash contrib/docker/darkwow-testnet/test-wallet.sh` |

> See [Testing Overview](doc/src/dev/testing/overview.md) for test taxonomy, bridge test specifications, and MoC boundaries.

> **USE AT YOUR OWN RISK.** No third-party audit. See [Smart Contract Safety](doc/src/dev/contracts/safety.md).

---

## License

AGPL-3.0-only. See [LICENSE.md](LICENSE.md).
