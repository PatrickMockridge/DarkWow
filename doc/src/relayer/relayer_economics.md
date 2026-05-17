# Relayer Economics

> **Note:** The economic layer described here builds on relayer infrastructure now implemented and tested. The feed market, staking pools, and capital deployment mechanisms are active on testnet. **Hardening applied May 2026**: proportional slashing, fee caps, force settlement, and circuit breaker are now live in the bridge and endowment contracts. See [Security Audit](../contract/audit.md).

*On-chain coordination, feed markets, staking pools, and capital deployment for DarkWow relayers*

## Overview

This document describes the economic layer that enables relayers to operate as a reliable, decentralized service infrastructure. It builds on the [Stake = Coverage](./slashing.md) model and adds:

1. **On-chain relayer coordination** - Service continuity via smart contracts
2. **Feed market** - Two-mode pricing for delivery guarantees
3. **Staking pools** - Collective stake for shared coverage
4. **Capital deployers** - External capital backing relayers for yield
5. **Betting stake** - Market-based consensus on relayer quality

## On-Chain Relayer Coordination

To ensure service continuity even if relayers go offline, the relayer coordination logic lives on-chain:

```
┌─────────────────────────────────────────────────────────────────────┐
│                  On-Chain Relayer Registry                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  pub struct OnChainRelayer {                                       │
│      pub key: Pubkey,                    // Relayer's public key   │
│      pub stake: StakeInfo,              // Current stake           │
│      pub status: RelayerStatus,         // Active/Inactive/Slashed │
│      pub reputation: ReputationScore,    // Historical performance  │
│      pub fee_tier: FeeTier,             // Pricing tier            │
│      pub available_stake: Value,        // Stake - in-flight      │
│  }                                                              │
│                                                                     │
│  pub enum RelayerStatus {                                         │
│      Active,                 // Accepting withdrawals              │
│      Inactive,               // Temporarily unavailable             │
│      Slashed,               // Punished, stake locked              │
│      Bankrupt,              // Stake exhausted,退出               │
│  }                                                                  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Service Continuity

When a relayer goes offline, the on-chain contract handles the aftermath:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Service Continuity Flow                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  RELAYER GOES OFFLINE:                                            │
│                                                                     │
│  1. Contract detects: No heartbeat for N blocks                   │
│                                                                     │
│  2. In-flight withdrawals:                                        │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ Timeout countdown starts                                    │ │
│     │ User can wait or initiate claim                           │ │
│     │ Other relayers can pick up pending withdrawals              │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  3. If relayer comes back online:                                  │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ Resume operations                                          │ │
│     │ Pending withdrawals can still be completed                   │ │
│     │ Reputation preserved (slate not wiped)                      │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  4. If relayer never returns:                                     │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ Users claim from stake after timeout                        │ │
│     │ Stake gradually released after all claims settled           │ │
│     │ Relayer marked Bankrupt                                     │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Feed Market

The feed market allows relayers to price their service in two modes:

### Mode 1: Standard Fee

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Mode 1: Standard Fee                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  User pays a fee for service. Relayer executes or user cancels.   │
│                                                                     │
│  Pricing:                                                          │
│  - Flat fee (e.g., 0.1 DAI per withdrawal)                        │
│  - Percentage of withdrawal amount (e.g., 0.5%)                   │
│  - Volume discounts for frequent users                            │
│                                                                     │
│  If relayer fails:                                                 │
│  - User can cancel withdrawal after timeout                        │
│  - User gets funds back (no extra compensation)                   │
│  - Relayer is slashed proportionally (10% of amount)               │
│  - Relayer reputation suffers                                       │
│                                                                     │
│  FEE CAP (May 2026 hardening):                                     │
│  - Bridge enforces MAX_FEE_BP = 1000 (10% maximum)                │
│  - Users can specify tighter per-withdrawal max_fee_bp            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Mode 2: Delivery Guarantee

```
┌─────────────────────────────────────────────────────────────────────┐
│                  Mode 2: Delivery Guarantee                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  User pays: Fee + Refund Premium                                   │
│                                                                     │
│  Example:                                                         │
│  - Withdrawal: 100 XMR                                             │
│  - Standard fee: 0.5 XMR (0.5%)                                  │
│  - Refund premium: 1 XMR (1% of amount)                          │
│  - Total user pays upfront: 101.5 XMR (locked)                   │
│                                                                     │
│  On successful delivery:                                          │
│  - Relayer receives: 1.5 XMR (fee + premium)                      │
│  - User receives: 100 XMR                                        │
│                                                                     │
│  On non-delivery (timeout):                                       │
│  - User receives: 101 XMR (refund + premium)                      │
│  - Relayer gets nothing                                           │
│  - Relayer reputation suffers                                      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Economic Signals

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Feed Market Price Signals                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  The ratio of Mode 2 vs Mode 1 requests reveals:                  │
│                                                                     │
│  HIGH Mode 2 ratio =                                              │
│  ├── Users don't trust relayer reliability                        │
│  ├── Relayer needs to improve reputation                         │
│  └── Price pressure on relayer to reduce fees                    │
│                                                                     │
│  LOW Mode 2 ratio =                                               │
│  ├── Users trust the relayer                                      │
│  └── Relayer can maintain higher fees                             │
│                                                                     │
│  Mode 2 premium becomes a RELIABILITY INDEX:                       │
│  └── Market prices in perceived reliability                       │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Staking Pools

Relayers can form pools to share stake and coverage:

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Relayer Staking Pool                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                      POOL STRUCTURE                         │   │
│  │                                                              │   │
│  │  Pool A                                                     │   │
│  │  ├── Relayer 1 (50% stake)                                 │   │
│  │  ├── Relayer 2 (30% stake)                                 │   │
│  │  └── Relayer 3 (20% stake)                                 │   │
│  │                                                              │   │
│  │  Total pool stake: 10,000 DAI + 5,000 NETHER              │   │
│  │  Combined coverage capacity: 15,000 DAI equivalent          │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Pool Economics

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Pool Economics                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  SHARING:                                                         │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Fees shared proportional to stake                           │   │
│  │ Relayer 1 (50% stake) = 50% of pool fees                   │   │
│  │ Relayer 2 (30% stake) = 30% of pool fees                   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  SLASHING (Updated May 2026):                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ PROPORTIONAL SLASHING: slash = max(MIN_SLASH, amount *     │   │
│  │ SLASH_BP / BP_PRECISION). Currently 10% of withdrawal.     │   │
│  │ Previously flat 1 DAI regardless of amount.                │   │
│  │                                                              │   │
│  │ If pool fails 1000 DAI claim (guaranteed withdrawal):     │   │
│  │   Total slash: 100 DAI (10% of 1000)                       │   │
│  │   Relayer 1 (50%): -50 DAI                                 │   │
│  │   Relayer 2 (30%): -30 DAI                                 │   │
│  │   Relayer 3 (20%): -20 DAI                                 │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  BENEFITS:                                                        │
│  ├── Higher coverage limits (combined stake)                       │
│  ├── Lower per-relayer capital requirement                        │
│  ├── Shared reputation (pool reputation)                          │
│  └── Ability to handle larger withdrawals                        │
│                                                                     │
│  RISKS:                                                           │
│  ├── One bad actor drags down entire pool                        │
│  ├── Free-rider problem (benefit from pool reputation)           │
│  └── Complexity of pool governance                               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Pool Formation

```rust
pub struct StakePool {
    pub pool_id: [u8; 32],
    pub members: Vec<PoolMember>,
    pub total_stake: Value,
    pub total_coverage: Value,  // Typically < total_stake
    pub governance: PoolGovernance,
}

pub enum PoolGovernance {
    // All members must agree to add/remove
    Unanimous,
    // Threshold of stake must agree
    Threshold(u8),  // e.g., 75% threshold
    // External oracle decides disputes
    Oracle(Pubkey),
}

pub fn create_pool(
    initial_members: Vec<(Pubkey, Value)>,
    governance: PoolGovernance,
) -> Result<StakePool> {
    // Verify all members consent
    // Initialize pool stake
    // Set coverage limits
}
```

### On-Chain Implementation

The [Pool Stake Contract](./pool_stake.md) provides the on-chain implementation of staking pools:

- **CreatePoolV1**: Create a new staking pool
- **JoinPoolV1**: Stakers join pool to provide coverage
- **LeavePoolV1**: Stakers exit after cooldown
- **AllocateCoverageV1**: Coverage allocated for guaranteed withdrawals
- **ReleaseCoverageV1**: Coverage released after successful execution
- **SlashCoverageV1**: Coverage slashed on failed withdrawal

## Capital Deployers

External capital providers can back relayers for yield:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Capital Deployer Model                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  CAPITAL DEPLOYER (Backer)                                          │
│  ├── Provides additional stake to relayer                          │
│  ├── Earns cut of relayer's fees (e.g., 10-20%)                  │
│  └── No operational responsibility                                 │
│                                                                     │
│  RELAYER                                                           │
│  ├── Controls operations                                          │
│  ├── Sets fees                                                    │
│  ├── Keeps majority of earnings                                    │
│  └── Repays backer from fees                                       │
│                                                                     │
│  USER                                                              │
│  ├── Sees relayer has strong stake backing                        │
│  └── Can verify backer stake via ZK proof                        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Economics

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Capital Deployer Economics                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Example:                                                         │
│  ────────                                                         │
│  Relayer has: 10,000 DAI own stake                                │
│  Capital deployer adds: 20,000 DAI backing                         │
│  Total coverage: 30,000 DAI                                       │
│                                                                     │
│  Revenue split:                                                   │
│  ├── Total fees earned: 100 DAI/month                             │
│  ├── Backer share (e.g., 15%): 15 DAI                            │
│  └── Relayer share: 85 DAI                                        │
│                                                                     │
│  Backer's perspective:                                            │
│  ├── 15 DAI / 20,000 DAI = 0.075% monthly yield                  │
│  ├── ~0.9% APY                                                    │
│  ├── Relayer reputation is key signal                             │
│  └── If relayer slashed, backer loses proportional stake         │
│                                                                     │
│  Relayer's perspective:                                           │
│  ├── Access to 3x more coverage capacity                          │
│  ├── Pays 15% of fees for capital access                         │
│  └── Maintains operational control                                │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### On-Chain Implementation

The [Relayer Endowment Contract](./endowment.md) provides the on-chain implementation of capital deployment:

- **InitializeV1**: Relayer initializes endowment account
- **DeployCapitalV1**: Backer deploys capital to relayer
- **WithdrawDeploymentV1**: Backer withdraws deployment + earnings
- **ClaimRelayerFeesV1**: Backer claims fee share
- **SettleFeesV1**: Relayer settles fees to backers
- **UpdateConfigV1**: Update fee configuration
- **ForceSettleV1** (May 2026): Backer force-settles fees after relayer inactivity timeout

### ZK Proof for Combined Stake

User can verify the relayer + backer combined stake:

```rust
// Relayer proves to user:
pub struct CombinedStakeProof {
    pub relayer_stake: Value,
    pub backer_stake: Value,
    pub backer_commitment: Commitment,  // H(backer_pubkey, amount)
    pub signature: Sig,
}

pub fn verify_combined_stake(
    proof: &CombinedStakeProof,
    withdrawal_amount: Value,
) -> bool {
    let total = proof.relayer_stake + proof.backer_stake;
    total >= withdrawal_amount  // Coverage check
}
```

## Betting Stake (Consensus via Market)

The [Betting Stake](../contract/betting_stake.md) contract can be used to create market-based consensus on relayer quality:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Betting Stake for Relayer Consensus               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Anyone can STAKE on a relayer's performance:                      │
│                                                                     │
│  BULL CASE (Relayer will perform well):                           │
│  ├── Stake DAI on "Relayer X succeeds withdrawal Y"              │
│  └── Earn payout if relayer delivers                              │
│                                                                     │
│  BEAR CASE (Relayer will fail):                                    │
│  ├── Stake DAI on "Relayer X fails withdrawal Y"                  │
│  └── Earn payout if relayer fails/timed out                       │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### How It Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Betting Flow                                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  BEFORE WITHDRAWAL:                                                │
│  ├── Bettors take positions                                       │
│  │   Bull: "Relayer will deliver on time"                        │
│  │   Bear: "Relayer will fail"                                   │
│  │                                                               │
│  │   Odds emerge from market (prediction market style)          │
│  └── Stakes locked until withdrawal resolves                      │
│                                                                     │
│  AFTER WITHDRAWAL RESOLUTION:                                      │
│  ├── If SUCCESS: Bull bettors win, bears lose                    │
│  └── If FAILURE: Bear bettors win, bulls lose                      │
│                                                                     │
│  ECONOMIC SIGNAL:                                                  │
│  ├── If many betting on failure → market doubts relayer          │
│  └── If few betting on failure → market trusts relayer            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Integration with Stake = Coverage

The betting stake creates an additional economic layer:

```
┌─────────────────────────────────────────────────────────────────────┐
│                Betting + Stake = Coverage                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Stake = Coverage: User's primary protection                       │
│  └── If relayer fails, user claims from stake                     │
│                                                                     │
│  Betting: Secondary market signal                                  │
│  └── If bettors think relayer will fail, they bet against        │
│      This creates price pressure on relayer to perform             │
│                                                                     │
│  COMPLEMENTARY EFFECTS:                                           │
│  ├── High failure bets → reputation damage                        │
│  │   → Users demand higher coverage                               │
│  │   → Relayer must stake more or leave market                   │
│  │                                                               │
│  ├── Consistent success → high bull bets                          │
│  │   → Reputation gains                                          │
│  │   → Can command premium fees                                  │
│  │                                                               │
│  └── Betting profits fund ecosystem participants                  │
│      → Economically aligned watchers monitor relayers              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Complete Economic Stack

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Complete Relayer Economic Stack                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  LAYER 1: STAKE (Foundation)                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Relayer stakes DAI + NETHER                                   │   │
│  │ ZK proof of stake to user                                    │   │
│  │ Stake = Coverage limit                                       │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                        │
│                              ▼                                        │
│  LAYER 2: FEED MARKET                                              │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Mode 1: Standard fee (no guarantee)                        │   │
│  │ Mode 2: Fee + premium (refund on failure)                   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                        │
│                              ▼                                        │
│  LAYER 3: POOLS & BACKING                                         │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Staking pools (shared stake, shared coverage)              │   │
│  │ Capital deployers (back relayers for yield)                │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                              │                                        │
│                              ▼                                        │
│  LAYER 4: BETTING (Market Consensus)                              │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ External bettors stake on relayer performance               │   │
│  │ Creates market-based quality signals                        │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Economic Properties

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Economic Properties                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  INCENTIVE ALIGNMENT:                                             │
│  ├── User: Pays for guaranteed delivery if needed                │
│  ├── Relayer: Earns fees, loses stake on failure                 │
│  ├── Backer: Earns yield for providing capital                  │
│  ├── Bettor: Profits from correctly predicting outcomes           │
│  └── Protocol: Collects fees, ensures liveness                   │
│                                                                     │
│  NO FREE RIDING:                                                  │
│  ├── Relayer must stake to operate                                │
│  ├── Backer stake is at risk                                      │
│  ├── Bettors put money where mouth is                             │
│  └── No one benefits without putting in capital                   │
│                                                                     │
│  MARKET DISCIPLINE:                                               │
│  ├── Bad relayers priced out (high premium, few takers)           │
│  ├── Good relayers command premium                                │
│  ├── Capital flows to productive relayers                         │
│  └── Consensus via betting markets                               │
│                                                                     │
│  RESILIENCE:                                                      │
│  ├── Multiple layers of protection (stake + pools + backing)      │
│  ├── No single point of failure                                   │
│  ├── Market can correct even if contracts fail                   │
│  └── Self-healing via economic incentives                        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Implementation Considerations

### What Lives On-Chain

```
┌─────────────────────────────────────────────────────────────────────┐
│                    On-Chain vs Off-Chain                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ON-CHAIN (DarkWow Contract) — Updated May 2026 (Phase 2d):         │
│  ├── Relayer registry (stake, status, reputation)                   │
│  ├── Relayer identity registration (RegisterRelayerV1)              │
│  ├── Stake locking/unlocking                                       │
│  ├── Withdrawal state machine (with reassignment)                  │
│  ├── Claim processing (proportional slashing)                      │
│  ├── Fee cap enforcement (MAX_FEE_BP)                               │
│  ├── Circuit breaker (GUARANTEED_PENDING counter)                   │
│  ├── Force settlement (backer-initiated fee distribution)           │
│  ├── Pool management with per-member slash tracking                 │
│  ├── Reputation-gated capital deployment (min_success_rate_bp,     │
│  │   max_slash_count thresholds)                                    │
│  ├── Fee schedule commitments via attestation (CommitFeeScheduleV1) │
│  └── Slash attestations with ZK proofs (AttestSlashV1)             │
│                                                                     │
│  OFF-CHAIN (Relayer Operations):                                   │
│  ├── Withdrawal execution (external chain)                         │
│  ├── Fee pricing decisions                                        │
│  ├── Backer negotiations                                          │
│  └── Operational decisions                                        │
│                                                                     │
│  BETTING CAN BE:                                                  │
│  ├── On DarkWow (native betting stake contract)                    │
│  └── Or external prediction market                                │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Open Questions

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Open Questions                                      │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  RESOLVED (May 2026 hardening):                                    │
│                                                                     │
│  ✓ FEE CAPS — Bridge enforces MAX_FEE_BP = 10%                    │
│  ✓ BACKER PROTECTION — ForceSettleV1 after 1000-block timeout      │
│  ✓ SLASH PROPORTIONALITY — Slash scales with withdrawal amount     │
│  ✓ WITHDRAWAL REASSIGNMENT — ReassignWithdrawalV1 for stuck txs    │
│  ✓ RELAYER IDENTITY — RegisterRelayerV1, register on-chain         │
│  ✓ PER-MEMBER SLASH TRACKING — PoolMemberStake.slash_count,       │
│    RebalancePoolSharesV1 adjusts shares by performance             │
│  ✓ FEE DISCOVERY — CommitFeeScheduleV1 + RegisterFeeScheduleV1     │
│  ✓ REPUTATION-GATED CAPITAL — DeployCapitalV1 accepts              │
│    min_success_rate_bp and max_slash_count thresholds              │
│  ✓ SLASH ATTESTATIONS — AttestSlashV1 ZK circuit for verifiable    │
│    slash history with privacy protection                           │
│                                                                     │
│  STILL OPEN:                                                        │
│                                                                     │
│  1. POOL GOVERNANCE                                               │
│     How to handle member disputes? Unanimous vs threshold?          │
│                                                                     │
│  2. BACKER RISK LIMITS                                            │
│     Should backers have maximum exposure per relayer?                │
│                                                                     │
│  3. BETTING INTEGRATION                                           │
│     Use existing betting_stake contract or build new?              │
│                                                                     │
│  4. POOL FORMATION                                                │
│     How to prevent pool from being dominated by one actor?         │
│                                                                     │
│  5. REPUTATION WEIGHTING (implemented, needs fine-tuning)          │
│     Current formula: adjusted_bp = base_share / (1 + slash_count) │
│     Future: integrate success_count, total_volume, frequency       │
│                                                                     │
│  6. MODE 2 PREMIUM CALCULATION                                     │
│     Fixed premium or market-determined?                            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Related Documentation

- [Slashing & Economic Security](./slashing.md) - Stake = Coverage model
- [Betting Stake](../contract/betting_stake.md) - Native betting contract
- [Bridge Architecture](./bridge.md) - Bridge and relayer integration
- [Relayer Documentation](../relayer/relayer.md) - Operational guide
