# DarkFi DrainProtection Contract

*⚠️ EXPERIMENTAL - This contract has NOT been audited. The protections described here are provisionally specified and require full implementation and security review.*

Governance-level protections for endowment/treasury funds against malicious DAO actions or mass exit attacks.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        DrainProtection Contract                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                         Rate Limiting                                   │  │
│  │                                                                       │  │
│  │   Transfers within base rate ──► Allowed (no vote)                     │  │
│  │   Transfers exceeding base ───► Requires 2/3 vote                       │  │
│  │                                                                       │  │
│  │   base_rate = total_funds × 1% / 1000_blocks (configurable)            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                      Vote Thresholds                                   │  │
│  │                                                                       │  │
│  │   Large withdrawal    │ 2/3 yes │ 50% quorum minimum                  │  │
│  │   Lock funds         │ 2/3 yes │ 50% quorum minimum                   │  │
│  │   Unlock funds       │ 2/3 yes │ 50% quorum minimum + 24hr timelock   │  │
│  │   Change authority   │ 2/3 yes │ 50% quorum + 48hr timelock          │  │
│  │   Renew lock         │ 2/3 yes │ 50% quorum minimum                  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                     Member Exit (Anti-Griefing)                        │  │
│  │                                                                       │  │
│  │   Any member may exit at any time                                     │  │
│  │                                                                       │  │
│  │   exit_value = (member_weight / total_weight) × funds × 0.666          │  │
│  │                                                                       │  │
│  │   Haircut: 1/3 withheld as insurance reserve                          │  │
│  │   Weight: Block-height-adjusted (longer deposits = more weight)       │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize protected fund |
| `ProposeV1` | `0x01` | Create vote proposal |
| `VoteV1` | `0x02` | Cast vote on proposal |
| `ExecuteV1` | `0x03` | Execute concluded proposal |
| `ExitV1` | `0x04` | Member exit with haircut |
| `TransferV1` | `0x05` | Transfer funds (rate-limited) |
| `LockV1` | `0x06` | Emergency lock |
| `UnlockV1` | `0x07` | Unlock after timelock |
| `UpdateConfigV1` | `0x08` | Update configuration |

## Key Formulas

### Exit Value Calculation

```
exit_value = (member_contribution_weight / total_endowment_weight) × current_funds × 0.666

member_contribution_weight = contribution × (1_000 + blocks_held / 10_000) / 1_000
```

### Rate Limit Check

```
base_rate = total_funds × base_rate_bps / 10_000

If amount ≤ base_rate: No vote required
If amount > base_rate: Requires 2/3 vote approval
```

## Provisional Status

This contract is **EXPERIMENTAL** and has **NOT been audited**.

The following work remains:

- [ ] ZK circuits for membership proof (exit_v1.zk)
- [ ] ZK circuits for proposal authorization
- [ ] Full vote weight calculation from DAO-Escrow integration
- [ ] Emergency lock duration enforcement
- [ ] Member weight tracking and updates
- [ ] Integration with DAO-Escrow for fund management
- [ ] Security audit

## Integration

The DrainProtection contract is designed to work alongside DAO-Escrow:

```
DAO-Escrow ──► Premium payments ──► Endowment Pool
                                             │
                                    ┌────────▼────────┐
                                    │ DrainProtection │
                                    │                 │
                                    │ Rate limiting   │
                                    │ Vote thresholds │
                                    │ Member exit     │
                                    └─────────────────┘
```

## See Also

- [DAO-Escrow Contract](../dao_escrow/README.md)
- [Subscription Contract](../subscription/README.md)
- [Security Analysis](../../doc/src/arch/security-analysis.md#issue-10-endowment-fund-has-no-drain-protection-major--provisional-fix-applied)