# DarkFi Development Fork

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

This is a development fork of the official DarkFi repository. **Development occurs on the `master` branch** (`PatrickM123/darkfi:master`).

## What is DarkFi?

DarkFi is a privacy-first blockchain with a focus on ZK-powered smart contracts. Key differentiators:

- **ZK-Native Contracts**: All contracts use zero-knowledge proofs for privacy
- **Consensus-First Token**: Native token (`native_token`) handles block rewards and fees as top priority
- **Privacy by Default**: Shielded transactions, anonymous voting, encrypted state
- **Multi-Chain Support**: Atomic swaps, bridges, and cross-chain interoperability

## Smart Contracts

DarkFi implements privacy-preserving smart contracts across multiple domains:

| Category | Contracts |
|----------|-----------|
| **Finance** | stablecoin, bridge, dex, atomic_swap, pool_stake |
| **Gaming** | lottery, baccarat, roulette, slot, darktoshi_dice, betting_stake |
| **Governance** | dao_escrow, subscription, labor_market, tender |
| **Identity** | attestation, oracle, identity |
| **Exchange** | auction, escrow, darkbet_exchange |

### Native Token Contract

The [`native_token`](dev/contracts/native_token.md) contract is the consensus-first native token:

- **PoWRewardV1**: Block rewards for miners
- **FeeV1**: Network fee payment
- **TransferV1**: Private token transfers
- **GenesisMintV1**: Initial supply creation

## Architecture

Key architectural documents:

- [Architecture Overview](arch/overview.md) — System design and components
- [Native Contracts](dev/native_contracts.md) — Built-in contracts (native_token, deployooor)
- [Consensus](arch/consensus.md) — PoW mining and block reward distribution
- [Transactions](arch/tx_lifetime.md) — Transaction lifecycle and ZK verification
- [ZK Circuits](zkas/index.md) — Zero-knowledge proof system

## Security

All contracts are **EXPERIMENTAL** and **UNAUDITED**.

Known security issues are documented in [Security Analysis](arch/security-analysis.md).

For ZK circuit security, see [ZK Circuit Troubleshooting](dev/zk-circuit-troubleshooting.md).

## Getting Started

### Running a Node

See [Running a Node](testnet/node.md) for setup instructions.

### Building from Source

```bash
# Clone this fork
git clone https://codeberg.org/PatrickM123/darkfi
cd darkfi

# Build the project
cargo build --release

# Build documentation
cd doc
mdbook build
```

### Local Development

```bash
# Start localnet
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml

# Mine tokens for testing
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet initialize
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet keygen
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine
```

See [Localnet Development](localnet-dev.md) for detailed setup.

## Documentation Structure

```
doc/src/
├── intro.md              # This file
├── start-here.md         # Detailed project overview
├── dev/                  # Developer documentation
│   ├── dev.md           # Development guide
│   ├── contracts/       # Contract specifications
│   │   └── native_token.md
│   └── zk-circuit-troubleshooting.md
├── arch/                 # Architecture documentation
│   ├── overview.md
│   ├── consensus.md
│   └── opcodes-status.md
├── zkas/                # ZK proof documentation
└── testnet/             # User guides
```

## Technical Debt

### Opcode Status

Opcodes for ZK circuits are being formally verified. Current status:

| Opcode | Status |
|--------|--------|
| `LessThanOrEqual` | ✅ Verified Sound |
| `BaseDiv` | ✅ Implemented |
| `IsEqualBase` | ⚠️ Use `ConstrainEqualBase` instead |

See [Opcodes and Formal Verification](arch/opcodes.md) for full analysis.

### Contract Status

| Contract | Status |
|----------|--------|
| native_token | ✅ Production-ready |
| dao_escrow | ✅ Complete |
| All other WASM contracts | ⚠️ Experimental |

## Join the Community

The core community organizes through our anonymous p2p chat system:

- Every Monday at 14:00 UTC (DST) or 15:00 UTC (ST) in #dev
- See [DarkIRC](misc/darkirc/darkirc.md) for joining instructions
