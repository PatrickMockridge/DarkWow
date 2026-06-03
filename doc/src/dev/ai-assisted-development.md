# AI-Assisted Development

DarkWow's architecture is intentionally AI-friendly. Two architectural
properties make it safe to "vibe-code" complex smart contracts with AI
assistance: O-Cap containment and deterministic consensus. Combined with
the four-level test pipeline, AI-generated contract code can achieve audit
quality superior to most of the industry before it ever touches mainnet.

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

This means AI-generated code has a **contained blast radius**. A bug in
one contract cannot leak into another. Contracts compose naturally because
they interact only through the capabilities they receive — the compiler
and runtime enforce the boundary, not programmer discipline.

### Deterministic Consensus

Most blockchains use speculative execution — the same transaction can
produce different results depending on when it lands, what else is in the
block, or which fork it ends up on. This makes AI-generated code hard to
test because the ground moves under it.

DarkWow uses **Uncle Merkle consensus** — a deterministic protocol where
the canonical chain with the most accumulated PoW obligates offering uncle
chains a one-time option to form a side chain and share the reward.
Stateless verification via pure Merkle proof. No overlay, no speculative
state, no `diff()` computation that depends on sequence history.

**Same code, same block, same result — every time.** AI-generated code is
tested under reproducible conditions. If a test passes once, it passes
always. If a test fails, the failure is real and reproducible — not a
timing artifact.

## The Test Pipeline as AI Safety Net

DarkWow's [four-level testing infrastructure](testing/overview.md) catches different classes of bugs at each layer. When used as the feedback loop for AI-assisted development, every iteration tightens the net. See the [Testing Overview](testing/overview.md) for the complete level descriptions and commands.

Each level catches what the previous level physically cannot. Level 1
won't catch a ZK circuit bug. Level 2 won't catch a P2P gossip failure.
Level 3 won't catch a deployment configuration issue. **You need all four.**
No gaps = no surprises.

## What "Audit Superiority" Means

The claim is specific and falsifiable.

A traditional smart contract audit is a human consultant reading your code
for 1-4 weeks. They check for known vulnerability patterns, review
business logic, and produce a report. They do not:
- Run your contract under real ZK proofs (Level 2)
- Test it in a multi-node network with live mining (Level 3)
- Deploy it across machines and verify sync (Level 4)
- Execute the full stack deterministically with reproducible results

DarkWow's pipeline does all of these. The combination of deterministic
architecture (Uncle Merkle) plus exhaustive pipeline coverage catches
entire classes of bugs that traditional audit firms never test for —
because they can't. Most blockchains don't have deterministic consensus,
so multi-node testing is inherently flaky. Most don't have O-Caps, so
contract interaction bugs require deep manual analysis.

This is not "better than a professional audit firm at reading code." It's
that the pipeline verifies things no human auditor can — and when used
completely, the surface area of verified behavior exceeds what any
traditional audit covers.

## The Developer's Responsibility

The architecture is the safety net, but **you must use it**. The compact is
simple:

1. Every contract passes Level 1 before Level 2
2. Every contract passes Level 2 before Level 3
3. Every contract passes Level 3 before Level 4
4. Every contract passes Level 4 before mainnet
5. **No skipped phases. No "it'll probably be fine."**

The pipeline is the gate. The infrastructure cannot save you from not
using it. Your responsibility is to leave no gaps — feed every
AI-generated change through all four levels before it ships.

## Practical Vibe-Coding Workflow

Here is the concrete loop for AI-assisted contract development on DarkWow:

```
1. AI generates contract code (Claude Code, Cursor, Copilot, etc.)

2. Level 1 catches immediate mistakes
   cargo test -p dwowd test_pipeline
   → Fix compilation errors, serialization bugs
   → Iterate with AI until green

3. Level 2 reveals ZK circuit and business logic issues
   RAYON_NUM_THREADS=10 cargo test --release -p dwowd test_heavyweight
   → Fix proof failures, state machine bugs
   → Iterate with AI until green

4. Level 3 confirms real-world readiness
   ./contrib/docker/darkwow-testnet/test_pipeline.sh --mode merge
   → Fix P2P, mining, block propagation issues
   → Iterate with AI until green

5. Level 4 confirms multi-machine deployment
   ./contrib/docker/darkwow-testnet/test_pipeline.sh --mode join-merge
   → Fix deployment configuration issues
   → Iterate with AI until green

6. Contract is now "pipeline-audited" — ready for mainnet
```

Each level is a feedback loop. The AI proposes changes, the pipeline
validates them. The tighter the loop, the faster the iteration. Level 1
should run constantly during development. Level 2 on every meaningful
change. Level 3 before merging. Level 4 before deploying.

## Python Consensus Models — Pre-Code Reasoning Layer

DarkWow ships with exhaustive Python models of its consensus protocol.
These are NOT simulations — they are **1:1 executable specifications**
that an AI agent can reason about, modify, and validate before touching
a single line of Rust.

### Available Models

| Model | File | Purpose |
|-------|------|---------|
| Chain Validation | `contrib/model/chain_validation_model.py` | Block production, PoW, difficulty adjustment, competing blocks, uncle-merkle consensus, chain reorganization, finality anchoring. 22/22 tests. |
| VM State Machine | `contrib/model/vm_state_model.py` | RandomX concurrency model — proves that per-VM Mutex wrapping eliminates all concurrent FFI access paths. 8/8 scenarios. |

### Why They Exist

Consensus bugs are the most expensive bugs in blockchain development. A
single off-by-one in target computation, a missing lock in a VM cache, or
an incorrect reorg condition can cause chain splits, segfaults, or
permanent divergence. These bugs take **hours to debug in Rust** (compile
→ deploy → run pipeline → inspect logs → repeat) but **seconds in Python**
(modify model → `python3 model.py` → instant feedback).

The models enable:
- **Pre-code reasoning**: Model the fix in Python, prove it works, then translate to Rust 1:1
- **HAZOP analysis**: Adversarial review of every failure mode before code exists
- **Cross-check**: Verify Rust produces identical outputs to Python for identical inputs
- **AI-assisted debugging**: Paste model output into an AI session — the AI can reason about deterministic consensus logic without needing to understand Rust, sled, RandomX FFI, or async runtime details

### How AI Agents Use the Models

The development model is strictly sequential. An AI agent following this
process will produce correct consensus code on the first Rust attempt:

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

### AI Agent Guardrails

These guardrails were accumulated over weeks of AI-assisted consensus
development. They exist because each one was violated at least once,
causing hours of wasted pipeline time. An AI agent working on DarkWow
consensus code MUST follow every one:

1. **Model first, Rust second.** Never write Rust until the Python model
   passes all scenarios. If the model has a gap (e.g., can't model FFI
   concurrency), build a separate state machine model for that domain.

2. **No invented mechanisms.** Every consensus rule must trace to a
   production reference: Ethereum (gETH), Bitcoin Core, or Polkadot
   (for uncle-merkle). If you can't cite the source, don't write the code.

3. **Never use sed or regex on Rust code.** Use only the Edit tool for
   precise, auditable, line-exact changes. sed doesn't understand Rust
   syntax and will mangle variable names, break macros, and create
   expressions that compile but are logically wrong.

4. **Compile after every file.** Fix one file, `cargo check`, verify
   clean. Never batch-fix across multiple files without intermediate
   compilation. Errors compound and debugging becomes exponentially
   harder.

5. **The pipeline is step 7 of 7.** "Code compiles" does not mean
   "run the pipeline." The pipeline validates that BOTH the model AND
   the Rust are correct. Running it prematurely tests broken code.

6. **Never poll a running pipeline.** Start it in the background and
   wait for harness notification. Polling wastes context and provides
   no actionable data until the run completes.

7. **Failures belong in the plan.** Every process failure — every
   skipped step, every shortcut, every incorrect assumption — must be
   written verbatim in the plan file with its learning. The plan is
   the durable record. Conversation context can be compacted away.

8. **Ask before running the pipeline.** Walk through every memory rule
   and confirm it's satisfied. If uncertain about any rule, ask the
   user. Never launch `test_pipeline.sh` without explicit confirmation.

9. **Full verification, not "got to block 2."** A fix that works for
   block 2 and fails at block 4 is not a fix. Verify continuous
   production to the pipeline's target height on ALL nodes.

10. **The spec is fixed.** Two native mining nodes. Both mine. Both
    produce blocks. They converge. If the code doesn't do this, the
    code is wrong — never change the spec to work around a bug.

## AI Tool Guidance

**Any AI coding tool works.** Claude Code, Cursor, GitHub Copilot, or any
LLM with code generation capability. The pipeline is tool-agnostic.

**Point AI at existing contracts as templates.** Every contract in
`src/contract/<name>/` is self-contained with its own ZK circuits, tests,
and harness. Tell the AI: "Model this new contract on
`src/contract/promissory_note/` — same structure, same test patterns."

**Give the AI context.** Paste the test pipeline help output
(`./test_pipeline.sh --help`) into your AI session. The four-level testing
commands are the feedback loop — the AI needs to know what "passes"
looks like.

**The pipeline output is your shared language.** When a test fails, paste
the failure into the AI session. The deterministic nature of DarkWow means
the failure is reproducible — the AI can reason about it without worrying
about flakiness or timing.

## Further Reading

- [Python Contract Simulations](testing/python-simulations.md) — Contract state machine models (27 contracts) AND consensus protocol models (block production, VM concurrency, reorg, finality)
- [Testing Overview](testing/overview.md) — Full four-level taxonomy with consensus model reference
- [Level 1: Lightweight Tests](testing/level-1-lightweight.md) — Fast feedback loop
- [Level 2: Heavyweight Tests](testing/level-2-heavyweight.md) — ZK proofs
- [Level 3: Containerized Localnet](testing/level-3-localnet.md) — Docker multi-node
- [Level 4: Containerized Devnet](testing/level-4-devnet.md) — Multi-machine deployment
- [Ideology](../philosophy/ideology.md) — Core design principles
- [O-Cap Authorization](../arch/ocap.md) — How capabilities replace access control
- [Uncle Merkle Consensus](../arch/consensus/consensus.md) — Deterministic consensus
- [Contract Development Guide](contracts.md) — Smart contract architecture and patterns
