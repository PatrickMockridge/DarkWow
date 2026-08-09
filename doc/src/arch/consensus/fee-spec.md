# Fee Payment and Collection — Formal Specification

*Specification for FeeV2, FeeCollectV1, and the coin Merkle tree.
FeeV1 (function code `0x00`, clear-text fee) is REMOVED.
Theorems, invariants, and formal predicates. Tests SHALL be derived from
this document — not from reverse-engineering production code.*

## Architecture Overview

The fee system has three ZK proofs spanning two architectural domains. These
domain terms (`mass_balance`, `fee_signalling`) are used throughout the codebase
in type names, file names, accessor methods, and variable names to distinguish
consensus-critical block proof operations from mempool/wallet coordination.

**`[domain: mass_balance]` — verified during `accept_block` via WASM (consensus-critical):**
- **Fee_V2** — Pedersen mass balance: `input = output + fee`. Proves no secret
  inflation. ZCash Orchard exploit defense-in-depth. Code lives in
  `src/contract/native_token/src/client/fee.rs`.

**`[domain: fee_signalling]` — mempool/wallet coordination (NOT consensus-critical):**
- **FeeThreshold_V1** — Proves `fee >= threshold` for mempool admission tier
  selection. Wallet→mempool gate. Code lives in
  `src/contract/native_token/src/client/fee_threshold.rs`.
  Proof construction spec: [wallet.md §6.4.3](../wallet.md).
  Mempool verification spec: [mempool.md §8.4](../mempool.md).

**`[domain: mass_balance]` — verified during `accept_block` via WASM (consensus-critical):**
- **FeeCollectV1** — Transfers accumulated fee pot to miner, resets accumulator.
  Contract logic. Code lives in `src/contract/native_token/`.

### Two-Widget Architecture — FeeThreshold_V1

FeeThreshold_V1 uses two WASM modules built from the SAME zkas circuit
(`fee_threshold_v1.zk`). Same `.zk.bin`, same constraint (`fee >= threshold`),
same k=11, same witnesses. Two wrappers with different roles:

```
┌──────────────────────────────────────────────────────────────────┐
│  fee_threshold_v1.zk                    THE GROUND TRUTH          │
│  k = 11; field = "pallas";                                       │
│  witness { fee, threshold, tx_commitment, tx_binding }           │
│  circuit {                                                       │
│    diff = base_sub(fee, threshold);                              │
│    range_check(64, diff);           // fee >= threshold          │
│    constrain_instance(threshold);   // public input #1           │
│    constrain_instance(tx_binding);  // public input #2           │
│  }                                                               │
└──────────────────────────────────────────────────────────────────┘
          │                                      │
          ▼                                      ▼
┌────────────────────────┐          ┌────────────────────────┐
│  Proving Widget        │          │  Verification Widget   │
│  (wallet-side)         │          │  (mempool + miners)    │
│                        │          │                        │
│  Embeds .zk.bin        │          │  Embeds .zk.bin        │
│  Exports:              │          │  Exports:              │
│    __metadata →        │          │    __metadata →        │
│    - witness map       │          │    - public inputs     │
│    - circuit params    │          │    - circuit params    │
│                        │          │                        │
│  Wallet loads WASM,    │          │  Mempool loads WASM,   │
│  reads witness map     │          │  calls __metadata,     │
│  FROM THE CIRCUIT,     │          │  extracts threshold +  │
│  binds witnesses by    │          │  tx_binding pub inputs,│
│  NAME (never index),   │          │  calls verify_zkp().   │
│  then Proof::create    │          │                        │
│  via native ZK stack.  │          │  Miners load same WASM │
│                        │          │  to verify mempool     │
│  NO manual Vec<Witness>│          │  isn't lying.          │
└────────────────────────┘          └────────────────────────┘
```

Specification:
- Proving widget: [wallet.md §6.4.3](../wallet.md)
- Verification widget: [mempool.md §8.4](../mempool.md)
- Circuit definition: §5.5 below

**Crate layout.** Both widgets live alongside the circuit in the contract crate:

```
src/contract/native_token/
├── proof/
│   └── fee_threshold_v1.zk          ← THE GROUND TRUTH
├── prove_fee_threshold/              ← proving widget crate (wallet-side)
│   ├── Cargo.toml                    # cdylib
│   └── src/lib.rs                    # define_contract!, metadata → witness map
└── verify_fee_threshold/             ← verification widget crate (mempool/miner-side)
    ├── Cargo.toml                    # cdylib
    └── src/lib.rs                    # define_contract!, metadata → public inputs
```

### Data Flow

```
Wallet                    Mempool                  Miner                    Chain
  │                         │                       │                        │
  ├─ FeeThreshold_V1 ──────►│                       │                        │
  │  (fee >= threshold)     ├─ premium/general/     │                        │
  │                         │  reject               │                        │
  │  [proving widget]       │  [verification widget]│                        │
  │                         │                       │                        │
  │                         │     transactions ────►│                        │
  │                         │     + fees            ├─ Build block ──────────►│
  │                         │                       │  + PoWReward            │
  │                         │                       │  + FeeCollectV1         │
  │                         │                       │                        │
  │                         │                       │  [re-verify threshold   │
  │                         │                       │   proofs via same       │
  │                         │                       │   verification widget]  │
  │                         │                       │                        ├─ Fee_V2
  │                         │                       │                        │  (no inflation)
  │                         │                       │                        ├─ FeeCollectV1
  │                         │                       │                        │  (claim + reset)
```

1. **Wallet** constructs FeeThreshold_V1 proof via the proving WASM widget
   (fee >= threshold) for mempool admission.
2. **Mempool** verifies the proof via the verification WASM widget and assigns
   a tier (premium/general) or rejects.
3. **Miner** collects pending transactions + fees from the mempool, builds a block
   with PoWReward + FeeCollectV1. Also loads the verification WASM widget to
   independently confirm the mempool isn't lying about proof validity.
4. **Chain** verifies Fee_V2 mass balance (no inflation) and FeeCollectV1
   (accumulator reset) during `accept_block`.

The Fee_V2 proof stays in the contract crate because it's defensive — verified
via WASM during block acceptance. FeeThreshold_V1 lives in the contract crate's
`client` module following the same pattern as every other ZK proof builder
(`fee.rs`, `burn.rs`, etc.) — the wallet and mempool import it from there, not
from a wallet-local copy.
belongs in transaction construction code, not contract logic.

### §0.1 Process Engineering Analogy

In Bitcoin, the fee system is transparent: you can see every transaction amount,
every fee, and the coinbase output directly on the ledger. The relationship
between fee payment and block reward is self-evident.

In a privacy-preserving system with hidden fees (Pedersen commitments) and
zero-knowledge proofs, you cannot "see inside the pipe." You need instrumentation
and proofs — exactly as in chemical and process engineering, where you can't see
inside a distillation column, reactor, or pipeline and must rely on flow meters,
pressure gauges, and control valves.

The DarkWow fee architecture maps directly to these process engineering concepts:

```
                        ┌─────────────────────────────┐
     transactions ────▶ │  FEE SIGNALLING              │
                        │  (flow control valve)         │
                        │                               │
                        │  threshold = choke position   │
                        │  higher fee = more pressure   │
                        │  drop required to pass        │
                        │                               │
                        │  fee window = PID controller  │
                        │  adapts thresholds to         │
                        │  observed congestion          │
                        └──────────────┬────────────────┘
                                       │
                                       │  admitted transactions
                                       │  (with fee commitments)
                                       ▼
                        ┌─────────────────────────────┐
                        │  MASS BALANCE                │
                        │  (flow meter / totalizer)     │
                        │                               │
                        │  Pedersen mass balance        │
                        │  proves: Σoutputs + Σfees     │
                        │  + Σburns == Σinputs          │
                        │                               │
                        │  nothing created,             │
                        │  nothing destroyed            │
                        │                               │
                        │  fee_commit_accumulator =     │
                        │  running totalizer reading    │
                        └─────────────────────────────┘
```

**Fee Signalling — The Control Valve**

The mempool's fee threshold system is a flow control valve on the transaction
pipeline. The threshold is the choke position: a higher threshold means more
pressure drop (fee) is required for a transaction to pass through to the
mempool.

- **Two-stage valve**: Premium tier (high choke) and general tier (low choke).
  Transactions must prove `fee >= threshold` via FeeThreshold_V1 to enter
  either tier. Below general threshold: REJECT (valve closed).
- **PID controller**: The fee window (`FeeWindowState`) observes congestion
  (block fill rate vs capacity) and adapts thresholds up or down — exactly as a
  PID controller adjusts a valve based on process variable vs setpoint.
- **Anti-tamper seal**: The `tx_binding` field in FeeThreshold_V1 binds the
  proof to a specific threshold, preventing replay against a different choke
  setting (see §5.5).

**Mass Balance — The Flow Meter**

The Pedersen mass balance proof is a flow totalizer: it proves that for every
block, Σoutputs + Σfees + Σburns == Σinputs. Monetary mass is conserved —
nothing can be created or destroyed except through the explicitly-audited
coinbase.

- **How the meter works**: Each FeeV2 transaction carries a `fee_value_commit`
  (Pedersen commitment to the fee amount). These commitments accumulate in
  `fee_commit_accumulator` across the block — each one is a *pulse* on the
  totalizer. FeeCollectV1 verifies that the accumulator matches the claimed
  total, then resets it to Identity (zeroes the meter) for the next block.
- **Why Pedersen**: You cannot see individual fee amounts inside the pipe.
  Pedersen commitments are computationally hiding (no information about the fee
  value leaks). But their homomorphic property allows the verifier to sum them:
  `Commit(f₁, b₁) + Commit(f₂, b₂) = Commit(f₁+f₂, b₁+b₂)`. The meter works
  blind — it verifies the sum without knowing any individual term.
- **Consensus-critical**: The meter reading is verified during `accept_block`.
  If it fails, the block is rejected. This is the defense-in-depth against
  hidden inflation (ZCash Orchard exploit class). See `consensus.md` §Supply
  Audit for the complete mass balance specification.

**Dual-Domain Instrument: FeeV2 (0x08)**

`MassBalanceFeeV2CallData` is the only type that carries both signals from a
single instrument tap:

| Barb | Domain | Role in Analogy |
|------|--------|-----------------|
| `↓pay-fee` | mass_balance | Value conservation — input = output + fee. The meter's flow equation. |
| `↓threshold-prove` | fee_signalling | Threshold proof for mempool admission. The valve's choke check. |

In process engineering terms: a combined pressure/temperature sensor that feeds
both the flow computer (mass balance) and the valve controller (fee signalling)
from a single instrument tap on the pipe.

**Separation of Concerns**

| Concern | Domain | Location | Analogy |
|----------|--------|----------|---------|
| Fee threshold proofs | fee_signalling | `FeeSignallingExtractor` trait, `src/contract/native_token/src/client/fee_threshold.rs` | Control valve + pressure gauge |
| Mempool admission gating | fee_signalling | `crates/dwow-mempool/src/lib.rs` | Valve actuation (open/close) |
| Fee window adaptation | fee_signalling | `src/linear/src/fee_window.rs` | PID controller |
| Pedersen mass balance | mass_balance | `src/linear/src/validation.rs`, `src/contract/native_token/` | Flow meter / totalizer |
| Fee commitment accumulation | mass_balance | `src/linear/src/chain_state.rs` (`fee_commit_accumulator`) | Totalizer register |
| Coinbase reward verification | mass_balance | `consensus-coinbase.md` §2 | Meter-opening event |
| Fee collector accumulator reset | mass_balance | `consensus-coinbase.md` §3 | Meter-close + reading event |

This separation of concerns is why the HAZOP naming convention (Phase 0) renamed
all types to make domain membership obvious: `mass_balance` operations are
consensus-critical (meter fraud == hidden inflation); `fee_signalling` operations
are non-consensus coordination (valve misconfiguration degrades UX but cannot
create money).

See: `consensus.md` §Supply Audit for the complete mass balance metering specification.
See: `consensus-coinbase.md` §2-3 for the meter endpoint events.

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

## 2. Block Production Model `[domain: mass_balance]`

PoWRewardV1 is not a fee type — it is the consensus-critical block-opening
coinbase, part of the Pedersen mass balance proof. Its full specification
is in [consensus.md](consensus.md) "PoWRewardV1 Nullifier Claim." It is
listed here only for block production ordering context.

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
Fee is computed via the two-component formula: `((wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)) / SCALE`.
See §12.4.1 for the full specification. Output value = input_value - fee.
Must be > 0 (else no FeeCollectV1 is needed).

**Q7: What is the output recipient?**
Any valid public key. The FeeV1 creates a new coin owned by this key.

## 4. FeeCollectV1 — Fee Collection Entrypoint `[domain: mass_balance]`

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

## 5. FeeV2 — Privacy-Preserving Fee Payment `[domain: mass_balance + fee_signalling]`

FeeV2 is dual-domain. Its Fee_V2 circuit performs Pedersen mass balance
(`↓pay-fee` — consensus-critical, verified during `accept_block`). Its
FeeThreshold_V1 circuit proves fee meets threshold (`↓threshold-prove` —
fee_signalling, verified at mempool admission). The `MassBalanceFeeV2CallData`
type (type-system.md §8.2.3) carries both barbs.

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

FeeV1 call data: `[0x00][fee: u64 LE 8 bytes][FeeParamsV1 encoded]` (REMOVED)
FeeV2 call data SHALL use the nominal `MassBalanceFeeV2CallData` type per
[type-system.md §8.2.3](../type-system.md). Its `encode()` method produces
`[0x08][FeeParamsV2::encode()]`. Consumers SHALL re-lift via
`MassBalanceFeeV2CallData::from_bytes(&data)` — the single absorber boundary per
type-system.md §10.5. No code path SHALL inspect `data[0]` to determine
the fee function; that determination SHALL come from the `↓gate` barb on
the `MassBalanceFeeV2CallData` name.

`FeeParamsV2` replaces `fee: u64` with `fee_value_commit: pallas::Point`
(Pedersen commitment to the fee amount) and adds `threshold_proof: Vec<u8>`
(FeeThreshold_V1 proof bytes).

### 5.3 Formal Preconditions

Let `params = FeeParamsV2 { input: Input, output: Output, fee_value_commit, 
threshold_proof, fee_value_blind, fee_v2_tx_binding, threshold_tx_binding, tx_nonce }`.

| Field | Type | Purpose |
|---|---|---|
| `fee_v2_tx_binding` | `FeeV2TxBinding` | Anti-replay for Fee_V2 proof — `poseidon(3, tx_commitment, tx_nonce)` |
| `threshold_tx_binding` | `ThresholdTxBinding` | Anti-replay for FeeThreshold_V1 proof — `poseidon(3, tx_commitment, threshold)` |

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| P1 | `params = FeeParamsV2::decode(&call_data[1..])` succeeds | ParseError | Custom(2) |
| P2 | `input.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P3 | `output.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P4 | `verify_threshold_proof(params.threshold_proof, threshold, params.threshold_tx_binding)` — fee ≥ threshold, binding matches proof | ↓bad-threshold-proof | Custom(0) |
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

#### 5.5.1 Type Contract — Nominal tx_binding Types

The term `tx_binding` names a Poseidon hash used for anti-replay across two
distinct ZK circuits. Both circuits use the same domain constant
`DOMAIN_TX_BINDING = 3`, but the second input differs — and therefore the
semantic meaning of the hash differs:

| Nominal Type | Computation | Circuit | Purpose |
|---|---|---|---|
| `FeeV2TxBinding` | `poseidon(3, tx_commitment, tx_nonce)` | fee.zk (Fee_V2) | Anti-replay: proof bound to a specific transaction |
| `ThresholdTxBinding` | `poseidon(3, tx_commitment, threshold)` | fee_threshold_v1.zk | Anti-replay: proof bound to a specific threshold tier |

These SHALL be distinct nominal types wrapping `pallas::Base`. The compiler
SHALL reject any assignment of `FeeV2TxBinding` to a slot expecting
`ThresholdTxBinding` (and vice versa). This follows the mass-balance naming
precedent where `input_blind` / `fee_blind` / `output_blind` are distinct
named types rather than bare `pallas::Scalar`.

**Rationale**: Before nominal typing, both values were stored as bare
`pallas::Base` in `FeeParamsV2.tx_binding`. A prover could supply
`poseidon(3, commit, tx_nonce)` (the Fee_V2 binding) where the
FeeThreshold_V1 verifier expects `poseidon(3, commit, threshold)`.
The compiler could not detect this because both are `pallas::Base`.
Nominal types make this collision a compile-time error.

**Constructors**:
- `FeeV2TxBinding::compute(tx_commitment, tx_nonce) → FeeV2TxBinding`
- `ThresholdTxBinding::compute(tx_commitment, threshold) → ThresholdTxBinding`

**Accessor**: `.inner() → pallas::Base` — explicit extraction, no `From`/`Into`.

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

### 5.8 MassBalanceFeeV2CallData — Nominal Call Data Type `[domain: mass_balance + fee_signalling]`

FeeV2 call data SHALL be represented by the nominal `MassBalanceFeeV2CallData` type,
declared in [type-system.md §8.2.3](../type-system.md). This type eliminates
raw-byte dispatch (`data[0] == 0x08`) from the fee system. It is dual-domain:
the `↓pay-fee` barb carries mass_balance authority (verified during `accept_block`);
the `↓threshold-prove` barb carries fee_signalling authority (verified at mempool
admission).

**Rho-calculus type signature:**
```
MassBalanceFeeV2CallData ≡ νselector, params. (
    selector!(0x08)          — MassBalanceFeeV2Selector, zero-sized witness
    | params!(FeeParamsV2)    — deserialized, validated FeeParamsV2
    | ↓gate                   — constrains function to FeeV2 (exhibited by selector)
    | ↓pay-fee       [domain: mass_balance]     — Pedersen value conservation + nullifier
    | ↓threshold-prove [domain: fee_signalling]  — fee ≥ threshold ZK proof
)
```

**Constructor (wallet side):**
```
MassBalanceFeeV2CallData::new(params: FeeParamsV2) → MassBalanceFeeV2CallData
```
The selector `0x08` is implicit — it is a property of the TYPE. The wallet SHALL
NOT manually prepend a selector byte. The `MassBalanceFeeV2CallData` carries the `↓gate`,
`↓pay-fee` [mass_balance], and `↓threshold-prove` [fee_signalling] barbs into the mempool.

**Absorber boundary (mempool/miner/chain side):**
```
MassBalanceFeeV2CallData::from_bytes(data: &[u8]) → Option<MassBalanceFeeV2CallData>
```
This is the SINGLE site where raw bytes are re-lifted to the nominal type. It
validates:
1. `data[0] == 0x08` (selector byte matches)
2. `FeeParamsV2::decode(&data[1..])` succeeds (params are well-formed)

Returns `None` if either check fails. The `Option` return forces every consumer
to handle both `Some(mb_fee_v2)` (valid FeeV2, barb-carrying) and `None`
(not a FeeV2 call). The compiler SHALL enforce this exhaustiveness. Per
type-system.md §10.5, this is the re-lift validation obligation at the absorber
boundary.

**Encoder (persistence/wire boundaries only):**
```
MassBalanceFeeV2CallData::encode() → Vec<u8>
```
Produces `[0x08][FeeParamsV2::encode()]`. Only used at serialization boundaries
per type-system.md §2.2. The byte sequence is identical to the pre-nominal
encoding — this change is at the type level, not the wire level.

**Barbs exhibited:**
| Barb | Domain | Exhibited by | Meaning |
|------|--------|-------------|---------|
| `↓gate` | dispatch | `MassBalanceFeeV2Selector` | Constrains function dispatch to FeeV2 specifically — the selector is `0x08` by construction |
| `↓pay-fee` | mass_balance | `MassBalanceFeeV2CallData` | The call data carries a Fee_V2 proof (Pedersen mass balance) and a nullifier |
| `↓threshold-prove` | fee_signalling | `MassBalanceFeeV2CallData` | The call data carries a FeeThreshold_V1 proof (fee ≥ threshold) |

**Contrast with raw-byte dispatch.** Before this type existed, the mempool,
miner, validation, and chain state all inspected `data[0] == 0x08` to route
transactions. Per the rho-calculus, `quote(data[0])?(b).([b = 0x08]...)` —
a raw byte with no behavioral constraints gates the entire FeeV2 path. An
adversary can send `[0x08][arbitrary_garbage]` and the `[b = 0x08]` guard
fires `true`, routing garbage into the FeeV2 path where `FeeParamsV2::decode`
eventually fails. The nominal type closes this gap: garbage never constructs
a `MassBalanceFeeV2CallData`, so it never crosses the admission gate.

**Bisimulation.** For honest senders (who always construct valid `MassBalanceFeeV2CallData`),
the byte-level and type-level processes are strongly bisimilar (P ∼ Q). For
adversarial senders, they diverge: the raw-byte process enters `FeeV2Path!`
before failing at param decode; the nominal-type process returns `None` at
the absorber boundary and never enters the fee path. The nominal type provides
strictly better security.

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

Threshold values are derived from `compute_fee()` at each fee window boundary
(see §12). The genesis block defines initial values; thereafter the
PID-controlled CongestionFactor governs adjustments. Miners signal updated
thresholds via the `fee_window_flags` field of each block header.

```
PREMIUM_THRESHOLD: u64   — minimum fee for premium mempool tier
GENERAL_THRESHOLD: u64   — minimum fee for general mempool tier
```

### 7.2 FeeSignallingExtractor Trait `[domain: fee_signalling]`

The `FeeSignallingExtractor` trait (defined in `crates/dwow-mempool/src/lib.rs`)
SHALL provide these methods for fee extraction and threshold verification.
It serves the fee_signalling domain exclusively — its methods extract fee
commitments and verify threshold proofs at mempool admission, never during
`accept_block`.

```
trait FeeSignallingExtractor {
    fn extract_fee_commitment(&self, tx: &Transaction) -> Option<FeeCommitment>;
    fn verify_threshold_proof(&self, tx: &Transaction, threshold: u64) -> bool;
}
```

Both methods are MANDATORY. `FeeCommitment` wraps `pallas::Point` — the
Pedersen commitment to the fee amount. For FeeV2 (0x08),
`extract_fee_commitment` reads the commitment from `FeeParamsV2`, and
`verify_threshold_proof` verifies the embedded `FeeThreshold_V1` proof.

**Verification path.** `verify_threshold_proof` SHALL use the verification
WASM widget ([mempool.md §8.4](../mempool.md)) to cryptographically verify
the ZK proof:

1. Load the verification WASM widget from the contracts sled tree.
2. Call `__metadata` with the FeeV2 call data → returns
   `[(FeeThreshold_V1, [threshold, tx_binding])]`.
3. Load `fee_threshold_v1.zk.bin` from the contracts sled tree.
4. Call `verify_zkp(threshold_proof, zkbin, [threshold, tx_binding])`.
5. Return `true` iff cryptographic verification succeeds.

The plain `params.threshold` u64 field is user-supplied and SHALL NOT be
trusted as a gate. Only cryptographic ZK proof verification constitutes a
valid admission check. The two-widget architecture diagram is at
[fee-spec.md §0](#architecture-overview).

**Miner re-verification.** Miners SHALL independently load the same
verification WASM widget and re-verify threshold proofs before including
transactions in a block. This closes the trust gap — the miner does not
blindly trust the mempool's word that a proof verified.

### 7.3 Further Specification

See [mempool.md §5](../mempool.md) for the two-tier admission algorithm,
[mempool.md §6](../mempool.md) for threshold announcement via P2P gossip,
[mempool.md §7](../mempool.md) for the fee structure (WASM size × ZK
complexity × state transitions × miner multiplier), and
[mempool.md §8](../mempool.md) for `FeeSignallingExtractor` integration
details including the verification WASM widget flow (§8.4).

## 8. Wallet Integration

FeeV2 transaction construction, the privacy model (who sees fee amounts),
threshold discovery, and fee estimation are specified in
[wallet.md §6.4.2](../wallet.md) (Fee_V2 fee payment) and
[wallet.md §6.4.3](../wallet.md) (FeeThreshold_V1 threshold proof).

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

| Barb | Domain | Observable Action | Exhibited By |
|------|--------|-------------------|--------------|
| `↓pay-fee` | mass_balance | Exercises FeeV2 — spends a coin via nullifier, splits value into change + fee. Fee commitment accumulated into `fee_commit_accumulator` | FeeV2, MassBalanceFeeV2CallData |
| `↓collect-fees` | mass_balance | Exercises FeeCollectV1 — verifies PedersenCommit(total, blind) == accumulator, mints fee coin to miner, resets accumulator and fees_db | FeeCollectV1, MassBalanceFeeCollectV1CallData |
| `↓threshold-prove` | fee_signalling | Proves hidden fee meets public threshold — gates mempool tier admission. Uses `ThresholdTxBinding` for anti-replay — the proof is cryptographically bound to a specific threshold value. | FeeThreshold_V1, MassBalanceFeeV2CallData |
| `↓fee-window-open` | fee_signalling | Window boundary detected — miner emits threshold signal | FeeWindow |
| `↓fee-window-advertise` | fee_signalling | Mempool advertises current thresholds via P2P | FeeWindow |
| `↓fee-window-enforce` | fee_signalling | Mempool enforces current window's thresholds at admission | FeeWindow |
| `↓fee-window-discover` | fee_signalling | Wallet queries mining nodes for threshold values | FeeWindow |
| `↓bad-fee-amount` | mass_balance | input.value <= fee — rejected at `FeeV2CallBuilder.build()` | FeeV2 |
| `↓bad-threshold-proof` | fee_signalling | FeeThreshold_V1 verification fails — transaction rejected from mempool | FeeThreshold_V1 |
| `↓bad-merkle-root` | mass_balance | Merkle root not found in coin_roots_db — rejected at `fee_v2` exec | FeeV2 |
| `↓double-spend` | mass_balance | Nullifier already in SMT — rejected at `fee_v2` exec | FeeV2 |
| `↓zero-claim` | mass_balance | FeeCollectV1 `total_fees == 0` — rejected as replay attack | FeeCollectV1, MassBalanceFeeCollectV1CallData |
| `↓bad-claim` | mass_balance | FeeCollectV1 `PedersenCommit(total, blind) != fee_commit_accumulator` — claimed amount mismatch against commitment sum | FeeCollectV1, MassBalanceFeeCollectV1CallData |

## 10. Constants

| Symbol | Domain | Value | Definition |
|--------|--------|-------|------------|
| `BASELINE_STORAGE` | fee_signalling | `1_000_000` | Per-kB WASM storage cost (0.01 DRKW at CF=1.0) |
| `OPCODE_DIFFICULTY` | fee_signalling | §12.4.2 table | Per-opcode ZK complexity factors (consensus-critical) |
| `WASM_CF` | fee_signalling | `CongestionFactor` | WASM deploy congestion multiplier (premium + standard) |
| `CIRCUIT_CF` | fee_signalling | `CongestionFactor` | Circuit execution congestion multiplier (premium + standard) |
| `COINBASE_MATURITY` | mass_balance | `100` | Blocks before coinbase coin is spendable |
| `INITIAL_REWARD` | mass_balance | `1_383_764_049` | Genesis block reward (1.383 DRKW) |
| `MERKLE_DEPTH` | mass_balance | `32` | Orchard tree depth (2^32 capacity) |
| `UNCOMMITTED_ORCHARD` | mass_balance | `pallas::Base::from(2)` | Empty leaf value |
| FeeV1 | mass_balance | `0x00` | REMOVED — returns InvalidFunction |
| FeeV2 | mass_balance + fee_signalling | `0x08` | Function selector (privacy-preserving, dual-domain) |
| FeeCollectV1 | mass_balance | `0x06` | Function selector (fee accumulator reset) |
| PoWRewardV1 | mass_balance | `0x05` | Function selector (coinbase nullifier claim) |
| Fee_V2 | mass_balance | k=11, pallas, 24 witnesses, 15 public inputs | Fee value conservation circuit |
| FeeThreshold_V1 | fee_signalling | k=11, pallas, 4 witnesses, 2 public inputs | Threshold proof circuit |
| `FeeV2TxBinding` | mass_balance | `poseidon(3, tx_commitment, tx_nonce)` | Fee_V2 proof anti-replay binding |
| `ThresholdTxBinding` | fee_signalling | `poseidon(3, tx_commitment, threshold)` | FeeThreshold_V1 proof anti-replay binding |
| `DRKW_TOKEN_ID` | mass_balance | `0` | Native token identifier |
| `SCALE` | fee_signalling | `1_000_000` | CongestionFactor fixed-point scale (CF at zero congestion) |
| `ALPHA_PREMIUM` | fee_signalling | `0.05` | Log₂ coefficient for premium CF |
| `ALPHA_STANDARD` | fee_signalling | `0.01` | Log₂ coefficient for standard CF |
| `MAX_ADJUSTMENT` | fee_signalling | `0.10` | Maximum ±10% CF change per window (I7) |
| `FEE_WINDOW_SIZE` | fee_signalling | `20` | Blocks per fee window |
| `FEE_WINDOW_TRANSITION_DELAY` | fee_signalling | `30` | Seconds after boundary block before new thresholds activate (§12.8.4) |
| `DEFAULT_PREMIUM` | fee_signalling | `2_000_000` | Initial premium threshold at genesis (2× SCALE) |
| `DEFAULT_GENERAL` | fee_signalling | `1_000_000` | Initial general threshold at genesis (1× SCALE) |
| `K_REF` | fee_signalling | `11` | Reference k for circuit difficulty scaling (§12.11.4) |
| `MAX_K` | fee_signalling | `16` | Maximum allowed k value (`src/zkas/constants.rs`) |
| `MAX_SCALE` | fee_signalling | `32` | `2^(MAX_K − K_REF)` — maximum circuit difficulty multiplier |

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

## 12. Fee Window Signalling — Adaptive Congestion Control `[domain: fee_signalling]`

*Specification for dynamic fee threshold adjustment across 20-block windows.
Formalized in rho-calculus with congestion-factor-driven pricing. Modular,
feature-gated implementation under `#[cfg(feature = "fee-window")]`.
Specification first, Python model second, Rust implementation third.*

### 12.1 Rho-Calculus Process Model

The fee window is a timed process emitting threshold signals at window
boundaries. Each signal propagates to the mempool (admission gate),
the wallet (proof construction), and the miner (block assembly).

```
FeeWindow(w, CF_premium, CF_standard, N) =
    nu premium, general. (
        WindowTick!(premium, general) |
        !(WindowTick?(p, g). (
            Mempool!(p, g) |
            Wallet!(p, g) |
            Miner!(window_end(p, g)) |
            FeeWindow(w+1, CF_premium', CF_standard', N)
        ))
    )

where:
    w              = current window index, starting at 0
    N              = window size in blocks (N = 20)
    CF_premium     = congestion factor for premium-tier circuits (rate ≥ 5)
    CF_standard    = congestion factor for standard circuits (rate 1–3)
    CF_premium'    = recomputed from mempool queue depth at window boundary
    CF_standard'   = recomputed from mempool queue depth at window boundary
    WindowTick     = signal emitted when height ≡ 0 (mod N), height > 0
    Mempool!(p,g)  = mempool receives (premium_threshold, general_threshold)
    Wallet!(p,g)   = wallet discovers (premium_threshold, general_threshold)
    Miner!(...)    = miner encodes threshold signal in block header
```

The process restarts with recomputed congestion factors at each window boundary.
Between boundaries, thresholds are stable — the mempool enforces the current
window's values, the wallet constructs proofs against them, and all participants
observe a consistent fee regime.

### 12.2 Barb Semantics

Four new barbs partition the fee window's trajectory space:

| Barb | Action | Precondition | Postcondition |
|------|--------|-------------|---------------|
| `↓fee-window-open` | Window boundary at `height ≡ 0 (mod N)` | `height > 0` | `CF_premium`, `CF_standard` recomputed |
| `↓fee-window-advertise` | Miner sets `fee_window_flags` in BlockHeader | Block is final in window w | Next window's thresholds encoded in header |
| `↓fee-window-enforce` | Mempool applies thresholds to new arrivals | Window w is active | Tx admitted/rejected per tier, FCFS within tier |
| `↓fee-window-discover` | Wallet reads `fee_window_flags` from latest header | Threshold bytes present | Wallet constructs FeeThreshold_V1 proof against active threshold |

These barbs are additive to the existing fee barbs (§9). The `↓fee-window-open`
barb fires exactly once per window boundary and is the trigger for all
subsequent window-transition actions.

### 12.3 Nominal Types

Three nominal types govern fee window state, following type-system.md §8.5:

```
FeeWindowId(u64)         — window index, computed as floor((height - 1) / N)
WindowSignalling(u8)     — bitfield encoding fee window state in block header
CongestionFactor(u32)    — compound type encapsulating two CfValue(u32) components
                           (premium and standard), 1.0 = SCALE = 1_000_000
```

Additional domain types for fee arithmetic per type-system.md §2.3.1:

```
CfValue(u32)             — congestion factor fixed-point value (1.0 = 1_000_000)
RiskFactor(u64)          — execution risk multiplier in RISK_FACTOR_SCALE units
                           (100_000 = 1.0×), applied to circuit component only
WasmKb(u64)              — WASM deploy size in kilobytes
ThresholdAmount(u64)     — mempool admission threshold, distinct from FeeAmount
```

Tier classification is proof-based: a transaction is premium if its
FeeThreshold_V1 proof verifies against the premium threshold; general if
it verifies against the general threshold. No static circuit classification
type is needed — the proof itself determines the tier.

All follow the `#[repr(transparent)]` pattern. `FeeWindowId` implements `succ()`,
`pred()`, `from_height(height, N)`. `CongestionFactor` implements fixed-point
arithmetic with `SCALE = 1_000_000`, providing `apply_premium(FeeAmount) -> FeeAmount`
and `apply_standard(FeeAmount) -> FeeAmount` to compute congestion-adjusted fees.
The compound type encapsulates two `CfValue(u32)` components; external code
SHALL use the accessor methods and `apply_*` functions rather than extracting
raw `u32` values.

### 12.4 Fee Computation

#### 12.4.1 Formula

```
fee = ((wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)) / SCALE

where:
    wasm_kB                    = max(1, ceil(wasm_bytes / 1024))
    BASELINE_STORAGE           = 1_000_000  (0.01 DRKW/kB at CF=1.0)
    WASM_CF                    = congestion factor for WASM deploy (premium tier)
    Σ opcode_difficulty        = sum of per-opcode ZK difficulty factors (§12.4.2)
    CIRCUIT_CF                 = congestion factor for circuit execution (premium tier)
```

This is a sum of two independent components: storage cost (WASM deploy size)
and computation cost (sum of per-opcode ZK difficulties). Each component has
its own congestion factor — WASM_CF for storage and CIRCUIT_CF for execution —
allowing miners to price storage and computation independently based on demand.

The formula always uses CF premium multipliers to determine the admission
threshold. Tier classification (premium vs general) compares the offered fee
against premium and standard CF thresholds separately.

#### 12.4.2 Per-Opcode Difficulty Table

Each ZK opcode carries a consensus-critical difficulty factor proportional to
its constraint system complexity (gate_count × column_count). The fee for a
transaction is the sum of its constituent opcode difficulties multiplied by
the circuit congestion factor.

| Category | Opcodes | Difficulty |
|----------|---------|------------|
| ECC | EcAdd, EcMul, EcMulBase, EcMulShort, EcMulVarBase, EcGetX, EcGetY | 1000 |
| Sinsemilla/Merkle | MerkleRoot, SparseMerkleRoot, SetMembership | 800 |
| Poseidon | PoseidonHash | 500 |
| BaseDiv | BaseDiv | 250 |
| Range/LessThan | RangeCheck, LessThanStrict, LessThanLoose, LessThanOrEqual, BaseLtStrict | 100 |
| BaseMul | BaseMul | 50 |
| Selection | CondSelect, ZeroCondSelect | 40 |
| Comparison | IsEqualBase, IsNotEqualBase, BoolCheck, NotBase | 30 |
| BaseAdd/Sub/Witness | BaseAdd, BaseSub, WitnessBase | 20 |
| Constrain | ConstrainEqualBase, ConstrainEqualPoint, ConstrainInstance | 5 |
| Noop/Debug | Noop, DebugPrint | 0 |

```
circuit_difficulty(ops) = Σ opcode_difficulty(op)
```

An average circuit (~20 mixed opcodes) sums to approximately 1000 difficulty
units. At CF=1.0 this yields a circuit execution cost of ~0.01 DRKW.

The difficulty table is consensus-critical — all wallet, mempool, and miner
implementations SHALL use identical values. The table is hardcoded rather than
derived from manifests to prevent manifest parsing from becoming a consensus
dependency.

#### 12.4.3 WASM Deployment Size

For `DeployV1` transactions, the WASM binary size incurs a proportional
storage cost:

```
wasm_kB_size = max(1, ceil(wasm_bincode.len() / 1024))
```

For all other transactions, `wasm_kB_size = 1`. This ensures large
contract deployments pay proportionally for on-chain storage while
standard transactions pay only for computation.

#### 12.4.4 Congestion Factor

The congestion factor maps mempool queue depth to a dimensionless multiplier
using logarithmic scaling. Separate factors are computed for premium and
standard tiers:

```
CF_premium  = SCALE + α_premium  × floor(SCALE × log₂(P_premium  + 1))
CF_standard = SCALE + α_standard × floor(SCALE × log₂(P_standard + 1))

where:
    SCALE        = 1_000_000          (fixed-point scale for integer arithmetic)
    P_premium    = pending count in mempool premium queue
    P_standard   = pending count in mempool general queue + fee_index
    α_premium    = premium congestion sensitivity coefficient
    α_standard   = standard congestion sensitivity coefficient
    α_premium > α_standard > 0       (premium always more sensitive)
    CF_premium > CF_standard         (structural invariant, always)
```

**Why logarithmic:** Doubling the queue depth adds at most `α × SCALE` to the
congestion factor. This prevents both premature saturation (linear) and
insufficient responsiveness (constant). The log₂ function maps queue depths
from 1 to 10,000 into congestion factors from 1.0 to ~1.0 + 13α.

**Coefficient defaults:**

```
α_premium  = 0.05   (CF doubles every ~1,000,000 premium transactions)
α_standard = 0.01   (CF doubles every ~2,000,000 standard transactions)
```

These defaults produce reasonable congestion pricing at mainnet scale while
remaining testable in devnet with smaller mempool sizes.

**Congestion factor consensus:** At each window boundary, every mining node
computes CF from its local mempool state. Nodes gossip their proposed CF
via the threshold announcement protocol (mempool.md §6). The MEDIAN CF
across all actively mining nodes becomes the window's consensus congestion
factor. This prevents single-miner manipulation — a miner with an empty
mempool cannot force CF to 1, nor can a miner with an artificially inflated
mempool force an extreme CF.

### 12.5 Threshold Computation from Congestion Factors

The two congestion factors (WASM_CF and CIRCUIT_CF) are applied via
`compute_fee()` to derive the minimum admission fee for a transaction:

```
compute_fee(circuit_costs, wasm_kB, wasm_cf, circuit_cf):
    total_opcode_cost = Σ circuit_costs
    wasm_part    = (wasm_kB × BASELINE_STORAGE × wasm_cf.premium) / SCALE
    circuit_part = (total_opcode_cost × circuit_cf.premium) / SCALE
    return wasm_part + circuit_part
```

At zero congestion (WASM_CF = CIRCUIT_CF = SCALE = 1_000_000):

```
min_fee = wasm_kB × 1_000_000 + Σ circuit_costs
```

For a non-deploy transaction (wasm_kB = 1) with average circuit difficulty
(~1000): min_fee ≈ 1_001_000 (0.01 DRKW).

For tier classification, the standard-tier minimum is computed identically
but using CF.standard multipliers in place of CF.premium. At zero congestion
(premium = standard = SCALE), all admitted transactions enter the premium tier.

### 12.6 BlockHeader Signalling

The final block of each fee window sets `fee_window_flags` in its header:

```
BlockHeader.fee_window_flags: u16  (new field, #[cfg(feature = "fee-window")])
                                    (serde default = 0 for backward compatibility)

Bit layout — two independent WindowSignalling bytes:
    Byte 0 (bits 0:7):   CIRCUIT_CF direction
        bit[0]    = FEE_WINDOW_ACTIVE
        bit[1:3]  = reserved
        bit[4:7]  = congestion_multiplier (cm)
    Byte 1 (bits 8:15):  WASM_CF direction (identical layout)
```

The 4-bit `congestion_multiplier` encodes the direction and magnitude of
the CF change from the current window to the next:

```
0b0000 = hold      (CF unchanged, within [low_water, high_water])
0b0001 = +10%      (CF increased by 10%)
0b0010 = -10%      (CF decreased by 10%)
0b0011..0b1111 = reserved for future granularity
```

Dual encode/decode:

```
encode_flags_dual(circuit_cf, wasm_cf, prev_circuit, prev_wasm) -> u16:
    circuit_byte = encode_flags(circuit_cf, prev_circuit)
    wasm_byte    = encode_flags(wasm_cf, prev_wasm)
    return (circuit_byte & 0xFF) | ((wasm_byte & 0xFF) << 8)

decode_flags_dual(flags: u16) -> (circuit_cm, wasm_cm):
    circuit_cm = (flags & 0xF0) >> 4
    wasm_cm    = (flags >> 12) & 0x0F
    return (circuit_cm.clamp(0, 2), wasm_cm.clamp(0, 2))
```

A wallet reading the flags can compute the next window's thresholds
without replaying the full adjustment logic for both CF dimensions.

### 12.7 Formal Invariants

**I1 — Window Determinism.** For any two nodes with identical chain state
at height H, `get_current_thresholds(H)` SHALL return identical values.
The adjustment is a pure function: `(CF_premium, CF_standard) = f(mempool_state_at_boundary)`.

**I2 — Backward Compatibility.** Blocks without `fee_window_flags`
(pre-activation, `fee_window_flags == 0`) SHALL be treated as having
zero congestion: WASM_CF = CIRCUIT_CF = SCALE (both premium and standard).
At zero congestion, `compute_fee()` at average circuit difficulty (~1000)
yields approximately 1_001_000 (0.01 DRKW). `#[serde(default)]` ensures
old blocks deserialize correctly.

**I3 — FCFS Preservation.** Transactions admitted under window N's
thresholds SHALL NOT be evicted when window N+1's thresholds activate.
Admission is durable. Within each tier, transactions SHALL be ordered
first-come-first-served (FIFO). Premium queue drains before general
queue. No transaction can jump the queue by paying a higher fee after
admission. No ex post facto eviction.

**I4 — Congestion Factor Ordering.** `CF_premium > CF_standard` at all
times. Premium-tier circuits (rate ≥ 5) always pay a strictly higher
congestion multiplier than standard circuits (rate 1–3). This prevents
premium transactions from being cheaper under any congestion regime.

**I5 — Opcode Difficulty Monotonicity.** A transaction with a higher
total opcode difficulty SHALL never pay a lower total fee than a
transaction with a lower total opcode difficulty, for identical WASM
size and congestion regime. The per-opcode difficulty table (§12.4.2)
is the sole determinant of circuit execution cost ordering.

**I6 — CF Convergence.** As mempool queue depth → 0, CF → 1 for both
tiers. As queue depth grows, CF grows logarithmically — doubling the
queue adds at most α to the factor. This prevents both premature
saturation (linear growth) and insufficient responsiveness (constant).

**I7 — Smooth Adjustment.** No single-window CF change SHALL exceed
±10% of the current value. This prevents fee shock and allows the
market to adapt gradually.

**I8 — Deterministic CF.** The window's congestion factor is computed
locally from the miner's mempool queue depth at the window boundary.
All nodes synced to the same chain tip observe the same mempool state
and therefore compute identical CF values — no coordination or gossip
is required. I1 (pure function) guarantees determinism.

### 12.8 Mempool Integration

The mempool applies fee window thresholds to incoming transactions at
admission time and preserves admitted transactions across window
boundaries.

#### 12.8.1 Admission Gate (per-transaction)

```
admit(tx, window):
    fee = extract_fee(tx)
    wasm_kB = extract_wasm_kB(tx)

    // Proof-based tiering: try premium first, fall back to general.
    // Tier is determined by which threshold proof verifies, not by a
    // static circuit classification.
    premium_threshold = compute_fee(window.premium_cf, wasm_kB)
    if fee >= premium_threshold AND verify_threshold_proof(tx, premium_threshold):
        admit to premium_queue (FIFO)
        return PREMIUM

    general_threshold = compute_fee(window.standard_cf, wasm_kB)
    if fee >= general_threshold AND verify_threshold_proof(tx, general_threshold):
        admit to general_queue (FIFO)
        return STANDARD

    reject — fee below applicable threshold
```

#### 12.8.2 Window Transition (at boundary block)

```
on_window_boundary(new_window):
    // Preserve existing queues — no eviction (I3)
    // New thresholds apply to NEW arrivals only
    // Premium queue drains FCFS, then general queue FCFS
    active_window = new_window
```

#### 12.8.3 Block Selection

```
select_for_block(limit):
    txs = []
    // 1. Drain premium_queue (FCFS) until gas/tx limit
    while premium_queue.not_empty() AND within_limit(txs):
        txs.append(premium_queue.pop_front())
    // 2. Drain general_queue (FCFS) until limit
    while general_queue.not_empty() AND within_limit(txs):
        txs.append(general_queue.pop_front())
    // 3. Fill remaining from fee_index (fee-descending)
    for tx in fee_index.descending():
        if within_limit(txs):
            txs.append(tx)
    return txs
```

#### 12.8.4 Window Transition Timing

```
FEE_WINDOW_TRANSITION_DELAY = 30 seconds  (after boundary block timestamp)

After the final block of window N at height H:
  T_0 = block_timestamp(H)                          // block timestamp
  T_activate = T_0 + FEE_WINDOW_TRANSITION_DELAY    // 30-second grace period

  During [T_0, T_activate):  GRACE PERIOD
    - Mempool continues admitting under window N thresholds
    - Wallets construct FeeThreshold_V1 proofs against window N+1 thresholds
    - Miners compute CF from local mempool state (deterministic, I1, I8)
    - New transactions may submit with window-N or window-N+1 proofs

  At T_activate:  THRESHOLD ACTIVATION
    - Mempool switches to window N+1 thresholds for NEW arrivals
    - Previously admitted transactions preserved (I3, FCFS)
    - New arrivals with window-N threshold proofs: REJECTED (stale-threshold-proof)
    - Window-N+1 proofs: accepted against new thresholds

  After T_activate:  WINDOW N+1 ACTIVE
    - Full enforcement of N+1 thresholds
    - Window-N threshold proofs permanently stale for new arrivals
```

The 30-second window aligns with the block time (120s), miner block assembly
time (< 5s), and sync poll interval (30s, observer.md). It gives wallets
adequate time to re-query headers and re-construct proofs after a CF change.
The FeeThreshold_V1 circuit (k=11, ~5 opcodes, circuit_difficulty=40) proves
in ~10-50ms — well within the 30-second budget, satisfying the requirement
that proof production time be an order of magnitude below the acceptance
window.

### 12.9 Wallet Integration

The wallet discovers current thresholds by reading the latest block header
before constructing FeeThreshold_V1 proofs.

```
construct_fee(circuit_costs, wasm_bytes, latest_block):
    flags = latest_block.header.fee_window_flags
    if flags & FEE_WINDOW_ACTIVE:
        (wasm_cf, circuit_cf) = decode_congestion_factors(flags, chain_state)
    else:
        (wasm_cf, circuit_cf) = (DEFAULT_WASM_CF, DEFAULT_CIRCUIT_CF)  // legacy

    wasm_kB = max(1, ceil(wasm_bytes.len() / 1024))

    // Identical formula to mempool compute_fee() (§12.4.1)
    fee = ((wasm_kB * BASELINE_STORAGE * wasm_cf.premium)
           + (sum(circuit_costs) * circuit_cf.premium)) / SCALE

    // Tier selection: proof determines tier, not static classification
    threshold = premium_threshold if fee >= premium_threshold else general_threshold

    proof = create_fee_threshold_proof(fee, threshold, ...)
    return (fee, proof)
```

If the window boundary passes before the transaction is mined, the wallet
SHALL re-query the latest header and may re-construct the proof with the
new threshold. The wallet SHALL NOT submit a transaction with a threshold
proof bound to a stale window.

### 12.10 Miner Integration

The miner computes CF deterministically from local mempool state at each
window boundary and encodes the result in the block header.

```
prepare_block(height, mempool, chain_state):
    header = build_header(height, ...)

    if is_window_boundary(height):
        // Deterministic from local mempool state (I1, I8)
        cf = chain_state.fee_window.compute_cf(
            mempool.premium_queue_len(),
            mempool.general_queue_len()
        )
        header.fee_window_flags = encode_flags(cf)
        mempool.update_thresholds(
            compute_fee(cf.premium),
            compute_fee(cf.standard)
        )

    return assemble_block(header, mempool.select_for_block())
```

The CF is computed locally and deterministically. All nodes with the same
mempool state arrive at the same CF — no P2P gossip or median consensus is
needed. The `fee_window_flags` in the block header provide the canonical
signal for all downstream consumers (wallets, sync clients).

### 12.11 Circuit k-Value Difficulty Scaling

#### 12.11.1 Rationale

The Halo2 PLONK proving system uses a parameter `k` that determines the domain
size: `2^k` rows in the constraint system polynomial. Proving and verification
cost (multi-scalar multiplication over `2^k` points) scales with `k`. Two
circuits with identical opcodes but different `k` values have substantially
different computational cost:

- A circuit with `k=11` (2,048 rows) is the smallest practical size.
- A circuit with `k=15` (32,768 rows) costs 16× as much to prove and verify.
- `MAX_K = 16` (65,536 rows) is the maximum allowed by the ZK circuit decoder.

The per-opcode difficulty table (§12.2) encodes **what** computation happens
(constraint type, column count, lookup table requirements). The `k` value
encodes **how much** computational capacity is allocated. Both are required
for a complete cost model.

#### 12.11.2 Formula

```
circuit_difficulty(opcodes, k) = base_cost(opcodes) × 2^(k - K_REF)
```

Where:
- `base_cost(opcodes)` = `Σ OPCODE_DIFFICULTY[op]` for each opcode in the circuit
- `K_REF = 11` — reference k value (FeeThreshold_V1's k, the smallest proven k)
- Scale factor capped at `2^(MAX_K - K_REF) = 32` (k=16 maximum)
- For `k < K_REF`: scale factor = 1 (no fractional scaling)

#### 12.11.3 Interaction with the Two-Component Formula

The circuit component of the fee formula uses k-scaled difficulty:

```
circuit_part = Σ circuit_difficulty(opcodes_i, k_i) × CIRCUIT_CF / SCALE
```

At zero congestion (CIRCUIT_CF = SCALE):
```
circuit_part = Σ base_cost(opcodes_i) × 2^(k_i - K_REF)
```

Each circuit in a transaction contributes its own k-scaled difficulty. A
transaction with two circuits (e.g., Fee_V2 at k=12 and FeeThreshold_V1 at
k=11) pays the sum of both.

#### 12.11.4 Constants

| Constant | Value | Source |
|----------|-------|--------|
| `K_REF` | 11 | FeeThreshold_V1 circuit k |
| `MAX_K` | 16 | `src/zkas/constants.rs` |
| `MAX_SCALE` | 32 | `2^(MAX_K - K_REF)` |

### 12.12 Architectural Principles

#### 12.12.1 Domain Separation: Rate Limiting vs Fee Model

Two independent mechanisms protect the network. They serve different purposes
and SHALL NOT be conflated:

| | Rate Limiting | Fee Model |
|---|---|---|
| **Purpose** | Computational circuit breaker | Economic mechanism |
| **Origin** | Inherited from upstream (wasmer metering middleware) | DarkWow-native threshold proof system |
| **Users pay?** | No — pure safety tripwire | Yes — Fee_V2 Pedersen mass balance |
| **Deterrent?** | No — attacker pays nothing | Yes — fee paid upfront |
| **Privacy** | N/A | Fee amount anonymized (Pedersen commitment) |

Rate limiting stops runaway execution but does not charge for wasted
computation. The fee model is the economic deterrent — attackers pay
proportionally to the resources they consume. Both are necessary; neither
is sufficient alone.

#### 12.12.2 O-Cap Foundation of Cost Predictability

DarkWow contracts follow the object capability model (see `ocap.md`,
`type-system.md`, `contract-wasm-type-system.md`). Contracts are composed
from proven primitives — Box, Purse, Promissory Note — rather than
arbitrary Turing-complete code.

This architectural choice makes deterministic cost prediction possible:

- **Cost profiles compose**: if box costs 1000 difficulty and purse costs
  1000, a transfer (box + purse) costs approximately 2000.
- **Attestation is tractable**: verifying "does this contract correctly
  compose known primitives?" is auditable. Verifying "does this arbitrary
  Solidity code do anything dangerous?" is not.
- **Trust is structural**: the user trusts the primitives and the
  composition rules, not the contract author.

The mempool and miner see a contract's cost profile as derivable from its
primitives, not as an opaque claim by an untrusted deployer.

#### 12.12.3 Risk Sharing: The Miner/User Compact

In Ethereum's gas model, the user bears all risk: if a transaction reverts
mid-execution, the gas is spent and the state change is discarded. The user
pays for failure.

In DarkWow's threshold proof model, risk is shared:

1. **User pays upfront** — the Fee_V2 proof commits to a fee amount. The
   FeeThreshold_V1 proof guarantees the fee meets the miner's threshold.
   Execution is guaranteed — no mid-execution revert from gas exhaustion.

2. **Miner accepts execution risk** — the coinbase reward compensates
   miners for accepting transactions with unknown computational cost.
   Miners are incentivized to maximize fee collection within their
   computational window. A miner who accepts too many expensive transactions
   and misses the block window loses both fees AND the coinbase reward.

3. **Miners police themselves** through resource awareness. They set
   thresholds via the fee window PID controller to balance fee revenue
   against computational cost. They don't offload risk onto users.

4. **Fee privacy protects users** — the fee amount is hidden behind a
   Pedersen commitment. Only the threshold is public. No traffic analysis
   of user fee/gas preferences is possible.

**Risk factor assignment** (see [manifest.md §Cost Profiles](../manifest.md)):

| Contract Status | Risk Factor |
|---|---|
| Genesis contract | 1.0× |
| Attested manifest + endowment | 1.0× |
| Attested manifest, no endowment | 1.25× |
| Self-declared manifest, no attestation | 1.5× |
| No manifest (unknown) | 2.0× |

The risk factor is a multiplier on the circuit component of the fee.
These tiers are the current specification — contracts are classified by
their manifest and attestation status at admission time. The automated
feedback loop (observation → reputation → dynamic adjustment) is
specified in §12.12.5 as future work; the static tier table above is
the operational baseline.

The endowment is the contract's on-chain stake — it can be slashed if
costs consistently exceed declared tolerance. This aligns incentives:
a contract author with 10,000 DRKW in an endowment has 10,000 reasons
to declare costs accurately. The economic gradient pushes toward attested
manifests with endowments. Contracts are infrastructure, not experiments.

#### 12.12.4 Contrast with Ethereum and Bitcoin

| | Ethereum | Bitcoin | DarkWow |
|---|---|---|---|
| **Execution model** | Turing-complete, arbitrary code | Single-purpose scripts | O-cap composition of proven primitives |
| **Cost prediction** | Gas guessing — user bears risk | N/A (simple scripts) | Deterministic from opcodes × k |
| **Fee privacy** | Public gas price + gas limit | Public fee | Anonymized (Pedersen commitment) |
| **Execution guarantee** | Can revert mid-call (out of gas) | No smart contracts | Threshold proof guarantees execution |
| **Risk allocation** | All on user (reverted = wasted gas) | All on user (no recourse) | Shared — miner accepts risk as part of coinbase reward |
| **Attestation model** | None (trust the code or don't) | None (trust no one) | Manifest declarations + third-party attestations |

#### 12.12.5 Feedback Loop (Future)

The per-opcode difficulty table and k-scaling formula provide the deterministic
baseline. Future layers build on this foundation:

1. **Manifest cost declaration**: contracts self-declare expected
   `circuit_difficulty`, `k_value`, `wasm_kb`, and tolerance range per
   state transition.
2. **Attestation**: third parties validate or challenge manifest accuracy.
3. **Observation**: the network compares observed computational cost
   (wasmer instruction count, ZK verification time) against declarations.
4. **Reputation**: persistent black marks for misdeclared contracts;
   fee multipliers escalate until declarations are corrected.
5. **Rate limit calibration**: computational rate limits tighten from
   arbitrary constants to 5-10× the expected value declared in manifests.

Layer 1 (this specification) provides the objective baseline. Without it,
the feedback loop has no reference point for "expected" cost.

#### 12.12.6 Risk-Sharing Model: A Genesis Case Study

The fee system is the first major case study from genesis demonstrating why
DarkWow's entire architecture exists as it does. In token-weighted governance
systems, whales are structurally incentivized to push more risk onto users —
they control governance, they set parameters, and they profit from user
extraction. DarkWow's genesis block contains specific o-cap primitives to
invert this dynamic. The fee model proves they work.

**The four-layer risk architecture:**

**Layer 1 — Users have bounded, private risk.** A user pays a threshold fee to
enter the mempool. The fee is Pedersen-committed: no traffic analysis of
fee/gas patterns is possible. If the state transition fails or consumes more
resources than the manifest declared, the user does NOT pay more. They cannot
fat-finger away their native token. In a plaintext gas model, that class of
risk — paying for failed or resource-exhausting execution — is broadly inherent.
DarkWow eliminates it.

**Layer 2 — Miners absorb execution risk in exchange for coinbase + fees.**
When a transaction exceeds its declared costs, the miner still executes it.
The miner earns the fee but may lose the coinbase opportunity if execution
overruns the block budget. The miner protects itself by:

- Reading the contract's manifest `[[cost_profiles]]` before block assembly
- Tracking observed-vs-declared cost accuracy across windows
- Applying higher risk factors to contracts that systematically under-declare
- Blacklisting contracts that cause block exhaustion (infinite loops or
  high-high trips that exhaust the entire block)
- Setting prohibitively expensive risk factors for blacklisted contracts

**Layer 3 — Deployers bear the burden of proof.** Deploying a new contract
means accepting responsibility for accurate cost declarations. The deployer
self-declares costs in `[[cost_profiles]]` — these are cryptographically bound
to the contract. A contract that lies about its costs gets priced out of the
mempool over time as miners collectively raise its risk factor. To lower the
risk factor from 2.0× (unknown) toward 1.0× (genesis), the deployer must:

- Have the contract attested via identity and attestation contracts
- Underwrite risk via endowment and escrow contracts (slashable stake)
- Maintain accurate cost declarations over time (reputation)

**Layer 4 — No governance token required.** The adjustment mechanism is
mechanical: miners observe, risk factors adjust, deployers respond. The o-cap
primitives in genesis are necessary and sufficient — no token-weighted
governance is needed to decide who bears risk. The architecture itself enforces
the risk distribution.

**Why each genesis contract exists for this model:**

| Genesis Contract | Role in Risk Architecture |
|---|---|
| `native_token` | Fee payment — Pedersen-committed (private), bounded to threshold |
| `manifest` | Self-declared cost profiles — deployer stakes reputation on accuracy |
| `identity` + `attestation` | Vouching — third parties verify contract safety, lower risk factor |
| `endowment` + `escrow` | Economic underwriting — slashable stake backs cost declarations |
| `deployooor` | Contract deployment — binds manifest to contract at birth |
| Fee window system (§12) | Miner feedback loop — observed vs declared costs → risk factor adjustment |

This is the defining differentiator: **decentralized self-governance through
o-cap primitives, not token voting.** The fee model is the case study that
proves the architecture works — infrastructure builders and deployers absorb
execution risk, users don't, and no whale vote can change that.
