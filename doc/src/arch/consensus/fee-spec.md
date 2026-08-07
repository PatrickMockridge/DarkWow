# Fee Payment and Collection — Formal Specification

*Specification for FeeV2, FeeCollectV1, and the coin Merkle tree.
FeeV1 (function code `0x00`, clear-text fee) is REMOVED.
Theorems, invariants, and formal predicates. Tests SHALL be derived from
this document — not from reverse-engineering production code.*

## 1. Coin Merkle Tree

### 1.1 Type Definition

The coin Merkle tree is an incremental Merkle tree of commitments to
native-token coins. It is shared by all native_token functions:
PoWRewardV1 appends coins to it; FeeV2, TransferV1, SpendV1, and BurnV1
prove inclusion of a coin at a prior root; FeeCollectV1 appends a final
coin and closes the tree for the block.

```
CoinTree = BridgeTree<MerkleNode, usize, MERKLE_DEPTH>
MerkleNode = MerkleNode(pallas::Base)   // inner = single base field element
MERKLE_DEPTH = 32                        // Zcash Orchard protocol
```

### 1.2 Hashing

```
MerkleNode::combine(altitude: u8, left: &MerkleNode, right: &MerkleNode) -> MerkleNode
  = SinsemillaHash(altitude || left.inner() || right.inner())
    truncated to pallas::Base
```

This is `MerkleCRH^Orchard` from Zcash Orchard protocol §5.4.1.5.
The altitude ranges from 0 (leaf pairs) to MERKLE_DEPTH-1 (root pair).

### 1.3 Empty Subtree Values

```
UNCOMMITTED_ORCHARD = pallas::Base::from(2)

MerkleNode::empty_leaf() = MerkleNode(UNCOMMITTED_ORCHARD)
```

The empty leaf value is `pallas::Base::from(2)`, NOT zero. This is a
Zcash Orchard protocol constant. Any position that has never had a leaf
appended has this value. The ZK circuit's merkle path verification uses
this value for empty subtrees.

### 1.4 Empty Roots Ladder

For any level where both child subtrees are empty, the canonical node is
computed by the ladder:

```
EMPTY_ROOTS[0] = MerkleNode(UNCOMMITTED_ORCHARD)
EMPTY_ROOTS[i] = MerkleNode::combine(i-1, EMPTY_ROOTS[i-1], EMPTY_ROOTS[i-1])   for i > 0
```

The empty roots ladder is precomputed at module load time. For a merkle
path at level L whose sibling subtree is empty, the sibling value is
`EMPTY_ROOTS[L]`.

### 1.5 Tree Initialization

At contract deployment, the coin Merkle tree is initialized with exactly
one append: a zero guard at position 0.

```
Position 0 = MerkleNode(pallas::Base::ZERO)   // zero guard, NOT empty leaf
Root after init = combine(31, ..., combine(1,
    combine(0, zero_guard, EMPTY_ROOTS[0]),
    EMPTY_ROOTS[1]), ..., EMPTY_ROOTS[30])
```

The zero guard at position 0 is a concrete value `pallas::Base::ZERO`.
It is NOT an empty leaf (`pallas::Base::from(2)`). This distinction
matters: the merkle path for leaf position 1 has sibling at level 0 =
`pallas::Base::ZERO`, while siblings at levels 1-31 are
`EMPTY_ROOTS[0..30]` = `pallas::Base::from(2)` and derivatives.

### 1.6 Position Enumeration

Positions are 0-indexed, monotonically incrementing. Each call to
`append(leaf)` writes leaf at the current position then increments
the counter.

```
After init:                    next_position = 1
First coin (genesis coinbase): position = 1, next_position = 2
Second coin (height 2 coinbase): position = 2, next_position = 3
...
Coin N:                        position = N, next_position = N+1
```

### 1.7 Root Storage

Each `merkle_add(coin)` operation:

1. Deserializes the current tree from the overlay
2. Appends `coin` at the next position P
3. Serializes the updated tree back to the overlay
4. Computes `new_root = tree.root(0)` — the root including all coins up to P
5. Inserts `new_root` → `coin_roots_db[new_root.to_bytes()] = [tx_hash || call_idx]`
6. Updates `LATEST_COIN_ROOT` pointer

The root table `coin_roots_db` serves as the inclusion-proof anchor.
FeeV1's check #6 queries this table: `db_contains_key(coin_roots_db, &input.merkle_root)`.

### 1.8 Merkle Path Derivation

**Theorem 1 (Merkle Path for Leaf Position P):**

Given a tree with N leaves at positions 0..N-1, the merkle path for
position P consists of 32 siblings, one per level. For each level L:

```
If bit L of P is 0:
  sibling position = P | (1 << L)
  If sibling position < N: sibling = leaf at that position, hashed up
  Else:                   sibling = EMPTY_ROOTS[L]
If bit L of P is 1:
  sibling position = P & ~(1 << L)
  sibling = leaf at that position, hashed up
  (Always < N since P < N and clearing a bit produces a smaller number)
```

**Example — leaf position 1 with N=2:**

| Level | Bit | Sibling pos | In tree? | Value |
|-------|-----|-------------|----------|-------|
| 0 | 1 | 0 | Yes (pos 0) | ZERO_GUARD = pallas::Base::ZERO |
| 1 | 0 | 2-3 subtree | No (≥2) | EMPTY_ROOTS[0] = pallas::Base::from(2) |
| 2 | 0 | 4-7 subtree | No (≥2) | EMPTY_ROOTS[1] |
| ... | ... | ... | ... | ... |
| 31 | 0 | ... | No | EMPTY_ROOTS[30] |

**Example — leaf position 1 with N=3:**

| Level | Bit | Sibling pos | In tree? | Value |
|-------|-----|-------------|----------|-------|
| 0 | 1 | 0 | Yes (pos 0) | ZERO_GUARD = pallas::Base::ZERO |
| 1 | 0 | 2-3 subtree | Yes (pos 2) | hash of coin at pos 2 |
| 2 | 0 | 4-7 subtree | No (≥3) | EMPTY_ROOTS[1] |
| ... | ... | ... | ... | ... |

### 1.9 Merkle Path as ZK Witness

The ZK circuit's merkle path verification iterates level 0 to 31:

```
current = MerkleNode::from_base(coin_commitment.inner())
for L in 0..32:
  if position & (1 << L) == 0:
    current = MerkleNode::combine(L, current, merkle_path[L])
  else:
    current = MerkleNode::combine(L, merkle_path[L], current)
// After loop: current == merkle_root — this is a public input
```

The circuit constrains that `current == merkle_root` (public input),
proving the prover knows a valid path from the coin to the claimed root.

## 2. Block Production Model

### 2.1 Transaction Ordering

Block N has the following canonical transaction order:

```
transactions[0]     = coinbase           (PoWRewardV1, fn_code 0x05)
transactions[1..k]  = user transactions  (FeeV2, TransferV1, SpendV1, BurnV1, deploys)
transactions[k+1]   = FeeCollectV1       (fn_code 0x06) — iff total_fees > 0
```

Phase 0 structural validation enforces: exactly one coinbase at index 0,
FeeCollectV1 (if present) at the final index. The coinbase transaction
SHALL contain exactly one contract call (compound coinbase prevention).

### 2.2 Sequential Execution Model

Within `execute_block`, each canonical transaction runs
`metadata()` → `exec()` → `apply()` sequentially in a shared overlay.

**Invariant 1 (Overlay Visibility)**: Call `i` observes the state writes of
calls `0..i-1` within the same block. Specifically, FeeV2's `exec()` (call
i) sees the coinbase's `apply_pow_reward()` writes (call 0), including the
merkle root inserted into `coin_roots_db`.

This is the mechanism that enables same-block fee payment: the coinbase
coin's merkle root IS visible to FeeV2 in the same block. This is NOT the
production path (where FeeV2 spends coins from prior blocks), but is a
valid test path when `tx.nullifiers` is empty (bypassing COINBASE_MATURITY).

### 2.3 Coin Tree Growth Per Block

For block at height H:

```
Starting tree: N leaves (from blocks 1..H-1)

1. PoWRewardV1 apply_pow_reward:
   append(coinbase_coin_H) → position N, root = R_H_0
   coin_roots_db[R_H_0] = ...

2. Each user FeeV2 apply_fee:
   append(output_coin_i) → position N+i, root = R_H_i
   coin_roots_db[R_H_i] = ...

3. Each TransferV1/SpendV1:
   append(output_coin_j) → position N+i+j, root = R_H_{i+j}
   coin_roots_db[R_H_{i+j}] = ...

4. FeeCollectV1 apply_fee_collect:
   append(fee_coin_H) → final position, root = R_H_final
   coin_roots_db[R_H_final] = ...
   fees_db[H] = 0
```

After the block, the tree has N + coins_created_this_block leaves.

### 2.4 COINBASE_MATURITY and Test Bypass

```
COINBASE_MATURITY = 100 blocks
```

**Production path**: A nullifier created at height H_c cannot be spent
until height ≥ H_c + 100. Enforced at `connect_block` by checking
`nullifier_set` (in-memory `BTreeMap<Nullifier, BlockHeight>`) populated
from `tx.nullifiers` of prior blocks.

**Test bypass**: Test transactions built via `build_contract_tx()` have
`nullifiers: vec![]`. The maturity check iterates `tx.nullifiers` and
skips when the vector is empty. Therefore tests can spend coins at any
height without triggering COINBASE_MATURITY.

The contract-level SMT nullifier check (FeeV2 check #7) still applies:
the nullifier must not exist in the contract's `nullifiers_db` SMT.

## 3. FeeV1 — Fee Payment Entrypoint (REMOVED)

**Function code**: `0x00`. **Status**: REMOVED. `0x00` returns `InvalidFunction`
at the contract dispatch layer. All fee payment SHALL use FeeV2 (§5).

FeeV1 is documented here for historical reference only. It exposed the fee
amount in clear text (`[0x00][fee: u64 LE 8 bytes][FeeParamsV1 encoded]`).
FeeV2 (§5) replaces it with a privacy-preserving Pedersen commitment.

### 3.1 Purpose (Historical)

FeeV1 spent an existing coin C, splitting it into:
- O: output coin returned to user (value = C.value - fee)
- F: fee accumulated into `fees_db[height]`

### 3.2 Formal Preconditions (Historical)

Let `params = FeeParamsV1 { input: Input, output: Output, fee: u64, ... }`
and `fee = u64::from_le_bytes(call_data[1..9])`.

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| P1 | `params = FeeParamsV1::decode(&call_data[9..])` succeeds | ParseError | Custom(2) |
| P2 | `input.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P3 | `output.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P4 | ~~`fee >= MIN_FEE_PER_CALL`~~ | REMOVED — mempool policy, not consensus | — |
| P5 | `db_contains_key(coin_roots_db, input.merkle_root.to_bytes())` | TransferMerkleRootNotFound | Custom(13) |
| P6 | `SMT.get_leaf(nullifiers_db, input.nullifier) = ZERO` | InsufficientBalance | Custom(0) |
| P7 | `!db_contains_key(coins_db, output.coin)` | InsufficientBalance | Custom(0) |
| P8 | `db_lookup` for coins_db, nullifiers_db, coin_roots_db succeeds | Custom(0) | — |

### 3.3 Formal Postconditions (Historical)

After successful exec+apply:

| # | Effect |
|---|--------|
| Q1 | `nullifiers_db[input.nullifier] = [1]` (input coin marked spent) |
| Q2 | `coins_db[output.coin] = []` (output coin registered) |
| Q3 | `coin_tree` appended with `output.coin`, new root inserted into `coin_roots_db` |
| Q4 | `fees_db[height] = fees_db[height] + fee` (saturating_add) |

### 3.4 ZK Circuit (Historical)

The Fee_V2 circuit constrains:

| Witness | Constraint |
|---------|-----------|
| input_value, output_value, fee | 64-bit range check, `input_value = output_value + fee` |
| input_coin, output_coin | Pedersen commitment to (value, value_blind) |
| nullifier | `poseidon(DOMAIN_NULLIFIER, secret, input_coin)` |
| merkle_root | Computed from `(input_coin.inner(), leaf_position, merkle_path)` |
| token_commit | `poseidon(DOMAIN_TOKEN_COMMIT, token_id=0, token_blind)` |
| signature_public | Derived from `ephemeral_signature_secret` |
| tx_binding | `poseidon(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` |

### 3.5 Test Derivation (Historical)

To construct a valid FeeV1 test call (for historical reference), the developer
SHALL answer these questions:

**Q1: Which coin is being spent?**
Must be a coin that was appended to the coin tree by a prior operation
(PoWRewardV1 or FeeCollectV1 or TransferV1 or SpendV1). Its creation root
must exist in `coin_roots_db`.

**Q2: What is the coin's leaf position?**
The position at which this coin was appended. Use §1.6 to compute from
the tree's history.

**Q3: What is the coin's merkle path?**
Use §1.8 (Theorem 1) to compute the 32 siblings from the tree state at
the time the coin was appended. The tree state = all coins up to and
including this one.

**Q4: What is the merkle root?**
The root after this coin was appended: `tree.root(0)` with the tree
containing all coins up to and including this coin.

**Q5: What key owns the coin?**
The secret key whose public key is in the coin's Pedersen commitment.
For coinbase coins, this is the mining key. For test coins, this is a
deterministic test key.

**Q6: What fee to pay?**
Must be ≥ `MIN_FEE_PER_CALL` (42,000,000). Output value = input_value - fee.
Must be > 0 (else no FeeCollectV1 is needed).

**Q7: What is the output recipient?**
Any valid public key. The FeeV1 creates a new coin owned by this key.

## 4. FeeCollectV1 — Fee Collection Entrypoint

**Function code**: `0x06`. **ZK circuit**: `FeeCollect_V2` (7 public inputs).

### 4.1 Purpose

FeeCollectV1 claims the accumulated fee pot and mints a new coin to the
miner. Closes the coin Merkle tree for the block.

For FeeV2 transactions, fees are hidden behind Pedersen commitments.
The contract accumulates `fee_value_commit` from each FeeV2 call into
`fee_commit_accumulator: pallas::Point`. FeeCollectV1 verifies the
miner's claimed total matches the commitment sum via Pedersen's
homomorphic property (§5.6).

### 4.2 Formal Preconditions

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| C1 | `fc.total_fees > 0` | `↓zero-claim` | Custom(0) |
| C2 | **FeeV2 path**: `PedersenCommit(fc.total_fees, fc.total_blind) == fee_commit_accumulator` — commitment sum matches accumulated commitments. **FeeV1 path (legacy)**: `fc.total_fees == fees_db[height]` | `↓bad-claim` | Custom(22) |
| C3 | `!db_contains_key(coins_db, fc.output.coin)` | InsufficientBalance | Custom(0) |
| C4 | `SMT.get_leaf(nullifiers_db, fc.output.nullifier) = ZERO` | InsufficientBalance | Custom(0) |
| C5 | `fc.output.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |

### 4.3 Formal Postconditions

| # | Effect |
|---|--------|
| R1 | `coins_db[fc.output.coin] = []` |
| R2 | `coin_tree` appended with `fc.output.coin`, new root in `coin_roots_db` (closes tree) |
| R3 | `fees_db[height] = 0` (prevents double-claim) |
| R4 | `fee_commit_accumulator = Identity` (resets for next block) |

### 4.4 Conditional Presence Rule

FeeCollectV1 SHALL be the final transaction when `total_fees > 0`.
FeeCollectV1 SHALL be absent when `total_fees == 0`.

**Rationale**: A zero-fee FeeCollectV1 would be a zero-value replay attack
(same nullifier reused across heights). The first zero-claim check (C1)
kills this at exec time. Building it unconditionally would produce
rejected blocks.

## 5. FeeV2 — Privacy-Preserving Fee Payment (NEW)

**Function code**: `0x08`. **ZK circuits**: `Fee_V2` (value conservation, 15 public
inputs) + `FeeThreshold_V1` (threshold proof, 2 public inputs).

FeeV2 is the privacy-preserving successor to FeeV1. It SHALL NOT expose the
fee amount in clear text. Instead, it carries a Pedersen commitment to the
fee value and a zero-knowledge threshold proof demonstrating `fee >= threshold`
without revealing `fee`. The miner learns only `total_fees` from the contract
accumulator — never individual fee amounts.

### 5.1 Purpose

Identical to FeeV1 (§3.1): spends an existing coin C, splits it into an
output coin O (change) and a fee F accumulated into `fees_db[height]`.
The difference is privacy: the fee amount is hidden from everyone except
the transaction author.

### 5.2 Call Data Format

FeeV1 call data: `[0x00][fee: u64 LE 8 bytes][FeeParamsV1 encoded]`
FeeV2 call data: `[0x08][FeeParamsV2 encoded]` — NO clear-text fee bytes.

`FeeParamsV2` replaces `fee: u64` with `fee_value_commit: pallas::Point`
(Pedersen commitment to the fee amount) and adds `threshold_proof: Vec<u8>`
(FeeThreshold_V1 proof bytes).

### 5.3 Formal Preconditions

Let `params = FeeParamsV2 { input: Input, output: Output, fee_value_commit, 
threshold_proof, fee_value_blind, tx_binding, tx_nonce }`.

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| P1 | `params = FeeParamsV2::decode(&call_data[1..])` succeeds | ParseError | Custom(2) |
| P2 | `input.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P3 | `output.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P4 | `verify_threshold_proof(params.threshold_proof, threshold, tx_binding)` — fee ≥ threshold | ↓bad-threshold-proof | Custom(0) |
| P5 | `PedersenVerify(params.fee_value_commit, fee, blind)` — commitment matches hidden fee (defense-in-depth) | Custom(0) | Custom(0) |
| P6 | `db_contains_key(coin_roots_db, input.merkle_root)` | TransferMerkleRootNotFound | Custom(13) |
| P7 | `SMT.get_leaf(nullifiers_db, input.nullifier) = ZERO` | InsufficientBalance | Custom(0) |
| P8 | `!db_contains_key(coins_db, output.coin)` | InsufficientBalance | Custom(0) |

### 5.4 Postconditions

After successful exec+apply:

| # | Effect |
|---|--------|
| Q1 | `nullifiers_db[input.nullifier] = [1]` (input coin marked spent) |
| Q2 | `coins_db[output.coin] = []` (output coin registered) |
| Q3 | `coin_tree` appended with `output.coin`, new root inserted into `coin_roots_db` |
| Q4 | `fee_commit_accumulator = fee_commit_accumulator + fee_value_commit` (Pedersen accumulation) |

The fee amount `fee` is a private witness to the Fee_V2 circuit. It is
constrained by value conservation (`input = output + fee`) within the ZK
proof. The contract never learns `fee` — it only knows `fee_value_commit`,
verifies the threshold proof, and adds the commitment to the accumulator.
The daemon patches `FeeUpdateV1.fee` from the ZK witness for the miner's
knowledge and for legacy `fees_db[height]` tracking.

### 5.5 FeeThreshold_V1 Circuit

**Purpose**: Prove `fee >= threshold` without revealing `fee`.

**Public inputs** (2 elements):

| # | Input | Type | Purpose |
|---|-------|------|---------|
| 1 | `threshold` | `Base` | The threshold the fee must meet or exceed |
| 2 | `tx_binding` | `Base` | Binds proof to transaction + threshold |

**Constraint**:
```
diff = fee - threshold
range_check(64, diff)   // diff in [0, 2^64-1] iff fee >= threshold
```
If `fee < threshold`, the subtraction underflows in the field, producing
a value near `p - (threshold - fee)` which fails the 64-bit range check.

**tx_binding**: `poseidon(DOMAIN_TX_BINDING, tx_commitment, threshold)`.
Including the threshold in the binding prevents a proof for one threshold
from being replayed against a different threshold.

**Circuit parameters**: `k = 11`, field = `pallas` (matching Fee_V2 and
FeeCollect_V1 circuits).

### 5.6 Fee Commitment Accumulation

FeeV2 hides individual fee amounts behind Pedersen commitments. To enable
fee collection without revealing individual fees, the contract accumulates
commitments additively using Pedersen's homomorphic property.

#### 5.6.1 Pedersen Homomorphic Property

```
PedersenCommit(v1, b1) + PedersenCommit(v2, b2)
  = (v1·G_v + b1·G_r) + (v2·G_v + b2·G_r)
  = (v1+v2)·G_v + (b1+b2)·G_r
  = PedersenCommit(v1+v2, b1+b2)
```

Addition on `pallas::Point` is the standard elliptic curve group operation.
This homomorphic property is the foundation of the fee accumulation scheme:
commitments can be summed without revealing individual values.

#### 5.6.2 Contract State: The Accumulator

The contract SHALL maintain `fee_commit_accumulator: pallas::Point` as
block-scoped state, initialized to the identity element (point at infinity)
at the start of each block.

```
Initial:         accumulator = Identity
After call i:    accumulator = accumulator + fee_value_commit_i
After all calls: accumulator = Σ PedersenCommit(f_i, b_i)
                             = PedersenCommit(Σf_i, Σb_i)
```

This is additive-only state: each FeeV2's `apply_fee` ADDS its
`fee_value_commit` to the accumulator. No subtraction, no overwrite.
The accumulator is reset to Identity by FeeCollectV1 (§4.3, R4).

#### 5.6.3 Privacy Model — Who Sees What

```
┌─────────────────────┬──────────────────────────────────────┐
│ Party               │ What They See                        │
├─────────────────────┼──────────────────────────────────────┤
│ Mempool / validators│ fee_value_commit + threshold_proof   │
│                     │ ONLY. Cannot learn individual fee_i. │
├─────────────────────┼──────────────────────────────────────┤
│ Block-producing     │ Extracts fee witness from each Fee_V2│
│ miner               │ ZK proof during block construction   │
│                     │ (FeeUpdateV1.fee patching). Sees     │
│                     │ each fee_i.                          │
├─────────────────────┼──────────────────────────────────────┤
│ Replaying validators│ Re-extract witnesses from proofs in  │
│                     │ the block. Verify PedersenCommit(    │
│                     │ total, blind) == accumulator WITHOUT │
│                     │ knowing individual fees.             │
└─────────────────────┴──────────────────────────────────────┘
```

**Key property**: Validators verify the total without seeing the terms.
The Pedersen homomorphic property proves correctness of the sum while
preserving privacy of the summands.

#### 5.6.4 FeeCollectV1 Verification

After executing all FeeV2 calls in the block, the miner (who extracted
individual `fee_i` from ZK witnesses) submits:

```
total_fees = Σ fee_i       (from ZK witness extraction)
total_blind = Σ blind_i    (from FeeParamsV2.fee_value_blind)
```

FeeCollectV1 precondition C2 (§4.2) verifies:

```
PedersenCommit(total_fees, total_blind) == fee_commit_accumulator
```

If the commitment matches: the miner mints a coin worth `total_fees` to
themselves, and the accumulator resets to Identity.

If the commitment does NOT match: `↓bad-claim` barb, reject.

#### 5.6.5 Soundness Theorem

**Theorem 2 (Fee Summation Soundness)**. A miner claiming `total_fees' ≠ Σf_i`
cannot satisfy `PedersenCommit(total_fees', b') == fee_commit_accumulator` for
any `b'` unless they break the Pedersen commitment binding property.

*Proof sketch.* The accumulator equals `PedersenCommit(Σf_i, Σb_i)`. For a
false claim `(total_fees', b')` to verify, we need:
```
PedersenCommit(total_fees', b') == PedersenCommit(Σf_i, Σb_i)
```
If `total_fees' ≠ Σf_i`, this pair `(total_fees', b')` and `(Σf_i, Σb_i)`
constitute an opening of the same commitment to two different values,
violating the binding property. The Pedersen commitment scheme is
computationally binding under the discrete log assumption in `pallas::Point`.

**Corollary.** A miner can only claim a total_fees value that equals the
actual sum of individual fees. Over-claiming requires breaking discrete log.

#### 5.6.6 Block Lifecycle with Commitment Accumulation

```
Block N execution:

  tx[0] = PoWRewardV1
    → coinbase coin at position P
    → fee_commit_accumulator = Identity

  tx[1] = FeeV2(f_1, b_1)
    → accumulator += PedersenCommit(f_1, b_1)
    → fees_db[N] += f_1  (daemon-patched from ZK witness)

  tx[2] = FeeV2(f_2, b_2)
    → accumulator += PedersenCommit(f_2, b_2)
    → fees_db[N] += f_2

  ...

  tx[k] = FeeCollectV1(total_fees=Σf_i, total_blind=Σb_i)
    → verify: PedersenCommit(total_fees, total_blind) == accumulator
    → mint coin(total_fees) → miner
    → accumulator = Identity
    → fees_db[N] = 0

After block: tree has N + coins_created_this_block leaves.
```

### 5.7 Test Derivation

In addition to the seven questions from FeeV1 (§3.5, historical), the
developer SHALL answer:

**Q8: What threshold is being proved against?**
Premium threshold or general threshold (see [mempool.md §5](../mempool.md)).
The threshold MUST match the tx_binding computation. A proof built for
`threshold = premium` cannot be verified against `threshold = general`.

**Q9: What is the fee commitment?**
`fee_value_commit = PedersenCommit(fee_amount, fee_blind)`. The blind SHALL
be derived deterministically from the wallet's secret. The commitment is a
public input to Fee_V2 and is stored in FeeParamsV2.

**Q10: What is the fee commitment accumulator root?**
After all FeeV2 calls in the block, the contract's `fee_commit_accumulator`
SHALL equal `PedersenCommit(Σfee_i, Σblind_i)`. The miner proves this by
providing `(total_fees, total_blind)` to FeeCollectV1. See §5.6.4.

**Q11: How does FeeCollectV1 verify the total?**
The contract checks `PedersenCommit(total_fees, total_blind) ==
fee_commit_accumulator`. The Pedersen binding property guarantees
the miner cannot over-claim. See §5.6.5.

## 6. FeeAmount — Nominal Domain Type

Per [type-system.md §2.3](type-system.md), consensus numeric domains SHALL be
nominal types. `FeeAmount(u64)` already exists at `src/sdk/src/blockchain.rs:481`.
It SHALL be applied end-to-end through the WASM boundary.

```
FeeAmount(u64) — inner u64, validating constructor.
↓denominate: identifies the fee class.
Constructor: FeeAmount::new(v) SHALL succeed for all v >= 0.
```

### 6.1 Critical Boundary — ZK Proof Witnesses

A bare `u64` SHALL NOT enter a ZK proof witness or cryptographic commitment.
All values entering `pedersen_commitment_u64()`, `poseidon_hash()`, or ZK
circuit witness construction SHALL pass through a nominal type or validated
constructor. The Fee_V2 circuit witness uses `FeeAmount` internally; the
public commitment hides the inner value.

### 6.2 High Boundary — Cross-Crate Consensus Arithmetic

Consensus arithmetic crossing crate boundaries SHALL use nominal types.
Internal-to-consensus-module arithmetic (same crate, same validation domain)
is exempt. `BlockReward.get()` and `BlockHeight.get()` at arithmetic sites
within `src/linear/` and `bin/dwowd/` are audited and accepted.

### 6.3 Medium Boundary — Display, Logging, RPC

Display, logging, and RPC serialization SHOULD use nominal types. Bare
primitives are acceptable with documented precision considerations.
`SupplyAmount.get() as f64` at the JSON-RPC boundary SHALL include a
precision guard: values above 2^53 lose integer precision in IEEE 754.

### 6.4 Domain Transitions — Documented Dispensation

`.get()` at a conversion boundary between distinct domains (e.g.,
`FeeAmount` → coin value in `FeeCollectV1`, `BlockReward` → `SupplyAmount`)
is a documented dispensation. The conversion is semantically a domain
transition, not a type escape. The pattern is `impl From<SourceType> for
TargetType` where the target type exists; where it does not (e.g., no
`CoinValue(u64)` type exists yet), `.get()` at the immediate conversion
site is accepted.

### 6.5 Structural Dispensations

The following are documented exemptions, not violations:
- **FFI boundaries** — C ABI requires primitive types
- **Atomic storage** — hardware atomics require primitive integers
- **Byte encoding** — `BlockVersion.get()` and similar encode methods
- **Fixed-base constants** — compile-time curve constants verified by tests
- **Model decode slice conversions** — `.try_into().unwrap()` on slices
  with length guaranteed by prior checks

## 7. Two-Tier Mempool

The two-tier mempool admission system, threshold announcement protocol, and
fee structure are defined in [mempool.md §5-8](../mempool.md). This section
(§7) provides the consensus-level interface; mempool.md owns the policy-level
specification.

### 7.1 Consensus Interface

The contract SHALL expose two constants for threshold verification:

```
PREMIUM_THRESHOLD: u64   — minimum fee for premium mempool tier
GENERAL_THRESHOLD: u64   — minimum fee for general mempool tier
```

These are consensus constants, defined at compile time. Changing them
requires a hard fork.

### 7.2 FeeExtractor Trait

The `FeeExtractor` trait (defined in `crates/dwow-mempool/src/lib.rs`) SHALL
provide these methods for fee extraction and threshold verification:

```
trait FeeExtractor {
    fn extract_fee_commitment(&self, tx: &Transaction) -> Option<FeeCommitment>;
    fn verify_threshold_proof(&self, tx: &Transaction, threshold: u64) -> bool;
}
```

Both methods are MANDATORY. `FeeCommitment` wraps `pallas::Point` — the
Pedersen commitment to the fee amount. For FeeV2 (0x08),
`extract_fee_commitment` reads the commitment from `FeeParamsV2`, and
`verify_threshold_proof` verifies the embedded `FeeThreshold_V1` proof.

### 7.3 Further Specification

See [mempool.md §5](../mempool.md) for the two-tier admission algorithm,
[mempool.md §6](../mempool.md) for threshold announcement via P2P gossip,
[mempool.md §7](../mempool.md) for the fee structure (WASM size × ZK
complexity × state transitions × miner multiplier), and
[mempool.md §8](../mempool.md) for `FeeExtractor` integration details.

## 8. Wallet Integration

FeeV2 transaction construction, the privacy model (who sees fee amounts),
threshold discovery, and fee estimation are specified in
[wallet.md §6.4.2](../wallet.md).

### 8.1 Transaction Construction

The wallet SHALL produce a FeeThreshold_V1 proof with every FeeV2 transaction.
Proof generation is deterministic per [wallet.md §6.1](../wallet.md).
The wallet SHALL produce dual ZK proofs: Fee_V2 (value conservation) +
FeeThreshold_V1 (threshold proof). Call data format: `[0x08][FeeParamsV2]`
with NO clear-text fee bytes.

**Threshold selection**:
- If user's chosen fee >= PREMIUM_THRESHOLD: use PREMIUM_THRESHOLD in proof
- Otherwise: use GENERAL_THRESHOLD in proof
- The actual fee paid may exceed the threshold — the proof only guarantees
  the lower bound

### 8.2 Threshold Discovery

Threshold discovery is specified in [wallet.md §6.4.2](../wallet.md) and
[mempool.md §6](../mempool.md). The wallet SHALL query connected mining
nodes for current threshold values before constructing FeeV2 transactions.

### 8.3 Privacy Model

The privacy model is specified in §5.6.3 (this document) and
[wallet.md §6.4.2](../wallet.md). Fee amounts are visible ONLY to the
block-producing miner. All other parties see commitments and threshold proofs.

## 9. Barbs

Per [type-system.md §1.1](type-system.md), every type SHALL define the barbs
its processes may exhibit. Fee operations exhibit these barbs:

| Barb | Observable Action | Exhibited By |
|------|-------------------|--------------|
| `↓pay-fee` | Exercises FeeV2 — spends a coin via nullifier, splits value into change + fee. Fee commitment accumulated into `fee_commit_accumulator` | FeeV2 |
| `↓collect-fees` | Exercises FeeCollectV1 — verifies PedersenCommit(total, blind) == accumulator, mints fee coin to miner, resets accumulator and fees_db | FeeCollectV1 |
| `↓threshold-prove` | Proves hidden fee meets public threshold — gates mempool tier admission | FeeThreshold_V1 |
| `↓bad-fee-amount` | input.value <= fee — rejected at `FeeV2CallBuilder.build()` | FeeV2 |
| `↓bad-threshold-proof` | FeeThreshold_V1 verification fails — transaction rejected from mempool | FeeThreshold_V1 |
| `↓bad-merkle-root` | Merkle root not found in coin_roots_db — rejected at `fee_v2` exec | FeeV2 |
| `↓double-spend` | Nullifier already in SMT — rejected at `fee_v2` exec | FeeV2 |
| `↓zero-claim` | FeeCollectV1 `total_fees == 0` — rejected as replay attack | FeeCollectV1 |
| `↓bad-claim` | FeeCollectV1 `PedersenCommit(total, blind) != fee_commit_accumulator` — claimed amount mismatch against commitment sum | FeeCollectV1 |

## 10. Constants

| Symbol | Value | Definition |
|--------|-------|------------|
| `PREMIUM_THRESHOLD` | TBD | Minimum fee for premium mempool tier |
| `GENERAL_THRESHOLD` | TBD | Minimum fee for general mempool tier |
| `COINBASE_MATURITY` | `100` | Blocks before coinbase coin is spendable |
| `INITIAL_REWARD` | `1_383_764_049` | Genesis block reward (1.383 DRKW) |
| `MERKLE_DEPTH` | `32` | Orchard tree depth (2^32 capacity) |
| `UNCOMMITTED_ORCHARD` | `pallas::Base::from(2)` | Empty leaf value |
| FeeV1 | `0x00` | REMOVED — returns InvalidFunction |
| FeeV2 | `0x08` | Function selector (privacy-preserving) |
| FeeCollectV1 | `0x06` | Function selector |
| PoWRewardV1 | `0x05` | Function selector |
| Fee_V2 | k=11, pallas, 24 witnesses, 15 public inputs | Fee value conservation circuit |
| FeeThreshold_V1 | k=11, pallas, 4 witnesses, 2 public inputs | Threshold proof circuit |
| `DRKW_TOKEN_ID` | `0` | Native token identifier |

## 11. Error Taxonomy

Every WASM error maps to a ContractError variant and a consensus barb.
Tests SHALL assert the specific barb, not a generic wrapper.

| Error | Barb | ContractError | Root Cause |
|-------|------|--------------|------------|
| Fee below threshold | ↓bad-threshold-proof | Custom(0) | FeeThreshold_V1 verification fails |
| Input value <= fee | ↓bad-fee-amount | Custom(0) | FeeV2CallBuilder pre-check |
| Merkle root not found | ↓bad-merkle-root | Custom(13) | Root not in coin_roots_db |
| Nullifier already spent | ↓double-spend | Custom(19) | Nullifier in SMT |
| Duplicate coin | Custom(14) | Coin already exists | Custom(14) |
| Token mismatch | ↓bad-token | Custom(0) | Wrong token_id or token_commit |
| Commitment sum mismatch | ↓bad-claim | Custom(22) | PedersenCommit(total, blind) ≠ fee_commit_accumulator |
| Zero-fee claim | ↓zero-claim | Custom(0) | FeeCollectV1 total_fees == 0 |
| Invalid signature | ↓bad-proof | Custom(1) | Bad signature public key |
| Invalid Merkle proof | ↓bad-proof | Custom(4) | Bad ZK proof merkle path |
| Value mismatch | ↓bad-proof | Custom(21) | Value commitment doesn't match |
| Parse error | ↓bad-params | Custom(2) | FeeParamsV2 decode failure |
| Value overflow | ↓bad-fee-amount | Custom(5) | u64 overflow in value computation |
