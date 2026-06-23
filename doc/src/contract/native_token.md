# NativeToken Contract

WASM contract for consensus-layer token operations.

## Supply Audit Capability

NativeToken enforces **proof of token balance** — an active consensus rule that
verifies no hidden darkw minting occurs beyond the coinbase reward. The Pedersen
cumulative commitment chain (`S_H = S_{H-1} + C_H`) combined with a per-block
mass balance equation (`Σ outputs + Σ burns + Σ fees == Σ inputs`) makes total
supply cryptographically auditable and actively enforced at every block acceptance
path in `dwowd`.

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
| Deployment | At genesis | At genesis |
| Upgrade frequency | Rare | As needed |

Separation means:
- NativeToken's minimal attack surface protects consensus
- Promissory Note can evolve independently for DeFi needs
- Different capabilities for different concerns

## Genesis Configuration

Nine contracts are deployed at genesis, each at a deterministic ContractId derived
from `poseidon_hash([42, 0, counter])` where counter increments from 2:

| Counter | Contract | Role | Consensus-Critical? |
|---------|----------|------|---------------------|
| 2 | **Deployooor** | WASM contract deployment, singleton enforcement, manifest storage | Yes (infrastructure) |
| 3 | **Promissory Note** | Universal DeFi primitive — tokens, transfers, swaps, redemption | No (ecosystem) |
| 4 | **NativeToken** | Block rewards, fee payment, supply audit | Yes |
| 5 | **Identity** | Credential issuance, selective disclosure, capability proofs | No (ecosystem) |
| 6 | **Oracle** | External data feeds — price, randomness, attestation data | No (ecosystem) |
| 7 | **Attestation** | Trust verification — on-chain attestations from trusted issuers | No (ecosystem) |
| 8 | **Purse** | Fungible capability container — hidden balances via Pedersen commitments | No (ecosystem) |
| 9 | **Box** | Capability delegation — Put/Take with linear consumption via nullifier | No (ecosystem) |
| 10 | **MultiSig** | Private threshold voting — N-of-M groups, zero-knowledge ballots | No (ecosystem) |

Only two contracts are consensus-critical: Deployooor (deployment infrastructure)
and NativeToken (block rewards and fee payment). The remaining seven are ecosystem
infrastructure — they provide canonical well-known ContractIds for composable
O-Cap primitives that every contract can reference without fragmentation from
replica deployments.

Identity, Oracle, and Attestation power the **contract manifest trust model**.
Purse and Box replace hand-rolled balance tracking and capability delegation
with modular child calls. MultiSig provides the first genesis-deployed private
voting primitive in any blockchain — N-of-M threshold groups with zero-knowledge
ballots, essential for democratic governance at all scales.

The ecosystem remains free to innovate: anyone can deploy alternative contracts
via Deployooor. Genesis status provides canonical ContractIds for composition,
not restrictions on what can be built.

## Related
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [NativeToken Developer Guide](../dev/contracts/native_token.md) — Full capability documentation, ZK circuits, client API
- [Promissory Note](./promissory_note.md) — DeFi token contract with redemption capability
