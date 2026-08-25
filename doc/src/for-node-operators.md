# Node Operator Guide

You want to run a DarkWow node or mine. Here's your path.

## Three node types

- **Mining node** (`dwowd`): Produces blocks via RandomX proof-of-work.
  Requires xmrig for mining. Full chain state, validates all blocks.
- **Observer / relay node** (`dwowd --observer`): Relays blocks and
  transactions without mining. Useful for network health and API access.
- **Wallet node** (`dww`): Full node that scans the chain for your
  capabilities. Derives identity from AccountManager, decrypts notes
  locally. Same P2P stack as dwowd — the wallet IS a full node.

Hardware: 8GB+ RAM recommended. The chain state is deterministic and
verifiable — same chain data always produces the same wallet state.

## Consensus: Uncle Merkle

DarkWow uses deterministic Uncle Merkle consensus. Unlike overlay-DAG
systems where state is speculative and provisional, Uncle Merkle
provides:

- **Stateless verification**: Validators verify blocks without maintaining
  a speculative overlay. No diffs, no rollbacks.
- **Deterministic fork resolution**: Same blocks always produce the same
  canonical chain. No ambiguity, no reorganization surprises.
- **Uncle rewards**: Competing blocks earn partial rewards via uncle
  inclusion — miners are incentivized to publish, not hide, competing blocks.

See [Uncle Merkle Consensus](arch/consensus/uncle_merkle.md) for the
full specification.

## Monetary policy

- **Hard cap**: 21,000,000 DRKW
- **Emission**: Continuous exponential decay, Satoshi-style
- **Premine**: Zero. Every commitment in circulation was mined
- **Supply audit**: Per-block Pedersen mass balance — Σ outputs + Σ burns +
  Σ fees == Σ inputs — verified at every block acceptance path

DarkWow's supply audit capability is a direct response to the Zcash
Orchard exploit (May 2026), where a silent inflation bug printed commitments
for years without detection. The cumulative supply commitment chain is
verifiable without ZK proofs.

See [Consensus & Coinbase](arch/consensus-coinbase.md) for the emission
schedule and reward curve.

## Merge mining

DarkWow supports merge-mining with Monero via p2pool:
- RandomX hashpower contributes to both Monero and DarkWow simultaneously
- Monero p2pool provides external hashpower anchoring
- Caribina (Arweave) provides external time-stamping

See [Merge Mining](arch/merge-mining.md) and [Monero Merge Mining](arch/monero-merge-mining.md).

## Getting started

1. **Build**: `make` (requires Rust toolchain)
2. **Configure**: See [Node Configuration](testnet/node.md)
3. **Set up mining**: See [Testnet Mining](testnet/testnet-mining.md)
4. **Join a network**: Devnet, testnet, or mainnet — see [Bootstrapping](testnet/bootstrapping.md)

## Reference

- [dwowd daemon](dwowd.md) — Full daemon documentation
- [Native Workflow](dev/native-workflow.md) — End-to-end mining setup
- [Network Troubleshooting](misc/network-troubleshooting.md)
- [Slashing & Economic Security](arch/slashing.md)
- [Merge Mining Architecture](arch/monero-merge-mining.md)
