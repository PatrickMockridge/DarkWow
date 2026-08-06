# Fee Payment and Collection — Formal Specification

*Specification for FeeV1, FeeCollectV1, and the coin Merkle tree.
Theorems, invariants, and formal predicates. Tests SHALL be derived from
this document — not from reverse-engineering production code.*

## 1. Coin Merkle Tree

### 1.1 Type Definition

The coin Merkle tree is an incremental Merkle tree of commitments to
native-token coins. It is shared by all native_token functions:
PoWRewardV1 appends coins to it; FeeV1, TransferV1, SpendV1, and BurnV1
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
transactions[1..k]  = user transactions  (FeeV1, TransferV1, SpendV1, BurnV1, deploys)
transactions[k+1]   = FeeCollectV1       (fn_code 0x06) — iff total_fees > 0
```

Phase 0 structural validation enforces: exactly one coinbase at index 0,
FeeCollectV1 (if present) at the final index.

### 2.2 Sequential Execution Model

Within `execute_block`, each canonical transaction runs
`metadata()` → `exec()` → `apply()` sequentially in a shared overlay.

**Invariant 1 (Overlay Visibility)**: Call `i` observes the state writes of
calls `0..i-1` within the same block. Specifically, FeeV1's `exec()` (call
i) sees the coinbase's `apply_pow_reward()` writes (call 0), including the
merkle root inserted into `coin_roots_db`.

This is the mechanism that enables same-block fee payment: the coinbase
coin's merkle root IS visible to FeeV1 in the same block. This is NOT the
production path (where FeeV1 spends coins from prior blocks), but is a
valid test path when `tx.nullifiers` is empty (bypassing COINBASE_MATURITY).

### 2.3 Coin Tree Growth Per Block

For block at height H:

```
Starting tree: N leaves (from blocks 1..H-1)

1. PoWRewardV1 apply_pow_reward:
   append(coinbase_coin_H) → position N, root = R_H_0
   coin_roots_db[R_H_0] = ...

2. Each user FeeV1 apply_fee:
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

The contract-level SMT nullifier check (FeeV1 check #7) still applies:
the nullifier must not exist in the contract's `nullifiers_db` SMT.

## 3. FeeV1 — Fee Payment Entrypoint

**Function code**: `0x00`. **ZK circuit**: `Fee_V2` (14 public inputs).

### 3.1 Purpose

FeeV1 spends an existing coin C, splits it into:
- O: output coin returned to user (value = C.value - fee)
- F: fee accumulated into `fees_db[height]`

### 3.2 Formal Preconditions

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

### 3.3 Formal Postconditions

After successful exec+apply:

| # | Effect |
|---|--------|
| Q1 | `nullifiers_db[input.nullifier] = [1]` (input coin marked spent) |
| Q2 | `coins_db[output.coin] = []` (output coin registered) |
| Q3 | `coin_tree` appended with `output.coin`, new root inserted into `coin_roots_db` |
| Q4 | `fees_db[height] = fees_db[height] + fee` (saturating_add) |

### 3.4 ZK Circuit

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

### 3.5 Test Derivation

To construct a valid FeeV1 test call, the developer SHALL answer these
questions. If any answer is "I don't know," the spec is incomplete and
must be extended before writing test code.

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

FeeCollectV1 claims the accumulated fee pot `fees_db[height]` and mints
a new coin to the miner. Closes the coin Merkle tree for the block.

### 4.2 Formal Preconditions

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| C1 | `fc.total_fees > 0` | InsufficientBalance | Custom(0) |
| C2 | `fc.total_fees == fees_db[height]` | FeeTotalMismatch | Custom(22) |
| C3 | `!db_contains_key(coins_db, fc.output.coin)` | InsufficientBalance | Custom(0) |
| C4 | `SMT.get_leaf(nullifiers_db, fc.output.nullifier) = ZERO` | InsufficientBalance | Custom(0) |
| C5 | `fc.output.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |

### 4.3 Formal Postconditions

| # | Effect |
|---|--------|
| R1 | `coins_db[fc.output.coin] = []` |
| R2 | `coin_tree` appended with `fc.output.coin`, new root in `coin_roots_db` (closes tree) |
| R3 | `fees_db[height] = 0` (prevents double-claim) |

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

Identical to FeeV1 (§3.3): nullifier marked spent, output coin registered,
coin tree appended, fee accumulated into `fees_db[height]`.

The fee amount `fee` is a private witness to the Fee_V2 circuit. It is
constrained by value conservation (`input = output + fee`) within the ZK
proof. The contract never learns `fee` — it only knows `fee_value_commit`
and verifies the threshold proof.

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

### 5.6 Test Derivation

In addition to FeeV1's seven questions (§3.5), the developer SHALL answer:

**Q8: What threshold is being proved against?**
Premium threshold or general threshold (§7). The threshold MUST match the
tx_binding computation. A proof built for `threshold = premium` cannot be
verified against `threshold = general`.

**Q9: What is the fee commitment?**
`fee_value_commit = PedersenCommit(fee_amount, fee_blind)`. The blind SHALL
be derived deterministically from the wallet's secret. The commitment is a
public input to Fee_V2 and is stored in FeeParamsV2.

## 6. FeeAmount — Nominal Domain Type

Per [type-system.md §2.3](type-system.md), consensus numeric domains SHALL be
nominal types. `FeeAmount(u64)` already exists at `src/sdk/src/blockchain.rs:481`.
It SHALL be applied end-to-end through the WASM boundary.

```
FeeAmount(u64) — inner u64, validating constructor.
↓denominate: identifies the fee class.
Constructor: FeeAmount::new(v) SHALL succeed for all v >= 0.
```

A bare `u64` fee SHALL NOT cross module boundaries. `FeeParamsV1.fee: u64`
SHALL be migrated to `FeeAmount`. The Fee_V2 circuit witness uses `FeeAmount`
internally; the public commitment hides the inner value.

## 7. Two-Tier Mempool

### 7.1 Architecture

The mempool SHALL admit transactions based on threshold proofs, not clear-text
fee rates. Two tiers provide differentiation without revealing individual fees:

| Tier | Proof Required | Ordering | Purpose |
|------|---------------|----------|---------|
| Premium | `fee >= premium_threshold` | FCFS (arrival order) | Urgent transactions |
| General | `fee >= general_threshold` | FCFS after premium exhausted | Normal transactions |

### 7.2 Admission Gate

Every transaction entering the mempool SHALL carry a valid FeeThreshold_V1
proof. The mempool verifies the proof against the current tier thresholds:

```
tx_arrives(tx):
  if verify_threshold_proof(tx, PREMIUM_THRESHOLD):
    admit_to_premium_queue(tx)
  else if verify_threshold_proof(tx, GENERAL_THRESHOLD):
    admit_to_general_queue(tx)
  else:
    REJECT  // fee below general threshold
```

### 7.3 Block Selection

`select_for_block(max_gas, max_txs)`:
1. Drain premium queue in FIFO order until `max_gas` or `max_txs` reached
2. Drain general queue in FIFO order until limits reached
3. Return selected transactions (non-destructive — call `mark_mined` after
   block acceptance)

### 7.4 Miner Consensus on Thresholds

**Initial deployment**: `PREMIUM_THRESHOLD` and `GENERAL_THRESHOLD` are
fixed consensus constants, defined at compile time. Changing them requires
a hard fork. This follows Bitcoin's approach: `minRelayTxFee` is a fixed
policy parameter.

**Future**: Fee-estimator-driven adjustment based on observed block fullness.
The existing `FeeEstimator` infrastructure can be extended. Not required
for initial deployment.

### 7.5 FeeExtractor Integration

The `FeeExtractor` trait SHALL be extended with two new methods carrying
default implementations for backward compatibility:

```
trait FeeExtractor {
    fn extract_fee(&self, tx: &Transaction) -> u64;              // unchanged
    fn extract_fee_commitment(&self, tx: &Transaction) -> Option<FeeCommitment> { None }
    fn verify_threshold_proof(&self, tx: &Transaction, threshold: u64) -> bool { false }
}
```

`FeeCommitment` wraps `pallas::Point` — the Pedersen commitment to the fee
amount. For V1 transactions, both new methods return `None`/`false`. For V2,
`extract_fee_commitment` reads the commitment from `FeeParamsV2`, and
`verify_threshold_proof` verifies the `FeeThreshold_V1` proof.

## 8. Wallet Integration

### 8.1 Transaction Construction

The wallet SHALL produce a FeeThreshold_V1 proof with every FeeV2 transaction.
The proof generation is deterministic: the RNG seed is
`poseidon(fee_secret, threshold, tx_nonce, domain=15)`.

**Threshold selection**:
- If user's chosen fee >= PREMIUM_THRESHOLD: use PREMIUM_THRESHOLD in proof
- Otherwise: use GENERAL_THRESHOLD in proof
- The actual fee paid may exceed the threshold — the proof only guarantees
  the lower bound

### 8.2 Threshold Discovery

The wallet fetches current threshold values via:
- A new RPC method `get_thresholds` returning `(premium, general)`
- Or reading the latest block header (if thresholds are stored there)
- Or using locally-configured consensus constants

## 9. Barbs

Per [type-system.md §1.1](type-system.md), every type SHALL define the barbs
its processes may exhibit. Fee operations exhibit these barbs:

| Barb | Observable Action | Exhibited By |
|------|-------------------|--------------|
| `↓pay-fee` | Exercises FeeV1/V2 — spends a coin via nullifier, splits value into change + fee. Fee accumulated into `fees_db[height]` | FeeV1, FeeV2 |
| `↓collect-fees` | Exercises FeeCollectV1 — claims `fees_db[height]`, mints fee coin to miner, zeroes pot | FeeCollectV1 |
| `↓threshold-prove` | Proves hidden fee meets public threshold — gates mempool tier admission | FeeThreshold_V1 |
| `↓bad-fee-amount` | input.value <= fee — rejected at `FeeCallBuilder.build()` | FeeV1, FeeV2 |
| `↓bad-threshold-proof` | FeeThreshold_V1 verification fails — transaction rejected from mempool | FeeThreshold_V1 |
| `↓bad-merkle-root` | Merkle root not found in coin_roots_db — rejected at `fee_v1/v2` exec | FeeV1, FeeV2 |
| `↓double-spend` | Nullifier already in SMT — rejected at `fee_v1/v2` exec | FeeV1, FeeV2 |
| `↓zero-claim` | FeeCollectV1 `total_fees == 0` — rejected as replay attack | FeeCollectV1 |
| `↓bad-claim` | FeeCollectV1 `total_fees != fees_db[height]` — claimed amount mismatch | FeeCollectV1 |

## 10. Constants

| Symbol | Value | Definition |
|--------|-------|------------|
| `PREMIUM_THRESHOLD` | TBD | Minimum fee for premium mempool tier |
| `GENERAL_THRESHOLD` | TBD | Minimum fee for general mempool tier |
| `COINBASE_MATURITY` | `100` | Blocks before coinbase coin is spendable |
| `INITIAL_REWARD` | `1_383_764_049` | Genesis block reward (1.383 DRKW) |
| `MERKLE_DEPTH` | `32` | Orchard tree depth (2^32 capacity) |
| `UNCOMMITTED_ORCHARD` | `pallas::Base::from(2)` | Empty leaf value |
| FeeV1 | `0x00` | Function selector (clear-text fee) |
| FeeV2 | `0x08` | Function selector (privacy-preserving) |
| FeeCollectV1 | `0x06` | Function selector |
| PoWRewardV1 | `0x05` | Function selector |
| FeeThreshold_V1 | k=11, pallas | Threshold proof circuit |
| `DRKW_TOKEN_ID` | `0` | Native token identifier |

## 11. Error Taxonomy

Every WASM error maps to a ContractError variant and a consensus barb.
Tests SHALL assert the specific barb, not a generic wrapper.

| Error | Barb | ContractError | Root Cause |
|-------|------|--------------|------------|
| Fee below threshold | ↓bad-threshold-proof | Custom(0) | FeeThreshold_V1 verification fails |
| Input value <= fee | ↓bad-fee-amount | Custom(0) | FeeAmount validates at construction |
| Merkle root not found | ↓bad-merkle-root | Custom(13) | Root not in coin_roots_db |
| Nullifier already spent | ↓double-spend | Custom(19) | Nullifier in SMT |
| Duplicate coin | Custom(14) | Coin already exists | Custom(14) |
| Token mismatch | ↓bad-token | Custom(0) | Wrong token_id or token_commit |
| Fee total mismatch | ↓bad-claim | Custom(22) | Claimed fees ≠ accumulated |
| Zero-fee claim | ↓zero-claim | Custom(0) | FeeCollectV1 total_fees == 0 |
| Invalid signature | ↓bad-proof | Custom(1) | Bad signature public key |
| Invalid Merkle proof | ↓bad-proof | Custom(4) | Bad ZK proof merkle path |
| Value mismatch | ↓bad-proof | Custom(21) | Value commitment doesn't match |
| Parse error | ↓bad-params | Custom(2) | FeeParamsV1/V2 decode failure |
| Value overflow | ↓bad-fee-amount | Custom(5) | u64 overflow in value computation |
