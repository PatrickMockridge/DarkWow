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
│   ├── client/     # Client-side transaction builders
│   ├── entrypoint/ # WASM contract entrypoint
│   ├── model/      # Data structures for contract calls
│   └── lib.rs      # Contract definitions and constants
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

This contract is a **draft/placeholder**. The following items need implementation:

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
