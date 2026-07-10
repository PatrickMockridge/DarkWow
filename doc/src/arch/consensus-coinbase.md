# Consensus & Coinbase Production

*Hard specification. Normative language (MUST/SHOULD/MAY) per RFC 2119.*

This document specifies block production, coinbase reward mechanics, emission
schedule, and the nullifier claim architecture that integrates coinbase rewards
with the wallet's pure-function model. It is the canonical reference for miner
and validator behavior.

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
per-block derived secret `sk_H` corresponding to the coin's public key.
Subsequent transactions build on top. This is the same capability-exercise
pattern as every other native token operation.

### 1.3 Block Header

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

### 2.3 Coin Commitment

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

Public inputs exposed to validators: `[C, vc.x, vc.y, tc, nf, S_H.x, S_H.y]`
plus `tx_binding` and `tx_nonce`.

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
| Genesis reward | 0 | Bootstrap block, height 1 |

### 3.2 Reward Function

```
R(h) = max( R₀ × 2^(-h / H), R_tail )
  where:
    h = block height (genesis = 0 returns 0; first real reward at height 2)
    R₀ = 1,383,764,049 base units
    H  = 1,051,920 blocks
    R_tail = 79,853,981 base units
```

The function is implemented at [`src/sdk/src/blockchain.rs`](../../src/sdk/src/blockchain.rs)
using `f64::powf` which is deterministic per IEEE 754 across x86_64 and ARM64.

### 3.3 Derivation

Initial reward from the total supply constraint:

```
∑(h=2 to ∞) max(R₀ × 2^(-h/H), R_tail) ≤ 21,000,000 × 10^8

R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
   = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
   = 1,383,764,049 base units
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

## 5. Target Adjustment

### 5.1 Algorithm

```
avg_interval = sum(last_10_intervals) / (n - 1)
ratio = clamp(target_block_time / avg_interval, 0.5, 2.0)
delta = clamp(ratio - 1.0, -0.10, +0.10)
new_target = clamp(target / (1.0 + delta), min_target, max_target)
```

### 5.2 Parameters

| Parameter | Value |
|-----------|-------|
| Target block time | 120 seconds |
| Window | Rolling average of last 10 intervals (up to 20 timestamps stored) |
| Delta cap | ±10% per adjustment step |
| Ratio bound | [0.5, 2.0] |
| Target bounds | [min_target, max_target] — default [1, u32::MAX] |

## 6. Mining Network Architecture

### 6.1 Three-Layer Model

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

### 6.2 Node Roles

| Role | Function |
|------|----------|
| **Miner** | Block producer — runs dwowd, mines PoW, creates coinbase |
| **Relayer** | Block propagation and hostlist discovery — essential P2P infrastructure |
| **Observer** | Chain monitoring, passive supply audit — verifies but does not mine |
| **Wallet** | Full node syncing chain, scanning for capabilities — see [Wallet](wallet.md) |

## 7. Wallet Integration — User Sovereignty

### 7.1 The Pure Function

The wallet is a pure mathematical function of its inputs:

```
WalletState = f(AccountManager, ChainBlocks)
```

See [Wallet Architecture](wallet.md), Cornerstone 2. Same keys + same chain =
identical wallet state, every time. The coinbase specification is designed so
the wallet can independently verify every claim the miner makes.

### 7.2 Deterministic Scan

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

### 7.3 Fee Payment Cycle

```
Wallet                          Miner
  │                               │
  │  selects DRKW coin             │
  │  builds FeeV1 ZK proof        │
  │  publishes nullifier           │
  │  ──── transaction ────────►   │
  │                               │  collects fees in coinbase
  │                               │  claims reward via PoWRewardV1 nullifier
  │                               │  can spend reward (FeeV1/BurnV1/TransferV1)
  │                               │
  │  scans block                   │
  │  detects fee nullifier ←────── │  (wallet revokes spent coin)
  │  detects new coinbase ←──────  │  (wallet discovers reward if this wallet's miner)
```

Fees flow from wallet to miner through the coinbase: the miner collects all
transaction fees in the block and adds them to the coinbase reward. The fee
payment is a capability exercise (FeeV1 nullifier) — the wallet proves it
can spend the DRKW input. The miner proves it can claim the reward (PoWRewardV1
nullifier). Both follow the same o-cap pattern.

### 7.4 User Sovereignty

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

## 8. Comparison: DRKW vs BTC vs XMR

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

## 9. See Also

- [Consensus](consensus/consensus.md) — 7-phase validation, PoW rules, cheat detection
- [Wallet Architecture](wallet.md) — Pure function design, key sovereignty, scan pipeline
- [What's Different from Upstream](../about/differences_from_upstream.md) — Fork rationale
- [Native Token Contract](../contract/native_token.md) — Consensus-first native token contract
- [Merge Mining](merge-mining.md) — Monero merge mining architecture
- [Architecture Overview](overview.md) — Full system design
