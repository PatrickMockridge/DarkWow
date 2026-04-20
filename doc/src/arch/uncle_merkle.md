# Uncle Merkle Consensus

This document describes the Uncle Merkle consensus mechanism, a simplification of DarkFi's original fork/overlay design that achieves Pareto efficiency and deterministic execution.

## Motivation

The original DarkFi consensus used a complex overlay/diff system for speculative block verification:
- Blocks were verified against an in-memory overlay
- Diffs tracked state changes for potential rollback
- Fork competition was implicit - losing forks meant wasted work

This had several problems:
1. **Non-deterministic in time**: State could be speculative, committed, or rolled back
2. **Complex state management**: Overlays, checkpoints, diffs all needed careful coordination
3. **Mining risk**: Blocks on losing forks earned zero reward, making mining risky
4. **Hard to test**: The speculative nature made testing difficult

The Uncle Merkle design replaces this with a simple merkle-tree-based system that is:
- Statelessly verifiable
- Pareto efficient (no wasted work)
- Deterministic (same block = same result)
- DAG-friendly without breaking consensus

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
    /// Header of the uncle block
    pub header: Header,
    /// Transactions in the uncle block
    pub txs: Vec<Transaction>,
    /// Depth in the uncle tree (1 = directly referenced, 2 = referenced by depth-1, etc.)
    pub depth: u8,
    /// Hash of this uncle block
    pub hash: HeaderHash,
}
```

### UncleProof

For stateless verification, we send merkle proofs:

```rust
pub struct UncleProof {
    /// Uncle header
    pub header: Header,
    /// Merkle proof path from uncle to root
    pub merkle_path: Vec<[u8; 32]>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
    /// Depth (for reward calculation)
    pub depth: u8,
}
```

### Header Extension

```rust
struct Header {
    // ... existing fields ...

    /// Merkle root of uncle blocks referenced by this canonical block
    pub uncle_merkle_root: [u8; 32],

    /// Total reward being distributed (canonical + uncle shares)
    pub total_reward: u64,
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

    // 2. Verify uncle merkle proof
    let computed_root = build_merkle_root(uncle_proofs)?;
    if computed_root != block.header.uncle_merkle_root {
        return Err(Error::InvalidUncleMerkleRoot);
    }

    // 3. Verify reward distribution
    let expected_total = base_reward +
        uncle_proofs.iter()
            .map(|p| base_reward / (2_u64.pow(p.depth as u32)))
            .sum::<u64>();

    if block.header.total_reward != expected_total {
        return Err(Error::InvalidRewardDistribution);
    }

    // 4. Verify each uncle's PoW and age
    for proof in uncle_proofs {
        if proof.depth > MAX_UNCLE_DEPTH {
            return Err(Error::UncleTooDeep);
        }
        // Uncle must be recent enough (within N blocks)
        verify_pow(&proof.header)?;
    }

    Ok(())
}
```

## Uncle Generation

Miners can create uncle blocks when they discover their block was not canonical:

```rust
fn create_uncle(header: Header, txs: Vec<Transaction>, depth: u8) -> UncleBlock {
    UncleBlock {
        header,
        txs,
        depth,
        hash: header.hash(),
    }
}

fn build_uncle_merkle(uncles: &[UncleBlock]) -> ([u8; 32], Vec<UncleProof>) {
    // Build merkle tree of uncle hashes
    let mut leaves: Vec<[u8; 32]> = uncles.iter().map(|u| u.hash.inner()).collect();
    let root = build_merkle_root_from_leaves(&mut leaves);

    // Create proofs for each uncle
    let proofs = uncles.iter().enumerate().map(|(i, u)| {
        UncleProof {
            header: u.header.clone(),
            merkle_path: get_merkle_proof(leaves, i),
            position: i as u32,
            depth: u.depth,
        }
    }).collect();

    (root, proofs)
}
```

## Comparison with Original Design

| Aspect | Original (Fork/Overlay) | Uncle Merkle |
|--------|--------------------------|--------------|
| Fork resolution | Implicit competition | Explicit reference |
| State management | Overlay + diffs + rollback | Merkle tree, stateless |
| Mining risk | All-or-nothing | Bounded (uncle gets partial) |
| Verification | Heavy WASM + sled lookup | Merkle proof only |
| Complexity | High (checkpoint, diff, apply) | Low (merkle math) |
| Determinism | Non-deterministic in time | Fully deterministic |
| DAG structure | No (single chain focus) | Yes (multiple paths) |
| Testability | Hard (speculative state) | Easy (pure function) |

## Security Considerations

1. **Uncle depth limit**: Prevents infinite uncle chains (depth ≤ 6)
2. **Recent uncle requirement**: Uncles must be within N blocks to prevent long-range attacks
3. **PoW verification**: Each uncle must still satisfy PoW requirements
4. **Merkle proof validation**: Ensures uncle is actually in the merkle tree
5. **Reward math validation**: Ensures total distribution matches expected

## Implementation Phases

### Phase 1: Documentation (this document)
- Define data structures
- Define verification logic
- Document reward formula

### Phase 2: Code Implementation
- Add UncleBlock, UncleProof structures
- Extend Header with uncle_merkle_root
- Implement stateless verification
- Modify sync module to use uncle verification

### Phase 3: Testing
- Build 5-node local testnet
- Verify reward distribution
- Verify deterministic verification

## References

- Ethereum Uncle Mechanism: https://ethereum.org/en/developers/docs/consensus-mechanisms/pow/mining/
- The design here is inspired by Ethereum's uncle concept but with deterministic reward distribution baked into the canonical block structure itself.