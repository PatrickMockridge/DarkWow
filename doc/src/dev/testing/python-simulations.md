# Python Contract Simulations — Smoke Test Layer

Python simulations that model each contract's state machine at the logical
level — no ZK proofs, no crypto, just "who can call what in which state."
They catch the exact class of bugs documented in [Smart Contract Inherent Safety](../contracts/safety.md)
in milliseconds rather than the 4-7 minutes a heavyweight test takes.

**Location:** `sim/` at the repository root.

## What Problem This Solves

The gap between Level 1 (lightweight tests — no ZK, deployment only) and
Level 2 (heavyweight tests — full ZK proofs, 4-7 min) is a cliff. A
developer iterating on a contract's authorization flow, state machine, or
business logic has no way to test those things quickly. The heavyweight
tests are too slow for design iteration; the lightweight tests don't
exercise contract functions at all.

Python simulations fill this gap. They model:

- **All functions** with caller restrictions (who can call what)
- **State machines** — every state, every legal transition, every illegal one
- **Authorization** — role-based access control enforced at the call level
- **Data model** — database trees, keys, values (Python dicts)
- **Capability lifecycle** — what capabilities are produced/consumed per action
- **Edge cases** — double-spend, timeout, coverage failure, race conditions,
  missing deactivation paths, authorization gaps

## What They Don't Model

- ZK proof generation or verification
- WASM execution
- Cryptographic operations (Poseidon, Pedersen, Schnorr)
- Network behavior, block production, or P2P messaging
- Gas metering or execution limits

These are logical simulations. They verify that the contract's design is
correct — not that its implementation compiles or its ZK circuits are sound.

## Architecture

```
sim/
├── contract.py          # Base class: instance mgmt, auth, db, block height
├── state.py             # StateMachine, StateError, AuthError, ConstraintError
├── contracts/
│   ├── escrow.py        # 5-state reference implementation
│   ├── bearer_bond.py   # Two-step interest, coverage void, emergency unstake
│   ├── auction.py       # Dual Auction+Bid state machines
│   ├── gaming.py        # Commit-reveal-settle pattern (7 games + betting_stake)
│   ├── infrastructure.py # Bridge, PoolStake, RelayerEndowment, DrainProtection,
│   │                     # Dex, OtcSwap, Stablecoin
│   └── identity.py      # Attestation, Identity, Tender, LaborMarket, Oracle,
│                         # Subscription, InsuranceMarket, DaoEscrow, DarkbetExchange
└── tests/
    ├── test_escrow.py
    └── test_bearer_bond.py
```

### Base Class

`sim/contract.py` provides the shared infrastructure every simulation uses:

```python
class Contract:
    instances: dict          # instance_id → Instance (with StateMachine)
    db: dict                 # tree_name → {key → value}
    block_height: int        # simulated block height

    def only(caller, *roles)       # authorization check
    def only_state(iid, *states)   # state guard
    def transition(iid, to)        # state transition
    def advance_block(n)           # simulate time passing
```

### State Machine

`sim/state.py` provides a directed graph of legal transitions:

```python
sm = StateMachine("Created")
sm.add_transition("Created", "Funded", "Cancelled")
sm.add_transition("Funded", "Claimed", "Refunded")
sm.transition("Funded")          # OK
sm.transition("Created")         # raises StateError
```

Terminal states (Claimed, Refunded, Cancelled) have no outgoing transitions —
any attempt to transition from them raises `StateError`.

### Error Types

| Error | When |
|-------|------|
| `StateError` | Illegal state transition attempted |
| `AuthError` | Unauthorized caller attempted an action |
| `ConstraintError` | Business rule violated (insufficient coverage, below minimum, timeout not reached) |

## Example: Bearer Bond Two-Step Interest Flow

The bearer bond simulation caught a design issue during implementation:

```python
bond = BearerBond()
issuer = Caller("issuer_co", ["issuer"])
holder = Caller("alice", ["holder"])

# Create series and issue stake
bond.create_series(issuer, "series-1", interest_rate_bps=500, maturity_block=1_000_000)
token = bond.issue_stake(issuer, "series-1", "coin-1", holder.name, principal=100_000)

# Holder requests interest — creates Pending claim, does NOT advance last_claim_block
bond.advance_block(500_000)
interest = bond.request_interest(holder, token, bond.block_height, "payment-key-1")

# Issuer must prove coverage before paying
bond.prove_coverage(issuer, "series-1", total_outstanding=100_000,
                    total_interest_obligation=interest, reserve_amount=200_000,
                    report_block=bond.block_height)

# Issuer pays — advances last_claim_block, marks claim Paid
bond.pay_interest(issuer, token, bond.block_height)
assert bond.claims[f"coin-1:{bond.block_height}"].status.value == "Paid"
```

## Design Bug Caught: ProveCoverageV1 / EmergencyUnstakeV1 Dead Path

During implementation, the simulation immediately revealed that
`ProveCoverageV1` requires `coverage_ratio_bps >= 10000` (full
collateralization), but `EmergencyUnstakeV1` requires a report showing
`coverage_ratio_bps < 10000`. The `prove_coverage()` function can only
file healthy reports — so emergency unstake can never be triggered
through the `ProveCoverageV1` path alone.

The simulation models this correctly: `prove_coverage()` enforces the
`>= 10000` check, while a separate `file_report()` method simulates the
external oracle or governance path that would be needed to file a
below-minimum report. See
[bearer_bond.py](../../../sim/contracts/bearer_bond.py) for the full
implementation with explanatory comments.

This is the exact class of bug the simulations are designed to catch —
a state machine hole that would survive code review but fail at the
logical level.

## Running the Simulations

```bash
# Import verification (all 27 contracts)
python3 -c "from sim.contracts.escrow import Escrow; from sim.contracts.bearer_bond import BearerBond; print('OK')"

# Run individual contract tests
python3 -c "from sim.contracts.bearer_bond import BearerBond; ..."

# With pytest (when available)
python -m pytest sim/tests/ -v
```

## Writing a New Simulation

1. **Read the contract's entrypoint** — understand what functions exist, who
   can call them, and what state transitions they trigger.
2. **Model the state machine** — each entity with a lifecycle gets a
   `StateMachine`. Add all legal transitions. Terminal states are implicit
   (no outgoing transitions).
3. **Implement each function as a method** — name it after the contract
   function. Call `self.only(caller, ...)` for auth, `self.only_state(...)`
   for state guards, `self.transition(...)` for state changes.
4. **Add edge case tests** — at minimum: all legal transitions, at least one
   illegal transition rejected, at least one auth check, and one business
   rule constraint (timeout, minimum, ratio threshold).

## When to Use

| You want to... | Use |
|---|---|
| Verify a state machine design before writing Rust | Python simulation |
| Check that authorization gates are correctly placed | Python simulation |
| Test "what if" scenarios (issuer never pays, coverage drops, timeout expires) | Python simulation |
| Verify that every `active` flag has a deactivation path | Python simulation |
| Test ZK circuit correctness | Level 2 heavyweight test |
| Test WASM execution and on-chain state | Level 2 heavyweight test |
| Test deployment through Deployooor | Level 1 lightweight test |
| Test multi-node P2P and mining | Level 3 containerized localnet |

## Consensus-Level Python Models

Beyond contract simulations, DarkWow ships with exhaustive Python models
of the consensus protocol itself. These are 1:1 executable specifications —
every function in the model has a corresponding function in the Rust source
that must produce identical outputs for identical inputs.

**Location:** `contrib/model/` at the repository root.

| Model | File | Tests | Purpose |
|-------|------|-------|---------|
| Chain Validation | `contrib/model/chain_validation_model.py` | 34/34 | Block production, PoW target computation, difficulty adjustment, competing block storage, uncle-merkle consensus (proof construction + verification), chain reorganization (Bitcoin ActivateBestChain), finality anchoring, timestamp validation |
| VM State Machine | `contrib/model/vm_state_model.py` | 8/8 | RandomX FFI concurrency model — proves that per-VM Mutex wrapping eliminates all concurrent access paths across miner task, broadcast handler, GetTip handler, RPC miner, stratum submit, and block template generation |
| Merge Mining | `contrib/model/merge_mining_model.py` | ALL VERIFIED | Monerod → p2pool sidecar → xmrig sidecar → share → mm_rpc → dwowd → DarkWow block. 2 merge-mining + 1 native node, consensus verified |

### Why Consensus Models Exist

Consensus bugs are the most expensive bugs in blockchain development.
A single off-by-one in target computation, a missing lock in a VM cache,
or an incorrect reorg condition can cause chain splits, segfaults, or
permanent divergence. These bugs take **hours to debug in Rust** (compile
→ deploy → run pipeline → inspect logs → repeat) but **seconds in Python**
(modify model → `python3 model.py` → instant feedback).

The models are the **specification** — the Rust code implements them.
If the model and Rust disagree, the model is correct until proven
otherwise. If the model passes all tests but the pipeline fails, the
model is incomplete — extend the model first, then fix the Rust.

### Running the Consensus Models

```bash
# Chain validation (33 scenarios)
python3 contrib/model/chain_validation_model.py

# VM concurrency state machine (8 scenarios)
python3 contrib/model/vm_state_model.py
```

### Dockernet Validation

#### Initial Verification (May 2026)

The five-node uncle-merkle predictions from `test_multi_node_uncle_merkle_convergence`
(70+ uncle blocks, 300+ competing blocks across 5 full-capacity miners) were confirmed
by the `--nodes 5` native mining dockernet. All 5 nodes mined continuously, blocks
propagated via P2P, competing blocks became uncles via `uncle_merkle_root`. The
dockernet ran 24 minutes, reached heights 17-20, zero segfaults, before hitting
resource limits on a 24-thread/48GB machine.

#### Uncle-Merkle Consensus Verification (June 2026)

After completing 1:1 uncle merkle proof verification in the Python model
(`UncleProof`, `verify_uncle_proof`, `check_uncles` — all verified against
Rust `src/linear/src/block.rs` and `src/linear/src/validation.rs`), the full
uncle-merkle flow was tested on a 5-node dockernet:

- **Pipeline**: `test_pipeline.sh --mode native --fresh --nodes 5` — 23 PASS, 0 FAIL
- **Uncle blocks observed**: Block 9 showed non-zero `uncle_merkle_root` (earlier
  blocks had zero roots as competing blocks had not yet accumulated — this
  matches the Python model's prediction that uncles appear after ~8 blocks
  with 5 miners)
- **Supply audit**: `blockchain.get_cumulative_supply` RPC returns cumulative
  Pedersen commitment chain state verifiable against the emission schedule
- **All three mining paths wired**: built-in miner, stratum, and merge mining
  all collect uncles from `take_competing_blocks` before generating block
  templates — matching Python's `MiningNode.miner_cycle()` exactly

The Python model's prediction that competing blocks take time to accumulate
before uncles appear was directly observed: blocks 1-7 had zero uncle roots,
block 9 had non-zero. This validates the model's timing assumptions.

### Development Model for Consensus Changes

The process is strictly sequential. No step may be skipped or done
out of order:

```
1. Model in Python  — write/modify the model until all tests pass
2. HAZOP the model  — adversarial review: what edge cases does it miss?
3. Line-by-line audit — verify every Python function has a Rust counterpart
4. Implement in Rust — translate the model 1:1, using only precise Edit operations
5. Cross-check      — verify Rust outputs match Python for identical inputs
6. Push + pipeline  — only after steps 1-5 are confirmed complete
```

### Guardrails for AI-Assisted Consensus Development

When using AI tools to modify consensus code:

1. **Model first.** Never write Rust until the Python model passes.
2. **No invented mechanisms.** Every consensus rule must trace to Ethereum,
   Bitcoin, or Polkadot.
3. **No sed/regex on Rust.** Use the Edit tool for precise, auditable changes.
4. **Compile after every file.** Never batch-fix across multiple files.
5. **The pipeline is the LAST step.** "Code compiles" ≠ "run the pipeline."
6. **Never poll a running pipeline.** Start in background, wait for notification.
7. **Failures go in the plan.** Every process failure must be recorded verbatim
   with its learning. Plans are the durable record.
8. **Ask before running the pipeline.** Walk through every guardrail and confirm.

## Related Documents

- [Smart Contract Inherent Safety](../contracts/safety.md) — the class of bugs these simulations catch
- [Testing Overview](overview.md) — the full 4-level testing taxonomy
- [AI-Assisted Development](../ai-assisted-development.md) — how AI tools use these models
- [Level 2: Heavyweight Tests](level-2-heavyweight.md) — ZK proof tests (the next level up)
- [Level 1: Lightweight Tests](level-1-lightweight.md) — deployment tests (the level below)
- [Block Explorer Guide](../../testnet/block-explorer.md) — querying nodes for uncle data and supply audit
- [Supply Audit](../../arch/consensus/consensus.md#supply-audit-pedersen-cumulative-commitment-chain) — Pedersen cumulative commitment chain
