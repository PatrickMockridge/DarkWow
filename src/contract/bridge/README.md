# DarkWow Bridge Contract

> **USE AT YOUR OWN RISK.** This contract has undergone internal simulation-based security review (May 2026) but has NOT been independently audited. Cross-chain bridges carry inherent risks. See [AUDIT.md](../AUDIT.md) for full findings, mitigations, and residual risks.

Anonymous bridge contract for cross-chain asset transfers.

## Overview

The bridge contract enables privacy-preserving transfers between DarkWow and
external blockchains (initially Ethereum). Key features:

- **Anonymous deposits**: External chain deposits are mixed, breaking on-chain links
- **ZK proofs**: All bridge operations verified via zero-knowledge proofs
- **Object Capability Security**: Replaces VSS with deterministic address derivation
- **No VSS required**: Users control their own funds via secrets, no threshold signing

## Core Security Model: VSS vs Object Capability

### The VSS Problem

Traditional bridge designs use **Verifiable Secret Sharing (VSS)** for custody:

```
User deposits → VSS nodes hold secret shards → Withdrawal requires n-of-m threshold
```

**Vulnerabilities:**
1. **VSS Node Compromise**: Any t of n nodes can reconstruct secret and steal funds
2. **Centralization**: Threshold nodes can censor withdrawals
3. **Complexity**: DKG, dishonest majority attacks, liveness requirements
4. **Slow**: Threshold signing round required for each withdrawal

### The Object Capability Solution

DarkWow bridge uses **deterministic address derivation** instead of VSS:

```
User knows secret → Derive bridge_address = H(recipient_identity, nonce) → Deposit
User knows secret → Compute nullifier = H(secret) → Withdraw (self-signed)
```

**Advantages:**
1. **No shared secrets**: Bridge nodes cannot know user's bridge secret
2. **Fast withdrawals**: No threshold coordination, just ZK proof verification
3. **Censorship resistant**: User alone authorizes, no gatekeepers
4. **Simple**: No DKG, no threshold cryptography

### Security Comparison

| Aspect | VSS-Based Bridge | DarkWow OCap Bridge |
|--------|-----------------|---------------------|
| Key custody | Distributed shards | User-held secrets |
| Withdrawal speed | Slow (round) | Fast (self-signed) |
| Node compromise | Catastrophic | Impossible |
| Censorship | Threshold can block | Cannot block |
| Complexity | High (DKG) | Low (hashing) |

## Structure

```
bridge/
├── proof/          # ZK proof circuits (.zk files)
├── src/
│   ├── client/     # Client-side transaction builders (DepositBuilder, WithdrawBuilder)
│   ├── entrypoint/ # WASM contract entrypoint (expanded with step-by-step logic)
│   ├── model/      # Data structures (DepositParams, WithdrawParams, Deposit, Withdrawal)
│   └── lib.rs      # Contract definitions (BridgeFunction enum) and constants
├── tests/          # Integration tests
├── Cargo.toml
└── Makefile
```

## Building

```bash
# Build WASM contract
make

# Compile ZK circuits
make proof

# Run tests
cargo test
```

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize bridge state |
| DepositV1 | 0x01 | Register external chain deposit |
| WithdrawV1 | 0x02 | Request withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge configuration |
| CancelWithdrawV1 | 0x04 | Cancel timed-out withdrawal |
| ExecuteGuaranteedWithdrawV1 | 0x05 | Execute guaranteed withdrawal with pool stake |
| CreateHtlcV1 | 0x06 | Create HTLC swap for cross-chain atomic swap |
| ClaimHtlcV1 | 0x07 | Claim HTLC swap with secret |
| RefundHtlcV1 | 0x08 | Refund HTLC swap after timelock expiry |
| ReassignWithdrawalV1 | 0x09 | Reassign stuck withdrawal to a new relayer |

## Implementation Flow

### How Deposit Works (Client-Side)

```rust
// 1. Derive bridge address for recipient
let bridge_address = derive_bridge_address(recipient_pub_x, recipient_pub_y, nonce);

// 2. User deposits ETH to bridge_address on Ethereum
//    (done via external wallet/interface)

// 3. Wait for confirmations, get Merkle proof from indexer
let merkle_proof = indexer.get_deposit_proof(tx_hash).await?;

// 4. Build deposit using DepositBuilder
let deposit = DepositBuilder::new()
    .secret(secret)
    .amount(eth_amount)
    .recipient_pub(recipient_pub_x, recipient_pub_y)
    .nonce(nonce)
    .merkle_proof(merkle_proof)
    .external_block_hash(block_hash)
    .build()?;

// 5. Submit to DarkWow bridge contract
client.submit(deposit).await?;
```

### How Withdrawal Works (Client-Side)

```rust
// 1. User has a note from a previous deposit
let note = user.get_bridged_note();

// 2. Compute nullifier = H(secret)
let nullifier = compute_nullifier(note.secret);

// 3. Determine recipient on Ethereum
let recipient_hash = hash(ethereum_address);

// 4. Build withdrawal using WithdrawBuilder
let withdrawal = WithdrawBuilder::new()
    .nullifier(nullifier)
    .recipient_hash(recipient_hash)
    .amount(withdraw_amount)
    .feed_mode(0)           // 0=standard, 1=guaranteed (with premium)
    .max_fee_bp(Some(500))  // cap relayer fee at 5% (optional)
    .build()?;

// 5. Submit to DarkWow bridge contract
client.submit(withdrawal).await?;

// 6. Relayer sees event, broadcasts ETH tx to Ethereum
// 7. If relayer unresponsive: reassign via ReassignWithdrawalV1, or cancel after timeout
```

### How Deposit is Processed (Contract-Side)

```
1. Verify Merkle proof of deposit on external chain
   └── Ensures deposit actually exists and is confirmed

2. Verify minimum confirmations reached
   └── Prevents reorg attacks

3. Verify deposit hasn't already been registered
   └── Prevents double-deposit

4. Derive bridge_address from params
   └── commitment = H(secret, amount, bridge_address)

5. Store deposit commitment in Merkle tree
   └── Makes deposit "claimable" by user

6. Emit DepositRegistered event
   └── Notifies indexers of new deposit
```

### How Withdrawal is Processed (Contract-Side)

```
1. Verify ZK proof of withdrawal authorization
   └── Proves user knows secret for a committed deposit

2. Check nullifier not yet spent
   └── Prevents double-spend

3. Mark nullifier as spent
   └── Permanently prevents reuse of this deposit

4. Emit WithdrawalRequested event
   └── Authorizes relayer to send ETH to user
```

### Security Checks at Each Step

| Step | Check | Why |
|------|-------|-----|
| Deposit | Merkle proof verification | Ensures deposit exists on external chain |
| Deposit | Minimum confirmations | Prevents reorg attacks |
| Deposit | Not already registered | Prevents double-deposit |
| Withdraw | ZK proof valid | Proves ownership without revealing secret |
| Withdraw | Nullifier not spent | Prevents double-spend |

## Hardening (May 2026)

The bridge contract underwent a hardening pass in May 2026 to address 14 failure modes identified by discrete-event simulation. See [AUDIT.md](../AUDIT.md) for the full audit report.

### HTLC State Machine Atomicity

**Problem**: Claim and refund could both succeed on the same HTLC if they arrived in the same block.

**Fix**: `HtlcSwapInfo` now tracks `claimed_at: Option<u64>` and `refunded_at: Option<u64>`. Claim only valid from `Pending` state. Refund checks `claimed_at.is_none()` atomically in `process_update`. Both timestamps provide mutual exclusion.

### Circuit Breaker for Guaranteed Withdrawals

**Problem**: Relayer could accept unlimited guaranteed withdrawals, leading to capital exhaustion when total pending exceeds available stake.

**Fix**: Bridge maintains a `GUARANTEED_PENDING` counter. `process_withdraw_instruction` rejects new guaranteed withdrawals when `guaranteed_pending + amount > max_guaranteed_total`. The counter is incremented on withdrawal acceptance and decremented on execution, cancellation, or timeout. Configurable via `MAX_GUARANTEED_TOTAL` constant.

### Withdrawal Reassignment

**Problem**: If a relayer crashed or was partitioned after accepting a withdrawal, no other relayer could take over. Funds stuck until timeout (100 blocks).

**Fix**: `ReassignWithdrawalV1` (opcode `0x09`) — any relayer can claim a stuck withdrawal after `reassignable_after` block height. `PendingWithdrawal` tracks `reassignable_after` (set at withdrawal acceptance) and `heartbeat_at`. Original relayer is partially slashed for abandonment.

### Proportional Slashing

**Problem**: Slash amount was flat `1_000_000` regardless of withdrawal size. A 1 DAI slash on a 1000 DAI withdrawal provided no meaningful deterrent.

**Fix**: Slash computed as `max(MIN_SLASH, amount * SLASH_BP / BP_PRECISION)`. Constants: `MIN_SLASH = 1_000_000` (floor), `SLASH_BP = 1000` (10%), `BP_PRECISION = 10000`. Slash now scales with withdrawal amount.

### Fee Caps

**Problem**: No upper bound on relayer fees. A monopoly relayer could charge extortionate rates.

**Fix**: Bridge enforces `MAX_FEE_BP = 1000` (10% maximum). Users can specify a tighter `max_fee_bp: Option<u64>` in `WithdrawParams`. Withdrawal validates `fee <= amount * effective_max_fee_bp / BP_PRECISION`.

### Token-Aware Dust Minimum

**Problem**: ZK circuit had a hardcoded dust threshold (`100_000_000`) with a TODO to make it token-aware.

**Fix**: `withdraw_v1.zk` now accepts `token_minimum: Base` as a public input. The bridge contract passes the token-specific minimum from bridge config in `get_metadata`. Circuit enforces `less_than_strict(token_minimum, amount)`.

### New Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `BP_PRECISION` | 10000 | Basis points precision |
| `MIN_SLASH` | 1,000,000 | Minimum slash amount (floor) |
| `SLASH_BP` | 1000 | Slash as proportion of amount (10%) |
| `MAX_FEE_BP` | 1000 | Maximum relayer fee (10%) |
| `MIN_GUARANTEED_COVERAGE_RATIO` | 15000 | Required relayer stake coverage (150%) |
| `WITHDRAWAL_TIMEOUT_BLOCKS` | 100 | Blocks before withdrawal can be cancelled |

### New Error Variants

| Error | Code | Description |
|-------|------|-------------|
| `InsufficientGuaranteeCoverage` | Custom(22) | Relayer stake too low for guaranteed withdrawal |
| `FeeExceedsCap` | Custom(23) | Relayer fee exceeds maximum allowed cap |

## Design Principles

### 1. Deterministic Address Derivation

Bridge addresses are derived as:
```
bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, bridge_nonce)
bridge_pub = bridge_secret * G
bridge_address = poseidon_hash(bridge_pub.x, bridge_pub.y)
```

This ensures:
- Fresh address per deposit (temporal privacy via nonce)
- No VSS key shards to steal
- Recipient alone controls address

### 2. Zero-Knowledge Membership Proofs

Withdrawal uses ZK proofs to demonstrate:
- Knowledge of deposit secret
- Deposit exists in bridge's Merkle tree
- Without revealing which deposit (hidden leaf index)

### 3. Nullifier-Based Double-Spend Prevention

```
nullifier = poseidon_hash(secret)
```

Spending a deposit reveals nullifier but not secret. Bridge contract
tracks spent nullifiers to prevent double-spend.

### 4. Temporal Boundary Enhancement

Each deposit gets unique bridge address via nonce. Even same recipient
depositing multiple times produces unlinkable addresses.

## Base Field Arithmetic

ZK circuits operate in a finite field — the Pallas field defined by prime `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`. All arithmetic wraps at `p`, which breaks normal integer intuitions:

```zk
# In the field, p-1 ≡ -1, so comparisons must be carefully designed
# An opcode that returns a comparison result must handle field wraparound correctly
```

**Why this matters for bridge**: Withdrawal conditions like "amount <= fee threshold" or "confirmations >= minimum" require comparing field elements as integers. The field wraparound means naive comparison can give incorrect results when values are near `p`.

**The core challenge**: Proving `a <= b` as integers requires determining whether `a - b` falls in `{0, 1, ..., (p-1)/2}` or `{(p+1)/2, ..., p-1}`. This is straightforward in normal code but requires careful gadget design in circuits.

**See**: [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md) for the full treatment.

## Opcode Discovery and Validation

**Opcode discovery must go hand-in-hand with building functionality** — not precede it.

When building the bridge contract's withdrawal circuit, we discovered that:
1. Merkle proof verification required the `merkle_root` opcode — not just a `poseidon_hash` of path elements
2. External block header verification requires additional opcodes that don't yet exist
3. Withdrawal conditions may need comparison opcodes for fee thresholds

**The correct workflow**:
1. Build the circuit with what exists
2. When a constraint can't be expressed, document the opcode gap
3. Implement the new opcode only when the actual use case is known
4. Validate the opcode against the specific circuit that needs it — not in isolation

The bridge's `deposit_v1.zk` now uses the real `merkle_root` opcode with proper `MerklePath` type (following the pattern from `money/burn_v1.zk`). The fix uses `zero_cond` for dummy leaf support and `constrain_equal_base` to verify the computed root matches the public input.

## Reasoned Opcodes

The bridge circuits use standard zkVM opcodes. Future enhancements may require:

### `LessThanOrEqual(a, b)` (IDEAL — but experimental with soundness issues)
**Purpose**: Compare if `Base a <= Base b`
**Reasoning**: Could enable more complex withdrawal conditions (e.g., "amount <= fee threshold").
**Technical debt**: Gate soundness is unverified — formal analysis needed before production use.

### `IsEqualBase(a, b)` (IDEAL — but experimental with soundness issues)
**Purpose**: Returns `0` or `1` for equality comparison
**Reasoning**: Could enable state machine transitions with proper equality checks.
**Technical debt**: Delta-invert soundness issue when `a == b`.

## Ideal vs Workaround: LessThanOrEqual vs Safemath

**`LessThanOrEqual` is the IDEAL solution**:
- Returns a 0/1 Boolean usable in downstream logic
- Single implementation in VM — no circuit bloat
- Full composability

**Safemath is a WORKAROUND with technical debt**:
- Only assertion gadgets (constrain-only, no Boolean return)
- Must be copied into each circuit
- Cannot replace LessThanOrEqual when Boolean return is needed

For fee thresholds or token-aware minimums, safemath can work **if** you only need to assert the constraint. If you need the result for further logic, LessThanOrEqual is required.

See [Safemath](../../../doc/src/arch/safemath.md) and [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for full analysis.

## Opcode Safety

**Comparison opcodes status**:

| Opcode | Status | Use in Bridge | Note |
|--------|--------|---------------|------|
| `LessThanOrEqual` | Implemented (experimental) | Future: fee thresholds, token-aware minimums | **IDEAL**: returns Boolean. **Debt**: gate soundness unverified |
| `IsEqualBase` | Implemented (experimental) | Future: state machine transitions | **IDEAL**: returns Boolean. **Debt**: delta-invert issue |
| `less_than_strict` | Sound (constrain-only) | ✅ Used in circuits | Safe but cannot return value |

**The bridge's current status**: The withdrawal circuit (`withdraw_v1.zk`) uses `constrain_equal_base`, `range_check`, and `less_than_strict` for minimum amount check. Future enhancements (fee thresholds, token-aware minimums) can use safemath assertion gadgets.

**See**:
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for the full analysis
- [Safemath](../../../doc/src/arch/safemath.md) for the workaround templates

## Key Blockers

| Blocker | Severity | Description |
|---------|----------|-------------|
| Merkle verification | **Fixed** | `deposit_v1.zk` uses real `merkle_root` opcode |
| External block header verification | **Critical** | `external_block_hash` not verified against actual chain |
| Light client integration | **High** | Requires external chain's header chain tracking |

**Note**: Current circuits avoid all experimental opcodes. Future features can use safemath assertion gadgets.

## Implementation Status

The contract **skeleton is expanded** to show actual implementation flow:

- `entrypoint.rs`: Contains step-by-step deposit/withdrawal processing with security checks
- `client/mod.rs`: Contains DepositBuilder and WithdrawBuilder with full transaction construction

### What Remains to Implement

The following items need actual Halo2/zkas circuit implementation:

### Phase 1: Core Deposit/Withdraw

1. **Deterministic Address Derivation**
   - Implement poseidon_hash for address computation
   - Implement EC operations (mul_base, get_x, get_y)
   - Verify address derivation matches commitment

2. **ZK Circuits**
   - Complete `deposit_v1.zk`: commitment + merkle proof + address derivation
   - Complete `withdraw_v1.zk`: nullifier + membership + range proof
   - Test with actual Halo 2 constraints

3. **External Chain Verification**
   - Ethereum block header verification
   - Merkle proof verification for contract storage
   - Light client integration (or oracle-based for v1)

4. **Deposit Flow**
   - Register deposit with sufficient confirmations
   - Cross-chain address derivation verification
   - Emit deposit event

5. **Withdrawal Flow**
   - ZK proof verification
   - Nullifier tracking (spent nullifiers tree)
   - External transaction construction (relayer or direct)

### Phase 2: Privacy Enhancement

6. **Deposit Mixing**
   - Merge multiple deposits into batch
   - Break on-chain deposit correlation
   - Increase anonymity set

7. **Temporal Privacy**
   - Random delay between deposit and claim
   - Random deposit ordering
   - Linkability resistance

### Phase 3: Trustless External Verification

8. **Light Client Integration**
   - Trustless Ethereum state verification
   - Block header relay
   - Reorg handling

9. **State Proofs**
   - Implement state proof verification
   - Verify arbitrary contract storage
   - BLS signature aggregation for proof of work

### Phase 4: Operational Security

10. **Slashing Conditions**
    - Invalid withdrawal proof slashing
    - Double-claim detection
    - Fraud proof system

11. **Emergency Mechanisms**
    - Emergency pause via DAO
    - Slashing oracle
    - Governance-controlled shutdown

## Security Considerations

- **No VSS means no VSS theft**: Even compromising all bridge nodes yields nothing
- **User custody**: Users hold their own secrets, bridge cannot spend
- **ZK proofs**: All verification is trustless, no trusted parties
- **Nullifiers**: Double-spend prevention without revealing identity
- **Fresh addresses**: Temporal privacy via per-deposit nonce

## How the Bridge Ensures Correctness, Security, and Ordered Operations

This section explains how the design guarantees basic bridge criteria,
bridged fund security, and correct operation ordering in both directions.

### 1. Basic Bridge Criteria

A functional bridge requires:

| Criterion | How It's Satisfied |
|-----------|-------------------|
| **Funds are accounted for** | Every deposit creates a commitment in the Merkle tree. Every withdrawal deducts from a nullified deposit. Arithmetic is verified in ZK. |
| **Operations are atomic** | Contract state changes happen in a single transaction. If proof verification fails, nothing is committed. |
| **No fund creation** | Withdrawals can only use deposited funds (proven via membership in deposit tree). Total minted ≤ total deposited. |
| **No fund destruction** | Burned deposits emit nullifiers. Unspent deposits remain in tree. |

### 2. Bridged Funds Security

**Who can spend user's deposit?**

Only the user knows `secret`. The withdrawal ZK proof requires demonstrating knowledge of `secret` corresponding to a commitment `C = H(secret, amount, bridge_address)`.

```
Attack: Can bridge nodes steal?
Answer: No. Bridge nodes never see secret. They only verify proofs.
        Even if all nodes are malicious, they cannot derive secret.

Attack: Can user double-spend?
Answer: No. Withdrawal reveals nullifier = H(secret).
        Contract tracks spent nullifiers. Second withdrawal fails.
```

**What prevents fake deposits?**

ZK proof in `deposit_v1` verifies:
1. Deposit exists in external chain (Merkle proof)
2. Commitment matches: `H(secret, amount, bridge_address)`

Without valid proof, no deposit is registered.

### 3. Operation Ordering: Deposit Direction (External Chain → DarkWow)

```
Step 1: User computes bridge_address
        bridge_address = H(secret * G) using user's identity

Step 2: User deposits to bridge_address on external chain
        (This happens outside DarkWow, on Ethereum)

Step 3: Oracle/light client detects deposit
        - Verifies Merkle proof of inclusion
        - Verifies block has sufficient confirmations

Step 4: User submits DepositV1 to DarkWow bridge contract
        - Submits commitment = H(secret, amount, bridge_address)
        - Submits ZK proof proving:
          a) Deposit exists on external chain
          b) User knows secret for this deposit
          c) Commitment is correctly formed

Step 5: Contract verifies proof
        - If valid: Inserts commitment into deposit Merkle tree
        - If invalid: Rejects, no state change

Correctness:
- Only real deposits get registered (external chain verification)
- Only commitment holder can later withdraw (secret knowledge required)
- Deposit order matches external chain order (block hash + height)
```

### 4. Operation Ordering: Withdrawal Direction (DarkWow → External Chain)

```
Step 1: User computes nullifier
        nullifier = H(secret)

Step 2: User generates withdrawal ZK proof proving:
        a) Commitment is in deposit Merkle tree
        b) User knows secret for this commitment
        c) nullifier = H(secret)
        d) Amount is valid (<= deposited amount)
        e) Recipient hash matches

Step 3: User submits WithdrawV1 to DarkWow bridge contract

Step 4: Contract verifies:
        a) ZK proof is valid
        b) nullifier has NOT been spent
        (Both must pass)

Step 5: Contract marks nullifier as spent
        - Inserts nullifier into spent_nullifiers tree
        - Records withdrawal

Step 6: Relayer broadcasts withdrawal tx to external chain
        (User can also broadcast directly)

Correctness:
- Proof verifies deposit exists without revealing which one
- Nullifier prevents double-spend
- Contract state and external state remain consistent
```

### 5. Why Each Step Must Happen in Order

| Direction | Step | Why It Must Come First |
|-----------|------|------------------------|
| Deposit | User deposits on external chain | Cannot register deposit before it exists |
| Deposit | Oracle confirms | Cannot register without proof of existence |
| Deposit | ZK proof verified | Cannot register invalid deposit |
| Deposit | Insert into Merkle tree | Finalizes deposit for withdrawals |
| Withdraw | ZK proof verified | Cannot withdraw without proving ownership |
| Withdraw | Nullifier check | Cannot withdraw if already withdrawn |
| Withdraw | Mark nullifier spent | Prevents double-withdrawal |
| Withdraw | Emit event | Triggers external chain broadcast |

### 6. Trustless Verification Without Oracles

**Problem**: Traditional bridges require trusted oracles to verify deposits.

**Solution**: ZK proofs + light client verification

For deposit:
- User proves deposit exists in external chain state
- Proof is verified by DarkWow contract (no oracle needed)
- Merkle root from block header commits to state

For withdrawal:
- No external verification needed
- DarkWow contract handles everything
- Relayer only broadcasts pre-authorized transaction

### 7. Consistency Guarantees

**What if external chain reorganizes?**

If a deposit's block is reorged out:
1. The deposit never existed on the canonical chain
2. The Merkle proof fails (root no longer matches)
3. Deposit registration fails → no funds minted

**What if withdrawal tx fails on external chain?**

Withdrawal is already recorded on DarkWow (nullifier spent).
User's funds are "gone" from DarkWow perspective.
Relayer can retry or user can submit direct tx.
(Trust model: relayer is trustless - withdrawal was pre-authorized)

**What if relayer censors withdrawal?**

User can broadcast directly to external chain.
Withdrawal was pre-authorized by ZK proof.
No threshold needed to release funds.

## Glossary

| Term | Definition |
|------|------------|
| **Pool** (UTXO) | In DarkWow's UTXO model, a "pool" refers to the collection of unspent transaction outputs (notes) held by the bridge contract. Unlike account-based systems where balances are stored at addresses, in UTXO systems, the pool tracks unspent outputs. When bridging assets, the bridge contract maintains a pool of notes representing deposited value. |
| **Note** | A UTXO representing a specific amount of value. In DarkWow, notes are encrypted commitments that can be spent by their owner using a zero-knowledge proof of knowledge of the secret. |
| **Asset Bridging** | The process of transferring value (tokens, coins) between chains. Requires liquidity on the destination chain and involves wrapping/unwrapping assets. Example: Wrapping ETH to create WETH on DarkWow. |
| **Data Bridging** | The process of passing arbitrary data (oracle data, state proofs, computations) between chains without value transfer. No liquidity required. Example: Passing a price feed from Ethereum to DarkWow. |
| **Merkle Inclusion Proof** | A cryptographic proof demonstrating that a specific element exists within a Merkle tree, without revealing all other elements. Used in the bridge to verify deposits exist on the external chain. |
| **OCap (Object Capability)** | A security model where access to objects is determined by capabilities (unforgeable references). In this bridge design, the "capability" is knowledge of the secret - no threshold signing needed. |
| **VSS (Verifiable Secret Sharing)** | A cryptographic scheme where a secret is split into shards distributed among participants. Withdrawal requires threshold signatures. Used in traditional bridges but avoided in this design due to centralization and key extraction risks. |
| **Nullifier** | A hash of the deposit secret, used to prevent double-spending. When a note is spent, its nullifier is recorded to prevent reuse. |
| **Commitment** | A cryptographic binding to a value. In this design: `C = H(secret, amount, bridge_address)`. |
| **Relayer** | An entity that broadcasts pre-authorized withdrawal transactions to the external chain on behalf of users, enabling user sovereignty without requiring users to hold ETH for gas. |

## Node Requirements for Bridge Operations

### User Node Types

The bridge is designed to work with **light clients**, not full nodes infrastructure:

| Operation | Required Node | Why |
|-----------|--------------|-----|
| **Deposit** | Light client or indexer | Only need Merkle proof of deposit, not full chain |
| **Withdraw** | DarkWow full node | ZK proof verification happens on DarkWow contract |
| **Monitor deposits** | Light wallet (view key) or indexer | For Monero/Zcash: can use view-key light clients |
| **Execute withdrawals** | Relayer service | External chain tx broadcast |

### Full Node vs Light Client

**Full nodes** (DarkWow validator, Ethereum geth) are needed for:
- Validating ZK proofs on DarkWow side
- Broadcasting withdrawal transactions to external chains (relayers)
- Tracking nullifier state to prevent double-spends

**Light clients** (or indexers) are sufficient for:
- Detecting deposits on external chains
- Generating Merkle proofs of deposit inclusion
- Verifying block confirmations (via SPV-style proofs)

### Practical Architecture

```
Deposit Flow (User Side):
┌──────────────────────────────────────────────────────────────┐
│ User needs:                                                   │
│   - Light client or indexer access to external chain        │
│   - NOT a full node                                          │
│                                                              │
│ Examples:                                                    │
│   - Ethereum: Infura/Alchemy RPC (light) or block explorer  │
│   - Monero: View key + remote node (no full sync needed)     │
│   - Zcash: lightwalletd or block explorer API                │
└──────────────────────────────────────────────────────────────┘

Withdraw Flow (User Side):
┌──────────────────────────────────────────────────────────────┐
│ User needs:                                                   │
│   - DarkWow full node access (to submit proofs)              │
│   - NOT required to run own node (can use RPC)               │
└──────────────────────────────────────────────────────────────┘

Relayer (Separate Service):
┌──────────────────────────────────────────────────────────────┐
│ Relayer needs:                                               │
│   - Full node on external chain (to broadcast withdrawals)   │
│   - DarkWow full node access (to observe withdrawal events)   │
└──────────────────────────────────────────────────────────────┘
```

### Indexer Dependency

Current implementation relies on an **external indexer** to provide:
- Merkle proofs for deposit verification
- Block header data for confirmation verification
- Deposit event monitoring and aggregation

**This is an architecture gap**: Trustless operation requires light client integration instead of trusting an indexer. See "Key Blockers" section.

### External Chain Requirements

| Chain | User Needs for Deposit | Relayer Needs for Withdraw |
|-------|----------------------|---------------------------|
| Ethereum | RPC to fetch Merkle proof (Infura/Alchemy) | Full geth/nethermind node |
| Monero | View key + remote node | Full monerod node |
| Zcash | lightwalletd or block explorer | Full zcashd node |
| Aztec | Rollup data availability | Aztec sequencer API |
| Litecoin | RPC + optional MWEB | Full litecoind node |

**Bottom line**: Users do **NOT** need to run full nodes for bridge deposits. They need:
1. Light client or RPC access to external chain (for Merkle proofs)
2. Access to DarkWow full node (for submitting proofs)

Relayers run the actual full nodes on external chains to execute withdrawals.

## Open Questions

1. **External chain finality**: How many confirmations before deposit is trustless?
2. **Relayer model**: Who broadcasts withdrawal transactions to external chain?
3. **Fee mechanism**: How are relayer fees paid anonymously?
4. **Deposit batching**: How to merge deposits for better privacy?
5. **Governance**: How to upgrade bridge without compromising security?

## MVP Status

**Partial MVP** — core deposit/withdraw structure exists, core ZK circuits verified.

| Circuit | Status | Opcode Safety | Notes |
|---------|--------|---------------|-------|
| `deposit_v1.zk` | **Verified** | ✅ Only proven opcodes | Uses real `merkle_root` opcode with `MerklePath` type |
| `withdraw_v1.zk` | **Verified** | ✅ Only proven opcodes | Uses `sparse_merkle_root` with `SparseMerklePath` type + `token_minimum` public input |

### Opcode Safety

`withdraw_v1.zk` uses ONLY proven opcodes:
- `poseidon_hash`, `ec_mul_base`, `ec_get_x/y` — standard operations
- `constrain_equal_base`, `range_check` — standard constraints
- `less_than_strict` — constrain-only comparison (sound, used for minimum amount check)

`deposit_v1.zk` uses ONLY proven opcodes:
- `poseidon_hash`, `ec_mul_base`, `ec_get_x/y` — standard operations
- `zero_cond` — sound conditional selection
- `merkle_root` — proper Merkle verification
- `constrain_equal_base`, `range_check` — standard constraints

No experimental grey-market opcodes (`LessThanOrEqual`, `IsEqualBase`, etc.) are used.

### Architecture Gaps (Not Opcode Issues)

1. **No external block header verification** — `external_block_hash` is accepted as public input but NOT verified against a real chain. A valid Merkle proof could be for a non-existent block.

2. **No light client integration** — Full verification requires tracking the external chain's header chain to prove block validity and finality.

3. **No deposit finality** — The circuit proves a deposit EXISTS, not that it's FINAL. Chain reorganizations could revert deposits after proof submission.

4. **No double-deposit prevention at bridge level** — Relies on external chain indexer not feeding the same Merkle proof twice.

### What It Needs

Light client integration to verify `external_block_hash` corresponds to a valid, finalized block on the external chain.

### Soundness Notes

See [Opcodes Reference](../../../doc/src/arch/opcodes.md) for opcode soundness verification. This contract uses only verified-sound opcodes.

**See also**:
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md) for the full cross-contract analysis
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for opcode implementation details

## References

- [Bridge Architecture Document](../../../doc/src/arch/bridge.md)
- [DarkWow SDK](../../../src/sdk/)
- [Halo 2 Documentation](https://halo2.dev/)
- [Object Capability Model](https://en.wikipedia.org/wiki/Object-capability_model)
- [Poseidon Hash](https://www.poseidon-hash.info/)
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md)
