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
│  │                    OPTIONAL BEST PRACTICES                               │  │
│  │                                                                       │  │
│  │  [✓] Graduated Withdrawal Tiers - Multi-level approval requirements │  │
│  │  [✓] Exit Queue (FCFS) - Prevents bank-run cascades                 │  │
│  │  [✓] Circuit Breaker - Auto-pause on anomalous drain                 │  │
│  │  [✓] Guardian Pause - Multisig emergency stop                        │  │
│  │  [✓] Observation Period - Delay for large withdrawals                 │  │
│  │  [✓] Split Proposals - Prevent single-proposal drains               │  │
│  │  [✓] No-Loss Reserve - Untouched insurance funds                     │  │
│  │  [✓] Dead Man's Switch - Auto-protocol on abandonment               │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick Start: Enable All Protections

```rust
use darkfi_drain_protection::{DrainConfig, *};

let config = DrainConfig::full();
```

## Feature Matrix

| Feature | Purpose | Risk Mitigated | Recommended For |
|---------|---------|----------------|-----------------|
| `graduated_tiers` | Multi-tier withdrawal limits | Single-vote large drains | All endowments >100K |
| `exit_queue` | FCFS exit processing | Bank-run cascades | Large member bases |
| `circuit_breaker` | Auto-pause on anomaly | Continued bleeding during attack | All endowments |
| `guardian_pause` | Multisig pause capability | Manual emergency stop | All endowments |
| `observation_period` | Delay before large withdrawals | Stealth drain attacks | Treasuries >1M |
| `split_proposals` | Split large proposals | Single proposal drains | All endowments |
| `no_loss_reserve` | Untouched insurance | Total loss scenarios | Large endowments |
| `dead_mans_switch` | Auto-protocol on inactivity | Abandonment protection | Long-term endowments |

---

## Feature Details

### 1. Graduated Withdrawal Tiers

Instead of binary (rate-limited or vote), use graduated tiers based on amount:

| Tier | Amount | Timeframe | Requirement |
|------|--------|-----------|-------------|
| 1 | ≤ 1% TVL | Per block | No vote (rate-limited only) |
| 2 | ≤ 5% TVL | Per week | 50% quorum + 1 day timelock |
| 3 | ≤ 20% TVL | Per month | 2/3 quorum + 7 day timelock |
| 4 | > 20% TVL | Any | 90% quorum + 30 day timelock |

**Configuration:**
```rust
GraduatedTiers {
    tier1_max_bps: 100,           // 1% TVL per block (no vote)
    tier2_max_bps: 500,           // 5% TVL per week
    tier2_quorum_bps: 5000,       // 50% quorum
    tier2_timelock_blocks: 600,   // 1 day
    tier3_max_bps: 2000,         // 20% TVL per month
    tier3_quorum_bps: 6670,       // 2/3 quorum
    tier3_timelock_blocks: 4200,  // 7 days
    tier4_threshold_bps: 2000,    // >20% TVL = emergency
    tier4_quorum_bps: 9000,       // 90% quorum
    tier4_timelock_blocks: 18000, // 30 days
}
```

### 2. Exit Queue (FCFS)

Prevents bank-run cascades by processing exits in strict FIFO order. Max exit per epoch prevents draining more than TVL can handle.

```
┌─────────────────────────────────────────────┐
│               Exit Queue                     │
├─────────────────────────────────────────────┤
│  Position 1: Member A - 5% - queued block X │
│  Position 2: Member B - 3% - queued block X │
│  Position 3: Member C - 8% - queued block X  │
│                                              │
│  Processing: FIFO by queue position          │
│  Max exit per epoch: 10% TVL                │
│  Prevents: Bank-run cascade                  │
└─────────────────────────────────────────────┘
```

**Configuration:**
```rust
ExitQueueConfig {
    max_exit_per_epoch_bps: 1000,  // Max 10% TVL can exit per epoch
    epoch_blocks: 600,             // Epoch: 1 day
    min_queue_blocks: 10,          // Must wait 10 blocks in queue
    force_fcfs: true,              // Enforce strict FCFS
}
```

### 3. Circuit Breaker

Auto-pauses withdrawals if drain rate exceeds threshold, preventing continued bleeding during an attack.

```
Anomalous Drain Detected
         │
         ▼
┌────────────────────┐
│ Circuit Breaker    │
│ Triggered!         │
└────────┬───────────┘
         │
         ▼
┌────────────────────┐
│ All Withdrawals    │
│ PAUSED for 24h     │
└────────┬───────────┘
         │
         ▼
┌────────────────────┐
│ Guardians Notified │
│ (manual resume)    │
└────────────────────┘
```

**Configuration:**
```rust
CircuitBreakerConfig {
    trigger_threshold_bps: 1000,   // Trigger if >10% drained in window
    window_blocks: 100,            // Measure over 100 blocks
    pause_duration_blocks: 600,    // Pause for 24 hours
    auto_resume: false,            // Manual resume required
    notify_guardians: true,        // Alert guardians
}
```

### 4. Guardian Pause (Multisig)

Designated watchers can pause withdrawals without full governance. Not full control - only pause ability.

```
Guardian Action:
1. 2-of-3 multisig pauses withdrawals
2. 24h timelock before unpause can begin
3. After timelock, 2-of-3 multisig can unpause
4. Max pause duration: 7 days (auto-resume)
```

**Configuration:**
```rust
GuardianPauseConfig {
    guardian_keys: vec![key1, key2, key3],  // 3 guardians
    required_signatures: 2,                  // 2-of-3 multisig
    unpause_timelock_blocks: 144,            // 24 hours
    max_pause_duration_blocks: 1008,          // 7 days auto-resume
}
```

### 5. Observation Period

Large withdrawals must be publicly visible for a period before execution, giving members time to react.

```
Large Withdrawal Proposed (>5% TVL)
           │
           ▼
┌─────────────────────────┐
│  48h Observation        │
│  Period Begins          │
│  (all members can see)  │
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ Members Can Raise       │
│ Objections / Exit       │
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ After 48h:              │
│ Vote proceeds or        │
│ Emergency bypass         │
│ (90% quorum)             │
└─────────────────────────┘
```

**Configuration:**
```rust
ObservationPeriodConfig {
    threshold_bps: 500,           // Trigger for >5% TVL
    observation_blocks: 288,       // 48 hours
    allow_emergency_bypass: true, // Allow bypass with higher quorum
    emergency_bypass_quorum_bps: 9000,  // 90% quorum
}
```

### 6. Split Proposals

Large proposals must be split into smaller chunks, preventing single malicious proposal drains.

```
Malicious Proposal: Drain 50% TVL
          │
          ▼
┌─────────────────────┐
│ SPLIT REQUIRED!    │
│ Max chunk: 10% TVL  │
└────────┬────────────┘
          │
          ▼
┌─────────────────────┐
│ Must split into:    │
│ 10% + 10% + 10% +   │
│ 10% + 10%           │
│                     │
│ Each needs separate │
│ vote. 1 day between  │
│ each chunk.          │
└─────────────────────┘
```

**Configuration:**
```rust
SplitProposalsConfig {
    threshold_bps: 1000,          // Split if >10% TVL
    max_chunk_bps: 1000,          // Max chunk: 10% TVL
    chunk_delay_blocks: 600,      // Wait 1 day between chunks
    separate_vote_each_chunk: true,  // Each chunk needs vote
}
```

### 7. No-Loss Reserve

A percentage of funds are never available for DAO governance and serve as permanent insurance.

```
┌─────────────────────────────────────┐
│         Endowment Structure          │
├─────────────────────────────────────┤
│  DAO-controlled: 80% of funds       │
│                                     │
│  ┌─────────────────────────────┐   │
│  │ NO-LOSS RESERVE: 20%        │   │
│  │ - Never DAO-governed         │   │
│  │ - Emergency vote only        │   │
│  │ - Always minimum 1% TVL      │   │
│  └─────────────────────────────┘   │
│                                     │
│  Reserve use:                       │
│  - Automatic coverage of exits       │
│  - Insurance claims                  │
│  - Cannot be voted to drain         │
└─────────────────────────────────────┘
```

**Configuration:**
```rust
NoLossReserveConfig {
    reserve_bps: 2000,             // 20% reserve
    reserve_spend_authority: ReserveSpendAuthority::EmergencyVoteOnly,
                                // Only emergency vote can spend
    min_reserve_absolute: 100,     // Keep at least 1% TVL minimum
}
```

### 8. Dead Man's Switch

Auto-engages protections if DAO is inactive for extended period.

```
No DAO Activity for 30 Days
           │
           ▼
┌─────────────────────────┐
│ NOTIFICATION SENT      │
│ to all members          │
│ (7 day notice)          │
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ SWITCH TRIGGERED        │
│ - Rate limit: 1% TVL/day│
│ - Social recovery opens  │
│ - 14 day timelock       │
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ Members can:            │
│ - Claim pro-rata exit   │
│ - Vote to restore       │
│ - Multisig recovery     │
└─────────────────────────┘
```

**Configuration:**
```rust
DeadMansSwitchConfig {
    inactivity_threshold_blocks: 43200,  // 30 days
    auto_rate_limit_bps: 100,            // 1% TVL per day
    notification_blocks: 1008,           // 7 day notice
    enable_social_recovery: true,         // Allow member claims
    social_recovery_timelock_blocks: 2016, // 14 day timelock
}
```

---

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize protected fund with config |
| `ProposeV1` | `0x01` | Create vote proposal |
| `VoteV1` | `0x02` | Cast vote on proposal |
| `ExecuteV1` | `0x03` | Execute concluded proposal |
| `ExitV1` | `0x04` | Member exit with haircut |
| `TransferV1` | `0x05` | Transfer funds (rate-limited) |
| `LockV1` | `0x06` | Emergency lock |
| `UnlockV1` | `0x07` | Unlock after timelock |
| `UpdateConfigV1` | `0x08` | Update configuration |

---

## DrainConfig: All-In-One Configuration

```rust
/// Comprehensive drain protection configuration.
/// All features are OPTIONAL and default to disabled.
pub struct DrainConfig {
    pub graduated_tiers: Option<GraduatedTiers>,
    pub exit_queue: Option<ExitQueueConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub guardian_pause: Option<GuardianPauseConfig>,
    pub observation_period: Option<ObservationPeriodConfig>,
    pub split_proposals: Option<SplitProposalsConfig>,
    pub no_loss_reserve: Option<NoLossReserveConfig>,
    pub dead_mans_switch: Option<DeadMansSwitchConfig>,
}

impl DrainConfig {
    /// Enable ALL protections with recommended defaults
    pub fn full() -> Self { ... }

    /// Conservative: circuit_breaker + guardian_pause + no_loss_reserve
    pub fn conservative() -> Self { ... }

    /// Minimal: circuit_breaker + guardian_pause only
    pub fn minimal() -> Self { ... }
}
```

---

## Integration with DAO-Escrow

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
                                    │ + All optional  │
                                    │   best practices│
                                    └─────────────────┘
```

Enable drain protection when initializing DAO-Escrow:

```rust
InitializeParamsV1 {
    dao_bulla: ...,
    owner_pubkey: ...,
    endowment_token_id: ...,
    bulla_blind: ...,
    enable_drain_protection: true,  // ← Enable here
}
```

---

## Key Formulas

### Exit Value Calculation (with optional features)

```
exit_value = (member_contribution_weight / total_endowment_weight) × current_funds × 0.666

member_contribution_weight = contribution × (1_000 + blocks_held / 10_000) / 1_000

# With exit queue: exit_value limited by queue position
# With no_loss_reserve: exit_value limited by (total_funds - reserve_balance)
```

### Rate Limit Check

```
base_rate = total_funds × base_rate_bps / 10_000

# Without graduated tiers:
If amount ≤ base_rate: No vote required
If amount > base_rate: Requires 2/3 vote approval

# With graduated tiers (example tier 2):
If amount ≤ tier1_max: No vote required
If tier1_max < amount ≤ tier2_max:
    - Requires tier2_quorum approval
    - Timelock: tier2_timelock_blocks
```

---

## Status

This contract is **EXPERIMENTAL** and has **NOT been audited**.

**Implemented Protections:**

| Protection | Status | Notes |
|------------|--------|-------|
| Rate limiting | ✅ Complete | Per-block base rate |
| Vote thresholds | ✅ Complete | Configurable quorum/approval |
| Member exit | ✅ Complete | Haircut enforcement |
| Graduated tiers | ✅ Complete | All tiers configurable |
| Exit queue | ✅ Complete | FCFS with epoch limits |
| Circuit breaker | ✅ Complete | Auto-pause on anomaly |
| Guardian pause | ✅ Complete | Multisig with timelock |
| Observation period | ✅ Complete | Configurable threshold |
| Split proposals | ✅ Complete | Max chunk size |
| No-loss reserve | ✅ Complete | Percentage + minimum |
| Dead man's switch | ✅ Complete | Inactivity + social recovery |

**Outstanding Work:**
- [ ] ZK circuit for vote authorization (for graduated tiers)
- [ ] ZK circuit for exit queue position verification
- [ ] Integration tests between DAO-Escrow and DrainProtection
- [ ] Security audit

---

## See Also

- [DAO-Escrow Contract](../dao_escrow/README.md)
- [Subscription Contract](../subscription/README.md)
- [Security Analysis](../../doc/src/arch/security-analysis.md#issue-10-endowment-fund-has-no-drain-protection-major--provisional-fix-applied)
