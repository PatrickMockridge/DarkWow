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
│                              ├── Uncle 0 (depth 1 → pin_confirmed = base / 2)
│                              └── Uncle 1 (depth 2 → pin_confirmed = base / 4)
└── reward split (subtractive mass balance):
      canonical_reward = base_reward − Σ pin_confirmed_i
      invariant: canonical_reward + Σ pin_confirmed_i == base_reward
```

Key insight: Uncle chains are **explicitly referenced** in the canonical block rather than implicitly competing. The reward model is **subtractive mass-balance**, not additive: the single coinbase `base_reward` is split so that `canonical_reward + Σ pin_confirmed_i == base_reward` (exactly 100%). The split neither creates nor destroys value.

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
    /// Pin confirmed — reward amount computed from depth via `split_for_uncle`:
    /// `pin_confirmed = base_reward / 2^depth` (50% at d1, 25% at d2, …).
    /// Actual payment is computed downstream by `compute_reward()` and
    /// `verify_uncle_split()`; only uncles with `pin_accepted == true` are paid.
    pub pin_confirmed: BlockReward,
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
    /// The canonical miner's EFFECTIVE reward: `base_reward − Σ pin_confirmed_i`.
    /// This is NOT the total emitted — it excludes the uncle pins. The
    /// invariant `total_reward + Σ pin_confirmed_i == base_reward` MUST hold.
    pub total_reward: BlockReward,
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

Each accepted uncle `i` at depth `depth_i` SHALL be paid a pin of
`pin_confirmed_i = base_reward / 2^depth_i`. This is the depth-adjusted pin,
computed by `BlockReward::split_for_uncle(depth) = self / (1 << depth)`.

| Depth | Pin (% of base reward) |
|-------|------------------------|
| 1 | 50% |
| 2 | 25% |
| 3 | 12.5% |
| 4 | 6.25% |
| 5 | 3.125% |
| 6 | 1.5625% |

Maximum depth is `MAX_UNCLE_DEPTH = 6` (see [Constants](#constants)).

**Production pattern:** Ethereum uncle/ommer rewards (a depth-decaying partial
reward so no valid PoW is fully wasted). DarkWow replaces Ethereum's
`(8 − depth) / 8` linear decay with an exponential `1 / 2^depth` decay.

### Subtractive Mass-Balance Model

The reward model is **subtractive**, not additive. The canonical miner receives
the base reward *minus* every accepted uncle pin:

```
canonical_reward = base_reward − Σ pin_confirmed_i   (accepted uncles only)
uncle_reward_i   = pin_confirmed_i                    (if pin_accepted, else 0)
invariant: canonical_reward + Σ uncle_reward_i == base_reward   (exactly 100%)
```

A rejected pin (`pin_accepted == false`) pays `0` and contributes nothing to the
sum — the canonical miner keeps that uncle's share of the base reward.

### Reward Calculation

```rust
/// Pin mechanism: an uncle is paid `pin_confirmed` ONLY if `pin_accepted == true`.
/// Canonical reward = base_reward − Σ pin_confirmed (no over-minting).
/// Invariant: canonical_reward + Σ uncle_rewards == base_reward.
/// Returns (canonical_reward, uncle_rewards).
fn compute_reward(base_reward: BlockReward, uncles: &[UncleBlock]) -> (BlockReward, Vec<u64>) {
    let base = base_reward.get();
    if uncles.is_empty() {
        return (base_reward, vec![]);
    }

    let mut uncle_rewards = Vec::with_capacity(uncles.len());
    for uncle in uncles {
        let pin = if uncle.pin_accepted { uncle.pin_confirmed.get() } else { 0 };
        uncle_rewards.push(pin);
    }

    let total_pin_confirmed: u64 = uncle_rewards.iter().sum();
    // Canonical reward is base minus what it pays in accepted pins.
    // verify_uncle_split() rejects any overflow at commit time.
    let canonical_reward = base.checked_sub(total_pin_confirmed).unwrap_or(0);

    (BlockReward::new(canonical_reward), uncle_rewards)
}
```

`header.total_reward` SHALL be set to `canonical_reward` — the first element of
the returned tuple, the canonical miner's effective reward — NOT the total
emitted (`base_reward`).

## Coinbase Split via Pedersen Mass Balance

When a canonical block includes uncles with accepted pins, the single coinbase
is atomically split at the consensus level. The canonical miner receives the
effective reward: `canonical_reward = base_reward - Σ pin_confirmed_i`. No new
ZK proofs are required — the split is pure Pedersen commitment arithmetic.

**Production pattern:** DarkWow-unique. The subtractive split is a
Pedersen-commitment mass balance (`C_base = C_effective + Σ C_uncle_i`),
verified by additive homomorphism rather than a dedicated ZK circuit. The
deterministic uncle blind `r_i` is the analogue of Ethereum's uncle hash being
committed in the block, but here it binds value as well as identity.

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
- `pin_confirmed_i = v / 2^depth_i` — the depth-adjusted pin reward
- `uncle_hash_i` — the uncle's block header hash

#### Uncle Commitment Creation

For each accepted uncle `i`, a new commitment is created at the consensus level
with a deterministic Pedersen commitment:

```
C_uncle_i = u_i * G_v + r_i * G_r

where:
    u_i = pin_confirmed_i
    r_i = blake3s(uncle_hash_i || u_i.to_le_bytes() || H.to_le_bytes()) mod p
```

The blinding factor `r_i` is purely deterministic — no randomness, no ZK proof.
Any node can independently compute `r_i` from the uncle hash, pin reward, and
block height, and verify the commitment. `r_i` SHALL bind all three of uncle
identity (`uncle_hash_i`), amount (`u_i`), and height (`H`).

**Status: normative target.** The current implementation derives `r_i` from
`blake3(to_mining_blob())` mapped through `from_uniform_bytes` — it binds uncle
identity only, not `u_i` or `H`. The amount-and-height binding above is the
target and SHALL be adopted in a later phase.

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
// verify_uncle_split — enforced in connect_block BEFORE the atomic sled commit.
let canonical_value = block.header.total_reward;   // the canonical miner's effective reward
let total_pin: u64 = uncles.iter()
    .filter(|u| u.pin_accepted && u.pin_confirmed > BlockReward::new(0))
    .map(|u| u.pin_confirmed.get())
    .sum();
if canonical_value.get() + total_pin != base_reward.get() {
    return Err(LinearError::BlockIsInvalid(
        "Supply invariant violated: canonical + uncles != base_reward"
    ));
}
```

The invariant `total_reward + Σ pin_confirmed_i == base_reward` is enforced by
`CumulativeSupplyChain::verify_uncle_split(base_reward, block.header.total_reward,
&pin_confirmed)` and SHALL run BEFORE the block reaches disk.

### Uncle Minting & Maturity

**Production pattern:** Bitcoin `COINBASE_MATURITY` (100 confirmations before a
coinbase output is spendable), applied uniformly to the canonical coinbase and
every uncle note. DarkWow-unique is the *atomic* mint of both the reduced
canonical share and the uncle shares in a single cross-tree sled transaction.

**Status: normative target.** The design below is the full specification; the
code is being brought into conformance with it. Until implemented, uncle pins
are computed and verified value-level only (in-memory `uncle_commitment_set`),
not minted as spendable notes.

#### Miner identity in the header

A spendable note is AEAD-encrypted to its recipient's public key. To mint an
uncle note the canonical miner MUST know the uncle miner's public key. This key
is therefore carried in the block header:

- `BlockHeader.miner: PublicKey` (32-byte compressed pallas point) SHALL be a
  mandatory header field.
- It SHALL be included in `to_mining_blob()`, appended **after** the
  `pow_source_disc` byte so the RandomX nonce offset (byte 39, xmrig's Monero
  rx/0 offset) is preserved. The mining blob length grows 228 → 260 bytes.
- It SHALL be covered by PoW: the miner commits to their own reward address as
  part of the mined header.
- The canonical miner SHALL set `miner` to the public key of the coinbase
  recipient `pk_H = PublicKey::from_secret(sk_H)` (consensus-coinbase.md §2.2).
- For an uncle block, `miner` identifies that uncle's miner; the canonical miner
  reads `uncle.header.miner` to encrypt the uncle note.

`miner` is a public key, not a separate address type. A DarkWow `StandardAddress`
wraps the public key (`[1-byte prefix][32-byte pubkey][4-byte checksum]`;
`StandardAddress::public_key()` returns it), so an address and a public key carry
the same information; the header stores the raw 32-byte public key.

#### Two-commitment distinction

Two distinct commitment kinds participate in uncle rewards, and they MUST NOT be
conflated:

1. **Pedersen audit commitments** (defined in [§Coinbase Split via Pedersen Mass
   Balance](#coinbase-split-via-pedersen-mass-balance)): `C_uncle_i = u_i·G_v +
   r_i·G_r` and `C_effective = C_base − Σ C_uncle_i`. These are EC points used by
   the deterministic supply audit (`verify_cumulative_supply`) to prove the
   subtractive mass balance `C_effective + Σ C_uncle_i = C_base`. They are NOT
   spendable — no nullifier, no note, no merkle path attaches to them.
2. **Spendable Poseidon notes** (this section): a spendable coin is a note
   `C' = poseidon(pk, value, asset, hook, data, blind)` + nullifier
   `nf' = poseidon(sk, C')` + an AEAD note, produced by the Mint_V2 circuit. Only
   these are spendable via `SpendV1`/`TransferV1`/`FeeV2`.

The uncle reward's *value* is the same in both (`u_i = pin_confirmed_i`), but the
Pedersen point is the audit record and the Poseidon note is the spendable coin.
The canonical coinbase's Pedersen value commitment and its spendable note are
similarly distinct (`C_base` vs `C'_effective`).

#### Canonical note reduction

The coinbase `pow_reward_v1` (Mint_V2) SHALL continue to mint the FULL base
reward into the cumulative supply chain — `S_H = S_{H-1} + C_base`, where
`C_base = pedersen_commit(base_reward, r)` — and the `expected_reward` supply
check is unchanged.

The canonical miner's **spendable note**, however, SHALL commit to the REDUCED
`effective_value = base_reward − Σ pin_confirmed_i`:

- Add a `Base effective_value` witness to Mint_V2.
- The note commitment SHALL be `C'_effective = poseidon(..., effective_value, ...)`.
- The nullifier SHALL bind to the reduced note: `nf' = poseidon(sk_H, C'_effective)`.
- `value` and `value_commit` SHALL remain on the FULL base reward (they drive
  `S_H` and the Pedersen cumulative chain).
- `range_check(64, effective_value)` SHALL be added.

#### Per-uncle note mint

For each accepted uncle `i` (`pin_accepted == true` and `pin_confirmed_i > 0`),
the canonical miner SHALL mint exactly one spendable note of value
`pin_confirmed_i`:

- Reuse the transfer-v1 mint path (`create_transfer_mint_proof`) with
  `output.value = pin_confirmed_i`, `effective_value = pin_confirmed_i`,
  `old_cumulative_value = 0`, `old_cumulative_blind = 0`, and a **fresh**
  per-uncle `spend_secret`. `new_cumulative_commit` is computed by the circuit
  but is NOT added to `S_H` — the uncle note's value is carved out of the
  coinbase's full base, so it is not new supply.
- Encrypt the note to `uncle.header.miner` with `AeadEncryptedNote::encrypt`
  (mirroring `transfer/mod.rs` output minting).
- Emit the uncle note as a native_token mint entrypoint call that verifies the
  Mint_V2 proof and writes the note to the contracts tree WITHOUT touching
  `cumulative_value_commit`/`supply_chain` (the production-consistent analog of
  `pow_reward_v1` minus the supply increment). It MUST NOT be emitted as a
  `pow_reward_v1` call, which would bump `new_supply` and over-mint.

Mass balance: `C'_effective (value = base − Σ pin) + Σ C'_uncle_i (value = pin_i)
= base`, and `S_H` advanced by exactly `C_base`. Total spendable = total emitted.

#### Maturity, persistence, and reversal

1. Each uncle note commitment + nullifier SHALL be persisted into the sled
   `commitment_set` / `nullifiers` trees, keyed at the canonical block's height,
   in the same atomic cross-tree sled transaction as the canonical coinbase.
2. `COINBASE_MATURITY` (100 blocks) SHALL apply uniformly to the canonical
   coinbase note and every uncle note — no uncle note may be spent before
   maturity.
3. `disconnect_block` SHALL reverse uncle notes: displaced uncle note
   commitments + nullifiers SHALL be removed from `commitment_set`/`nullifiers`
   along with the displaced canonical coinbase note, in the same cross-tree sled
   transaction. The per-block record of uncle notes is the block's own
   transactions (the uncle-mint calls), so no separate undo tree is required.

#### Uncle blind

`r_i` SHALL be `r_i = blake3s(uncle_hash ‖ u_i ‖ H) mod p` for the Pedersen audit
commitment `C_uncle_i`, binding uncle identity (`uncle_hash`), amount (`u_i`),
and canonical height (`H`) — see
[§Uncle Commitment Creation](#uncle-commitment-creation).

### Audit Compatibility

The Pedersen cumulative supply audit (`verify_cumulative_supply()`) walks the chain
recomputing `S_H = S_{H-1} + C_base` from each block's coinbase commitment. The
subtractive split is auditable because `r_i` is deterministic — an auditor can
recompute every `C_uncle_i` and verify `C_effective + sum(C_uncle_i) == C_base` for
every block independently. The audit does not verify ZK proofs; it verifies Pedersen
binding.

## Verification (Stateless)

Block verification is a function of merkle proofs and math. Uncle proof checks
are `check_uncles()` in `src/linear/src/validation.rs`; the reward split is
`verify_uncle_split()` in `src/linear/src/supply_chain.rs`, called from
`connect_block()`:

```rust
// Uncle proof verification (validation.rs::check_uncles) — per uncle:
//   1. uncle count <= MAX_UNCLE_COUNT
//   2. uncle_merkle_root recomputed from uncles == header.uncle_merkle_root
//   3. uncle PoW valid (RandomX over to_mining_blob() meets target)
//   4. uncle merkle proof verifies against uncle_merkle_root
//   5. uncle recency: uncle_height > current_height - MAX_UNCLE_DEPTH
//   6. uncle uniqueness (not already stored)

// Reward distribution (supply_chain.rs::verify_uncle_split) — SUBTRACTIVE:
let total_pin: u64 = uncles.iter()
    .filter(|u| u.pin_accepted && u.pin_confirmed > BlockReward::new(0))
    .map(|u| u.pin_confirmed.get())
    .sum();

// header.total_reward == canonical_reward == base_reward - Σ pin_confirmed
if block.header.total_reward.get() + total_pin != base_reward.get() {
    return Err(Error::InvalidRewardDistribution);
}
```

The reward check is **subtractive**: `header.total_reward` SHALL equal
`base_reward − Σ pin_confirmed_i` (accepted uncles only). The invariant
`total_reward + Σ pin_confirmed_i == base_reward` MUST hold for every block —
there is no additive bonus over `base_reward`.

## Uncle Generation

Miners can create uncle blocks when they discover their block was not canonical:

```rust
fn create_uncle(block: Block, depth: u8, base_reward: BlockReward) -> UncleBlock {
    UncleBlock {
        header: block.header,
        transactions: block.transactions,
        depth: depth.min(MAX_UNCLE_DEPTH),
        pin_offered: true,
        pin_accepted: false,                       // uncle must later call accept_pin()
        pin_confirmed: base_reward.split_for_uncle(depth),  // base / 2^depth
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

1. **Uncle depth limit**: Uncle depth SHALL NOT exceed `MAX_UNCLE_DEPTH` (6), preventing infinite uncle chains.
2. **RandomX PoW binding**: Each `UncleProof.pow_hash` MUST match the re-computed hash over `to_mining_blob()` using `header.randomx_key` — prevents fake proofs without actual RandomX work.
3. **Difficulty check**: Each uncle's PoW MUST meet the canonical block's target.
4. **Merkle proof validation**: The uncle header MUST verify against `uncle_merkle_root`.
5. **Reward math validation**: `total_reward + Σ pin_confirmed_i == base_reward` MUST hold (subtractive mass balance) — prevents over-minting.

## Comparison with Ethereum

| Aspect | Ethereum Uncle | Linear Uncle Merkle |
|--------|---------------|---------------------|
| Hash function | Ethash (memory-hard) | RandomX |
| Uncle structure | Ommer (header only) | UncleBlock (header + txs) |
| PoW verification | On canonical block only | On each uncle (bound in proof) |
| Pin mechanism | None | Optional pin offers with one-time accept/reject |
| Reward formula | `(8 − depth) / 8` linear decay | `1 / 2^depth` subtractive pin |

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

## Constants

These named constants are the single source of truth. Spec prose SHALL reference
them by name rather than re-declaring local literals.

| Constant | Value | Defined in | Meaning |
|----------|-------|-----------|---------|
| `MAX_UNCLE_DEPTH` | `6` | `block.rs` | Maximum depth of an uncle in the reference tree |
| `MAX_UNCLE_COUNT` | `6` | `block.rs` | Maximum uncles per canonical block |
| `MAX_COMPETING_BLOCKS` | `20` | `chain_state.rs` | Maximum competing blocks stored per height |
| `COINBASE_MATURITY` | `100` | `linear/src/lib.rs` | Blocks before a coinbase/uncle commitment is spendable |

> Note: `MAX_COMPETING_BLOCKS` and `COINBASE_MATURITY` are currently duplicated
> as local `const` declarations in `chain_state.rs`. They SHALL be consolidated
> to a single `pub const` per value so the spec and the code cannot drift.

## Implementation Status

The uncle-merkle consensus is implemented value-level; full uncle minting is a
tracked gap. Function names are referenced (not fragile line numbers).

| Feature | Spec Section | Status |
|---------|-------------|--------|
| Uncle block creation | §Uncle Generation | Implemented — `block.rs::create_uncle()` |
| Uncle merkle tree construction | §Uncle Generation | Implemented — `block.rs::build_uncle_merkle()` |
| Uncle proof verification | §Verification (Stateless) | Implemented — `validation.rs::check_uncles()` |
| Pin computation (value-level) | §Reward Distribution | Implemented — `block.rs::compute_reward()` |
| Value-level split invariant | §Coinbase Split — Supply Invariant | Implemented — `supply_chain.rs::verify_uncle_split()`, called from `chain_state.rs::connect_block()` before the sled commit |
| Pedersen uncle commitment precompute | §Coinbase Split — Mass Balance Proof | Implemented — `chain_state.rs::connect_block()` pre-computes `C_uncle_i = pedersen_commitment_u64(u_i, Blind(r_i))` |
| Deterministic uncle blind `r_i` (identity+amount+height) | §Uncle Commitment Creation | **To be implemented** — current code binds identity only (`blake3(to_mining_blob())`) |
| Full uncle minting into `commitment_set` | §Uncle Minting & Maturity | **To be implemented** — current code mints full base and tracks uncles in in-memory `uncle_commitment_set` only |
| Uncle commitment reversal on disconnect | §Uncle Minting & Maturity | **To be implemented** |
| Uncle commitment set restoration on restart | — | Implemented — `chain_state.rs::CChainState::new()` |

## References

- Ethereum Uncle Mechanism: https://ethereum.org/en/developers/docs/consensus-mechanisms/pow/mining/
- RandomX: Memory-hard proof-of-work for CPU mining
- The design here is inspired by Ethereum's uncle concept but with RandomX PoW binding and deterministic reward distribution baked into the canonical block structure itself.