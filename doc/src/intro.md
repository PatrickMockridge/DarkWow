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

The [`native_token`](dev/contracts/native_token.md) contract handles consensus and implements a **Z-cash style burn-mint privacy model** with **no token freezing**:

- **PoWRewardV1**: Block rewards for miners
- **FeeV1**: Network fee payment
- **MintV1**: Create new coins with Pedersen commitments
- **BurnV1**: Destroy coins (with nullifier to prevent double-spend)
- **TransferV1**: Private token transfers

## Architecture

Key architectural documents:

- [Architecture Overview](arch/overview.md) — System design and components
- [Native Contracts](dev/native_contracts.md) — Built-in contracts (native_token, deployooor)
- [Consensus](arch/consensus.md) — PoW mining and block reward distribution
- [Transactions](arch/tx_lifetime.md) — Transaction lifecycle and ZK verification
- [ZK Circuits](zkas/index.md) — Zero-knowledge proof system

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**.

This fork addresses **critical security vulnerabilities** in upstream DarkFi's ZK circuit design.

### ZK Circuit Heap Bugs: Why Poseidon-Only

Upstream DarkFi's Money V1, V2, and related contracts use **elliptic curve (EC) operations** in ZK circuits. These operations have caused heap memory corruption bugs:

| Circuit | EC Operations | Status |
|---------|-------------|--------|
| Fee_V2 | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| Mint_V2 | ec_mul_short, ec_mul, ec_add | **BUGGY** |
| Burn_V2 | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| AuthTokenMint_V2 | ec_mul_base | **BUGGY** |

**This fork uses Poseidon-only circuits.** EC heap bugs cannot occur in pure Poseidon arithmetic — there is no memory corruption vector when no EC operations exist.

See [Contract Standards](dev/contracts/standards.md) for full analysis of EC vs Poseidon tradeoffs.

### Why Poseidon is Sufficient for DarkFi

DarkFi uses **burn-mint** (not transfer-with-change):

```
TRADITIONAL (Pedersen/homomorphic):
  Spend coin A → Receive coin B
  Need: C_change = C_input - C_output  ← Requires EC addition

DARKFI (burn-mint):
  Burn coin A (emit nullifier)
  Mint coin B (new commitment)
  Value balance checked at contract layer
  No EC addition needed
```

For the full security rationale, see [standards.md](dev/contracts/standards.md).

### Cold vs Hot Circuits

| Circuit Type | Example | Design Choice |
|-------------|---------|---------------|
| **Hot** (frequent) | open_position, mint_stable | Poseidon-only (no bugs) |
| **Cold** (rare) | governance_report, accrue_interest | BaseDiv for precision |

Cold circuits execute monthly and can use more complex arithmetic. Hot circuits execute thousands of times per day and must be bug-free.

### Known Security Issues

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
