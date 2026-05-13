# Consensus

DarkWow uses **Uncle Merkle consensus** with RandomX Proof-of-Work for new networks (`linear-testnet`, `darkwow-testnet`). The original fork/overlay consensus remains active for the legacy `testnet` network. Both implementations coexist in the codebase, with new development targeting the linear blockchain.

## Why Uncle Merkle Was Chosen Over the Overlay/Diff Architecture

The upstream DarkWow consensus uses a speculative overlay/diff system with sled-overlay for transactional state management. This architecture exists primarily to support the "under one tent" DAO governance model: a complex fork-deciding mechanism is required because the DAO must adjudicate which fork is canonical, preventing natural chain splits.

This fork rejects that entire design stack for several reasons:

**1. Engineering complexity from governance requirements**

The overlay/diff system exists because upstream's DAO governance requires a mechanism to decide between competing forks. This creates a cascade of complexity: speculative state verification, overlay checkpoints, diff logging for rollback, and implicit fork competition. None of this is necessary in a pure PoW system where the canonical chain is simply the one with the most work — and where hard forks are a natural, healthy part of the ecosystem.

**2. Non-deterministic behavior breaks testing**

The upstream consensus uses speculative verification where state can be committed, rolled back, or left in limbo. `diff()` computation depends on sequence history — the same code produces different results depending on timing. This makes deterministic testing impossible. Race conditions and timing-dependent state create flaky tests that erode confidence in the entire contract system.

**3. The overlay adds complexity without benefit**

The sled-overlay system provides transactional state with automatic rollback. On this fork, there is no DAO deciding between forks — so there is nothing to roll back. Plain sled is simpler, faster, and predictable. Every state change is final.

**4. Pure PoW means forks are a feature, not a bug**

Without a governance DAO motivated to keep everything under one tent, chain splits are handled the Bitcoin way: miners follow the chain with the most work. If a contentious hard fork occurs, both sides can coexist (like BTC/BCH). The Uncle Merkle mechanism makes this Pareto efficient — competing miners still get partial reward rather than zero.

The old upstream overlay-DAG consensus specification is preserved for reference at [legacy/consensus_dag.md](../legacy/consensus_dag.md).

## Current Design: Uncle Merkle with Pin Mechanism

### The Pin Mechanism

The linear blockchain uses a **use-it-or-lose-it pin mechanism** for uncle blocks:

**Rules:**
1. Canonical chain is **obligated** to offer a pin to valid uncle chains
2. Pin reward: 50% at depth 1, halving each depth (25%, 12.5%, 6.25%...)
3. Uncle chain has a **one-time** option to accept or reject the pin
4. If accepted: Uncle gets pin reward, canonical absorbs all uncle transactions
5. If rejected: Uncle loses coinbase entirely, canonical absorbs transactions anyway

**Why this is elegant:**
- **No secret mining incentive**: By the time an uncle publishes, canonical already has the transactions
- **No uncle-farming**: Canonical must offer, cannot refuse a valid uncle
- **No complex multi-step distribution**: Simple one-shot accept/reject decision
- **Fork absorption**: Uncle chain gives up reward but gains inclusion

**The equilibrium:**
Rejection is strictly dominated - accepting gives 50%+ reward, rejecting gives 0. Rational miners always accept.

### Uncle Merkle Structure

Uncle blocks are referenced in canonical blocks via a merkle tree:
- Uncle merkle root stored in canonical block header
- Merkle proof provides stateless verification
- No uncle storage required for verification (only for archival)

### Reward Distribution

The canonical block pays pin rewards from its own block reward - **no over-minting**:

| Uncle Depth | Pin Reward | Uncle Gets | Canonical Gets |
|-------------|------------|------------|---------------|
| None        | -          | -          | 100%          |
| 1           | 50%        | 50% of block reward | 50% (100% - 50%) |
| 2           | 25%        | 25% of block reward | 75% (100% - 25%) |
| 3           | 12.5%      | 12.5% of block reward | 87.5% (100% - 12.5%) |

**Invariant:** `canonical_reward + sum(uncle_rewards) = base_reward` (exactly 100%)

### Testing Benefits

The linear blockchain's consensus model is **ideal for testing**:

1. **Deterministic**: Same input → same output every time. No race conditions.
2. **No rollback**: State changes are final. No speculative commits that could vanish.
3. **Stateless verification**: Only block headers + merkle proofs needed. No WASM execution.
4. **Plain storage**: Uses sled directly, not sled-overlay. Simpler, faster, predictable.
5. **Isolated**: Can run 5-node localnet harness with full consensus without the full validator stack.

Example test harness behavior:
```rust
// Create 5-node localnet
let harness = LinearFiveNodeHarness::new()?;

// Deploy contracts
harness.deploy_genesis_contracts()?;

// Alice mines genesis, broadcast to all
let genesis_block = harness.alice_create_genesis();
harness.broadcast_block(&genesis_block)?;

// Alice mines blocks, each broadcast to all
// All nodes verify sync
harness.verify_sync()?;
```

### Comparison to Upstream Consensus

| Aspect | Upstream (Fork/Overlay) | This Fork (Uncle Merkle) |
|--------|----------------------|----------------------|
| State management | Overlay + diffs + rollback | Plain sled |
| Fork resolution | Implicit competition | Explicit uncle reference |
| Mining risk | All-or-nothing | Bounded partial reward |
| Verification | Heavy WASM + sled lookups | Merkle proof only |
| Determinism | Non-deterministic in time | Fully deterministic |
| Testing | Flaky, timing-dependent | Deterministic, isolated |
| Complexity | High | Low |

## Current State (May 2026)

Both consensus implementations are active. The network name in `dwowd_config.toml`
determines which one is used at startup (`bin/darkfid/src/main.rs:188-189`):

| Network | Consensus | Location | Status |
|---------|-----------|----------|--------|
| `testnet` | Fork/overlay (DAG) | `src/validator/consensus.rs` | Legacy — maintained, no new features |
| `linear-testnet` | Uncle Merkle (linear) | `src/linear/` | Development — WASM/Runtime integration in progress |
| `darkwow-testnet` | Uncle Merkle (linear) | `src/linear/` | Public testnet — mining, contracts, merge mining |

**In the fork-based validator**, uncle Merkle verification is a placeholder
("Phase 2 TODO" at `src/validator/verification.rs:314`). Only the zero/empty
uncle root is accepted — no actual uncle blocks are validated or rewarded.

**In the linear blockchain**, uncle structures and verification are implemented
in `src/linear/src/block.rs`. The `runtime` integration (WASM contract execution
during block validation) is partially complete — marked TODO at
`src/linear/src/blockchain.rs:192-193`.

All new feature development (contract testing, merge mining, p2pool adaptor,
anchoring finality) targets the linear blockchain. The fork-based validator is
kept for compatibility with existing `testnet` deployments.

## Glossary

| Name                   | Description                                                                            |
|------------------------|----------------------------------------------------------------------------------------|
| Consensus              | Algorithm for reaching blockchain consensus between participating nodes                |
| Node/Validator         | DarkWow daemon participating in the network                                             |
| Lilith Handshake       | Base P2P networking layer — every computer must handshake to participate               |
| Miner                  | Block producer                                                                         |
| Uncle Block            | Block that was mined but not canonical, but referenced by a canonical block            |
| Pin                    | Use-it-or-lose-it reward offer from canonical to uncle chain                            |
| Uncle Merkle           | Merkle tree of uncle blocks referenced by a canonical block                             |
| Block proposal         | Block that has not yet been appended onto the canonical blockchain                     |
| P2P network           | Peer-to-peer network on which nodes communicate with each other                          |
| Confirmation           | State achieved when a block and its contents are appended to the canonical blockchain  |
| Anchor                 | Monero block reference providing finality for a DarkWow block                          |
| Anchoring Finality     | Modular security overlay — finalized blocks cannot be reorganized                      |

See [Uncle Merkle Consensus](uncle_merkle.md) for detailed specification.
See [Mining Tokenomics](../mining-tokenomics.md#anchoring-finality-gadget) for the anchoring finality gadget specification.

The original fork/overlay DAG consensus specification has been archived to [legacy/consensus_dag.md](legacy/consensus_dag.md).
