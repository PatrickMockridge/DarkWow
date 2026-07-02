# AI-Assisted Development

DarkWow's architecture is intentionally AI-friendly. Three architectural
properties make it safe to "vibe-code" complex smart contracts with AI
assistance: O-Cap containment, deterministic consensus, and manifest-first
wallet integration. Combined with the test pipeline, AI-generated contract
code reaches audit quality before it touches mainnet.

## Why DarkWow is AI-Friendly

### O-Cap Containment

Traditional smart contract platforms use access control lists — a contract
checks `msg.sender` against a stored whitelist. If the AI generates a
flawed check, the contract is compromised.

DarkWow uses **object capabilities** (O-Caps). A contract holds only the
capability tokens explicitly passed to it at invocation time. There is no
ambient authority — no `msg.sender`, no global namespace, no storage that
any contract can read. A vibe-coded contract simply cannot access tokens,
state, or operations it wasn't explicitly granted.

**Contained blast radius.** A bug in one contract cannot leak into another.
Contracts compose naturally through the capabilities they receive — the
runtime enforces the boundary, not programmer discipline.

### Deterministic Consensus

Most blockchains use speculative execution — the same transaction can
produce different results depending on timing, block contents, or fork
choice. This makes AI-generated code hard to test because the ground
moves under it.

DarkWow uses **Uncle Merkle consensus** — deterministic fork resolution
where competing blocks at the same height become uncles sharing the reward.
Stateless verification via pure Merkle proof. No overlay, no speculative
state, no `diff()` computation depending on sequence history.

**Same code, same block, same result — every time.** If a test passes once,
it passes always. Failures are real and reproducible — not timing artifacts.

### Manifest-First Architecture

Every DarkWow contract carries a **TOML manifest on-chain** declaring its
interface: WASM exports, ZK circuits, state schema, and capability
requirements. The wallet reads these manifests and auto-configures.

**Zero wallet code changes for new contracts.** An AI generates a contract,
deploys it via Deployooor, and the manifest is on-chain. Every wallet on
the network automatically knows how to interact with it. No hardcoded ABIs.
No per-contract client code. No ecosystem fragmentation where different
wallets support different subsets of contracts.

This also means the **AccountManager** key architecture is AI-friendly:
keys are storage-agnostic (sled for mining, SQLite for wallet), with a
clean import/export API. AI-assisted key management is a single command
away — `dwowd --export-secret | dwow_wallet wallet import-secrets`.

Combined with the wallet's **clean 2-database architecture** (chain sled
for blocks, SQLite for everything else — no dual-stores, no bridges),
the system has clear separation of concerns that an AI can reason about
without understanding sled internals, SQLCipher, or async runtime details.

## The Test Pipeline as AI Safety Net

DarkWow's [testing infrastructure](testing/overview.md) catches different
classes of bugs at each layer. When used as the feedback loop for
AI-assisted development, every iteration tightens the net:

| Level | Scope | Typical AI Loop |
|-------|-------|-----------------|
| 1 — Lightweight | Deployooor deployment (34 contracts, no ZK) | Seconds — fix compilation, serialization |
| 2 — Heavyweight | ZK proofs, contract execution, uncle-merkle | Minutes — fix proof failures, state machine bugs |
| 3 — Localnet | Multi-node Docker mining + wallet sync | ~20 min build + run — fix P2P, block propagation |
| 4 — Devnet/Join | Multi-machine or public testnet deployment | Variable — fix deployment config |

Each level catches what the previous level physically cannot. Level 1 won't
catch a ZK circuit bug. Level 2 won't catch a P2P gossip failure. Level 3
won't catch a deployment configuration issue. **Use all four.** No gaps.

See the [Testing Overview](testing/overview.md) for the complete descriptions.

## What "Audit Superiority" Means

A traditional smart contract audit is a human consultant reading code for
1-4 weeks. They check for known vulnerability patterns, review business
logic, and produce a report. They do not:

- Run your contract under real ZK proofs (Level 2)
- Test it in a multi-node network with live mining (Level 3)
- Verify wallet sync, scan, and balance across nodes (Level 3 wallet)
- Deploy across machines and verify sync (Level 4)
- Execute the full stack deterministically with reproducible results

DarkWow's pipeline does all of these. Deterministic consensus means
multi-node testing is reliable, not flaky. O-Caps mean contract interaction
bugs are contained by the runtime. Manifests mean the wallet verifies
interface claims automatically. The combination catches entire classes of
bugs that traditional audits never test for.

## The Developer's Responsibility

The architecture is the safety net, but **you must use it**:

1. Every contract passes Level 1 before Level 2
2. Every contract passes Level 2 before Level 3
3. Every contract passes Level 3 before Level 4
4. Every contract passes Level 4 before mainnet
5. **No skipped phases. No "it'll probably be fine."**

The pipeline is the gate. Feed every AI-generated change through all four
levels before it ships.

## Practical Vibe-Coding Workflow

Here is the concrete loop for AI-assisted contract development:

```
1. AI generates contract code (Claude Code, Cursor, Copilot, etc.)

2. Level 1 catches immediate mistakes
   cargo test -p dwowd test_pipeline
   → Fix compilation errors, serialization bugs
   → Iterate with AI until green

3. Level 2 reveals ZK circuit and business logic issues
   RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd test_heavyweight
   → Fix proof failures, state machine bugs
   → Iterate with AI until green

4. Level 3 confirms real-world readiness
   RAYON_NUM_THREADS=10 contrib/docker/darkwow-testnet/test_pipeline.sh --mode native --with-wallet 2
   → Fix P2P, mining, block propagation, wallet sync/scan issues
   → Iterate with AI until green

5. Level 4 confirms multi-machine/public deployment
   contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-native
   → Fix deployment configuration issues
   → Iterate with AI until green

6. Contract is now "pipeline-audited" — ready for mainnet
```

Each level is a feedback loop. Level 1 should run constantly during
development. Level 2 on every meaningful change. Level 3 before merging.
Level 4 before deploying.

## Python Consensus Models — Pre-Code Reasoning Layer

DarkWow ships with exhaustive Python models. These are **1:1 executable
specifications** that an AI agent can modify and validate before touching
Rust. Python leads, Rust follows.

| Model | File | Purpose |
|-------|------|---------|
| Chain Validation | `contrib/model/chain_validation_model.py` | Block production, PoW, difficulty, uncles, reorg, finality. All passing. |
| VM State Machine | `contrib/model/vm_state_model.py` | RandomX concurrency — per-VM Mutex eliminates FFI races. All passing. |
| Wallet (production) | `contrib/model/wallet_model.py` | Full wallet: keys, AEAD, scan, capabilities, DB. 1:1 Rust mapping. |
| Dockernet | `contrib/model/dockernet_model.py` | Multi-node mining + wallet pipeline. Key management failure modes. |
| Key Management | `contrib/model/key_management.py` | AccountManager spec: import/export, roundtrip, AEAD. 20 tests. |
| Wallet Simulation | `contrib/model/wallet_simulation.py` | Chain→wallet bridge with mining + reorg scenarios. |
| Token Balance | `contrib/model/proof_of_token_balance.py` | Per-block Pedersen mass balance verification. |
| Merge Mining | `contrib/model/merge_mining_model.py` | Monero p2pool merge mining protocol. |

Models enable:
- **Pre-code reasoning**: Model the fix in Python, prove it works, then translate to Rust 1:1
- **HAZOP analysis**: Adversarial review of every failure mode before code exists
- **Cross-check**: Verify Rust produces identical outputs to Python for identical inputs
- **AI-assisted debugging**: Paste model output into an AI session — the AI can reason about consensus logic without understanding Rust, sled, RandomX FFI, or async runtime details

### How AI Agents Use the Models

```
1. Model in Python  — write/modify the model until all tests pass
2. HAZOP the model  — adversarial review: what edge cases does it miss?
3. Line-by-line audit — verify every Python function has a Rust counterpart
4. Implement in Rust — translate the model 1:1, using only the Edit tool
5. Cross-check      — verify Rust outputs match Python for identical inputs
6. Push + pipeline  — only after steps 1-5 are confirmed complete
```

**The Python model is the specification. The Rust code implements it.**
If the model and Rust disagree, the model is correct until proven
otherwise. If the model passes all tests but the pipeline fails, the
model is incomplete — extend the model first, then fix the Rust.

## AI Agent Guardrails

These guardrails exist because each was violated at least once, causing
wasted pipeline time. An AI agent working on DarkWow MUST follow them:

1. **Model first, Rust second.** Never write Rust until the Python model
   passes all scenarios.

2. **No invented mechanisms.** Every consensus rule must trace to a
   production reference: Bitcoin Core, Polkadot (uncle-merkle), or
   Ethereum. If you can't cite the source, don't write the code.

3. **Use only the Edit tool for Rust.** Never sed or regex on Rust code.
   Edit tool gives precise, auditable, line-exact changes.

4. **Compile after every file.** Fix one file, `cargo check`, verify clean.
   Never batch-fix across files without intermediate compilation.

5. **The pipeline is step 7 of 7.** "Code compiles" does not mean "run the
   pipeline." Verify everything locally first.

6. **Never poll a running pipeline.** Start it in the background and wait
   for harness notification. Polling wastes context.

7. **Failures belong in the plan.** Every process failure, shortcut, and
   incorrect assumption must be written verbatim in the plan file.

8. **Ask before running the pipeline.** Walk through every memory rule.
   If uncertain, ask the user.

9. **Full verification, not partial.** A fix that works for 2 blocks and
   fails at 4 is not a fix. Verify continuous production.

10. **The spec is fixed.** Two mining nodes. Both mine. Both converge. If
    the code doesn't do this, the code is wrong — never change the spec.

11. **AccountManager is the key authority.** All key operations flow
    through AccountManager. No shell-level key manipulation. Use
    `import_base58`/`export_base58` for key sharing.

12. **Everything is a capability.** Coinbase, bearer bonds, stablecoins —
    all discovered through the generic AEAD scanner. No per-contract
    special cases in the wallet.

## AI Tool Guidance

**Any AI coding tool works.** Claude Code, Cursor, GitHub Copilot, or any
LLM with code generation capability. The pipeline is tool-agnostic.

**Point AI at existing contracts as templates.** Every contract in
`src/contract/<name>/` is self-contained with its own ZK circuits, tests,
and harness. Tell the AI: "Model this new contract on
`src/contract/promissory_note/` — same structure, same test patterns."

**Give the AI context.** Paste the test pipeline help output
(`./test_pipeline.sh --help`) into your AI session. The testing commands
are the feedback loop — the AI needs to know what "passes" looks like.

**The pipeline output is your shared language.** When a test fails, paste
the failure into the AI session. Deterministic consensus means the failure
is reproducible — the AI can reason about it without worrying about
flakiness or timing.

## Further Reading

- [README](/README.md) — Architecture overview, five commitments, quick start
- [Testing Overview](testing/overview.md) — Full test taxonomy
- [Python Models](testing/python-simulations.md) — All simulation specs
- [Contract Development Guide](contracts.md) — Smart contract architecture
- [O-Cap Authorization](../arch/ocap.md) — How capabilities replace access control
- [Uncle Merkle Consensus](../arch/consensus/consensus.md) — Deterministic consensus
- [Wallet Architecture](../arch/wallet.md) — Manifest-first, full node design
- [Key Management](../arch/key-management.md) — AccountManager specification
- [Ideology](../philosophy/ideology.md) — Core design principles
