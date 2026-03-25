# DarkFi Bridge Contract

Anonymous bridge contract for cross-chain asset transfers.

## Overview

The bridge contract enables privacy-preserving transfers between DarkFi and
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

DarkFi bridge uses **deterministic address derivation** instead of VSS:

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

| Aspect | VSS-Based Bridge | DarkFi OCap Bridge |
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
| WithdrawV1 | 0x02 | Claim withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge operators/threshold |

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

// 5. Submit to DarkFi bridge contract
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
    .build()?;

// 5. Submit to DarkFi bridge contract
client.submit(withdrawal).await?;

// 6. Relayer sees event, broadcasts ETH tx to Ethereum
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

### 3. Operation Ordering: Deposit Direction (External Chain → DarkFi)

```
Step 1: User computes bridge_address
        bridge_address = H(secret * G) using user's identity

Step 2: User deposits to bridge_address on external chain
        (This happens outside DarkFi, on Ethereum)

Step 3: Oracle/light client detects deposit
        - Verifies Merkle proof of inclusion
        - Verifies block has sufficient confirmations

Step 4: User submits DepositV1 to DarkFi bridge contract
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

### 4. Operation Ordering: Withdrawal Direction (DarkFi → External Chain)

```
Step 1: User computes nullifier
        nullifier = H(secret)

Step 2: User generates withdrawal ZK proof proving:
        a) Commitment is in deposit Merkle tree
        b) User knows secret for this commitment
        c) nullifier = H(secret)
        d) Amount is valid (<= deposited amount)
        e) Recipient hash matches

Step 3: User submits WithdrawV1 to DarkFi bridge contract

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
- Proof is verified by DarkFi contract (no oracle needed)
- Merkle root from block header commits to state

For withdrawal:
- No external verification needed
- DarkFi contract handles everything
- Relayer only broadcasts pre-authorized transaction

### 7. Consistency Guarantees

**What if external chain reorganizes?**

If a deposit's block is reorged out:
1. The deposit never existed on the canonical chain
2. The Merkle proof fails (root no longer matches)
3. Deposit registration fails → no funds minted

**What if withdrawal tx fails on external chain?**

Withdrawal is already recorded on DarkFi (nullifier spent).
User's funds are "gone" from DarkFi perspective.
Relayer can retry or user can submit direct tx.
(Trust model: relayer is trustless - withdrawal was pre-authorized)

**What if relayer censors withdrawal?**

User can broadcast directly to external chain.
Withdrawal was pre-authorized by ZK proof.
No threshold needed to release funds.

## Glossary

| Term | Definition |
|------|------------|
| **Pool** (UTXO) | In DarkFi's UTXO model, a "pool" refers to the collection of unspent transaction outputs (notes) held by the bridge contract. Unlike account-based systems where balances are stored at addresses, in UTXO systems, the pool tracks unspent outputs. When bridging assets, the bridge contract maintains a pool of notes representing deposited value. |
| **Note** | A UTXO representing a specific amount of value. In DarkFi, notes are encrypted commitments that can be spent by their owner using a zero-knowledge proof of knowledge of the secret. |
| **Asset Bridging** | The process of transferring value (tokens, coins) between chains. Requires liquidity on the destination chain and involves wrapping/unwrapping assets. Example: Wrapping ETH to create WETH on DarkFi. |
| **Data Bridging** | The process of passing arbitrary data (oracle data, state proofs, computations) between chains without value transfer. No liquidity required. Example: Passing a price feed from Ethereum to DarkFi. |
| **Merkle Inclusion Proof** | A cryptographic proof demonstrating that a specific element exists within a Merkle tree, without revealing all other elements. Used in the bridge to verify deposits exist on the external chain. |
| **OCap (Object Capability)** | A security model where access to objects is determined by capabilities (unforgeable references). In this bridge design, the "capability" is knowledge of the secret - no threshold signing needed. |
| **VSS (Verifiable Secret Sharing)** | A cryptographic scheme where a secret is split into shards distributed among participants. Withdrawal requires threshold signatures. Used in traditional bridges but avoided in this design due to centralization and key extraction risks. |
| **Nullifier** | A hash of the deposit secret, used to prevent double-spending. When a note is spent, its nullifier is recorded to prevent reuse. |
| **Commitment** | A cryptographic binding to a value. In this design: `C = H(secret, amount, bridge_address)`. |
| **Relayer** | An entity that broadcasts pre-authorized withdrawal transactions to the external chain on behalf of users, enabling user sovereignty without requiring users to hold ETH for gas. |

## Open Questions

1. **External chain finality**: How many confirmations before deposit is trustless?
2. **Relayer model**: Who broadcasts withdrawal transactions to external chain?
3. **Fee mechanism**: How are relayer fees paid anonymously?
4. **Deposit batching**: How to merge deposits for better privacy?
5. **Governance**: How to upgrade bridge without compromising security?

## References

- [Bridge Architecture Document](../../doc/src/arch/bridge.md)
- [DarkFi SDK](../../sdk/)
- [Halo 2 Documentation](https://halo2.dev/)
- [Object Capability Model](https://en.wikipedia.org/wiki/Object-capability_model)
- [Poseidon Hash](https://www.poseidon-hash.info/)
