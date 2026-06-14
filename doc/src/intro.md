# DarkWow

**These smart contracts have been reviewed and tested via the DarkWow test pipeline according to best reasonable efforts. Use at your own risk.**

**Development occurs on the `linear-master` branch** — the main development branch featuring Uncle Merkle consensus and a linear blockchain architecture.

## Where to Start

- **New to DarkWow?** Read the [Formal Specification](arch/formal-specification.md) — one page covering everything.
- **Want to build?** Start with [Developer Quick Start](dev/quickstart.md).
- **Deep dive?** The [Architecture](arch/README.md) index maps every subsystem.
- **Philosophy?** See [Philosophy](philosophy/philosophy.md) for the political-economic context.

## What is DarkWow?

DarkWow is a privacy-first blockchain built on a plain vanilla chassis with a
novel zero-knowledge execution engine.

The chassis is assembled from proven, multi-decade components:

- **Satoshi's supply model**: 21M DRKW hard cap, fair launch with zero premine
- **Monero's mining model**: RandomX CPU-friendly PoW, permanent tail emission for long-term security
- **Continuous exponential decay**: Same 4-year half-life as Bitcoin, smoothed so rewards don't drop 50% overnight
- **Uncle Merkle pin rewards**: Non-canonical blocks earn partial rewards — no wasted miner work

The only fundamentally new piece is the **zkVM**: a zero-knowledge virtual
machine that proves WASM contract execution. Every smart contract runs inside
ZK proofs — shielded transactions, anonymous voting, and encrypted state are
the defaults, not opt-in layers.

This is the paradigm shift. Everything else is the same chassis the ecosystem
has relied on for decades. Nothing overly complex, nothing exotic except the
star of the show.

> See [Mining Tokenomics](arch/mining-tokenomics.md) for the full reward schedule, emission curve, and design rationale.

## Smart Contracts

DarkWow implements privacy-preserving smart contracts across multiple domains:

| Category | Contracts |
|----------|-----------|
| **Finance** | stablecoin, bridge, dex, pool_stake |
| **Gaming** | lottery, baccarat, roulette, slot, darktoshi_dice, betting_stake |
| **Governance** | dao_escrow, subscription, labor_market, tender |
| **Identity** | attestation, oracle, identity |
| **Exchange** | auction, escrow, darkbet_exchange |

### Native Token Contract

The [`native_token`](dev/contracts/native_token.md) contract handles consensus and implements a **burn-mint privacy model** with **proof of token balance** (active Pedersen mass balance enforcement) and **no governance control**:

- **PoWRewardV1**: Block rewards with active cumulative supply enforcement
- **FeeV1**: Network fee payment
- **BurnV1**: Destroy coins (with nullifier to prevent double-spend)
- **TransferV1**: Private token transfers

**Critical difference from upstream:** This contract has NO governance. Upstream's DAO can freeze their native token through token-holder voting — this fork cannot.

> **Design Philosophy: Tokens are Infrastructure**
>
> DarkWow's tokens (NativeToken, PromissoryNote) follow a minimal design: they move value, nothing more. Business logic lives in smart contracts (DEX, stablecoin, etc.). This is intentional:
>
> - **Simplicity** = fewer bugs in frequently-called code
> - **Isolation** = bugs in DEX don't cascade to all token transfers
> - **Permissionless** = anyone can deploy custom token contracts
>
> This mirrors process safety principles: isolate complexity to where it's required, don't over-instrument the pipework.

## Architecture

Key architectural documents:

- [Architecture Overview](arch/overview.md) — System design and components
- [NativeToken Contract](dev/contracts/native_token.md) — Consensus-first native token
- [Consensus](arch/consensus/consensus.md) — PoW mining and block reward distribution
- [Transactions](arch/sc/tx-lifetime.md) — Transaction lifecycle and ZK verification
- [ZK Circuits](zkas/index.md) — Zero-knowledge proof system

## Security Status

All contracts are **EXPERIMENTAL** and **UNAUDITED**.

## Relationship to Upstream

This project is a fork of [DarkFi](https://codeberg.org/PatrickM123/darkwow). It inherits the core zkVM, ZKAS circuit language, P2P networking stack, and WASM contract runtime.

For a complete comparison of design differences — native token governance, privacy model, token distribution, consensus, and opcodes — see [What's Different from Upstream](about/differences_from_upstream.md).

### Privacy Architecture

DarkWow uses **ZK predicates** for authorization:

```
ZK Predicate (this fork):
  - Prove: "I am a verified contractor"
  - Verifier learns: ✓ Boolean (yes/no)
  - Verifier DOES NOT learn: Public key, balance, identity

ACL (upstream approach):
  - Prove: "I have 1000 tokens"
  - Verifier learns: Public key AND token balance
  - Simpler to implement and audit, but reveals identity
```

### Known Security Issues

Known security issues are documented in [Security Analysis](arch/security-analysis.md).

For ZK circuit security, see [ZK Circuit Troubleshooting](dev/zk-circuit-troubleshooting.md).

## Getting Started

- **Users**: Start with [Start Here](start-here.md) for project overview and community joining.
- **Developers**: Start with the [Developer Quick Start Guide](dev/quickstart.md) for building, testing, and customizing contracts.
- **Node Operators**: See [Running a Node](testnet/node.md).

### Building from Source

```bash
git clone https://codeberg.org/PatrickM123/darkwow
cd darkwow
make
```

See [Localnet Development](localnet-dev.md) for manual localnet setup. For
Docker-based local development (the recommended approach), see Level 3 of the
[Developer Quick Start Guide](dev/quickstart.md).

## Documentation Structure

```
doc/src/
├── intro.md              # This file
├── start-here.md         # Detailed project overview
├── dev/                  # Developer documentation
│   ├── contracts/       # Contract implementation guides
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

`LessThanOrEqual` (0x55), `IsNotEqual` (0x62), and `BaseDiv` (0x58) are **DarkWow additions** to the zkVM — they do not exist in upstream DarkWow. All three were formally verified in Lean4 on this fork (`proofs/lean/`). `LessThanOrEqual` enables conditional logic and O-Cap predicate evaluation in circuits; `IsNotEqual` is the first fully constrained pure Boolean operator in the zkVM; `BaseDiv` enables precise field division for cold-circuit governance operations.

| Opcode | Status |
|--------|--------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound — DarkWow addition |
| `IsNotEqual` (0x62) | ✅ Pure — First fully constrained Boolean operator |
| `BaseDiv` (0x58) | ✅ Implemented — DarkWow addition |
| `IsEqualBase` (0x54) | ⚠️ Use `IsNotEqual` or `ConstrainEqualBase` instead |

See [Opcodes and Formal Verification](arch/zk/opcodes.md) for full analysis.

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
