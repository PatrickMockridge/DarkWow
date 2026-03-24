Anonymous Bridge (DRAFT)
========================

*This document is a draft proposal for an anonymous bridge design.
 Feedback and iteration are welcome.*

We present an overview of a possibility to develop anonymous bridges
from any blockchain network that has tokens/balances on some address
owned by a secret key. Usually in networks, we have a secret key which
we use to derive a public key (address) and use this address to receive
funds. In this overview, we'll go through such an operation on the
Ethereum network and see how we can bridge funds from ETH to DarkFi.

## Preliminaries

**Verifiable secret sharing**[^1]

Verifiable secret sharing ensures that even if the dealer is malicious
there is a well-defined secret that the players can later reconstruct.
VSS is defined as a secure multi-party protocol for computing the
randomized functionality corresponding to some secret sharing scheme.

**Secure multiparty computation**[^2]

Multiparty computation is typically accomplished by making secret
shares of the inputs, and manipulating the shares to compute some
function. To handle "active" adversaries (that is, adversaries that
corrupt nodes and make them deviate from the protocol), the secret
sharing scheme needs to be verifiable to prevent the deviating nodes
from throwing off the protocol.

**Object Capability Security**

Object capability (ocap) security is a least-privilege security model
where references to objects serve as capabilities. A process can only
interact with an object if it holds a reference (capability) to it.
This principle can be applied to bridge design by ensuring:

1. Bridge nodes hold scoped capabilities to specific functions (e.g., signing)
2. Capabilities are compartmented—each bridge operation uses minimal privileges
3. Revocation is possible by invalidating capability references

**Reference**: [Object Capabilities for Security](https://en.wikipedia.org/wiki/Object-capability_model)

## Problem Space

### Trustless Bridge Requirements

1. **Privacy**: Bridge deposits must not be linkable to withdrawals
2. **Censorship Resistance**: No single party can block valid withdrawals
3. **Liveness**: Bridge must function even if some nodes are adversarial
4. **Fungibility**: All bridge funds must be interchangeable

### Threat Model

- **External adversary**: Observes all blockchain state, attempts to de-anonymize
- **Active adversary**: Controls up to t of n bridge nodes, attempts theft
- **Collusion**: Adversaries may coordinate, but cannot break the VSS threshold

### Existing Approaches and Limitations

| Approach | Privacy | Trustlessness | Limitations |
|----------|---------|---------------|-------------|
| Multisig bridges | None | Requires trusted parties | Centralization, surveillance |
| zkBridge | Strong | Moderate | Heavy proof generation |
| Light clients | Strong | Strong | High verification cost |

## Architecture Overview

### State Proofs (Preliminary Implementation)

DarkFi has begun preliminary work on bridging infrastructure by adding
state proofs to the block structure. This allows external chains to
commit to DarkFi state at a given height, enabling light-client style
verification without full node trust.

```
StateProof {
    header_hash: HeaderHash,
    block_height: u32,
    state_root: StateHash,
    signature: BLS signature
}
```

This infrastructure can be extended to prove:
- Existence of a specific note/UTXO
- Non-existence of a spent note
- Contract state at a given height

**Reference**: See `src/blockchain/` for header and block store implementations.

### Bridge Node Network

```
┌─────────────────────────────────────────────────────────────┐
│                     Bridge Node Network                       │
│                                                              │
│  ┌─────────┐   ┌─────────┐   ┌─────────┐                   │
│  │ Node 1  │   │ Node 2  │   │ Node 3  │   ...            │
│  │ (VSS)   │   │ (VSS)   │   │ (VSS)   │                   │
│  └────┬────┘   └────┬────┘   └────┬────┘                   │
│       │              │              │                         │
│       └──────────────┼──────────────┘                         │
│                      │                                        │
│              ┌───────┴───────┐                                │
│              │  Threshold    │                                │
│              │  Signing      │                                │
│              └───────┬───────┘                                │
└──────────────────────┼────────────────────────────────────────┘
                       │
                       ▼
         ┌─────────────────────────────┐
         │   Derived Ethereum Key      │
         │   (new address per bridge)  │
         └─────────────────────────────┘
```

## General Bridge Flow

Assume Alice wants to bridge 10 ETH from the Ethereum network into
DarkFi. Alice would issue a bridging request and perform a VSS scheme
with a network of nodes in order to create an Ethereum secret key,
and with it - derive an Ethereum address. Using such a scheme should
prevent any single party to retrieve the secret key and steal funds.
This also means, for every bridging operation, a fresh and unused
Ethereum address is generated and as such gives no convenient ways
of tracing bridge deposits.

Once the new address has been generated, Alice can now send funds
to the address and either create some proof of deposit, or there can
be an oracle that verifies the state on Ethereum in order to confirm
that the funds have actually been sent.

Once confirmed, the bridging smart contract is able to freshly mint
the counterpart of the deposited funds on a DarkFi address of Alice's
choice.

## Object Capability Security Model

### Capability Scoping

Each bridge node operates with minimal privileges:

```rust
// Capability-limited bridge session
trait BridgeCapability {
    // Can only sign transactions for derived addresses
    fn can_sign_for_address(&self, addr: &EthAddress) -> bool;
    // Can only initiate withdrawals up to a limit
    fn withdrawal_limit(&self) -> u64;
    // Can verify proofs but not forge them
    fn verify_proof(&self, proof: &ZkProof) -> bool;
}
```

### Compartmentalized Operations

1. **Key Generation**: Nodes hold `KeyGenCapability` - can participate in VSS
2. **Signing**: Nodes hold `SigningCapability` - can sign for specific addresses only
3. **Verification**: Any node holds `VerifyCapability` - can verify proofs
4. **Minting**: Bridge contract holds `MintCapability` - can create tokens

### Revocation

If a node is suspected compromised, its capabilities can be revoked:
- Remove its VSS share from active sessions
- Invalidate its signing capability for future addresses
- Existing derived addresses remain secure (threshold remains t-of-n)

## Provisional ZKAS Primitives

DarkFi's ZK proving system (zkas) can provide privacy-preserving proofs
for bridge operations.

### ZKAS Binary Structure

```
ZkBinary {
    namespace: String,
    k: u32,                    // Circuit complexity parameter
    constants: Vec<(VarType, String)>,
    literals: Vec<(LitType, String)>,
    witnesses: Vec<VarType>,
    opcodes: Vec<OpCode>
}
```

### Bridge-Relevant ZK Primitives

**Deposit Proof** (`deposit_v1.zk`):
```
// Prove: I know a secret x such that H(x) = commitment
// Public: commitment, amount, recipient
circuit deposit(prover: Witness) {
    secret: Scalar = prover.witness("secret");
    commitment: Scalar = pedersen_commit(secret, amount);

    // Verify commitment matches on-chain
    verify_equal(commitment, public_commitment);
}
```

**Withdrawal Proof** (`withdraw_v1.zk`):
```
// Prove: I can spend note N without revealing it
// Public: nullifier, root, recipient
circuit withdraw(prover: Witness) {
    secret: Scalar = prover.witness("secret");
    nullifier: Scalar = hash(secret, nonce);

    // Merkle membership proof
    path: MerklePath = prover.witness("merkle_path");
    leaf: Scalar = pedersen_commit(secret, amount);
    root: Scalar = path.verify(leaf);

    verify_equal(nullifier, public_nullifier);
    verify_equal(root, public_root);
}
```

**Range Proof** (`range_proof.zk`):
```
// Prove: value is in [0, 2^64) without revealing exact amount
circuit range_proof(prover: Witness) {
    value: Scalar = prover.witness("value");
    bits: [Uint32; 64] = decompose(value);

    // Each bit is proven in range [0,1]
    for bit in bits {
        assert(bit * (bit - 1) == 0);
    }
}
```

### Variable Types for Bridge Circuits

| Type | Encoding | Use Case |
|------|----------|----------|
| `Scalar` | Fp element | Secrets, hashes |
| `EcPoint` | JubJub point | Public keys, commitments |
| `MerklePath` | 32-element path | Membership proofs |
| `Uint64` | 64-bit integer | Amounts, nonces |

## WASM Primitives for Bridge Contracts

DarkFi contracts run as WASM binaries. Bridge contracts can use these primitives:

### Contract Entrypoint

```rust
// src/sdk/src/wasm/entrypoint.rs
define_contract! {
    init: bridge_init,
    exec: bridge_exec,
    apply: bridge_apply,
    metadata: bridge_metadata
}
```

### Available WASM Primitives

| Module | Primitives | Purpose |
|--------|------------|---------|
| `db` | `put`, `get`, `delete` | State storage |
| `merkle` | `insert_leaf`, `get_root`, `verify` | Merkle tree operations |
| `smt` | `insert`, `get`, `verify` | Sparse Merkle trees |
| `util` | `hash`, `verify_signature` | Cryptographic utilities |

### Bridge Contract Interface

```rust
// Provisional bridge contract API
#[no_mangle]
pub unsafe extern "C" fn __initialize(input: *mut u8) -> i64 {
    // Initialize bridge state
}

#[no_mangle]
pub unsafe extern "C" fn __entrypoint(input: *mut u8) -> i64 {
    // Execute bridge operations:
    // - verify_deposit(proof, amount)
    // - initiate_withdrawal(addr, amount, proof)
    // - finalize_withdrawal(tx_hash)
}

#[no_mangle]
pub unsafe extern "C" fn __update(input: *mut u8) -> i64 {
    // Apply state updates from verified transactions
}
```

## State Proof Integration

The preliminary work on state proofs enables bridge verification without
full DarkFi nodes:

```rust
// State proof structure
struct StateProof {
    header_hash: [u8; 32],
    block_height: u32,
    state_commitment: [u8; 32],
    validator_signatures: Vec<BLSSignature>,
}

// Prove a note exists in DarkFi state
fn prove_note_existence(
    note: &Note,
    proof: &StateProof
) -> bool {
    // 1. Verify proof.header_hash commits to proof.block_height
    // 2. Verify validator signatures (BLS threshold)
    // 3. Verify note inclusion in state_root merkle tree
    verify_inclusion(proof, note.commitment())
}
```

## Open Questions

* **What to do with the deposited funds?**

It is possible to send them to some pool or smart contract on ETH,
but this becomes an address that can be blacklisted as adversaries can
assume it is the bridge's funds. Alternatively, it could be sent into
an L2 such as Aztec in order to anonymise the funds, but (for now)
this also limits the variety of tokens that can be bridged (ETH & DAI).

* **How to handle network fees?**

In the case where the token being bridged cannot be used to pay network
fees (e.g. bridging DAI from ETH), there needs to be a way to cover
the transaction costs. The bridge nodes could fund this themselves
but then there also needs to be some protection mechanism to avoid
people being able to drain those wallets from their ETH.

* **What is the threshold (t-of-n) for VSS?**

A higher threshold increases security but reduces liveness.
Recommendation: 3-of-5 for initial deployment, adjustable via governance.

* **How to handle reorgs and chain reorganizations?**

Bridge deposits should wait for sufficient confirmations before minting.
State proofs must account for chain reorganizations via checkpointing.

* **Integration with existing ZK circuits?**

Existing DarkFi circuits (transfer, mint, burn) may need extension
or new wrapper circuits for bridge-specific operations.

## Implementation Roadmap (Provisional)

1. **Phase 1**: State proof infrastructure (in progress)
   - Add state commitment to block headers
   - Implement BLS threshold signatures
   - Light client verification library

2. **Phase 2**: VSS protocol implementation
   - Implement Feldman/VSS scheme
   - Threshold key generation
   - Distributed signing

3. **Phase 3**: Bridge contract
   - WASM contract with deposit/withdrawal logic
   - Integration with zkas for deposit proofs
   - Merkle tree for deposit tracking

4. **Phase 4**: Anonymous withdrawal
   - Nullifier-based withdrawal
   - ZK proof generation (client-side)
   - TOR integration for metadata hiding

## References

[^1]: <https://en.wikipedia.org/wiki/Verifiable_secret_sharing>

[^2]: <https://en.wikipedia.org/wiki/Secure_multiparty_computation>

- DarkFi SDK: `src/sdk/`
- ZKAS VM: `src/zkas/`, `src/zk/`
- Blockchain storage: `src/blockchain/`
- BridgeTree: `src/serial/src/types/bridgetree.rs`
- Object Capability Model: <https://en.wikipedia.org/wiki/Object-capability_model>
