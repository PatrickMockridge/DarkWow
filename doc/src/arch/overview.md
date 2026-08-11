# Overview

DarkWow is a layer-one Proof-of-Work blockchain supporting anonymous WASM smart contracts.

## Consensus

DarkWow uses **Uncle Merkle consensus** with RandomX Proof-of-Work. This replaces upstream's overlay/diff architecture with a deterministic design:

- **Pure PoW**: The canonical chain is the one with the most accumulated work. No governance DAO decides between forks.
- **Uncle Merkle pin mechanism**: The canonical chain is **obligated** to offer competing uncle chains a one-time option (within minutes) to form a side chain and share the PoW reward. Uncle chains can accept or reject.
- **Pareto efficient**: Miners who produce non-canonical blocks still earn partial reward (50% at depth 1, halving each depth). No wasted work.
- **Deterministic**: No overlay, no speculative state, no rollback. Same block = same result every time.
- **Hard forks are natural**: Without a DAO motivated to keep everything under one tent, chain splits follow the Bitcoin model — both sides coexist (like BTC/BCH). No complex mechanism needed.

See [Consensus](consensus/consensus.md) and [Uncle Merkle](consensus/uncle_merkle.md) for the full specification.

### Network Participants

DarkWow has three node types, each with a distinct role:

| Type | Mines | Validates | Relays | Genesis | Hardware |
|------|-------|-----------|--------|---------|----------|
| **Mining node** | Yes | Yes | Yes | Optional | 6+ GB RAM, fast CPU |
| **Observer (relay)** | No | Yes | Yes | No | 2 GB RAM, Raspberry Pi |
| **Wallet (full node)** | No | Scan-only | Yes | No | 2 GB RAM |

See [Observer](observer.md) for the relay node specification.
See [Wallet Architecture](wallet.md) for the wallet full node design.

## WASM Contracts

DarkWow uses WASM smart contracts deployed via the **Deployooor** contract. This model provides:

- **Upgradeable contracts**: Contracts can be upgraded without hard forking the network
- **Genesis contracts**: Nine contracts deployed at block 1 (see [Genesis Contracts](genesis.md) for the full list)
- **Composable applications**: 23 additional contracts deployed post-genesis via Deployooor (see [Contracts](../contracts.md) for the full catalog)
- **On-chain metadata**: Contracts carry self-declared metadata (name, symbol, category)
  in the `ix` field of `DeployParamsV1`, with an extensible attestation slot for future
  DAO/auditor verification. See [Contract Metadata](contract-metadata.md).

## Token Architecture

DarkWow separates token concerns into two specialized contracts:

| Contract | Purpose | Use Case |
|----------|---------|----------|
| **NativeToken (WASM)** | Consensus-layer operations | Block rewards, fee payment |
| **promissory_note** | Privacy-first DeFi tokens | User tokens, DeFi operations |

### NativeToken

Minimal WASM contract handling only consensus requirements:
- Block reward distribution
- Fee payment

Philosophy: **Tokens are pipework, not reactors.** One job, done well.

### promissory_note

Privacy-first DeFi token contract:
- **Poseidon-only ZK circuits**: All cryptographic operations use Poseidon hash. No EC operations in ZK.
- **Coin model**: `poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)`
- **Function IDs**: TokenMintV1, MintV1, BurnV1, TransferV1, OtcSwapV1

## Cross-Contract Calls

Contracts communicate via **spend hooks**. A contract can call another by specifying:

- `spend_hook`: Which function to invoke (function ID)
- `user_data`: Arbitrary data passed to the hook

Example usage:
- **DEX ExecuteSwapV1**: Uses `otc_swap_v1` child call for bilateral token swap
- **Stablecoin MintStableV1**: Uses `transfer_v1` child call to move minted stablecoins to user
- **DarkbetExchange**: Uses `transfer_v1` child calls for position minting/burning

## ZK Proofs

All private state transitions use ZK proofs verified on-chain:

- **zkVM**: DarkWow's virtual machine executes Halo2 proofs
- **Transparent setup**: Halo2 uses polynomial commitments over the Pasta cycle (Pallas/Vesta) — no trusted setup ceremony required
- **Privacy**: Zero-knowledge proofs hide amounts, identities, and state changes

## Testing

DarkWow provides a four-level testing infrastructure, from fast unit tests to
multi-machine Docker devnets — a key differentiator from upstream DarkFi:

1. **Level 1 (Lightweight)**: Unit and integration tests with no ZK overhead (seconds)
2. **Level 2 (Heavyweight)**: Full ZK proof generation and contract execution (minutes)
3. **Level 3 (Containerized Localnet)**: Multi-node Docker testnet for P2P and mining tests
4. **Level 4 (Containerized Devnet)**: Single-container node for LAN/internet shared devnets

Start with the [Developer Quick Start Guide](../dev/quickstart.md) for a
goal-oriented entry point ("I want to X — what do I run?"). For full details,
see [Testing Overview](../dev/testing/overview.md).

## P2P Messaging Layer

DarkWow maintains a separate P2P messaging layer for decentralized
communication applications:

- **[darkirc](../misc/darkirc/darkirc.md)**: P2P IRC daemon for
  decentralized chat (rooms, DMs, contacts)
- **[Event Graph](legacy/event_graph.md)**: P2P message DAG providing
  synchronization, ordering, and replay for chat events

### Storage quarantine: sled-overlay

The event graph uses `sled-overlay` for batched DAG writes. This crate
introduces non-determinism (overlay/diff/inverse-diff semantics) and is
**strictly quarantined** from the blockchain execution layer.

| Layer | Storage | Determinism |
|-------|---------|-------------|
| Blockchain execution (`dwowd`, `dwow_chain`) | Plain `sled` | Strictly deterministic |
| P2P messaging (`darkirc`; event-graph relay at `script/evgrd/`) | `sled-overlay` | Non-deterministic (acceptable for messaging) |

The quarantine is enforced via Rust's feature-gate system — `sled-overlay`
is only enabled by the `event-graph` Cargo feature. No blockchain feature
(`blockchain`, `linear`) or binary (`dwowd`) pulls it in. See the
[Event Graph](legacy/event_graph.md) documentation for the full
rationale.

Nine contracts are deployed at genesis — two consensus-critical (Deployooor, NativeToken)
and seven ecosystem infrastructure (PromissoryNote, Identity, Oracle, Attestation,
Purse, Box, MultiSig). See [Genesis Contracts](genesis.md) for the full list, and
[Contracts](../contracts.md) for the complete catalog of all 32 contracts.