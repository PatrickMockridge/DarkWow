# DarkFi-Jailbroken: O-Cap Authorization for Privacy-Preserving ZK Contracts

![Build Status](https://img.shields.io/github/actions/workflow/status/darkrenaissance/darkfi/ci.yml?branch=master&style=flat-square)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white&style=flat-square)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white&style=flat-square)](https://dark.fi/manifesto.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white&style=flat-square)](https://dark.fi/book/)

## Fork Name: "Jailbroken"

**This fork is called "darkfi-jailbroken" because we broke free from upstream's security flaws and governance lock-in.**

### Security Advantage Over Upstream

**Upstream DarkFi has critical ZK circuit heap bugs** caused by elliptic curve (EC) operations in their circuits:

| Upstream Circuit | EC Operations | Status |
|-----------------|---------------|--------|
| Fee_V2 | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| Mint_V2 | ec_mul_short, ec_mul, ec_add | **BUGGY** |
| Burn_V2 | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| AuthTokenMint_V2 | ec_mul_base | **BUGGY** |

**This fork uses Poseidon-only circuits.** EC heap bugs cannot occur in pure Poseidon arithmetic — there is no memory corruption vector when no EC operations exist.

See [Contract Standards](doc/src/dev/contracts/standards.md) for full analysis.

### Governance Problems

Upstream DarkFi also has problematic governance:

- **Pre-mine**: Early investors, team, and SAFT participants received DARK tokens at genesis
- **Venture Capital Influence**: Large token holders can dominate governance proposals
- **Whale Problem**: Token concentration allows wealthy entities to control DAO voting

**We removed Money V1, DAO V1, and all pre-mined/controlled token distributions.**
**We run on pure Proof of Work - the only legitimate sybil resistance.**

### What We Changed

| Aspect | Upstream DarkFi | darkfi-jailbroken |
|--------|-----------------|-------------------|
| Money Contract | Money V1 (legacy) | Money V2 (refactored to fix bugs) + Native Token (Z-cash burn-mint, no freezing) |
| Governance | DAO V1 (ACL/token-holder voting) | DAO Escrow (ZK predicate, voluntary) |
| Authorization | Merkle proofs leak balance | Pedersen commitments hide balance |
| Block Rewards | Mixed V1/V2 | Native Token + Money V2 |
| Genesis | Pre-mine, SAFT, team tokens | Pure PoW - earn through mining |
| Consensus | PoW + governance tokens | **Pure PoW only** |
| Privacy Math | ACL model leaks identity | ZK predicates reveal only boolean |

**The Fundamental Problem with Upstream**: Their ACL-based governance (token-holder voting via Merkle proofs) is mathematically unsound for privacy. When you vote, observers learn your public key AND your token balance. See [The Zero-Knowledge Authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) for the proof.

### Satoshi-Style Principles

True to Bitcoin's original vision:
1. **No Pre-Mined Tokens**: Every DARK is earned through PoW mining
2. **Voluntary Governance**: Opt-in DAO Escrow - no forced governance
3. **No SAFT/VC Distribution**: No early investor advantages
4. **Pure Sybil Resistance**: PoW is the only consensus mechanism

This fork is for those who believe in **mining true randomness** and **voluntary governance** - not astroturfed token-holder democracy.

**Development occurs on the `master` branch** (`PatrickM123/darkfi-jailbroken:master`).

---

## The Problem: Authorization Without Privacy

**Traditional blockchain asks**: "WHO has access?"
- Public keys link transactions to identities
- Signatures prove ownership but don't hide the transaction
- Transaction graphs can be analyzed to deanonymize users

**DarkFi asks**: "Can you prove you have access?"
- Identity never revealed, only capabilities proven
- O-Cap (Object Capability) = authorization without revelation

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Authorization                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Alice proves: "I am a verified smart contract auditor"          │
│  Verifier learns: ✓ Alice can audit                               │
│  Verifier DOES NOT learn: Alice's name, employer, salary          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## The Complete O-Cap Pipeline

DarkFi's contracts weave together into a complete privacy-preserving system:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  IDENTITY          →    TENDER         →    LABOR MARKET    →    INSURANCE    │
│  (capabilities)         (selection)          (execution)           (risk)     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Worker registers capabilities once:                                         │
│  - "qualified_contractor" from Identity contract                              │
│  - "senior_engineer" via DAG (multiple paths to qualify)                    │
│                                                                              │
│  Worker proves capability everywhere without revealing identity:             │
│  - Tender: Submit bid proving capability (0x08)                              │
│  - Labor Market: Accept job proving same capability (0x0d)                   │
│  - Insurance: Act as underwriter proving different capability (0x09-0x0c)     │
│                                                                              │
│  THE CAPABILITY FOLLOWS THE WORKER - IDENTITY NEVER REVEALED               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key insight**: Workers prove qualifications once via Identity, use those proofs everywhere. No re-verification, no identity disclosure.

---

## O-Cap Authorization (0x09-0x0d)

The Identity contract provides full O-Cap authorization:

| Opcode | Function | Purpose |
|--------|----------|---------|
| `0x09` | `RegisterCapabilityV1` | Define capability types (e.g., "can_audit_smart_contracts") |
| `0x0a` | `IssueCapabilityV1` | Issue capability to qualified holders |
| `0x0b` | `VerifyCapabilityV1` | Cross-contract verification (authorization happens here) |
| `0x0c` | `RevokeCapabilityV1` | Revoke capability (issuer can invalidate) |
| `0x0d` | `CreateClaimDAGV1` | Multi-path competency (OR logic between paths, AND within) |

### DAG Example: "Senior Engineer" Competency

```
PATH A:                          PATH B:
BSC Degree ──► 5yr Exp ──► Lead  │  Industry Cert ──► 10yr Exp
        │                             │
        └─────────────────────────────┼────► "Senior Engineer"
                                      │      (Either path qualifies)
```

Worker proves they satisfied a path without revealing which path or exact credentials.

---

## Smart Contracts by Purpose

### Identity & Authorization
- **Identity**: O-Cap primitives, credentials, DAG-based competency claims

### Finance
- **Native Token**: Consensus-first native token (block rewards, fees, transfers)
- **Stablecoin**: Synthetix-style pooled debt model
- **DEX**: Privacy-preserving decentralized exchange
- **Escrow**: Conditional value escrow
- **DAO-Escrow**: DAO-governed endowment with voting
- **Subscription**: Recurring payment streams
- **Bridge**: Cross-chain transfers

### Labor & Tendering
- **Labor Market**: Job posting and acceptance with O-Cap (0x0d AcceptJobWithCapabilityV1)
- **Tender**: Sealed-bid procurement with O-Cap (0x07-0x08 capability-based bids)
- **Attestation**: Generalized claims and evidence verification

### Risk & Insurance
- **Insurance Market**: Underwriting and coverage with O-Cap (0x09-0x0c capability-based)
- **Prediction Market**: Risk probability pricing

### Gaming & Entertainment
- **Baccarat**, **Roulette**, **Slot**: Privacy-preserving casino games
- **Lottery**: Configurable lottery combining BettingStake and Insurance
- **DarkBet Exchange**: Unified betting with order-book and AMM modes
- **Game Room**: Generalized betting for poker, backgammon, etc.

### Infrastructure
- **Oracle**: Push-model oracle with attestation
- **Block Height Prediction**: Entropy-based randomness

---

## Key Differentiators

### O-Cap Authorization
Authorization based on **what you can prove**, not **who you are**.
- Identity hidden at every step
- Only the specific capability is revealed
- Capabilities are bound to secrets only the holder knows
- Issuers can revoke capabilities

### Composable Privacy
Contracts build on each other:
- **Auction** uses **Escrow** for deposits (not built-in)
- **Tender** uses **Attestation** for competency claims
- **Labor Market** creates jobs from **Tender** winners
- **Insurance Market** integrates with **Money** for premiums/claims

### ZK Circuits
All contracts use zero-knowledge proofs for:
- Commitment schemes (hide values on-chain)
- Range proofs (e.g., "balance >= X" without revealing balance)
- Membership proofs (e.g., Merkle tree verification)
- Predicate verification (e.g., "age >= 18" without revealing age)

### Provable Randomness
Shared `darkfi_sdk::crypto::entropy` module for entropy across contracts.

---

## Architecture Documentation

- [O-Cap & Composable Privacy](doc/src/arch/ocap.md) — The central paradigm
- [Opcodes Reference](doc/src/arch/opcodes.md) — Opcode soundness with Lean 4 proofs
- [Composability](doc/src/arch/composability.md) — Contract composition patterns
- [Safemath](doc/src/arch/safemath.md) — Legacy ZK arithmetic templates
- [Entropy Module](doc/src/arch/entropy.md) — Provable randomness via block hash

---

## Build

```shell
% git clone https://codeberg.org/PatrickM123/darkfi-jailbroken
% cd darkfi-jailbroken
% rustup target add wasm32-unknown-unknown
% make
```

Minimum Rust version: **1.87.0**.

For development hacking, see the [DarkFi book](https://dark.fi/book/dev/dev.html).

---

## Reference Materials

**Uncensorable ZK and DarkFi Reference Material (Arweave)**

DarkFi Reference Material stored permanently on Arweave:
- [DarkFi Reference Material](https://app.ardrive.io/#/drives/f79597cd-8a4e-426e-840e-25c1453e418d?name=DarkFi+Reference+Material) - Textbooks, papers, and materials on ZK circuits, cryptography, and DarkFi

---

## Security Status

All new contracts are **EXPERIMENTAL** and **UNAUDITED**.

Known issues are documented in [Security Analysis](doc/src/arch/security-analysis.md).

---

## Connect

- [DarkFi Alpha Testnet](https://dark.fi/book/testnet/node.html) — PoW blockchain with anonymous transactions and ZK contracts
- [DarkFi IRC](https://dark.fi/book/misc/darkirc/darkirc.html#installation) — P2P IRC daemon

---

**Go Dark.**

Let's liberate people from the claws of big tech and create the democratic paradigm of technology. Self-defense is integral to any organism's survival and growth. Power to the minuteman.