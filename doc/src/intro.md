# DarkWow

**WARNING: This branch contains experimental, unaudited smart contracts. Do NOT deploy or use these contracts with real funds. They are for research and educational purposes only.**

**Development occurs on the `linear-master` branch** — the main development branch featuring Uncle Merkle consensus and a linear blockchain architecture. The old upstream overlay-DAG code is preserved on the `master-upstream` branch.

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

> **Design Philosophy: Tokens are Infrastructure**
>
> DarkWow's tokens (NativeToken, MoneyV3) follow a minimal design: they move value, nothing more. Business logic lives in smart contracts (DEX, stablecoin, etc.). This is intentional:
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

This fork addresses **critical governance and identity leakage vulnerabilities** in upstream DarkWow.

### Why This Fork Exists: Upstream DarkWow's Critical Flaws

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

Upstream DarkWow uses **ACL-based governance** where voters must reveal:

| What is revealed | Impact |
|-----------------|--------|
| Public key | Wallet address traceable |
| Token balance | Rich/poor status exposed |
| Vote choices | Political views deanonymized |

This leaks identity from poor to rich. If you're poor, your vote matters less. If you're rich, you're a target.

#### 3. SAFT Pre-mine Creates Whale Dominance

Upstream distributed DRKW tokens at genesis to:
- Early investors
- Team members
- SAFT participants

This concentrates governance in the hands of the wealthiest token holders.

#### 4. Overlay/Diff Consensus Exists to Serve the Anti-Fork DAO

Upstream's consensus uses a complex overlay/diff architecture with sled-overlay for transactional state management. This isn't gratuitous complexity — it exists because the DAO governance model requires a mechanism to adjudicate between competing forks:

- **Speculative verification**: Blocks are verified against an in-memory overlay that can be committed or rolled back
- **Diff logging**: Every state change is tracked for potential rollback, creating non-deterministic behavior where the same code produces different results depending on timing
- **Implicit fork competition**: The overlay system decides which fork wins, rather than letting the chain with the most accumulated work naturally dominate

This makes deterministic testing effectively impossible. Race conditions, timing-dependent state, and speculative commits create flaky tests that erode confidence in the entire contract system. All of this complexity exists for one reason: the DAO must keep everything under one tent to preserve token-holder voting power across a unified chain.

This fork replaces the entire overlay/diff stack with **Uncle Merkle consensus**: the canonical chain with the most accumulated work **obligates** offering uncle chains a one-time option (within a short time window, minutes) to form a side chain and share the PoW reward. The uncle chain can accept or reject. This achieves the Pareto efficient benefit of upstream's fork-handling — miners aren't punished for producing non-canonical blocks — without the complex rewind and sled overlay logic.

With pure PoW and no governance DAO, hard forks are handled the Bitcoin way: if nodes want to fork, they can (like BCash). Both chains coexist. No complex mechanism needed to keep everything under one tent.

### This Fork's Solutions

| Problem | Upstream | This Fork |
|---------|----------|-----------|
| Native token freeze | DAO can control it | **No governance, no freeze** |
| Identity leakage | ACL voting reveals balance | **ZK predicates reveal boolean only** |
| Token distribution | SAFT/pre-mine at genesis | **Pure PoW mining only** |
| Governance | Token-holder ACL voting | **DAO Escrow (ZK predicate, voluntary)** |
| Consensus complexity | Overlay/diff for anti-fork DAO | **Uncle Merkle — simple, deterministic, Pareto efficient** |

For the full analysis, see [Contract Standards](dev/contracts/standards.md).

### Privacy Architecture

DarkWow uses **ZK predicates** instead of ACL:

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
git clone https://codeberg.org/PatrickM123/darkfi-jailbroken
cd darkfi-jailbroken

# Build the project
cargo build --release

# Build documentation
cd doc
mdbook build
```

### Local Development

```bash
# Start localnet
./target/release/dwowd -c contrib/localnet/dwowd-single-node/dwowd.toml

# Mine tokens for testing
./target/release/dww -c bin/drk/drk_config.toml -n localnet wallet initialize
./target/release/dww -c bin/drk/drk_config.toml -n localnet wallet keygen
./target/release/dww -c bin/drk/drk_config.toml -n localnet mine
```

See [Localnet Development](localnet-dev.md) for detailed setup.

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

`LessThanOrEqual` (0x55) and `BaseDiv` (0x58) are **DarkWow additions** to the zkVM — they do not exist in upstream DarkWow. Both were formally verified in Lean4 on this fork (`proofs/lean/`). `LessThanOrEqual` enables conditional logic and O-Cap predicate evaluation in circuits; `BaseDiv` enables precise field division for cold-circuit governance operations.

| Opcode | Status |
|--------|--------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound — DarkWow addition |
| `BaseDiv` (0x58) | ✅ Implemented — DarkWow addition |
| `IsEqualBase` (0x54) | ⚠️ Use `ConstrainEqualBase` instead |

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
