# Money

> [!NOTE]
> **DEPRECATED**: Money V1 and Money V2 are deprecated. Native Token is the current native token contract.
>
> See [Native Token](../../dev/contracts/native_token.md) for the current implementation.
>
> This document describes the legacy Money V2 contract for historical reference.

## Abstract

The _Money_ contract implements network fees, token transfers,
atomic swaps, token minting, and staking/unstaking of consensus tokens.

The functions provided by this smart contract are:
```rust
{{#include ../../../../../src/contract/money/src/lib.rs:money-function}}
```

- [Model](model.md)
- [Scheme](scheme.md)

