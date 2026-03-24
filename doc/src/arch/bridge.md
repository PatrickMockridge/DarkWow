Anonymous Bridge (DRAFT v2)
==========================

*This document describes a proposed anonymous bridge design using Object
 Capability Security. This replaces the earlier VSS-based approach.*

## Abstract

We present an overview of an anonymous bridge from any blockchain network
that has tokens/balances on some address owned by a secret key. Unlike
traditional bridge designs that rely on Verifiable Secret Sharing (VSS),
this design uses **Object Capability Security** (OCap) to achieve:

- No VSS key shards that can be stolen
- Fast, self-sovereign withdrawals (no threshold signing)
- Censorship-resistant operation
- Fresh addresses per deposit for temporal privacy

## Motivation: Why Not VSS?

Traditional bridge designs use VSS where bridge nodes collectively hold
secret shards. Withdrawal requires threshold signing from n-of-m nodes.

**Problems with VSS-based bridges:**

1. **VSS Node Compromise**: Any t of n nodes can reconstruct the secret
   and steal all funds. An attacker compromising enough nodes = catastrophic loss.

2. **Centralization**: Threshold nodes can collectively censor withdrawals.
   They can refuse to sign, effectively freezing the bridge.

3. **Complexity**: Distributed Key Generation (DKG), honest-majority
   assumptions, liveness requirements, and slashable conditions.

4. **Slow**: Each withdrawal requires a threshold signing round.
   Users wait for enough nodes to respond.

5. **Oracle Dependency**: Deposit verification typically requires trusted
   oracles, creating centralization and attack surfaces.

## Object Capability Security Model

Instead of VSS, we use **deterministic address derivation** from user
identity. The bridge never holds secrets - users alone control their funds.

### Core Principles

1. **User-Held Secrets**: Bridge address derived from user's identity,
   not from shared VSS shards.

2. **Self-Sovereign Withdrawal**: Users authorize withdrawals via their
   own secret. No threshold coordination required.

3. **Deterministic Addresses**: Bridge addresses are computed, not generated
   through multi-party protocols.

4. **ZK Proofs for Privacy**: Zero-knowledge proofs verify all operations
   without revealing secrets.

## Preliminaries

**Pedersen Commitment**[^1]

A commitment hides a value while binding the committer to it.

```
C = g^value * h^nonce
```

**Merkle Trees**[^2]

Efficient proof of set membership without revealing other elements.

**Poseidon Hash**[^3]

A ZK-friendly hash function using elements from the Pasta curves.
Used for all hash operations in circuits.

**Zero-Knowledge Proofs**[^4]

Proving knowledge of a statement without revealing the witness.

## Design

### Bridge Address Derivation

Instead of VSS-generated addresses, bridge addresses are derived
deterministically from user identity:

```
bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, bridge_nonce)
bridge_pub = bridge_secret * G
bridge_address = poseidon_hash(ec_get_x(bridge_pub), ec_get_y(bridge_pub))
```

Where:
- `recipient_pub` is the user's public key on the external chain
- `bridge_nonce` ensures fresh address per deposit (temporal privacy)
- `G` is the generator point (NULLIFIER_K)

**Security Properties:**
- Fresh address per deposit (nonce ensures unlinkability)
- Only user knows bridge_secret (no VSS shards to steal)
- Bridge cannot derive user's address without user identity + nonce

### Deposit Flow

```
1. Alice computes bridge_address from her identity + nonce
2. Alice deposits to bridge_address on external chain
3. Oracle/light client verifies deposit exists
4. Alice registers deposit on DarkFi with commitment:
   commitment = poseidon_hash(secret, amount, bridge_address)
5. Deposit is recorded in bridge's Merkle tree
```

The commitment proves Alice knows `secret`. Only someone knowing `secret`
can later withdraw.

### Withdrawal Flow

```
1. Alice computes nullifier = poseidon_hash(secret)
2. Alice generates ZK proof demonstrating:
   - Knowledge of secret corresponding to a deposit
   - Deposit exists in bridge's Merkle tree (without revealing which one)
   - Amount is valid
   - Recipient hash matches
3. Bridge verifies ZK proof
4. Bridge checks nullifier hasn't been spent
5. Bridge marks nullifier as spent, records withdrawal
```

**No threshold signing required!** Alice alone authorizes via her secret.

### Double-Spend Prevention

```
nullifier = poseidon_hash(secret)
```

When Alice withdraws, she reveals `nullifier` but not `secret`. The
bridge tracks spent nullifiers. Revealing nullifier doesn't compromise
the deposit (secret is still needed to claim).

## Comparison: VSS vs Object Capability

| Aspect | VSS-Based Bridge | OCap Bridge (This Design) |
|--------|------------------|---------------------------|
| Key custody | Distributed shards | User-held secrets |
| Withdrawal | Threshold signing | Self-signed (ZK proof) |
| Speed | Slow (round) | Fast (instant) |
| Node compromise | Catastrophic | Impossible |
| Censorship | Threshold can block | Cannot block |
| Complexity | High (DKG) | Low (hashing) |
| Oracle dependency | High | Low (ZK proofs) |

## Security Analysis

### Threat: VSS Node Compromise

**VSS**: t of n nodes compromised → secret reconstructed → all funds stolen

**OCap**: Bridge nodes don't hold secrets. Compromising all bridge nodes
yields nothing - only user knows their bridge_secret.

### Threat: Withdrawal Censorship

**VSS**: Threshold nodes refuse to sign → withdrawal impossible

**OCap**: No threshold. User self-signs via ZK proof. Bridge must
verify if proof is valid.

### Threat: Deposit Linkability

**VSS**: Fresh address per deposit, but VSS operations leave traces

**OCap**: Fresh address via nonce. User identity can be hidden via
external chain privacy (zkip, mixers, etc.).

### Threat: Double-Spend

**VSS**: VSS signing prevents double-spend

**OCap**: Nullifier tree tracks spent deposits. ZK proof proves
deposit exists without revealing which one.

## Implementation

See `src/contract/bridge/` for the draft contract implementation.

### Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize bridge state |
| DepositV1 | 0x01 | Register external chain deposit |
| WithdrawV1 | 0x02 | Claim withdrawal to external chain |
| UpdateConfigV1 | 0x03 | Update bridge configuration |

### ZK Circuits

**deposit_v1.zk**: Proves deposit exists and commitment is valid

**withdraw_v1.zk**: Proves deposit membership and withdrawal authorization

### Data Structures

```rust
DepositParams {
    commitment: [u8; 32],      // H(secret, amount, bridge_address)
    recipient_pub_x: [u8; 32],  // For address derivation
    recipient_pub_y: [u8; 32],
    bridge_nonce: u64,          // Fresh address per deposit
    chain: ExternalChain,
    external_block_hash: [u8; 32],
    merkle_proof: Vec<[u8; 32]>,
    external_state_root: [u8; 32],
    fee: u64,
    proof: Vec<u8>,
}

WithdrawParams {
    nullifier: [u8; 32],        // H(secret)
    recipient_hash: [u8; 32],   // H(recipient) for privacy
    amount: u64,
    proof: Vec<u8>,
    fee: u64,
}
```

## Open Questions

* **External chain finality**: How many confirmations before deposit is trusted?

* **Light client vs Oracle**: Trustless verification vs trusted oracle for
  deposit confirmation. Initial version may use oracle for simplicity.

* **Relayer model**: Who broadcasts withdrawal transactions to external chain?
  Relayer fees? Privacy-preserving fee payment?

* **Deposit batching**: How to merge deposits for better anonymity set?

* **Governance**: How to upgrade bridge parameters without compromising
  security model?

* **Deposit to external chain**: How does user send funds anonymously to
  derived bridge address? External chain privacy required.

## References

[^1]: <https://en.wikipedia.org/wiki/Commitment_scheme>

[^2]: <https://en.wikipedia.org/wiki/Merkle_tree>

[^3]: <https://www.poseidon-hash.info/>

[^4]: <https://en.wikipedia.org/wiki/Zero-knowledge_proof>

- Bridge Contract: `src/contract/bridge/`
- Object Capability Model: <https://en.wikipedia.org/wiki/Object-capability_model>
