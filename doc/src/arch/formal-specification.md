# DarkWow — Formal Specification

This is the one page to read. For new developers, it's your starting point.
For experienced contributors, it's the reference you check when in doubt.

## What DarkWow Is

DarkWow is a privacy-preserving smart contract platform built on a zkVM
(zero-knowledge virtual machine) with Halo2 proving, WASM contract runtime,
and uncle-Merkle consensus. It began as a fork of DarkFi and makes six
architectural commitments:

1. **No DAO governance** — OCap (object capability) model instead of token-weighted voting
2. **No premine** — every DRKW minted via PoW
3. **Formally verified** — All 32 zkVM opcodes, 10 gadgets, and 120 contract ZK circuits proven sound in Lean4 (Orchard-class audit complete, 1 critical bug found and fixed)
4. **No overlay/DAG consensus** — deterministic uncle-Merkle chain with linear blocks

## Genesis

Nine contracts are deployed at genesis. See [Genesis Contracts](genesis.md)
for the complete list with ContractId derivation, consensus vs. ecosystem
classification, and bootstrap sequence.
Attestation power Layer 3 (trusted binary attestation), Oracle provides the external
data feeds that attestation predicates depend on. None of these contracts play any
role in chain consensus — they are ecosystem infrastructure, like ERC-20 pre-deploys.

All other contracts (23 of them) are WASM-deployed post-genesis via Deployooor.

## Wallet Architecture

The wallet follows the **Bitcoin Core pattern**: process separation between a
sync daemon (`dwowd`) and a command-line wallet (`dwow_wallet`) that talks to it
via JSON-RPC.

- **Sync by default** — only 5 network commands use `smol::block_on`. Everything else is synchronous local computation.
- **Visible code** — `fn main()` is 55 lines of wiring. No macro-generated code paths.
- **Modular** — `args.rs`, `config.rs`, `dispatch.rs`, `wallet.rs` are independent modules.
- **Result propagation** — no `exit()`, no `unwrap()`. Every function returns `Result`.

See [Wallet Architecture](wallet.md) for the full specification.

## Trust Model — Don't Trust, Verify

Three independent layers:

| Layer | Question | Mechanism | Trust Required |
|-------|----------|-----------|----------------|
| **Trust Tier** | Who deployed this? | Genesis check, self-deploy check, attestation lookup | Social |
| **WASM Verification** | Does the manifest match the binary? | Parse WASM exports + circuits, compare against manifest | **None** (mechanical) |
| **Attestation** | Does the binary do what it claims? | Trusted issuer inspects WASM, creates on-chain attestation | Social (deferred) |

See [Contract Trust Model](contract-trust-model.md) for the full specification.

## Contract Ecosystem — 32 Contracts with Manifests

Every contract has an on-chain **manifest** — a TOML document describing its
functions, capability types, actions, state trees, and ZK circuits. The manifest
enables any wallet to interact with any contract without hardcoded Rust knowledge.
See [Contract Manifest](manifest.md) for the full specification.

The wallet discovers contracts by scanning `DeployV1` transactions. When a
manifest is found (magic byte `0x4D` prefix), it's parsed, stored in SQLite,
and used for capability resolution. Contracts without manifests fall back to
generic AEAD discovery.

### Genesis (9)

All nine genesis contracts — see [Genesis Contracts](genesis.md).

### DeFi (8)

- [Stablecoin](../contract/stablecoin.md) — CDP stablecoin with governance
- [DEX](../contract/dex.md) — atomic swap DEX with governance-signed config
- [Bridge](../contract/bridge.md) — cross-chain bridge (ETH, XMR, ZEC, etc.)
- [OTC Swap](../contract/otc_swap.md) — peer-to-peer atomic swap
- [Escrow](../contract/escrow.md) — simple timeout-based escrow
- [Auction](../contract/auction.md) — sealed-bid auction
- [Bearer Bond](../contract/bearer_bond.md) — bond issuance and staking
- [Pool Stake](../contract/pool_stake.md) — risk pooling and coverage allocation

### Gaming (7)

- [Baccarat](../contract/baccarat.md), [Darktoshi Dice](../contract/darktoshi_dice.md), [Roulette](../contract/roulette.md), [Slot](../contract/slot.md), [Lottery](../contract/lottery.md), [Game Room](../contract/game_room.md), [DarkBet Exchange](../contract/darkbet_exchange.md)

### DAO & Governance (3)

- [DAO Escrow](../contract/dao_escrow.md) — OCap-governed endowment
- [Drain Protection](../contract/drain_protection.md) — fund locking with exit mechanism
- [Betting Stake](../contract/betting_stake.md) — staking with risk updates

### Identity & Reputation (3)

- [Identity](../contract/identity.md) — credential issuance and verification
- [Attestation](../contract/attestation.md) — attestation framework
- [Subscription](../contract/subscription.md) — subscription service

### Labor & Markets (3)

- [Labor Market](../contract/labor_market.md) — job marketplace with milestones
- [Insurance Market](../contract/insurance_market.md) — risk underwriting
- [Tender](../contract/tender.md) — sealed-bid tendering

### Infrastructure (2)

- [Oracle](../contract/oracle.md) — data feed with ZK-proof authentication
- [Relayer Endowment](../contract/relayer_endowment.md) — relayer funding and deployment

## Testing — 29/29 Coverage

Every contract has both lightweight and heavyweight tests:

- **Lightweight** (29 tests): deploy contract via Deployooor, verify manifest storage
- **Heavyweight** (29 tests): deploy, build ZK proofs, execute through dwowd runtime
- **Python model** (71 tests): wallet specification — parse_args, load_config, manifest lifecycle, trust resolution, WASM verification

See [Testing Overview](../dev/testing/overview.md) for the full taxonomy.

## Getting Started

```
# Clone and build
git clone https://codeberg.org/PatrickM123/darkwow
# Mirror: git clone https://github.com/PatrickMockridge/DarkWow
cd darkwow
cargo build --release -p dwowd -p dwow_wallet

# Run the test pipeline (Docker required)
./contrib/docker/darkwow-testnet/test_pipeline.sh --mode wallet --fresh

# Generate a wallet keypair
./target/release/dwow_wallet -c dww_config.toml wallet keygen

# Deploy a contract with a manifest
./target/release/dwow_wallet -c dww_config.toml contract deploy <auth> <wasm> --manifest manifest.toml

# Show a contract's interface from its manifest
./target/release/dwow_wallet -c dww_config.toml contract show <contract_id>

# Verify a contract's manifest against its WASM
./target/release/dwow_wallet -c dww_config.toml contract verify <contract_id>
```

## Where to Go Next

| I want to... | Read this |
|-------------|-----------|
| Understand the architecture | [Wallet Architecture](wallet.md) |
| Deploy a contract | [Contract Manifest](manifest.md) |
| Verify a contract | [Contract Trust Model](contract-trust-model.md) |
| Write a ZK circuit | [Writing ZK Proofs](../zkas/writing-zk-proofs.md) |
| Run a node | [Running a Node](../testnet/node.md) |
| Understand the consensus | [Consensus](consensus/consensus.md) |
| Build a DEX or bridge | [DEX](../contract/dex.md), [Bridge](../contract/bridge.md) |
| Mine | [Mining on Testnet](../testnet/testnet-mining.md) |
| See all docs | [Documentation Index](../SUMMARY.md) |
