# NativeToken Contract

WASM contract for consensus-layer token operations.

## Supply Audit Capability

NativeToken provides a **supply audit capability** — a verifiable property that
any holder of the blockchain can exercise independently. The Pedersen cumulative
commitment chain (`S_H = S_{H-1} + C_H`) makes total supply cryptographically
auditable without trusting any ZK proof. This is a passive capability (like
Bitcoin's halving schedule), not an active consensus circuit breaker.

→ [NativeToken Developer Guide](../dev/contracts/native_token.md) — Full capability documentation, ZK circuits, client API

## Function IDs

| ID | Function | Description |
|----|----------|-------------|
| 0x00 | `FeeV1` | Pay network fees |
| 0x01 | `MintV1` | ~~Create new coins~~ (DISABLED — opcode reserved, use PoWRewardV1) |
| 0x02 | `BurnV1` | Destroy coins with nullifier |
| 0x03 | `TransferV1` | Private transfers |
| 0x04 | `SpendV1` | Spend with change output |
| 0x05 | `PoWRewardV1` | Block rewards + cumulative supply chain |

## Privacy Model

NativeToken uses a burn-mint privacy model:

- **PoWRewardV1**: Block rewards with cumulative supply audit capability
- **BurnV1**: Destroy coins (nullifier prevents double-spend)
- **TransferV1**: Private token transfers between parties
- **SpendV1**: Spend coins with change output

All value commitments, nullifiers, and Merkle proofs are verified through ZK
circuits. Coin attributes are committed via Pedersen commitments and revealed
only inside ZK proofs.

## Use Case

NativeToken handles only consensus-layer token operations:

- **Block rewards**: Newly minted tokens as incentive for miners (PoWRewardV1)
- **Fee payment**: Transaction fees paid to validators (FeeV1)
- **Private transfers**: ZK-shielded transfers between users (TransferV1, SpendV1)

All user-facing DeFi token operations (stablecoins, wrapped assets, ERC-20 style
tokens) use [promissory_note](../dev/contracts/promissory_note.md) instead.

## Why Separate from promissory_note?

| Concern | NativeToken | promissory_note |
|---------|-------------|----------|
| Circuit complexity | Minimal | Full DeFi circuits |
| Capability | Supply audit | Redemption |
| Use case | Consensus (fees/rewards) | User applications |
| Deployment | At genesis | Via Deployooor |
| Upgrade frequency | Rare | As needed |

Separation means:
- NativeToken's minimal attack surface protects consensus
- promissory_note can evolve independently for DeFi needs
- Different capabilities for different concerns

## Genesis Configuration

At genesis, only NativeToken and Deployooor exist:

```
Genesis Contracts:
├── Deployooor — Deploys WASM contracts
└── NativeToken — Consensus token operations + supply audit capability
```

Additional contracts (promissory_note, DEX, stablecoin, dao_escrow, etc.) are
deployed via Deployooor as needed.

## Related

- [NativeToken Developer Guide](../dev/contracts/native_token.md) — Full capability documentation, ZK circuits, client API
- [Promissory Note](./promissory_note.md) — DeFi token contract with redemption capability
