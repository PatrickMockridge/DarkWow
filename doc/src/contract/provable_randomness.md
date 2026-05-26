# Provable Randomness in DarkWow Contracts

This document analyzes randomness generation and usage in DarkWow smart contracts, focusing on the DarkToshi Dice contract as a case study, and explores how DarkWow's proof-of-work mechanism can be leveraged for trustworthy randomness.

## Table of Contents

1. [The Randomness Problem](#the-randomness-problem)
2. [DarkWow's Randomness Sources](#darkfis-randomness-sources)
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

## DarkWow's Randomness Sources

DarkWow provides multiple sources of randomness, each with different properties:

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

/// Deprecated: Use calculate_roll_with_depth for production gambling.
#[deprecated(since = "0.1.0", note = "Use calculate_roll_with_depth with adjustable confirmation depth")]
pub fn calculate_roll(tx_hash_bytes: [u8; 32], bet_id: BetId, secret_nonce: pallas::Base) -> u8 {
    // ... legacy single-block roll
}

/// Production implementation with adjustable confirmation depth.
/// Collects K consecutive block hashes for cumulative PoW entropy.
pub fn calculate_roll_with_depth(
    block_hashes: &[pallas::Base],
    bet_id: BetId,
    secret_nonce: pallas::Base,
) -> u8 {
    // Combines cumulative block hash entropy with bet_id + secret_nonce
    // Secure: attacker must control K consecutive blocks to manipulate
}
```

See [Entropy Module](entropy.md) for the composable randomness API.

### Current Flow

```
1. Commit Phase:
   - Player commits to bet (value, target, secret_nonce, blind)
   - Contract stores: bet_id = poseidon_hash(params...)
   - Value is locked via Money::Burn + spend_hook

2. Reveal Phase (when tx is included in block):
   - tx_hash is determined by block inclusion (wasm::util::get_tx_hash())
   - K consecutive block hashes collected from verifying block height
   - roll = calculate_roll_with_depth(&block_hashes, bet_id, secret_nonce)
   - The tx_hash is split into 4 field elements and passed as Vec<pallas::Base>
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

### DarkWow's PoW Mechanism

DarkWow uses **RandomX**, a CPU-intensive and memory-hard PoW algorithm:

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

### Adjustable Confirmation Depth

Security improves with **depth** (K) — the number of blocks confirmed after the
bet. The probability of an attacker with hash-power fraction `p` controlling K
consecutive blocks is `p^K`. At K=6 with 33% hash power, manipulation probability
is ~0.14%; at K=10 it's ~0.005%.

Players select depth (higher = more secure, longer wait). The roll combines K
consecutive block hashes via poseidon: `roll = H(block_N || ... || block_{N+K} || commit || secret)`.
Even if a miner controls one block, controlling K consecutive blocks becomes
exponentially harder.

---

## VRF-Based Randomness

### ECVRF in DarkWow

DarkWow implements ECVRF (Elliptic Curve Verifiable Random Function) based on [draft-irtf-cfrg-vrf-04](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04). See `src/sdk/src/crypto/ecvrf.rs` for the full implementation (prove, verify, hash_output).

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

| Attack | Severity | Mitigation |
|--------|----------|------------|
| **Miner manipulation** (withholds blocks to influence hash) | Medium | Use confirmed blocks + commit-reveal + multi-block depth. At 33% hash rate, 2 consecutive blocks = 11% probability — not viable for typical bet sizes. |
| **Player pre-computation** | Low | Commit phase binds player before roll is known. |
| **Oracle manipulation** (VRF-only solutions) | High | Use PoW hash as primary source; VRF adds verifiable contribution. |
| **Transaction ordering** (affects tx_hash) | Low | Full tx_hash used, not position. Use block hash from next block for high-stakes. |

### Randomness Quality

| Source | Entropy (bits) | Suitable for Gambling? |
|--------|---------------|------------------------|
| tx_hash | ~64 | No (manipulable) |
| PoW block hash | ~256 | Yes (with commit-reveal) |
| ECVRF | ~256 | Conditional |
| PoW + VRF + Commit | ~256+ | Recommended |

---
## Design Note: Block Height Prediction Market

A block height prediction market was designed as a case study for PoW-backed
randomness resolution. The `prediction_market` contract referenced by the
original design does not exist as a standalone contract in the codebase.
Prediction market functionality is available via [DarkBet Exchange](darkbet_exchange.md),
which supports AMM-based binary outcome markets with block hash entropy.

Key design principles from the study remain applicable:
- Block hash entropy from confirmation depth K provides `p^K` manipulation resistance
- Combined PoW + commit-reveal is the recommended randomness pattern
- DarkWow's rank-based fork resolution provides deterministic chain selection in normal operation

---
## Randomness Primitives: Implementation Status

### WASM Runtime Imports (Available to Contracts)

These are import functions provided by the WASM runtime — they are NOT zkVM opcodes:

| Function | Description | Status |
|----------|-------------|--------|
| `wasm::util::get_block_hash(height)` | Get block hash at height | Implemented |
| `wasm::util::get_block_height()` | Get current verifying block height | Implemented |
| `wasm::util::get_tx_hash()` | Get transaction hash | Implemented |
| `wasm::util::get_block_timestamp()` | Get block timestamp | Implemented |

### zkVM Opcodes (In-Circuit)

| Opcode | Description | Status |
|--------|-------------|--------|
| `VRFVerify` | Verify ECVRF proof in-circuit | Not implemented |
| `TimeGet` | Get current block time in-circuit | Not implemented |
| `TimestampCompare` | Compare timestamps in-circuit | Not implemented |

### Usage

```rust
// PoW block hash as randomness source (WASM runtime import)
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

- **Use PoW block hash, not tx_hash alone**, for high-stakes randomness
- **Combine sources**: PoW + user contribution + optional VRF
- **Use commit-reveal** to prevent pre-computation
- **Prefer cumulative multi-block hash** over single block: K=1 for real-time, K=3–6 for standard, K=10+ for institutional
- **Player-selected confirmation depth** gives participants control over the security/latency tradeoff

Randomness security ≈ f(Time, PoW_Work, Depth). This is why Bitcoin's "6 confirmations" became standard — combined PoW work is exponentially harder to reverse.

---

## Case Study: Baccarat

Baccarat (Punto Banco) demonstrates cumulative PoW entropy for **multi-card dealing** — a more complex randomness use case than single-value dice rolls. With no player decisions and completely deterministic drawing rules, Baccarat is a natural fit for blockchain gambling (no disputes possible).

### Card Dealing via Cumulative Entropy

Unlike dice (single roll value), Baccarat deals 4 cards using cumulative PoW entropy from K consecutive block hashes. A single 256-bit hash is expanded into multiple card values by treating different byte ranges as separate seeds — secure as long as the original hash is unpredictable.

The Baccarat contract (`src/contract/baccarat/`) implements four phases: CommitBetV1 (player commits with secret nonce), DrawCardsV1 (K-block cumulative entropy derivation), SettleBetV1 (fixed drawing rules applied), and HouseCloseV1 (timeout handling).

### Confirmation Depth

Like DarkToshi Dice, Baccarat uses player-selected confirmation depth. Economic tradeoff:

| Depth | Wait Time | Security | Typical Use |
|-------|-----------|----------|-------------|
| 1-2 | 2-4 min | Low | Micro-bets |
| 3-6 | 6-12 min | High | Standard casino play |
| 10+ | 20+ min | Very High | High-stakes |

### Security Properties

- Cards derived from PoW entropy — neither player nor miner can predict
- Cumulative entropy from K consecutive blocks makes manipulation exponentially harder (p^K)
- Commit-reveal: secret nonce committed before cards are known

## game_room EntropyMode::TrustedSetup

The [game_room](game_room.md) contract implements a commit-reveal entropy scheme
via `EntropyMode::TrustedSetup` at `src/contract/game_room/src/entrypoint/entropy.rs`.
This allows trusted operators or house-selected oracle addresses to contribute
entropy for multi-player game rooms, using `ContributeEntropyV1` (0x04).

The TrustedSetup mode works as follows:
1. Game room is initialized with an entropy mode and a list of trusted entropy contributors
2. Contributors call `ContributeEntropyV1` with a commitment to their entropy value
3. After all contributors have committed, the room operator reveals the combined entropy
4. The entropy is fed into the game's randomness derivation

This provides a flexible entropy source for games that require coordinated randomness
across multiple players, complementing the block-hash-based approach used by
DarkToshi Dice, Baccarat, Roulette, and Slot.

## Contracts Using Block Hash Entropy

| Contract | Entropy Source | Mechanism |
|----------|---------------|-----------|
| **DarkToshi Dice** | `wasm::util::get_block_hash()` | `calculate_roll_with_depth()` — K-block cumulative hash |
| **Baccarat** | `wasm::util::get_block_hash()` | `deal_cards()` — K-block cumulative hash for 4-card dealing |
| **Roulette** | `wasm::util::get_block_hash()` | Own `draw_winning_number()` — manual entropy extraction |
| **Slot** | `wasm::util::get_block_hash()` | Direct extraction from 32-byte block hash into 4 × u64 |
| **Lottery** | Block hash via `dwow_sdk::crypto::entropy::draw_unique_range()` | LCG-based without-replacement sampling |
| **game_room** | `EntropyMode::TrustedSetup` commit-reveal | `ContributeEntropyV1` — multi-party entropy contribution |

### See Also

- [Baccarat Contract](baccarat.md) - Full contract documentation
- [DarkToshi Dice Contract](darktoshi_dice.md) - Simpler randomness example
- [game_room Contract](game_room.md) - TrustedSetup entropy
- [Entropy Module](entropy.md) - Composable randomness API
- [Block Height Prediction Market](#design-note-block-height-prediction-market) - Design concept

---

## See Also

- [DarkToshi Dice Contract](darktoshi_dice.md)
- [game_room Contract](game_room.md) - TrustedSetup entropy
- [Baccarat Contract](baccarat.md)
- [Roulette Contract](roulette.md)
- [Slot Contract](slot.md)
- [Lottery Contract](lottery.md)
- [DarkBet Exchange](darkbet_exchange.md) - AMM-based prediction markets
- [Entropy Module](entropy.md)
- [ECVRF Implementation](../../../src/sdk/src/crypto/ecvrf.rs)
- Consensus Mechanism — fork resolution and rank-based chain selection
- Fork Ranking Utils — `block_rank()` and `best_fork_index()`
- [VRF Research](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04)
- [RandomX Paper](https://eprint.iacr.org/2018/1033)
