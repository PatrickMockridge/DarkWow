# Consensus & Coinbase Production

*Hard specification. Normative language (MUST/SHOULD/MAY) per RFC 2119.*

This document specifies block production, coinbase reward mechanics, emission
schedule, and the nullifier claim architecture that integrates coinbase rewards
with the wallet's pure-function model. It is the canonical reference for miner
and validator behavior.

## Design Decisions

DarkWow's tokenomics are assembled from proven, battle-tested parts:

| Component | Source | Status |
|-----------|--------|--------|
| **21M DRKW supply cap** | Satoshi / Bitcoin | Deployed 2009, untouched since |
| **RandomX PoW** | Monero | CPU-mining since 2019 |
| **Permanent tail emission** | Monero | 1% per annum, secures chain forever |
| **Fair launch** | Satoshi | No premine, no SAFT, no insider allocation |
| **Continuous exponential decay** | Novel (math, not mechanism) | Same 4-year half-life as Bitcoin, just smoothed |
| **Uncle Merkle pin rewards** | Novel | Pareto-efficient fork handling — no wasted work |
| **PoWRewardV1 nullifier claim** | Novel (this fork) | ZK capability-exercise coinbase — single path, miner/wallet symmetry |

The chassis is boring on purpose. Satoshi's supply model and Monero's mining model
have worked for a combined 30+ years. The novel pieces — ZK nullifier claim and
Uncle Merkle — are the minimum necessary innovation to achieve deterministic,
user-verifiable coinbase rewards.

### 21M Cap (Satoshi)

Same hard cap as Bitcoin. No inflation beyond what the tail emission adds.
Supply is deterministic from genesis — there are no governance knobs, no
minting authorizations, no token-holder votes that can change issuance.

### RandomX PoW (Monero)

CPU-optimized proof-of-work. ~4 GB dataset forces memory-hard computation —
ASICs and GPUs can't get a meaningful advantage. Anyone with a consumer laptop
can mine. This keeps mining distributed rather than concentrated in industrial
farms.

### Tail Emission (Monero)

1% per annum of the 21M cap, permanently. This works out to 79,853,981 base
units per block (~0.80 DRKW), or 210,000 DRKW/year. Monero's tail exists for
the same reason: when the main emission curve approaches zero, you need a floor
on the security budget. Without it, miners rely entirely on fees, and fee
markets are volatile. A permanent subsidy guarantees a minimum hash rate forever.

### Continuous Decay (Smoothed Halving)

Bitcoin halves every 4 years in a single step — miners lose 50% of revenue
overnight. DarkWow uses the same 4-year half-life but applies it continuously:
`R(h) = max(R₀ × 2^(-h/H), R_tail)`. Every block's reward is fractionally
smaller than the last. The emission curve is identical in total area under the
curve — it just doesn't have step-function shocks.

### Fair Launch (No Premine)

No tokens were allocated to founders, investors, or early participants.
Every DRKW in circulation was mined. This is the Bitcoin model: the only
way to acquire the native token is to contribute proof-of-work.

## 1. Block Production

### 1.1 Block Structure

A block is a header followed by an ordered list of transactions. The header
carries a `merkle_root` computed from the transactions via a binary blake3
Merkle tree (odd-layer padding duplicates the last element).

### 1.2 Transaction Ordering — Coinbase as Leaf 0

The first transaction (`transactions[0]`) MUST be the coinbase transaction.
The coinbase transaction MUST carry a `PoWRewardV1` contract call (function
code `0x05`) as `contract_calls[0]`. There MUST be exactly one coinbase
transaction per block.

```
Block Merkle Tree:
  Leaf 0:    Coinbase tx → PoWRewardV1 call → nullifier nf
  Leaf 1..N: User transactions (fees, transfers, burns, spends, deployments)
```

The PoWRewardV1 nullifier is the first entry in the nullifier SMT for this
block. It "unlocks" the block — the nullifier proves the miner knows the
per-block derived secret `sk_H` corresponding to the commitment's public key.
Subsequent transactions build on top. This is the same capability-exercise
pattern as every other native token operation.

### 1.3 Block Header

The `BlockHeader` structural type is defined in [type-system.md §8.2](type-system.md).

```
BlockHeader {
    version: u8,
    previous: [u8; 32],       // blake3 hash of parent block
    merkle_root: [u8; 32],    // binary blake3 Merkle root of transactions
    target: u32,              // PoW target (hash_u32 LE <= target)
    nonce: u32,               // RandomX nonce
    height: u64,              // Block height, genesis = 1
    timestamp: u64,           // Unix seconds
    uncle_merkle_root: [u8; 32],
    total_reward: u64,        // expected_reward(height) — verifiable by all nodes
    randomx_key: [u8; 32],   // derived from height: blake3(height.to_le_bytes())
    coin_merkle_root: [u8; 32],
    nullifier_root: [u8; 32], // root of nullifier SMT after this block
    anchor_tx_id: [u8; 32],  // Caribina Arweave anchor (zero if none)
}
```

### 1.4 Validation Sequence

Validators MUST verify blocks in the following order. Phases execute
sequentially — if any phase fails, the block is rejected and subsequent
phases are skipped. Cheapest checks run first.

See [Consensus](consensus/consensus.md) for the full 7-phase validation
sequence with cheat detection table.

## 2. Coinbase Production — PoWRewardV1 Nullifier Claim

### 2.1 Architecture

The coinbase reward follows the same object-capability (o-cap) pattern as
every other native token operation. The miner who finds valid PoW gains the
capability to claim the reward by publishing a nullifier against the
PoWRewardV1 commitment:

```
PoW valid → miner derives sk_H → miner computes C + nf → miner proves ZK →
miner publishes block with PoWRewardV1 at transactions[0].contract_calls[0] →
validators verify nf against nullifier SMT → reward claimed
```

This is the same pattern as FeeV1, BurnV1, SpendV1, and TransferV1:
`nullifier = poseidon_hash(secret, coin_commitment)`. The miner exercises
the coinbase capability by publishing the nullifier. The nullifier SMT
prevents double-claiming.

### 2.2 Deterministic Key Derivation

The miner MUST use a deterministic per-block key derived from their declared
identity. Random key material is forbidden — the wallet must be able to
independently derive the same key.

```
sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H.to_le_bytes())
pk_H = PublicKey::from_secret(sk_H)

derive_instance(sk, cid, data):
    cid_fp = cid.inner()          // pallas::Base
    inst   = pad32(data)          // zero-pad to 32 bytes
    inst_fp = from_repr(inst)    // interpret as field element
    return poseidon_hash([sk.inner(), cid_fp, inst_fp])
```

`NATIVE_TOKEN_CONTRACT_ID = poseidon_hash([42, 0, 4])`.

The wallet derives `sk_H` identically via `AccountManager::secrets_for_contract()`.
No shared state between miner and wallet — they compute the same hash independently.

### 2.3 Commitment

The commitment `C` is a `CoinCommitment` ([type-system.md §8.2](type-system.md)):

```
C = poseidon_hash([pk_H.x, pk_H.y, reward, DRKW_TOKEN_ID, 0, 0, blind])

where:
  pk_H.x, pk_H.y  = coordinates of per-block public key
  reward          = expected_reward(H)  (see Section 3)
  DRKW_TOKEN_ID   = pallas::Base::zero()
  blind           = fresh random per block (privacy-preserving)
```

### 2.4 Nullifier

```
nf = poseidon_hash([sk_H.inner(), C])
```

The nullifier is a linear capability — it can be exercised exactly once.
After insertion into the nullifier SMT, any duplicate `nf` is rejected
(Phase 3.2).

### 2.5 ZK Proof

The `Mint_V1` ZK circuit constrains:

| # | Constraint | What It Proves |
|---|-----------|----------------|
| 1 | `C = poseidon_hash(pk_H.x, pk_H.y, reward, DRKW_TOKEN_ID, 0, 0, blind)` | Coin attributes are correctly committed |
| 2 | `vc = pedersen_commit(reward, value_blind)` | Value commitment is correct |
| 3 | `tc = poseidon_hash(DRKW_TOKEN_ID, token_blind)` | Only native token can be minted |
| 4 | `nf = poseidon_hash(coin_secret, C)` | Miner knows `sk_H` — the per-block derived secret |
| 5 | `S_H = S_{H-1} + vc` | Cumulative supply chain invariant holds |
| 6 | `range_check(64, reward)` | Reward value fits in u64 |

Nine (9) public inputs are exposed to validators via `ZkPublicInputs<9>`:
`[C, nf, vc.x, vc.y, tc, S_H.x, S_H.y, tx_binding, tx_nonce]`.

The circuit also constrains `range_check(64, old_cumulative_value)` as a
defense-in-depth witness constraint (not a public input).

Witness (private): `sk_H`, `pk_H`, `reward`, `blind`, `value_blind`,
`token_blind`, old cumulative values.

### 2.6 WASM Entrypoint Verification

The `pow_reward_v1` WASM handler performs defense-in-depth verification:

1. Token is `DRKW_TOKEN_ID` — only native token can be minted
2. Pedersen commitment matches clear input
3. Token commitment matches clear input
4. Coin does not already exist (duplicate coin prevention)
5. Nullifier is non-zero (Phase 0 already rejects zero, this is defense-in-depth)
6. Nullifier is not already in nullifier SMT (duplicate claim prevention)
7. Reward meets or exceeds `expected_reward(H)` (emission schedule)
8. Cumulative supply invariant: `S_H = S_{H-1} + coin_value_commit`

### 2.7 Miner Obligation

The miner MUST:
- Use `sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H)` — no random keys
- Compute `C` and `nf` as specified in Sections 2.3-2.4
- Generate a `Mint_V1` ZK proof with `nf` as a public input
- Place the coinbase transaction at `transactions[0]` with `PoWRewardV1` as `contract_calls[0]`
- Publish exactly one coinbase per block

### 2.8 Validator Obligation

The validator MUST reject blocks that:
- Have no transactions or missing/misplaced PoWRewardV1 call (Phase 0)
- Fail PoW verification (Phase 1)
- Have wrong height or previous hash (Phase 2)
- Have invalid ZK proof or duplicate nullifier (Phase 3)
- Fail WASM execution (Phase 4)
- Fail transaction validation (Phase 5)
- Fail Merkle/nullifier root verification (Phase 6)

Every deviation is detectable at a specific phase. See [Consensus](consensus/consensus.md)
for the cheat detection table.

## 3. Emission Schedule

### 3.1 Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| Supply cap | 21,000,000 DRKW | Same as Bitcoin |
| Initial reward (R₀) | 1,383,764,049 base units | ~13.84 DRKW |
| Half-life (H) | 1,051,920 blocks | ~4 years at 2-min blocks |
| Tail reward (R_tail) | 79,853,981 base units | ~0.80 DRKW |
| Tail emission rate | 1% per annum | 210,000 DRKW/year |
| Block time | 120 seconds | 262,980 blocks/year |
| Genesis reward | INITIAL_REWARD | ~13.84 DRKW, height 1 |

### 3.2 Reward Function [IMPLEMENTED]

The reward function uses true exponential decay with closed-form binary exponentiation
(`fixed_pow_decay`) for deterministic, cross-platform consensus safety:

```
For h = 0: R(0) = 0 (pre-genesis)
For h ≥ 1:
    R(h) = max(R₀ × 2^(-h/H), R_tail)

where 2^(-h/H) is computed via integer binary exponentiation:
    exp = fixed_pow_decay(h, H)  // ≈ 2^(-h/H), deterministically
    R(h) = max(R₀ × exp / 2^64, R_tail)

Constants:
    R₀ = 1,383,764,049 base units (~13.84 DRKW)
    H  = 1,051,920 blocks (half-life, ~4 years at 2-min blocks)
    R_tail = 79,853,981 base units (~0.80 DRKW, 1% per annum of 21M cap)
```

This is the production-default formula — there is no feature gate. The exponential
function is implemented at [`src/sdk/src/blockchain.rs:106-137`](../../src/sdk/src/blockchain.rs)
(`expected_reward()`) using integer-only fixed-point arithmetic. Floating point
MUST NOT be used.

> **Historical note (HAZID H-C3 — RESOLVED):** A prior version of the code
> implemented a linear approximation `R(h) = R_tail + (R₀ - R_tail) × (1 - (h-1)/H)`.
> This was replaced by the closed-form exponential on 2026-07-16 (commits
> `d6c78eb5b9` and `d1e062fa8f`). The linear approximation underpaid by ~8.7×
> at the half-life.

### 3.3 Cumulative Supply Bootstrap

The cumulative supply chain `S_H` tracks the Pedersen commitment to total
minted supply at each height:

```
S_0 = PedersenIdentity (pre-genesis: total_supply=0, blind=0)
S_H = S_{H-1} + C_H  where C_H = pedersen_commit(R(H), blind_H)

At genesis (H=1):
    S_1 = identity + C_1
    total_supply = 0 + INITIAL_REWARD

The WASM contract `pow_reward_v1` enforces S_H correctness from H=2 onward.
At H=1 (genesis), the cumulative supply is bootstrapped directly into the
NativeToken contract's TOTAL_SUPPLY key during `init_genesis_contracts()`
without WASM execution. See [genesis.md](genesis.md) for the full bootstrap
specification.

### 3.3 Derivation

Initial reward from the total supply constraint:

```
∑(h=1 to ∞) max(R₀ × 2^(-h/H), R_tail) ≤ 21,000,000 × 10^8

R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
   = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
   = 1,383,764,049 base units
```

Genesis (height 1) receives INITIAL_REWARD. Height 2 is the first decay step:
`R(2) = max(R₀ × 2^(-2/H), R_tail)`.
```

Tail emission (1% per annum of 21M cap):

```
R_tail = ⌊21,000,000 × 0.01 × 10^8 / 262,980⌋
       = 79,853,981 base units
```

### 3.4 Supply Over Time

| Years after launch | Approx total supply | Annual inflation rate |
|-------------------|---------------------|----------------------|
| 20 | ~21.0M | 1.0% |
| 50 | ~27.3M | 0.77% |
| 100 | ~37.8M | 0.56% |
| 200 | ~58.8M | 0.36% |
| 500 | ~115.5M | 0.18% |

Inflation approaches zero as total supply grows. The tail maintains a minimum
security budget — it does not meaningfully inflate the supply.

## 4. Block Template Generation

### 4.1 Template Structure

Block templates are generated by `generate_linear_block_template()` at
[`bin/dwowd/src/registry/model.rs`](../../bin/dwowd/src/registry/model.rs).

```
LinearBlockTemplate {
    previous: [u8; 32],              // Previous block hash
    height: u64,                      // Block height
    target: u32,                      // PoW target
    timestamp: u64,                   // Unix seconds
    value: u64,                       // Coinbase reward = expected_reward(height)
    zk_proof: Vec<u8>,               // Mint_V1 ZK proof bytes
    zk_public_inputs: [[u8; 32]; 7], // [coin, vc.x, vc.y, tc, nf, S_H.x, S_H.y]
    coin: [u8; 32],                  // Coin commitment C
    value_commit_x: [u8; 32],        // Pedersen value commitment x
    value_commit_y: [u8; 32],        // Pedersen value commitment y
    token_commit: [u8; 32],          // Poseidon token commitment
    nullifier: [u8; 32],             // nf = poseidon_hash(sk_H.inner(), C)
    new_cumulative_x: [u8; 32],      // S_H.x
    new_cumulative_y: [u8; 32],      // S_H.y
    pow_reward_call_data: Vec<u8>,   // Serialized PoWRewardV1 contract call (0x05 + params)
    encrypted_note: Vec<u8>,         // AEAD encrypted note
    coin_merkle_root: [u8; 32],      // Coin Merkle root after including this coin
    nullifier_root: [u8; 32],        // Nullifier SMT root
    transactions: Vec<Transaction>,  // Pre-selected mempool transactions
    merkle_root: blake3::Hash,       // Merkle root of transactions (in mining blob)
}
```

### 4.2 Algorithm

1. Compute `height = current_height + 1`
2. Get `previous_hash` from latest block
3. Read current `target` from consensus
4. Compute `reward = expected_reward(height)`
5. Capture `timestamp = now()` (MUST be consistent — reused for blob + verification)
6. Build ZK coinbase via `build_linear_coinbase()`:
   - Derive `sk_H` deterministically from declared identity (MUST, not random)
   - Compute `C`, `nf`, `vc`, `tc`, `S_H` as specified in Section 2
   - Generate `Mint_V1` ZK proof with 7 public inputs
   - Build PoWRewardV1 contract call (selector `0x05` + serialized `PoWRewardParamsV1`)
   - Store `pow_reward_call_data` in template for stratum/mm_rpc miners
   - Compute `coin_merkle_root` including new coin
   - Compute `nullifier_root` from tracked nullifiers

### 4.3 Lazy Initialization

ZK proving materials are initialized on first template request, not at daemon
startup. This avoids blocking startup on proving key construction.

## 5. Uncle Merkle Consensus

### Problem

In standard PoW chains, when two miners find blocks at similar heights, one
becomes canonical and the other is orphaned — the miner wasted electricity for
nothing. This punishes smaller miners with higher latency and encourages pool
centralization.

### Solution: Obligated Pin Mechanism

The canonical chain MUST offer competing uncle chains a pin reward — a
one-time option to join and share the PoW reward.

| Uncle depth | Pin reward (% of base reward) |
|-------------|-------------------------------|
| 1 | 50% |
| 2 | 25% |
| 3 | 12.5% |
| 4+ | Geometric decay, capped at max depth |

Rules:
- Pin is use-it-or-lose-it — uncle chain accepts or rejects within a short window
- Accepting gives >0 reward, rejecting gives 0 — strictly dominated
- Not slashing — no one is punished, uncle miners gain, canonical miner keeps majority

**Invariant:** `canonical_reward + sum(uncle_rewards) = base_reward` (exactly 100%).
The coinbase split uses Pedersen commitment subtraction at the consensus level.
No new ZK proofs are needed — the split is verifiable via additive homomorphism.

```
C_base = C_effective + Σ C_uncle_i
```

The ZK circuit constrains `S_H = S_{H-1} + C_base` (total minted correctly).
Any node can recompute every blind deterministically and verify
`C_effective + Σ C_uncle_i = C_base` using only public data.

### PoWReward Function — Relationship to Uncle Split

The `PoWRewardCallBuilder` (Rust: `build_linear_coinbase()` at
`bin/dwowd/src/registry/model.rs:136`) SHALL always commit to the **full
base reward** `C_base = pedersen_commit(expected_reward(H), blind_H)` in the
Mint_V1 ZK proof. The ZK proof is constructed BEFORE the uncle split is applied.

The uncle split SHALL be applied at the **consensus layer** by
`CChainState::connect_block()` after the ZK proof is already generated:

1. `build_linear_coinbase()` — builds ZK proof committing to `C_base` (full reward)
2. `connect_block()` — subtracts `Σ C_uncle_i` from `C_base` via Pedersen
   arithmetic, producing `C_effective` for the canonical miner
3. `compute_reward()` — computes value-level split: `canonical_reward = base_reward - Σ pin_rewards`
4. `verify_uncle_split()` — enforces `canonical_value + Σ pin_rewards == base_reward` PRE-commit

The canonical miner's actual coin is `C_effective = C_base - Σ C_uncle_i`.
The cumulative supply chain SHALL accumulate `C_base` (the total minted),
NOT `C_effective`. Uncle coins `C_uncle_i` are tracked separately in
`uncle_coin_set` as Pedersen compressed points.

**Key invariant**: the miner ALWAYS proves knowledge of the full `base_reward`
in the ZK circuit. The uncle deduction happens at the consensus level, not in
the proof. This means:
- The ZK proof is independent of whether uncles exist
- The proof verifies identically for blocks with and without uncles
- The supply audit can recompute `C_uncle_i` deterministically and verify the split
- No new ZK proving key or circuit is needed for uncle blocks

This is Pareto efficient: miners are never punished for producing non-canonical
blocks, smaller miners aren't excluded from rewards, and uncle references live
in the canonical block header.

## 6. Emission Curve

```
Reward
  ^
  |  R₀ ≈ 13.84 DRKW
  |  *
  |   *
  |    *
  |     **
  |       *
  |        **
  |          ***
  |              ****
  |                   *****
  |                         ********
  |                                  **********
  |                                              ************ R_tail ≈ 0.80 DRKW
  |                                                          ~~~~~~~~~~~~~~~~~~~~~
  +-----------------------------------------------------------------------------> Height
  0         4yr         8yr        12yr       16.5yr      20yr        forever
             |           |           |           |           |
          1 half-life  2 half-life  3 half-life  tail start
```

The main emission phase runs ~16.5 years before the exponential reward drops
below the per-block tail threshold. After that, the tail takes over permanently.

## 7. Target Adjustment

### 6.1 Algorithm

```
avg_interval = sum(last_10_intervals) / (n - 1)
ratio = clamp(target_block_time / avg_interval, 0.5, 2.0)
delta = clamp(ratio - 1.0, -0.10, +0.10)
new_target = clamp(target / (1.0 + delta), min_target, max_target)
```

### 6.2 Parameters

| Parameter | Value |
|-----------|-------|
| Target block time | 120 seconds |
| Window | Rolling average of last 10 intervals (up to 20 timestamps stored) |
| Delta cap | ±10% per adjustment step |
| Ratio bound | [0.5, 2.0] |
| Target bounds | [min_target, max_target] — default [1, u32::MAX] |

## 8. MemPool Design

The mempool collects transactions with contract calls before they are included
in blocks. Source: [`bin/dwowd/src/mempool.rs`](../../bin/dwowd/src/mempool.rs).

### Data Structure

A simple `Vec<Transaction>` behind `Arc<Mutex>`. No priority queue, no size
limits, no eviction. These are intentional simplifications for testnet.

### Transaction Lifecycle

**Path A — RPC-driven mining (dev / solo):**

```
User ──submit_transaction──► mempool.add(tx)
                                  │
Miner ──mine_linear────────► generate_linear_block_template()
                                  │
                            take_all() → drain mempool into block
                                  │
                            build coinbase (ZK with nullifier)
                                  │
                            create block header, mine RandomX nonce
                                  │
                            insert_validated_block()
                                  │
                            broadcast to P2P peers
```

**Path B — Stratum mining (external xmrig):**

```
xmrig ──login──► generate_linear_block_template()
                      │
                cached in current_linear_template
                      │
                push mining.notify to all stratum clients
                      │
xmrig mines RandomX nonce on external hardware
                      │
xmrig ──submit──► verify PoW, reconstruct block
                      │
                insert_validated_block()
                      │
                generate new template, push to all clients
```

## 9. Mining Flow

1. `dwowd` generates a RandomX key for the next block template
2. Miner receives 228-byte mining blob (header with zeroed nonce) + target
3. Miner initializes RandomX VM with the key, hashes the blob with different nonces
4. If hash meets target (`hash_u32 <= target`), miner submits solved header
5. `dwowd` verifies the proof-of-work and assembles the block
6. Coinbase reward = `expected_reward(height)` paid via NativeToken::PoWRewardV1
7. If uncle, partial reward via pin mechanism (Section 4)

Target configuration:

```toml
[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds
initial_target = 16777215     # 0x00FFFFFF, easy first block (~1/256 hashes)
min_target = 1                # hardest possible
max_target = 4294967295       # u32::MAX, easiest possible
min_block_interval = 10       # seconds between blocks
```

## 10. Coinbase Reward Forwarding

Miners MAY redirect coinbase rewards to any address — a wallet, DAO, or contract
treasury — without changing the mining keypair. The recipient is changed *inside
the coinbase itself*: the `build_linear_coinbase` function takes a `MiningRecipient`
derived from the declared identity, but the forwarding destination overrides the
recipient address. Zero extra transactions, zero Merkle tree churn, zero new
consensus rules.

### How It Works

`parse_forward_destination()` handles address parsing. Empty or invalid strings
fall back to the mining address. Called from all three mining paths:

| Path | File | Behavior |
|------|------|----------|
| Built-in miner | [lib.rs](../../bin/dwowd/src/lib.rs) — `miner_task` | Checks `forward_destination` each block |
| Stratum | [stratum.rs](../../bin/dwowd/src/rpc/stratum.rs) — template generation | Overrides the login-time recipient config |
| Merge mining | [mm_rpc.rs](../../bin/dwowd/src/rpc/mm_rpc.rs) — template generation | Same as stratum |

**Zero consensus impact.** The coinbase transaction is structurally identical
regardless of recipient — same Mint_V1 ZK proof, same nullifier `nf`, same
block structure. The recipient is encrypted inside the AeadEncryptedNote. Other
nodes cannot distinguish a forwarded coinbase from a normal one.

### Key Ownership

The **destination address's keypair** is required to spend the forwarded rewards.
The mining keypair's secret is used to build the ZK proof but **cannot**
decrypt the note or spend the coins. Ensure you control the destination's keypair
before enabling forwarding.

### Configuration

```bash
FORWARD_DESTINATION="dV1abc123destaddr..."
```

Set at node startup via env var. Read once during init, stored in
`MiningState.forward_destination`, immutable after startup. No runtime API to
change it — restart required.

## 11. Mining Network Architecture

### 11.1 Three-Layer Model

The mining network operates in three layers. Every node handshakes via P2P.
Pool mining and merge mining are overlays — they add capabilities without
replacing the base layer.

**Layer 1: DarkWow P2P (mandatory)**

Every node — solo miner, pool operator, or merge miner — participates via
the P2P network. Relayer nodes handle block propagation and hostlist
discovery. Observer nodes provide chain monitoring and passive audit.
All nodes communicate via the same P2P protocol.

```
┌──────────────┐    P2P         ┌──────────────┐
│   dwowd A    │◄─────────────►│   dwowd B    │
│ (solo miner) │  hostlist     │ (pool op)    │
└──────────────┘               └──────┬───────┘
       │                              │
       │ stratum (local)              │ mm_rpc (local)
       ▼                              ▼
  ┌─────────┐                  ┌───────────┐
  │  xmrig  │                  │  p2pool   │
  └─────────┘                  └───────────┘
```

**Layer 2: Pool Mining Overlay (optional)**

A p2pool operator runs a stratum server alongside their dwowd node. Individual
miners connect xmrig to p2pool's stratum port. p2pool aggregates hashrate,
distributes mining jobs, and pays rewards via PPLNS (Pay Per Last N Shares).

**Layer 3: Merge Mining Overlay (optional)**

Monero merge mining via p2pool + monerod sidecar. See [Merge Mining](merge-mining.md).

### 11.2 Node Roles

| Role | Function |
|------|----------|
| **Miner** | Block producer — runs dwowd, mines PoW, creates coinbase |
| **Relayer** | Block propagation and hostlist discovery — essential P2P infrastructure |
| **Observer** | Chain monitoring, passive supply audit — verifies but does not mine |
| **Wallet** | Full node syncing chain, scanning for capabilities — see [Wallet](wallet.md) |

## 12. Wallet Integration — User Sovereignty

### 12.1 The Pure Function

The wallet is a pure mathematical function of its inputs:

```
WalletState = f(AccountManager, ChainBlocks)
```

See [Wallet Architecture](wallet.md), Cornerstone 2. Same keys + same chain =
identical wallet state, every time. The coinbase specification is designed so
the wallet can independently verify every claim the miner makes.

### 12.2 Deterministic Scan

The wallet scans the coinbase transaction exactly as the miner built it:

```
scan_coinbase(secrets, block):
    1. tx = block.transactions[0]
    2. call = tx.contract_calls[0]               // PoWRewardV1, function 0x05
    3. sk_H = derive_from_secrets(secrets, NATIVE_TOKEN, height)
    4. note = aead_decrypt(call.data[1..], sk_H)  // same key miner used
    5. C = poseidon_hash(pk_H.x, pk_H.y, value, DRKW_TOKEN_ID, 0, 0, blind)
    6. nf' = poseidon_hash(sk_H.inner(), C)
    7. if nf' == params.nullifier:                // defense-in-depth
           build CapRecord(coin=C, secret=sk_H, value)
```

The wallet derives the same `sk_H` as the miner — independently,
deterministically, with zero shared state. If the keys match, the note
decrypts. If the nullifier matches, the claim is valid.

### 12.3 Fee Payment Cycle

```
Wallet                          Miner
  │                               │
  │  selects DRKW capability        │
  │  builds FeeV1 ZK proof        │
  │  publishes nullifier           │
  │  ──── transaction ────────►   │
  │                               │  collects fees in coinbase
  │                               │  claims reward via PoWRewardV1 nullifier
  │                               │  can spend reward (FeeV1/BurnV1/TransferV1)
  │                               │
  │  scans block                   │
  │  detects fee nullifier ←────── │  (wallet revokes spent capability)
  │  detects new coinbase ←──────  │  (wallet discovers reward if this wallet's miner)
```

Fees flow from wallet to miner through the coinbase: the miner collects all
transaction fees in the block and adds them to the coinbase reward. The fee
payment is a capability exercise (FeeV1 nullifier) — the wallet proves it
can spend the DRKW input. The miner proves it can claim the reward (PoWRewardV1
nullifier). Both follow the same o-cap pattern.

### 12.4 User Sovereignty

The architecture is user-centric from genesis:

- **Keys never delegated.** The `AccountManager` is the single key authority.
  The wallet derives identity on boot — no key store, no daemon holding secrets.
- **Wallet as full node.** The wallet holds the complete blockchain. No RPC
  queries to a trusted server. The user verifies everything locally.
- **Pure function.** Wallet state is deterministically computable from identity
  and chain data. No hidden state, no server-side balances.
- **No premine.** Every DRKW was mined. No insider allocation, no SAFT, no
  contributor tokens. The only way to acquire DRKW is PoW.
- **Censorship resistance.** No seed node dependency, no governance knob to
  freeze funds, no ACL to gatekeep transactions.

See [What's Different from Upstream](../about/differences_from_upstream.md)
for the full comparison.

## 13. Comparison: DRKW vs BTC vs XMR

| | Bitcoin | Monero | DarkWow |
|---|---------|--------|---------|
| Supply cap | 21M (fixed) | ~18.4M + tail | 21M cap + tail |
| Halving | 4-year step | 4-year step → tail | Continuous exponential → tail |
| Premine | 0 | 0 | 0 |
| PoW | SHA-256 (ASIC) | RandomX (CPU) | RandomX (CPU) |
| Uncle rewards | No (orphaned) | No | Yes (obligated pin, 50%→) |
| Key model | User-held | User-held | User-held (AccountManager — never delegated) |
| Wallet model | Full node or SPV | Full node or light | Full node (pure function) |
| Coinbase model | Transparent UTXO | Transparent output | ZK nullifier claim (capability exercise) |

The last three rows are DarkWow's architectural differentiators. The key model
is specified in [wallet.md](wallet.md). The wallet-as-full-node design means
every user verifies the coinbase independently. The ZK nullifier claim model
means coinbase rewards follow the same privacy-preserving capability pattern
as every other transaction.

## 14. Open Questions

### Absolute Supply Under Tail

The tail means supply is technically unbounded. After 100 years the total is
~37.8M DRKW (still under 2× cap), and the annual rate is 0.56% and falling.
Whether this matters depends on whether you view the tail as a security
mechanism (intent) or an inflation source (side effect).

### Pool Centralization

CPU-friendly PoW doesn't prevent pool formation — miners still join pools for
steady payouts. Stratum centralizes block template creation. This is true of
all PoW chains and RandomX doesn't solve it.

### ASIC Risk

No PoW algorithm has remained ASIC-free indefinitely. RandomX has held since
2019 but there's no guarantee it stays that way.

### Economic Security at Tail

At ~0.80 DRKW/block tail rate, the daily security budget is ~576 DRKW/day.
Security depends on DRKW market price — if the tail value drops below the
cost of attack, the chain becomes vulnerable.

## 15. See Also

- [Consensus](consensus/consensus.md) — 7-phase validation, PoW rules, cheat detection
- [Wallet Architecture](wallet.md) — Pure function design, key sovereignty, scan pipeline
- [What's Different from Upstream](../about/differences_from_upstream.md) — Fork rationale
- [Native Token Contract](../contract/native_token.md) — Consensus-first native token contract
- [Merge Mining](merge-mining.md) — Monero merge mining architecture
- [Architecture Overview](overview.md) — Full system design
