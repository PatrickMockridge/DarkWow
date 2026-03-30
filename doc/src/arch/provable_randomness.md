# Provable Randomness in DarkFi Contracts

This document analyzes randomness generation and usage in DarkFi smart contracts, focusing on the DarkToshi Dice contract as a case study, and explores how DarkFi's proof-of-work mechanism can be leveraged for trustworthy randomness.

## Table of Contents

1. [The Randomness Problem](#the-randomness-problem)
2. [DarkFi's Randomness Sources](#darkfis-randomness-sources)
3. [Current DarkToshi Dice Implementation](#current-darktoshi-dice-implementation)
4. [Leveraging Proof-of-Work for Randomness](#leveraging-proof-of-work-for-randomness)
5. [VRF-Based Randomness](#vrf-based-randomness)
6. [Security Analysis](#security-analysis)
7. [Case Study: Block Height Prediction Market](#case-study-block-height-prediction-market)
8. [Case Study: Baccarat](#case-study-baccarat)
9. [Missing Opcodes and Future Work](#missing-opcodes-and-future-work)

---

## The Randomness Problem

In cryptographic systems, randomness is essential but problematic:

| Problem | Description | Impact |
|---------|-------------|--------|
| **Trusted Third Party** | Someone must generate the random value | Centralization, trust assumptions |
| **Fairness** | All participants should get the same random value | Cannot favor early or late players |
| **Unpredictability** | No one should know the outcome before it happens | Gambling applications |
| **Verifiability** | Anyone can verify the random value was generated correctly | Trustless systems |
| **Non-Manipulation** | The generator cannot choose the outcome | Fairness |

**The Blockchain Dilemma**: Blockchains aim to be deterministic, but randomness requires non-determinism. Finding a solution that is simultaneously trustless, fair, unpredictable, verifiable, and non-manipulable is an open research problem.

---

## DarkFi's Randomness Sources

DarkFi provides multiple sources of randomness, each with different properties:

### 1. Transaction Hash (`wasm::util::get_tx_hash()`)

```
tx_hash = Hash(transaction_data)
```

| Property | Value | Notes |
|----------|-------|-------|
| **Source** | Transaction content | Includes sender, receiver, amounts, etc. |
| **Availability** | At transaction time | Known to all full nodes |
| **Unpredictability** | Medium | If attacker controls tx content, they have some influence |
| **Verifiability** | High | Hash is deterministic given tx content |
| **Cost** | Free | Already computed for consensus |

### 2. Block Hash (PoW Output)

```
block_hash = RandomX(header_blob)
```

| Property | Value | Notes |
|----------|-------|-------|
| **Source** | RandomX PoW on block header | CPU-intensive, memory-hard |
| **Availability** | After block is mined | Requires waiting for block confirmation |
| **Unpredictability** | Very High | Depends on miner's nonce search, not controllable |
| **Verifiability** | High | Can verify hash < target |
| **Cost** | High | PoW is computationally expensive |

### 3. ECVRF (Verifiable Random Function)

```rust
proof = VrfProof::prove(secret_key, input)
vrf_output = proof.hash_output()
```

| Property | Value | Notes |
|----------|-------|-------|
| **Source** | Secret key + input | Deterministically derived |
| **Availability** | Immediate | Anyone can compute given proof |
| **Unpredictability** | High | Requires secret key to predict |
| **Verifiability** | Very High | Proof can be verified by anyone |
| **Cost** | Medium | EC scalar multiplication |

### 4. Future Block Hash (Commit-Reveal Pattern)

```
commit = Hash(secret, params)  # Revealed later
reveal_hash = Block.hash_at_future_height  # Wait for block
final_value = Hash(reveal_hash, commit)
```

| Property | Value | Notes |
|----------|-------|-------|
| **Source** | Future block + committed secret | Combines off-chain secret with on-chain PoW |
| **Availability** | After commit + block | Requires time delay |
| **Unpredictability** | Very High | Neither party alone controls outcome |
| **Verifiability** | High | Can verify commit was made before block |
| **Cost** | Free | Uses existing block hash |

---

## Current DarkToshi Dice Implementation

### Roll Calculation

```rust
// From src/contract/darktoshi_dice/src/model/mod.rs

pub fn calculate_roll(tx_hash_bytes: [u8; 32], bet_id: BetId, secret_nonce: pallas::Base) -> u8 {
    // Convert tx_hash bytes to field elements
    let a = u64::from_le_bytes(tx_hash_bytes[0..8].try_into().unwrap());
    let b = u64::from_le_bytes(tx_hash_bytes[8..16].try_into().unwrap());
    let c = u64::from_le_bytes(tx_hash_bytes[16..24].try_into().unwrap());
    let d = u64::from_le_bytes(tx_hash_bytes[24..32].try_into().unwrap());

    // Hash the block hash components
    let block_hash = poseidon_hash([
        pallas::Base::from(a),
        pallas::Base::from(b),
        pallas::Base::from(c),
        pallas::Base::from(d),
    ]);

    // Combine with bet_id and secret_nonce
    let roll_input = poseidon_hash([block_hash, bet_id, secret_nonce]);
    let bytes = roll_input.to_repr();

    // Extract first byte as roll value (0-99)
    ((bytes[0] as u64) % (ROLL_RANGE as u64)) as u8
}
```

### Current Flow

```
1. Commit Phase:
   - Player commits to bet (value, target, secret_nonce, blind)
   - Contract stores: bet_id = poseidon_hash(params...)
   - Value is locked via Money::Burn + spend_hook

2. Reveal Phase (when tx is included in block):
   - tx_hash is determined by block inclusion
   - roll = calculate_roll(tx_hash, bet_id, secret_nonce)
   - If roll < target: player wins

3. Settlement Phase:
   - Player claims winnings via ZK proof
   - Or house closes after timeout
```

### Security Analysis of Current Implementation

**Strengths**:
- Uses block inclusion (PoW-backed) for randomness
- Combines multiple sources (tx_hash, bet_id, secret_nonce)
- Secret nonce prevents pre-computation by miner
- Commit-reveal prevents player manipulation

**Weaknesses**:
- Relies on `wasm::util::get_tx_hash()` which may not be properly random
- Miner could have some influence if they also created the bet
- No formal proof of unpredictability
- tx_hash depends on transaction ordering in block

---

## Leveraging Proof-of-Work for Randomness

### DarkFi's PoW Mechanism

DarkFi uses **RandomX**, a CPU-intensive and memory-hard PoW algorithm:

```
block_hash = RandomX(header_blob)
where header_blob includes:
  - Previous block hash
  - Block height
  - Timestamp
  - Transaction merkle root
  - Nonce (mined until hash < target)
```

**Key Properties**:
1. **Non-Manipulation**: Finding a valid nonce requires iterative search; cannot target specific hash values
2. **Difficulty Adjustment**: Target recalculates every ~720 blocks to maintain ~120 second block time
3. **Verifiability**: Any node can verify hash < target using the block header

### Can We Use PoW Hash Directly?

```
YES, with caveats:

Pros:
- Truly unpredictable until block is mined
- Distributed generation (any miner can find)
- Verifiable by all nodes
- No single party can manipulate

Cons:
- Requires waiting for block confirmation (1-10 blocks for confidence)
- Miners have slight advantage (can withhold blocks if they don't like the outcome)
- Cannot be used for real-time applications
```

### Recommended Approach: PoW + Commit-Reveal

For maximum security, combine PoW with commit-reveal:

```
1. COMMIT:
   secret = random()
   commit = poseidon_hash(secret, bet_id)
   # Store commit on-chain

2. REVEAL (after block N is mined):
   # Use block N's hash as source of randomness
   block_hash = get_block_hash(N)

   # Combine: player cannot predict block hash
   #          miner cannot know secret
   roll = poseidon_hash(block_hash, commit, secret)
```

### Why This Is Secure

| Attacker | Difficulty |
|----------|-----------|
| **Player** | Cannot predict `block_hash`; must wait for block |
| **Miner** | Cannot know `secret`; would need to mine multiple blocks |
| **Collusion** | Would require player + miner + significant hash power |
| **Oracle** | Not involved; no trust in third party |

---

### Adjustable Confirmation Depth: Accumulating PoW Entropy

The security of PoW-based randomness improves with **depth** - the number of blocks confirmed after the bet. Each block adds independent PoW entropy to the randomness pool.

#### The Concept

```
Betting Transaction (Block N)
    │
    ├── Player commits: commit = poseidon_hash(secret, bet_id)
    │
    └── House acknowledges, betting period begins

Waiting Period (Player-Selected Depth: K blocks)
    │
    ├── Block N+1: PoW hash adds entropy
    ├── Block N+2: Another independent PoW sample
    ├── ...
    └── Block N+K: Final entropy addition

Resolution (Block N+K)
    │
    └── roll = poseidon_hash(
           block_hash(N),
           block_hash(N+1),
           ...
           block_hash(N+K),
           commit,
           secret
       )
```

#### Security Scaling with Depth

| Depth (K) | Probability of Manipulation | Security Level |
|-----------|---------------------------|---------------|
| 1 | ~33% (with 33% hash power) | Low |
| 2 | ~11% (with 33% hash power) | Medium |
| 3 | ~3.7% | Medium-High |
| 6 | ~0.14% | High |
| 10 | ~0.005% | Very High |

**Formula**: For an attacker with `p` fraction of hash power, probability of getting K consecutive blocks is `p^K`.

#### Time + PoW: Why Depth Matters

Time and PoW combine to create strong randomness:

1. **Time**: Block timestamps establish causal ordering
2. **PoW**: Each block's hash is unpredictable until mined
3. **Depth**: Accumulating multiple blocks makes manipulation exponentially harder

```
Single Block (K=1):
- Attacker with 33% hash power: 33% chance to manipulate

Six Blocks (K=6):
- Attacker needs 6 consecutive blocks: (0.33)^6 ≈ 0.14%
- Realistic attack cost: >$1M in electricity for typical hash rate

Ten Blocks (K=10):
- Attacker needs 10 consecutive blocks: (0.33)^10 ≈ 0.005%
- Requires >$10M in sustained mining
```

#### Player Choice vs House Agreement

The innovation here is **player-selected depth with house agreement**:

```
1. Player proposes depth K (higher = more secure)
2. House agrees (or negotiates minimum depth)
3. Both parties commit knowing:
   - Randomness improves with depth
   - Time-to-resolution increases with depth
4. Roll computed from cumulative hash:
   roll = H(block_hash_N || block_hash_N+1 || ... || block_hash_N+K || commit || secret)
```

#### Economic Model

| Depth | Wait Time (approx) | Security | Use Case |
|-------|------------------|----------|---------|
| 1-2 | 2-4 min | Low | Micro-bets, real-time gaming |
| 3-6 | 6-12 min | High | Standard bets |
| 10+ | 20+ min | Very High | High-stakes, institutional |

**Trade-off**: Higher depth = more security but longer wait. Players choose based on:
- Stake size (high stakes = wait longer)
- Risk tolerance (some players prefer speed)
- Economic opportunity cost

#### Implementation: Cumulative Hash Chain

```rust
/// Compute roll from cumulative block hashes
fn compute_cumulative_roll(
    start_height: u64,
    depth: u8,
    commit: pallas::Base,
    secret: pallas::Base,
) -> u8 {
    let mut combined_hash = pallas::Base::zero();

    // Accumulate PoW hashes from each block
    for i in 0..depth {
        let block_hash = get_block_hash(start_height + u64::from(i));
        combined_hash = poseidon_hash([combined_hash, block_hash]);
    }

    // Final roll combines cumulative entropy with commit/secret
    let roll_input = poseidon_hash([combined_hash, commit, secret]);
    let bytes = roll_input.to_repr();
    ((bytes[0] as u64) % (ROLL_RANGE as u64)) as u8
}
```

#### Why This Beats Single-Block Randomness

| Approach | Entropy Source | Manipulation Difficulty |
|----------|--------------|------------------------|
| Single tx_hash | 1 tx's content | Miner can influence ordering |
| Single block hash | 1 PoW sample | Miner can withhold block |
| **Cumulative (K blocks)** | K independent PoW samples | Must control K consecutive blocks |

**Key insight**: Even if a miner controls one block, they cannot control the cumulative hash of K blocks without controlling all K - which becomes exponentially harder with each additional block.

---

## VRF-Based Randomness

### ECVRF in DarkFi

DarkFi implements ECVRF (Elliptic Curve Verifiable Random Function) based on [draft-irtf-cfrg-vrf-04](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04):

```rust
// From src/sdk/src/crypto/ecvrf.rs

impl VrfProof {
    /// Generate VRF proof
    pub fn prove(secret_key: SecretKey, alpha_string: &[u8]) -> Self {
        let Y = PublicKey::from_secret(secret_key);
        let H = pallas::Point::hash_to_curve(VRF_DOMAIN)(&[Y.to_bytes(), alpha_string].concat());
        let gamma = H * fp_mod_fv(secret_key.inner());

        // Generate proof components...
        // Returns: VrfProof { gamma, c, s }
    }

    /// Verify VRF proof
    pub fn verify(&self, public_key: PublicKey, alpha_string: &[u8]) -> bool {
        // Verify the proof is valid
        // Returns true if verified
    }

    /// Get VRF output (call AFTER verify!)
    pub fn hash_output(&self) -> blake3::Hash {
        // Returns domain-separated hash of gamma
    }
}
```

### VRF vs PoW for Randomness

| Criterion | PoW Block Hash | ECVRF |
|-----------|----------------|-------|
| **Unpredictability** | Very High | High |
| **Verifiability** | High | Very High |
| **Decentralization** | High (miners) | Depends on key holder |
| **Cost** | High (energy) | Low (computation) |
| **Latency** | ~2 min (block time) | Immediate |
| **Key Management** | None | Required |

### Hybrid Approach: PoW + VRF

For maximum security, combine both:

```
Randomness_Input = poseidon_hash(
    block_hash,      # PoW output (unpredictable)
    vrf_output,      # VRF from oracle (verifiable)
    user_commit      # User's committed value
)
```

**This gives**:
- PoW's unpredictability (no one can predict block hash)
- VRF's verifiability (oracle cannot manipulate)
- User's contribution (no collusion without user)

---

## Security Analysis

### Attack Vectors

#### 1. Miner Manipulation

**Attack**: Miner who also plays dice, mines a block, and withholds if they don't like the hash.

**Severity**: Medium

**Mitigation**:
- Use block hash from 1-2 blocks ago (miner's block already finalized)
- Combine with commit-reveal (secret unknown to miner)
- Require multiple blocks for confirmation

**Analysis**:
```
If miner controls 33% of hash rate:
- Probability of getting 2 consecutive blocks: 0.33^2 = 11%
- Not economically viable for typical bet sizes
- Worthwhile for large bets (> 10x block reward)
```

#### 2. Player Pre-Computation

**Attack**: Player computes roll before committing to get favorable outcome.

**Severity**: Low (solved by commit-reveal)

**Mitigation**:
- Commit phase binds player to bet_id before knowing roll
- Roll uses tx_hash determined at block inclusion
- Player cannot influence tx_hash

#### 3. Oracle Manipulation

**Attack**: Oracle provides false randomness.

**Severity**: High (for VRF-only solutions)

**Mitigation**:
- Use PoW hash as primary source (cannot be faked)
- VRF only adds verifiable contribution
- Require multiple oracles for large values

#### 4. Transaction Ordering

**Attack**: Transaction ordering in block affects tx_hash.

**Severity**: Low (multi-source hashing mitigates)

**Mitigation**:
- Roll uses full tx_hash, not just position
- Even if tx is first/last in block, hash differs
- For high-stakes: use block hash from next block

### Randomness Quality Assessment

| Source | Entropy (bits) | Trust Model | Suitable for Gambling? |
|--------|---------------|-------------|------------------------|
| tx_hash | ~64 | Single tx | No (manipulable) |
| PoW block hash | ~256 | Distributed miners | Yes (with commit-reveal) |
| ECVRF | ~256 | Key holder | Conditional |
| PoW + VRF + Commit | ~256+ | Hybrid | Recommended |

---

## Case Study: Block Height Prediction Market

### Concept

A prediction market where participants bet on the **next canonical DarkFi block height** at a specific time. Unlike traditional prediction markets that resolve on subjective events, this market leverages DarkFi's PoW blockchain as a **trustless, verifiable randomness source**.

**Core Question**: "What will be the canonical block height at timestamp T?"

### Why This Is Interesting

| Property | Description |
|----------|-------------|
| **Natural Randomness** | Block arrival time (~120s target) is inherently probabilistic due to PoW mining |
| **Observable** | Current block height is public information |
| **Verifiable** | Block hashes provide cryptographic proof of work done |
| **Predictable Variance** | Block time follows an exponential distribution - predictable in aggregate but random individually |
| **Practical Use Cases** | Scheduled lotteries, time-locked reveals, smart contract randomness |

### DarkFi Consensus Deep Dive

Before designing the prediction market, we must understand how DarkFi determines the **canonical block**:

#### Fork Resolution Algorithm

DarkFi does **NOT** use simple longest-chain rule. Instead it uses a **rank-based system** (`src/validator/utils.rs`):

```rust
// Each block has a "rank" computed from two components:
// 1. target_distance_sq: Squared distance from mining target to MAX_32_BYTES
// 2. hash_distance_sq: Squared distance from RandomX output hash to MAX_32_BYTES

// MAX_32_BYTES = 2^256 - 1 (all 0xFF bytes)
// This is essentially the "closest to infinity" metric
```

#### Canonical Chain Selection

Forks are ranked by:
1. **Primary**: Sum of `targets_rank` across all blocks (higher is better)
2. **Tiebreaker**: Sum of `hashes_rank` across all blocks (higher is better)

```
Fork A: blocks [100, 101, 102] with ranks [R1, R2, R3]
Fork B: blocks [100, 101, 102'] with ranks [R1, R2, R3']

Winner = argmax(sum(R1+R2+R3), sum(R1+R2+R3'))
```

#### Critical: When Forks Are Equal

**When competing forks have equal `targets_rank` AND equal `hashes_rank`:**
- Confirmation is **BLOCKED** - no canonical chain switch occurs
- The network waits until one fork accumulates strictly more work
- This creates genuine uncertainty during fork races

#### RandomX PoW

DarkFi uses **RandomX** for PoW - an ASIC-resistant, CPU-friendly algorithm:
- Block hash = `RandomX_VM(hashing_blob)`
- Hashing blob = serialized block header (prev_hash, height, timestamp, txs_merkle_root, nonce)
- The VM execution is opaque - even knowing all inputs, predicting output is infeasible

#### Difficulty Calculation

```rust
// Sliding window of 720 blocks (600 used after trimming outliers)
// target = MAX_32_BYTES / difficulty
// next_difficulty = (cumulative_work * target + time_span - 1) / time_span
```

### Contract Design

The Block Height Prediction Market extends the existing `prediction_market` contract with specialized resolution logic:

```rust
/// BlockHeightMarket extends base Market
struct BlockHeightMarket {
    // Base prediction market fields
    market_id: MarketId,
    creator: PublicKey,
    question: Vec<u8>,           // e.g., "Block height at 2026-04-01 12:00:00 UTC?"
    resolve_time: u64,           // Target timestamp for resolution
    betting_closes: u64,         // When betting stops

    // Block-height specific fields
    base_block_height: u64,      // Block height at market creation
    target_block: Option<u64>,   // Resolved block height (set by oracle)
    confirmation_depth: u8,      // PoW confirmation depth for resolution
    resolution_data: Vec<u8>,    // Oracle attestation or ZK proof
}

/// Position includes predicted height range
struct BlockHeightPosition {
    position_id: PositionId,
    market_id: MarketId,
    owner: PublicKey,
    outcome: u8,                  // 0 = BELOW, 1 = EXACT, 2 = ABOVE
    predicted_height: u64,        // Exact predicted height
    height_range: u8,            // +/- tolerance for "close" payouts
    amount: u64,
    potential_payout: u64,
    claimed: bool,
    created_at: u64,
}
```

### Resolution Mechanisms

#### Option 1: Oracle Attestation (Current)

```
1. At resolve_time, oracle observes canonical block height H
2. Oracle signs: "At time T, block height was H"
3. Oracle submits attestation to contract
4. Contract verifies signature, resolves market
```

**Limitation**: Requires trusted oracle (see [Oracle Contract](oracle.md))

#### Option 2: PoW-Backed Resolution (Recommended)

Uses DarkFi's PoW with adjustable confirmation depth. The key insight is that **block hashes are unpredictable** due to RandomX, making them suitable for randomness generation:

```rust
/// Resolve using cumulative PoW hash
/// Key property: RandomX output is unpredictable even to miners
fn resolve_with_pow(
    market: BlockHeightMarket,
    current_block: u64,
) -> Result<u64, Error> {
    // Wait until we have enough confirmations past resolve_time
    let confirmations_needed = market.confirmation_depth as u64;
    let min_resolve_block = get_block_at_time(market.resolve_time)?;
    let resolve_block = min_resolve_block + confirmations_needed;

    // CRITICAL: Use block hash from K blocks AFTER resolve_time
    // This ensures:
    // 1. Block at resolve_time is already mined (not withholding)
    // 2. K confirmation blocks add PoW entropy
    let resolution_hash = cumulative_pow_hash(
        resolve_block,
        confirmations_needed
    );

    // Derive resolved height from PoW entropy
    // The randomness comes from RandomX, NOT from block timing
    let resolved_height = derive_height_from_hash(
        resolution_hash,
        market.base_block_height,
        market.resolve_time
    );

    Ok(resolved_height)
}

/// Cumulative PoW hash for strong randomness
/// Security: K blocks = attacker needs K consecutive blocks
fn cumulative_pow_hash(start_block: u64, depth: u8) -> pallas::Base {
    let mut combined = pallas::Base::zero();
    for i in 0..depth {
        let block_hash = get_block_hash(start_block + u64::from(i))?;
        combined = poseidon_hash([combined, block_hash]);
    }
    combined
}

/// Derive height using PoW entropy
/// Key insight: RandomX hash is the randomness source, NOT block timing
fn derive_height_from_hash(
    pow_hash: pallas::Base,
    base_height: u64,
    target_time: u64,
) -> u64 {
    // Expected blocks since creation
    let elapsed = target_time - creation_time;
    let expected_blocks = elapsed / 120; // ~120 second block time

    // Use RandomX hash (not time) for variance
    // This is secure because RandomX output cannot be predicted
    let hash_bytes = pow_hash.to_repr();
    let variance: i64 = (hash_bytes[0] as i64) % 50 - 25; // +/- 25 blocks variance

    (expected_blocks as i64 + variance).max(0) as u64 + base_height
}
```

#### Option 3: Hybrid (PoW + Oracle Tiebreaker)

```
1. Primary: Use PoW hash from K blocks after resolve_time
2. If fork race with equal ranks: Oracle provides tiebreaker attestation
3. ZK proof verifies PoW computation in circuit
```

**Note**: Given DarkFi's fork resolution, during a **rank tie** the oracle's attestation determines which fork wins. This is the ONLY scenario where oracle trust is critical.

### Integration with Existing Prediction Market

Rather than creating a new contract, extend `prediction_market`:

```rust
// In prediction_market/src/model/mod.rs

/// Resolution type for markets
pub enum ResolutionType {
    OracleAttestation = 0,    // Traditional oracle signature
    PowDelayed = 1,           // PoW-backed with confirmation depth
    Hybrid = 2,               // PoW + oracle tiebreaker
}

/// BlockHeight-specific params for CreateMarket
pub struct BlockHeightMarketParams {
    pub base_resolution_type: ResolutionType,
    pub confirmation_depth: u8,        // For PowDelayed/Hybrid
    pub target_time: u64,              // Unix timestamp to resolve at
    pub tolerance_range: u8,           // For "close" payouts
}

/// Create a block height prediction market
pub fn create_block_height_market(
    params: BlockHeightMarketParams,
) -> Result<Market, PredictionMarketError> {
    // Validate confirmation depth
    if params.confirmation_depth < 1 || params.confirmation_depth > 10 {
        return Err(PredictionMarketError::InvalidConfirmationDepth)
    }

    // Validate target time is in future
    let current_time = wasm::util::get_block_timestamp()?;
    if params.target_time <= current_time {
        return Err(PredictionMarketError::InvalidResolveTime)
    }

    // Derive market ID
    let market_id = derive_market_id(...);

    Ok(Market {
        id: market_id,
        resolution_type: ResolutionType::PowDelayed,
        confirmation_depth: params.confirmation_depth,
        target_time: params.target_time,
        base_block_height: wasm::util::get_block_height()?,
        ..Default::default()
    })
}
```

### Payout Structure

```rust
/// Calculate payouts for block height prediction
fn calculate_height_payout(
    position: &BlockHeightPosition,
    resolved_height: u64,
    total_pool: u64,
    winning_pool: u64,
) -> u64 {
    let distance = abs_diff(position.predicted_height, resolved_height);

    // Jackpot: exact prediction
    if distance == 0 {
        return total_pool * position.amount / winning_pool * 3 // 3x bonus
    }

    // Close: within tolerance
    if distance <= position.height_range as u64 {
        return total_pool * position.amount / winning_pool * 2 // 2x bonus
    }

    // Standard: in correct direction
    let correct_direction = (resolved_height > position.predicted_height) ==
                           (position.outcome == ABOVE);
    if correct_direction {
        return total_pool * position.amount / winning_pool
    }

    0 // Lost
}
```

### Security Analysis

#### What Can and Cannot Be Predicted

Given DarkFi's consensus mechanism, here's what market participants should understand:

| Aspect | Predictable? | Why |
|--------|-------------|-----|
| **Difficulty** | ✅ Yes | Calculated from 720-block sliding window |
| **Target** | ✅ Yes | Derived directly from difficulty |
| **Next block hash** | ❌ No | RandomX VM output is opaque |
| **Canonical chain (normal)** | ✅ Yes | Higher accumulated rank wins |
| **Canonical chain (rank tie)** | ❌ No | Fork race outcome is genuinely uncertain |
| **Block timing** | ⚠️ Statistical | ~120s expected, exponential distribution |

**Critical insight for prediction markets**: During normal operation (no fork race), the canonical block at any given time is **deterministic** given the blockchain state. The **randomness** comes from:
1. **RandomX output** - unpredictable even to miners
2. **Fork race resolution** - only uncertain during rank ties

#### PoW-Based Resolution Security

| Attack | Difficulty | Mitigation |
|--------|------------|------------|
| Oracle manipulation | N/A | No oracle needed for normal operation |
| Miner manipulates block hash | p^K | Need K consecutive blocks |
| Fork race during resolution | Rank-based | Wait for confirmation depth |
| **Rank tie scenario** | **Unpredictable** | Oracle tiebreaker or K-more blocks |

#### DarkFi Fork Resolution Details

```
Normal fork resolution (ranks unequal):
1. Compare sum of targets_rank across fork
2. Higher sum = winner (deterministic)
3. No randomness involved

Tie scenario (ranks equal):
1. Compare sum of hashes_rank across fork
2. Higher sum = winner
3. If still tied → confirmation BLOCKED
4. Requires more work to resolve

Implication: Prediction markets should wait past resolve_time
by K blocks to avoid fork race uncertainty.
```

#### Confirmation Depth Security

```
K=1: 33% manipulation chance (1 block miner advantage)
K=6: ~0.14% (requires 6 consecutive blocks)
K=10: ~0.005% (institutional-grade security)
```

**Note on fork races**: The above analysis assumes we're past the resolution time. If a fork race is active at resolution time, the market should wait until:
1. One fork has strictly higher rank, OR
2. K additional blocks confirm one fork over the other

### Implementation Roadmap

1. **Phase 1: Oracle-Based (Quick)**
   - Extend prediction_market with BlockHeightMarketParams
   - Use existing oracle attestation for resolution
   - Limited to low-stakes (oracle trust required)

2. **Phase 2: PoW-Delayed (Medium)**
   - Add `cumulative_pow_hash` utility
   - Implement confirmation depth validation
   - Replace oracle with block hash resolution

3. **Phase 3: ZK-Verified (Production)**
   - Implement `BlockHashGet` opcode for ZK circuits
   - Verify PoW computation inside ZK proof
   - Remove oracle entirely

### Comparison to DarkToshi Dice

| Aspect | DarkToshi Dice | Block Height Market |
|--------|---------------|-------------------|
| **Randomness Source** | tx_hash (single block) | Cumulative PoW (K blocks) |
| **Resolution Time** | Immediate (next block) | Delayed (confirmation depth) |
| **Player Choice** | confirmation_depth (new!) | confirmation_depth |
| **Outcome** | Discrete (0-99) | Continuous (any height) |
| **Oracle Required** | No | Optional |

Both contracts now share the **adjustable confirmation depth** pattern for enhancing randomness security through cumulative PoW entropy.

---

## Implemented Opcodes for Randomness

### BlockHashGet - Now Available!

The `wasm::util::get_block_hash(block_height)` function is now implemented in the WASM runtime!

| Opcode | Description | Status | Enables |
|--------|-------------|--------|---------|
| `BlockHashGet(u64 height)` | Get block hash at height | ✅ Implemented | Direct PoW randomness access |
| `VRFVerify` | Verify ECVRF proof in circuit | ❌ Missing | VRF-based randomness in ZK |
| `TimeGet` | Get current block time in circuit | ❌ Missing | Time-based resolution |
| `TimestampCompare` | Compare timestamps in ZK | ❌ Missing | Conditional execution based on time |

### Usage

```rust
// Get PoW block hash for randomness
let block_hash = wasm::util::get_block_hash(block_height)?;

// Cumulative entropy from K blocks
for i in 0..depth {
    let hash = wasm::util::get_block_hash(start_height + u64::from(i))?;
    combined = poseidon_hash([combined, hash]);
}
```

### Recommended Implementation: VRF Opcode

```zk
# Ideal VRFVerify opcode for zkVM:
circuit "VRFVerify" {
    # Inputs:
    Base public_key_x,
    Base public_key_y,
    Base alpha,           # Input to VRF
    Base gamma_x,          # VRF proof component
    Base gamma_y,
    Base c,               # Challenge hash
    Base s,               # Response scalar

    # Constraints verify:
    # 1. public_key is valid curve point
    # 2. gamma = H(alpha) * secret_key
    # 3. c = Hash(gamma, H^1, H^2) matches

    # Output:
    # gamma (for hash_output extraction)
}
```

### Current Workaround

Since `VRFVerify` is not implemented, current contracts use:

1. **Oracle attestation**: Oracle signs the randomness, contract verifies signature
2. **Challenge-response**: User provides randomness with ZK proof it's correct
3. **PoW delay**: Wait for block confirmation, use block hash

---

## Recommendations

### For DarkToshi Dice

1. **Use PoW block hash from 1 block ago** instead of tx_hash:
   ```rust
   // Instead of tx_hash:
   let block_hash = wasm::util::get_block_hash(current_height - 1)?;
   ```

2. **Add explicit commit-reveal** for the secret nonce:
   - Player commits `commit = poseidon_hash(secret, bet_id)` at bet time
   - Reveal `secret` when claiming winnings
   - Roll = `poseidon_hash(block_hash, commit, secret)`

3. **Add player-selected confirmation depth** (the key enhancement!):
   - Player proposes depth K (e.g., 6 blocks for high-stakes)
   - House agrees to minimum depth based on stake size
   - Roll uses cumulative hash of K consecutive blocks
   - See [Adjustable Confirmation Depth](#adjustable-confirmation-depth-accumulating-pow-entropy)

4. **Implement cumulative hash for multi-block randomness**:
   ```rust
   fn compute_roll_with_depth(
       start_block: u64,
       depth: u8,
       commit: pallas::Base,
       secret: pallas::Base,
   ) -> u8 {
       let mut combined = pallas::Base::zero();
       for i in 0..depth {
           let hash = get_block_hash(start_block + u64::from(i));
           combined = poseidon_hash([combined, hash]);
       }
       let roll_input = poseidon_hash([combined, commit, secret]);
       let bytes = roll_input.to_repr();
       ((bytes[0] as u64) % 100) as u8
   }
   ```

### For General Randomness

1. **Never use tx_hash alone** for high-stakes randomness
2. **Always combine sources**: PoW + user contribution + optional VRF
3. **Consider commit-reveal** for predictability reduction
4. **Prefer multi-block cumulative hash** over single block:
   - K=1: Basic security (real-time use)
   - K=3-6: Standard security (most applications)
   - K=10+: High-stakes security (institutional level)

### For Block Height Prediction Market

1. Use oracle to observe and attest block height at target time
2. Allow betting on block height ranges, not exact values
3. Use cumulative PoW hash for tie-breaking
4. Implement as case study to test randomness integration

### The Time + PoW = Trustworthy Randomness Equation

```
Randomness_Security ≈ f(Time, PoW_Work, Depth)

Where:
- Time = Block timestamps establish causal ordering
- PoW_Work = Iterative search makes hash unpredictable
- Depth = Number of independent PoW samples accumulated

This is why Bitcoin's "6 confirmations" became standard:
- 6 blocks ≈ 12 minutes
- Combined PoW work is exponentially harder to reverse
- No single party can manipulate without revealing

---

## Case Study: Baccarat

Baccarat (Punto Banco) is a casino classic featured in James Bond films that demonstrates cumulative PoW entropy for **multi-card dealing** - a more complex randomness use case than single-value dice rolls.

### Why Baccarat for Blockchain Gambling?

| Property | Description | Why It Matters |
|----------|-------------|----------------|
| **No player decisions** | Drawing rules are completely fixed | No disputes possible |
| **Deterministic outcomes** | Rules determine everything | Casino-friendly |
| **3 outcomes** | Player/Banker/Tie | More complex than dice |
| **Multiple cards** | 2-4 cards per hand | Requires multiple random values |
| **Commit-reveal friendly** | Cards revealed after commitment | Perfect for blockchain |

### Card Dealing via Block Hash Entropy

Unlike dice (single roll value), Baccarat requires dealing 4 cards (2 to player, 2 to banker) with complex drawing rules. This is achieved using cumulative PoW entropy:

```rust
/// Deal cards using cumulative PoW block hash entropy
/// Returns ((player_card1, player_card2), (banker_card1, banker_card2))
fn deal_cards(block_hashes: &[TransactionHash], bet_id: BetId) -> (Hand, Hand) {
    // Combine entropy from K consecutive block hashes
    let mut entropy = bet_id;
    for (i, hash) in block_hashes.iter().enumerate() {
        // Convert 32-byte block hash to 4 x u64
        let hash_bytes = hash.0;
        let a = u64::from_le_bytes(hash_bytes[0..8]);
        let b = u64::from_le_bytes(hash_bytes[8..16]);
        let c = u64::from_le_bytes(hash_bytes[16..24]);
        let d = u64::from_le_bytes(hash_bytes[24..32]);

        // Poseidon hash of block entropy
        let block_entropy = poseidon_hash([
            pallas::Base::from(a),
            pallas::Base::from(b),
            pallas::Base::from(c),
            pallas::Base::from(d),
        ]);

        // Cumulative entropy
        entropy = poseidon_hash([entropy, block_entropy, pallas::Base::from(i)]);
    }

    // Derive 4 cards from final entropy
    let bytes = entropy.to_repr();
    let seed1 = u64::from_le_bytes(bytes[0..8]);
    let seed2 = u64::from_le_bytes(bytes[8..16]);
    let seed3 = u64::from_le_bytes(bytes[16..24]);
    let seed4 = u64::from_le_bytes(bytes[24..32]);

    let player_card1 = Card::new(seed1 as u8);
    let player_card2 = Card::new(seed2 as u8);
    let banker_card1 = Card::new(seed3 as u8);
    let banker_card2 = Card::new(seed4 as u8);

    (Hand { card1: player_card1, card2: player_card2, third_card: None },
     Hand { card1: banker_card1, card2: banker_card2, third_card: None })
}
```

### Why Cumulative Entropy Matters for Card Games

| Approach | Entropy Source | Problem for Card Games |
|----------|--------------|------------------------|
| Single block hash | 1 PoW sample | Only gives 1 random value |
| Single tx hash | 1 tx's content | Predictable by miner |
| **Cumulative (K blocks)** | K independent PoW samples | Can derive 4+ card values |

**Key insight**: A single 256-bit hash can be expanded into multiple card values by treating different byte ranges as separate seeds. This is secure as long as the original hash is unpredictable.

### Confirmation Depth: Player-Selected Security

Like DarkToshi Dice, Baccarat uses **player-selected confirmation depth** for the time+Pow security model:

```
Player bets on "Player" outcome
 │
 ├── Proposes confirmation_depth = 6 blocks
 │
 └── House accepts (or sets minimum)

Waiting Period:
 │
 ├── Block N+1: PoW entropy #1
 ├── Block N+2: PoW entropy #2
 ├── ...
 └── Block N+6: Final entropy

Resolution:
 │
 └── Cards derived from cumulative hash of blocks N+1 to N+6
```

**Economic tradeoff**:

| Depth | Wait Time | Security | Typical Use |
|-------|-----------|----------|-------------|
| 1-2 | 2-4 min | Low | Micro-bets, real-time |
| 3-6 | 6-12 min | High | Standard casino play |
| 10+ | 20+ min | Very High | High-stakes |

### Comparison: Dice vs Baccarat Randomness

| Aspect | DarkToshi Dice | Baccarat |
|--------|---------------|----------|
| **Randomness source** | Single tx_hash or block hash | Cumulative K blocks |
| **Output type** | 1 value (0-99) | 4 card values (0-51 each) |
| **Entropy expansion** | Direct use | Poseidon hash + seed expansion |
| **Drawing rules** | N/A | Complex (fixed rules) |
| **Confirmation depth** | Yes | Yes |
| **Time+Pow security** | Yes | Yes |

### Baccarat Contract Implementation

The Baccarat contract (`src/contract/baccarat/`) implements:

1. **CommitBetV1**: Player commits to bet (Player/Banker/Tie) with secret nonce
2. **DrawCardsV1**: Uses `wasm::util::get_block_hash(height)` for K-block cumulative entropy
3. **SettleBetV1**: Applies drawing rules, pays winners
4. **HouseCloseV1**: Timeout handling for abandoned bets

```rust
/// DrawCardsV1: Cards drawn using PoW entropy
fn baccarat_draw_cards_process_instruction_v1(...) {
    // Collect K block hashes for entropy
    let confirmation_depth = bet.confirmation_depth as usize;
    let mut block_hashes = vec![];

    for i in 0..confirmation_depth {
        let block_height = current_block.saturating_sub(i as u32);
        let block_hash = wasm::util::get_block_hash(block_height)?;
        block_hashes.push(block_hash);
    }

    // Deal cards using cumulative entropy
    let (mut player_hand, mut banker_hand) = deal_cards(&block_hashes, bet.id);

    // Calculate outcome using fixed Baccarat rules
    let game_outcome = calculate_outcome(&mut player_hand, &mut banker_hand);
    // ...
}
```

### Why Baccarat Is a Better Blockchain Game than Blackjack

| Aspect | Blackjack | Baccarat |
|--------|-----------|---------|
| **Player decisions** | Hit/stand/double/split | None (fixed rules) |
| **Rule complexity** | Complex strategy variations | Simple fixed rules |
| **Dispute potential** | Higher (decisions matter) | None (rules decide) |
| **Blockchain fit** | Poor (subjective) | Perfect (deterministic) |

### Security Properties

1. **Card unpredictability**: Cards derived from PoW entropy that neither player nor miner can predict
2. **No card counting defense needed**: Blockchain transparency prevents hidden information
3. **Cumulative entropy**: K consecutive blocks required for manipulation
4. **Commit-reveal**: Secret nonce committed before cards are known

### See Also

- [Baccarat Contract](baccarat.md) - Full contract documentation
- [DarkToshi Dice Contract](darktoshi_dice.md) - Simpler randomness example
- [Block Height Prediction Market](#case-study-block-height-prediction-market) - Time-based resolution

---

## See Also

- [DarkToshi Dice Contract](darktoshi_dice.md)
- [ECVRF Implementation](../../src/sdk/src/crypto/ecvrf.rs)
- [Consensus Mechanism](../../src/validator/consensus.rs) - Fork resolution and rank-based chain selection
- [PoW Module](../../src/validator/pow.rs) - RandomX implementation and difficulty calculation
- [Fork Ranking Utils](../../src/validator/utils.rs) - `block_rank()` and `best_fork_index()`
- [VRF Research](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04)
- [RandomX Paper](https://eprint.iacr.org/2018/1033)
