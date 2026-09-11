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
| **PoWRewardV1 nullifier claim** | Novel (this fork) | Plaintext capability-exercise coinbase — single path, miner/wallet symmetry |

The chassis is boring on purpose. Satoshi's supply model and Monero's mining model
have worked for a combined 30+ years. The novel pieces — the nullifier-claim
coinbase and Uncle Merkle — are the minimum necessary innovation to achieve
deterministic, user-verifiable coinbase rewards.

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

The PoWRewardV1 nullifier is the first entry in the host-tracked nullifier
set for this block (the header's `nullifier_root` is a blake3 root over this
set — not an SMT, see §1.3). It "unlocks" the block — the nullifier proves the miner knows the
per-block derived secret `sk_H` corresponding to the commitment's public key.
Subsequent transactions build on top. This is the same capability-exercise
pattern as every other native token operation.

### 1.3 Block Header

The `BlockHeader` structural type is defined in [type-system.md §8.2](type-system.md).

```
BlockHeader {
    version: BlockVersion,
    previous: [u8; 32],              // blake3 hash of parent block
    merkle_root: [u8; 32],           // binary blake3 Merkle root of transactions
    target: BlockTarget,             // PoW target (hash_u32 LE <= target)
    nonce: u32,                      // RandomX nonce
    height: BlockHeight,             // Block height, genesis = 1
    timestamp: BlockTimestamp,       // Unix seconds
    uncle_merkle_root: [u8; 32],
    total_reward: BlockReward,       // expected_reward(height) — verifiable by all nodes
    randomx_key: [u8; 32],          // derived from height: blake3(height.to_le_bytes())
    miner: [u8; 32],               // miner's cycled per-block address (pk_H = derive_instance(..).public()); uncle-note encryption target
    commitment_merkle_root: [u8; 32],
    nullifier_root: [u8; 32],        // blake3 root over the block's nullifier set (not an SMT)
    anchor_tx_id: [u8; 32],          // Caribina Arweave anchor (zero if none)
    anchor_monero_height: MoneroBlockHeight,  // Monero p2pool anchor height (0 if none)
    anchor_monero_hash: [u8; 32],    // Monero p2pool anchor hash
    finality_flags: FinalityFlags,   // 0x01=Caribina, 0x02=Monero, 0x04=Signaled
    fee_window_flags: FeeWindowFlags,
    pow_source: PowSource,           // Native or Monero (merge-mined)
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
data type is `MassBalanceCoinbaseV1CallData` per [type-system.md §8.2.2](type-system.md).
Full supply audit specification: [consensus.md §Supply Audit](consensus/consensus.md).

**Process engineering context:** The coinbase is the **meter-opening event** —
it creates the coinbase UTXO at position 0 of the commitment Merkle tree that carries
the block reward (reward only — fees are minted separately by FeeCollectV1, §17.2).
This UTXO is the only legitimate source
of new monetary mass in the system. Every FeeV2 transaction within the block
accumulates its plaintext fee into `fees_db[height]`; the coinbase establishes the
meter's zero point (`fees_db[height+1] = 0`, §17.3.2). See [fee-spec.md §0.1](consensus/fee-spec.md)
for the full pipe/valve/meter analogy.

### 2.1 Architecture

The coinbase reward follows the same object-capability (o-cap) pattern as
every other native token operation. The miner who finds valid PoW gains the
capability to claim the reward by publishing a nullifier against the
PoWRewardV1 commitment:

```
PoW valid → miner derives sk_H → miner computes C + nf + vc + S_H (plaintext) →
miner publishes block with PoWRewardV1 at transactions[0].contract_calls[0] →
validators verify nf / vc / S_H via plaintext Pedersen arithmetic → reward claimed
```

This is the same pattern as FeeV2, BurnV1, SpendV1, and TransferV1:
`nullifier = poseidon_hash(secret, coin_commitment)`. The miner exercises
the coinbase capability by publishing the nullifier. The host-tracked
nullifier set prevents double-claiming.

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

**Privacy model — per-block address cycling.** `sk_H`/`pk_H` is derived fresh for
every height, so each block's reward address is a **cycled per-block address** —
unlinkable across blocks. The block header's `miner` field (uncle_merkle.md
§"Miner identity in the header") publishes this cycled `pk_H` (not the stable
`sk_owner`/master key), preserving miner unlinkability. A stable master identity
SHALL NOT appear in any block.

### 2.3 Commitment

The commitment `C` is a `Commitment` ([type-system.md §8.2](type-system.md)):

```
C = poseidon_hash([pk_H.x, pk_H.y, effective_value, DRKW_ASSET_ID, 0, 0, blind])

where:
  pk_H.x, pk_H.y  = coordinates of per-block public key
  effective_value = expected_reward(H) − Σ pin_confirmed_i  (see §6 — reduced by uncle pins)
  DRKW_ASSET_ID   = pallas::Base::zero()
  blind           = fresh random per block (privacy-preserving)
```

The spendable note commits to the **reduced** `effective_value`; the Pedersen
value commitment (§2.5) still commits to the **full** `expected_reward(H)` so
the cumulative supply chain `S_H` accumulates the total emission. When no uncles
are present, `effective_value == expected_reward(H)`.

### 2.4 Nullifier

```
nf = poseidon_hash([sk_H.inner(), C])
```

The nullifier is a linear capability — it can be exercised exactly once.
After insertion into the host nullifier set, any duplicate `nf` is rejected
(Phase 3.2).

### 2.5 Plaintext Verification (no ZK circuit)

PoW rewards are **plaintext**: the reward value is public (`expected_reward(H)`,
`effective_value`), so the coinbase needs no ZK circuit. Every invariant is
verified by the WASM entrypoint in plaintext Pedersen/poseidon arithmetic:

| # | Check | What It Enforces |
|---|-------|------------------|
| 1 | `C = poseidon_hash(pk_H.x, pk_H.y, effective_value, DRKW_ASSET_ID, 0, 0, blind)` | Note commits to the reduced spendable value |
| 2 | `vc = pedersen_commit(reward, value_blind)` | Value commitment is correct (full base) |
| 3 | `tc = poseidon_hash(DRKW_ASSET_ID, token_blind)` | Only native token can be minted |
| 4 | `nf = poseidon_hash(spend_secret, C)` (deterministic, wallet-recomputed) | Capability claim (re-verified at spend) |
| 5 | `S_H = S_{H-1} + vc` (`old_cumulative + value_commit`, Pedersen add) | Cumulative supply chain invariant |

The `Mint_V2` ZK circuit is **NOT used** for the coinbase. It survives only for
TransferV1/SpendV1 output minting, where the value genuinely stays hidden. The
"miner knows `sk_H`" property the circuit used to prove is instead enforced at
spend time by the SpendV1 proof — a miner who publishes a wrong nullifier simply
makes their own coinbase unspendable, with no consensus risk.

### 2.6 WASM Entrypoint Verification

The `pow_reward_v1` WASM handler performs defense-in-depth verification:

1. Token is `DRKW_ASSET_ID` — only native token can be minted
2. Pedersen commitment matches clear input
3. Token commitment matches clear input
4. Commitment does not already exist (duplicate commitment prevention)
5. Nullifier is non-zero (Phase 0 already rejects zero, this is defense-in-depth)
6. Nullifier is not already in the host nullifier set (duplicate claim prevention)
7. Reward meets or exceeds `expected_reward(H)` (emission schedule)
8. Cumulative supply invariant: `S_H = S_{H-1} + value_commit`

### 2.7 Miner Obligation

The miner MUST:
- Use `sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H)` — no random keys
- Compute `C` and `nf` as specified in Sections 2.3-2.4 (deterministic, plaintext)
- Compute `vc`, `tc`, `S_H` in plaintext Pedersen/poseidon arithmetic — no ZK proof
- Place the coinbase transaction at `transactions[0]` with `PoWRewardV1` as `contract_calls[0]`
- Publish exactly one coinbase per block

### 2.8 Validator Obligation

The validator MUST reject blocks that:
- Have no transactions or missing/misplaced PoWRewardV1 call (Phase 0)
- Fail PoW verification (Phase 1)
- Have wrong height or previous hash (Phase 2)
- Have invalid plaintext commitment / reward / supply, or duplicate nullifier (Phase 3)
- Fail WASM execution (Phase 4)
- Fail transaction validation (Phase 5)
- Fail Merkle/nullifier root verification (Phase 6)

Every deviation is detectable at a specific phase. See [Consensus](consensus/consensus.md)
for the cheat detection table.

## 3. Fee Collection Plate — FeeCollectV1 `[domain: mass_balance]` [IMPLEMENTED]

*Consensus-critical. The final transaction in every block — forwards accumulated
FeeV2 fees to the miner and closes the commitment merkle tree. Nominal call data type:
`MassBalanceFeeCollectV1CallData` per [type-system.md §8.2.2](type-system.md).*

**Process engineering context:** FeeCollectV1 is the **meter-close event** —
it verifies the claimed total matches the plaintext pot (`total_fees ==
fees_db[height]`, plain u64 — no Pedersen accumulator), transfers the fee pot
to the miner, and zeroes the pot for the next block. After FeeCollectV1
executes, `fees_db[height]` MUST be `0`. This is the final meter reading for
the block — the plain sum of all FeeV2 fees is checked against the claimed
total. See [fee-spec.md §0.1](consensus/fee-spec.md) for the full
pipe/valve/meter analogy, and [consensus.md §Supply Audit](consensus/consensus.md)
for the metering proof specification.

> **Status:** Specification audited and implementation verified (2026-07-17).
> Post-implementation re-audit: claims 1–22 verified against code at file:line;
> OsRng zero on consensus path; Phase 0.5 structural rules (9 tests), fee-collect
> model layout (5 tests), wallet scan discovery (1 test) all pass. Wallet
> integration test passes through reworked sequential execute_block. The claim
> nullifier follows the PoWRewardV1 model (contract-SMT-excluded, host-tracked)
> to keep fee commitments spendable — see §3.7/§3.8.

### 3.1 Architecture

FeeCollectV1 (opcode `0x06`) is the "collection plate" — the mirror image of
PoWRewardV1. The coinbase at `transactions[0]` opens the commitment merkle tree with
a mint operation. FeeCollectV1 at `transactions[N]` closes it with a
redistribution operation. Together they bookend every block:

```
Block Merkle Tree:
  Leaf 0:          Coinbase tx → PoWRewardV1 → opens merkle tree (mints new supply)
  Leaf 1..N-1:     User transactions (FeeV2, TransferV1, SpendV1, BurnV1, deploys)
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
FeeV2 tx pays fee → fees_db[H] += fee
    ... (all txs in block) ...
FeeCollectV1: claims fees_db[H] → miner's commitment, fees_db[H] = 0
```

After FeeCollectV1 executes, `fees_db[height]` is zeroed and the commitment merkle
tree is closed: no further commitments can be added at this height. This is the
deterministic "closing of the books" for the block.

The FeeCollectV1 transaction MUST be present iff `total_fees > 0` for this
block. It MUST be the final transaction in the block. Both rules are
consensus-enforced by Phase 0 structural validation (§3.15) — placing it
earlier would also mean later transactions' commitments aren't in this block's
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
→ both can coexist in the same host nullifier set.

### 3.3 Commitment

The fee commitment commitment `C_fee` is a standard `Commitment`
([type-system.md §8.2](type-system.md)):

```
C_fee = poseidon_hash([
    pk_H.x,
    pk_H.y,
    total_fees,
    DRKW_ASSET_ID,
    0,        // spend_hook — no restrictions
    0,        // user_data
    blind,
])

where:
  pk_H.x, pk_H.y  = per-block public key (same as coinbase §2.2)
  total_fees      = Σ FeeV2 fee for all FeeV2 calls in this block
  DRKW_ASSET_ID   = pallas::Base::zero()
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
capability by publishing this nullifier. The host-tracked nullifier set
prevents double-claiming: after insertion, any duplicate `nf_fee` is rejected
(Phase 3.2).

Distinct from the coinbase nullifier `nf_coinbase = poseidon_hash(sk_H.inner(),
C_coinbase)` because `C_fee ≠ C_coinbase`. Same secret key, different
commitment — the nullifier domain-separates naturally via the commitment
itself.

### 3.5 Plaintext Verification (no ZK circuit)

Since 2026-09, FeeCollectV1 carries NO ZK proof. The FeeCollect_V2 circuit
(`src/contract/native_token/proof/fee_collect.zk`) is deleted: fees are
plaintext amounts by design — they are dynamic, transaction-specific values,
and encrypting them proved too complex. The miner's obligations collapse to
the commitments the AEAD-note machinery already produces:

- **Fee commitment** `C_fee` = poseidon_hash(pk_H.x, pk_H.y, total_fees,
  DRKW_ASSET_ID, 0, 0, commitment_blind) — published in the plaintext
  `FeeCollectParamsV1` (fee-spec.md §3.8).
- **Nullifier** `nf_fee` = poseidon_hash(sk_H.inner(), C_fee) — the miner's
  capability claim over the fee commitment.
- **Value commitment** `vc` = pedersen_commit(total_fees, value_blind) —
  value_blind = poseidon_hash(sk_H, H, domain=10).
- **Token commitment** `tc` = poseidon_hash([DOMAIN_TOKEN_COMMIT,
  DRKW_ASSET_ID, 0]) — token_blind is fixed at ZERO (native DRKW only).

**tx_binding:** `FeeCollectParamsV1.tx_binding` remains
`poseidon_hash([tx_commitment, tx_nonce])` — a metadata field, no longer a
proof binding. With `tx_commitment = 0` and `tx_nonce = 0` this is
`poseidon_hash([0, 0])`, which is NOT zero.

**No cumulative supply constraint.** FeeCollectV1 does not constrain
`S_H = S_{H-1} + C_H`. Fees are redistribution — the supply audit invariant
is:

```
total_supply_after_fee_collect == total_supply_before_fee_collect
```

**Enforcement is the WASM entrypoint alone** (fee-spec.md §3.9). The L2
proof-metadata tables still carry one empty per-call proof slot for
wire-shape compatibility (see §3.7).

### 3.6 Determinism Proof

**Theorem:** For a fixed `(sk_owner, height)` and a fixed set of mempool
transactions, every validator re-executing the block produces identical
`(C_fee, nf_fee, vc, tc)`. The resulting commitment merkle tree root is
identical. No ambient randomness.

**Proof:** Every input is derived from one of three sources:

| Source | Inputs |
|--------|--------|
| Constants | `asset_id = 0`, `coin_spend_hook = 0`, `user_data = 0`, `token_blind = 0`, `tx_commitment = 0`, `tx_nonce = 0` |
| `derive_instance(sk_owner, cid, H)` | `spend_secret`, `public_x`, `public_y` |
| `poseidon_hash(sk_H.inner(), H, domain)` | `commitment_blind` (domain=12), `value_blind` (domain=10) |
| Block tx set | `value = Σ FeeV2 fee` (deterministic sum over fixed set) |

The poseidon_hash function is deterministic. The Pedersen commitment is
deterministic. The fee total is a sum over a fixed ordered set of
transactions. Therefore the entire output is deterministic. ∎

**Domain separator assignments:**

| Domain | Purpose | Rationale |
|--------|---------|-----------|
| 10 | `value_blind` | Fee-collection value blinding — distinct from coinbase (1) |
| 12 | `commitment_blind` | Fee-collection coin blinding — distinct from coinbase (3) |
| 13 | AEAD ephemeral secret | `encrypt_deterministic()` ephemeral key — never reused across purposes |

All three are computed as `poseidon_hash([sk_H.inner(), pallas::Base::from(H),
pallas::Base::from(domain)])`. Domain 14 (proof RNG seed) is retired with the
circuit.

`token_blind` is fixed at ZERO — the fee commitment is native DRKW only
(fee-spec.md §4.2 C5). Blinds are domain-separated from the coinbase's
(1/2/3), and collision is additionally impossible because
`total_fees ≠ expected_reward(H)` for any realistic fee level, producing
different poseidon outputs even with a shared domain separator.

**Consensus requirements for determinism:**

1. AEAD note encryption MUST use `encrypt_deterministic()` with ephemeral
   secret `SecretKey::from(poseidon_hash([sk_H.inner(), pallas::Base::from(H),
   pallas::Base::from(13)]))` rather than `encrypt(&OsRng)`.
2. The fee total MUST be computed as a sum over the exact ordered set of
   transactions included in the block — the same set that produces the block's
   merkle root.

Any ambient randomness source breaks cross-validator determinism and MUST be
eliminated.

### 3.7 WASM Entrypoint Verification

The `fee_collect_v1` WASM handler at
[`src/contract/native_token/src/entrypoint/mod.rs`](../../../src/contract/native_token/src/entrypoint/mod.rs)
performs defense-in-depth verification:

| # | Check | Failure |
|---|-------|---------|
| 1 | `fc.total_fees > 0` — zero-value claims rejected (kills 0-fee replay: after the pot is zeroed, a second FeeCollect claiming `total_fees = 0` would otherwise pass check #2 and mint a 0-value commitment, reopening the closed tree) | `ZeroFeeClaim` |
| 2 | `fc.total_fees == fees_db[height]` — claimed total matches the accumulated pot (plain u64 since 2026-09; the Pedersen fee accumulator is removed — entrypoint/mod.rs:1201-1217) | `FeeTotalMismatch` |
| 3 | `fc.output.commitment` not already in `commitment_set` — no duplicate commitment | `DuplicateCommitment` |
| 4 | `fc.output.token_commit == poseidon_hash([DOMAIN_TOKEN_COMMIT, 0, 0])` — token is DRKW | `TokenMismatch` |

There is **no** contract-level nullifier SMT check — the claim nullifier equals the
future spend nullifier and SHALL NOT be in the contract SMT (see §3.8). Replay
prevention is host-level (§17.5): zero-claim rejection, pot zeroing, and the
COINBASE_MATURITY gate over host-tracked claim nullifiers.

**Nullifier semantics (§3.4):** the claim nullifier `nf_fee = poseidon_hash(sk_H, C_fee)`
is the SAME value as the future spend nullifier for this commitment. Inserting it into
the contract nullifiers_db would make the fee commitment born-unspendable (the spend
would hit `DuplicateNullifier`). PoWRewardV1 follows the identical model
(`apply_pow_reward` calls `sparse_merkle_insert_batch(..., &[])` — empty batch,
[entrypoint/mod.rs:1154-1162](../../../src/contract/native_token/src/entrypoint/mod.rs)).
The claim nullifier is tracked at the host level only (`tx.nullifiers`,
sled batches, in-memory cache) and is covered by the COINBASE_MATURITY gate.
The "replay" attack from the
first audit (a second FeeCollect claiming zero after pot-zero) is killed by
check #1; no SMT insertion is required.

**Execution-ordering dependency:** check #2 reads `fees_db[height]` as
accumulated by **this block's** FeeV2 calls. This requires the layer-2
sequential-visibility guarantee — canonical calls execute in block order
against one shared overlay, so the fee-collect call (final transaction) sees
every prior `apply_fee` write. See
[Execution Ordering & Atomicity Layers](consensus/consensus.md#execution-ordering---atomicity-layers).

**No signature verification.** Miner identity is proven via the nullifier
(knowledge of `sk_H`). This is the same model as PoWRewardV1 — the nullifier
IS the authentication.

**Metadata (plaintext since 2026-09):** the entrypoint exposes the
`plaintext_call_get_metadata` shape — two encoded empty vectors (no public
inputs, no signature public keys). The L2 proof-metadata tables still require
one per-call proof slot for wire-shape compatibility: the fee-collect core tx
carries `proofs: vec![vec![]]` — an empty inner vec — because
`verify_core_tx_with_tables` enforces `proofs.len() == metadata_call_count`.
An empty OUTER vec would fail block accept.

**Fork boundary (replay of old blocks):** historical fee-bearing blocks mined
before 2026-09 carry a one-proof FeeCollect core tx. Re-executed under the
new entrypoint, the per-call length guard (1 ≠ 0) rejects them with
`InvalidProof` — the same accepted hazard as coinbase-era blocks after
b6bf44f79. Post-2026-09 blocks re-execute cleanly.

### 3.8 State Update

`apply_fee_collect` executes three state mutations atomically:

| # | Operation | Database | Effect |
|---|-----------|----------|--------|
| 1 | `commitment_set[C_fee] = []` | commitments | Fee commitment enters UTXO set |
| 2 | `merkle_add(commitment_merkle_tree, [C_fee])` | info + commitment_roots | Closes commitment merkle tree for this block |
| 3 | `fees_db[height] = 0` | fees | Zeros fee pot — prevents double-claim |

`FeeCollectUpdateV1` is `{commitment, height, total_fees}`. The claim nullifier
SHALL NOT be inserted into the contract `nullifiers_db` — it equals the commitment's
future spend nullifier `poseidon_hash(sk_H, C_fee)` and would make the fee commitment
born-unspendable (the spend path hits `DuplicateNullifier` at the SMT check).
PoWRewardV1 uses the identical model: `apply_pow_reward` calls
`sparse_merkle_insert_batch(..., &[])` with an **empty batch**
([entrypoint/mod.rs:1154-1162](../../../src/contract/native_token/src/entrypoint/mod.rs)),
so the coinbase nullifier never enters the contract SMT. Both claim nullifiers
live at the host level only.

Claim-replay prevention without contract-SMT insertion:
- Check #1 (§3.7): zero-claim rejected (`total_fees > 0` — kills the 0-fee
  re-spend after pot zero)
- Mutation #3: pot zeroed — a second claim sees `fees_db[H] = 0`
- Phase 0.5 (§3.15): at most one FeeCollectV1 call, present iff fees > 0
- Phase 6 (consensus.md): nullifier root covers the host-tracked nullifier
- Host-level nullifier tracking: the claim nullifier is published in
  `tx.nullifiers` and recorded in both the sled commitment/nullifier batches and
  the in-memory cache (chain_state.rs) — this gives fee commitments the same
  COINBASE_MATURITY treatment as coinbase coins (the nullifier_height check
  at chain_state.rs:947-960).

Closing the commitment merkle tree is the **layer-2 integrity boundary** (the fee
release check): PoWRewardV1 opens the tree at `transactions[0]`, every
commitment-creating call appends to it in block order, and FeeCollectV1 closes it
at `transactions[last]` — mutation #4 zeroing the pot is only reachable when
check #2 confirmed every fee accumulated. See
[Execution Ordering & Atomicity Layers](consensus/consensus.md#execution-ordering---atomicity-layers).

The cumulative supply state is NOT updated:

| Key | Touched? | Reason |
|-----|----------|--------|
| `TOTAL_SUPPLY` | No | Fees are redistribution, not minting |
| `CUMULATIVE_VALUE_COMMIT` | No | Supply chain unaffected |
| `CUMULATIVE_BLIND` | No | Supply chain unaffected |

### 3.9 Mass Balance Invariant

The proof-of-token-balance checker at
[`src/linear/src/proof_of_token_balance.rs`](../../../src/linear/src/proof_of_token_balance.rs)
skips FeeCollectV1 calls:

```
match func {
    FeeV2 | BurnV1 | SpendV1 | TransferV1 => { /* check value conservation */ }
    PoWRewardV1 | MintV1 | FeeCollectV1 => {} // minting or redistribution
}
```

FeeCollectV1 is skipped because it redistributes existing commitments — it does not
create or destroy value. The mass balance equation for a block is:

```
Σ(inputs) + coinbase_reward = Σ(outputs) + Σ(burns) + Σ(fees)
```

FeeCollectV1 moves `total_fees` from the fee pot to the miner's commitment. The fees
were already accounted for in the FeeV2 transactions (`output + fee == input`).
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

`prepare_block()` at [`bin/dwowd/src/lib.rs`](../../../bin/dwowd/src/lib.rs)
assembles the block in deterministic order:

1. Build plaintext coinbase (PoWRewardV1) — fallible, must succeed first
2. Collect uncles with pin rewards
3. Select mempool transactions
4. Filter immature coinbase spends (COINBASE_MATURITY soft gate)
5. Assemble coinbase transaction at position 0
6. **Sum FeeV2 fees** across all selected transactions → `total_fees`
7. **If `total_fees > 0`:** build the plaintext FeeCollectV1 call (no ZK proof) using the same `sk_H` as coinbase, create fee-collect transaction, append at final position
8. Mine the block (RandomX PoW)

The fee summation MUST:

1. **Filter by contract:** only calls with
   `contract_id == NATIVE_TOKEN_CONTRACT_ID` AND selector `0x08` (FeeV2) count.
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
`total_fees = 0` claim would pass check #2 and mint a 0-value commitment, reopening
the "closed" tree — this is the 0-fee replay identified in the red team audit
(finding D12).

**Wrong total attack:** A FeeCollectV1 with `total_fees ≠ fees_db[height]` is
rejected by the entrypoint. Under-claiming means the miner leaves money on the
table (no incentive). Over-claiming is prevented by the entrypoint check.

**Genesis block (height 1):** Genesis executes WASM through the standard
`accept_block` path (§4.3) — `apply_pow_reward` runs at height 1 and creates the
height-2 fee pot. `init_contract` also runs during genesis deployment
(via `apply_genesis_deployments()` — see [genesis.md](genesis.md)) and MUST
therefore seed `fees_db[2] = 0`. From height 2 onward,
`apply_pow_reward` sets `fees_db[H+1] = 0` for each block. If the key for a
height is missing, `apply_fee` and `fee_collect_v1` abort with `DbGetEmpty` —
a chain-halting failure, which is why initialization ownership must be
explicit. At height 1 itself there are no prior FeeV2 transactions, so
`total_fees = 0` and no FeeCollectV1 tx is created.

**FeeCollectV1 at wrong position:** Rejected by Phase 0 structural validation
(§3.15) — FeeCollectV1 MUST be the final transaction. The miner incentive
aligns: any commitments added after it would fail to be included in that block's
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
- Use deterministic AEAD encryption — `encrypt_deterministic()`
- Place FeeCollectV1 as the final transaction in the block
- Include FeeCollectV1 iff `total_fees > 0`

No ZK circuit — FeeCollectV1 is a plaintext call since 2026-09 (no proof,
no proving RNG).

### 3.15 Validator Obligation

The validator MUST reject blocks that:

| Rule | Phase | Status |
|------|-------|--------|
| More than one FeeCollectV1 call in the block (per-call count) | Phase 0 (structural) | IMPLEMENTED ([validation.rs:282-348](../../../src/linear/src/validation.rs)) |
| FeeCollectV1 present but block's summed FeeV2 fees == 0 | Phase 0 (structural) | IMPLEMENTED (validation.rs:322-331) |
| FeeCollectV1 with non-zero fees absent | Phase 0 (structural) | IMPLEMENTED (validation.rs:327-331) |
| FeeCollectV1 not the final transaction | Phase 0 (structural) | IMPLEMENTED (validation.rs:336-344) |
| FeeCollect_V1 ZK proof fails verification | Phase 3.1 | N/A — plaintext since 2026-09 (no proof; the empty per-call proof slot in the L1 witness still satisfies the L2 length guard, §3.7) |
| Duplicate fee-collect nullifier at host level | Phase 3.2 | IMPLEMENTED — claim nullifier in `tx.nullifiers` + both sled and in-memory batches ([chain_state.rs:819-827, 911-922](../../../src/linear/src/chain_state.rs)); COINBASE_MATURITY gate applies |
| WASM rejection: zero/mismatched fee total, duplicate commitment, duplicate nullifier (defense-in-depth, §3.7 check #4), non-DRKW token | Phase 4 | IMPLEMENTED ([entrypoint/mod.rs:971-1007](../../../src/contract/native_token/src/entrypoint/mod.rs)) |

All seven rules are enforced. The Phase 0 rules and §3.13's economic-incentive
discussion are consistent: the consensus rule and the miner incentive both put
FeeCollectV1 last.

### 3.16 Wallet Integration

The wallet discovers fee-collection commitments via the same scan mechanism as
coinbase rewards (§13.2). The scan gate at
[`bin/dww/src/scan.rs`](../../../bin/dww/src/scan.rs) includes selector `0x06`
alongside `0x05` (coinbase), `0x00` (FeeV1), `0x03` (TransferV1), and `0x04`
(SpendV1) in the output-discovery path. The per-block key `sk_H` is already in
`trial_secrets` from `secrets_for_contract(NATIVE_TOKEN_CONTRACT_ID, height)` —
the wallet derives it independently with zero shared state. The AEAD-encrypted
note is found by the same sliding-window decode used for every other call type;
the `FeeCollectParamsV1` layout is irrelevant to the scan. The claim nullifier
is NOT treated as a spend record (same exclusion as `0x05` for coinbase — both
nullifiers are capability claims for NEW commitments, not spends of held capabilities):

```
scan_fee_collect(secrets, block):
    1. fee_tx = block.transactions[last]
    2. if fee_tx has no contract call with selector 0x06: return None
    3. sk_H = derive_from_secrets(secrets, NATIVE_TOKEN, height)
    4. note = aead_decrypt(fee_call.encrypted_note, sk_H)
    5. C_fee = poseidon_hash(pk_H.x, pk_H.y, total_fees, DRKW_ASSET_ID, 0, 0, blind)
    6. nf' = poseidon_hash(sk_H.inner(), C_fee)
    7. if nf' == params.nullifier:
           build CapRecord(commitment=C_fee, secret=sk_H, value=total_fees, nullifier=nf')
```

The miner's wallet sees an additional commitment at each height where fees were
collected. The commitment is spendable via the same SpendV1/TransferV1/FeeV1 paths
as any other DRKW commitment. The total miner revenue at height H is:

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
| ZK circuit | None — plaintext since b6bf44f79 | None — plaintext since 2026-09 |
| Public inputs | — (plaintext params) | — (plaintext params) |
| Key derivation | derive_instance(sk_owner, cid, H) | Same |
| Nullifier model | nf = poseidon_hash(sk_H, C) | Same |
| Signature required | No (nullifier proves identity) | No |
| Wallet scan | §13.2 | §3.16 |

## 4. Emission Schedule

### 4.1 Constants

| Parameter | Value | Notes |
|-----------|-------|-------|
| Reference supply (tail onset) | 21,000,000 DRKW | NOT a hard cap — perpetual tail emission |
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
For h = 1: R(1) = R₀ (genesis receives the full initial reward)
For h ≥ 2:
    R(h) = max(R₀ × 2^(-(h-1)/H), R_tail)

where the decay factor is computed via integer binary exponentiation:
    exp   = h - 1
    decay = fixed_pow_decay(exp)  // ≈ 2^(-(h-1)/H), 32-bit fixed point
    R(h)  = max((R₀ × decay) >> 32, R_tail)

Constants:
    R₀ = 1,383,764,049 base units (~13.84 DRKW)
    H  = 1,051,920 blocks (half-life, ~4 years at 2-min blocks)
    R_tail = 79,853,981 base units (~0.80 DRKW, 1% per annum of 21M reference supply)
```

This is the production-default formula — there is no feature gate. The exponential
function is implemented at [`reward::expected_reward`](../../../src/sdk/src/blockchain.rs)
using integer-only fixed-point arithmetic. Floating point
MUST NOT be used. The canonical emission section is
[genesis.md §Emission Schedule](genesis.md#emission-schedule).

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
NativeToken contract's TOTAL_SUPPLY key during genesis-block execution, through
the standard `accept_block` path which executes WASM
(`apply_genesis_deployments()` then `pow_reward_v1`). See
[genesis.md](genesis.md) for the full bootstrap specification.

### 4.4 Derivation

Initial reward from the total supply constraint:

```
R₀ + ∑(h=2 to ∞) max(R₀ × 2^(-(h-1)/H), R_tail) ≤ 21,000,000 × 10^8 (reference supply)

R₀ = ⌊total_supply × ln(2) / half_life_blocks⌋
   = ⌊2,100,000,000,000,000 × ln(2) / 1,051,920⌋
   = 1,383,764,049 base units
```

Genesis (height 1) receives INITIAL_REWARD. Height 2 is the first decay step:
`R(2) = max(R₀ × 2^(-1/H), R_tail)` — the exponent is `height - 1`, not `height`.

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
[`bin/dwowd/src/registry/model.rs`](../../../bin/dwowd/src/registry/model.rs).

```rust
LinearBlockTemplate {
    previous: [u8; 32],              // Previous block hash
    height: BlockHeight,             // Block height
    target: BlockTarget,             // PoW target
    timestamp: u64,                  // Unix seconds (reused for blob + verification)
    value: BlockReward,              // Coinbase reward = expected_reward(height)
    pow_reward_call_data: Vec<u8>,   // Plaintext PoWRewardV1 call (0x05 + params), pre-built at template time
    miner: [u8; 32],                 // Miner's reward public key (pk_H) — set into BlockHeader.miner
    transactions: Vec<Transaction>,  // Pre-selected mempool transactions
    merkle_root: Blake3Hash,         // Merkle root of transactions (in mining blob)
    uncles: Vec<UncleBlock>,         // Uncle blocks included (competing blocks from previous height)
    uncle_merkle_root: [u8; 32],     // Merkle root of uncle headers (in mining blob)
    uncle_proofs: Vec<UncleProof>,   // Merkle proofs for each uncle (stateless verification)
}
```

The pre-plaintext fields (`zk_proof`, `zk_public_inputs`, `commitment`,
`value_commit_x/y`, `token_commit`, `nullifier`, `new_cumulative_x/y`,
`encrypted_note`, `commitment_merkle_root`, `nullifier_root`) were deleted
with b6bf44f79 — the coinbase is a plaintext contract call.

### 5.2 Algorithm

1. Compute `height = current_height + 1`
2. Get `previous_hash` from latest block
3. Read current `target` from consensus
4. Compute `reward = expected_reward(height)`
5. Capture `timestamp = now()` (MUST be consistent — reused for blob + verification)
6. Build the plaintext coinbase via `build_linear_coinbase()`:
   - Derive `sk_H` deterministically from declared identity (MUST, not random)
   - Compute `C`, `nf`, `vc`, `tc`, `S_H` in plaintext as specified in Section 2
   - Build the PoWRewardV1 contract call (selector `0x05` + serialized plaintext `PoWRewardParamsV1`) — no ZK proof rides in the transaction since b6bf44f79
   - Store `pow_reward_call_data` in template for stratum/mm_rpc miners
   - Compute `commitment_merkle_root` including the new commitment
   - Compute `nullifier_root` from tracked nullifiers

### 5.3 Lazy Initialization

No ZK keygen exists anymore: the coinbase is plaintext since b6bf44f79 and
FeeCollectV1 is plaintext since 2026-09, so no proving materials are loaded
at startup or on first template request. `LinearPowRewardZk` is deleted.

## 6. Uncle Merkle Consensus

### Problem

In standard PoW chains, when two miners find blocks at similar heights, one
becomes canonical and the other is orphaned — the miner wasted electricity for
nothing. This punishes smaller miners with higher latency and encourages pool
centralization.

### Solution: Obligated Pin Mechanism

The canonical chain MUST offer competing uncle chains a pin reward — a
one-time option to join and share the PoW reward.

| Uncle depth | Pin confirmed (`pin_confirmed`) |
|-------------|-------------------------------|
| 1 | `base / 2` = 50% |
| 2 | `base / 4` = 25% |
| 3 | `base / 8` = 12.5% |
| `d` | `base / 2^d`, capped at `MAX_UNCLE_DEPTH = 6` |

Rules:
- Pin is use-it-or-lose-it — the uncle chain accepts or rejects within a short window
- Accepting gives >0 reward, rejecting gives 0 — strictly dominated
- Not slashing — no one is punished, uncle miners gain, canonical miner keeps the majority

**Production pattern:** Ethereum uncle/ommer rewards (depth-decaying partial
reward), with DarkWow-unique exponential `1/2^depth` decay and a subtractive
Pedersen split instead of an additive reward.

**Invariant:** `canonical_reward + Σ pin_confirmed_i = base_reward` (exactly
100%). The coinbase split uses Pedersen commitment subtraction at the consensus
level. No new ZK proofs are needed — the split is verifiable via additive
homomorphism.

```
C_base = C_effective + Σ C_uncle_i
```

The `pow_reward_v1` entrypoint computes and persists `S_H = S_{H-1} + C_base`
(total minted correctly). Any node can recompute every blind deterministically
and verify `C_effective + Σ C_uncle_i = C_base` using only public data.

The header field `total_reward` SHALL equal the canonical miner's effective
reward: `total_reward == canonical_reward == base_reward − Σ pin_confirmed_i`.
The invariant `total_reward + Σ pin_confirmed_i == base_reward` is enforced by
`verify_uncle_split()` before the block reaches disk.

### PoWReward Function — Relationship to Uncle Split

The uncle reward is a **subtractive** split of the base coinbase reward: the
canonical miner's note is reduced by `Σ pin_confirmed_i`, and each accepted
uncle receives its own spendable note of `pin_confirmed_i`. Two commitment
kinds participate, and they MUST NOT be conflated — see
[uncle_merkle.md §Uncle Minting & Maturity](consensus/uncle_merkle.md#uncle-minting---maturity)
for the full normative specification.

1. **Cumulative supply chain (full base).** The coinbase `pow_reward_v1`
   (0x05, plaintext since b6bf44f79) SHALL keep
   `value_commit = pedersen_commit(expected_reward(H), r)`
   and advance `S_H = S_{H-1} + C_base` on the FULL base reward. The
   `expected_cumulative_supply` check is unchanged — the block still mints
   exactly `expected_reward(H)` of new supply.
2. **Canonical note (reduced).** The canonical miner's spendable note SHALL
   commit to `effective_value = base_reward − Σ pin_confirmed_i` via the
   plaintext `total_pin = value − effective_value` field in `PoWRewardParamsV1`;
   `nf = poseidon(sk_H, C'_effective)` binds the reduced note.
   `build_linear_coinbase()` SHALL therefore take the reduced `effective_value`
   (from the uncle set) in addition to the full `value`.
3. **Uncle notes (per uncle).** Each accepted uncle SHALL mint one spendable
   note of `pin_confirmed_i` to `uncle.header.miner`, via the `uncle_mint_v1`
   entrypoint (0x07, plaintext; `old_cumulative_value = 0`, NOT added to `S_H`).
4. **Pedersen supply audit.** `connect_block()` SHALL still compute the
   deterministic Pedersen commitments `C_uncle_i = u_i·G_v + r_i·G_r` and verify
   the subtractive mass balance `C_effective + Σ C_uncle_i = C_base` via
   `verify_uncle_split()` PRE-commit. These Pedersen points are audit artifacts,
   not spendable coins.
5. `compute_reward()` SHALL compute the value-level split:
   `canonical_reward = base_reward − Σ pin_confirmed_i`.

The header field `total_reward` SHALL equal the canonical miner's effective
reward: `total_reward == canonical_reward == base_reward − Σ pin_confirmed_i`.
The invariant `total_reward + Σ pin_confirmed_i == base_reward` is enforced by
`verify_uncle_split()` before the block reaches disk.

> **Status: implemented (plaintext since b6bf44f79).** The coinbase mints the
> full base reward into the cumulative supply chain via a plaintext
> `pow_reward_v1` call whose params carry `total_pin = value − effective_value`;
> per-uncle notes are plaintext `uncle_mint_v1` (0x07) calls; uncle pins are
> computed and verified value-level, and the subtractive mass balance is checked
> pre-commit via `verify_uncle_split()`. Uncle entries in the sled `uncles` tree
> are NOT removed during disconnect (see uncle_merkle.md §"Maturity,
> persistence, and reversal").

**Key invariant**: the cumulative supply chain ALWAYS accumulates the full
`base_reward` (`S_H = S_{H-1} + C_base`), while the spendable notes sum to the
same total — `effective_value + Σ pin_confirmed_i = base_reward`. No over-mint
and no under-mint: total spendable == total emitted. No new ZK proving key or
circuit namespace is needed — the split is expressed entirely in plaintext
params (`total_pin`).

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

### 9.1 Three-Tier Plaintext Fee Model

Fees are plaintext (`FeeV2` 0x08 decodes `FeeParamsV3` with a clear `fee` and
`tier`). No threshold proof is needed — the mempool compares the fee against
the tier price directly. Three FIFO tiers:

| Tier | Admission | Ordering | Purpose |
|------|-----------|----------|---------|
| High (4x) | `fee >= price_high` | FIFO (arrival order) | Urgent transactions |
| Medium (2x) | `price_medium <= fee < price_high` | FIFO after high | Normal transactions |
| Low (1x) | `price_low <= fee < price_medium` | FIFO after medium | Best-effort transactions |

The miner learns each fee in the clear — no hidden amounts, no accumulator.
The fee model is specified in [fee-spec.md §12.8.1](consensus/fee-spec.md).

### 9.2 Admission Gate

Admission is plain comparison against the declared tier prices
(`crates/dwow-mempool/src/lib.rs` `Mempool::add`):

```
tx_arrives(tx):
  // Step 1: L2 witness verification (structural, existing)
  verify_single_tx(tx)  // ↓bad-proof if fails

  // Step 2: Plaintext tier admission (no ZK threshold proof)
  plain_fee = fee_extractor.extract_fee(tx)   // FeeAmount — plaintext
  if plain_fee >= price_high:
    admit_to_high_queue(tx)
  else if plain_fee >= price_medium:
    admit_to_medium_queue(tx)
  else if plain_fee >= price_low:
    admit_to_low_queue(tx)
  else:
    REJECT  // ↓bad-fee — fee below the low tier price (fee-spec.md §12.8.1)
```

### 9.3 Data Structure

Three `VecDeque<blake3::Hash>` FIFO queues (high, medium, low) with
`AtomicU64` approximate length counters (lock-free congestion measurement)
alongside a `HashMap<blake3::Hash, MempoolEntry>` for O(1) lookup. A
`BTreeSet<Nullifier>` provides O(log n) nullifier deduplication. The
sled-backed persistence layer survives restarts.

**Legacy**: A `BTreeSet<FeeIndexEntry>` ordered by fee_rate (descending) remains
for non-FeeV3 transactions. FeeV3 transactions drop out of the legacy index and
use the three queues only.

### 9.4 Block Selection

`select_for_block(config)`:
1. Drain high queue in FIFO order until `max_gas` or `max_txs` reached
2. Drain medium queue in FIFO order until limits reached
3. Drain low queue in FIFO order until limits reached
4. Drain legacy fee_index (fee-rate ordered) until limits reached
5. Return selected transactions without removing from mempool
6. After block acceptance, `mark_mined(&tx_hashes)` removes confirmed txs

Miner configuration (`MinerConfig`) specifies `max_gas` and `max_txs` limits.

### 9.5 Transaction Lifecycle

**Path A — RPC-driven mining (dev / solo):**

```
User ──submit_transaction──► mempool.add(tx)
                                  │
                            plaintext tier admission — high/medium/low queue
                                  │
Miner ──mine_linear────────► select_for_block(&config) → non-destructive tx selection
                                  │
                            build coinbase (PoWRewardV1, plaintext nullifier claim)
                                  │
                            build_fee_collect_tx() — sums plaintext fees from FeeV2 calls
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
    fn extract_fee(&self, tx: &Transaction) -> FeeAmount;      // plaintext fee (type-system.md §2.3.1)
    fn extract_tier(&self, tx: &Transaction) -> FeeTier;       // priority selector (1=low, 2=medium, 4=high)
    fn declare_charge(&self, tx: &Transaction) -> BlockCharge; // block-capacity charge
}
```

FeeV3 has no threshold proof, no encrypted-fee channel, and no Pedersen fee
commitment. `FeeAmount` and `BlockCharge` are distinct domain types so gas
arithmetic cannot mix with fee or supply accounting.

### 9.7 Tier Prices

Default deployment values (`MempoolConfig::default()`,
`crates/dwow-mempool/src/lib.rs`):

| Price | Default | Meaning |
|-------|---------|---------|
| `price_high` | `4 * CongestionFactor::SCALE` | admission to the high queue |
| `price_medium` | `2 * CongestionFactor::SCALE` | admission to the medium queue |
| `price_low` | `1 * CongestionFactor::SCALE` | admission floor; below this → REJECT |

Tier prices are runtime-updatable via `update_tier_prices()` (congestion
response). See [fee-spec.md §12.8.1](consensus/fee-spec.md).

## 10. Mining Flow

1. `dwowd` generates a RandomX key for the next block template
2. Miner receives 260-byte mining blob (header with zeroed nonce) + target
3. Miner initializes RandomX VM with the key, hashes the blob with different nonces
4. If hash meets target (`hash_u32 <= target`), miner submits solved header
5. `dwowd` verifies the proof-of-work and assembles the block
6. Coinbase reward = `expected_reward(height)` paid via NativeToken::PoWRewardV1
7. If uncle, partial reward via pin mechanism (Section 6)

Target configuration:

```toml
[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds
initial_target = 268435455    # 0x0FFFFFFF, ~16 hashes expected per block
min_target = 1                # hardest possible
max_target = 4294967295       # u32::MAX, easiest possible
min_block_interval = 10       # seconds between blocks
```

## 11. Coinbase Reward Forwarding (Removed)

Coinbase rewards always go to the mining node's declared key — there is no
`FORWARD_DESTINATION` override in the current codebase. `build_linear_coinbase`
(`bin/dwowd/src/registry/model.rs`) derives the recipient from the node's declared
identity (`MiningRecipient::from_account` → `derive_instance`), with no forwarding
recipient field.

In local testing, a wallet decrypts coinbase rewards by declaring the **same** secret as
the mining node in the shared `keys.toml` (key sharing by declaration). In production, the
mining keypair and wallet keypair MUST be separate; reward distribution to another address
is a post-maturity `NativeToken::TransferV1`, not an in-coinbase override.

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
    5. C = poseidon_hash(pk_H.x, pk_H.y, value, DRKW_ASSET_ID, 0, 0, blind)
    6. nf' = poseidon_hash(sk_H.inner(), C)
    7. if nf' == params.nullifier:                // defense-in-depth
           build CapRecord(commitment=C, secret=sk_H, value)
```

The wallet derives the same `sk_H` as the miner — independently,
deterministically, with zero shared state. If the keys match, the note
decrypts. If the nullifier matches, the claim is valid.

### 13.3 Fee Payment Cycle

```
Wallet                          Miner
  │                               │
  │  selects DRKW capability        │
  │  builds FeeV2 call (plaintext fee) │
  │  publishes nullifier           │
  │  ──── transaction ────────►   │
  │                               │  collects fees via FeeCollectV1 (separate from coinbase)
  │                               │  claims reward via PoWRewardV1 nullifier
  │                               │  can spend reward (FeeV2/BurnV1/TransferV1)
  │                               │
  │  scans block                   │
  │  detects fee nullifier ←────── │  (wallet revokes spent capability)
  │  detects new coinbase ←──────  │  (wallet discovers reward if this wallet's miner)
```

Fees flow from wallet to miner as a separate FeeCollectV1 claim — the miner
collects all transaction fees in the block and mints them via FeeCollectV1,
NOT through the coinbase (the coinbase is reward-only, §17.3). The fee
payment is a capability exercise (FeeV2 nullifier) — the wallet proves it
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
| Coinbase model | Transparent UTXO | Transparent output | Plaintext nullifier claim (deterministic, no ZK) |

The last three rows are DarkWow's architectural differentiators. The key model
is specified in [wallet.md](wallet.md). The wallet-as-full-node design means
every user verifies the coinbase independently. The plaintext nullifier claim
model means coinbase rewards follow the same privacy-preserving capability
pattern as every other transaction.

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
four consensus-critical native_token entrypoints — FeeV2 (fee-spec.md §5),
FeeCollectV1 (§17.2), PoWRewardV1 (§17.3), UncleMintV1 (§17.4) — plus the
removed FeeV1 (§17.1, historical). Each check is specified with its exact
condition, failure variant, consensus barb, and enforcement layer. Tests SHALL
be derived from these tables — not from reverse-engineering code.*

### 17.1 FeeV1 — Fee Payment Entrypoint (HISTORICAL — REMOVED)

**Status**: REMOVED. Function code `0x00` returns `InvalidFunction` at the contract
dispatch layer. All fee payment SHALL use FeeV2 (`0x08`) per
[fee-spec.md §5](consensus/fee-spec.md). This section (§17.1) is preserved
for historical reference only.

FeeV1 (historical) spent an existing commitment, splitting it into a change output
and a fee payment. The fee was accumulated into `fees_db[height]` for later
collection by FeeCollectV1. The input commitment MUST exist in the commitment Merkle tree
at a committed root.

#### 17.1.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Deserialize fee | `fee = u64::from_le_bytes(params[0..8])` | `InsufficientBalance` | ↓bad-fee-amount | Primary |
| 2 | Deserialize FeeParamsV1 | `FeeParamsV1::decode(&params[8..])` | `ParseError` | ↓bad-fee-params | Primary |
| 3 | Input token is DRKW | `input.token_commit == poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | `InsufficientBalance` | ↓bad-token | Primary |
| 4 | Output token is DRKW | `output.token_commit == poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | `InsufficientBalance` | ↓bad-token | Primary |
| 5 | Minimum fee | fee ≥ two-component formula (§12.4.1): `(wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)` | `InsufficientBalance` | ↓bad-fee-amount | Primary |
| 6 | Merkle root exists | `db_contains_key(commitment_roots_db, &input.merkle_root)` | `TransferMerkleRootNotFound` | ↓bad-merkle-root | Primary |
| 7 | Nullifier not spent | `SMT.get_leaf(input.nullifier) == ZERO` | `InsufficientBalance` | ↓double-spend | Primary |
| 8 | Output commitment not duplicate | `!db_contains_key(commitment_set, &output.commitment)` | `InsufficientBalance` | ↓duplicate-commitment | Defense-in-depth |
| 9 | Get verifying height | `block_height` from host context | N/A (read-only) | — | — |

**Return**: `FeeUpdateV1 { nullifier, commitment, height, fee }` encoded.

#### 17.1.2 State Mutations (`apply_fee`)

Executed after successful verification. All writes are atomic within the
block overlay. Order matters: nullifier MUST be marked spent before the new
commitment is added (prevents intra-block double-spend via SMT lookups).

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Mark nullifier spent | `nullifiers_db` | `input.nullifier` | `[1u8]` |
| 2 | Add output commitment | `commitment_set` | `output.commitment` | `[]` |
| 3 | Update Merkle tree | `commitment_merkle_tree` | Merkle leaf | `output.commitment` (via `merkle_add`) |
| 4 | Accumulate fee | `fees_db[height]` | `height` | `fees_db[height] + fee` (saturating_add) |

#### 17.1.3 Client Build Algorithm (`FeeCallBuilder::build()`)

```
Input:  input_value, asset_id, spend_hook, user_data, commitment_blind,
        leaf_position, merkle_path, secret, ephemeral_signature_secret,
        recipient, output_spend_hook, output_user_data, fee_amount
        tx_commitment, tx_nonce

1.  Validate input_value > fee_amount (else InsufficientBalance)
2.  Compute output_value = input_value - fee_amount
3.  Compute input_value_blind from commitment_blind
4.  Set output_value_blind = input_value_blind (Pedersen homomorphic balance)
5.  Set token_blind = BaseBlind::ZERO (DRKW token)
6.  Derive signature_public from ephemeral_signature_secret
7.  Compute tx_binding = poseidon(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)
8.  Build ZK proof (Fee_V2 circuit, 22 witnesses):
    - input_value, output_value, fee_amount (u64 range-checked)
    - input_commitment, output_commitment (Pedersen commitments)
    - nullifier (poseidon(DOMAIN_NULLIFIER, secret, input_commitment))
    - merkle_root (computed from leaf_position + merkle_path)
    - token_commit, user_data_enc
    - signature_public, tx_binding, tx_nonce
9.  Construct Input { value_commit, token_commit, nullifier, merkle_root,
    user_data_enc, spend_hook, signature_public }
10. Construct Output { value_commit, token_commit, commitment, nullifier, note }
11. Encrypt AEAD note with domain 12
12. Construct FeeParamsV1 { input, output, fee_value_blind, fee_token_blind,
    fee: fee_amount, tx_binding, tx_nonce }
13. Pack call_data: [0x00] + fee_amount.to_le_bytes() + FeeParamsV1::encode()
14. Return FeeResult { call_data, params, proofs }
```

### 17.2 FeeCollectV1 — Fee Collection Entrypoint

**Function code**: `0x06`. **ZK-gated**: NO — plaintext since 2026-09
(no FeeCollect_V2 circuit).
**Client builder**: `FeeCollectCallBuilder` (`src/contract/native_token/src/client/fee_collect.rs`).

FeeCollectV1 claims the accumulated fee pot `fees_db[height]` and mints a
new commitment to the miner. It closes the commitment Merkle tree for the block. The
transaction MUST be the final transaction in the block (Phase 0 enforcement).

#### 17.2.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Non-zero claim | `fc.total_fees > 0` | `ZeroFeeClaim` | ↓zero-claim | Primary |
| 2 | Claim matches accumulated | `fc.total_fees == fees_db[height]` (plain u64) | `FeeTotalMismatch` | ↓bad-claim | Primary |
| 3 | Commitment not duplicate | `!db_contains_key(commitment_set, &fc.output.commitment)` | `DuplicateCommitment` | ↓duplicate-commitment | Defense-in-depth |
| 4 | Token is DRKW | `fc.output.token_commit == poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | `TokenMismatch` | ↓bad-token | Primary |

There is **no** contract-level nullifier check — the claim nullifier is
host-tracked and covered by the COINBASE_MATURITY gate (§17.6).

**Return**: `FeeCollectUpdateV1 { commitment, height, total_fees }` encoded.

#### 17.2.2 State Mutations (`apply_fee_collect`)

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Add fee commitment | `commitment_set` | `fc.output.commitment` | `[]` |
| 2 | Update Merkle tree | `commitment_merkle_tree` | Merkle leaf | `fc.output.commitment` (via `merkle_add`, closes tree) |
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
4.  Compute commitment = poseidon(DOMAIN_COMMITMENT, pk.x, pk.y, total_fees,
    DRKW_ASSET_ID, spend_hook=0, user_data=0, value_blind)
5.  Compute nullifier = poseidon(DOMAIN_NULLIFIER, sk_H, commitment)
6.  Compute value_commit = pedersen_commit(total_fees, value_blind) and
    token_commit = poseidon(DOMAIN_TOKEN_COMMIT, asset_id, 0) — plaintext
    values, no ZK proof since 2026-09
7.  Encrypt AEAD note with domain 13 (deterministic)
8.  Construct FeeCollectParamsV1 { output: Output { value_commit, token_commit,
    commitment, nullifier, note }, total_fees, value_blind }
9.  Pack call_data: [0x06] + FeeCollectParamsV1::encode()
10. Return Transaction { contract_calls: [call], nullifiers: [nullifier] } — no ZK
    proof since 2026-09; the core tx carries `proofs: vec![vec![]]` (one empty
    inner vec per metadata call) to satisfy the L2 proof-metadata tables
    (`verify_core_tx_with_tables` enforces `proofs.len() == metadata_call_count`)
```

### 17.3 PoWRewardV1 — Coinbase Reward Entrypoint

**Function code**: `0x05`. **ZK-gated**: NO (plaintext — value is public; verified by Pedersen/poseidon arithmetic).
**Client builder**: `build_linear_coinbase()` (`bin/dwowd/src/registry/model.rs`).

PoWRewardV1 mints the block reward to the miner. It is ALWAYS at
`transactions[0]` (Phase 0 enforcement) and opens the commitment Merkle tree for
the block.

#### 17.3.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Deserialize params | `PoWRewardParamsV1::decode(params)` | `ParseError` | ↓bad-params | Primary |
| 2 | Token is DRKW (clear) | `pr.input.asset_id == DRKW_ASSET_ID` (0) | `InsufficientBalance` | ↓bad-token | Primary |
| 3 | Value commit matches | `pedersen_commit(value, value_blind) == pr.output.value_commit` | `InsufficientBalance` | ↓bad-commit | Primary |
| 4 | Token commit matches | `poseidon(DOMAIN_TOKEN_COMMIT, asset_id, token_blind) == pr.output.token_commit` | `InsufficientBalance` | ↓bad-commit | Primary |
| 5 | Commitment not duplicate | `!db_contains_key(commitment_set, &pr.output.commitment)` | `InsufficientBalance` | ↓duplicate-commitment | Primary |
| 6 | Nullifier non-zero | `pr.output.nullifier != Nullifier::zero()` | `InsufficientBalance` | ↓bad-nullifier | Defense-in-depth |
| 7 | Nullifier not in SMT | `SMT.get_leaf(pr.output.nullifier) == ZERO` | `InsufficientBalance` | ↓double-spend | Defense-in-depth |
| 8 | Exact reward | `pr.input.value == expected_reward(height)` | `InsufficientBalance` | ↓bad-reward | Primary |
| 9 | Supply check | `current_supply + reward == pr.expected_cumulative_supply` | `InsufficientBalance` | ↓bad-supply | Primary |
| 10 | Old cumulative commit | `pr.current_cumulative_commit == CUMULATIVE_VALUE_COMMIT` | `InsufficientBalance` | ↓bad-supply | Primary |
| 11 | Old cumulative blind | `pr.current_blind == CUMULATIVE_BLIND` (skip if supply=0) | `InsufficientBalance` | ↓bad-supply | Primary |
| 12 | Blind continuity | `new_blind = old_blind + value_blind` | (computed, not checked against input) | — | — |
| 13 | New cumulative commit | `pedersen_add(old_commit, value_commit) == pr.new_cumulative_commit` | `InsufficientBalance` | ↓bad-supply | Primary |

**Return**: `PoWRewardUpdateV1 { commitment, height, new_total_supply, cumulative_value_commit, aggregate_blind }` encoded.

#### 17.3.2 State Mutations (`apply_pow_reward`)

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Seed next fee pot | `fees_db[height+1]` | `height+1` | `0` |
| 2 | Update nullifier snapshot | `nullifiers_db` | (batch) | empty (claim nullifier EXCLUDED) |
| 3 | Record total supply | `info_db` | `TOTAL_SUPPLY` | `new_total_supply` (u64 LE) |
| 4 | Record cumulative commit | `info_db` | `CUMULATIVE_VALUE_COMMIT` | `cumulative_value_commit` (Pedersen point) |
| 5 | Record cumulative blind | `info_db` | `CUMULATIVE_BLIND` | `aggregate_blind` (pallas::Scalar) |
| 6 | Add commitment | `commitment_set` | `output.commitment` | `[]` |
| 7 | Update Merkle tree | `commitment_merkle_tree` | Merkle leaf | `output.commitment` (via `merkle_add`, opens tree) |

### 17.4 UncleMintV1 — Uncle Reward Entrypoint

**Function code**: `0x07`. **ZK-gated**: NO — plaintext since b6bf44f79
(no Mint_V2 proof).
**Client builder**: `build_uncle_mint()`
(`src/contract/native_token/src/client/uncle_mint.rs`), wrapped by
`build_uncle_mint_tx()` (`bin/dwowd/src/registry/model.rs`).

UncleMintV1 mints one spendable note per accepted uncle, carved out of the
coinbase's full base reward (see §6). It does NOT touch `fees_db`,
`TOTAL_SUPPLY`, or the cumulative Pedersen chain — the uncle note's value is
part of the coinbase's already-committed reward mass, so there is no supply
bump (`S_H` unchanged).

**No on-chain reward check**: the uncle note value (`pin_confirmed_i`) is
validated host-side at block assembly (§6 — obligated pin mechanism); the
entrypoint mints whatever clear value the host-approved call carries.

#### 17.4.1 Verification Checks (execution order)

| # | Check | Condition | Failure | Barb | Layer |
|---|-------|-----------|---------|------|-------|
| 1 | Deserialize params | `UncleMintParamsV1::decode(params)` | `ParseError` | ↓bad-params | Primary |
| 2 | Token is DRKW (clear) | `um.input.asset_id == DRKW_ASSET_ID` (0) | `TokenMismatch` | ↓bad-token | Primary |
| 3 | Value commit matches | `pedersen_commitment_u64(um.input.value, um.input.value_blind) == um.output.value_commit` | `ValueMismatch` | ↓bad-commit | Primary |
| 4 | Token commit matches | `poseidon_hash([DOMAIN_TOKEN_COMMIT, asset_id, token_blind]) == um.output.token_commit` | `TokenMismatch` | ↓bad-commit | Primary |
| 5 | Commitment not duplicate | `!db_contains_key(commitment_set, &um.output.commitment)` | `DuplicateCommitment` | ↓duplicate-commitment | Primary |
| 6 | Nullifier non-zero | `um.nullifier != Nullifier::zero()` | `InvalidFunction` | ↓bad-nullifier | Defense-in-depth |

**Return**: `UncleMintUpdateV1 { commitment, height }` encoded.

#### 17.4.2 State Mutations (`apply_uncle_mint`)

| # | Operation | Database | Key | Value |
|---|-----------|----------|-----|-------|
| 1 | Add uncle commitment | `commitment_set` | `um.output.commitment` | `[1]` |
| 2 | Update Merkle tree | `commitment_merkle_tree` | Merkle leaf | `um.output.commitment` (via `merkle_add`) |

No `fees_db`, `TOTAL_SUPPLY`, or `CUMULATIVE_VALUE_COMMIT/BLIND` writes — no
supply change.

#### 17.4.3 Client Build Algorithm (`build_uncle_mint()`)

```
Input:  value (uncle pin reward), uncle_miner (uncle.header.miner),
        uncle_hash (blake3 of the uncle's mining blob), height,
        tx_commitment, tx_nonce

1.  Derive per-uncle spend_secret = poseidon(uncle_hash, height, domain=20)
    — deterministic from the public uncle hash, so the uncle miner's wallet
    can re-derive it independently (same pure-function model as the coinbase,
    "no random keys")
2.  Derive ephemeral_secret = poseidon(spend_secret, height, domain=21)
3.  Derive blinds, all poseidon(spend_secret, height, domain): value_blind
    (domain=22), token_blind (domain=23), commitment_blind (domain=24)
4.  Build ClearInput { value, asset_id = DRKW, value_blind, token_blind,
    signature_public = pk(spend_secret) }
5.  Build output commitment attributes { pk = uncle_miner, value, asset_id,
    spend_hook = 0, user_data = 0, commitment_blind }
6.  Compute revealed inputs via compute_transfer_mint_revealed(...,
    old_cumulative_value = 0, old_cumulative_blind = 0) — the cumulative
    supply chain is NOT touched
7.  total_pin = 0 (an uncle note is not split further)
8.  Encrypt AEAD note deterministically to uncle_miner (ephemeral_secret) —
    the note hides the blinds, not the value (value is plaintext)
9.  nullifier = poseidon(DOMAIN_NULLIFIER, spend_secret, commitment)
10. Construct UncleMintParamsV1 { input, total_pin: 0, output, nullifier,
    tx_binding, tx_nonce }
11. Pack call_data: [0x07] + UncleMintParamsV1::encode()
12. Return UncleMintCallDebris — parameters only, no ZK proof (plaintext)
```

### 17.5 Claim-Nullifier Exclusion Invariant

Both PoWRewardV1 and FeeCollectV1 produce a nullifier that is tracked by
the host (`tx.nullifiers`, sled batches, in-memory `nullifier_set`) but NOT
inserted into the contract's `nullifiers_db` SMT.

**Rationale**: The claim nullifier `nf_claim` equals the spend nullifier
`nf_spend` for the same commitment (both are `poseidon(DOMAIN_NULLIFIER, sk_H, commitment)`).
If the claim nullifier were inserted into the contract SMT, the commitment would be
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

### 17.6 COINBASE_MATURITY

```
COINBASE_MATURITY = 100 blocks  (src/linear/src/lib.rs:70)
```

Every non-coinbase nullifier in `tx.nullifiers` is checked at `connect_block`
(`src/linear/src/chain_state.rs:1055`):

```rust
let created_at = self.nullifier_height(&nullifier)?;
if height.saturating_sub(created_at) < COINBASE_MATURITY {
    return Err("Immature coinbase spend");
}
```

This applies to: PoWRewardV1 nullifiers, FeeCollectV1 nullifiers, and any
spend nullifier from a commitment created by PoWRewardV1 or FeeCollectV1.

The host tracks nullifiers in three synchronized stores:
- Per-block `tx.nullifiers` (inserted at block assembly)
- Sled `store.nullifiers` batch (persisted atomically with the block)
- In-memory `nullifier_set: BTreeMap<Nullifier, BlockHeight>` (for fast lookup)

After each block, entries where `height - created_at >= COINBASE_MATURITY` are
pruned from the in-memory cache (they remain in sled for historical queries).

### 17.7 Constants

| Constant | Value | Location |
|----------|-------|----------|
| `BASELINE_STORAGE` | `1_000_000` (0.01 DRKW/kB at CF=1.0) | `src/linear/src/fee_window.rs` |
| `COINBASE_MATURITY` | `100` blocks | `src/linear/src/lib.rs` (`pub const COINBASE_MATURITY`) |
| `INITIAL_REWARD` | `1_383_764_049` (~13.84 DRKW) | `src/sdk/src/blockchain.rs` (`reward::INITIAL_REWARD`) |
| `FeeV1` selector (removed) | `0x00` | Returns `InvalidFunction` per fee-spec.md §3 |
| `PoWRewardV1` selector | `0x05` | `src/contract/native_token/src/lib.rs:68` |
| `FeeCollectV1` selector | `0x06` | `src/contract/native_token/src/lib.rs:69` |
| `UncleMintV1` selector | `0x07` | `src/contract/native_token/src/lib.rs:70` |
| `FeeV2` selector | `0x08` | `src/contract/native_token/src/lib.rs:71` |
| DRKW token ID | `0` | `src/contract/native_token/src/lib.rs` |

### 17.8 Error Taxonomy

Every entrypoint failure maps to a consensus barb. Tests SHALL assert the
specific barb, not generic "canonical call failed at exec."

| Entrypoint | Failure Variant | Consensus Barb | Rejection Level |
|-----------|----------------|----------------|-----------------|
| FeeV1 (removed) | `InsufficientBalance` (fee below two-component threshold) | ↓bad-fee-amount | Block rejected |
| FeeV1 (removed) | `InsufficientBalance` (input <= fee) | ↓bad-fee-amount | Block rejected |
| FeeV1 (removed) | `TransferMerkleRootNotFound` | ↓bad-merkle-root | Block rejected |
| FeeV1 (removed) | `InsufficientBalance` (nullifier spent) | ↓double-spend | Block rejected |
| FeeV1 | `ParseError` | ↓bad-fee-params | Block rejected |
| FeeCollectV1 | `ZeroFeeClaim` (total_fees == 0) | ↓zero-claim | Block rejected |
| FeeCollectV1 | `FeeTotalMismatch` (claim != accumulated) | ↓bad-claim | Block rejected |
| FeeCollectV1 | `DuplicateCommitment` | ↓duplicate-commitment | Block rejected |
| FeeCollectV1 | `TokenMismatch` (wrong token) | ↓bad-token | Block rejected |
| UncleMintV1 | `TokenMismatch` (wrong token) | ↓bad-token | Block rejected |
| UncleMintV1 | `ValueMismatch` (value commit mismatch) | ↓bad-commit | Block rejected |
| UncleMintV1 | `DuplicateCommitment` | ↓duplicate-commitment | Block rejected |
| UncleMintV1 | `InvalidFunction` (null nullifier) | ↓bad-nullifier | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (reward != expected) | ↓bad-reward | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (supply mismatch) | ↓bad-supply | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (duplicate commitment) | ↓duplicate-commitment | Block rejected |
| PoWRewardV1 | `InsufficientBalance` (nullifier spent) | ↓double-spend | Block rejected |

Tests SHALL use INFRA-FAIL prefix for block-proof violations (any of the
above) because they represent consensus-level rejection — not contract-
specific logic errors.
