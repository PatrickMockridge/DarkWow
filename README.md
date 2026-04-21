# DarkFi-Jailbroken: O-Cap Authorization for Privacy-Preserving ZK Contracts

![Build Status](https://img.shields.io/github/actions/workflow/status/darkrenaissance/darkfi/ci.yml?branch=master&style=flat-square)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white&style=flat-square)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white&style=flat-square)](https://dark.fi/manifesto.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white&style=flat-square)](https://dark.fi/book/)

## Fork Name: "Jailbroken"

**This fork is called "darkfi-jailbroken" because we broke free from upstream's governance attacks and identity leakage.**

### Critical Issues in Upstream DarkFi

#### 1. Governance Can Freeze Native Token

Upstream DarkFi's DAO can freeze the native token through token-holder voting:

```
Attack Vector:
1. Large token holders form coalition via DAO
2. Vote to restrict native token minting/burning
3. Miners can't get paid → PoW consensus fails
4. Network becomes extortable
```

**Why this is catastrophic for PoW:**
- Native token pays block rewards to miners
- Native token pays fees to validators
- If governance can restrict it, consensus itself becomes attackable

See [Contract Standards](doc/src/dev/contracts/standards.md) Part 3 for full analysis.

#### 2. ACL Identity Leakage (Poor to Rich Deanonymization)

Upstream uses ACL-based governance where voters must reveal:

| What is revealed | Impact |
|-----------------|--------|
| Public key | Wallet address traceable |
| Token balance | Rich/poor status exposed |
| Vote choices | Political views deanonymized |

**"The Fundamental Problem with Upstream ACL":**
When you vote via token-holder DAO, observers learn your public key AND your token balance. This leaks identity from poor to rich - if you're poor, your vote matters less; if you're rich, you're a target.

#### 3. SAFT Pre-mine at Genesis

Upstream DarkFi distributed tokens at genesis to:
- Early investors
- Team members
- SAFT participants

This creates **whale dominance** in governance - the richest token holders control the DAO.

### This Fork: No Governance, No Identity Leakage

**We removed:**
- All pre-mined/SAFT token distributions
- ACL-based token-holder voting (leaks identity)
- Governance control over native token (freezing vectors)

**We keep:**
- Pure Proof of Work (only legitimate sybil resistance)
- ZK predicates for authorization (reveals only boolean, not balance)
- Voluntary DAO Escrow (opt-in, no identity leakage)

### What We Changed

| Aspect | Upstream DarkFi | darkfi-jailbroken |
|--------|-----------------|-------------------|
| Token distribution | SAFT/pre-mine at genesis | Pure PoW mining only |
| Governance | ACL DAO (leaks identity) | DAO Escrow (ZK predicate, voluntary) |
| Native token | Controllable via DAO | No governance, no freeze |
| Authorization | Merkle proofs leak balance | ZK predicates reveal boolean only |
| Consensus | PoW + governance tokens | **Pure PoW only** |

**The Fundamental Problem with Upstream**: Their ACL-based governance (token-holder voting) is mathematically unsound for privacy. When you vote, observers learn your public key AND your token balance. See [The Zero-Knowledge Authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) for the proof.

### Satoshi-Style Principles

True to Bitcoin's original vision:
1. **No Pre-Mined Tokens**: Every DARK is earned through PoW mining
2. **No ACL Governance**: No token-holder voting that leaks identity
3. **No Native Token Control**: Governance cannot freeze consensus tokens
4. **Pure Sybil Resistance**: PoW is the only consensus mechanism

This fork is for those who believe in **mining true randomness** and **ZK-based authorization** - not astroturfed token-holder democracy that freezes when the rich disagree.

**Development occurs on the `master` branch** (`PatrickM123/darkfi-jailbroken:master`).

### Uncle Merkle Consensus (linear-testnet)

This fork implements **linear-testnet**, an alternative consensus mode using Uncle Merkle consensus.

**The problem with upstream's overlay/diff system:**
- State can be speculative (checkpoint), committed, or rolled back
- `diff()` computation depends on sequence history - same code produces different results
- Checkpoint/revert doesn't update the diff log, breaking fork rebuild assumptions
- Non-deterministic bug reproduction

**The Uncle Merkle solution:**
- Uncle chains explicitly referenced in canonical blocks (no speculative fork competition)
- Stateless verification - pure merkle proof + math, no overlay needed
- Deterministic execution - same block always produces the same result
- No overlay/diff complexity - simplifies testing significantly

See [Uncle Merkle Consensus](doc/src/arch/uncle_merkle.md) for the detailed specification.

**Linear-Testnet Network Mode:**
To run darkfid in linear-testnet mode:

```bash
./target/debug/darkfid --network linear-testnet
```

The `miner.mine_linear` RPC endpoint mines PoW blocks on the linear chain:
```json
{"jsonrpc": "2.0", "method": "miner.mine_linear", "params": ["recipient_base58", reward_value], "id": 1}
```

**Branches:**
- `master` - Current consensus with overlay/diff system (linear-testnet available as network mode)
- `linear-master` - Uncle Merkle consensus (experimental branch)

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