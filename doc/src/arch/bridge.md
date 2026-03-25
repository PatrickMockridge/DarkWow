Anonymous Bridge (DRAFT v2)
==========================

*This document describes a proposed anonymous bridge design using Object
 Capability Security. This replaces the earlier VSS-based approach.*

## Abstract

We present an overview of an anonymous bridge from any blockchain network
that has tokens/balances on some address owned by a secret key. This is a
**universal bridge** for assets and arbitrary data, serving multiple use cases:
- **Value transfer**: Moving tokens between chains
- **Data bridging**: Passing arbitrary data (oracle data, state proofs, etc.)
- **Cross-chain computation**: Enabling contracts on one chain to verify state on another

Unlike traditional bridge designs that rely on Verifiable Secret Sharing (VSS),
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

### Clarification: UTXO vs Account Model

DarkFi is a **UTXO-based blockchain** (like Bitcoin), not account-based
(like Ethereum). This distinction is critical for bridge design:

| Model | State | How to "Prove Balance" |
|-------|-------|----------------------|
| Account (Ethereum) | Balances at addresses | Show signature proving you control address |
| UTXO (DarkFi) | Unspent transaction outputs | Prove output exists and is unspent |

In our bridge design:
- **On external chain**: User proves they sent funds (account or UTXO model)
- **On DarkFi**: User receives a UTXO (note) representing bridged value

The "bridging" happens via **ZK proofs of external chain state**,
not via a single transaction spanning both chains.

### Two Models for External Chain Interaction

**Model A: User Controls Deposit Address (Current Design)**

User derives a fresh address on the external chain. They deposit there,
then prove to DarkFi that they deposited. DarkFi releases notes from its pool.

```
1. User computes: bridge_address = H(user_identity, nonce)
2. User deposits ETH to this address (they control the private key)
3. User proves to DarkFi: "I deposited X ETH to this address"
4. DarkFi mints tokens to user
```

**Problem**: User can withdraw ETH from the address before step 3,
causing DarkFi to mint tokens without backing.

**Solution**: For Model A to work, the "deposit" must be irreversible.
This requires either:
- The address is a contract that cannot withdraw (one-way deposit)
- The user proves they burned/locked funds permanently

**Model B: Locked Deposit Contract**

This model uses a bridge contract that holds deposits and provides
Merkle inclusion proofs to secure the bridged asset:

```
1. User sends ETH to BridgeDeposit contract on Ethereum
2. Contract emits event with deposit details
3. User proves to DarkFi: "I locked X ETH in bridge contract"
4. Bridge provides Merkle inclusion proof securing the deposit
5. User receives note on DarkFi with verified backing
```

The Merkle inclusion proof demonstrates:
- The deposit exists in the bridge contract's state
- The bridge contract holds sufficient backing for the issued note
- The note's value is cryptographically linked to the deposited assets

This is similar to "wrapped asset" bridging (WBTC, WETH model), but with
ZK proofs providing cryptographic verification instead of trusted custodians.

### Current Design: Commitment-Based Deposit (Model B Refined)

The design uses **commitment-based deposits** where the external chain
deposit is verifiable but not directly controlled by user:

```
1. User computes secret S and commitment C = H(S, amount)
2. User deposits ETH + C to bridge deposit address on Ethereum
3. User (or oracle) submits ZK proof to DarkFi:
   - External deposit exists (Merkle proof of ETH tx)
   - Commitment C is correctly formed
   - User knows S
4. DarkFi verifies proof and provides Merkle inclusion proof
5. User receives note on DarkFi with verified backing
6. User can later prove they know S to withdraw (spend note on DarkFi)
```

**Key insight**: The "deposit" on Ethereum is NOT to an address the user
controls after the fact. The commitment `C` is revealed AT THE TIME of deposit,
binding the user to that deposit. The bridge provides Merkle inclusion proofs
that mathematically guarantee the security of the bridged asset.

### Bridge Address Derivation (For Withdrawal)

For withdrawal (DarkFi → External Chain), we use deterministic derivation:

```
withdrawal_address = H(user_external_pubkey, nonce)
```

User burns tokens on DarkFi, proves knowledge of secret, and receives
authorization to claim ETH at `withdrawal_address` on Ethereum.

### Deposit Flow (Refined)

```
1. User generates secret S
2. User computes commitment C = H(S, amount, bridge_nonce)
3. User deposits ETH + C to bridge deposit contract on Ethereum
4. Bridge oracle/indexer detects deposit, verifies:
   - ETH amount matches
   - Commitment C is included in deposit data
   - Sufficient confirmations
5. User submits DepositV1 to DarkFi with:
   - Commitment C
   - ZK proof: external deposit exists + commitment valid + knows S
6. DarkFi verifies proof, mints tokens
```

### Withdrawal Flow (Refined)

```
1. User wants to withdraw ETH from DarkFi to Ethereum
2. User computes nullifier N = H(S)
3. User burns tokens on DarkFi, revealing N
4. User generates ZK proof:
   - N is derived from known S
   - User knows S (controls the original deposit)
5. User submits WithdrawV1 to DarkFi
6. DarkFi verifies proof, records nullifier N as spent
7. Relayer sees withdrawal request, sends ETH to user's Ethereum address
```

**Note on Roles**:
- "User" is the same party on both sides - they control funds on both chains
- The ZK proof cryptographically links the two actions without trust
- No threshold signing needed because user alone authorizes via secret S

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

## Correctness, Security, and Operation Ordering

### Basic Bridge Criteria

| Criterion | How It's Satisfied |
|-----------|-------------------|
| **Funds are accounted for** | Every deposit creates a commitment in the Merkle tree. Every withdrawal nullifies a deposit. Arithmetic is verified in ZK. |
| **Operations are atomic** | Contract state changes happen in a single transaction. If proof verification fails, nothing is committed. |
| **No fund creation** | Withdrawals can only use deposited funds (proven via membership in deposit tree). |
| **No fund destruction** | Burned deposits emit nullifiers. Unspent deposits remain. |

### Security: Who Can Spend Bridged Funds?

**Deposit direction (External → DarkFi):**

The "bridging" happens via cryptographic proof, not via a single
cross-chain transaction. The user:
1. Locks ETH in a deposit contract on Ethereum (irreversible once confirmed)
2. Proves to DarkFi: "I locked X ETH" via ZK proof
3. DarkFi mints corresponding tokens

**Withdrawal direction (DarkFi → External):**

The user:
1. Burns tokens on DarkFi (irreversible)
2. Proves to Ethereum: "I burned X tokens" via ZK proof
3. Bridge contract on Ethereum releases ETH to user

**Key point**: The "bridge" is the cryptographic proof, not a single transaction.
Each chain independently verifies proofs. No threshold signing.

Bridge nodes cannot steal funds because they never see `secret`.

### Operation Ordering: Deposit (External → DarkFi)

```
1. User generates secret S
2. User computes commitment C = H(S, amount, nonce)
3. User deposits ETH + C to bridge deposit contract on Ethereum
4. Ethereum emits event with deposit details
5. Oracle/indexer detects deposit, verifies confirmations
6. User submits DepositV1 with commitment C + ZK proof proving:
   - Deposit exists on Ethereum (Merkle proof)
   - User knows S
   - Commitment C is correctly formed
7. DarkFi verifies proof
8. DarkFi mints tokens to user
```

**The "bridging" is step 6-8**: User proves to DarkFi that ETH was locked.
This is ZK verification, not a direct transaction.

**Why each step first:**
- Step 3 must precede 5: Cannot verify non-existent deposit
- Step 5 must precede 7: Cannot register unverified deposit
- Step 7 must precede 8: Cannot mint before verification

### Operation Ordering: Withdrawal (DarkFi → External)

```
1. User generates ZK proof of token burn:
   - Proves user owns tokens
   - Proves tokens are being burned
   - Computes nullifier N = H(S)
2. User submits WithdrawV1 to DarkFi
3. DarkFi verifies proof + nullifier not spent
4. DarkFi marks nullifier N as spent
5. User receives authorization proof
6. Relayer sees withdrawal request + proof
7. Relayer sends ETH to user's Ethereum address
```

**The "bridging" is step 2-5**: User proves to DarkFi that tokens were burned.
DarkFi records this, and the proof serves as authorization for step 7.

**Why each step first:**
- Step 1 must precede 2: Cannot submit without proof
- Step 3 must precede 4: Cannot spend before verification
- Step 4 must precede 6: Cannot release funds before state update

### Trustless Verification

Traditional bridges require trusted oracles. This design uses:

- **ZK proofs**: User proves deposit existence without revealing which deposit
- **Merkle trees**: Efficient proof of inclusion
- **Light client headers**: Trustless state verification (or trusted indexer for v1)

For deposits: User's ZK proof includes Merkle proof against external chain state root.
For withdrawals: DarkFi contract handles verification locally.

### Consistency Guarantees

**External chain reorg:**
- Deposit's block reorged → Merkle proof fails → Deposit rejected

**Withdrawal tx fails on external chain:**
- Withdrawal already recorded on DarkFi (nullifier spent)
- Relayer retries or user broadcasts directly

**Relayer censorship:**
- User can broadcast directly
- Withdrawal was pre-authorized by ZK proof

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

## Glossary

| Term | Definition |
|------|------------|
| **Pool** (UTXO) | In DarkFi's UTXO model, a "pool" refers to the collection of unspent transaction outputs (notes) held by the bridge contract. Unlike account-based systems where balances are stored at addresses, in UTXO systems, the pool tracks unspent outputs. When bridging assets, the bridge contract maintains a pool of notes representing deposited value. |
| **Note** | A UTXO representing a specific amount of value. In DarkFi, notes are encrypted commitments that can be spent by their owner using a zero-knowledge proof of knowledge of the secret. |
| **Asset Bridging** | The process of transferring value (tokens, coins) between chains. Requires liquidity on the destination chain and involves wrapping/unwraping assets. Example: Wrapping ETH to create WETH on DarkFi. |
| **Data Bridging** | The process of passing arbitrary data (oracle data, state proofs, computations) between chains without value transfer. No liquidity required. Example: Passing a price feed from Ethereum to DarkFi. |
| **Merkle Inclusion Proof** | A cryptographic proof demonstrating that a specific element exists within a Merkle tree, without revealing all other elements. Used in the bridge to verify deposits exist on the external chain. |
| **OCap (Object Capability)** | A security model where access to objects is determined by capabilities (unforgeable references). In this bridge design, the "capability" is knowledge of the secret - no threshold signing needed. |
| **VSS (Verifiable Secret Sharing)** | A cryptographic scheme where a secret is split into shards distributed among participants. Withdrawal requires threshold signatures. Used in traditional bridges but avoided in this design due to centralization and key extraction risks. |
| **Nullifier** | A hash of the deposit secret, used to prevent double-spending. When a note is spent, its nullifier is recorded to prevent reuse. |
| **Commitment** | A cryptographic binding to a value. In this design: `C = H(secret, amount, bridge_address)`. |
| **Relayer** | An entity that broadcasts pre-authorized withdrawal transactions to the external chain on behalf of users, enabling user sovereignty without requiring users to hold ETH for gas. |

## References

[^1]: <https://en.wikipedia.org/wiki/Commitment_scheme>

[^2]: <https://en.wikipedia.org/wiki/Merkle_tree>

[^3]: <https://www.poseidon-hash.info/>

[^4]: <https://en.wikipedia.org/wiki/Zero-knowledge_proof>

- Bridge Contract: `src/contract/bridge/`
- Object Capability Model: <https://en.wikipedia.org/wiki/Object-capability_model>
