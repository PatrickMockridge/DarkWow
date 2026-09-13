# Uncle Merkle Consensus

Uncle Merkle consensus is a Pareto efficient mechanism: the canonical chain is **obligated** to offer competing uncle chains a one-time option to form a side chain and share the PoW reward. The uncle chain has a short time window (minutes) to accept or reject. Miners aren't punished for producing blocks that don't become canonical.

## Motivation

Chain splits are handled the Bitcoin way: miners follow the most-work chain.
Mining on the losing side of a fork race earns zero reward — an
all-or-nothing gamble that punishes propagation and rewards fork hiding. The
Uncle Merkle mechanism removes that gamble: a block that loses the fork race
is still referenced as an uncle by the next canonical block and earns a
partial pin reward.

The Uncle Merkle design is a simple merkle-tree-based mechanism that is:
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
    /// Uncle chain accepted the pin (use it or lose it - one time decision)
    pub pin_accepted: bool,
    /// Pin confirmed — reward amount computed from depth via `split_for_uncle`:
    /// `pin_confirmed = base_reward / 2^depth` (50% at d1, 25% at d2, …).
    /// Actual payment is computed downstream by `compute_reward()` and
    /// `verify_uncle_split()`; only uncles with `pin_accepted == true` are paid.
    pub pin_confirmed: BlockReward,
}
```

Depth is not stored in the struct — it is derived on demand via
`UncleBlock::depth_for(current_height, uncle_height)` at the creation sites:
`clamp(current_height − uncle_height, MAX_UNCLE_DEPTH)`.

> **Devnet wipe required.** The sled `uncles`-tree binary format and the JSON
> wire format are struct-layout-locked to `UncleBlock` (`src/linear/src/block.rs`) —
> a sled DB or peer running a different build fails uncle deserialization. A
> devnet restart SHALL start from wiped sled DBs and run one build across all
> nodes.

### UncleProof

For stateless verification, the merkle path carries only what cannot be derived
from the `UncleBlock` the caller already holds:

```rust
pub struct UncleProof {
    /// Merkle proof path from uncle to root
    pub merkle_path: Vec<[u8; 32]>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
}
```

**Security invariant**: the uncle's PoW hash SHALL be RECOMPUTED by the verifier
from `uncle.header` using `header.randomx_key`, and SHALL be required to meet the
target in force at the uncle's own height. It is never read from the proof — an
earlier revision carried `header` and `pow_hash` here as well, which meant the
verifier recomputed a value the proof also asserted (redundant) and the builder ran
a full RandomX cache+VM initialisation per uncle purely to fill it (2N
initialisations per block, on the accept path).

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

Each uncle block's header contains a valid RandomX proof-of-work, which the
verifier RECOMPUTES from the header — nothing about the PoW is trusted from the
proof itself.

### Verification Process

When verifying an `UncleProof`:

1. **Re-compute PoW hash**: Using the uncle's `header.randomx_key`, compute the RandomX hash of the header bytes.

2. **Check difficulty**: Verify the PoW hash meets **the target in force at the uncle's own height**, supplied by the caller. This is NOT the referencing block's target: the target is recomputed every block from a sliding timestamp window, so an uncle mined a few blocks earlier was mined against a different target, and judging it by the current one would reject honest work (and make which stale work is payable a function of the current target).

3. **Verify merkle inclusion**: Verify the header is included in the uncle merkle tree rooted at `uncle_merkle_root`.

```rust
pub fn verify_uncle_proof(
    header: &BlockHeader,
    proof: &UncleProof,
    merkle_root: &[u8; 32],
    target: BlockTarget,          // the target at `header.height`, NOT the current one
) -> bool {
    // Step 1: Re-compute RandomX PoW from the canonical 260-byte mining blob
    // (block.rs::to_mining_blob — JSON is variable-length and non-canonical,
    // and cannot be used for merkle proofs).
    let flags = randomx::RandomXFlags::get_recommended_flags();
    let cache = randomx::RandomXCache::new(flags, &header.randomx_key)?;
    let verify_vm = randomx::RandomXVM::new(flags, Some(cache), None)?;
    let blob = header.to_mining_blob();
    let rx_hash = verify_vm.calculate_hash(&blob)?;
    let computed_pow_hash: [u8; 32] = rx_hash[..32].try_into().unwrap();

    // Step 2: Difficulty check against the caller-supplied per-height target
    let hash_u32 = u32::from_le_bytes(computed_pow_hash[0..4].try_into().unwrap());
    if !target.hash_is_valid(hash_u32) {
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

Depth SHALL be at least 1: `depth == 0` (a sibling block at the SAME height as the
referencing block) is not an uncle and SHALL be rejected, since the schedule above
is defined from depth 1 and depth 0 would pay the full base reward.

`pin_confirmed_i` SHALL be DERIVED by validation, not trusted from the wire:
`check_uncles` SHALL require `pin_confirmed_i == expected_reward(H) / 2^depth_i` for
every accepted pin. `pin_confirmed` is a producer-controlled field and it sits
OUTSIDE `uncle_merkle_root` (which commits only to `uncle.header`), so without this
check a relaying producer could rewrite the split for any included uncle. A pin
that is NOT accepted (`pin_accepted == false`) pays nothing, so its field is
unconstrained.

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
/// Σ of the pins actually payable — accepted pins only. THE single implementation
/// of Σ pin, shared by the builder, the host check, and the connect-time split
/// check so the three cannot diverge. Overflow is an error, never a wrap.
pub fn total_accepted_pin(uncles: &[UncleBlock]) -> Result<BlockReward>;

/// Pin mechanism: an uncle is paid `pin_confirmed` ONLY if `pin_accepted == true`.
/// Canonical reward = base_reward − Σ pin_confirmed (no over-minting).
/// Invariant: canonical_reward + Σ uncle_rewards == base_reward.
/// A violated invariant aborts — it must never degrade to a zero reward.
fn compute_reward(
    base_reward: BlockReward,
    uncles: &[UncleBlock],
) -> Result<(BlockReward, Vec<u64>)> {
    let base = base_reward.get();
    if uncles.is_empty() {
        return Ok((base_reward, vec![]));
    }

    let mut uncle_rewards = Vec::with_capacity(uncles.len());
    for uncle in uncles {
        let pin = if uncle.pin_accepted { uncle.pin_confirmed.get() } else { 0 };
        uncle_rewards.push(pin);
    }

    let total_pin_confirmed = total_accepted_pin(uncles)?;
    let canonical_reward = base.checked_sub(total_pin_confirmed.get()).ok_or_else(|| {
        // pin rewards exceeding base is a consensus bug, not a recoverable condition
        LinearError::BlockIsInvalid(/* … */)
    })?;
    Ok((BlockReward::new(canonical_reward), uncle_rewards))
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

Let the canonical block at height `H` have a coinbase transaction whose
plaintext PoWRewardV1 (0x05) call carries a Pedersen value commitment:

```
C_base = v * G_v + r * G_r
```

where:
- `v = base_reward = expected_reward(H)` — the emission schedule value
- `G_v, G_r` — independent Pedersen generators (NUMS, nothing-up-my-sleeve)
- `r` — the plaintext deterministic `value_blind` field in `PoWRewardParamsV1`
  (derived from `sk_H`; the entrypoint recomputes the commitment in the clear)

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

**Status: implemented.** `connect_block` computes
`r_i = from_uniform_bytes(blake3(uncle_hash || u_i.to_le_bytes() || H.to_le_bytes()) doubled)`
(`chain_state.rs::connect_block`, where `uncle_hash = blake3(to_mining_blob())`),
binding uncle identity, amount, and height.

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
`C_effective` is the commitment they actually control. The plaintext PoWRewardV1
entrypoint verified `C_base` was correctly minted; the consensus split verifies
it was correctly distributed.

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

**Status: implemented** (`bin/dwowd/src/tests/uncle_minting.rs` regression
tests the full path). The sections below describe the deployed behavior:
uncle notes are minted by `uncle_mint_v1` (0x07) via `build_uncle_mint_tx`
and persisted at connect time.

#### Miner identity in the header

A spendable note is AEAD-encrypted to its recipient's public key. To mint an
uncle note the canonical miner MUST know the uncle miner's public key. This key
is therefore carried in the block header:

- `BlockHeader.miner: [u8; 32]` SHALL be a mandatory header field holding the
  miner's **cycled per-block address** — the 32-byte compressed `pk_H` that is the
  spending key of the miner's `StandardAddress`.
- It SHALL be included in `to_mining_blob()`, appended **after** the
  `pow_source_disc` byte so the RandomX nonce offset (byte 39, xmrig's Monero
  rx/0 offset) is preserved. The mining blob is 260 bytes
  (`BlockHeader::MINING_BLOB_LEN`).
- It SHALL be covered by PoW: the miner commits to their own reward address as
  part of the mined header.
- The canonical miner SHALL set `miner` to its cycled coinbase recipient key
  `pk_H = sk_owner.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &H.to_le_bytes()).public()`
  (consensus-coinbase.md §2.2). For an uncle block, `miner` is that uncle's cycled
  `pk_H` at the uncle's own height; the canonical miner reads `uncle.header.miner`
  to encrypt the uncle note.

**Privacy model.** `miner` is the miner's **per-block cycled address**, NOT a
stable/master identity. Because `pk_H` is derived fresh per height
(`derive_instance`), no two blocks' `miner` values link to the same miner — this
is the same unlinkability guarantee the coinbase already relies on
(consensus-coinbase.md §2.2, "per-block address cycling for privacy-preserving
rewards"). A stable owner/master public key SHALL NOT appear in the header: that
would be a linkable identity and a privacy break.

`miner` is the raw 32-byte `pk_H`, not the full `StandardAddress` encoding. The
`StandardAddress` wraps the public key (`[1-byte prefix][32-byte pubkey][4-byte
checksum]`); the prefix and checksum are human-facing metadata, so the header
stores only the 32-byte spending key for consensus.

#### Two-commitment distinction

Two distinct commitment kinds participate in uncle rewards, and they MUST NOT be
conflated:

1. **Pedersen audit commitments** (defined in [§Coinbase Split via Pedersen Mass
   Balance](#coinbase-split-via-pedersen-mass-balance)): `C_uncle_i = u_i·G_v +
   r_i·G_r` and `C_effective = C_base − Σ C_uncle_i`. These are EC points in the
   stored cumulative supply chain (`blockchain.get_cumulative_supply`) that
   prove the subtractive mass balance `C_effective + Σ C_uncle_i = C_base`. They are NOT
   spendable — no nullifier, no note, no merkle path attaches to them.
2. **Spendable Poseidon notes** (this section): a spendable coin is a note
   `C' = poseidon(pk, value, asset, hook, data, blind)` + nullifier
   `nf' = poseidon(sk, C')` + an AEAD note, produced by the plaintext mint path
   (no ZK proof — the reward values are public). Only these are
   spendable via `SpendV1`/`TransferV1`/`FeeV2`/`BurnV1`.

The uncle reward's *value* is the same in both (`u_i = pin_confirmed_i`), but the
Pedersen point is the audit record and the Poseidon note is the spendable coin.
The canonical coinbase's Pedersen value commitment and its spendable note are
similarly distinct (`C_base` vs `C'_effective`).

#### Canonical note reduction

The coinbase `pow_reward_v1` (0x05, plaintext) SHALL continue
to mint the FULL base reward into the cumulative supply chain —
`S_H = S_{H-1} + C_base`, where `C_base = pedersen_commit(base_reward, r)` —
and the `expected_reward` supply check is unchanged.

The canonical miner's **spendable note**, however, SHALL commit to the REDUCED
`effective_value = base_reward − Σ pin_confirmed_i`:

- The note commitment SHALL be `C'_effective = poseidon(..., effective_value, ...)`.
- The nullifier SHALL bind to the reduced note: `nf' = poseidon(sk_H, C'_effective)`.
- `value` and `value_commit` SHALL remain on the FULL base reward (they drive
  `S_H` and the Pedersen cumulative chain).
- No circuit witness is needed: `total_pin = value − effective_value` SHALL be a
  plaintext field in `PoWRewardParamsV1`, so the consensus layer can bind the
  actually-spendable coinbase note to the header reward without breaking the
  note's hiding (the value is already public as the block reward).

#### Spendable-note mass balance (consensus enforcement)

The value-level mass balance (`verify_uncle_split`) is NOT sufficient: it checks
`header.total_reward + Σ pin == base_reward` against the *claimed* pins, but the
spendable notes themselves could deviate. A malicious miner could set
`effective_value` to the FULL base while also emitting uncle notes — over-minting
by `Σ pin` (total spendable = base + Σ pin,
while `S_H` and `TOTAL_SUPPLY` only advance by base). Bitcoin/Zcash precedent: the
coinbase's spendable value is a public, consensus-checked quantity, never a
prover-asserted hidden value.

To close this, the consensus layer SHALL verify the spendable-note mass balance
in PLAINTEXT (the coinbase and uncle notes are plaintext — no Mint_V2 proof):

```
coinbase.effective_value + Σ uncle_note.effective_value == base_reward
```

where `base_reward = expected_reward(height)`. `effective_value` is the ONE term
for a note's committed value throughout this section: the uncle note's
`effective_value` is its `pin_confirmed_i` (the two are required equal by
`uncle_mint_v1`, which SHALL reject any other value).

**The value MUST be carried in the clear, together with the note preimage.** The
call data SHALL carry, as plaintext fields:

- `effective_value` — the value the spendable note commits to, and
- `commitment_attrs` — the `CommitmentAttributes` preimage of the note
  (`version`, `public_key`, `value`, `asset_id`, `spend_hook`, `user_data`,
  `blind`), with `commitment_attrs.value == effective_value`.

This is load-bearing. Stating the value alone is not enough: the coinbase's spend
key is the producer's own, so a producer could declare a compliant `effective_value`
and still commit the note to any other value, since nothing on-chain re-derives the
commitment. The entrypoint SHALL therefore recompute
`commitment_attrs.to_commitment()` and SHALL require it to equal the committed
output commitment. `value_blind` and `token_blind` SHALL NOT be published: they are
the Pedersen blinding factors and no check needs them.

The host (`block_acceptor.rs` reward check + `validate_block_structure` +
`verify_uncle_split`) SHALL verify `header.total_reward == base_reward − Σ pin`,
`coinbase.total_pin == Σ pin`, `Σ uncle_note.value == Σ pin`, and
`coinbase.effective_value + Σ pin == expected_reward(height)`. Additionally, the
coinbase note's `commitment_attrs.public_key` SHALL equal `header.miner`, and each
uncle note's `commitment_attrs.public_key` SHALL equal the `header.miner` of an
uncle **included in that block** whose pin equals the note's value. Combined, this
forces total spendable value to exactly equal total emitted value. Over-minting by
Σ pin is impossible.

#### Per-uncle note mint

For each accepted uncle `i` (`pin_accepted == true` and `pin_confirmed_i > 0`),
the canonical miner SHALL mint exactly one spendable note of value
`pin_confirmed_i`, in PLAINTEXT (no Mint_V2 proof — same as the coinbase):

- Build the note with plaintext Pedersen/Poseidon (deterministic per-uncle blinds
  from `uncle_hash` + height, `value = pin_confirmed_i`,
  `value_commit = pedersen_commitment_u64(value, value_blind)`,
  `token_commit`, `commitment = poseidon(attributes)`). `old_cumulative_value = 0`
  and `old_cumulative_blind = 0`; the note is NOT added to `S_H` — its value is
  carved out of the coinbase's full base, so it is not new supply.
- **Spend authority.** The commitment SHALL commit to `public_key =
  uncle.header.miner` — the uncle miner's own key, whose secret only that miner
  holds. The burn/spend circuit derives the committed public key from the witness
  `spend_secret` in-circuit (`burn.zk`: `pub = ec_mul_base(spend_secret,
  NULLIFIER_K)`, then the coin hash from `pub`'s coordinates), so this binding
  makes the note spendable ONLY by the uncle miner. It SHALL NOT be derived from a
  publicly-computable value: the canonical miner knows only public data, so any
  derivation it can perform is one that any network observer can also perform, and
  the note would be spendable by anyone.
- Encrypt the note to `uncle.header.miner` with
  `AeadEncryptedNote::encrypt_deterministic` (deterministic per-uncle ephemeral
  secret, domains 21-24 — mirroring `transfer/mod.rs` output minting). The
  encrypted payload's `spend_secret` field is a placeholder: the minter does not
  know the spend key. The recipient wallet SHALL resolve the spend key from the
  on-chain plaintext preimage, by matching `commitment_attrs.public_key` against
  its own keys.
- **No mint-time nullifier.** The mint SHALL publish no nullifier, in the call
  data or in the output, because the minter cannot compute one (it does not know
  the spend key). `output.nullifier` SHALL be `None`, and the mint's
  `effective_value`/`commitment_attrs` fields carry the value binding instead. The
  note's nullifier is revealed at SPEND, as for every other note. A published
  mint-time nullifier SHALL be rejected — it would be an unauthenticated write into
  the nullifier tree, letting an attacker poison an arbitrary nullifier and block a
  legitimate spend.
- Emit the uncle note as a native_token `uncle_mint_v1` (0x07) entrypoint call
  that verifies the value/token/duplicate-commitment/preimage checks in PLAINTEXT
  and writes the note to the contracts tree WITHOUT touching
  `cumulative_value_commit`/`supply_chain` (the production-consistent analog of
  `pow_reward_v1` minus the supply increment). It MUST NOT be emitted as a
  `pow_reward_v1` call, which would bump `new_supply` and over-mint.

Mass balance: `C'_effective (value = base − Σ pin) + Σ C'_uncle_i (value = pin_i)
= base`, and `S_H` advanced by exactly `C_base`. Total spendable = total emitted.

#### Maturity, persistence, and reversal

1. Each uncle note COMMITMENT SHALL be persisted into the sled `commitment_set`
   tree, keyed at the canonical block's height, in the same atomic cross-tree sled
   transaction as the canonical coinbase. No nullifier is persisted at mint: the
   mint publishes none (see §"Per-uncle note mint"), so there is nothing to record
   and the note's nullifier lands in `spent_nullifiers` at spend, as usual.
2. `COINBASE_MATURITY` (100 blocks) SHALL apply uniformly to the canonical
   coinbase note and every uncle note — no uncle note may be spent before
   maturity.
3. `disconnect_block` SHALL reverse uncle notes: displaced uncle note
   commitments SHALL be removed from `commitment_set` along with the displaced
   canonical coinbase note, in the same cross-tree sled transaction. There is no
   mint-time nullifier to reverse. The per-block record of uncle notes is the
   block's own transactions (the uncle-mint calls), so no separate undo tree is
   required.

#### Uncle blind

`r_i` SHALL be `r_i = blake3s(uncle_hash ‖ u_i ‖ H) mod p` for the Pedersen audit
commitment `C_uncle_i`, binding uncle identity (`uncle_hash`), amount (`u_i`),
and canonical height (`H`) — see
[§Uncle Commitment Creation](#uncle-commitment-creation).

### Audit Compatibility

The Pedersen cumulative supply chain state (readable via
`blockchain.get_cumulative_supply`) records `S_H = S_{H-1} + C_base` from each
block's coinbase commitment. The subtractive split is auditable because `r_i`
is deterministic — an auditor can recompute every `C_uncle_i` and verify
`C_effective + sum(C_uncle_i) == C_base` for every block independently. The
audit does not verify ZK proofs; it verifies Pedersen binding.

## Verification (Stateless)

Block verification is a function of merkle proofs and math. Uncle proof checks
are `check_uncles()` in `src/linear/src/validation.rs`; the reward split is
`verify_uncle_split()` in `src/linear/src/supply_chain.rs`, called from
`connect_block()`:

```rust
// Uncle proof verification (validation.rs::check_uncles) — per uncle:
//   1. uncle count <= MAX_UNCLE_COUNT, and one target per uncle supplied
//   2. depth explicit and bounded: 1 <= depth <= MAX_UNCLE_DEPTH
//      (depth 0 — a sibling at the same height — is not an uncle)
//   3. uncle PoW valid: RandomX over to_mining_blob() re-hashed with the uncle's
//      OWN randomx_key, meeting the target IN FORCE AT THE UNCLE'S OWN HEIGHT
//      (passed in as `uncle_targets[i]`, resolved from chain state), NOT the
//      referencing block's target — the target is recomputed every block from a
//      sliding timestamp window, so the current target is not the uncle's rule
//   4. uncle merkle proof verifies against uncle_merkle_root
//   5. an ACCEPTED pin is derived: pin_confirmed_i == expected_reward(H)/2^depth_i
//   6. uncle uniqueness (not already stored)
// (the uncle_merkle_root recomputation lives with the caller —
//  block_acceptor / connect_block — not inside check_uncles)

// UncleProof carries only { merkle_path, position }: the header comes from the
// caller's UncleBlock and the PoW hash is recomputed, never trusted from the proof.

// Reward distribution (supply_chain.rs::verify_uncle_split) — SUBTRACTIVE,
// with Σ pin from the single shared implementation (block.rs::total_accepted_pin):
let total_pin = total_accepted_pin(uncles)?;

// header.total_reward == canonical_reward == base_reward - Σ pin_confirmed
if block.header.total_reward.get() + total_pin.get() != base_reward.get() {
    return Err(LinearError::BlockIsInvalid(format!(
        "Supply invariant violated: canonical({}) + uncles({}) != base_reward({})",
        block.header.total_reward.get(), total_pin, base_reward.get(),
    )));
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
    let depth = depth.min(MAX_UNCLE_DEPTH);        // P2-9-3: feeds the split only
    let pin_confirmed = base_reward.split_for_uncle(depth);
    UncleBlock {
        header: block.header,
        transactions: block.transactions,
        pin_accepted: false,                       // uncle must later call accept_pin()
        pin_confirmed,                             // base / 2^depth
    }
}

fn build_uncle_merkle(uncles: &[UncleBlock]) -> Result<([u8; 32], Vec<UncleProof>)> {
    // 1. Compute pow_hash for each uncle using their randomx_key over the
    //    canonical 260-byte to_mining_blob() (JSON is non-canonical)
    let pow_hashes: Vec<[u8; 32]> = uncles.iter().map(|u| {
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &u.header.randomx_key)?;
        let uncle_vm = randomx::RandomXVM::new(flags, Some(cache), None)?;
        let hash_bytes = uncle_vm.calculate_hash(&u.header.to_mining_blob())?;
        let mut pow_hash = [0u8; 32];
        pow_hash.copy_from_slice(&hash_bytes[..32]);
        Ok(pow_hash)
    }).collect::<Result<_>>()?;

    // 2. Build merkle tree of uncle hashes over to_mining_blob() (uses blake3
    //    for structure; odd leaf counts duplicate the last leaf)
    let mut leaves: Vec<blake3::Hash> = uncles.iter()
        .map(|u| blake3::hash(&u.header.to_mining_blob()))
        .collect();
    // ... build merkle root ...

    // 3. Create proofs with pow_hash bound
    let proofs = uncle_proofs.iter().enumerate().map(|(i, u)| {
        UncleProof {
            header: u.header.clone(),
            pow_hash: pow_hashes[i],
            merkle_path: get_merkle_proof(leaves, i),
            position: i as u32,
        }
    }).collect();

    Ok((root, proofs))
}
```

## Uncle Transactions and Block Construction

### Transaction-First Model

Uncle blocks participate in block execution with isolation from the canonical
sequence: canonical calls run sequentially in block order on ONE shared overlay
(`execution.rs`), while each uncle call runs on an isolated pre-block clone
(failed uncle calls are tolerated — `uncle_calls_failed` — whereas a canonical
failure rejects the block).

### Deterministic Merge

After execution, results are merged deterministically:

1. Canonical calls execute sequentially in block order on one shared overlay
   (their writes land in block order — no sorting)
2. Each uncle call runs on an isolated pre-block overlay clone
3. Uncle diffs subtract the canonical total before merging (`remove_diff`) —
   canonical values win on conflicts
4. Uncle-vs-uncle duplicate key writes are a hard reject (`DuplicateKeyConflict`)
5. Single `sled::Batch` atomic commit

This naturally fits the uncle-merkle structure: uncle blocks are alternative merkle
trees of transactions, and the tx merkle tree cascades through blocks regardless of
whether the transactions are canonical or uncle.

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
| `MAX_COMPETING_BLOCKS` | `20` | `block.rs` | Maximum competing blocks stored per height |
| `COINBASE_MATURITY` | `100` | `src/linear/src/lib.rs` | Blocks before a coinbase/uncle commitment is spendable |

Consumers SHALL reference these via their crate re-exports (`crate::MAX_COMPETING_BLOCKS`,
`crate::MAX_UNCLE_DEPTH`) rather than local literals, so the spec and the code cannot drift.

## Implementation Status

The uncle-merkle consensus and full uncle minting are implemented and tested
(`bin/dwowd/src/tests/uncle_minting.rs`). Function names are referenced (not
fragile line numbers).

| Feature | Spec Section | Status |
|---------|-------------|--------|
| Uncle block creation | §Uncle Generation | Implemented — `block.rs::create_uncle()` |
| Uncle merkle tree construction | §Uncle Generation | Implemented — `block.rs::build_uncle_merkle()` |
| Uncle proof verification | §Verification (Stateless) | Implemented — `validation.rs::check_uncles()` |
| Pin computation (value-level) | §Reward Distribution | Implemented — `block.rs::compute_reward()` |
| Value-level split invariant | §Coinbase Split — Supply Invariant | Implemented — `supply_chain.rs::verify_uncle_split()`, called from `chain_state.rs::connect_block()` before the sled commit |
| Pedersen uncle commitment precompute | §Coinbase Split — Mass Balance Proof | Implemented — `chain_state.rs::connect_block()` pre-computes `C_uncle_i = pedersen_commitment_u64(u_i, Blind(r_i))` |
| Deterministic uncle blind `r_i` (identity+amount+height) | §Uncle Commitment Creation | Implemented — `chain_state.rs::connect_block()` binds identity, amount, and height |
| Full uncle minting into `commitment_set` | §Uncle Minting & Maturity | Implemented — coinbase note minted at the reduced effective value (`registry/model.rs::build_linear_coinbase_effective`); uncle notes minted via `uncle_mint_v1` (0x07) and persisted (`chain_state.rs::connect_block`) |
| Uncle commitment reversal on disconnect | §Uncle Minting & Maturity | Implemented — `chain_state.rs::disconnect_block()` |
| Uncle commitment set restoration on restart | — | Not implemented — in-memory only, initialized empty on restart (uncle Pedersen commitments are deterministically recomputable from chain data) |

## References

- Ethereum Uncle Mechanism: https://ethereum.org/en/developers/docs/consensus-mechanisms/pow/mining/
- RandomX: Memory-hard proof-of-work for CPU mining
- The design here is inspired by Ethereum's uncle concept but with RandomX PoW binding and deterministic reward distribution baked into the canonical block structure itself.