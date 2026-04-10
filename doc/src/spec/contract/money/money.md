# Money

> [!NOTE]
> **Money V2**: This is Money V2 (formerly known as Money V2), now simply called "Money" on this fork.
>
> Money V2 is a WASM-based contract with secure ZK circuits using `constrain_equal_base` for provably correct proofs. It is fully composable and replaces the deprecated Money V1.
>
> The legacy Money V1 remains available but is **deprecated** due to non-composable architecture.

## Abstract

The _Money_ contract implements network fees, token transfers,
atomic swaps, token minting, and staking/unstaking of consensus tokens.

The functions provided by this smart contract are:
```rust
{{#include ../../../../../src/contract/money/src/lib.rs:money-function}}
```

- [Model](model.md)
- [Scheme](scheme.md)

