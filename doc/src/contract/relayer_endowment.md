# Relayer Endowment Contract

External capital providers ("backers") deploy capital to relayers in exchange for a share of the relayer's fees. This enables relayers to operate with more coverage than their own stake alone.

## Composition

**Not implemented as described.** The crate defines `RELAYER_ENDOWMENT_PURSE_CONTRACT_ID`
and writes it to its info tree, but no entrypoint calls Purse. `DeployCapitalV1` and
`WithdrawDeploymentV1` each require exactly **one** child,
`promissory_note::transfer_v1 (0x04)` — deployed capital moves as a promissory-note
transfer.

The composition the contract is being built toward: total deployed capital and
per-deployment amounts tracked in Purses, with `DeployCapital` calling `Purse::DepositV1`
and `SettleFees` calling `Purse::WithdrawV1`, so the Purse handles balance integrity via
Pedersen commitments.

## Overview

This contract enables:
1. **Relayers** to attract external capital for increased coverage
2. **Backers** to earn yield by providing capital to relayers
3. **Fee sharing** between relayers and their backers

## How It Works

```
┌─────────────────┐      ┌──────────────────┐      ┌─────────────────┐
│     Relayer     │ ◄──► │ Relayer Endowment│ ◄──► │     Backer      │
│  (Needs Capital)│      │    (Layer)       │      │ (Provides $)    │
└─────────────────┘      └──────────────────┘      └─────────────────┘
        │                         │                         │
        │ Initializes             │ Deploys                  │ Deploys capital
        │ endowment               │ capital                  │ Earns fee share
        ▼                         ▼                         ▼
   Earns fees            Tracks deployments          Withdraws principal
   (after backer cut)    and fee allocations         + accumulated fees
```

### Endowment Flow

1. **Initialize**: A relayer initializes an endowment account with a default backer cut
2. **Deploy Capital**: Backer deploys DAI/NETHER to the relayer's endowment
3. **Earn Fees**: Backer receives a percentage (`backer_cut_bp`) of relayer's earned fees
4. **Settle Fees**: Relayer settles accumulated fees to deployments
5. **Claim Fees**: Backer claims their share of accumulated fees
6. **Withdraw**: Backer can withdraw their deployment + accumulated fees

## Contract Functions

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | Initialize endowment account for a relayer |
| 0x01 | DeployCapitalV1 | Backer deploys capital to a relayer's endowment |
| 0x02 | WithdrawDeploymentV1 | Backer withdraws their deployment |
| 0x03 | ClaimRelayerFeesV1 | Backer claims their share of relayer fees |
| 0x04 | SettleFeesV1 | Relayer settles fees to backers |
| 0x05 | UpdateConfigV1 | Update fee configuration |
| 0x06 | ForceSettleV1 | Backer force-settles fees after relayer inactivity timeout |
| 0x07 | DeactivateEndowmentV1 | Deactivate an endowment account |
| 0x0b | RecordCollectedFeesV1 | Relayer records fees collected and not yet allocated |

## Data Model

### RelayerEndowmentAccount

```rust
pub struct RelayerEndowmentAccount {
    pub version: u8,
    pub instance_seed: [u8; 32],
    pub relayer_pub: PublicKey,
    pub total_deployed: u64,
    pub active_deployments: u64,
    pub accumulated_fees: u64,
    pub default_backer_cut_bp: u32,
    pub created_at: u64,
    pub last_settlement_height: u64,      // track settlement activity
    pub total_collected_fees_log: u64,    // backer-auditable fee log
    pub is_active: bool,
    pub total_slashed: u64,
    pub total_successful: u64,
}
```

`version` and `instance_seed` lead every record (`version` is the first byte of every stored
record), and `total_slashed`/`total_successful` were missing from this snippet and from every other
struct table in the contract docs until 2026-10-12 (`F-527`).

### EndowmentDeployment

```rust
pub struct EndowmentDeployment {
    pub version: u8,
    pub deployment_id: pallas::Base,
    pub relayer_pub: PublicKey,
    pub backer_pub: PublicKey,
    pub amount: u64,
    pub backer_cut_bp: u32,
    pub accumulated_fees: u64,
    pub deployed_at: u64,
    pub withdraw_requested_at: Option<u64>,
    pub withdrawn: bool,
}
```

## Force Settlement (May 2026 Hardening)

If a relayer fails to call `SettleFeesV1` within `FORCE_SETTLEMENT_TIMEOUT` blocks (1000), any backer with an active deployment can call `ForceSettleV1` (opcode `0x06`) to claim their pro-rata share of accumulated fees. This protects backers from relayer fee evasion.

```
ForceSettleV1 flow:
1. Backer checks current_block - account.last_settlement_height > FORCE_SETTLEMENT_TIMEOUT
2. Backer submits ForceSettleV1 with deployment_id
3. Contract computes pro-rata fee share from total_collected_fees_log
4. Fees credited to backer's deployment
5. last_settlement_height updated to current block
```

### The log's writer

`ForceSettleV1` distributes `total_collected_fees_log`, but the entrypoint that moves
money — `SettleFeesV1` — writes `accumulated_fees` instead. Until 2026-10-11 nothing
incremented the log at all: it had exactly two assignment sites, `= 0` at initialize
and a `saturating_sub` here, so the pro-rata share above was always zero and the
endpoint was callable but never payable (`F-513`, `OBL-C221`). `RecordCollectedFeesV1`
(`0x0b`) is the writer: the relayer records fees it has collected and not yet
allocated, and the log grows.

It deliberately does **not** touch `last_settlement_height`. That field is what the
timeout above is measured against, so a writer that reset it would let a relayer
suppress force-settlement indefinitely simply by recording often — the endpoint that
makes this section work would become the means of defeating it.

What it does not fix: the log is the *relayer's claim*, because only the relayer knows
its off-chain fee income. A relayer that means to evade never calls it and the log
stays where it was. The protection above therefore covers a relayer that is honest but
late — which is what a timeout models — and not one that has decided not to settle.

## Economic Model

| Role | Deposit | Earn | Risk |
|------|---------|------|------|
| Relayer | Own stake | Full fees (minus backer cut) | Reputation, slashing |
| Backer | Deployment capital | `backer_cut_bp` of relayer fees | Relayer default, mitigated by ForceSettleV1 |

### Fee Calculation

When a relayer settles fees:
```
total_fee_share = total_fees × backer_cut_bp / 10000
per_deployment = total_fee_share / active_deployments
```

Each deployment accumulates fees proportionally to its share of total deployed capital.

## Database Trees

| Tree | Purpose |
|------|---------|
| `endowment_registry` | Relayer endowment accounts |
| `endowment_deployments` | Individual deployments |
| `endowment_fees` | Fee allocations per deployment |
| `relayer_endowment_info` | Contract info and version |

## Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `RELAYER_ENDOWMENT_MIN_DEPLOY` | 1,000,000 | Minimum deployment amount |
| `RELAYER_ENDOWMENT_BP_PRECISION` | 10000 | Basis points precision |

## Composability

This contract composes patterns from:
- **dao_escrow**: Endowment management
- **betting_stake**: Proportional share calculations

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [Pool Stake](./pool_stake.md) - Similar pooled capital pattern
- [Bridge Contract](./bridge.md) - Relayer operations
- [DAO Escrow](./dao_escrow.md) - Endowment management pattern