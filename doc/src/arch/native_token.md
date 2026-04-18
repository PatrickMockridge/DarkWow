# NativeToken Contract

WASM contract for consensus-layer token operations.

## Overview

NativeToken is a minimal WASM contract handling only what consensus requires. It exists at genesis alongside Deployooor, providing block rewards and fee payment functionality.

**Philosophy**: Tokens are pipework, not reactors. One job, done well.

## Function IDs

| ID | Function | Description |
|----|----------|-------------|
| 0x00 | `InitializeV1` | Initialize the native token contract |
| 0x01 | `MintV1` | Mint tokens (block rewards) |
| 0x02 | `SetNotesV1` | Configure fee parameters |

## Use Case

NativeToken handles only consensus-layer token operations:

- **Block rewards**: Newly minted tokens as incentive for miners
- **Fee payment**: Transaction fees paid to validators

All user-facing token operations (DeFi, gambling, exchanges) use [money_v3](./money_v3.md) instead.

## Why Separate from money_v3?

| Concern | NativeToken | money_v3 |
|---------|-------------|----------|
| Circuit complexity | Minimal | Full ZK circuits |
| Cryptography | None | Poseidon hash |
| Use case | Consensus (fees/rewards) | User applications |
| Upgrade frequency | Rare | As needed |

Separation means:
- NativeToken's minimal attack surface protects consensus
- money_v3 can evolve independently for DeFi needs
- Different security models for different concerns

## Genesis Configuration

At genesis, only NativeToken and Deployooor exist:

```
Genesis Contracts:
├── Deployooor (0x00) - Deploys WASM contracts
└── NativeToken (0x01) - Consensus token operations
```

Additional contracts (money_v3, DEX, stablecoin, etc.) are deployed via Deployooor as needed.