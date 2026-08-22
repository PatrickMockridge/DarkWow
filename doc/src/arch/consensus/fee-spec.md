# Fee Payment and Collection — Formal Specification

*Specification for FeeV3 (public gas-based fee, three-tier pricing), FeeCollectV1,
and the coin Merkle tree. FeeV1 (clear-text flat fee) and the privacy-preserving
fee model (Pedersen fee + FeeThreshold_V1 + encrypted-fee-to-miner) are REMOVED. Theorems,
invariants, and formal predicates. Tests SHALL be derived from this document — not
from reverse-engineering production code.*

## Architecture Overview

The fee system is **public and gas-based**. A transaction declares its fee in the
clear; the fee is the product of the measured work ("gas") and a **three-tier price**
(low / medium / high). There is no ZK proof hiding the fee amount, no threshold
proof, and no encrypted-fee channel to the miner.

**Why the privacy model was removed.** The privacy-preserving fee model encrypted
the fee to the miner's per-block key. This is unworkable in practice: the wallet
builds the transaction
*before* the block is mined, so it cannot know which miner will produce the block
nor the miner's per-block public key. Every production call site passed
`miner_public_key = None`, shipping a 68-byte zero placeholder that the miner could
not decrypt — so the fee was silently burned. The `FeeThreshold_V1` proof also
proved only `fee >= fee` (the threshold was set equal to the computed fee), and the
`FeeParamsV2` Pedersen commitment was openable anyway because its blinding factor
was public. The privacy layer was therefore both unworkable and redundant.

**The replacement.** The fee amount is plaintext, deterministic, and verifiable:

```
fee = gas × price_tier        price_tier ∈ { low, medium, high }  (wow per gas)
```

- **gas** — units of work done in the block. Measured as the WASM-metered gas
  (`BLOCK_GAS_LIMIT` / per-call `GAS_LIMIT`) plus the circuit row count
  (`Σ rows(opcode)`, §12.4.2) and WASM deployment size (`wasm_kB`). Gas is
  fully metered: a transaction may consume all of its gas before the state
  transition completes, and the fee is charged on actual work.
- **price_tier** — one of three uniform price levels (wow per gas). The user picks
  a tier rather than an arbitrary fee amount. This removes **fat-finger risk** (an
  accidental absurd fee) and **deanonymisation via idiosyncratic fee behaviour**
  (users converge on three uniform prices instead of leaking a unique fee fingerprint).

**Two domains survive:**

**`[domain: mass_balance]` — verified during `accept_block` via WASM (consensus-critical):**
- **Fee_V2** — Pedersen mass balance: `input = output + fee`. Proves no secret
  inflation. ZCash Orchard exploit defense-in-depth. Retained (it binds the hidden
  input/output coin values to the now-public fee).

**`[domain: mass_balance]` — verified during `accept_block` via WASM (consensus-critical):**
- **FeeCollectV1** — Transfers the accumulated plaintext fee pot to the miner and
  resets it. Contract logic in `src/contract/native_token/`.

The `[domain: fee_signalling]` proof (FeeThreshold_V1) is deleted; admission is a
plain `fee >= tier_price` comparison on the declared plaintext fee.

### Data Flow

```
Wallet                    Mempool                  Miner                    Chain
  │                         │                       │                        │
  ├─ declare fee (plain) ──►│                       │                        │
  │  fee = gas × tier       ├─ high/medium/low/     │                        │
  │                         │  reject               │                        │
  │                         │                       │                        │
  │                         │     transactions ────►│                        │
  │                         │     + plain fees      ├─ Build block ──────────►│
  │                         │                       │  + PoWReward            │
  │                         │                       │  + FeeCollectV1         │
  │                         │                       │  + update BlockCharge   │
  │                         │                       │    (observed vs declared)│
  │                         │                       │                        ├─ Fee_V2
  │                         │                       │                        │  (no inflation)
  │                         │                       │                        ├─ FeeCollectV1
  │                         │                       │                        │  (claim + reset)
```

1. **Wallet** computes `fee = gas × tier` from the contract's declared cost profile
   and the user's chosen tier; writes the fee in the clear.
2. **Mempool** compares the declared fee against the three tier prices and assigns
   a priority (high/medium/low) or rejects — no ZK proof.
3. **Miner** collects pending transactions + their plaintext fees, builds a block
   with PoWReward + FeeCollectV1, and — after execution — compares *observed* gas to
   each contract's self-declared `BlockCharge`, updating the charge via the risk
   multiplier (§12.12.3).
4. **Chain** verifies Fee_V2 mass balance (no inflation) and FeeCollectV1
   (claim + reset) during `accept_block`.

**Risk is no longer a fee multiplier.** Risk moves to the *user*: the wallet computes
a basic trust metric for a contract — from contract age, whether the tx path has been
used before, attestation, and wallet-side checks of the WASM — to inform the user's
decision. The `ContractRiskTracker` and the manifest's self-declared `BlockCharge`
remain as miner-side governance/observability (updated by the risk multiplier when
the transaction runs), not as admission gates.

### §0.1 Process Engineering Analogy

In Bitcoin, the fee system is transparent: you can see every transaction amount,
every fee, and the coinbase output directly on the ledger. The relationship
between fee payment and block reward is self-evident.

In a privacy-preserving system with hidden fees (Pedersen commitments) and
zero-knowledge proofs, you cannot "see inside the pipe." You need instrumentation
and proofs — exactly as in chemical and process engineering, where you can't see
inside a distillation column, reactor, or pipeline and must rely on flow meters,
pressure gauges, and control valves.

The DarkWow fee architecture maps to these process engineering concepts. The
transaction pipeline is a **pressurized header** carrying a multi-phase fluid
(different contract types, different computational loads). You cannot see
individual flow rates — you can only read instruments.

```
                        ┌──────────────────────────────────────┐
     transactions ────▶ │  CONTROL VALVE (mempool admission)    │
                        │                                        │
                        │  high/medium/low = three-stage choke  │
                        │  plaintext fee = measured flow rate   │
                        │  fee_window_flags = valve position     │
                        │       indicator (public)               │
                        └──────────────┬─────────────────────────┘
                                       │
                                       │  admitted (with fee commitments)
                                       ▼
                        ┌──────────────────────────────────────┐
                        │  PID CONTROLLER (FeeWindowState)       │
                        │                                        │
                        │  Inputs: queue depths × sensitivity    │
                        │  Output: CF adjustment (±10% cap)      │
                        │  Cycle: every 20 blocks (window)       │
                        │  FI-WINDOW-1,2,3                       │
                        └──────────────┬─────────────────────────┘
                                       │
                                       │  signals to valve + instruments
                                       ▼
     ┌──────────────────────────────────────────────────────────────┐
     │                    THE PIPE (block execution)                  │
     │                                                               │
     │  ┌──────────┐   ┌──────────┐   ┌──────────┐                 │
     │  │Contract A│   │Contract B│   │Contract C│   ← each section │
     │  │ pipe roughness = risk factor │              has its own    │
     │  │ (evolves with wear/fouling)  │              friction factor│
     │  └──────────┘   └──────────┘   └──────────┘                 │
     │                                                               │
     │  ┌──────────────────────────────────────────────────┐        │
     │  │  FOULING DETECTOR (ContractRiskTracker)            │        │
     │  │                                                    │        │
     │  │  Measures: ΔP_observed vs ΔP_declared             │        │
     │  │  Escalates: pipe roughness for under-declaring    │        │
     │  │  De-escalates: for sustained accurate declaration │        │
     │  │  FI-RISK-1 through FI-RISK-6                      │        │
     │  └──────────────────────────────────────────────────┘        │
     └──────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                        ┌──────────────────────────────────────┐
                        │  FLOW TOTALIZER (FeeCollectV1)         │
                        │                                        │
                        │  fees_db[height] = plain running total │
                        │  Each FeeV3 = one pulse on the meter   │
                        │  Verifies Σfees = claimed total        │
                        │  Resets to zero each block             │
                        │  FI-COLLECT-1,2                        │
                        └──────────────────────────────────────┘
```

**The Four Instruments**

**1. Control Valve (Mempool Admission).** FI-ADMIT-1,2,3. The mempool's three-tier
admission gate is a flow control valve. The tier price is the choke position: higher
tier = more pressure drop (fee) required for a transaction to pass. High tier is a
wider choke opening; low tier is narrower. Below the low tier: valve closed (REJECT).
The fee is plaintext, so admission is a plain `fee >= tier_price` comparison — no
anti-tamper proof is needed. The `fee_window_flags` in the block header are the public
valve position indicator — every node can read them.

**2. PID Controller (FeeWindowState).** FI-WINDOW-1,2,3. Every 20 blocks (one
window), the controller reads queue depths from the mempool (process variable),
compares against capacity (setpoint), and adjusts the valve position. The
adjustment is capped at ±10% per window — process stability requires slow, damped
response. Two independent control loops run in parallel: one for circuit execution
congestion (ZK proof complexity) and one for WASM storage congestion (deploy size).
The controller is deterministic: all nodes compute identical outputs from identical
inputs. No floating-point arithmetic. No coordination required.

**3. Fouling Detector (ContractRiskTracker).** FI-RISK-1 through FI-RISK-6. In a
chemical plant, fouling builds up inside pipes over time, increasing the actual
pressure drop above the design specification. A pipe section that was designed for
ΔP=1000 Pa may actually require ΔP=2000 Pa due to fouling. The fouling detector
compares observed vs declared pressure drop and adjusts the **pipe roughness
coefficient** (risk factor) accordingly.

Contracts are pipe sections. Each contract declares its expected pressure drop in
its manifest `[[cost_profiles]]`: "this function should cost circuit_difficulty=X."
The miner and wallet observe the contract's attested condition — its declared block
charge, whether its stated functionality is attested, and its slashable endowment —
alongside the actual execution cost during WASM execution. If a contract
systematically under-declares (declared 1000, observed 2000) or ships an unattested,
questionable circuit, the fouling detector escalates its roughness coefficient —
`compute_total_fee()` now multiplies the declared circuit cost by the elevated risk
factor, and the contract's users pay higher fees. If the contract fixes its
declarations, obtains attestation, and sustains accuracy, the detector de-escalates
the coefficient back toward baseline.

Critically: **each pipe section has its own roughness coefficient, earned through
its own behavior.** There is no global lookup table mapping "pipe material grade"
to roughness. A new pipe starts at baseline roughness (1.0×). A pipe that runs for
a million cycles without fouling drops below pipes that foul immediately —
regardless of what grade was stamped on it at the factory. The fouling detector
observes behavior, not labels. FI-RISK-6: the manifest declares costs; the tracker
assigns risk. These are separate concerns.

The risk factor is a *view*, not a consensus rule. Its canonical specification —
including the attestation/endowment risk table and the invariant that only the
Pedersen mass balance is hardwired — is the [Risk & Governance
Specification](../risk-and-governance.md).

**4. Flow Totalizer (FeeCollectV1).** FI-COLLECT-1,2. The `fees_db[height]` pot is
a plain flow totalizer. Each FeeV3 transaction emits one pulse on the meter: its
plaintext `fee`. The pot sums pulses arithmetically: `Σf_i`. The fee is plaintext —
every observer can read it. At each block, FeeCollectV1 reads the totalizer, verifies
it matches the claimed total, transfers
the fee value to the miner, and resets the meter to zero (Identity) for the next
block. Supply neutrality: fees transfer value, they don't create or destroy it.

**Declared Charge — The Nameplate Rating**

FI-WASM-1,2 and `BlockCharge` implement a **declarative capacity model.** In
process engineering, every piece of equipment has a nameplate: a pump is rated
for 100 m³/hr, a heat exchanger for 500 kW, a compressor for 10 bar discharge.
The nameplate is a PROMISE made by the manufacturer: "this unit will not exceed
these operating limits." If you run the pump at 150 m³/hr, it cavitates and
fails. The nameplate is not a measurement of actual operation — it's a
declarative boundary that the operator must respect.

`BlockCharge` is the nameplate on a transaction: "this transaction declares it
consumes N units of block capacity." It is NOT gas (thermodynamic, measured,
WYSIWYG — actual work performed). It is a declarative promise made BEFORE
execution. The miner uses declared charges to pack blocks: the sum of nameplate
ratings must not exceed the block's capacity rating. If a transaction declares
400M units but only uses 200M, the unused capacity is wasted — exactly as
running a 100 m³/hr pump at 50 m³/hr wastes the pump's capacity.

This is the inverse of the gas model. Gas measures what WAS consumed; charge
declares what WILL be consumed. The difference is the contract's *declaration
accuracy* — and that's what the fouling detector measures. A contract that
declares 400M and consistently uses 200M is over-declaring (wasting block
capacity — its users overpay). A contract that declares 100M and uses 200M is
under-declaring (risk of execution failure — its risk factor escalates).

The WASM storage component (`wasm_kB`) works the same way: a DeployV1
transaction declares its WASM size on the nameplate. The miner checks the
nameplate against the mempool admission threshold. A 50 kB deploy has a much
higher nameplate rating than a 1 kB transfer — and pays proportionally.

**The Two Instrument Channels**

| Channel | What It Carries | Who Can Read It | Analogy |
|---------|----------------|-----------------|---------|
| Public: `fee_window_flags` | Congestion direction (hold/+10%/-10%) | Everyone | Valve position indicator on the control room panel |
| Public: `fee` in `FeeParamsV3` | Exact fee amount (plaintext) | Everyone | Visible flow reading on the instrument panel |

FI-PLAIN-1,2 govern the fee channel: the fee is mandatory plaintext and
deterministic (`gas × tier_price`).

**Invariants as Instrument Calibration**

Each instrument has a calibration certificate — the invariants in §14. They define
acceptable operating ranges:

| Instrument | Calibration Invariants | What Happens If Violated |
|-----------|----------------------|--------------------------|
| Control Valve | FI-ADMIT-1,2,3 | Transactions admitted below threshold, FCFS ordering violated, double-spends pass |
| PID Controller | FI-WINDOW-1,2,3 | Thresholds diverge across nodes, floating-point non-determinism, CF ordering violated |
| Fouling Detector | FI-RISK-1 through FI-RISK-6 | Contracts pay wrong fees, risk factors not observable, manifest mis-declares risk |
| Flow Totalizer | FI-COLLECT-1,2,3,4,5 | Hidden inflation (ZCash Orchard class), supply not conserved, state machine violations, overlay visibility gaps, encoding corruption |
| Valve Position Indicator | FI-FLAG-1,2,3 | Wallet derives wrong CFs, circular hash dependency, flags treated as consensus |
| Private Channel | FI-ENCRYPT-1,2,3 | Fees visible, key reuse across blocks, silent estimate substitution |
| System Parameters | FI-GEN-1,2 | Parameters not initialized at genesis, compile-time constants for economic values |
| WASM Detection | FI-WASM-1,2 | Deploy transactions underpriced, WASM component ignored in admission |
| Proof Timing | FI-TIME-1 | Proof generation exceeds block interval, transaction unpublishable |

**Separation of Concerns**

| Concern | Domain | Location | Instrument |
|----------|--------|----------|------------|
| Fee extraction | fee_signalling | `FeeSignallingExtractor` trait, `crates/dwow-mempool/src/lib.rs` | Plaintext fee reading |
| Mempool admission gating | fee_signalling | `crates/dwow-mempool/src/lib.rs` | Valve actuation |
| Fee window PID controller | fee_signalling | `src/linear/src/fee_window.rs` | PID controller |
| Per-contract risk tracking | fee_signalling | `ContractRiskTracker` (chain_state sled tree) | Fouling detector |
| Pedersen mass balance | mass_balance | `src/linear/src/validation.rs`, `native_token/` | Flow totalizer |
| Fee commitment accumulation | mass_balance | `src/linear/src/chain_state.rs` | Totalizer register |
| Encrypted fee channel | fee_signalling | `fee_builder.rs` + `prepare_block()` | Sealed flow reading |

This separation is why the HAZOP naming convention renamed all types to make domain
membership obvious: `mass_balance` operations are consensus-critical (meter fraud ==
hidden inflation); `fee_signalling` operations are non-consensus coordination (valve
misconfiguration degrades UX but cannot create money). The fouling detector occupies
an intermediate position: its OUTPUTS (risk factors) participate in fee computation
and therefore in the mass balance; its INPUTS (cost observations) are informational.

See: `consensus.md` §Supply Audit for the complete mass balance metering specification.
See: `consensus-coinbase.md` §2-3 for the meter endpoint events.
See: §14 for the complete invariant catalogue referenced throughout this analogy.

## 1. Coin Merkle Tree

### 1.1 Type Definition

The coin Merkle tree is an incremental Merkle tree of commitments to
native-token coins. It is shared by all native_token functions:
PoWRewardV1 appends coins to it; FeeV3, TransferV1, SpendV1, and BurnV1
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
transactions[1..k]  = user transactions  (FeeV3, TransferV1, SpendV1, BurnV1, deploys)
transactions[k+1]   = FeeCollectV1       (fn_code 0x06) — iff total_fees > 0
```

Phase 0 structural validation enforces: exactly one coinbase at index 0,
FeeCollectV1 (if present) at the final index. The coinbase transaction
SHALL contain exactly one contract call (compound coinbase prevention).

### 2.2 Sequential Execution Model

Within `execute_block`, each canonical transaction runs
`metadata()` → `exec()` → `apply()` sequentially in a shared overlay.

**Invariant 1 (Overlay Visibility)**: Call `i` observes the state writes of
calls `0..i-1` within the same block. Specifically, FeeV3's `exec()` (call
i) sees the coinbase's `apply_pow_reward()` writes (call 0), including the
merkle root inserted into `coin_roots_db`.

This is the mechanism that enables same-block fee payment: the coinbase
coin's merkle root IS visible to FeeV3 in the same block. This is NOT the
production path (where FeeV3 spends coins from prior blocks), but is a
valid test path when `tx.nullifiers` is empty (bypassing COINBASE_MATURITY).

### 2.3 Coin Tree Growth Per Block

For block at height H:

```
Starting tree: N leaves (from blocks 1..H-1)

1. PoWRewardV1 apply_pow_reward:
   append(coinbase_coin_H) → position N, root = R_H_0
   coin_roots_db[R_H_0] = ...

2. Each user FeeV3 apply_fee:
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

The contract-level nullifier check (FeeV3 check #7) still applies: the
nullifier must not exist in the contract's `nullifiers_db` sled tree, checked
via `db_contains_key` (per
[contract-wasm-standards-best-practices.md §9](../contract-wasm-standards-best-practices.md)).

## 3. FeeV1 — Fee Payment Entrypoint (REMOVED)

**Function code**: `0x00`. **Status**: REMOVED. `0x00` returns `InvalidFunction`
at the contract dispatch layer. All fee payment SHALL use FeeV3 (§5).

FeeV1 is documented here for historical reference only. It exposed the fee
amount in clear text (`[0x00][fee: u64 LE 8 bytes][FeeParamsV1 encoded]`).
FeeV3 (§5) replaces it with a plaintext deterministic fee.

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
| P6 | `db_contains_key(nullifiers_db, input.nullifier) == false` | InsufficientBalance | Custom(0) |
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

### 3.4 Fee_V2 ZK Circuit (Historical — documented here for reference)

NOTE: This section describes the Fee_V2 circuit (used by FeeV3, §5), not the
removed FeeV1 circuit. It is placed under the historical FeeV1 section (§3) for
contextual reference. The active specification is at §5 (FeeV3).

The Fee_V2 circuit constrains:

| Witness | Constraint |
|---------|-----------|
| input_value, output_value, fee | 64-bit range check, `input_value = output_value + fee` |
| input_coin, output_coin | Pedersen commitment to (value, value_blind) |
| nullifier | `poseidon(DOMAIN_NULLIFIER, secret, input_coin)` |
| merkle_root | Computed from `(input_coin.inner(), leaf_position, merkle_path)` |
| token_commit | `poseidon(DOMAIN_TOKEN_COMMIT, asset_id=0, token_blind)` |
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

For FeeV3 transactions, fees are plaintext. The contract accumulates each
FeeV3 call's `fee` into `fees_db[height]` (a plain u64 sum). FeeCollectV1
claims `fees_db[height]` — the miner's claimed total SHALL equal the plain
sum.

### 4.2 Formal Preconditions

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| C1 | `fc.total_fees > 0` | `↓zero-claim` | Custom(0) |
| C2 | `fc.total_fees == fees_db[height]` — the claimed total equals the plain accumulated sum | `↓bad-claim` | Custom(22) |
| C3 | `!db_contains_key(coins_db, fc.output.coin)` | InsufficientBalance | Custom(0) |
| C4 | `db_contains_key(nullifiers_db, fc.output.nullifier) == false` | InsufficientBalance | Custom(0) |
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

## 5. FeeV3 — Public Gas-Based Fee Payment `[domain: mass_balance]`

FeeV3 is single-domain. Its Fee_V2 circuit performs Pedersen mass balance
(`↓pay-fee` — consensus-critical, verified during `accept_block`). The fee
amount is **plaintext**: there is no FeeThreshold_V1 proof, no Pedersen commitment
to the fee, and no encrypted-fee channel to the miner.

**Function code**: `0x08`. **ZK circuit**: `Fee_V2` (value conservation, 15 public
inputs). The fee amount is a public field, not a ZK witness.

FeeV3 is the public fee model. It SHALL expose the fee amount in the
clear. The fee is deterministic — `fee = gas × price_tier` — so the wallet and
miner independently derive the same value; there is nothing to hide. The privacy
layer was removed because it was unworkable (the wallet cannot know the miner's
per-block key ahead of time) and redundant (the Pedersen blinding factor was public).

### 5.1 Purpose

Identical to FeeV1 (§3.1): spends an existing coin C, splits it into an
output coin O (change) and a fee F accumulated into `fees_db[height]`.
The fee amount is plaintext and deterministic.

### 5.2 Call Data Format

FeeV3 call data SHALL use the nominal `MassBalanceFeeV3CallData` type. Its
`encode()` method produces
`[0x08][FeeParamsV3::encode()]`. Consumers SHALL re-lift via
`MassBalanceFeeV3CallData::from_bytes(&data)`. No code path SHALL inspect
`data[0]` to determine the fee function; that determination SHALL come from the
`↓gate` barb on the `MassBalanceFeeV3CallData` name.

`FeeParamsV3` carries the plaintext fee and the user's chosen tier:

| Field | Type | Purpose |
|---|---|---|
| `fee` | `FeeAmount` | plaintext fee = gas × price_tier |
| `tier` | `FeeTier` (u8) | priority multiplier: `1 = low`, `2 = medium`, `4 = high` (§12.5) |
| `input` / `output` | `Input` / `Output` | the spent coin and change coin |

There is no `threshold_proof`, and no `encrypted_fee_value`. The `fee_value_commit`
field is retained in the Rust `FeeParamsV3` (a deliberate deviation from the
earlier "no fee_value_commit" wording) so the host verifier can recover the
Fee_V2 mass-balance proof's Pedersen coordinates.

### 5.3 Formal Preconditions

Let `params = FeeParamsV3 { input, output, fee, tier, tx_nonce }`.

| # | Predicate | Failure | Error Code |
|---|-----------|---------|------------|
| P1 | `params = FeeParamsV3::decode(&call_data[1..])` succeeds | ParseError | Custom(2) |
| P2 | `input.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P3 | `output.token_commit = poseidon(DOMAIN_TOKEN_COMMIT, 0, 0)` | InsufficientBalance | Custom(0) |
| P4 | `params.tier ∈ {low, medium, high}` | ParseError | Custom(2) |
| P5 | `params.fee = gas × tier_price` (deterministic, re-derived from the manifest cost profile) | FeeMismatch | Custom(0) |
| P6 | `db_contains_key(coin_roots_db, input.merkle_root)` | TransferMerkleRootNotFound | Custom(13) |
| P7 | `db_contains_key(nullifiers_db, input.nullifier) == false` | InsufficientBalance | Custom(0) |
| P8 | `!db_contains_key(coins_db, output.coin)` | InsufficientBalance | Custom(0) |

### 5.4 Postconditions

After successful exec+apply:

| # | Effect |
|---|--------|
| Q1 | `nullifiers_db[input.nullifier] = [1]` (input coin marked spent) |
| Q2 | `coins_db[output.coin] = []` (output coin registered) |
| Q3 | `coin_tree` appended with `output.coin`, new root inserted into `coin_roots_db` |
| Q4 | `fees_db[height] += fee` (plain u64 accumulation — no Pedersen accumulator) |

The fee amount `fee` is a public field, additionally constrained by value
conservation (`input = output + fee`) inside the Fee_V2 proof. The contract reads
the plaintext fee directly and accumulates it into `fees_db[height]` (the plain
path already present as the legacy fallback).

### 5.7 Test Derivation

In addition to the seven questions from FeeV1 (§3.5, historical), the
developer SHALL answer:

**Q8: What tier was selected?**
One of low / medium / high. The fee is `gas × tier_price` (§12.5). The declared
`tier` in `FeeParamsV3` SHALL match the re-derived fee.

**Q9: What is the fee?**
`fee = gas × tier_price`, a plaintext `FeeAmount`. There is no commitment and no
blind. The fee is a public field in `FeeParamsV3`.

**Q10: What is the fee sum?**
After all FeeV3 calls in the block, the contract's `fees_db[height]` SHALL equal
`Σfee_i` (a plain u64 sum). The miner claims this total in FeeCollectV1. See §14.6.

**Q11: How does FeeCollectV1 verify the total?**
The contract checks `total_fees == fees_db[height]` — a plain u64 comparison.
There is no Pedersen binding to verify.

### 5.8 MassBalanceFeeV3CallData — Nominal Call Data Type `[domain: mass_balance]`

FeeV3 call data SHALL be represented by the nominal `MassBalanceFeeV3CallData` type,
declared in [type-system.md §8.2.3](../type-system.md). This type eliminates
raw-byte dispatch (`data[0] == 0x08`) from the fee system. It is single-domain:
the `↓pay-fee` barb carries mass_balance authority (verified during `accept_block`).
There is no fee_signalling barb — the fee is plaintext.

**Rho-calculus type signature:**
```
MassBalanceFeeV3CallData ≡ νselector, params. (
    selector!(0x08)          — MassBalanceFeeV3Selector, zero-sized witness
    | params!(FeeParamsV3)    — deserialized, validated FeeParamsV3
    | ↓gate                   — constrains function to FeeV3 (exhibited by selector)
    | ↓pay-fee       [domain: mass_balance]     — Pedersen value conservation + nullifier
)
```

**Constructor (wallet side):**
```
MassBalanceFeeV3CallData::new(params: FeeParamsV3) → MassBalanceFeeV3CallData
```
The selector `0x08` is implicit — it is a property of the TYPE. The wallet SHALL
NOT manually prepend a selector byte. The `MassBalanceFeeV3CallData` carries the `↓gate`
and `↓pay-fee` [mass_balance] barbs into the mempool.

**Absorber boundary (mempool/miner/chain side):**
```
MassBalanceFeeV3CallData::from_bytes(data: &[u8]) → Option<MassBalanceFeeV3CallData>
```
This is the SINGLE site where raw bytes are re-lifted to the nominal type. It
validates:
1. `data[0] == 0x08` (selector byte matches)
2. `FeeParamsV3::decode(&data[1..])` succeeds (params are well-formed)

Returns `None` if either check fails. The `Option` return forces every consumer
to handle both `Some(mb_fee_v3)` (valid FeeV3, barb-carrying) and `None`
(not a FeeV3 call). The compiler SHALL enforce this exhaustiveness. Per
type-system.md §10.5, this is the re-lift validation obligation at the absorber
boundary.

**Encoder (persistence/wire boundaries only):**
```
MassBalanceFeeV3CallData::encode() → Vec<u8>
```
Produces `[0x08][FeeParamsV3::encode()]`. Only used at serialization boundaries
per type-system.md §2.2. The byte sequence is identical to the pre-nominal
encoding — this change is at the type level, not the wire level.

**Barbs exhibited:**
| Barb | Domain | Exhibited by | Meaning |
|------|--------|-------------|---------|
| `↓gate` | dispatch | `MassBalanceFeeV3Selector` | Constrains function dispatch to FeeV3 specifically — the selector is `0x08` by construction |
| `↓pay-fee` | mass_balance | `MassBalanceFeeV3CallData` | The call data carries a Fee_V2 proof (Pedersen mass balance), a plaintext fee, and a nullifier |

**Contrast with raw-byte dispatch.** Before this type existed, the mempool,
miner, validation, and chain state all inspected `data[0] == 0x08` to route
transactions. Per the rho-calculus, `quote(data[0])?(b).([b = 0x08]...)` —
a raw byte with no behavioral constraints gates the entire FeeV3 path. An
adversary can send `[0x08][arbitrary_garbage]` and the `[b = 0x08]` guard
fires `true`, routing garbage into the FeeV3 path where `FeeParamsV3::decode`
eventually fails. The nominal type closes this gap: garbage never constructs
a `MassBalanceFeeV3CallData`, so it never crosses the admission gate.

**Bisimulation.** For honest senders (who always construct valid `MassBalanceFeeV3CallData`),
the byte-level and type-level processes are strongly bisimilar (P ∼ Q). For
adversarial senders, they diverge: the raw-byte process enters `FeeV3Path!`
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

## 7. Three-Tier Mempool

The three-tier mempool admission system, tier-price announcement protocol, and
gas-based fee structure are defined in [mempool.md §5-8](../mempool.md). This
section (§7) provides the consensus-level interface; mempool.md owns the
policy-level specification.

### 7.1 Consensus Interface

Tier prices are derived from `compute_fee()` (the gas measure) at each fee window
boundary (see §12), scaled by the three tier multipliers. The genesis block
defines initial values; thereafter the PID-controlled CongestionFactor governs
adjustments. Miners signal updated prices via the `fee_window_flags` field of each
block header.

```
PRICE_LOW: u64     — price per gas for the low-priority tier
PRICE_MEDIUM: u64  — price per gas for the medium-priority tier
PRICE_HIGH: u64    — price per gas for the high-priority tier
```

### 7.2 FeeSignallingExtractor Trait `[domain: fee_signalling]`

The `FeeSignallingExtractor` trait (defined in `crates/dwow-mempool/src/lib.rs`)
SHALL provide a single method for fee extraction. It serves the fee_signalling
domain exclusively — it extracts the plaintext fee at mempool admission, never
during `accept_block`.

```
trait FeeSignallingExtractor {
    fn extract_fee(&self, tx: &Transaction) -> FeeAmount;
}
```

`extract_fee` reads the plaintext fee from `FeeParamsV3` (the `0x08` call).
Admission is a plain comparison `fee >= tier_price`, assigning the tx to the
highest tier whose price its fee meets (or rejecting it below the low tier).
There is no ZK threshold proof and no Pedersen commitment.

**Miner re-derivation.** Miners SHALL independently re-derive the expected fee
from the manifest cost profile (`gas × tier_price`) before including a transaction
in a block — a plain arithmetic check, not a cryptographic proof.

### 7.3 Further Specification

See [mempool.md §5](../mempool.md) for the three-tier admission algorithm,
[mempool.md §6](../mempool.md) for tier-price announcement via P2P gossip, and
[mempool.md §7](../mempool.md) for the gas-based fee structure (measured gas ×
tier price).

## 8. Wallet Integration

FeeV3 transaction construction, tier selection, and fee estimation are specified
in [wallet.md §6.4.2](../wallet.md) (Fee_V2 fee payment). There is no threshold
proof — the fee is plaintext.

### 8.1 Transaction Construction

The wallet SHALL produce a Fee_V2 proof (value conservation) with every FeeV3
transaction. The fee amount is plaintext. Call data format: `[0x08][FeeParamsV3]`
with a plaintext `fee: FeeAmount` and `tier: FeeTier`.

**Tier selection**: the user picks one of three tiers (low / medium / high). The
fee is `gas × tier_price` (§12.4). The wallet SHALL re-derive the fee from the
manifest cost profile and the chosen tier.

### 8.2 Tier Discovery

Tier discovery is specified in [wallet.md §6.4.2](../wallet.md) and
[mempool.md §6](../mempool.md). The wallet SHALL query connected mining nodes for
the current tier prices (and congestion flags) before constructing FeeV3
transactions.

### 8.3 Trust Metric

The wallet SHALL compute a basic trust metric for the target contract — from
contract age, whether the transaction path has been used before, attestation, and
wallet-side checks of the WASM — to inform the user's decision. This metric SHALL
NOT gate consensus or mempool admission (§14.7).

## 9. Barbs

Per [type-system.md §1.1](type-system.md), every type SHALL define the barbs
its processes may exhibit. Fee operations exhibit these barbs:

| Barb | Domain | Observable Action | Exhibited By |
|------|--------|-------------------|--------------|
| `↓pay-fee` | mass_balance | Exercises FeeV3 — spends a coin via nullifier, splits value into change + fee. Plain fee added to `fees_db[height]` | FeeV3, MassBalanceFeeV3CallData |
| `↓collect-fees` | mass_balance | Exercises FeeCollectV1 — claims `fees_db[height]`, mints fee coin to miner, resets it | FeeCollectV1, MassBalanceFeeCollectV1CallData |
| `↓fee-window-open` | fee_signalling | Window boundary detected — miner emits price signal | FeeWindow |
| `↓fee-window-advertise` | fee_signalling | Mempool advertises current tier prices via P2P | FeeWindow |
| `↓fee-window-enforce` | fee_signalling | Mempool enforces current window's tier prices at admission | FeeWindow |
| `↓fee-window-discover` | fee_signalling | Wallet queries mining nodes for tier prices | FeeWindow |
| `↓bad-fee-amount` | mass_balance | input.value <= fee — rejected at `FeeV3CallBuilder.build()` | FeeV3 |
| `↓bad-fee-tier` | fee_signalling | fee below the declared tier's price — rejected from mempool | FeeV3 |
| `↓bad-merkle-root` | mass_balance | Merkle root not found in coin_roots_db — rejected at `fee_v3` exec | FeeV3 |
| `↓double-spend` | mass_balance | Nullifier already in nullifiers_db — rejected at `fee_v3` exec | FeeV3 |
| `↓zero-claim` | mass_balance | FeeCollectV1 `total_fees == 0` — rejected as replay attack | FeeCollectV1, MassBalanceFeeCollectV1CallData |
| `↓bad-claim` | mass_balance | FeeCollectV1 `total_fees != fees_db[height]` — claimed amount mismatch against the plain sum | FeeCollectV1, MassBalanceFeeCollectV1CallData |
> **REMOVED.** The accumulator barbs (`↓acc-*`) are deleted in the public gas/fee
> model — the fee is a plain u64 sum in `fees_db[height]`, with no Pedersen
> accumulator. The `↓pay-fee` barb now writes the plaintext fee directly.

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
| FeeV3 | mass_balance | `0x08` | Function selector (public gas-based fee, plaintext) |
| FeeCollectV1 | mass_balance | `0x06` | Function selector (fee collection + reset) |
| PoWRewardV1 | mass_balance | `0x05` | Function selector (coinbase nullifier claim) |
| Fee_V2 | mass_balance | k=11, pallas, 24 witnesses, 15 public inputs | Fee value conservation circuit |
| `FeeV3TxBinding` | mass_balance | `poseidon(3, tx_commitment, tx_nonce)` | Fee_V2 proof anti-replay binding |
| `PRICE_LOW` | fee_signalling | `1_000_000` | Price per gas, low tier (wow/gas) |
| `PRICE_MEDIUM` | fee_signalling | `2_000_000` | Price per gas, medium tier (wow/gas) |
| `PRICE_HIGH` | fee_signalling | `4_000_000` | Price per gas, high tier (wow/gas) |
| `DRKW_ASSET_ID` | mass_balance | `0` | Native token identifier |
| `SCALE` | fee_signalling | `1_000_000` | CongestionFactor fixed-point scale (CF at zero congestion) |
| `ALPHA_PREMIUM` | fee_signalling | `0.05` | Log₂ coefficient for premium CF |
| `ALPHA_STANDARD` | fee_signalling | `0.01` | Log₂ coefficient for standard CF |
| `MAX_ADJUSTMENT` | fee_signalling | `0.10` | Maximum ±10% CF change per window (I7) |
| `FEE_WINDOW_SIZE` | fee_signalling | `20` | Blocks per fee window |
| `FEE_WINDOW_TRANSITION_DELAY` | fee_signalling | `30` | Seconds after boundary block before new thresholds activate (§12.8.4) |
| `DEFAULT_PREMIUM` | fee_signalling | `2_000_000` | REMOVED — replaced by `PRICE_MEDIUM` |
| `DEFAULT_GENERAL` | fee_signalling | `1_000_000` | REMOVED — replaced by `PRICE_LOW` |
| `K_REF` | fee_signalling | `11` | Reference k for circuit difficulty scaling (§12.11.4) |
| `MAX_K` | fee_signalling | `16` | Maximum allowed k value (`src/zkas/constants.rs`) |
| `MAX_SCALE` | fee_signalling | `32` | `2^(MAX_K − K_REF)` — maximum circuit difficulty multiplier |

## 11. Error Taxonomy

Every WASM error maps to a ContractError variant and a consensus barb.
Tests SHALL assert the specific barb, not a generic wrapper.

| Error | Barb | ContractError | Root Cause |
|-------|------|--------------|------------|
| Fee below tier price | ↓bad-threshold-proof | Custom(0) | `fee < PRICE_LOW` (three-tier admission) |
| Input value <= fee | ↓bad-fee-amount | Custom(0) | FeeV3CallBuilder pre-check |
| Merkle root not found | ↓bad-merkle-root | Custom(13) | Root not in coin_roots_db |
| Nullifier already spent | ↓double-spend | Custom(19) | Nullifier in nullifiers_db |
| Duplicate coin | Custom(14) | Coin already exists | Custom(14) |
| Token mismatch | ↓bad-token | Custom(0) | Wrong asset_id or token_commit |
| Fee sum mismatch | ↓bad-claim | Custom(22) | total_fees ≠ fees_db[height] |
| Zero-fee claim | ↓zero-claim | Custom(0) | FeeCollectV1 total_fees == 0 |
| Invalid signature | ↓bad-proof | Custom(1) | Bad signature public key |
| Invalid Merkle proof | ↓bad-proof | Custom(4) | Bad ZK proof merkle path |
| Value mismatch | ↓bad-proof | Custom(21) | Value commitment doesn't match |
| Parse error | ↓bad-params | Custom(2) | FeeParamsV3 decode failure |
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
FeeWindow(w, CF, N) =
    nu low, medium, high. (
        WindowTick!(low, medium, high) |
        !(WindowTick?(l, m, h). (
            Mempool!(l, m, h) |
            Wallet!(l, m, h) |
            Miner!(window_end(l, m, h)) |
            FeeWindow(w+1, CF', N)
        ))
    )

where:
    w              = current window index, starting at 0
    N              = window size in blocks (N = 20)
    CF             = congestion factor for the base gas price (rate ≥ 1)
    CF'            = recomputed from mempool queue depth at window boundary
    WindowTick     = signal emitted when height ≡ 0 (mod N), height > 0
    Mempool!(l,m,h)= mempool receives (PRICE_LOW, PRICE_MEDIUM, PRICE_HIGH)
    Wallet!(l,m,h) = wallet discovers (PRICE_LOW, PRICE_MEDIUM, PRICE_HIGH)
    Miner!(...)    = miner encodes price signal in block header
```

The process restarts with a recomputed congestion factor at each window boundary.
Between boundaries, the tier prices are stable — the mempool enforces the current
window's values, the wallet derives fees against them, and all participants
observe a consistent fee regime.

#### 12.1.1 Mempool Admission as an Object Capability

The mempool admission gate — `↓fee-window-enforce` — is an object-capability
boundary per [ocap.md](../../ocap.md). A transaction's **plaintext fee** is the
admission credential: the fee must meet the declared tier's price to be admitted
at that tier.

Admission is a plain arithmetic check (`fee >= tier_price`), not a cryptographic
proof. Every miner independently re-derives the expected fee from the manifest
cost profile and re-checks the comparison — a fee that passes at one miner passes
at all miners with identical chain state (I8, Deterministic CF). A transaction
whose fee is below the low tier simply does not cross the admission gate. There is
no central gatekeeper and no token-weighted vote.

The capability is mechanical: pay the tier price, cross the gate. The tier price
is economically derived from the congestion control loop (§12.7). The wallet's
trust metric (§14.7) informs the user's choice of tier but does not gate admission.

### 12.2 Barb Semantics

Four barbs partition the fee window's trajectory space:

| Barb | Action | Precondition | Postcondition |
|------|--------|-------------|---------------|
| `↓fee-window-open` | Window boundary at `height ≡ 0 (mod N)` | `height > 0` | `CF` recomputed |
| `↓fee-window-advertise` | Miner sets `fee_window_flags` in BlockHeader | Block is final in window w | Next window's tier prices encoded in header |
| `↓fee-window-enforce` | Mempool applies tier prices to new arrivals | Window w is active | Tx admitted/rejected per tier, FCFS within tier |
| `↓fee-window-discover` | Wallet reads `fee_window_flags` from latest header | Price bytes present | Wallet derives fee against the active tier price |

These barbs are additive to the existing fee barbs (§9). The `↓fee-window-open`
barb fires exactly once per window boundary and is the trigger for all
subsequent window-transition actions.

### 12.3 Nominal Types

Nominal types govern fee window state, following type-system.md §8.5:

```
FeeWindowId(u64)         — window index, computed as floor((height - 1) / N)
WindowSignalling(u8)     — bitfield encoding fee window state in block header
CongestionFactor(u32)    — fixed-point congestion factor, 1.0 = SCALE = 1_000_000
```

Additional domain types for fee arithmetic per type-system.md §2.3.1:

```
CfValue(u32)             — congestion factor fixed-point value (1.0 = 1_000_000)
WasmKb(u64)              — WASM deploy size in kilobytes
FeeTier(u8)              — the user's chosen tier: 1 = low, 2 = medium, 4 = high (multiplier)
FeeAmount(u64)           — the plaintext fee, distinct from the tier price
```

Tier classification is price-based: a transaction is high-tier if its plaintext
fee meets `PRICE_HIGH`, medium if it meets `PRICE_MEDIUM`, low if it meets
`PRICE_LOW`. The fee is plaintext, so no static circuit classification or proof
is needed — the declared fee and tier determine admission.

All follow the `#[repr(transparent)]` pattern. `FeeWindowId` implements `succ()`,
`pred()`, `from_height(height, N)`. `CongestionFactor` implements fixed-point
arithmetic with `SCALE = 1_000_000`, providing `apply(FeeAmount) -> FeeAmount`
to compute the congestion-adjusted base gas. External code SHALL use the accessor
methods rather than extracting the raw `u32`.

### 12.4 Fee Computation

#### 12.4.1 Formula

```
fee = gas × base_price × CF × tier × risk

where:
    gas        = Σ rows(opcode)                       (§12.4.2 — circuit ZK row count)
    base_price = flat wow-per-gas constant            (placeholder, tuned to real gas economics)
    CF         = congestion factor                    (§12.4.4 — fee-window CF)
    tier       = { low:1, medium:2, high:4 }          (uniform priority multipliers)
    risk       = ContractRiskTracker factor           (1.0× → 2.0×, §12.12.6)
```

The fee is a single multiplicative product of the circuit's measured work
(gas = ZK row count) and four scale factors: a flat asking price, the
congestion multiplier, the user's chosen priority tier, and the contract's
dynamic risk multiplier. The `wasm_kB` deployment storage cost (§12.4.3) is a
separate, additive one-time charge applied to `DeployV1` transactions only.

#### 12.4.2 Per-Opcode Row-Count (Gas) Table

Gas is the number of Halo2 **advice rows** an opcode's gadget consumes in the
constraint system. This is the rigorous basis: a circuit's total rows determine
its `k` (domain size `2^k`), and the verifier's dominant cost is the
multi-scalar multiplication over `2^k` points. One gas unit = one advice row.

Each opcode's row count is a deterministic function of its opcode and operand
annotations (bit width, array length), derived from the gadget source
(`src/zk/vm.rs`, `src/zk/gadget/*.rs`, vendored `halo2_gadgets`).

| Opcode | Rows | Derivation (source) |
|--------|------|---------------------|
| BaseAdd, BaseSub, BaseMul | 1 | `arithmetic.rs` — 1 gate, 3 advice cols |
| WitnessBase | 1 | `vm.rs` — `constrain_constant` |
| ConstrainEqualBase, ConstrainInstance | 1 | 1 copy constraint |
| ConstrainEqualPoint | 2 | x + y copy constraints |
| IsEqualBase, IsNotEqualBase | 1 | `is_equal.rs` — 1 gate, 4 advice cols |
| BoolCheck | 1 | `small_range_check.rs` — range-2 gate |
| NotBase | 2 | bool check + arithmetic sub |
| CondSelect, ZeroCondSelect | 1 | 1 gate, 4 advice cols |
| BaseDiv | 331 | 254 squarings + 76 conditional multiplies + 1 final (p−2: 255 bits, 77 set) |
| RangeCheck(bits) | `⌈bits/10⌉ + (bits%10 ? 2 : 0)` | running-sum window W=10 (`sinsemilla::K`) |
| LessThanStrict, LessThanLoose, LessThanOrEqual, BaseLtStrict | 57 | 1 compare gate + 2 × RangeCheck(253) |
| PoseidonHash(N) | `⌈N/2⌉ × 36` | P128Pow5T3: R_F=8 + R_P/2=28, RATE=2 |
| EcAdd | 6 | incomplete addition (10 advice cols) |
| EcMul, EcMulVarBase | 510 | 255-bit double-and-add (2 rows/bit) |
| EcMulBase, EcMulShort | 85 | fixed-base windowed |
| EcGetX, EcGetY | 0 | coordinate extraction, no new gate |
| MerkleRoot | 1632 | 32 levels × 51 Sinsemilla rows (2×255 bits / K=10) |
| SparseMerkleRoot, SetMembership | 9180 | 255 levels × 36 Poseidon rows |
| Noop, DebugPrint | 0 | no constraint |

```
gas(opcodes) = Σ rows(opcode, operands)
```

Fixed-length opcodes use the constant above; variable-length opcodes
(`RangeCheck`, `PoseidonHash`) compute their row count from the operand
annotations (bit width, input count) already present in the opcode list.

The gas table is consensus-critical — all wallet, mempool, and miner
implementations SHALL use identical values and identical formulas. The table is
hardcoded (with formulas for the variable-length ops) rather than derived from
manifests to prevent manifest parsing from becoming a consensus dependency.

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
using logarithmic scaling. Separate factors are computed for the high-priority
tier and the medium/low tiers:

```
CF_premium  = SCALE + α_premium  × floor(SCALE × log₂(P_premium  + 1))
CF_standard = SCALE + α_standard × floor(SCALE × log₂(P_standard + 1))

where:
    SCALE        = 1_000_000          (fixed-point scale for integer arithmetic)
    P_premium    = pending count in mempool high queue
    P_standard   = pending count in mempool medium + low queues + fee_index
    α_premium    = high-priority congestion sensitivity coefficient
    α_standard   = medium/low congestion sensitivity coefficient
    α_premium > α_standard > 0       (high priority always more sensitive)
    CF_premium > CF_standard         (structural invariant, always)
```

**Why logarithmic:** Doubling the queue depth adds at most `α × SCALE` to the
congestion factor. This prevents both premature saturation (linear) and
insufficient responsiveness (constant). The log₂ function maps queue depths
from 1 to 10,000 into congestion factors from 1.0 to ~1.0 + 13α.

**Coefficient defaults:**

```
α_premium  = 0.05   (CF doubles every ~1,000,000 high-priority transactions)
α_standard = 0.01   (CF doubles every ~2,000,000 medium/low transactions)
```

These defaults produce reasonable congestion pricing at mainnet scale while
remaining testable in devnet with smaller mempool sizes.

**Congestion factor consensus:** At each window boundary, every mining node
computes CF from its local mempool state deterministically (I1, I8). All
nodes synced to the same chain tip observe the same mempool state and compute
identical CF values — no coordination, gossip, or median consensus is
required. The `fee_window_flags` in the block header provide the canonical
signal for all downstream consumers.

#### 12.4.5 Block Charge — Declarative Capacity Promise

A transaction's **charge** is a **declarative promise** by the contract
deployer, stated in the manifest `[[cost_profiles]]`, of the block capacity
the transaction will consume. It is expressed as the nominal type
`BlockCharge(u64)` per type-system.md §2.3.1.

Unlike gas in thermodynamic systems (which measures actual work performed —
a WYSIWYG quantity), charge is **potential energy**: a pre-execution
commitment that the miner uses for block packing headroom. The distinction:

- **Gas is retrospective**: you learn the actual work after execution.
- **Charge is prospective**: the deployer declares it before execution, and
  the miner prices it through the risk model (§12.12.6).

A deployer who under-declares charge saves nothing — the miner observes the
deviation between declared and actual resource consumption, raises the
contract's risk factor, and the contract pays more over time. A deployer
who declares accurately converges toward `risk_factor = 1.0×` (genesis or
attested_endowed). The economic gradient pushes toward honest declaration
without requiring runtime gas metering.

The trait method `declare_charge(&tx) -> BlockCharge` provides the
per-transaction declared charge. In `select_for_block`, the miner
accumulates declared charges via `BlockCharge::saturating_add` to ensure
the block stays within its capacity budget. Charge does NOT limit execution,
does NOT determine fees directly, and does NOT appear in consensus
validation. A transaction whose declared charge is exceeded at runtime
still executes fully — the miner absorbs the cost and records the deviation.

Constants defined by this specification:

| Constant | Value | Description |
|----------|-------|-------------|
| `CHARGE_PER_CALL` | `400_000_000` | Declared charge per contract call |

This value is calibrated so that a block at `BLOCK_GAS_LIMIT` can
accommodate approximately 250 average contract calls, matching the
`MinerConfig.max_txs = 250` default. It is the deployer's responsibility
to declare a higher charge in `[[cost_profiles]]` for circuits that
exceed the average ZK complexity.

### 12.5 Tier Price Computation

The three tier prices are the flat `base_price` scaled by fixed priority
multipliers:

```
PRICE_LOW    = base_price × LOW_MULTIPLIER      // 1×
PRICE_MEDIUM = base_price × MEDIUM_MULTIPLIER   // 2×
PRICE_HIGH   = base_price × HIGH_MULTIPLIER     // 4×
```

`base_price` is a flat wow-per-gas constant — a placeholder pending real gas
economics. The admission fee for a transaction is:

```
fee = gas × PRICE_tier × CF × risk
```

where `PRICE_tier` is one of the three prices above, `CF` is the congestion
factor (§12.4.4), and `risk` is the `ContractRiskTracker` factor (§12.12.6).

The multipliers are fixed so the tiers are uniform and predictable: a user
picks a tier, never an arbitrary fee. The congestion factor (§12.4.4) scales
all three tiers together at window boundaries, so the tiers move together with
demand.

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
first-come-first-served (FIFO). High queue drains before medium, medium
before low. No transaction can jump the queue by paying a higher fee after
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
    fee = extract_fee(tx)              // plaintext fee from FeeParamsV3
    declared_tier = extract_tier(tx)   // low / medium / high

    // Re-derive the expected fee from the manifest cost profile and tier.
    expected = gas(tx) × tier_price(declared_tier)
    if fee < expected:
        reject — fee below the declared tier's price

    // Plain three-tier priority (no ZK proof).
    if fee >= PRICE_HIGH:   admit to high_queue   (FIFO); return HIGH
    if fee >= PRICE_MEDIUM: admit to medium_queue (FIFO); return MEDIUM
    if fee >= PRICE_LOW:    admit to low_queue    (FIFO); return LOW
    reject — fee below PRICE_LOW
```

#### 12.8.2 Window Transition (at boundary block)

```
on_window_boundary(new_window):
    // Preserve existing queues — no eviction (I3)
    // New thresholds apply to NEW arrivals only
    // High queue drains FCFS, then medium, then low
    active_window = new_window
```

#### 12.8.3 Block Selection

```
select_for_block(limit):
    txs = []
    // 1. Drain high_queue (FCFS) until gas/tx limit
    while high_queue.not_empty() AND within_limit(txs):
        txs.append(high_queue.pop_front())
    // 2. Drain medium_queue (FCFS) until limit
    while medium_queue.not_empty() AND within_limit(txs):
        txs.append(medium_queue.pop_front())
    // 3. Drain low_queue (FCFS) until limit
    while low_queue.not_empty() AND within_limit(txs):
        txs.append(low_queue.pop_front())
    return txs
```

#### 12.8.4 Window Transition Timing

```
FEE_WINDOW_TRANSITION_DELAY = 30 seconds  (after boundary block timestamp)

After the final block of window N at height H:
  T_0 = block_timestamp(H)                          // block timestamp
  T_activate = T_0 + FEE_WINDOW_TRANSITION_DELAY    // 30-second grace period

  During [T_0, T_activate):  GRACE PERIOD
    - Mempool continues admitting under window N tier prices
    - Wallets re-derive fees against window N+1 tier prices
    - Miners compute CF from local mempool state (deterministic, I1, I8)
    - New transactions may submit with window-N or window-N+1 tier prices

  At T_activate:  PRICE ACTIVATION
    - Mempool switches to window N+1 tier prices for NEW arrivals
    - Previously admitted transactions preserved (I3, FCFS)
    - New arrivals with window-N tier prices: REJECTED (stale tier price)
    - Window-N+1 tier prices: accepted against new prices

  After T_activate:  WINDOW N+1 ACTIVE
    - Full enforcement of N+1 tier prices
    - Window-N tier prices permanently stale for new arrivals
```

The 30-second window aligns with the block time (120s), miner block assembly
time (< 5s), and sync poll interval (30s, observer.md). It gives wallets
adequate time to re-query headers and re-derive fees after a CF change.

### 12.9 Wallet Integration

The wallet discovers the current congestion factors by reading the latest block
header before computing the fee.

```
construct_fee(circuit_costs, wasm_bytes, latest_block, tier):
    flags = latest_block.header.fee_window_flags
    if flags & FEE_WINDOW_ACTIVE:
        (wasm_cf, circuit_cf) = decode_congestion_factors(flags, chain_state)
    else:
        (wasm_cf, circuit_cf) = (DEFAULT_WASM_CF, DEFAULT_CIRCUIT_CF)  // legacy

    wasm_kB = max(1, ceil(wasm_bytes.len() / 1024))

    // Identical formula to mempool compute_fee() (§12.4.1)
    base_gas = ((wasm_kB * BASELINE_STORAGE * wasm_cf)
               + (sum(circuit_costs) * circuit_cf)) / SCALE

    // Three-tier price: the user picks a tier, the fee is deterministic.
    fee = base_gas × tier_multiplier(tier)
    return fee
```

If the window boundary passes before the transaction is mined, the wallet SHALL
re-query the latest header and re-derive the fee with the new congestion factors.
The wallet SHALL NOT submit a transaction priced with a stale window.

### 12.10 Miner Integration

The miner computes CF deterministically from local mempool state at each
window boundary and encodes the result in the block header.

```
prepare_block(height, mempool, chain_state):
    header = build_header(height, ...)

    if is_window_boundary(height):
        // Deterministic from local mempool state (I1, I8)
        cf = chain_state.fee_window.compute_cf(
            mempool.high_queue_len(),
            mempool.medium_queue_len(),
            mempool.low_queue_len()
        )
        header.fee_window_flags = encode_flags(cf)
        mempool.update_tier_prices(PRICE_LOW, PRICE_MEDIUM, PRICE_HIGH)

    return assemble_block(header, mempool.select_for_block())
```

The CF is computed locally and deterministically. All nodes with the same
mempool state arrive at the same CF — no P2P gossip or median consensus is
needed. The `fee_window_flags` in the block header provide the canonical
signal for all downstream consumers (wallets, sync clients).

### 12.11 Circuit k-Value and Row Count

#### 12.11.1 Relationship

The Halo2 PLONK proving system uses a parameter `k` that determines the domain
size: `2^k` rows in the constraint system polynomial. Proving and verification
cost (multi-scalar multiplication over `2^k` points) scales with `2^k`.

The circuit's `k` is **derived** from its total row count, not chosen
independently: `k = ceil(log2(total_rows))` (with a small safety margin and a
minimum). Because gas (§12.4.2) already equals `Σ rows(opcode)`, the total gas
of a circuit *is* its row count, which *determines* its `k`. There is therefore
no separate `2^(k−K_REF)` multiplier — that scaling is a redundant proxy for the
row count and is **removed**.

```
gas(circuit)        = Σ rows(opcode)              (§12.4.2)
k(circuit)          = ceil(log2(gas(circuit)))     (derived, not a fee input)
verifier_work       = O(2^k) = O(gas(circuit))     (linear in total rows)
```

#### 12.11.2 Constants

| Constant | Value | Source |
|----------|-------|--------|
| `MAX_K` | 16 | `src/zkas/constants.rs` (max domain size) |
| `WINDOW_SIZE` (range check) | 10 | `sinsemilla::K` |
| `MERKLE_DEPTH_ORCHARD` | 32 | `src/sdk/src/crypto/constants.rs` |
| `L_ORCHARD_MERKLE` | 255 | Sinsemilla bits per field element |
| `SMT_FP_DEPTH` | 255 | `src/sdk/src/crypto/smt/mod.rs` |
| Poseidon `R_F`, `R_P`, `RATE` | 8, 56, 2 | `halo2_poseidon/p128pow5t3.rs` |

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

In DarkWow's gas-based model, risk is shared:

1. **User pays upfront** — the Fee_V2 proof commits to the input/output values,
   and the plaintext fee is `gas × tier_price`. Execution is gas-metered — a
   transaction may consume all of its gas before the state transition completes.

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

#### 12.12.7 Proving Parallelization — A Rho-Calculus Property

The fee model's proof architecture demonstrates a structural ρ-calculus
property: **proving is embarrassingly parallel across wallets, verification
is cheap and centralized at the miner.** This follows directly from the
object-capability model (ocap.md):

- Each wallet holds its own `ν`-bound secret key — a name that no other
  process possesses. Proving requires this name, so proving cannot be
  delegated or pooled. The wallet alone can construct the Fee_V2
  (mass-balance) proof.

- Verification requires only the public verification key and public inputs.
  No secret material. The miner verifies the plaintext fee at mempool admission —
  a single cheap comparison (~70ms
  per transaction, ~18 seconds for a full 250-tx block).

- Proving time per wallet: ~2 seconds (Fee_V2, measured
  via FI-TIME-1 benchmark). This
  is entirely local — the wallet generates proofs while offline or during
  block assembly, with no coordination. A network with 10,000 active
  wallets has 10,000× the proving throughput of a single wallet.

- The miner's mempool verification is the bottleneck (~250 txs × 70ms =
  ~18s), not the wallets' proving. This matches the ρ-calculus principle:
  work stays at the name holder, verification is a public computation.

This is the inverse of token-weighted systems where computation is
centralized (a single sequencer or validator set) and governance controls
admission. In DarkWow, every wallet proves independently, every miner
verifies independently, and the architecture guarantees throughput without
any coordination layer. The ρ-calculus primitives in genesis are necessary
and sufficient — no additional infrastructure is required.

## 13. Active Guardrails and Chain-Synced Values

This section defines the safety rules specific to the fee signalling system.
The general type-system rules (no bare integer literals, no bare `unwrap_or`,
nominal newtypes for domain quantities) are defined in `type-system.md` and
apply universally. The rules here address fee-specific failure modes: silent
fallbacks that bypass the dynamic update mechanism, values that diverge
between nodes, and missing diagnostics on consensus-critical paths.

### 13.1 Architectural Principle

> All fee values SHALL be initialized at genesis and updated at window
> boundaries (every 20 blocks) via chain state. Nodes SHALL read current
> values from the chain, not from compile-time constants. A value that
> doesn't sync to the chain isolates the node and makes it non-functional.

The fee system is the universal coordination mechanism across the stack.
Every node, wallet, miner, and contract that participates in fee signalling
MUST produce identical results from the same chain state. A divergence in
any component — a different fallback value, a different threshold
computation, a different flag encoding — IS a consensus failure.

A compile-time constant cannot adapt to network conditions. A value that is
correct for a 2-node testnet is wrong for a 1000-node mainnet. Chain-synced
values ensure every node converges on the same parameters by reading the same
chain. This is the defining property: **no node has local fee parameters.**

The `fee_window_flags` in the block header are the public channel for this
coordination. They encode congestion direction (hold/+10%/-10%) for both the
circuit execution CF and the WASM storage CF. Every node reads these flags
from the block header and derives identical congestion factors via
`derive_cfs()`. A node that substitutes a local value for a chain value has
forked itself from the network.

### 13.2 SPEC-1: Genesis-Initialized, Window-Updated

Every value that affects fee computation SHALL be initialized in genesis
state and SHALL be updatable at window boundaries (every 20 blocks) through
the PID controller defined in §12.

Compile-time constants SHALL be limited to:
- Pure mathematical scaling factors (`SCALE = 1_000_000`, `RISK_FACTOR_SCALE = 100_000`)
- Structural parameters that define the update mechanism, not the values
  (`WINDOW_SIZE = 20`, `K_REF = 11`, `MAX_K = 16`)

Values that define economic parameters SHALL NOT be compile-time constants:
- Baseline storage cost (currently `BASELINE_STORAGE = 1_000_000`)
- Congestion sensitivity coefficients (currently `ALPHA_PREMIUM = 0.05`,
  `ALPHA_STANDARD = 0.01`)
- Adjustment caps (currently `MAX_ADJUSTMENT = 0.10`)
- Risk factor classifications and their multipliers

These SHALL be stored in chain state (sled trees under the native_token
contract) and initialized at genesis. The PID controller SHALL read current
values from chain state at each window boundary, not from compile-time
constants. A future governance mechanism MAY update these parameters; the
mechanism for doing so is out of scope for this specification.

### 13.3 SPEC-2: Chain-Derived, Not Local

When computing a fee, verifying a threshold, or admitting a transaction,
nodes SHALL derive fee parameters from the current chain state. Acceptable
sources are:

1. Block header `fee_window_flags` → `derive_cfs()` → `compute_fee()`
2. Contract manifest `[[cost_profiles]]` → `resolve_cost_profile()` →
   `compute_total_fee()`
3. Chain state sled trees (fee accumulator, contract risk state)
4. Values deterministically derived from (1), (2), or (3)

A node that substitutes a local constant for a chain value SHALL be
considered out of sync. Specifically:

- `prepare_block()` SHALL compute `total_fees` from `NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner()` (`bin/dwowd/src/lib.rs`)
  results, NOT from a compile-time estimate.
- `Mempool::add()` SHALL compute admission thresholds from the current CF
  values stored in the mempool (updated at each window boundary by the miner),
  NOT from a local default.
- The wallet SHALL compute fees using `fee_window_flags` from the latest
  synced block header, NOT from `FeeWindowFlags::default()`.

**Case study — the `1_001_000` fallback.** During the 2026-08 red team audit,
`prepare_block()` was found to use `.unwrap_or(1_001_000)` when fee
decryption failed. The value `1_001_000` is `compute_fee(&[1000], 1, cf, cf)`
at identity CF — a compile-time constant that never updates. When the wallet
starts encrypting fees (SPEC-5), the decrypted real value diverges from the
hardcoded constant → `total_fees` mismatches the Pedersen accumulator →
`fee_collect_v1()` Check 2 fails → block rejected. The root cause: substituting
a local constant for a chain-derived value. The fix: remove the constant,
skip transactions whose fees cannot be verified.

### 13.4 SPEC-3: No Silent Fallbacks on Consensus-Critical Computation

Any fallback value that participates in block hash, state root, or transaction
inclusion SHALL be either:

**(a) Proven identical across all honest nodes by construction.** All nodes
derive the same value from the same chain state through deterministic
computation. Example: two miners computing `compute_fee()` with identical
`(circuit_costs, wasm_kb, circuit_cf, wasm_cf)` produce identical `FeeAmount`
values because all inputs are chain-derived.

**(b) Absent — the call site SHALL fail hard.** Return `Err`, reject the
block, skip the transaction with a logged diagnostic. Example: if
`NativeTokenFeeSignallingExtractor::decrypt_fee_for_miner()` (`bin/dwowd/src/lib.rs`) fails, the miner SHALL skip that fee call and
log a warning — NOT substitute an estimate and proceed as if decryption
succeeded.

A fallback value that is neither proven-identical nor hard-failing is a
consensus-divergence hazard. The `decrypt_fee_for_miner().unwrap_or(1_001_000)`
pattern is the exemplar: the fallback value is not chain-derived, not proven
identical, and silently produces different `total_fees` when decryption
results differ between miners.

**Diagnostic requirement.** When a consensus-critical operation fails and the
node skips or rejects, it SHALL emit a diagnostic (`warn!` or `error!`) that
identifies:
- Which operation failed (e.g., `fee decrypt`)
- Why it failed (e.g., `EmptyCiphertext`, `DecryptionFailed`, `WrongKey`)
- Which transaction or block was affected
- What action was taken (e.g., `skipping fee call`, `rejecting block`)

The `decrypt_fee_for_miner() -> Option<u64>` pattern (all failure modes
collapse to `None` with zero diagnostic) is insufficient. Use `Result<u64,
FeeDecryptError>` with distinct error variants.

### 13.5 SPEC-4: No Feature Gates on Consensus-Critical Paths

The fee window threshold update, congestion factor adjustment, and flag
encoding paths SHALL NOT be behind `#[cfg(feature = "...")]` or any other
compile-time conditional. Consensus-critical code that can be compiled out
creates a fork risk between nodes with different feature flags.

**Case study — `#[cfg(feature = "fee-window")]`.** During the 2026-08 red
team audit, the entire threshold update path in `miner_task()` was found
behind `#[cfg(feature = "fee-window")]`, with `#[cfg(not(feature =
"fee-window"))]` using `FeeWindowFlags::default()`. Two nodes compiled with
different features produce blocks with different `fee_window_flags` and
different mempool admission outcomes → chain fork.

If a runtime toggle is needed for testing, use a field in the consensus
configuration that all nodes agree on (e.g., a `fee_window_active: bool`
in the chain state initialized at genesis). The toggle itself becomes a
chain-synced value per SPEC-2.

### 13.6 SPEC-5: Encrypted Fee Channel (REMOVED)

> **REMOVED.** The encrypted fee channel is deleted in the public gas/fee model —
> the fee is plaintext in `FeeParamsV3` and read directly by the miner. There is no
> AEAD ciphertext and no `encrypted_fee_value` field. The historical rationale below
> is retained for reference only.

A privacy-preserving fee transaction SHALL carry a non-empty `encrypted_fee_value` field.
The encrypted fee channel is the ONLY path by which the miner learns exact
fee amounts — the `threshold` field proves `fee >= threshold` but does not
reveal the fee itself.

A privacy-preserving fee transaction with `encrypted_fee_value.len() < 68` SHALL be rejected
at mempool admission. The 68-byte format is:
```
[ephemeral_public (32 bytes)] [nonce (12 bytes)] [ciphertext+tag (24 bytes)]
```

**Rationale.** The fee privacy model requires that fee amounts are hidden
from all parties except the miner. The Pedersen commitment in the accumulator
provides public verifiability of the total; the AEAD ciphertext provides
private knowledge of individual amounts to the miner. An empty ciphertext
breaks both properties: no party learns the exact fee, and the miner cannot
compute a correct `total_fees` for FeeCollectV1.

**Activation hazard.** The wallet and miner sides of this channel SHALL be
implemented together. Fixing the wallet to encrypt fees without also removing
the miner's `unwrap_or(estimate)` fallback creates an immediate consensus
divergence: the first wallet to encrypt produces a transaction that the old miner
miscomputes, producing a different `total_fees` → different FeeCollectV1 →
different block hash → chain fork.

### 13.7 SPEC-6: Accurate Congestion Measurement Under Load

`premium_queue_len()` (the high queue) and `standard_queue_len()` (the medium
+ low queues, per §12.4.4) drive the PID controller that sets network-wide fee
thresholds at each window boundary (§12.4). These accessors SHALL return
accurate queue depths.

The `try_lock().unwrap_or(0)` pattern is prohibited for congestion
measurement. Under mempool load — the exact moment accurate measurement is
most critical — lock contention causes `try_lock()` to fail, returning 0.
The PID controller sees zero pending transactions and leaves thresholds
unchanged, disabling the fee market precisely when it is needed.

Acceptable alternatives:
- Blocking `lock()` — the miner's window boundary check is infrequent
  (once per 20 blocks) and can tolerate brief blocking
- Approximate counters in `AtomicU64` updated on each `add()` and
  `select_for_block()` — slightly stale but never catastrophically wrong

### 13.8 Guardrail Summary

| Guardrail | Rule | Verification |
|-----------|------|-------------|
| GS-1 | Fee values are genesis-initialized, window-updated | Audit: no compile-time economic constants outside §13.2 list |
| GS-2 | Nodes read fee parameters from chain state | Audit: no `const` fee value used in consensus path |
| GS-3 | No silent fallbacks on consensus-critical paths | Audit: no `.unwrap_or(non_zero)` on values affecting block hash |
| GS-4 | No feature gates on consensus-critical fee code | Audit: grep for `#[cfg(feature` in fee window, threshold, CF paths |
| GS-5 | `encrypted_fee_value` mandatory, ≥68 bytes | CI: mempool admission rejects short ciphertext |
| GS-6 | Congestion measurement accurate under load | Audit: no `try_lock().unwrap_or(0)` in queue length accessors |
| GS-7 | All failures produce diagnostics | Audit: `decrypt_fee_for_miner` returns `Result`, not `Option` |

## 14. Fee System Invariants

Each invariant has a unique tag (`FI-{domain}-{number}`), states the invariant
precisely with SHALL/SHALL NOT language, declares its scope (which components it
spans), and specifies the minimum testing level required (L1, L1.5, L2, L3 per
`doc/src/dev/testing/overview.md`).

### 14.1 Genesis Initialization

**FI-GEN-1: Genesis initialization.** System fee parameters (baseline storage cost,
congestion sensitivity coefficients, adjustment caps, ContractRiskTracker parameters)
SHALL be stored in genesis sled state. `FeeWindowState::load()` SHALL reject partial
persistence (a non-empty strict subset of the 8 parameter keys, indicating a crash
during save); an empty store (0 keys, pre-activation blocks) is accepted. Scope:
chain_state. Level: L1.

**FI-GEN-2: No compile-time fee constants.** No `const` or `static` of type
`FeeAmount`, `CongestionFactor`, `RiskFactor`, or `BlockCharge` SHALL exist. The
only permitted compile-time constants are pure mathematical scaling factors (`SCALE`,
`RISK_FACTOR_SCALE`) and structural parameters that define the update mechanism
(`WINDOW_SIZE`, `K_REF`, `MAX_K`). Scope: all crates. Level: CI grep gate.

### 14.2 Fee Window + Congestion Factors

**FI-WINDOW-1: Window boundary adjustment.** At every height ≡ 0 (mod WINDOW_SIZE),
congestion factors SHALL be recomputed from current mempool queue depths, capped at
±MAX_ADJUSTMENT of previous values, and encoded into `fee_window_flags` on the next
block header. Scope: miner_task + FeeWindowState + BlockHeader. Level: L2.

**FI-WINDOW-2: Deterministic CF computation.** `compute_cf(premium_pending,
standard_pending, alpha_premium, alpha_standard)` SHALL produce identical results
on all nodes given the same inputs. Floating-point arithmetic SHALL NOT be used.
Scope: fee_window.rs. Level: L1.

**FI-WINDOW-3: CF ordering.** If `premium_pending > 0` or `standard_pending > 0`,
`cf.premium >= cf.standard`. At zero congestion, equality is acceptable. Scope:
fee_window.rs. Level: L1.

**FI-WINDOW-4: Backward compatibility (I2).** Blocks without `fee_window_flags`
(pre-activation, `fee_window_flags == 0`) SHALL be treated as having zero
congestion: WASM_CF = CIRCUIT_CF = SCALE. `#[serde(default)]` ensures old
blocks deserialize correctly. Scope: BlockHeader + fee_window.rs. Level: L1.

**FI-WINDOW-5: Opcode difficulty monotonicity (I5).** A transaction with higher
total opcode difficulty SHALL never pay a lower total fee than one with lower
difficulty, for identical WASM size and congestion regime. The per-opcode
difficulty table (§12.4.2) SHALL be the sole cost-ordering determinant.
Scope: fee_window.rs + opcode_cost.rs. Level: L1.

**FI-WINDOW-6: CF convergence (I6).** As mempool queue depth → 0, CF SHALL
converge to SCALE for both tiers. As queue depth grows, CF SHALL grow
logarithmically — doubling the queue adds at most α to the factor.
Scope: fee_window.rs. Level: L1.

**FI-WINDOW-7: Deterministic CF computation (I8).** The window's congestion
factor SHALL be computed locally from the miner's mempool queue depth at the
window boundary. All nodes synced to the same chain tip observe identical
CF values — no coordination or gossip is required. I1 guarantees determinism.
Scope: fee_window.rs + miner_task. Level: L1.

### 14.3 Flags

**FI-FLAG-1: Flags chain-synced.** The `fee_window_flags` field in the block header
SHALL encode the congestion direction computed at the most recent window boundary.
A wallet reading flags from block N SHALL derive the same CFs that the miner used
to set mempool thresholds for block N+1. Scope: miner → BlockHeader → wallet.
Level: L1.5.

**FI-FLAG-2: Flags excluded from block hash.** `fee_window_flags` SHALL NOT
participate in the block hash computation. This prevents circular dependency:
flags depend on mempool state at mining time, block hash depends on header fields.
Scope: BlockHeader + mining. Level: L1.

**FI-FLAG-3: Flags advisory.** `accept_block` SHALL NOT reject a block for invalid
or reserved `fee_window_flags` bits. Flags are signalling hints, not consensus rules.
Scope: accept_block. Level: L1.

### 14.4 Plaintext Fee (replaces Encrypted Fee Channel)

**FI-PLAIN-1: Mandatory plaintext fee.** Every FeeV3 transaction SHALL carry a
plaintext `fee: FeeAmount` in `FeeParamsV3`. The fee SHALL be readable directly from
call data — no encryption, no Pedersen commitment. Scope: wallet → mempool. Level: L1.

**FI-PLAIN-2: Deterministic fee.** The fee SHALL equal `gas × tier_price`, where
`gas` is the deterministic work measure (§12.4) and `tier_price` is one of the three
tier prices (§7.1). The wallet and miner SHALL independently derive the same fee. A
fee that does not match the re-derived value for its declared tier SHALL be rejected.
Scope: wallet + miner. Level: L1.

### 14.5 Mempool Admission

**FI-ADMIT-1: Three-tier admission.** Mempool SHALL admit a FeeV3 transaction to the
high tier if `fee >= PRICE_HIGH`, the medium tier if `fee >= PRICE_MEDIUM`, the low
tier if `fee >= PRICE_LOW`, and reject otherwise. Tier prices SHALL be updated at
window boundaries from chain-derived CF values. Scope: mempool. Level: L1.

**FI-ADMIT-2: FCFS within tiers.** High SHALL drain before medium, medium before low.
Within each tier, transactions SHALL be selected in FIFO order. Scope: mempool.
Level: L1.

**FI-ADMIT-3: Nullifier replay rejection.** Mempool SHALL reject a transaction whose
nullifier is already in the mempool or on-chain. Scope: mempool + chain_state.
Level: L1.5.

### 14.6 Fee Collection

**FI-COLLECT-1: Plain fee accumulation.** At block start, `fees_db[height]` SHALL be
zero. Each FeeV3 `apply_fee` SHALL add its plaintext `fee` to `fees_db[height]`
(plain u64 addition — no Pedersen accumulator). `FeeCollectV1` SHALL claim
`fees_db[height]` for the miner and reset it. Scope: native_token contract. Level: L2.

**FI-COLLECT-2: Supply neutrality.** FeeCollectV1 SHALL NOT change the cumulative
token supply. Fees transfer value from fee-payer to miner; they do not mint or burn.
Scope: native_token contract + chain_state. Level: L2.

### 14.7 Risk → Dynamic Tracker Multiplier on the Fee

Risk is a dynamic fee multiplier (1.0× → 2.0×) sourced from
`ContractRiskTracker` — the observed-vs-declared `BlockCharge` ratio stored per
`contract_id` in the `contract_risk` sled tree and updated at fee-window
boundaries. The wallet additionally computes a trust metric for observability
(not consensus-gating).

**FI-RISK-1: Risk multiplier on the fee.** `compute_fee()` SHALL multiply the
circuit component by the `ContractRiskTracker` factor (1.0× → 2.0×) for the
contract being called. The fee is `gas × base_price × CF × tier × risk`. Scope:
fee_window.rs + wallet + mempool + miner. Level: L1.

**FI-RISK-2: Wallet trust metric (observability).** The wallet SHALL compute a basic
trust metric for a contract — from contract age, whether the transaction path has
been used before, attestation, and wallet-side checks of the WASM — to inform the
user's decision. This metric SHALL NOT gate consensus or mempool admission. Scope:
wallet. Level: L2.

**FI-RISK-3: BlockCharge update loop.** The manifest SHALL declare a self-declared
`BlockCharge` (expected gas). When a transaction runs, the miner SHALL compare
*observed* gas to the declared charge and update the stored charge via the risk
multiplier (observed / declared), through `ContractRiskTracker`. Scope: miner +
ContractRiskTracker. Level: L2.

**FI-RISK-4: Per-contract chain-state storage.** The updated BlockCharge/risk
multiplier SHALL be stored per `contract_id` in the `contract_risk` sled tree, read
by the miner when pricing deviations. No global classification table. Scope:
chain_state. Level: L1.5.

**FI-RISK-5: Manifest role separation.** The manifest's `[[cost_profiles]]` section
SHALL declare expected circuit difficulty, k-value, WASM size, and the self-declared
`BlockCharge`. The manifest SHALL NOT declare a fee risk multiplier. Scope:
manifest.rs. Level: CI grep gate.

### 14.8 WASM Deployment

**FI-WASM-1: DeployV1 wasm_kB detection.** `extract_tx_wasm_kb()` SHALL detect
DeployV1 transactions (contract_id == DEPLOYOOOR_CONTRACT_ID, selector == 0x00)
and return `max(1, ceil(wasm_bytes.len() / 1024))`. For non-deploy transactions,
return 1. Scope: mempool. Level: L1.5.

**FI-WASM-2: WASM component in admission.** The mempool admission threshold SHALL
include the WASM storage component: `wasm_kB × baseline_storage × wasm_cf / SCALE`.
A deploy transaction with wasm_kB > 1 SHALL pay a proportionally higher threshold
than a simple transfer. Scope: mempool. Level: L2.

### 14.9 Proof Timing

**FI-TIME-1: Proof generation within window.** Fee_V2 proof generation
time SHALL be less than the window boundary deadline (block production interval).
A proof that takes longer than the block interval to generate cannot be included
in a block. Scope: wallet. Level: L1 benchmark.
