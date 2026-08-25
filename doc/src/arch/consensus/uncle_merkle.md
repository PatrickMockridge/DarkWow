# Uncle Merkle Consensus

Uncle Merkle consensus replaces upstream's overlay/diff architecture with a Pareto efficient mechanism: the canonical chain is **obligated** to offer competing uncle chains a one-time option to form a side chain and share the PoW reward. The uncle chain has a short time window (minutes) to accept or reject. This achieves the significant benefit of upstream's fork-handling — miners aren't punished for producing blocks that don't become canonical — without the rewind and sled overlay logic.

## Motivation

The upstream DarkWow consensus uses a complex overlay/diff system for speculative block verification. This complexity exists to support the DAO governance model: a mechanism must adjudicate between competing forks to prevent chain splits from undermining token-holder voting. This creates a cascade of engineering problems:

1. **Non-deterministic in time**: State can be speculative, committed, or rolled back — same code, different results depending on timing
2. **Complex state management**: Overlays, checkpoints, and diffs all need careful coordination across the validator stack
3. **Mining risk**: Losing forks earn zero reward, making mining an all-or-nothing gamble
4. **Testing fragility**: Speculative state makes deterministic unit testing effectively impossible

On this fork, there is no DAO governance that needs to keep everything under one tent. Chain splits are handled the Bitcoin way: miners follow the most-work chain. If a contentious hard fork occurs, both sides coexist. This makes the engineering drastically simpler — and the Uncle Merkle mechanism ensures that even competing miners aren't wasting their work.

The Uncle Merkle design replaces the overlay/diff system with a simple merkle-tree-based mechanism that is:
- Statelessly verifiable (pure math, no overlay state)
- Pareto efficient (no wasted mining work)
- Deterministic (same block = same result every time)

## Core Concept

```
Canonical Block N
├── transactions[]
├── uncle_merkle_root ──────→ MerkleTree
│                              ├── Uncle 0 (depth 1 → 50% reward)
│                              └── Uncle 1 (depth 2 → 25% reward)
└── reward_distribution = [canonical_miner: 50%, uncle_0: 50%, uncle_1: 25%]
```

Key insight: Uncle chains are **explicitly referenced** in the canonical block rather than implicitly competing. This makes consensus **additive** (canonical + uncles) rather than **selective** (one fork wins, others discarded).

## Data Structures

### UncleBlock

```rust
pub struct UncleBlock {
    /// Header of the uncle block (contains PoW from RandomX)
    pub header: BlockHeader,
    /// Transactions in the uncle block
    pub transactions: Vec<Transaction>,
    /// Depth in the uncle tree (1 = directly referenced, 2 = referenced by depth-1, etc.)
    pub depth: u8,
    /// Pin offered by canonical chain (obligated offer if uncle meets criteria)
    pub pin_offered: bool,
    /// Uncle chain accepted the pin (use it or lose it - one time decision)
    pub pin_accepted: bool,
    /// Pin reward amount if accepted (computed from depth: 50% at d1, 25% at d2...)
    pub pin_reward: u64,
}
```

### UncleProof

For stateless verification, we send merkle proofs with **bound RandomX PoW**:

```rust
pub struct UncleProof {
    /// Uncle header (includes randomx_key for PoW verification)
    pub header: BlockHeader,
    /// RandomX PoW hash computed from header using header.randomx_key
    /// This is the critical security binding - must match re-computed hash
    pub pow_hash: [u8; 32],
    /// Merkle proof path from uncle to root
    pub merkle_path: Vec<[u8; 32]>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
    /// Depth (for reward calculation)
    pub depth: u8,
}
```

**Security invariant**: The `pow_hash` field must equal the RandomX hash computed from `header` using `header.randomx_key`. This prevents fake uncle proofs without actual RandomX work.

### BlockHeader Extension

```rust
pub struct BlockHeader {
    // ... existing fields ...
    /// Merkle root of uncle blocks referenced by this canonical block
    pub uncle_merkle_root: [u8; 32],
    /// Total reward being distributed (canonical + uncle shares)
    pub total_reward: u64,
    /// RandomX key for PoW mining (key used to create VM for this block)
    pub randomx_key: [u8; 32],
}
```

## RandomX PoW in Uncle Verification

Each uncle block's header contains a valid RandomX proof-of-work. The `UncleProof` structure binds this PoW into the proof itself via the `pow_hash` field.

### Verification Process

When verifying an `UncleProof`:

1. **Re-compute PoW hash**: Using the uncle's `header.randomx_key`, compute the RandomX hash of the header bytes. This must equal `pow_hash`.

2. **Check difficulty**: Verify the PoW hash meets the difficulty target.

3. **Verify merkle inclusion**: Verify the header is included in the uncle merkle tree rooted at `uncle_merkle_root`.

```rust
pub fn verify_uncle_proof(
    uncle: &UncleProof,
    merkle_root: &[u8; 32],
    difficulty_target: u32,
) -> bool {
    // Step 1: Re-compute RandomX PoW from header using header.randomx_key
    let flags = randomx::RandomXFlags::get_recommended_flags();
    let cache = randomx::RandomXCache::new(flags, &uncle.header.randomx_key)?;
    let verify_vm = randomx::RandomXVM::new(flags, Some(cache), None)?;
    let header_bytes = serde_json::to_vec(&uncle.header)?;
    let rx_hash = verify_vm.calculate_hash(&header_bytes)?;
    let computed_pow_hash: [u8; 32] = rx_hash[..32].try_into().unwrap();

    // pow_hash must match re-computed hash (binds PoW to proof)
    if computed_pow_hash != uncle.pow_hash {
        return false;
    }

    // Step 2: Difficulty check
    let hash_u32 = u32::from_le_bytes(computed_pow_hash[0..4].try_into().unwrap());
    if hash_u32 > difficulty_target {
        return false;
    }

    // Step 3: Merkle proof verification
    // ... verify header is in merkle tree at position ...
}
```

## Reward Distribution

### Formula

Uncle reward at depth `d` is: `base_reward / 2^d`

| Depth | Reward % |
|-------|----------|
| 1 | 50% |
| 2 | 25% |
| 3 | 12.5% |
| 4 | 6.25% |
| 5 | 3.125% |
| 6 | 1.5625% |

Maximum depth is 6 (similar to Ethereum's uncle mechanism).

### Reward Calculation

```rust
fn compute_reward_distribution(uncle_count: usize, base_reward: u64) -> (u64, Vec<u64>) {
    let mut canonical_reward = base_reward;
    let mut uncle_rewards = Vec::new();

    for i in 0..uncle_count {
        let depth = (i / 2) + 1; // Group by depth levels
        let reward = base_reward / (2_u64.pow(depth as u32));
        if i % 2 == 0 {
            canonical_reward += reward;
        } else {
            uncle_rewards.push(reward);
        }
    }

    (canonical_reward, uncle_rewards)
}
```

### Implementation

```rust
pub struct RewardDistribution {
    /// Share for canonical miner
    pub canonical: u64,
    /// Shares for uncle miners, ordered by merkle position
    pub uncles: Vec<UncleShare>,
}

pub struct UncleShare {
    pub hash: HeaderHash,
    pub depth: u8,
    pub reward: u64,
}
```

## Coinbase Split via Pedersen Mass Balance

When a canonical block includes uncles with accepted pins, the single coinbase
is atomically split at the consensus level. The canonical miner receives the
effective reward: `canonical_reward = base_reward - sum(pin_rewards)`. No new
ZK proofs are required — the split is pure Pedersen commitment arithmetic.

### Formal Specification

#### Definitions

Let the canonical block at height `H` have a coinbase transaction with ZK proof
producing a Pedersen value commitment:

```
C_base = v * G_v + r * G_r
```

where:
- `v = base_reward = expected_reward(H)` — the emission schedule value
- `G_v, G_r` — independent Pedersen generators (NUMS, nothing-up-my-sleeve)
- `r` — blinding factor from the ZK proof (witness, not publicly known)

Let `U = {u_1, ..., u_n}` be the set of accepted uncles in this block.
Each uncle `i` has:
- `pin_reward_i = v / 2^depth_i` — the depth-adjusted pin reward
- `uncle_hash_i` — the uncle's block header hash

#### Uncle Commitment Creation

For each accepted uncle `i`, a new commitment is created at the consensus level
with a deterministic Pedersen commitment:

```
C_uncle_i = u_i * G_v + r_i * G_r

where:
    u_i = pin_reward_i
    r_i = blake3s(uncle_hash_i || u_i.to_le_bytes() || H.to_le_bytes()) mod p
```

The blinding factor `r_i` is purely deterministic — no randomness, no ZK proof.
Any node can independently compute `r_i` from the uncle hash, pin reward, and
block height, and verify the commitment.

#### Pedersen Mass Balance Proof

The subtractive split is the equation:

```
C_effective = C_base - Σ_{i=1}^{n} C_uncle_i
```

Expanding by Pedersen homomorphism:

```
C_effective = (v - Σ u_i) * G_v + (r - Σ r_i) * G_r
```

**Mass balance:** The sum of commitments after the split equals the original:

```
C_effective + Σ C_uncle_i
    = [(v - Σ u_i) * G_v + (r - Σ r_i) * G_r] + Σ [u_i * G_v + r_i * G_r]
    = [v - Σ u_i + Σ u_i] * G_v + [r - Σ r_i + Σ r_i] * G_r
    = v * G_v + r * G_r
    = C_base                                                                  ∎
```

This is the fundamental invariant: **the split neither creates nor destroys value.**
It holds unconditionally — no trusted setup, no ZK proof, no cryptographic assumption
beyond the discrete log hardness that makes Pedersen commitments binding.

#### Supply Invariant

The value invariant follows directly from the mass balance:

```
v_effective + Σ u_i = v
    where v_effective = v - Σ u_i
```

Since `v = base_reward = expected_reward(H)`, the total value minted in this block
is exactly the emission schedule amount. No over-minting is possible.

#### Block-Level Mass Balance

The `proof_of_token_balance` module verifies per-block mass balance across all
transactions using the equation:

```
Σ C_outputs + Σ C_burns + Σ C_fees = Σ C_inputs
```

For the coinbase split, this extends to:

```
C_base = C_effective + Σ C_uncle_i
```

Uncle reward commitments `C_uncle_i` are included in `C_outputs`. The canonical miner's
`C_effective` is the commitment they actually control. The ZK proof verified `C_base`
was correctly minted; the consensus split verifies it was correctly distributed.

### Properties Summary

| Property | Formula | How verified |
|----------|---------|-------------|
| **Mass balance** | `C_effective + Σ C_uncle_i = C_base` | Additive homomorphism (always holds) |
| **Supply invariant** | `v_effective + Σ u_i = v` | Checked in `connect_block` before commit |
| **No over-minting** | `v_effective + Σ u_i = expected_reward(H)` | Same as supply invariant |
| **Determinism** | `r_i = blake3s(uncle_hash \|\| u_i \|\| H)` | Same input → same output |
| **Pedersen binding** | Cannot open `C_uncle_i` to any `u_i' ≠ u_i` | Discrete log hardness |

### Consensus Enforcement

The supply invariant is verified in `connect_block` (`src/linear/src/chain_state.rs`)
before the atomic sled transaction:

```rust
let canonical_value = block.header.total_reward;
let total_pin: u64 = uncles.iter()
    .filter(|u| u.pin_accepted)
    .map(|u| u.pin_reward)
    .sum();
if canonical_value + total_pin != expected_reward(height) {
    return Err(LinearError::BlockIsInvalid(
        "Supply invariant violated: canonical + uncles != base_reward"
    ));
}
```

### Uncle Commitments and Maturity

Uncle reward commitments are inserted into the `commitment_set` with the canonical block's height,
identical to how canonical coinbase commitments are tracked. This means:

- `COINBASE_MATURITY` (100 blocks) applies uniformly to both canonical and uncle commitments
- `is_coin_mature()` works identically for both
- Uncle commitments cannot be spent before maturity

### Audit Compatibility

The Pedersen cumulative supply audit (`verify_cumulative_supply()`) walks the chain
recomputing `S_H = S_{H-1} + C_base` from each block's coinbase commitment. The
subtractive split is auditable because `r_i` is deterministic — an auditor can
recompute every `C_uncle_i` and verify `C_effective + sum(C_uncle_i) == C_base` for
every block independently. The audit does not verify ZK proofs; it verifies Pedersen
binding.

## Verification (Stateless)

Block verification becomes purely a function of merkle proofs and math:

```rust
pub async fn verify_canonical_block(
    block: &BlockInfo,
    uncle_proofs: &[UncleProof],
    base_reward: u64,
) -> Result<()> {
    // 1. Verify PoW (existing logic)
    verify_pow(&block.header)?;

    // 2. Verify uncle merkle proof and each uncle's RandomX PoW
    for proof in uncle_proofs {
        // Verify the pow_hash is valid and meets difficulty
        verify_uncle_proof(proof, block.header.uncle_merkle_root, block.header.difficulty_target)?;
    }

    // 3. Verify reward distribution
    let expected_total = base_reward +
        uncle_proofs.iter()
            .map(|p| base_reward / (2_u64.pow(p.depth as u32)))
            .sum::<u64>();

    if block.header.total_reward != expected_total {
        return Err(Error::InvalidRewardDistribution);
    }

    // 4. Verify each uncle's age and depth
    for proof in uncle_proofs {
        if proof.depth > MAX_UNCLE_DEPTH {
            return Err(Error::UncleTooDeep);
        }
        // Uncle must be recent enough (within N blocks)
    }

    Ok(())
}
```

## Uncle Generation

Miners can create uncle blocks when they discover their block was not canonical:

```rust
fn create_uncle(block: Block, depth: u8, base_reward: u64) -> UncleBlock {
    UncleBlock {
        header: block.header,
        transactions: block.transactions,
        depth: depth.min(MAX_UNCLE_DEPTH),
        pin_offered: true,
        pin_accepted: false,
        pin_reward: base_reward / (2_u64.pow(depth as u32)),
    }
}

fn build_uncle_merkle(uncles: &[UncleBlock], _vm: &RandomXVM) -> ([u8; 32], Vec<UncleProof>) {
    // 1. Compute pow_hash for each uncle using their randomx_key
    let pow_hashes: Vec<[u8; 32]> = uncles.iter().map(|u| {
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &u.header.randomx_key)?;
        let uncle_vm = randomx::RandomXVM::new(flags, Some(cache), None)?;
        let hash_bytes = uncle_vm.calculate_hash(&serde_json::to_vec(&u.header)?)?;
        let mut pow_hash = [0u8; 32];
        pow_hash.copy_from_slice(&hash_bytes[..32]);
        Ok(pow_hash)
    }).collect::<Result<_>>()?;

    // 2. Build merkle tree of uncle hashes (uses blake3 for structure)
    let mut leaves: Vec<blake3::Hash> = uncles.iter()
        .map(|u| blake3::hash(&serde_json::to_vec(&u.header).unwrap()))
        .collect();
    // ... build merkle root ...

    // 3. Create proofs with pow_hash bound
    let proofs = uncle_proofs.iter().enumerate().map(|(i, u)| {
        UncleProof {
            header: u.header.clone(),
            pow_hash: pow_hashes[i],
            merkle_path: get_merkle_proof(leaves, i),
            position: i as u32,
            depth: u.depth,
        }
    }).collect();

    (root, proofs)
}
```

## Uncle Transactions and Block Construction

### Transaction-First Model

Uncle blocks participate in block execution the same way canonical transactions do:
each contract call (canonical and uncle) executes with its own `TxBackend` — a
minimal per-transaction state backend with an independent `SledTreeOverlay` clone.
This means uncle transactions have the same state isolation guarantees as canonical
transactions.

### Deterministic Merge

After execution, results are merged deterministically:

1. All results sorted by transaction hash bytes
2. Canonical diffs applied first (they "win" on key conflicts)
3. Uncle diffs subtract the canonical total before merging — conflicting keys
   retain canonical values
4. Single `sled::Batch` atomic commit

This naturally fits the uncle-merkle structure: uncle blocks are alternative merkle
trees of transactions, and the tx merkle tree cascades through blocks regardless of
whether the transactions are canonical or uncle.

## Comparison with Original Design

| Aspect | Original (Fork/Overlay) | Uncle Merkle |
|--------|--------------------------|--------------|
| Fork resolution | Implicit competition | Explicit reference |
| State management | Overlay + diffs + rollback | Merkle tree, stateless |
| Mining risk | All-or-nothing | Bounded (uncle gets partial) |
| Verification | Heavy WASM + sled lookup | Merkle proof + RandomX PoW |
| Complexity | High (checkpoint, diff, apply) | Low (merkle math) |
| Determinism | Non-deterministic in time | Fully deterministic |
| DAG structure | No (single chain focus) | Yes (multiple paths) |
| Testability | Hard (speculative state) | Easy (pure function) |

## Security Considerations

1. **Uncle depth limit**: Prevents infinite uncle chains (depth ≤ 6)
2. **RandomX PoW binding**: Each `UncleProof.pow_hash` must match re-computed hash using `header.randomx_key` - prevents fake proofs
3. **Difficulty check**: Each uncle's PoW must meet the canonical block's difficulty target
4. **Merkle proof validation**: Ensures uncle is actually in the uncle merkle tree
5. **Reward math validation**: Ensures total distribution matches expected

## Comparison with Ethereum

| Aspect | Ethereum Uncle | Linear Uncle Merkle |
|--------|---------------|---------------------|
| Hash function | Ethash (memory-hard) | RandomX |
| Uncle structure | Ommer (header only) | UncleBlock (header + txs) |
| PoW verification | On canonical block only | On each uncle (bound in proof) |
| Pin mechanism | None | Optional pin offers with one-time accept/reject |
| Reward formula | Same depth = same reward | Depth-based with pin multiplier |

## Testnet Verification

The uncle-merkle consensus was verified with a 5-node native mining dockernet
(`test_pipeline.sh --mode native --nodes 5`). All 5 nodes mined at full RandomX
capacity, the P2P mesh held, blocks propagated between all nodes, competing blocks
were stored as uncles and included via `uncle_merkle_root`. The Python model's
predictions (70+ uncle blocks, 300+ competing blocks) were confirmed. The dockernet
ran 24 minutes, reached block heights 17-20, zero segfaults, before hitting
resource limits on a 24-thread/48GB machine.

For daily development, a 1-node (solo) or 2-node (native) profile is sufficient.
The 5-node profile is reserved for consensus protocol verification. See the
[Testing Overview](../../dev/testing/overview.md) for resource requirements.

## Implementation Status

The uncle-merkle consensus is fully implemented. The table below documents
the implementation status of each feature.

| Feature | Spec Section | Implementation Status |
|---------|-------------|----------------------|
| Uncle block creation (`create_uncle`) | §Uncle Generation | ✅ Implemented in `src/linear/src/block.rs` |
| Uncle merkle tree construction | §Uncle Merkle Tree Construction | ✅ Implemented in `block.rs::build_uncle_merkle()` |
| Uncle proof verification | §Verification (Stateless) | ✅ Implemented in `validation.rs::check_uncles()` |
| Pin reward computation (value-level) | §Reward Distribution | ✅ Implemented in `block.rs::compute_reward()` using u64 arithmetic |
| Value-level uncle split invariant | §Coinbase Split — Supply Invariant | ✅ Implemented in `chain_state.rs::connect_block()` |
| Pedersen commitment-level uncle split | §Coinbase Split — Mass Balance Proof | ✅ Implemented in `chain_state.rs:736-760`. Uncle commitment commitments `C_uncle_i = pedersen_commitment_u64(u_i, Blind(r_i))` with deterministic blinds `r_i = blake3(uncle_hash)` mapped to scalar via `from_uniform_bytes`. Supply invariant `canonical + sum(pin_rewards) == expected_reward(height)` verified at lines 722-734. |
| Uncle commitment maturity tracking | §Uncle Commitments and Maturity | ✅ Uncle commitments tracked in `uncle_commitment_set` with creation height; COINBASE_MATURITY applies uniformly |
| Uncle commitment set restoration on restart | — | ✅ Implemented in `chain_state.rs::CChainState::new()` (Phase 3 H-H4 fix) |

## References

- Ethereum Uncle Mechanism: https://ethereum.org/en/developers/docs/consensus-mechanisms/pow/mining/
- RandomX: Memory-hard proof-of-work for CPU mining
- The design here is inspired by Ethereum's uncle concept but with RandomX PoW binding and deterministic reward distribution baked into the canonical block structure itself.