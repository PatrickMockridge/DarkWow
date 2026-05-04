# DarkFi for Dummies

*This document provides an accessible introduction to DarkFi's architecture
 and cryptographic primitives. For a deeper technical dive, see
 [Start Here](../start-here.md) and the [Architecture Overview](../arch/overview.md).*

## What is DarkFi?

DarkFi is a **Layer 1 blockchain** designed around **anonymous by default**
operation. All transactions, smart contract executions, and application state
are private. Unlike pseudonymous chains (Bitcoin, Ethereum), where addresses
and amounts are publicly visible, DarkFi uses zero-knowledge proofs to
verify correctness without revealing any information.

## Core Architecture

### Blockchain

DarkFi operates as a **Proof of Work** chain using the **RandomX** algorithm.
Miners compete to find valid blocks through computational lottery. The chain
extends through consensus: nodes agree on canonical blocks, forming an
immutable, ordered transaction ledger.

### P2P Network

DarkFi nodes communicate over a **peer-to-peer network** with support for:

- **TCP/TLS**: Standard internet transport
- **Tor**: Network-layer anonymity (onion routing)
- **I2P**: Network-level anonymity (garlic routing)

This means DarkFi can operate as a fully anonymous network, invisible to
network observers.

## Cryptographic Primitives

DarkFi's privacy stems from combining several primitives:

### Poseidon Hash-Based Commitments

A **commitment** hides a value while binding the committer to it.

```
C = PoseidonHash(value, nonce)
```

You can reveal `value` later and prove `C` was computed with it, but
observing `C` reveals nothing about `value`.

**In DarkFi**: Poseidon-based commitments hide token amounts in transactions.
DarkFi uses the Poseidon hash function throughout — no Pedersen commitments,
no elliptic curve arithmetic in circuits.

### Merkle Trees

A **Merkle tree** efficiently proves set membership without revealing
other set elements.

**In DarkFi**: Coins are leaves in a Merkle tree. You prove a coin exists
without revealing which coin, breaking the link between old and new tokens.

### Nullifiers

A **nullifier** is a unique hash derived from a secret. When you spend a
coin, you reveal its nullifier. The network checks if it's been used before,
preventing double-spend - without revealing which coin was spent.

**In DarkFi**: Every coin has a nullifier. Spending reveals the nullifier,
not the coin's identity.

### Mint-Burn Scheme

The core privacy mechanism:

1. **Mint**: Create a coin with secret `s`. Publish commitment `C(s, nonce)`.
2. **Transfer**: Spend old coin (reveal nullifier `H(s)`), create new coin with
   new secret `s'`. No link between old and new coin.
3. **Burn**: Destroy a coin by revealing its nullifier.

This breaks the transaction graph - coins cannot be traced.

## Zero-Knowledge Proofs

Zero-knowledge proofs (ZKPs) allow **verifying computation correctness without
revealing inputs**.

DarkFi uses **Halo 2**, a recursive proof system with:

- **No trusted setup**: Unlike Groth16, no toxic waste parameters
- **Trustless verification**: Anyone can verify proofs with just public data
- **Proof recursion**: Efficient proofs via proof composition

### zkVM

DarkFi's **zero-knowledge virtual machine** executes smart contracts and
produces ZK proofs. The zkVM:

- Loads bytecode compiled by **zkas**
- Executes contract logic with formal verification
- Outputs a proof that computation was done correctly

### zkas

**zkas** is DarkFi's assembly language for ZK circuits. It provides:

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

DarkFi contracts execute on the zkVM. Contract execution:

1. **exec()**: Read-only phase - compute state changes
2. **apply()**: Write phase - apply state changes

This separation prevents re-entrancy attacks, a common vulnerability in
Ethereum contracts.

### Native Contracts

DarkFi includes several pre-built contracts:

| Contract | Function |
|----------|----------|
| **NativeToken** | Consensus-first native token for block rewards and fees |
| **MoneyV3** | DeFi token contract with hidden token IDs |
| **DAO Escrow** | Anonymous voting, hidden treasuries |
| **Deployooor** | Deploy custom WASM contracts |

### P2P Messaging

For applications requiring message persistence (chat, feeds), DarkFi uses its
**P2P network** with multi-transport support (TCP/TLS, Tor, I2P). Messages
propagate through the peer-to-peer network with eventual consistency.

DarkIRC uses this for censorship-resistant messaging.

### Rate-Limit Nullifiers (RLN)

Spam prevention without centralization:

- Users stake tokens as collateral
- Each message reveals a nullifier unique to that epoch
- Double-posting causes automatic slashing of stake

RLN balances spam prevention with free-tier access.

## History of Primitives

| Period | Primitive | Purpose |
|--------|-----------|---------|
| 2018-2019 | Sapling scheme | Private payments via commitments + nullifiers |
| 2020-2021 | zkVM | Programmable ZK contracts |
| 2021-2022 | zkas | Accessible ZK contract development |
| 2021-2022 | Event Graph | Censorship-resistant messaging |
| 2022-2023 | RLN | Decentralized spam prevention |
| 2023 | Anonymous DAO | Private governance and treasuries |
| 2024+ | Bridge Protocol (draft) | Cross-chain anonymity |

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
