# DarkWow for Dummies

*This document provides an accessible introduction to DarkWow's architecture
 and cryptographic primitives. For a deeper technical dive, see
 [Start Here](../start-here.md) and the [Architecture Overview](../arch/overview.md).*

## What is DarkWow?

DarkWow is a **Layer 1 blockchain** designed around **anonymous by default**
operation. All transactions, smart contract executions, and application state
are private. Unlike pseudonymous chains (Bitcoin, Ethereum), where addresses
and amounts are publicly visible, DarkWow uses zero-knowledge proofs to
verify correctness without revealing any information.

## Core Architecture

### Blockchain

DarkWow operates as a **Proof of Work** chain using the **RandomX** algorithm.
Miners compete to find valid blocks through computational lottery. The chain
extends through consensus: nodes agree on canonical blocks, forming an
immutable, ordered transaction ledger.

### P2P Network

DarkWow nodes communicate over a **peer-to-peer network** with support for:

- **TCP/TLS**: Standard internet transport
- **Tor**: Network-layer anonymity (onion routing)
- **I2P**: Network-level anonymity via external I2P router (SOCKS5 proxy)
- **QUIC**: Low-latency encrypted transport

This means DarkWow can operate as a fully anonymous network, invisible to
network observers.

## Cryptographic Primitives

DarkWow's privacy stems from combining several primitives:

### Poseidon Hash-Based Commitments

A **commitment** hides a value while binding the committer to it.

```
C = PoseidonHash(value, nonce)
```

You can reveal `value` later and prove `C` was computed with it, but
observing `C` reveals nothing about `value`.

**In DarkWow**: Coin commitments are Poseidon hashes that bind together all
of a coin's attributes — its public key, value, token type, spend conditions,
and user data — with a random blinding factor that keeps them hidden.

### Merkle Trees

A **Merkle tree** efficiently proves set membership without revealing
other set elements.

**In DarkWow**: Coins are leaves in a Merkle tree. You prove a coin exists
without revealing which coin, breaking the link between old and new tokens.

### Nullifiers

A **nullifier** is a unique hash derived from a secret. When you spend a
coin, you reveal its nullifier. The network checks if it's been used before,
preventing double-spend - without revealing which coin was spent.

**In DarkWow**: Every coin has a nullifier. Spending reveals the nullifier,
not the coin's identity.

### Coin Lifecycle

The core privacy mechanism:

1. **Coin Creation**: New coins enter circulation as block rewards for miners
   (not via user-initiated minting). Each coin has a secret spending key and
   a commitment binding all its attributes.
2. **Transfer**: Spend an old coin by revealing its nullifier
   (`poseidon_hash(secret_key, coin_hash)`), and create new coins with fresh
   secrets. The ZK proof hides which old coin was spent and which new coins
   were created — no link between them.
3. **Burn**: Destroy a coin by revealing its nullifier along with verification
   data, removing value from circulation.

This breaks the transaction graph — coins cannot be traced.

## Zero-Knowledge Proofs

Zero-knowledge proofs (ZKPs) allow **verifying computation correctness without
revealing inputs**.

DarkWow uses **Halo 2**, a recursive proof system with:

- **No trusted setup**: Unlike Groth16, no toxic waste parameters
- **Trustless verification**: Anyone can verify proofs with just public data
- **Proof recursion**: Efficient proofs via proof composition

### zkVM

DarkWow's **zero-knowledge virtual machine** executes smart contracts and
produces ZK proofs. The zkVM:

- Loads bytecode compiled by **zkas**
- Executes contract logic with formal verification
- Outputs a proof that computation was done correctly

### zkas

**zkas** is DarkWow's assembly language for ZK circuits. It provides:

```
circuit deposit(prover: Witness) {
    secret: Scalar = prover.witness("secret");
    nullifier: Scalar = hash(secret, nonce);
    ...
}
```

This compiles to Halo 2 circuits. Developers write contracts in zkas without
needing to understand field arithmetic or elliptic curves.

## Application Layer

### Smart Contracts

DarkWow contracts execute on the zkVM. Contract execution:

1. **exec()**: Read-only phase — compute state changes
2. **apply()**: Write phase — apply state changes

(At the WASM level, these are `__entrypoint` and `__update`; the runtime
exposes them as `exec()` and `apply()`.)

This separation prevents re-entrancy attacks, a common vulnerability in
Ethereum contracts.

### Contracts

DarkWow includes 32 smart contracts — 9 deployed at genesis and 23
deployed post-genesis via Deployooor. See [Contracts](contracts.md) for the
full catalog covering DeFi, Gaming, DAO, Identity, Markets, and Infrastructure
contracts.

### P2P Messaging

For applications requiring message persistence (chat, feeds), DarkWow uses its
**P2P network** with multi-transport support (TCP/TLS, Tor, I2P). Messages
propagate through the peer-to-peer network with eventual consistency.

DarkIRC uses this for censorship-resistant messaging.

### Rate-Limit Nullifiers (RLN)

Spam prevention without centralization:

- Users register an RLN identity with a secret key
- Each message reveals a nullifier unique to that epoch and a Shamir secret share
- Double-posting within the same epoch reveals two shares, allowing the network
  to recover the user's secret key and add them to a ban list — no central
  moderator needed

RLN balances spam prevention with free-tier access.

## History of Primitives

*These dates reflect the broader ecosystem's evolution. DarkWow's implementation
timeline may differ.*

| Period | Primitive | Purpose |
|--------|-----------|---------|
| 2018-2019 | Sapling scheme | Private payments via commitments + nullifiers |
| 2020-2021 | zkVM | Programmable ZK contracts |
| 2021-2022 | zkas | Accessible ZK contract development |
| 2021-2022 | Event Graph | Censorship-resistant messaging |
| 2022-2023 | RLN | Decentralized spam prevention |
| 2023 | Anonymous DAO | Private governance and treasuries |
| 2024+ | Bridge Protocol (partial MVP) | Cross-chain anonymity |

## Terminology Reference

| Term | Definition |
|------|------------|
| **Commitment** | Cryptographic binding to a value without revealing it |
| **Nullifier** | Unique hash preventing double-spend |
| **zkVM** | Virtual machine producing ZK proofs for contract execution |
| **zkas** | Assembly language for ZK circuits |
| **Note/Coin** | Private token with secret and commitment |
| **Witness** | Private inputs to a ZK proof |
| **Prover** | Party generating a ZK proof |
| **Verifier** | Party checking a ZK proof |
| **P2P Network** | Multi-transport peer-to-peer communication |
| **RLN** | Rate-limiting via nullifier slashing |

## Further Reading

- [Architecture Overview](../arch/overview.md) - Technical architecture
- [Zero-Knowledge Explainer](../crypto/zk_explainer.md) - ZK fundamentals
- [Start Here](../start-here.md) - Development setup
- [Philosophy](../philosophy/philosophy.md) - Design motivations
