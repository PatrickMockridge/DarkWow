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

## Related Documents

- [Smart Contract Inherent Safety](../contracts/safety.md) — the class of bugs these simulations catch
- [Testing Overview](overview.md) — the full 4-level testing taxonomy
- [Level 2: Heavyweight Tests](level-2-heavyweight.md) — ZK proof tests (the next level up)
- [Level 1: Lightweight Tests](level-1-lightweight.md) — deployment tests (the level below)
