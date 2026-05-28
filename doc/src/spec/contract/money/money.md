# Money

> [!WARNING]
> **DEPRECATED**: Money V1 and Money V2 are not used on this fork.
>
> **Current contracts:**
> - **[NativeToken](../../dev/contracts/native_token.md)** - For consensus functions (PoW mining rewards, network fees)
> - **[Promissory Note](../../dev/contracts/promissory_note.md)** - For DeFi functions (ERC-20 tokens, stablecoins, wrapped assets)
>
> Money V2 circuits use EC operations that were implicated in heap corruption in the halo2 stack. Promissory Note avoids these issues by using Poseidon-only design.
>
> This document describes the legacy Money V2 contract for historical reference.

## Abstract

The _Money_ contract implements network fees, token transfers,
atomic swaps, token minting, and staking/unstaking of consensus tokens.

The functions provided by this smart contract are:
```rust
pub enum MoneyFunction {
    TransferV1,   // Transfer tokens between parties
    MintV1,       // Mint new tokens (requires auth)
    BurnV1,       // Burn existing tokens
    StakeV1,      // Stake tokens for consensus
    UnstakeV1,    // Unstake consensus tokens
}
```

- [Model](model.md)
- [Scheme](scheme.md)

