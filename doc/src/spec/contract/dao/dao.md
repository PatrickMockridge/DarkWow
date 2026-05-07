# DAO

> [!WARNING]
> **DEPRECATED**: DAO V1 is not used on this fork.
>
> DAO V1 uses token-weighted ACL voting tightly coupled to the Money contract. This model reveals voter identity and token balance through the ACL mechanism.
>
> This fork uses **DAO Escrow** (WASM contract) instead for governance. DAO Escrow uses ZK predicates for membership — composable, optional, and preserves user privacy.
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

