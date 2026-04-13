# Money

> [!WARNING]
> **DEPRECATED**: Money V1 and Money V2 are deprecated.
>
> **Current contracts:**
> - **[NativeToken](../../dev/contracts/native_token.md)** - For consensus functions (PoW mining rewards, network fees)
> - **[MoneyV3](../../dev/contracts/money_v3.md)** - For DeFi functions (ERC-20 tokens, stablecoins, wrapped assets)
>
> Money V2 contains EC heap bugs in 4 of its 5 circuits (Fee_V2, Mint_V2, Burn_V2, AuthTokenMint_V2). Only TokenMint_V2 is safe (Poseidon-only).
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

