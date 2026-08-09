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
| **21M DRKW reference supply** | Satoshi / Bitcoin | Tail emission onset target (~16.5y), perpetual 1% thereafter |
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

### 21M Reference Supply (Satoshi)

The 21M DRKW figure is the approximate total supply at which the main
exponential emission curve reaches the tail floor (~16.5 years after
launch). It is **not a hard cap** — tail emission adds 1% per annum
permanently after this point, ensuring a minimum security budget forever.

Supply is deterministic from genesis — there are no governance knobs, no
minting authorizations, no token-holder votes that can change issuance.

### RandomX PoW (Monero)

CPU-optimized proof-of-work. ~4 GB dataset forces memory-hard computation —
ASICs and GPUs can't get a meaningful advantage. Anyone with a consumer laptop
can mine. This keeps mining distributed rather than concentrated in industrial
farms.

### Tail Emission (Monero)

1% per annum of the 21M reference supply, permanently. This works out to 79,853,981 base
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
    anchor_tx_id: [u8; 32],       // Caribina Arweave anchor (zero if none)
    anchor_monero_height: u64,     // Monero p2pool anchor height (0 if none)
    anchor_monero_hash: [u8; 32],  // Monero p2pool anchor hash
    finality_flags: u8,            // 0x01=Caribina, 0x02=Monero, 0x04=Signaled
    pow_source: PowSource,         // Native or Monero (merge-mined)
}
```

### 1.4 Validation Sequence

Validators MUST verify blocks in the following order. Phases execute
sequentially — if any phase fails, the block is rejected and subsequent
phases are skipped. Cheapest checks run first.

See [Consensus](consensus/consensus.md) for the full 7-phase validation
sequence with cheat detection table.

## 2. Coinbase Production — PoWRewardV1 Nullifier Claim `[domain: mass_balance]`

PoWRewardV1 is the consensus-critical block-opening coinbase, part of the
Pedersen mass balance proof verified during `accept_block`. Its nominal call
data type is `MassBalanceCoinbaseV1CallData` per [type-system.md §8.2.2](../type-system.md).
Full supply audit specification: [consensus.md §Supply Audit](consensus/consensus.md).

**Process engineering context:** The coinbase is the **meter-opening event** —
it creates the coinbase UTXO at position 0 of the coin Merkle tree that carries
the block reward plus accumulated fees. This UTXO is the only legitimate source
of new monetary mass in the system. Every FeeV2 transaction within the block
pulses the fee_commit_accumulator totalizer; the coinbase establishes the
meter's zero point. See [fee-spec.md §0.1](consensus/fee-spec.md) for the full
pipe/valve/meter analogy.

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
  reward          = expected_reward(H)  (see Section 4)
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

## 3. Fee Collection Plate — FeeCollectV1 `[domain: mass_balance]` [IMPLEMENTED]

*Consensus-critical. The final transaction in every block — forwards accumulated
FeeV2 fees to the miner and closes the coin merkle tree. Nominal call data type:
`MassBalanceFeeCollectV1CallData` per [type-system.md §8.2.2](../type-system.md).*

**Process engineering context:** FeeCollectV1 is the **meter-close event** —
it verifies the `fee_commit_accumulator` totalizer matches the claimed fees,
transfers the fee pot to the miner, and resets the totalizer to Identity (zeroes
the meter) for the next block. After FeeCollectV1 executes, the accumulator
MUST be `pallas::Point::identity()`. This is the final meter reading for the
block — the Pedersen homomorphic sum of all FeeV2 commitments is checked against
the claimed total. See [fee-spec.md §0.1](consensus/fee-spec.md) for the full
pipe/valve/meter analogy, and [consensus.md §Supply Audit](consensus/consensus.md)
for the metering proof specification.

> **Status:** Specification audited and implementation verified (2026-07-17).
> Post-implementation re-audit: claims 1–22 verified against code at file:line;
> OsRng zero on consensus path; Phase 0.5 structural rules (9 tests), fee-collect
> model layout (5 tests), wallet scan discovery (1 test) all pass. Wallet
> integration test passes through reworked sequential execute_block. The claim
> nullifier follows the PoWRewardV1 model (contract-SMT-excluded, host-tracked)
> to keep fee coins spendable — see §3.7/§3.8.

### 3.1 Architecture

FeeCollectV1 (opcode `0x06`) is the "collection plate" — the mirror image of
PoWRewardV1. The coinbase at `transactions[0]` opens the coin merkle tree with
a mint operation. FeeCollectV1 at `transactions[N]` closes it with a
redistribution operation. Together they bookend every block:

```
Block Merkle Tree:
  Leaf 0:          Coinbase tx → PoWRewardV1 → opens merkle tree (mints new supply)
  Leaf 1..N-1:     User transactions (FeeV1, TransferV1, SpendV1, BurnV1, deploys)
  Leaf N:           FeeCollectV1 tx → closes merkle tree (redistributes fees)
```

PoWRewardV1 mints new supply into existence. FeeCollectV1 redistributes supply
that already exists — every fee unit flowing into `fees_db[height]` via
`apply_fee` is forwarded to the miner. Zero fees are burned or lost.

Both functions share the same capability model: the miner proves knowledge of
the per-block derived secret `sk_H` by publishing a nullifier. No public key is
ever exposed. The miner's identity is the capability to produce a valid
nullifier — same o-cap pattern as every other native token operation.

```
FeeV1 tx pays fee → fees_db[H] += fee
    ... (all txs in block) ...
FeeCollectV1: claims fees_db[H] → miner's coin, fees_db[H] = 0
```

After FeeCollectV1 executes, `fees_db[height]` is zeroed and the coin merkle
tree is closed: no further coins can be added at this height. This is the
deterministic "closing of the books" for the block.

The FeeCollectV1 transaction MUST be present iff `total_fees > 0` for this
block. It MUST be the final transaction in the block. Both rules are
consensus-enforced by Phase 0 structural validation (§3.15) — placing it
earlier would also mean later transactions' coins aren't in this block's
merkle tree, so the miner incentive aligns with the consensus rule.

### 3.2 Deterministic Key Derivation

Uses the same derivation as PoWRewardV1 (§2.2). The miner MUST use the identical
`sk_H` for both coinbase and fee collection:

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

The same `sk_H` proves both the coinbase claim (PoWRewardV1 nullifier) and the
fee-collection claim (FeeCollectV1 nullifier). The nullifiers are distinct
because the coin commitments differ: `C_coinbase ≠ C_fee` (different values,
different blinds). Same secret, different commitments → different nullifiers
→ both can coexist in the same nullifier SMT.

### 3.3 Commitment

The fee coin commitment `C_fee` is a standard `CoinCommitment`
([type-system.md §8.2](type-system.md)):

```
C_fee = poseidon_hash([
    pk_H.x,
    pk_H.y,
    total_fees,
    DRKW_TOKEN_ID,
    0,        // spend_hook — no restrictions
    0,        // user_data
    blind,
])

where:
  pk_H.x, pk_H.y  = per-block public key (same as coinbase §2.2)
  total_fees      = Σ FeeV1.fee for all FeeV1 calls in this block
  DRKW_TOKEN_ID   = pallas::Base::zero()
  blind           = deterministic, see §3.6
```

The commitment construction is identical to the coinbase commitment (§2.3)
except `total_fees` replaces `expected_reward(H)` and the domain separator for
the blind differs.

### 3.4 Nullifier

```
nf_fee = poseidon_hash([sk_H.inner(), C_fee])
```

The nullifier is the capability claim — the miner exercises the fee-collection
capability by publishing this nullifier. The nullifier SMT prevents
double-claiming: after insertion, any duplicate `nf_fee` is rejected
(Phase 3.2).

Distinct from the coinbase nullifier `nf_coinbase = poseidon_hash(sk_H.inner(),
C_coinbase)` because `C_fee ≠ C_coinbase`. Same secret key, different
commitment — the nullifier domain-separates naturally via the commitment
itself.

### 3.5 FeeCollect_V1 ZK Circuit

A dedicated ZK circuit at
[`src/contract/native_token/proof/fee_collect_v1.zk`](../../src/contract/native_token/proof/fee_collect_v1.zk).
This is NOT the Mint_V1 circuit — FeeCollectV1 does not constrain cumulative
supply because fees are redistribution, not minting.

**Circuit parameters:** `k = 11`, field = `"pallas"`.

**Constants** (same as Mint_V1):
| Constant | Type | Purpose |
|----------|------|---------|
| `VALUE_COMMIT_VALUE` | `EcFixedPointShort` | G_v for Pedersen value commitment |
| `VALUE_COMMIT_RANDOM` | `EcFixedPoint` | G_r for blinding |
| `NULLIFIER_K` | `EcFixedPointBase` | Base point for deriving public keys from secrets |

**Witnesses** (12 total):

| # | Witness | Type | Derivation |
|---|---------|------|------------|
| 1 | `coin_public_x` | `Base` | pk_H.x — miner's per-block public key |
| 2 | `coin_public_y` | `Base` | pk_H.y |
| 3 | `coin_value` | `Base` | total_fees — sum of all FeeV1 fees in block |
| 4 | `coin_token_id` | `Base` | DRKW_TOKEN_ID = 0 |
| 5 | `coin_spend_hook` | `Base` | 0 (no restrictions) |
| 6 | `coin_user_data` | `Base` | 0 |
| 7 | `coin_blind` | `Base` | poseidon_hash(sk_H, H, domain=12) |
| 8 | `coin_secret` | `Base` | sk_H.inner() — proves knowledge of derived key |
| 9 | `value_blind` | `Scalar` | poseidon_hash(sk_H, H, domain=10) |
| 10 | `token_blind` | `Base` | poseidon_hash(sk_H, H, domain=11) |
| 11 | `tx_commitment` | `Base` | 0 (fee-collect tx has no sigs) |
| 12 | `tx_nonce` | `Base` | 0 (only one fee-collect tx per block) |

**Key difference from Mint_V1:** No `old_cumulative_value`, `old_cumulative_blind`,
`new_cumulative_x`, `new_cumulative_y` witnesses. FeeCollectV1 does not touch
the cumulative supply chain.

**Constraints** (7 `constrain_instance` calls):

| # | Constraint | Public Input | What It Proves |
|---|-----------|-------------|----------------|
| C1 | `pk = ec_mul_base(coin_secret, NULLIFIER_K)` | — | Derive pk_H from sk_H |
| C2 | `constrain_equal_base(pk_x, coin_public_x)` | — | pk_H.x matches witness |
| C3 | `constrain_equal_base(pk_y, coin_public_y)` | — | pk_H.y matches witness |
| C4 | `C = poseidon_hash(pk_x, pk_y, value, token_id, spend_hook, user_data, blind)` | `C` | Fee coin attributes correctly committed |
| C5 | `nf = poseidon_hash(coin_secret, C)` | `nf` | Miner knows sk_H — valid capability claim |
| C6 | `vc = pedersen_commit(value, value_blind)` | `vc.x`, `vc.y` | Value commitment is correct |
| C7 | `tc = poseidon_hash(token_id, token_blind)` | `tc` | Token commitment is correct (DRKW enforced by entrypoint) |
| C8 | `tx_binding = poseidon_hash(tx_commitment, tx_nonce)` | `tx_binding`, `tx_nonce` | Proof bound to this transaction |
| C9 | `range_check(64, coin_value)` | — | Value fits in u64 (defense-in-depth) |

**Seven (7) public inputs:** `[C, nf, vc.x, vc.y, tc, tx_binding, tx_nonce]`.

**tx_binding construction (consensus-critical, D11):** The value declared in
`FeeCollectParamsV1.tx_binding` MUST be `poseidon_hash([tx_commitment, tx_nonce])`
— the same value the circuit constrains as public input C8. With
`tx_commitment = 0` and `tx_nonce = 0`, this is `poseidon_hash([0, 0])`, which
is NOT zero. Declaring `tx_binding = tx_commitment` (i.e. raw zero) creates a
metadata-vs-proof mismatch and verification MUST fail. The same construction is
used by PoWRewardV1 via `create_transfer_mint_proof`
([client/transfer_v1/proof.rs:282](../../src/contract/native_token/src/client/transfer_v1/proof.rs),
called at `pow_reward_v1.rs:192`; params copy at `pow_reward_v1.rs:247`).

**No cumulative supply constraint.** The circuit does NOT constrain
`S_H = S_{H-1} + C_H`. This is the defining difference from Mint_V1. Fees are
redistribution — the supply audit invariant is:

```
total_supply_after_fee_collect == total_supply_before_fee_collect
```

### 3.6 Determinism Proof

**Theorem:** For a fixed `(sk_owner, height)` and a fixed set of mempool
transactions, every validator re-executing the block produces identical
`(C_fee, nf_fee, vc, tc, proof)`. The resulting coin merkle tree root is
identical. No ambient randomness.

**Proof:** Every witness is derived from one of three sources:

| Source | Witnesses |
|--------|-----------|
| Constants | `coin_token_id = 0`, `coin_spend_hook = 0`, `coin_user_data = 0`, `tx_commitment = 0`, `tx_nonce = 0` |
| `derive_instance(sk_owner, cid, H)` | `coin_secret`, `coin_public_x`, `coin_public_y` |
| `poseidon_hash(sk_H.inner(), H, domain)` | `coin_blind` (domain=12), `value_blind` (domain=10), `token_blind` (domain=11) |
| Block tx set | `coin_value = Σ FeeV1.fee` (deterministic sum over fixed set) |

The poseidon_hash function is deterministic. The zkas circuit is deterministic.
The Pedersen commitment is deterministic. The fee total is a sum over a fixed
ordered set of transactions. Therefore the entire proof is deterministic. ∎

**Domain separator assignments:**

| Domain | Purpose | Rationale |
|--------|---------|-----------|
| 10 | `value_blind` | Fee-collection value blinding — distinct from coinbase (1) |
| 11 | `token_blind` | Fee-collection token blinding — distinct from coinbase (2) |
| 12 | `coin_blind` | Fee-collection coin blinding — distinct from coinbase (3) |
| 13 | AEAD ephemeral secret | `encrypt_deterministic()` ephemeral key — never reused across purposes |
| 14 | Proof RNG seed | Seeds the proving RNG — deterministic proof bytes (RFC 6979 pattern) |

All five are computed as `poseidon_hash([sk_H.inner(), pallas::Base::from(H),
pallas::Base::from(domain)])`.

These are the same domain indices that would be used by a coinbase at the same
height — collision is impossible because `total_fees ≠ expected_reward(H)` for
any realistic fee level, producing different poseidon outputs even with the same
domain separator.

**Consensus requirements for determinism:**

1. Proof generation MUST use a seeded RNG whose 32-byte seed is
   `poseidon_hash([sk_H.inner(), pallas::Base::from(H), pallas::Base::from(14)]).to_repr()`
   rather than `OsRng`. Deriving the proving randomness from the secret key is
   the RFC 6979 pattern — deterministic without weakening zero-knowledge.
2. AEAD note encryption MUST use `encrypt_deterministic()` with ephemeral
   secret `SecretKey::from(poseidon_hash([sk_H.inner(), pallas::Base::from(H),
   pallas::Base::from(13)]))` rather than `encrypt(&OsRng)`.
3. The fee total MUST be computed as a sum over the exact ordered set of
   transactions included in the block — the same set that produces the block's
   merkle root.

Any ambient randomness source breaks cross-validator determinism and MUST be
eliminated.

### 3.7 WASM Entrypoint Verification

The `fee_collect_v1` WASM handler at
[`src/contract/native_token/src/entrypoint/mod.rs`](../../src/contract/native_token/src/entrypoint/mod.rs)
performs defense-in-depth verification:

| # | Check | Failure |
|---|-------|---------|
| 1 | `fc.total_fees > 0` — zero-value claims rejected (kills 0-fee replay: after the pot is zeroed, a second FeeCollect claiming `total_fees = 0` would otherwise pass check #2 and mint a 0-value coin, reopening the closed tree) | `FeeTotalMismatch` |
| 2 | `fc.total_fees == fees_db[height]` — claimed total matches accumulated pot | `FeeTotalMismatch` |
| 3 | `fc.output.coin` not already in `coins_db` — no duplicate coin | `DuplicateCoin` |
| 4 | `fc.nullifier` not already in nullifier SMT — defense-in-depth against collision with a previously SPENT coin (the claim nullifier equals the future spend nullifier and SHALL NOT be in the contract SMT — see §3.8) | `DuplicateNullifier` |
| 5 | `fc.output.token_commit == poseidon_hash([0, 0])` — token is DRKW | `TokenMismatch` |

**Nullifier semantics (§3.4):** the claim nullifier `nf_fee = poseidon_hash(sk_H, C_fee)`
is the SAME value as the future spend nullifier for this coin. Inserting it into
the contract nullifiers_db would make the fee coin born-unspendable (the spend
would hit `DuplicateNullifier`). PoWRewardV1 follows the identical model
(`apply_pow_reward` calls `sparse_merkle_insert_batch(..., &[])` — empty batch,
[entrypoint/mod.rs:1154-1162](src/contract/native_token/src/entrypoint/mod.rs)).
The claim nullifier is tracked at the host level only (`tx.nullifiers`,
sled batches, in-memory cache) and is covered by the COINBASE_MATURITY gate.
Check #4 here is defense-in-depth — it catches collision with a previously
SPENT coin (same nullifier formula reused for a different height's fee coin
with the same key), not with the claim itself. The "replay" attack from the
first audit (a second FeeCollect claiming zero after pot-zero) is killed by
check #1; no SMT insertion is required.

**Execution-ordering dependency:** check #2 reads `fees_db[height]` as
accumulated by **this block's** FeeV1 calls. This requires the layer-2
sequential-visibility guarantee — canonical calls execute in block order
against one shared overlay, so the fee-collect call (final transaction) sees
every prior `apply_fee` write. See
[Execution Ordering & Atomicity Layers](consensus/consensus.md#execution-ordering--atomicity-layers).

**No signature verification.** Miner identity is proven via the nullifier
(knowledge of `sk_H`). This is the same model as PoWRewardV1 — the nullifier
IS the authentication.

The `fee_collect_get_metadata` function exposes 7 public inputs for ZK
verification: `[C, nf, vc.x, vc.y, tc, tx_binding, tx_nonce]`. No signature
public keys are returned (empty vector).

### 3.8 State Update

`apply_fee_collect` executes three state mutations atomically:

| # | Operation | Database | Effect |
|---|-----------|----------|--------|
| 1 | `coins_db[C_fee] = []` | coins | Fee coin enters UTXO set |
| 2 | `merkle_add(coin_merkle_tree, [C_fee])` | info + coin_roots | Closes coin merkle tree for this block |
| 3 | `fees_db[height] = 0` | fees | Zeros fee pot — prevents double-claim |

`FeeCollectUpdateV1` is `{coin, height, total_fees}`. The claim nullifier
SHALL NOT be inserted into the contract `nullifiers_db` — it equals the coin's
future spend nullifier `poseidon_hash(sk_H, C_fee)` and would make the fee coin
born-unspendable (the spend path hits `DuplicateNullifier` at the SMT check).
PoWRewardV1 uses the identical model: `apply_pow_reward` calls
`sparse_merkle_insert_batch(..., &[])` with an **empty batch**
([entrypoint/mod.rs:1154-1162](src/contract/native_token/src/entrypoint/mod.rs)),
so the coinbase nullifier never enters the contract SMT. Both claim nullifiers
live at the host level only.

Claim-replay prevention without contract-SMT insertion:
- Check #1 (§3.7): zero-claim rejected (`total_fees > 0` — kills the 0-fee
  re-spend after pot zero)
- Mutation #3: pot zeroed — a second claim sees `fees_db[H] = 0`
- Phase 0.5 (§3.15): at most one FeeCollectV1 call, present iff fees > 0
- Phase 6 (consensus.md): nullifier root covers the host-tracked nullifier
- Host-level nullifier tracking: the claim nullifier is published in
  `tx.nullifiers` and recorded in both the sled coin/nullifier batches and
  the in-memory cache (chain_state.rs) — this gives fee coins the same
  COINBASE_MATURITY treatment as coinbase coins (the nullifier_height check
  at chain_state.rs:947-960).

Closing the coin merkle tree is the **layer-2 integrity boundary** (the fee
release check): PoWRewardV1 opens the tree at `transactions[0]`, every
coin-creating call appends to it in block order, and FeeCollectV1 closes it
at `transactions[last]` — mutation #4 zeroing the pot is only reachable when
check #2 confirmed every fee accumulated. See
[Execution Ordering & Atomicity Layers](consensus/consensus.md#execution-ordering--atomicity-layers).

The cumulative supply state is NOT updated:

| Key | Touched? | Reason |
|-----|----------|--------|
| `TOTAL_SUPPLY` | No | Fees are redistribution, not minting |
| `CUMULATIVE_VALUE_COMMIT` | No | Supply chain unaffected |
| `CUMULATIVE_BLIND` | No | Supply chain unaffected |

### 3.9 Mass Balance Invariant

The proof-of-token-balance checker at
[`src/linear/src/proof_of_token_balance.rs`](../../src/linear/src/proof_of_token_balance.rs)
skips FeeCollectV1 calls:

```
match func {
    FeeV1 | BurnV1 | SpendV1 | TransferV1 => { /* check value conservation */ }
    PoWRewardV1 | MintV1 | FeeCollectV1 => {} // minting or redistribution
}
```

FeeCollectV1 is skipped because it redistributes existing coins — it does not
create or destroy value. The mass balance equation for a block is:

```
Σ(inputs) + coinbase_reward = Σ(outputs) + Σ(burns) + Σ(fees)
```

FeeCollectV1 moves `total_fees` from the fee pot to the miner's coin. The fees
were already accounted for in the FeeV1 transactions (`output + fee == input`).
FeeCollectV1 is a state transition within the native token contract, not a
cross-transaction value flow.

### 3.10 Supply Audit Invariant

```
total_supply_after_fee_collect == total_supply_before_fee_collect
S_H_after_fee_collect == S_H_before_fee_collect     // Pedersen chain unchanged
```

The cumulative Pedersen commitment chain `S_H` is updated only by PoWRewardV1
(which mints new supply). FeeCollectV1 does not touch the cumulative supply
state. Any node can verify this invariant by comparing `S_H` before and after
the fee-collect transaction.

### 3.11 Interaction with Uncle Coinbase Splits

Fees go entirely to the canonical miner — they are NOT split with uncle miners.
The uncle pin mechanism (§6) splits only the base coinbase reward
`expected_reward(H)`. The fee pot at `fees_db[height]` belongs to the miner who
assembles the canonical block.

**Rationale:** Uncle miners did not process transactions, validate signatures,
or verify ZK proofs. They contributed only PoW. They are compensated for that
PoW via the pin mechanism. Fee processing is a separate service provided
exclusively by the canonical miner.

### 3.12 Block Assembly

`prepare_block()` at [`bin/dwowd/src/lib.rs`](../../bin/dwowd/src/lib.rs)
assembles the block in deterministic order:

1. Build ZK coinbase (PoWRewardV1) — fallible, must succeed first
2. Collect uncles with pin rewards
3. Select mempool transactions
4. Filter immature coinbase spends (COINBASE_MATURITY soft gate)
5. Assemble coinbase transaction at position 0
6. **Sum FeeV1 fees** across all selected transactions → `total_fees`
7. **If `total_fees > 0`:** build FeeCollectV1 ZK proof using the same `sk_H` as coinbase, create fee-collect transaction, append at final position
8. Mine the block (RandomX PoW)

The fee summation MUST:

1. **Filter by contract:** only calls with
   `contract_id == NATIVE_TOKEN_CONTRACT_ID` AND selector `0x00` (FeeV1) count.
   Without the contract_id filter, any contract's call whose data begins with
   `0x00` would be miscounted as a fee.
2. **Deserialize properly:** extract the fee via `FeeParamsV1` layout
   (`[selector: u8][fee: u64 LE][FeeParamsV1]`). Malformed call data MUST be
   an error, never silently treated as zero.
3. **Use checked arithmetic:** the sum MUST use `checked_add` — overflow is an
   explicit block-preparation error, never a wrap or panic.

If the FeeCollectV1 build fails while `total_fees > 0`, block preparation
MUST fail — silently omitting fee collection violates the §3.1 MUST
(present iff `total_fees > 0`) and produces a block validators reject.

### 3.13 Edge Cases

**Zero-fee block:** If no transactions pay fees (`total_fees == 0`), no
FeeCollectV1 transaction is created. The block has transactions `[coinbase,
user_txs...]`. The merkle tree closes after the last user transaction. This
is the common case in early testnet when there are few fee-paying transactions.

**All transactions filtered:** If the immature-coinbase-spend filter removes
all mempool transactions, `total_fees = 0` and no FeeCollectV1 tx is created.

**Duplicate FeeCollectV1 attack:** A malicious miner includes two FeeCollectV1
calls in the same block. Phase 0 structural validation rejects the block
(more than one FeeCollectV1 call). Defense-in-depth at the entrypoint: the
second call claims a pot already zeroed by the first — entrypoint check #1
(`total_fees > 0`) rejects a 0-value claim, and check #2 rejects any non-zero
claim against the zeroed pot (`FeeTotalMismatch`). Without check #1, a
`total_fees = 0` claim would pass check #2 and mint a 0-value coin, reopening
the "closed" tree — this is the 0-fee replay identified in the red team audit
(finding D12).

**Wrong total attack:** A FeeCollectV1 with `total_fees ≠ fees_db[height]` is
rejected by the entrypoint. Under-claiming means the miner leaves money on the
table (no incentive). Over-claiming is prevented by the entrypoint check.

**Genesis block (height 1):** Genesis bootstraps WITHOUT WASM execution
(§4.3) — `apply_pow_reward` does not run at height 1, so it cannot create the
height-2 fee accumulator. The NativeToken `init_contract` (executed during
genesis contract deployment via `init_genesis_contracts()`) MUST therefore
seed `fees_db[2] = 0`. From height 2 onward,
`apply_pow_reward` sets `fees_db[H+1] = 0` for each block. If the key for a
height is missing, `apply_fee` and `fee_collect_v1` abort with `DbGetEmpty` —
a chain-halting failure, which is why initialization ownership must be
explicit. At height 1 itself there are no prior FeeV1 transactions, so
`total_fees = 0` and no FeeCollectV1 tx is created.

**FeeCollectV1 at wrong position:** Rejected by Phase 0 structural validation
(§3.15) — FeeCollectV1 MUST be the final transaction. The miner incentive
aligns: any coins added after it would fail to be included in that block's
merkle tree. Consensus rule and economic incentive point the same way.

**Missing FeeCollectV1 with non-zero fees:** The miner fails to claim fees
they are entitled to. The fees are stranded in `fees_db[height]` permanently —
`fee_collect_v1` only reads the pot at the CURRENT verifying height, so no
future block can claim a past height's pot. This is why the block assembler
MUST fail block preparation rather than silently omit fee collection
(§3.12), and why §3.1 makes presence-iff-fees a consensus rule.

### 3.14 Miner Obligation

The miner MUST:
- Use the same `sk_H` as the coinbase — `derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H)`
- Compute all blinds deterministically from `poseidon_hash(sk_H.inner(), H, domain_sep)`
- Use the FeeCollect_V1 ZK circuit (namespace `"FeeCollect_V1"`)
- Use seeded RNG for proof generation — `poseidon_hash(sk_H.inner(), H, domain_rng)`
- Use deterministic AEAD encryption — `encrypt_deterministic()`
- Place FeeCollectV1 as the final transaction in the block
- Include FeeCollectV1 iff `total_fees > 0`

### 3.15 Validator Obligation

The validator MUST reject blocks that:

| Rule | Phase | Status |
|------|-------|--------|
| More than one FeeCollectV1 call in the block (per-call count) | Phase 0 (structural) | IMPLEMENTED ([validation.rs:282-348](../../src/linear/src/validation.rs)) |
| FeeCollectV1 present but block's summed FeeV1 fees == 0 | Phase 0 (structural) | IMPLEMENTED (validation.rs:322-331) |
| FeeCollectV1 with non-zero fees absent | Phase 0 (structural) | IMPLEMENTED (validation.rs:327-331) |
| FeeCollectV1 not the final transaction | Phase 0 (structural) | IMPLEMENTED (validation.rs:336-344) |
| FeeCollect_V1 ZK proof fails verification | Phase 3.1 | IMPLEMENTED — proof in L1 witness via `build_fee_collect_tx` ([model.rs:386-415](../../bin/dwowd/src/registry/model.rs)), verified by L2 `decode_and_reconcile` + `verify_core_tx_with_tables` |
| Duplicate fee-collect nullifier at host level | Phase 3.2 | IMPLEMENTED — claim nullifier in `tx.nullifiers` + both sled and in-memory batches ([chain_state.rs:819-827, 911-922](../../src/linear/src/chain_state.rs)); COINBASE_MATURITY gate applies |
| WASM rejection: zero/mismatched fee total, duplicate coin, duplicate nullifier (defense-in-depth, §3.7 check #4), non-DRKW token | Phase 4 | IMPLEMENTED ([entrypoint/mod.rs:971-1007](../../src/contract/native_token/src/entrypoint/mod.rs)) |

All seven rules are enforced. The Phase 0 rules and §3.13's economic-incentive
discussion are consistent: the consensus rule and the miner incentive both put
FeeCollectV1 last.

### 3.16 Wallet Integration

The wallet discovers fee-collection coins via the same scan mechanism as
coinbase rewards (§13.2). The scan gate at
[`bin/dww/src/scan.rs`](../../bin/dww/src/scan.rs) includes selector `0x06`
alongside `0x05` (coinbase), `0x00` (FeeV1), `0x03` (TransferV1), and `0x04`
(SpendV1) in the output-discovery path. The per-block key `sk_H` is already in
`trial_secrets` from `secrets_for_contract(NATIVE_TOKEN_CONTRACT_ID, height)` —
the wallet derives it independently with zero shared state. The AEAD-encrypted
note is found by the same sliding-window decode used for every other call type;
the `FeeCollectParamsV1` layout is irrelevant to the scan. The claim nullifier
is NOT treated as a spend record (same exclusion as `0x05` for coinbase — both
nullifiers are capability claims for NEW coins, not spends of held capabilities):

```
scan_fee_collect(secrets, block):
    1. fee_tx = block.transactions[last]
    2. if fee_tx has no contract call with selector 0x06: return None
    3. sk_H = derive_from_secrets(secrets, NATIVE_TOKEN, height)
    4. note = aead_decrypt(fee_call.encrypted_note, sk_H)
    5. C_fee = poseidon_hash(pk_H.x, pk_H.y, total_fees, DRKW_TOKEN_ID, 0, 0, blind)
    6. nf' = poseidon_hash(sk_H.inner(), C_fee)
    7. if nf' == params.nullifier:
           build CapRecord(coin=C_fee, secret=sk_H, value=total_fees, nullifier=nf')
```

The miner's wallet sees an additional coin at each height where fees were
collected. The coin is spendable via the same SpendV1/TransferV1/FeeV1 paths
as any other DRKW coin. The total miner revenue at height H is:

```
miner_revenue(H) = canonical_coinbase_reward(H) + total_fees(H)
```

Both components are independently discoverable by the wallet via deterministic
scan.

### 3.17 Comparison: PoWRewardV1 vs FeeCollectV1

| Property | PoWRewardV1 (0x05) | FeeCollectV1 (0x06) |
|----------|-------------------|---------------------|
| Position | transactions[0] | transactions[last] |
| Effect on tree | Opens merkle tree | Closes merkle tree |
| Value source | Emission schedule | Accumulated fees |
| Supply effect | Mints new supply | Redistributes existing supply |
| Cumulative supply | S_H = S_{H-1} + C_H | Unchanged |
| ZK circuit | Mint_V1 (17 witnesses) | FeeCollect_V1 (12 witnesses) |
| Public inputs | 9 (includes S_H.x, S_H.y) | 7 (no cumulative supply) |
| Key derivation | derive_instance(sk_owner, cid, H) | Same |
| Nullifier model | nf = poseidon_hash(sk_H, C) | Same |
| Signature required | No (nullifier proves identity) | No |
| Wallet scan | §13.2 | §3.16 |

## 4. Emission Schedule

### 4.1 Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| Supply cap | 21,000,000 DRKW | Same as Bitcoin |
| Initial reward (R₀) | 1,383,764,049 base units | ~13.84 DRKW |
| Half-life (H) | 1,051,920 blocks | ~4 years at 2-min blocks |
| Tail reward (R_tail) | 79,853,981 base units | ~0.80 DRKW |
| Tail emission rate | 1% per annum | 210,000 DRKW/year |
| Block time | 120 seconds | 262,980 blocks/year |
| Genesis reward | INITIAL_REWARD | ~13.84 DRKW, height 1 |

### 4.2 Reward Function [IMPLEMENTED]

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
    R_tail = 79,853,981 base units (~0.80 DRKW, 1% per annum of 21M reference supply)
```

This is the production-default formula — there is no feature gate. The exponential
function is implemented at [`src/sdk/src/blockchain.rs:121`](../../src/sdk/src/blockchain.rs)
(`expected_reward()`) using integer-only fixed-point arithmetic. Floating point
MUST NOT be used.

### 4.3 Cumulative Supply Bootstrap

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

### 4.4 Derivation

Initial reward from the total supply constraint:

```
∑(h=1 to ∞) max(R₀ × 2^(-h/H), R_tail) ≤ 21,000,000 × 10^8

R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
   = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
   = 1,383,764,049 base units
```

Genesis (height 1) receives INITIAL_REWARD. Height 2 is the first decay step:
`R(2) = max(R₀ × 2^(-2/H), R_tail)`.

Tail emission (1% per annum of 21M reference supply):

```
R_tail = ⌊21,000,000 × 0.01 × 10^8 / 262,980⌋
       = 79,853,981 base units
```

### 4.5 Supply Over Time

| Years after launch | Approx total supply | Annual inflation rate |
|-------------------|---------------------|----------------------|
| 20 | ~21.0M | 1.0% |
| 50 | ~27.3M | 0.77% |
| 100 | ~37.8M | 0.56% |
| 200 | ~58.8M | 0.36% |
| 500 | ~115.5M | 0.18% |

Inflation approaches zero as total supply grows. The tail maintains a minimum
security budget — it does not meaningfully inflate the supply.

## 5. Block Template Generation

### 5.1 Template Structure

Block templates are generated by `generate_linear_block_template()` at
[`bin/dwowd/src/registry/model.rs`](../../bin/dwowd/src/registry/model.rs).

```rust
LinearBlockTemplate {
    previous: [u8; 32],              // Previous block hash
    height: u64,                      // Block height
    target: u32,                      // PoW target
    timestamp: u64,                   // Unix seconds
    value: u64,                       // Coinbase reward = expected_reward(height)
    zk_proof: Vec<u8>,               // Mint_V1 ZK proof bytes
    zk_public_inputs: [[u8; 32]; 9], // [C, nf, vc.x, vc.y, tc, S_H.x, S_H.y, tx_binding, tx_nonce]
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

### 5.2 Algorithm

1. Compute `height = current_height + 1`
2. Get `previous_hash` from latest block
3. Read current `target` from consensus
4. Compute `reward = expected_reward(height)`
5. Capture `timestamp = now()` (MUST be consistent — reused for blob + verification)
6. Build ZK coinbase via `build_linear_coinbase()`:
   - Derive `sk_H` deterministically from declared identity (MUST, not random)
   - Compute `C`, `nf`, `vc`, `tc`, `S_H` as specified in Section 2
   - Generate `Mint_V1` ZK proof with 9 public inputs
   - Build PoWRewardV1 contract call (selector `0x05` + serialized `PoWRewardParamsV1`)
   - Store `pow_reward_call_data` in template for stratum/mm_rpc miners
   - Compute `coin_merkle_root` including new coin
   - Compute `nullifier_root` from tracked nullifiers

### 5.3 Lazy Initialization

ZK proving materials are initialized on first template request, not at daemon
startup. This avoids blocking startup on proving key construction.

## 6. Uncle Merkle Consensus

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

## 7. Emission Curve

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

## 8. Target Adjustment

### 8.1 Algorithm

```
avg_interval = sum(last_10_intervals) / (n - 1)
ratio = clamp(target_block_time / avg_interval, 0.5, 2.0)
delta = clamp(ratio - 1.0, -0.10, +0.10)
new_target = clamp(target / (1.0 + delta), min_target, max_target)
```

### 8.2 Parameters

| Parameter | Value |
|-----------|-------|
| Target block time | 120 seconds |
| Window | Rolling average of last 10 intervals (up to 20 timestamps stored) |
| Delta cap | ±10% per adjustment step |
| Ratio bound | [0.5, 2.0] |
| Target bounds | [min_target, max_target] — default [1, u32::MAX] |

## 9. MemPool Design

The mempool collects transactions with contract calls before they are included
in blocks. Source: [`crates/dwow-mempool/src/lib.rs`](../../../crates/dwow-mempool/src/lib.rs).

### 9.1 Two-Tier Threshold Model

Fees are ZK-private. No one sees individual fee amounts. Instead, every
transaction carries a `FeeThreshold_V1` proof demonstrating `fee >= threshold`
without revealing `fee`. The mempool uses two tiers:

| Tier | Proof Required | Ordering | Purpose |
|------|---------------|----------|---------|
| Premium | `fee >= PREMIUM_THRESHOLD` | FIFO (arrival order) | Urgent transactions |
| General | `fee >= GENERAL_THRESHOLD` | FIFO after premium | Normal transactions |

The miner learns only `total_fees` from the contract accumulator — never
individual fee amounts. The privacy model is specified in
[fee-spec.md](fee-spec.md).

### 9.2 Admission Gate

Every transaction entering the mempool SHALL carry a valid `FeeThreshold_V1`
proof. The `FeeSignallingExtractor::verify_threshold_proof()` method (implemented by
`NativeTokenFeeSignallingExtractor`) verifies the proof against the current thresholds:

```
tx_arrives(tx):
  // Step 1: L2 witness verification (structural, existing)
  verify_single_tx(tx)  // ↓bad-proof if fails

  // Step 2: Threshold proof verification (NEW)
  if verify_threshold_proof(tx, PREMIUM_THRESHOLD):
    admit_to_premium_queue(tx)
  else if verify_threshold_proof(tx, GENERAL_THRESHOLD):
    admit_to_general_queue(tx)
  else:
    REJECT  // ↓bad-threshold-proof — fee below general threshold
```

### 9.3 Data Structure

Two `VecDeque<blake3::Hash>` FIFO queues (premium, general) alongside a
`HashMap<blake3::Hash, MempoolEntry>` for O(1) lookup. A `BTreeSet<Nullifier>`
provides O(log n) nullifier deduplication. The sled-backed persistence layer
survives restarts.

**Legacy**: A `BTreeSet<FeeIndexEntry>` ordered by fee_rate (descending) remains
for backward compatibility with FeeV1 transactions that expose clear-text fees.
This is superseded by the threshold queues for FeeV2.

### 9.4 Block Selection

`select_for_block(max_gas, max_txs)`:
1. Drain premium queue in FIFO order until `max_gas` or `max_txs` reached
2. Drain general queue in FIFO order until limits reached
3. Drain legacy fee_index (FeeV1, fee-rate ordered) until limits reached
4. Return selected transactions without removing from mempool
5. After block acceptance, `mark_mined(&tx_hashes)` removes confirmed txs

Miner configuration (`MinerConfig`) specifies `max_gas` and `max_txs` limits.

### 9.5 Transaction Lifecycle

**Path A — RPC-driven mining (dev / solo):**

```
User ──submit_transaction──► mempool.add(tx)
                                  │
                            verify_threshold_proof() — admit to premium or general queue
                                  │
Miner ──mine_linear────────► select_for_block(&config) → non-destructive tx selection
                                  │
                            build coinbase (ZK with nullifier)
                                  │
                            build_fee_collect_tx() — sums fees from V1 data + V2 commitments
                                  │
                            create block header, mine RandomX nonce
                                  │
                            insert_validated_block()
                                  │
                            mark_mined(&tx_hashes) — remove confirmed txs
                                  │
                            broadcast to P2P peers
```

**Path B — Stratum mining (external xmrig):**

```
xmrig ──login──► select_for_block(&config) → cached in current_linear_template
                      │
                push mining.notify to all stratum clients
                      │
xmrig mines RandomX nonce on external hardware
                      │
xmrig ──submit──► verify PoW, reconstruct block
                      │
                insert_validated_block()
                      │
                mark_mined(&tx_hashes)
                      │
                select_for_block(&config) → generate new template, push to all clients
```

### 9.6 FeeSignallingExtractor Trait `[domain: fee_signalling]`

The `FeeSignallingExtractor` trait (implemented by `NativeTokenFeeSignallingExtractor` in
`bin/dwowd/src/lib.rs`) bridges the mempool and consensus:

```rust
pub trait FeeSignallingExtractor: Send + Sync {
    fn extract_fee(&self, tx: &Transaction) -> u64;              // V1 clear-text
    fn estimate_gas(&self, tx: &Transaction) -> u64;              // gas accounting
    fn extract_fee_commitment(&self, tx: &Transaction) -> Option<FeeCommitment> { None }     // V2 hidden
    fn verify_threshold_proof(&self, tx: &Transaction, threshold: u64) -> bool { false }     // V2 gate
}
```

New methods carry default implementations for backward compatibility.
`FeeCommitment` wraps `pallas::Point` — a Pedersen commitment to the fee amount.

### 9.7 Threshold Values

Initial deployment uses fixed consensus constants (`PREMIUM_THRESHOLD`,
`GENERAL_THRESHOLD`). Future: fee-estimator-driven adjustment based on
observed block fullness. See [fee-spec.md §7.4](fee-spec.md).

## 10. Mining Flow

1. `dwowd` generates a RandomX key for the next block template
2. Miner receives 228-byte mining blob (header with zeroed nonce) + target
3. Miner initializes RandomX VM with the key, hashes the blob with different nonces
4. If hash meets target (`hash_u32 <= target`), miner submits solved header
5. `dwowd` verifies the proof-of-work and assembles the block
6. Coinbase reward = `expected_reward(height)` paid via NativeToken::PoWRewardV1
7. If uncle, partial reward via pin mechanism (Section 6)

Target configuration:

```toml
[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds
initial_target = 16777215     # 0x00FFFFFF, easy first block (~1/256 hashes)
min_target = 1                # hardest possible
max_target = 4294967295       # u32::MAX, easiest possible
min_block_interval = 10       # seconds between blocks
```

## 11. Coinbase Reward Forwarding

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

## 12. Mining Network Architecture

### 12.1 Three-Layer Model

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

### 12.2 Node Roles

| Role | Function |
|------|----------|
| **Miner** | Block producer — runs dwowd, mines PoW, creates coinbase |
| **Relayer** | Block propagation and hostlist discovery — essential P2P infrastructure |
| **Observer** | Chain monitoring, passive supply audit — verifies but does not mine |
| **Wallet** | Full node syncing chain, scanning for capabilities — see [Wallet](wallet.md) |

## 13. Wallet Integration — User Sovereignty

### 13.1 The Pure Function

The wallet is a pure mathematical function of its inputs:

```
WalletState = f(AccountManager, ChainBlocks)
```

See [Wallet Architecture](wallet.md), Cornerstone 2. Same keys + same chain =
identical wallet state, every time. The coinbase specification is designed so
the wallet can independently verify every claim the miner makes.

### 13.2 Deterministic Scan

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

### 13.3 Fee Payment Cycle

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

### 13.4 User Sovereignty

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

## 14. Comparison: DRKW vs BTC vs XMR

| | Bitcoin | Monero | DarkWow |
|---|---------|--------|---------|
| Supply | 21M ref + perpetual tail | ~18.4M + tail | 21M ref + perpetual tail |
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

## 15. Open Questions

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

## 16. See Also

- [Consensus](consensus/consensus.md) — 7-phase validation, PoW rules, cheat detection
- [Wallet Architecture](wallet.md) — Pure function design, key sovereignty, scan pipeline
- [What's Different from Upstream](../about/differences_from_upstream.md) — Fork rationale
- [Native Token Contract](../contract/native_token.md) — Consensus-first native token contract
- [Merge Mining](merge-mining.md) — Monero merge mining architecture
- [Architecture Overview](overview.md) — Full system design

## 17. Formal Entrypoint Specifications

*This section enumerates every check, state mutation, and failure mode for the
four consensus-critical native_token entrypoints. Each check is specified with
its exact condition, failure variant, consensus barb, and enforcement layer.
Tests SHALL be derived from these tables — not from reverse-engineering code.*

### 17.1 FeeV1 — Fee Payment Entrypoint

**Function code**: `0x00`. **ZK-gated**: YES (Fee_V2 circuit, 14 public inputs).
**Client builder**: `FeeCallBuilder` (`src/contract/native_token/src/client/fee.rs`).

FeeV1 spends an existing coin, splits it into a change output and a fee
payment. The fee is accumulated into `fees_db[height]` for later collection
by FeeCollectV1. The input coin MUST exist in the coin Merkle tree at a
committed root.

#### 17.1.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Deserialize fee | `fee = u64::from_le_bytes(params[0..8])` | `InsufficientBalance` | ↓bad-fee-amount | Primary |
| 2 | Deserialize FeeParamsV1 | `FeeParamsV1::decode(&params[8..])` | `ParseError` | ↓bad-fee-params | Primary |
| 3 | Input token is DRKW | `input.token_commit == poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | `InsufficientBalance` | ↓bad-token | Primary |
| 4 | Output token is DRKW | `output.token_commit == poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | `InsufficientBalance` | ↓bad-token | Primary |
| 5 | Minimum fee | fee ≥ two-component formula (§12.4.1): `(wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)` | `InsufficientBalance` | ↓bad-fee-amount | Primary |
| 6 | Merkle root exists | `db_contains_key(coin_roots_db, &input.merkle_root)` | `TransferMerkleRootNotFound` | ↓bad-merkle-root | Primary |
| 7 | Nullifier not spent | `SMT.get_leaf(input.nullifier) == ZERO` | `InsufficientBalance` | ↓double-spend | Primary |
| 8 | Output coin not duplicate | `!db_contains_key(coins_db, &output.coin)` | `InsufficientBalance` | ↓duplicate-coin | Defense-in-depth |
| 9 | Get verifying height | `block_height` from host context | N/A (read-only) | — | — |

**Return**: `FeeUpdateV1 { nullifier, coin, height, fee }` encoded.

#### 17.1.2 State Mutations (`apply_fee`)

Executed after successful verification. All writes are atomic within the
block overlay. Order matters: nullifier MUST be marked spent before the new
coin is added (prevents intra-block double-spend via SMT lookups).

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Mark nullifier spent | `nullifiers_db` | `input.nullifier` | `[1u8]` |
| 2 | Add output coin | `coins_db` | `output.coin` | `[]` |
| 3 | Update Merkle tree | `coin_tree` | Merkle leaf | `output.coin` (via `merkle_add`) |
| 4 | Accumulate fee | `fees_db[height]` | `height` | `fees_db[height] + fee` (saturating_add) |

#### 17.1.3 Client Build Algorithm (`FeeCallBuilder::build()`)

```
Input:  input_value, token_id, spend_hook, user_data, coin_blind,
        leaf_position, merkle_path, secret, ephemeral_signature_secret,
        recipient, output_spend_hook, output_user_data, fee_amount
        tx_commitment, tx_nonce

1.  Validate input_value > fee_amount (else InsufficientBalance)
2.  Compute output_value = input_value - fee_amount
3.  Compute input_value_blind from coin_blind
4.  Set output_value_blind = input_value_blind (Pedersen homomorphic balance)
5.  Set token_blind = BaseBlind::ZERO (DRKW token)
6.  Derive signature_public from ephemeral_signature_secret
7.  Compute tx_binding = poseidon(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)
8.  Build ZK proof (Fee_V2 circuit, 22 witnesses):
    - input_value, output_value, fee_amount (u64 range-checked)
    - input_coin, output_coin (Pedersen commitments)
    - nullifier (poseidon(DOMAIN_NULLIFIER, secret, input_coin))
    - merkle_root (computed from leaf_position + merkle_path)
    - token_commit, user_data_enc
    - signature_public, tx_binding, tx_nonce
9.  Construct Input { value_commit, token_commit, nullifier, merkle_root,
    user_data_enc, spend_hook, signature_public }
10. Construct Output { value_commit, token_commit, coin, nullifier, note }
11. Encrypt AEAD note with domain 12
12. Construct FeeParamsV1 { input, output, fee_value_blind, fee_token_blind,
    fee: fee_amount, tx_binding, tx_nonce }
13. Pack call_data: [0x00] + fee_amount.to_le_bytes() + FeeParamsV1::encode()
14. Return FeeResult { call_data, params, proofs }
```

### 17.2 FeeCollectV1 — Fee Collection Entrypoint

**Function code**: `0x06`. **ZK-gated**: YES (FeeCollect_V2 circuit, 7 public inputs).
**Client builder**: `FeeCollectCallBuilder` (`src/contract/native_token/src/client/fee_collect.rs`).

FeeCollectV1 claims the accumulated fee pot `fees_db[height]` and mints a
new coin to the miner. It closes the coin Merkle tree for the block. The
transaction MUST be the final transaction in the block (Phase 0 enforcement).

#### 17.2.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Non-zero claim | `fc.total_fees > 0` | `InsufficientBalance` | ↓zero-claim | Primary |
| 2 | Claim matches accumulated | `fc.total_fees == fees_db[height]` | `InsufficientBalance` | ↓bad-claim | Primary |
| 3 | Coin not duplicate | `!db_contains_key(coins_db, &fc.output.coin)` | `InsufficientBalance` | ↓duplicate-coin | Defense-in-depth |
| 4 | Nullifier not spent | `SMT.get_leaf(fc.output.nullifier) == ZERO` | `InsufficientBalance` | ↓double-spend | Defense-in-depth |
| 5 | Token is DRKW | `fc.output.token_commit == poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | `InsufficientBalance` | ↓bad-token | Primary |

**Return**: `FeeCollectUpdateV1 { coin, height, total_fees }` encoded.

#### 17.2.2 State Mutations (`apply_fee_collect`)

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Add fee coin | `coins_db` | `fc.output.coin` | `[]` |
| 2 | Update Merkle tree | `coin_tree` | Merkle leaf | `fc.output.coin` (via `merkle_add`, closes tree) |
| 3 | Zero fee pot | `fees_db[height]` | `height` | `0` (prevents double-claim) |

#### 17.2.3 Client Build Algorithm (`FeeCollectCallBuilder::build()`)

```
Input:  recipient (MiningRecipient), fee_txs (Vec<Transaction>), height

1.  Sum fees from fee_txs:
    for each tx in fee_txs:
      for each call where contract_id == NATIVE_TOKEN_CONTRACT_ID
                      and call.data[0] == 0x00:
        fee = u64::from_le_bytes(call.data[1..9])
        total_fees = total_fees.checked_add(fee)?  // overflow = block error
    if total_fees == 0: return None  // no FeeCollectV1 needed
2.  Derive sk_H = mining secret key at height (deterministic)
3.  Compute value_blind = ScalarBlind(poseidon([sk_H, height, domain=10]))
4.  Compute coin = poseidon(DOMAIN_COIN_COMMIT, pk.x, pk.y, total_fees,
    DRKW_TOKEN_ID, spend_hook=0, user_data=0, value_blind)
5.  Compute nullifier = poseidon(DOMAIN_NULLIFIER, sk_H, coin)
6.  Build ZK proof (FeeCollect_V2 circuit, 12 witnesses, 7 constraints)
7.  Encrypt AEAD note with domain 13 (deterministic: RNG seeded from
    poseidon([sk_H, height, domain=14]))
8.  Construct FeeCollectParamsV1 { output: Output { value_commit, token_commit,
    coin, nullifier, note }, total_fees, value_blind }
9.  Pack call_data: [0x06] + FeeCollectParamsV1::encode()
10. Return Transaction { contract_calls: [call], witness: [proof], nullifiers: [nullifier] }
```

### 17.3 PoWRewardV1 — Coinbase Reward Entrypoint

**Function code**: `0x05`. **ZK-gated**: YES (Mint_V2 circuit, 9 public inputs).
**Client builder**: `build_linear_coinbase()` (`bin/dwowd/src/registry/model.rs`).

PoWRewardV1 mints the block reward to the miner. It is ALWAYS at
`transactions[0]` (Phase 0 enforcement) and opens the coin Merkle tree for
the block.

#### 17.3.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Deserialize params | `PoWRewardParamsV1::decode(params)` | `ParseError` | ↓bad-params | Primary |
| 2 | Token is DRKW (clear) | `pr.input.token_id == DRKW_TOKEN_ID` (0) | `InsufficientBalance` | ↓bad-token | Primary |
| 3 | Value commit matches | `pedersen_commit(value, value_blind) == pr.output.value_commit` | `InsufficientBalance` | ↓bad-commit | Primary |
| 4 | Token commit matches | `poseidon(DOMAIN_TOKEN_COMMIT, token_id, token_blind) == pr.output.token_commit` | `InsufficientBalance` | ↓bad-commit | Primary |
| 5 | Coin not duplicate | `!db_contains_key(coins_db, &pr.output.coin)` | `InsufficientBalance` | ↓duplicate-coin | Primary |
| 6 | Nullifier non-zero | `pr.output.nullifier != Nullifier::zero()` | `InsufficientBalance` | ↓bad-nullifier | Defense-in-depth |
| 7 | Nullifier not in SMT | `SMT.get_leaf(pr.output.nullifier) == ZERO` | `InsufficientBalance` | ↓double-spend | Defense-in-depth |
| 8 | Exact reward | `pr.input.value == expected_reward(height)` | `InsufficientBalance` | ↓bad-reward | Primary |
| 9 | Supply check | `current_supply + reward == pr.expected_cumulative_supply` | `InsufficientBalance` | ↓bad-supply | Primary |
| 10 | Old cumulative commit | `pr.current_cumulative_commit == CUMULATIVE_VALUE_COMMIT` | `InsufficientBalance` | ↓bad-supply | Primary |
| 11 | Old cumulative blind | `pr.current_blind == CUMULATIVE_BLIND` (skip if supply=0) | `InsufficientBalance` | ↓bad-supply | Primary |
| 12 | Blind continuity | `new_blind = old_blind + value_blind` | (computed, not checked against input) | — | — |
| 13 | New cumulative commit | `pedersen_add(old_commit, value_commit) == pr.new_cumulative_commit` | `InsufficientBalance` | ↓bad-supply | Primary |

**Return**: `PoWRewardUpdateV1 { coin, height, new_total_supply, cumulative_value_commit, aggregate_blind }` encoded.

#### 17.3.2 State Mutations (`apply_pow_reward`)

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Seed next fee pot | `fees_db[height+1]` | `height+1` | `0` |
| 2 | Update nullifier snapshot | `nullifiers_db` | (batch) | empty (claim nullifier EXCLUDED) |
| 3 | Record total supply | `info_db` | `TOTAL_SUPPLY` | `new_total_supply` (u64 LE) |
| 4 | Record cumulative commit | `info_db` | `CUMULATIVE_VALUE_COMMIT` | `cumulative_value_commit` (Pedersen point) |
| 5 | Record cumulative blind | `info_db` | `CUMULATIVE_BLIND` | `aggregate_blind` (pallas::Scalar) |
| 6 | Add coin | `coins_db` | `output.coin` | `[]` |
| 7 | Update Merkle tree | `coin_tree` | Merkle leaf | `output.coin` (via `merkle_add`, opens tree) |

### 17.4 Claim-Nullifier Exclusion Invariant

Both PoWRewardV1 and FeeCollectV1 produce a nullifier that is tracked by
the host (`tx.nullifiers`, sled batches, in-memory `nullifier_set`) but NOT
inserted into the contract's `nullifiers_db` SMT.

**Rationale**: The claim nullifier `nf_claim` equals the spend nullifier
`nf_spend` for the same coin (both are `poseidon(DOMAIN_NULLIFIER, sk_H, coin)`).
If the claim nullifier were inserted into the contract SMT, the coin would be
born-unspendable — the first FeeV1/TransferV1/SpendV1 attempt would hit a
"nullifier already spent" SMT lookup.

**Replay prevention** (no contract SMT, so no SMT-based rejection):
1. **Zero-claim rejection**: `total_fees == 0` → block rejected (kills D12 replay)
2. **Pot zeroing**: `fees_db[height] = 0` after collection (prevents double-claim)
3. **Phase 0 structural rules**: FeeCollectV1 MUST be final transaction, MUST
   have `0x06` selector, call_data >= 9 bytes
4. **Host-level COINBASE_MATURITY gate**: host tracks claim nullifiers in
   `tx.nullifiers` → sled batches → `connect_block` checks
   `height - created_at >= COINBASE_MATURITY` (100 blocks)
5. **In-memory pruning**: `nullifier_set` entries older than COINBASE_MATURITY
   are pruned from the host cache after each block

### 17.5 COINBASE_MATURITY

```
COINBASE_MATURITY = 100 blocks  (src/linear/src/lib.rs:56)
```

Every non-coinbase nullifier in `tx.nullifiers` is checked at `connect_block`:

```rust
let created_at = self.nullifier_height(&nullifier)?;
if height.saturating_sub(created_at) < COINBASE_MATURITY {
    return Err("Immature coinbase spend");
}
```

This applies to: PoWRewardV1 nullifiers, FeeCollectV1 nullifiers, and any
spend nullifier from a coin created by PoWRewardV1 or FeeCollectV1.

The host tracks nullifiers in three synchronized stores:
- Per-block `tx.nullifiers` (inserted at block assembly)
- Sled `store.nullifiers` batch (persisted atomically with the block)
- In-memory `nullifier_set: BTreeMap<Nullifier, BlockHeight>` (for fast lookup)

After each block, entries where `height - created_at >= COINBASE_MATURITY` are
pruned from the in-memory cache (they remain in sled for historical queries).

### 17.6 Constants

| Constant | Value | Location |
|----------|-------|----------|
| `BASELINE_STORAGE` | `1_000_000` (0.01 DRKW/kB at CF=1.0) | `src/linear/src/fee_window.rs` |
| `COINBASE_MATURITY` | `100` blocks | `src/linear/src/lib.rs:56` |
| `INITIAL_REWARD` | `1_383_764_049` (1.383 DRKW) | `src/sdk/src/blockchain.rs:606` |
| `FeeV1` selector | `0x00` | `src/contract/native_token/src/lib.rs:60` |
| `FeeCollectV1` selector | `0x06` | `src/contract/native_token/src/lib.rs:66` |
| `PoWRewardV1` selector | `0x05` | `src/contract/native_token/src/lib.rs:65` |
| DRKW token ID | `0` | `src/contract/native_token/src/lib.rs` |

### 17.7 Error Taxonomy

Every entrypoint failure maps to a consensus barb. Tests SHALL assert the
specific barb, not generic "canonical call failed at exec."

| Entrypoint | Failure Variant | Consensus Barb | Rejection Level |
|-----------|----------------|----------------|-----------------|
| FeeV1 | `InsufficientBalance` (fee below two-component threshold) | ↓bad-fee-amount | Block rejected |
| FeeV1 | `InsufficientBalance` (input <= fee) | ↓bad-fee-amount | Block rejected |
| FeeV1 | `TransferMerkleRootNotFound` | ↓bad-merkle-root | Block rejected |
| FeeV1 | `InsufficientBalance` (nullifier spent) | ↓double-spend | Block rejected |
| FeeV1 | `ParseError` | ↓bad-fee-params | Block rejected |
| FeeCollectV1 | `InsufficientBalance` (total_fees == 0) | ↓zero-claim | Block rejected |
| FeeCollectV1 | `InsufficientBalance` (claim != accumulated) | ↓bad-claim | Block rejected |
| FeeCollectV1 | `InsufficientBalance` (wrong token) | ↓bad-token | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (reward != expected) | ↓bad-reward | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (supply mismatch) | ↓bad-supply | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (duplicate coin) | ↓duplicate-coin | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (nullifier spent) | ↓double-spend | Block rejected |

Tests SHALL use INFRA-FAIL prefix for block-proof violations (any of the
above) because they represent consensus-level rejection — not contract-
specific logic errors.
