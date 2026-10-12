# Drain Protection Contract

*⚠️ EXPERIMENTAL - This contract has NOT been audited. The protections described here require full implementation and security review.*

Governance-level protections for endowment/treasury funds against malicious DAO actions or mass exit attacks.

## Composition

**Not implemented as the design describes.** `InitializeV1` writes the genesis
`PURSE_CONTRACT_ID` and `BOX_CONTRACT_ID` into this contract's info tree, and neither is
ever called — no entrypoint makes a Purse or Box child call. Every value-moving entrypoint
(`ExitV1`, `TransferV1`) requires exactly **one** child, `promissory_note::transfer_v1
(0x04)`, and `ExecuteV1` requires one child, `multisig::FinalizeV1 (0x03)`. Value and spend
authority both travel through other contracts today.

The composition the contract is being built toward:

- **Purse**: the ProtectedFund's total tracked in a Purse, transfers calling
  `Purse::DepositV1`/`WithdrawV1` as child calls, so the Purse handles balance integrity
  via Pedersen commitments and this contract does no value arithmetic of its own.
- **Box**: spend authority and governance rights (propose, vote, authorize transfers)
  delegated via Box, DAO members consuming Boxes via `Box::TakeV1`.

See [Purse](purse.md) and [Box](box.md) for the genesis primitives.

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | `0x00` | Initialize a new protected fund with governance configuration |
| ProposeV1 | `0x01` | Propose a governance action (large withdrawal, lock, authority change) |
| VoteV1 | `0x02` | Cast a vote on an active proposal |
| ExecuteV1 | `0x03` | Execute a concluded proposal after voting period ends |
| ExitV1 | `0x04` | Exit the fund with a haircut penalty (any member, any time) |
| TransferV1 | `0x05` | Transfer funds from the protected pool (rate-limited) |
| LockV1 | `0x06` | Emergency lock of funds |
| UnlockV1 | `0x07` | Unlock previously locked funds |
| UpdateConfigV1 | `0x08` | Update contract configuration parameters |

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        DrainProtection Contract                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  All features are OPTIONAL and configurable by the contract deployer.        │
│  DAO members control features via governance proposals.                       │
│                                                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                    8 OPTIONAL BEST PRACTICES                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  [✓] Graduated Tiers     - Multi-level withdrawal approval (1%→5%→20%→EMG) │
│  [✓] Exit Queue (FCFS)   - Prevents bank-run cascades                       │
│  [✓] Circuit Breaker     - Auto-pause on anomalous drain                    │
│  [✓] Guardian Pause      - Multisig emergency stop                           │
│  [✓] Observation Period  - 48h delay before large withdrawals                │
│  [✓] Split Proposals    - Chunk large withdrawals                           │
│  [✓] No-Loss Reserve     - 20% untouched insurance                          │
│  [✓] Dead Man's Switch   - Auto-protocol on DAO inactivity                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Deployer Control

The **contract deployer** chooses which protections to enable at initialization:

`DrainConfig` is built field by field. There are **no** `full()`, `conservative()` or `minimal()`
presets and no `graduated_tiers` field — this example showed all four until 2026-10-12. The struct
has seven fields, four of which are optional and default to `None`:

```rust
pub struct DrainConfig {
    pub guardian_multisig_group_id: pallas::Base,   // required
    pub exit_queue: Option<ExitQueueConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub observation_period: Option<ObservationPeriodConfig>,
    pub split_proposals: Option<SplitProposalsConfig>,
    pub no_loss_reserve: Option<NoLossReserveConfig>,
    pub dead_mans_switch: Option<DeadMansSwitchConfig>,
}
```

`Default` enables `exit_queue` and `circuit_breaker` and leaves the rest `None`, with a zero
`guardian_multisig_group_id` — so a deployer starting from `DrainConfig::default()` must set the
guardian group, and opts the other protections in by assignment:

```rust
let mut config = DrainConfig::default();
config.guardian_multisig_group_id = guardian_group;
config.observation_period = Some(ObservationPeriodConfig { .. });
config.split_proposals = Some(SplitProposalsConfig { .. });
```

## DAO Member Control

Once deployed, **DAO members** control protections through governance:

| Action | How to Change |
|--------|---------------|
| Enable/disable features | Governance proposal |
| Adjust thresholds | Governance proposal |
| Add/remove guardians | Governance proposal |
| Trigger circuit breaker | Automatic (on-chain) |
| Emergency pause | Guardian multisig |
| Exit queue processing | First-come-first-served |

## Feature Details

### 1. Graduated Withdrawal Tiers

Multi-level approval based on withdrawal amount:

| Tier | Amount | Requirement |
|------|--------|-------------|
| 1 | ≤ 1% TVL | No vote (rate-limited only) |
| 2 | ≤ 5% TVL/week | 50% quorum + 1 day timelock |
| 3 | ≤ 20% TVL/month | 2/3 quorum + 7 day timelock |
| 4 | > 20% TVL | 90% quorum + 30 day timelock |

### 2. Exit Queue (FCFS)

- Prevents bank-run cascades
- Max 10% TVL exits per epoch (1 day)
- Strict FIFO processing
- Minimum 10 blocks in queue

### 3. Circuit Breaker

- Triggers if >10% drained in 100 blocks
- Auto-pauses for 24 hours
- Alerts guardians
- Manual resume required

### 4. Guardian Pause

- 2-of-3 multisig emergency stop
- 24h unpause timelock
- Max 7 days auto-resume

### 5. Observation Period

- >5% TVL withdrawals: 48h public visibility
- Members can raise objections or exit
- Emergency bypass with 90% quorum

### 6. Split Proposals

- >10% TVL must be chunked into ≤10% pieces
- 1 day between chunks
- Separate vote per chunk

### 7. No-Loss Reserve

- 20% of funds never DAO-governable
- Only emergency vote can access
- Minimum 1% TVL absolute floor

### 8. Dead Man's Switch

- Triggers after 30 days DAO inactivity
- Auto-limits to 1% TVL/day
- 7 day member notification
- Social recovery mode opens

## Integration with DAO-Escrow

```
DAO-Escrow ──► Premium payments ──► Endowment Pool
                                             │
                                    ┌────────▼────────┐
                                    │ DrainProtection │
                                    │                 │
                                    │ Deployer:       │
                                    │ - Chooses which │
                                    │   features to   │
                                    │   enable        │
                                    │                 │
                                    │ DAO Members:    │
                                    │ - Control via   │
                                    │   governance    │
                                    │ - Guardians     │
                                    │   emergency    │
                                    └─────────────────┘
```

## Security Considerations

| Feature | Protects Against | Trade-off |
|---------|-----------------|-----------|
| Graduated tiers | Single-vote large drains | Slower for large withdrawals |
| Exit queue | Bank-run cascades | Less liquidity for members |
| Circuit breaker | Continued bleeding | Potential for false positives |
| Guardian pause | Centralization risk | Requires trusted guardians |
| Observation period | Stealth drains | Less privacy for large txs |
| Split proposals | Flash loan attacks | More governance overhead |
| No-loss reserve | Total loss | Less DAO-controlled capital |
| Dead man's switch | Abandonment | Auto-protocol may not suit all |

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [DAO-Escrow Contract](dao_escrow.md)
- [Subscription Contract](subscription.md)
- [Security Analysis](../arch/security-analysis.md#issue-10--endowment-fund-has-no-drain-protection--major----fixed)
- [DrainProtection Source](../../../src/contract/drain_protection/README.md)
