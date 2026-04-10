# DAO

> [!WARNING]
> **DEPRECATED**: DAO V1 is deprecated on this fork.
>
> DAO V1 had tight coupling to Money V1's ACL (Access Control List), creating forced vendor lock-in that breaks privacy and anonymity fundamentals. All token flows through DAO governance were visible to DAO operators.
>
> Use **DAO Escrow** (WASM contract) instead for governance functionality. DAO Escrow is composable, optional, and preserves user privacy by using Merkle proofs for membership rather than ACL coupling.
>
> See [DAO Escrow concepts](../dao_escrow/concepts.md) for the recommended governance solution.

## Abstract

This contract enables anonymous on chain DAOs which can make arbitrary contract calls.
In this system, holders of the governance token specified by the DAO can
make proposals which are then voted on. When proposals pass a specified
threshold they are confirmed, then the proposal can be executed.

- [Concepts](concepts.md)
- [Model](model.md)
- [Scheme](scheme.md)

