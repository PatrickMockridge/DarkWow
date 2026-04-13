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

The [`native_token`](dev/contracts/native_token.md) contract handles consensus and implements a **Z-cash style burn-mint privacy model** with **no governance control**:

- **PoWRewardV1**: Block rewards for miners
- **FeeV1**: Network fee payment
- **MintV1**: Create new coins with Poseidon commitments
- **BurnV1**: Destroy coins (with nullifier to prevent double-spend)
- **TransferV1**: Private token transfers

**Critical difference from upstream:** This contract has NO governance. Upstream's DAO can freeze their native token through token-holder voting — this fork cannot.

## Architecture

Key architectural documents:

- [Architecture Overview](arch/overview.md) — System design and components
- [Native Contracts](dev/native_contracts.md) — Built-in contracts (native_token, deployooor)
- [Consensus](arch/consensus.md) — PoW mining and block reward distribution
- [Transactions](arch/tx_lifetime.md) — Transaction lifecycle and ZK verification
- [ZK Circuits](zkas/index.md) — Zero-knowledge proof system

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**.

This fork addresses **critical governance and identity leakage vulnerabilities** in upstream DarkFi.

### Why This Fork Exists: Upstream DarkFi's Critical Flaws

#### 1. Governance Can Freeze Native Token (Catastrophic for PoW)

Upstream's DAO can control the native token, creating a **plutocratic freeze attack**:

```
Attack Scenario:
1. Large token holders dominate DAO via SAFT/pre-mine holdings
2. Vote to restrict native token operations
3. Miners can't receive block rewards → PoW consensus fails
4. Validators can't collect fees → Consensus weakens
5. Network becomes extortable
```

**Why this breaks PoW:**
- Native token serves consensus-critical functions (block rewards, fees)
- If governance can freeze minting, miners may not get paid
- The network becomes attackable by wealthy token holders

#### 2. ACL Identity Leakage: Poor to Rich Deanonymization

Upstream DarkFi uses **ACL-based governance** where voters must reveal:

| What is revealed | Impact |
|-----------------|--------|
| Public key | Wallet address traceable |
| Token balance | Rich/poor status exposed |
| Vote choices | Political views deanonymized |

This leaks identity from poor to rich. If you're poor, your vote matters less. If you're rich, you're a target.

#### 3. SAFT Pre-mine Creates Whale Dominance

Upstream distributed DARK tokens at genesis to:
- Early investors
- Team members
- SAFT participants

This concentrates governance in the hands of the wealthiest token holders.

### This Fork's Solutions

| Problem | Upstream | This Fork (darkfi-jailbroken) |
|---------|----------|-------------------------------|
| Native token freeze | DAO can control it | **No governance, no freeze** |
| Identity leakage | ACL voting reveals balance | **ZK predicates reveal boolean only** |
| Token distribution | SAFT/pre-mine at genesis | **Pure PoW mining only** |
| Governance | Token-holder ACL voting | **DAO Escrow (ZK predicate, voluntary)** |

For the full analysis, see [Contract Standards](dev/contracts/standards.md).

### Privacy Architecture

DarkFi uses **ZK predicates** instead of ACL:

```
ZK Predicate (GOOD):
  - Prove: "I am a verified contractor"
  - Verifier learns: ✓ Boolean (yes/no)
  - Verifier DOES NOT learn: Public key, balance, identity

ACL (BAD - upstream uses this):
  - Prove: "I have 1000 tokens"
  - Verifier learns: Public key AND token balance
  - Identity leaked from rich to poor
```

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
