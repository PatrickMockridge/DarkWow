# AI Documentation Index

This document is the structured entry point for AI agents dropped into the
DarkWow repository. It maps every key document with file paths, purpose
annotations, and dependency ordering. Read this first, then follow the
references.

**For humans:** this is also a curated reading list. Start at §1 and work down.

**For the mdbook table of contents:** see [SUMMARY.md](../SUMMARY.md).

---

## 1. What This Project Is

Start here. These three documents define the project's identity, design intent,
and architectural commitments.

- **[DarkWow Intro](../intro.md)** — project overview, what it does, who it's for.
- **[Formal Specification](formal-specification.md)** — the one-page reference.
  *Six commitments, genesis contracts, wallet architecture, trust model, 32-contract ecosystem.*
- **[Philosophy](../philosophy/philosophy.md)** — design commitments and
  architectural principles. *Why no DAO governance, why no premine, why formal verification.*

**After these three, you should know:** what DarkWow is, what it commits to, and why.

---

## 2. Repository Structure

Key crates and directories. Paths are relative to the repo root (`/home/patrick/Darkfi/darkfi/`).

| Path | What it IS |
|------|------------|
| `src/sdk/` | Shared SDK: crypto, WASM host interface, tx, serialization, capability types |
| `src/linear/` | Consensus engine: chain state, execution, block production, RandomX VM |
| `src/runtime/` | WASM runtime: host function imports, VM runtime, gas accounting |
| `src/contract/<name>/` | Contract crates. Each self-contained: ZK circuits, entrypoint, model, tests |
| `src/contract/test-harness/` | Shared test harness for heavyweight contract tests |
| `bin/dwowd/` | Mining node binary: chain, mempool, P2P, RPC, miner |
| `bin/dww/` | Wallet binary: CLI wallet, sync daemon client |
| `vendor/halo2/` | Vendored halo2 (rev 98d449b — mandatory ZCash-Orchard exploit fix, never regress) |
| `doc/src/` | All documentation (mdbook source) |
| `proofs/lean/` | Lean4 formal verification: Combinatorial proofs, ceiling derivation |
| `contrib/model/` | Python consensus models (1:1 executable specifications) |
| `contrib/docker/` | Docker pipeline: testnet deployment, multi-node mining |

---

## 3. Formal Foundations

The theoretical basis. Read in this order.

### 3.1 Type System and ρ-Calculus

- **[Type System](type-system.md)** — the specification. *ρ-calculus foundation,
  Authorization Inversion Theorem, type unification, o-cap model.*
- **[O-Cap: Emergent Types from the ρ-Calculus](ocap.md)** — how capabilities
  compose from primitive names. *Quote/eval, barb semantics, capability
  descriptors.*
- **[Capability Composition](composition.md)** — additive vs multiplicative
  composition. *O-cap composition preserves privacy budgets; shared mutable
  state explodes them.*

### 3.2 Contract WASM Type System

- **[Contract WASM Type System](contract-wasm-type-system.md)** — the
  specification. Read Parts A, B, and C.
  - **Part A** (§A.0–§A.8): Shared foundation. Entrypoints, barbs, host
    interface, error propagation, witness binding. Applies to ALL contracts.
  - **Part B** (§B.0–§B.11): L2 type system. 1-trajectory semantics, barb union,
    direct KV lookup. For static records (Identity, Attestation, Oracle, DAO).
  - **Part C** (§C.0–§C.8): L1 type system. N^K state space, trajectory
    identification, barb ordering DAG, additive composition, two-level Merkle
    anchoring, nominal domain types, combinatorial error theory.

### 3.3 Formal Verification

- **[Combinatorial GeneralTheorem](../../../proofs/lean/src/DarkFi/Combinatorial/GeneralTheorem.lean)**
  — mechanized proof of `T(C,N,K) = N^K` for L1, `T(C,K) = 1` for L2.
- **[CeilingDerivation](../../../proofs/lean/src/DarkFi/Combinatorial/CeilingDerivation.lean)**
  — structural ceiling constants: P_CEILING=9, W_CEILING=13, O_CEILING=3.

---

## 4. Privacy Architecture

- **[Privacy Model](privacy.md)** — the specification. *L1/L2 distinction,
  Merkle inclusion proofs, consume+create model, L1/L2 fungibility gradient
  (§2.4), architectural principles (§5), Universal Theorem (§6).*
- **[Anonymous Assets](anonymous_assets.md)** — how asset types are hidden
  behind ZK proofs.
- **[Contract Trust Model](contract-trust-model.md)** — three-layer trust:
  social (who deployed), mechanical (manifest match), deferred (attestation).

---

## 5. Consensus

- **[Consensus](consensus/consensus.md)** — the specification. *Block
  production, validation, fork resolution.*
- **[Uncle Merkle](consensus/uncle_merkle.md)** — deterministic fork resolution.
  *Competing blocks at same height become uncles sharing reward.*
- **[Chain Architecture](consensus/chain_architecture.md)** — implementation
  detail: block structure, state management, commit pipeline.
- **[Linear zkVM](consensus/linear_zkvm.md)** — ZK proof verification in
  consensus. *How proofs are batch-verified during block acceptance.*
- **[Caribina Finality](caribina.md)** — Arweave-anchored finality widget.
- **[Scaling & Sharding](consensus/scaling.md)** — future directions.

### 5.1 Mining

- **[Consensus & Coinbase](consensus-coinbase.md)** — block rewards, fee
  collection, supply audit.
- **[Merge Mining](merge-mining.md)** — Monero RandomX PoW merge mining.
  *Unifies architecture, economics, setup, and finality.*
- **[Stratum Protocol](consensus/stratum.md)** — mining pool protocol.

---

## 6. Contracts

### 6.1 Contract Architecture

- **[Contract Manifest](manifest.md)** — on-chain TOML manifest declaring
  functions, capability types, actions, state trees, ZK circuits.
- **[Contract Deployment Pipeline](dwowd_contract_pipeline.md)** — how
  contracts go from WASM to deployed.
- **[Contract Invocation API](contract_invoke_api.md)** — how transactions call
  contracts.
- **[Contract Metadata](contract-metadata.md)** — self-declared on-chain
  metadata (name, symbol, category).
- **[Circuit Versioning](circuit-versioning.md)** — ZK circuit versioning
  conventions, V1→V2 migration rationale (HAZOP RC3 domain separation),
  naming conventions, and manifest-driven versioning going forward.

### 6.2 Genesis Contracts (L1 — transferable o-caps)

- **[Genesis Contracts](genesis.md)** — canonical list with ContractId
  derivation and bootstrap sequence.
- **[Promissory Note](../contract/promissory_note.md)** — fully fungible
  private coins. *The reference L1 contract.*
- **[Box](../contract/box.md)** — ZK-native o-cap delegation primitive.
  *Put creates capability, Take consumes via nullifier.*
- **[Purse](../contract/purse.md)** — ZK-native value store. *Deposit/Withdraw
  with Pedersen-hidden balances.*
- **[NativeToken](../contract/native_token.md)** — consensus-critical: block
  rewards, fee payment, supply audit. *Only contract with bespoke wallet path.*
- **[Deployooor](../contract/deployooor.md)** — genesis contract deployer.

### 6.3 Genesis Contracts (L2 — static records)

- **[Identity](../contract/identity.md)** — ZK credential system. *Non-fungible,
  zero identity leak. Selective disclosure.*
- **[Oracle](../contract/oracle.md)** — external data feeds.
- **[Attestation](../contract/attestation.md)** — trusted binary attestation.
- **[MultiSig](../contract/multisig.md)** — threshold signature groups.

### 6.4 Contract Development

- **[Contract Developer Journey](../dev/contracts/journey.md)** — step-by-step
  from concept to deployed contract.
- **[Contract Safety Checklist](../dev/contracts/checklist.md)** — pre-deploy
  verification items.
- **[Contract Safety (Formal Verification)](../dev/contracts/safety.md)** —
  23 lessons from contract audits. *Lesson 16: constrain_instance must have
  visible derivation. Lesson 23: L1 complexity ceilings.*
- **[Contract Standards](../contract-standards.md)** — minimum standards and
  best practices.
- **[Contract Catalog](../contracts.md)** — all 32 contracts with manifests.

---

## 7. Wallet Architecture

- **[Wallet Architecture](wallet.md)** — the specification. *Authorization
  Inversion Theorem, capability construction engine, transport layer split.*
- **[Wallet vs Daemon](wallet-vs-daemon.md)** — architectural identity: wallet
  and mining node are structurally identical full nodes.
- **[Key & Account Management](key-management.md)** — AccountManager
  specification. *Pallas curve keys, import/export, AEAD.*

---

## 8. Testing

- **[Testing Overview](../dev/testing/overview.md)** — four-level taxonomy.
  *Level 1: lightweight (no ZK). Level 2: heavyweight (ZK proofs). Level 3:
  localnet (Docker multi-node). Level 4: devnet/public.*
- **[Python Models](../dev/testing/python-simulations.md)** — 1:1 executable
  specifications. *Python leads, Rust follows. Model first, code second.*
- **[Test Harness Guide](test_harness_guide.md)** — shared harness for
  contract heavyweight tests.
- **[Genesis Harness](genesis_harness.md)** — genesis block test infrastructure.
- **[Contract Testing Pipeline](pipeline.md)** — Docker-contained testnet
  pipeline. *Single entry point: test_pipeline.sh.*
- **[Build Resource Tuning](../dev/testing/build-resource-tuning.md)** —
  RAYON_NUM_THREADS, RUST_MIN_STACK, build flags.

---

## 9. AI-Assisted Development

- **[AI-Assisted Development](../dev/ai-assisted-development.md)** — the
  specification for AI agents. *Why DarkWow is AI-friendly, test pipeline as
  safety net, Python model workflow, 12 agent guardrails.*
  **An AI agent MUST read this document before writing any code.**

---

## 10. Key Invariants

These are architectural constraints extracted from the specification documents.
Every change MUST preserve them. Source document and section in parentheses.

### Privacy & Capabilities

- **L1 for transferable o-caps, L2 for static records.** Not a hierarchy.
  ([privacy.md §2](privacy.md))
- **consume+create preserves N.** Every non-terminal L1 operation nullifies
  exactly 1 old state, creates exactly 1 new Merkle leaf.
  ([contract-wasm-type-system.md §C.0.1](contract-wasm-type-system.md))
- **O-cap composition is additive.** T(A ∘ B) = T(A) + T(B). Shared mutable
  state produces multiplicative explosion T(A × B) = T(A) × T(B).
  ([privacy.md §6](privacy.md))
- **Nullifier is unified across all L1 contracts.** Contract-local Nullifier
  definitions prohibited. Zero-element rejected at construction.
  ([contract-wasm-type-system.md §C.3.5](contract-wasm-type-system.md))

### Circuits & Proofs

- **Every constrain_instance MUST have a visible derivation in the circuit
  body.** A constrain_instance of a bare witness is an Orchard-class
  vulnerability. ([safety.md Lesson 16](../dev/contracts/safety.md))
- **Domain constants on every poseidon_hash.** 7 constants (witness_base(1..7)).
  Every hash call MUST prepend the appropriate domain constant.
  ([privacy.md §5.2](privacy.md))
- **Circuit-harness-metadata triad must agree.** Circuit constrain_instance
  order == harness public input order == metadata return order. Mismatch at
  any vertex is a protocol violation. ([privacy.md §5.3](privacy.md))

### Contracts

- **No per-contract wallet grammar.** Wallet is a generic capability engine.
  Contract-specific client code in contract crates is phantom code.
- **Contracts are instances, not special cases.** Only NativeToken has a
  bespoke wallet path (consensus-critical). Every other contract uses
  manifest-driven machinery.
  ([contract-wasm-type-system.md §A.0.4](contract-wasm-type-system.md))
- **Exec SHALL NOT write state. Apply SHALL NOT validate.** Exec validates
  (nullifier, root check) and returns an update. Apply writes (merkle_add,
  db_set) using only circuit-constrained values.
  ([contract-wasm-type-system.md §A.1.1](contract-wasm-type-system.md))

### Consensus

- **Deterministic fork resolution.** Uncle Merkle: competing blocks at same
  height become uncles sharing reward. No speculative execution.
  ([consensus.md](consensus/consensus.md))
- **Mining keys never copied.** Only wallet forwarding keys may be copied.
  Mining keys stay on mining nodes.
- **Block anchor tree is per-block.** Reset after each block commit. The
  anchor root is deterministic from transactions but NOT in the block header
  (would invalidate PoW).
  ([contract-wasm-type-system.md §C.3.7](contract-wasm-type-system.md))

### Implementation

- **No sed on Rust code.** Edit tool only — precise, auditable, line-exact.
- **No partial compilations mid-plan.** Build/test only when the plan is
  code-complete.
- **Failures read from recorded sources verbatim.** Never reconstructed from
  memory. Failures written inline in plans, never placeholdered.
- **Python model is the specification.** If model and Rust disagree, the model
  is correct until proven otherwise. Fix the model first, then the Rust.

---

## 11. Document Dependency Graph

Reading order for an AI agent onboarding from zero:

```
intro.md + formal-specification.md
    │
    ▼
type-system.md ───────────────────────┐
    │                                 │
    ▼                                 │
ocap.md ── composition.md             │
    │                                 │
    ▼                                 │
contract-wasm-type-system.md          │
    │                                 │
    ├── Part A (shared)               │
    ├── Part B (L2)                   │
    └── Part C (L1) ── privacy.md ────┘
    │
    ▼
consensus/consensus.md ── uncle_merkle.md
    │
    ▼
wallet.md
    │
    ▼
manifest.md ── genesis.md
    │
    ▼
dev/contracts/safety.md
    │
    ▼
dev/ai-assisted-development.md
```

For contract-specific work, insert the contract's page (e.g., `contract/box.md`)
after `genesis.md` and read the corresponding source in `src/contract/<name>/`.

---

## 12. Quick Reference — By Task

| Task | Start Here |
|------|------------|
| Understand the project | [Formal Specification](formal-specification.md) |
| Understand the type system | [Type System](type-system.md) |
| Understand L1 vs L2 | [Contract WASM Type System](contract-wasm-type-system.md) Parts B, C |
| Understand privacy | [Privacy Model](privacy.md) |
| Understand consensus | [Consensus](consensus/consensus.md) |
| Write a contract | [Contract Developer Journey](../dev/contracts/journey.md) |
| Audit a contract | [Contract Safety](../dev/contracts/safety.md) |
| Test a contract | [Testing Overview](../dev/testing/overview.md) |
| Run the pipeline | [Contract Testing Pipeline](pipeline.md) |
| Debug a ZK circuit | [ZK Circuit Troubleshooting](../dev/zk-circuit-troubleshooting.md) |
| Understand the wallet | [Wallet Architecture](wallet.md) |
| Write an AI agent | [AI-Assisted Development](../dev/ai-assisted-development.md) |
| Find a contract spec | [Contract Catalog](../contracts.md) |
| Find a formal proof | [GeneralTheorem.lean](../../../proofs/lean/src/DarkFi/Combinatorial/GeneralTheorem.lean) |
| Review security audit findings | [Contract Safety](../dev/contracts/safety.md) |
| Check audit status | [Audit Documents](audit/README.md) |
| Understand serialization conformance | [Serialization Handover](serialization-conformance-handover.md) |

---

## 13. Security Audits & Historical Handover

- **[Contract Safety — Audit Finding Status](../dev/contracts/safety.md)** — Verified status
  of all security audit findings (Red Team + HAZOP + Comprehensive), consolidated and
  verified against current code as of 2026-08-03. *Start here for current security posture.*
- **[Audit Documents](audit/README.md)** — Directory index of the three original audit
  reports (2026-07-31), preserved as historical snapshots with cross-audit contradiction
  reconciliation.
  - **[Red Team Findings](audit/red-team-findings.md)** — 47 findings with file:line verification.
  - **[Red Team HAZOP Analysis](audit/red-team-hazop-analysis.md)** — 9 root cause families, 6 structural changes.
  - **[Comprehensive Security Audit](audit/comprehensive-security-audit.md)** — ~314 findings, independent methodology.
- **[Serialization Conformance Handover](serialization-conformance-handover.md)** —
  Completed remediation of serialize/deserialize anti-patterns across 32 contracts (2026-07-27).
