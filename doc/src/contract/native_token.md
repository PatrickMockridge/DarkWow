# NativeToken Contract

WASM contract for consensus-layer token operations.

## Overview

NativeToken is a minimal WASM contract handling only what consensus requires. It exists at genesis alongside Deployooor, providing block rewards, fee payment, and privacy-preserving token operations through burn-mint ZK circuits.

**Design principle**: Tokens are pipework, not reactors. One job, done well.

## Function IDs

| ID | Function | Description |
|----|----------|-------------|
| 0x00 | `FeeV1` | Pay network fees |
| 0x01 | `MintV1` | Create new coins (Z-cash style mint) |
| 0x02 | `BurnV1` | Destroy coins with nullifier |
| 0x03 | `TransferV1` | Private transfers |
| 0x04 | `SpendV1` | Spend with change output |
| 0x05 | `PoWRewardV1` | Block rewards for miners |

## Privacy Model

NativeToken uses a Z-cash style burn-mint privacy model:

- **MintV1**: Create new coins with Poseidon commitments
- **BurnV1**: Destroy coins (nullifier prevents double-spend)
- **TransferV1**: Private token transfers between parties
- **SpendV1**: Spend coins with change output

All value commitments, nullifiers, and Merkle proofs are verified through ZK circuits. Coin attributes (public key, value, token ID, spend hook, user data) are committed via Pedersen commitments and revealed only inside ZK proofs.

## Use Case

NativeToken handles only consensus-layer token operations:

- **Block rewards**: Newly minted tokens as incentive for miners (PoWRewardV1)
- **Fee payment**: Transaction fees paid to validators (FeeV1)
- **Private transfers**: ZK-shielded transfers between users (TransferV1, SpendV1)

All user-facing DeFi token operations (stablecoins, wrapped assets, ERC-20 style tokens) use [money_v3](./money_v3.md) instead.

## Why Separate from money_v3?

| Concern | NativeToken | money_v3 |
|---------|-------------|----------|
| Circuit complexity | Minimal | Full DeFi circuits |
| Privacy model | Burn-mint (Z-cash) | Burn-mint (Z-cash) |
| Use case | Consensus (fees/rewards) | User applications |
| Deployment | At genesis | Via Deployooor |
| Upgrade frequency | Rare | As needed |

Separation means:
- NativeToken's minimal attack surface protects consensus
- money_v3 can evolve independently for DeFi needs
- Different security models for different concerns

## Genesis Configuration

At genesis, only NativeToken and Deployooor exist:

```
Genesis Contracts:
├── Deployooor — Deploys WASM contracts
└── NativeToken — Consensus token operations
```

Additional contracts (money_v3, DEX, stablecoin, dao_escrow, etc.) are deployed via Deployooor as needed.

## Related

- [NativeToken Developer Guide](../dev/contracts/native_token.md) — Implementation details, ZK circuits, client API
- [Money V3 Migration](./money_v3_migration.md) — Why Money V1/V2 were replaced
