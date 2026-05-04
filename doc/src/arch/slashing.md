# Slashing & Economic Security

*The design space for economic incentives, slashing, and endowment models in DarkFi*

## Overview

As DarkFi's contract system grows in complexity—from simple atomic swaps to the bridge, DEX, and beyond—the question of **economic security** becomes central. How do we incentivize correct behavior? What happens when participants misbehave? And where do slashed funds go?

This page explores the **design space** for slashing and economic security, presenting the tradeoffs and open questions rather than prescribing single solutions.

## The Trust Spectrum

DarkFi operates across a trust spectrum:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    The Trust Spectrum                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  CRYPTOGRAPHIC                  ECONOMIC                            │
│  ─────────────                  ────────                            │
│                                                                     │
│  • ZK proof verification        • Slashing mechanisms              │
│  • Nullifiers                   • Endowment pools                   │
│  • Merkle proofs                • Reputation systems                │
│  • Hash locks                   • Bonding requirements              │
│                                                                     │
│  Properties:                    Properties:                         │
│  • Trustless execution          • Trust-but-verify                 │
│  • Math guarantees              • Economic incentives               │
│  • No trusted third party       • Penalties for misbehavior          │
│                                                                     │
│  Applicable to:                 Applicable to:                       │
│  • Deposit verification         • External chain execution           │
│  • State transitions            • Relayer services                  │
│  • Internal transfers           • Cross-chain operations            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### What Can Be Cryptographically Enforced

| Operation | Trust Model | Why |
|-----------|-------------|-----|
| Deposit to DarkFi | **Trustless** | ZK proof verifies without trusted party |
| Internal transfers | **Trustless** | Nullifiers prevent double-spend |
| Withdraw from DarkFi | **Trustless*** | User proves secret, but relayer must execute externally |
| External chain delivery | **Economic** | We cannot force Bitcoin to send somewhere |

*Withdrawals are trustless in the sense that if the relayer fails, the user can cancel and reclaim. But the relayer must still execute on the external chain.

## Misbehavior Vectors

Understanding what participants can do wrong is the first step to designing slashing.

### Relayer Misbehavior

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Relayer Misbehavior Vectors                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. WITHDRAWAL CENSORSHIP                                           │
│     Relayer selectively ignores withdrawals based on:                │
│     - Amount (large tx more profitable to delay)                     │
│     - Recipient (certain addresses flagged)                         │
│     - Token type (lower fee tokens ignored)                          │
│     Mitigation: Multiple competing relayers                           │
│                                                                     │
│  2. FRONT-RUNNING (Generally Not Possible)                         │
│     Bridge HTLC design binds secret to specific withdrawal.         │
│     Cannot redirect funds to different address.                      │
│     Exception: Correlation attacks on timing                         │
│                                                                     │
│  3. FALSE REPORTING                                                 │
│     Claim transaction failed when it actually succeeded.             │
│     Claim success when transaction still pending.                     │
│     Mitigation: External chain verification (trust but verify)       │
│                                                                     │
│  4. FEE SKIMMING                                                    │
│     Take more than the agreed-upon fee percentage.                   │
│     Mitigation: Fee enforced on-chain at withdrawal submission       │
│                                                                     │
│  5. GRIEFING / TIMEOUT EXPLOITATION                                │
│     Execute withdrawal at the last possible moment.                  │
│     Stress users who expect prompt execution.                        │
│     Mitigation: Time preference competition among relayers           │
│                                                                     │
│  6. REPLAY ATTACKS                                                  │
│     Submit same withdrawal multiple times.                           │
│     Mitigation: Nullifiers prevent replay                           │
│                                                                     │
│  7. WRONG EXECUTION                                                 │
│     Execute to wrong address or wrong amount.                        │
│     Mitigation: User can cancel timed-out withdrawals                │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### User Misbehavior

```
┌─────────────────────────────────────────────────────────────────────┐
│                    User Misbehavior Vectors                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. FALSE CLAIMS                                                   │
│     Claim relayer failed when withdrawal was executed.               │
│     Mitigation: External chain state verification                   │
│                                                                     │
│  2. SYBIL ATTACKS                                                  │
│     Create many identities to spam relayers.                        │
│     Mitigation: Minimum withdrawal amounts, spam costs              │
│                                                                     │
│  3. TIMING EXPLOITS                                                │
│     Submit withdrawal, then cancel right before timeout.             │
│     Grief relayer who prepared execution.                            │
│     Mitigation: Commitment mechanisms, cancellation fees             │
│                                                                     │
│  4. ZK PROOF FORGERY (Not Possible)                                │
│     Submit invalid proof to steal funds.                            │
│     Mitigation: Cryptographic verification                           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Slashing Design Space

### Option 1: Burn All Slashed Funds

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Burn Model                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  SlashEvent → Funds removed from circulation                         │
│                                                                     │
│  Pros:                                                             │
│  • Creates deflation pressure (token value increase)                 │
│  • Simple implementation                                            │
│  • No management overhead                                           │
│                                                                     │
│  Cons:                                                             │
│  • Victim gets nothing                                              │
│  • Economic loss is socialized to all token holders                 │
│  • No direct incentive to avoid being slashed                       │
│  • Funds lost permanently                                           │
│                                                                     │
│  Use when:                                                         │
│  • Slashing is rare (high trust environment)                        │
│  • Deflation is desired                                            │
│  • Compensation infrastructure is complex                           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Option 2: Endowment Pool

```
┌─────────────────────────────────────────────────────────────────────┐
│                      Endowment Model                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  SlashEvent → Funds to endowment pool → Pay victims                  │
│                                                                     │
│  Pros:                                                             │
│  • Victims get compensated                                          │
│  • Relayers have skin in the game                                  │
│  • Creates insurance-like mechanism                                 │
│  • Economic loss localized to relayer network                       │
│                                                                     │
│  Cons:                                                             │
│  • Pool management overhead                                        │
│  • Governance needed (who approves payouts?)                        │
│  • Potential for pool drain attacks                                 │
│  • Need claim verification process                                  │
│                                                                     │
│  Use when:                                                         │
│  • High-value transactions common                                   │
│  • Trust model requires economic guarantees                         │
│  • User expectation of recourse                                    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Option 3: Stake = Coverage Limit (Recommended for Relayers)

This is the recommended model for DarkFi relayers. It is simple, elegant, and avoids the complexity of pool governance:

```
┌─────────────────────────────────────────────────────────────────────┐
│                Stake = Coverage Limit Model                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Core Principles:                                                  │
│                                                                     │
│  1. RELAYER STAKE = MAXIMUM BRIDGING CAPACITY                      │
│     Relayer can only have withdrawals in-flight up to their stake.   │
│                                                                     │
│  2. ZK PROOF OF STAKE                                             │
│     User verifies relayer has sufficient stake before accepting.     │
│                                                                     │
│  3. DIRECT CLAIM ON STAKE                                          │
│     On failure, user can claim directly from relayer's stake.      │
│                                                                     │
│  4. SELF-RECOVERY                                                 │
│     Relayer recovers user's funds, rebuilds stake.                  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### How It Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Stake = Coverage Flow                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. RELAYER STAKES                                                 │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ Relayer deposits DAI + NETHER into staking contract.        │ │
│     │ Stake is locked, cannot be withdrawn while active.         │ │
│     │ Example: 10,000 DAI + 5,000 NETHER staked                 │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                              │                                        │
│                              ▼                                        │
│  2. STAKE PROOF TO USER                                             │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ User requests withdrawal                                    │ │
│     │ Relayer sends ZK proof: "My stake is 10,000 DAI equivalent"│ │
│     │ User verifies proof on-chain                                │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                              │                                        │
│                              ▼                                        │
│  3. WITHDRAWAL EXECUTES                                           │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ Relayer executes withdrawal on external chain               │ │
│     │ Withdrawal amount ≤ Relayer's available stake              │ │
│     │ Stake is held as coverage during withdrawal window          │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                              │                                        │
│                              ▼                                        │
│  4. SUCCESS → STAKE RELEASED                                       │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ User confirms receipt on external chain                     │ │
│     │ Relayer's stake released for new withdrawals                │ │
│     │ Relayer earned fee                                         │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ON FAILURE:                                                       │
│                              │                                        │
│                              ▼                                        │
│     ┌─────────────────────────────────────────────────────────────┐ │
│     │ User submits claim after timeout                             │ │
│     │ "Relayer R did not deliver withdrawal W"                   │ │
│     │ Proof: tx hash not found on external chain                  │ │
│     │                                                           │ │
│     │ Slashed stake transfers to user                             │ │
│     │ Relayer keeps residual (if any)                            │ │
│     │ Relayer must now recover user's funds externally             │ │
│     │ Then can rebuild stake to resume operations                 │ │
│     └─────────────────────────────────────────────────────────────┘ │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### Why This Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Why Stake = Coverage Works                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ✓ NO GOVERNANCE NEEDED                                            │
│    User claims directly from stake. No endowment pool to manage.    │
│                                                                     │
│  ✓ NO COMPLEX PAYOUT RATIOS                                        │
│    Full compensation because stake = coverage limit.                │
│                                                                     │
│  ✓ SELF-BALANCING                                                  │
│    If stake is too low, relayer can't take large withdrawals.       │
│    Economic pressure to maintain sufficient stake.                   │
│                                                                     │
│  ✓ INCENTIVE TO RECOVER                                            │
│    Relayer must recover user's funds to rebuild business.           │
│    No central fund to bail out, relayer is on the hook.            │
│                                                                     │
│  ✓ NO ENDOULMENT SURPLUS PROBLEM                                   │
│    Funds sitting idle in pool? Stake less next time.               │
│                                                                     │
│  ✓ FALSE CLAIMS DIFFICULT                                          │
│    External chain state is verifiable.                              │
│    Relayer can contest with tx proof.                              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### Stake Asset Selection

The stake should be in **stable, liquid assets** to avoid volatility issues:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Stake Asset Requirements                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  NEEDED:                                                           │
│  • Stable value (not volatile)                                     │
│  • Liquid (easy to acquire/exit)                                   │
│  • Widely available                                                │
│                                                                     │
│  RECOMMENDED FOR DARKFI RELAYERS:                                  │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ DAI (External)           │ NETHER (DarkFi Native)          │   │
│  │ • USD pegged             │ • DarkFi's stablecoin           │   │
│  │ • Deep liquidity        │ • Native to ecosystem           │   │
│  │ • Battle-tested         │ • Aligns relayer with protocol  │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  WHY NOT VOLATILE TOKENS?                                          │
│  • Stake could become insufficient if token price drops            │
│  • User might not be fully covered                                 │
│  • Complexity of oracle-based adjustment                           │
│                                                                     │
│  RATIO SUGGESTION:                                                │
│  • 50% DAI + 50% NETHER (or similar)                             │
│  • Balances external stability with protocol alignment              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

#### Implementation Sketch

```rust
// Relayer Stake Contract
pub struct RelayerStake {
    pub relayer: Pubkey,
    pub dai_amount: u64,
    pub nether_amount: u64,
    pub active_withdrawals: Vec<ActiveWithdrawal>,
    pub status: RelayerStatus,
}

pub struct ActiveWithdrawal {
    pub withdrawal_id: [u8; 32],
    pub amount: u64,
    pub asset: AssetId,
    pub locked_until: u64,
}

pub fn verify_stake_proof(
    stake: &RelayerStake,
    withdrawal_amount: u64,
) -> bool {
    let available = calculate_available_stake(stake);
    available >= withdrawal_amount
}

pub fn claim_failed_withdrawal(
    stake: &mut RelayerStake,
    claim: &ClaimProof,
) -> Result<()> {
    // Verify:
    // 1. Withdrawal was in active_withdrawals
    // 2. Timeout has passed
    // 3. Tx hash not found on external chain (verified externally)

    let slash_amount = calculate_slash(claim);
    transfer(slash_amount).to(claim.user);

    // Relayer keeps residual
    stake.dai_amount -= slash_amount;
}
```

#### Comparison with Endowment

| Aspect | Endowment Pool | Stake = Coverage |
|--------|----------------|------------------|
| Governance | Who decides payouts? | None needed |
| Complexity | High (payout ratios, pool health) | Low |
| Payout | Partial (ratio < 100%) | Full (stake = coverage) |
| Pool surplus | What to do with excess? | Not applicable |
| False claims | Pool drains if too many | Individual relayer bears cost |
| Recovery | Protocol-wide recovery | Relayer-specific |
| New relayer entry | Pays into pool | Only stakes for themselves |

## The Endowment Model: Detailed Considerations

## The Endowment Model: Detailed Considerations

This model is gaining adoption in DeFi for good reason, but several design decisions must be made:

### Governance of Payouts

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Governance Options                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. AUTOMATED ORACLE                                               │
│     External oracle verifies claims and triggers payout.             │
│     Pros: Fast, no human intervention                                │
│     Cons: Oracle trust required, potential for oracle manipulation   │
│                                                                     │
│  2. DAO VOTING                                                     │
│     Token holders vote on disputed claims.                           │
│     Pros: Decentralized, democratic                                 │
│     Cons: Slow, voter apathy, potential capture                      │
│                                                                     │
│  3. ARBITRATION COURTS                                             │
│     Kleros or similar dispute resolution.                            │
│     Pros: Established mechanism, specialized                         │
│     Cons: Cost, appeal complexity                                   │
│                                                                     │
│  4. INSURANCE PANEL                                                │
│     Elected/selected experts review claims.                         │
│     Pros: Accountability, expertise                                 │
│     Cons: Selection process, potential corruption                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Payout Ratio

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Payout Ratio Considerations                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  100% Payout:                                                     │
│  • Full compensation for users                                      │
│  • Users fully protected                                            │
│  • Risk: Pool may not be sustainable                                │
│                                                                     │
│  50-80% Payout:                                                   │
│  • User bears some risk                                             │
│  • Pool more sustainable                                            │
│  • Incentive for users to verify success                            │
│                                                                     │
│  Sliding Scale:                                                   │
│  • Based on pool health                                            │
│  • Higher payout when pool is healthy                              │
│  • Lower payout when pool stressed                                  │
│                                                                     │
│  Cap on Payouts:                                                  │
│  • Per-incident cap (e.g., max 1000 DARK per claim)               │
│  • Total pool cap (prevents catastrophic drain)                     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Entry Requirements

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Entry Threshold Considerations                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Low Threshold (100-1000 DARK):                                    │
│  • More relayers = more competition                                 │
│  • Lower barrier = more inclusion                                   │
│  • Risk: Sybil attacks                                             │
│                                                                     │
│  High Threshold (10000+ DARK):                                     │
│  • Serious participants only                                        │
│  • More economic security                                           │
│  • Risk: Centralization                                            │
│                                                                     │
│  Per-Chain Thresholds:                                             │
│  • Different chains have different risk profiles                    │
│  • ETH/AZT = higher (more value)                                  │
│  • LTC = lower (lower value per tx)                                │
│                                                                     │
│  Graduated Entry:                                                 │
│  • Start with lower stake, increase over time                      │
│  • Prove track record before full privileges                        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### False Claim Prevention

```
┌─────────────────────────────────────────────────────────────────────┐
│                    False Claim Prevention                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Problem: User claims relayer failed when tx succeeded.            │
│                                                                     │
│  Solutions:                                                        │
│                                                                     │
│  1. EXTERNAL VERIFICATION                                          │
│     Require tx hash on claim. Verify on external chain.             │
│                                                                     │
│  2. TIMESTAMP CHECKS                                               │
│     Withdrawal must be pending for minimum time before claim.       │
│     Prevents griefing relayers who haven't had time to execute       │
│                                                                     │
│  3. RELAYER DEFENSE WINDOW                                         │
│     Relayer can contest claim within grace period.                   │
│                                                                     │
│  4. REPUTATION BONDING                                             │
│     Users with false claims get flagged.                            │
│     Future claims scrutinized more heavily.                          │
│                                                                     │
│  5. SLASHING FALSE CLAIMERS                                        │
│     If claim is proven false, user loses bond.                      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Open Questions

The following are genuine design questions without clear right answers. Different projects have made different choices based on their use cases:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Open Questions                                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. WHO GOVERNS THE ENDOWMENT?                                     │
│     Automated, DAO, oracle, or multi-sig?                          │
│     Tradeoff: Speed vs decentralization vs trust                    │
│                                                                     │
│  2. SHOULD SLASHED FUNDS BURN OR ENDOW?                           │
│     Burn favors token holders. Endow favors victims.                │
│     Tradeoff: Efficiency vs fairness                               │
│                                                                     │
│  3. WHAT IS THE RIGHT PAYOUT RATIO?                                 │
│     100% fully protects users. Partial reduces pool drain risk.    │
│     Tradeoff: User protection vs sustainability                    │
│                                                                     │
│  4. HOW TO HANDLE FALSE CLAIMS?                                    │
│     Slash users who make false claims?                             │
│     Tradeoff: Prevents abuse vs creates friction                   │
│                                                                     │
│  5. SHOULD RELAYERS HAVE DIFFERENT TIERS?                         │
│     Higher stake = lower fees = more business                     │
│     Tradeoff: Competition vs commitment                            │
│                                                                     │
│  6. HOW TO HANDLE ENDOWMENT SURPLUS?                              │
│     If pool grows too large, redistribute to relayers?             │
│     Tradeoff: Incentivizes joining vs maintaining safety buffer   │
│                                                                     │
│  7. CROSS-CHAIN SLASHING COORDINATION?                             │
│     If relayer on Chain A misbehaves affecting Chain B?            │
│     Tradeoff: Complexity vs comprehensive enforcement              │
│                                                                     │
│  8. INSURANCE MODEL VS BONDING MODEL?                             │
│     Insurance: Relayers pay premiums to pool                       │
│     Bonding: Direct stake is slashed                               │
│     Tradeoff: Risk distribution vs simplicity                      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Design Principles

Regardless of specific implementation choices, some principles seem broadly applicable:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Design Principles                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. INCENTIVE COMPATIBILITY                                       │
│     Participant behavior should align with protocol health.          │
│     Misbehavior should be more costly than correct behavior.        │
│                                                                     │
│  2. COMPENSATION BEFORE PUNISHMENT                                │
│     Focus on making victims whole, not punishing offenders.        │
│     Punishment without compensation is less effective deterrent.     │
│                                                                     │
│  3. PROPORTIONALITY                                                │
│     Slash should fit the crime. Small offenses get small slashes.   │
│     Catastrophic offenses can be larger but shouldn't be fatal.     │
│                                                                     │
│  4. DUE PROCESS                                                    │
│     Participants should have ability to contest accusations.         │
│     Fast resolution without sacrificing fairness.                    │
│                                                                     │
│  5. TRANSPARENCY                                                   │
│     Slashing events, pool status, claim outcomes publicly visible.  │
│     Builds trust in the system.                                    │
│                                                                     │
│  6. MINIMUM VIABLE COMPLEXITY                                      │
│     Start simple, add complexity only when needed.                  │
│     Each additional mechanism is itself a trust assumption.         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Related Documentation

- [Bridge Architecture](../contract/bridge.md) - How the bridge works with relayers
- [Relayer Economics](../relayer/relayer_economics.md) - Feed markets, staking pools, capital deployment
- [DAO Architecture](../contract/dao.md) - Potential governance for endowment
- [Economic Security](./economic_security.md) - (when added) Broader economic models
- [Trust Models](./trust_models.md) - (when added) Trust spectrum discussion

## Further Reading

- [Mechanism Design Basics](https://en.wikipedia.org/wiki/Mechanism_design) - Academic foundation
- [Schelling Coin](https://en.wikipedia.org/wiki/Schelling_coin) - Oracle design pattern
- [Futarchy](https://en.wikipedia.org/wiki/Futarchy) - Prediction market governance
- [Bonding Curves](https://en.wikipedia.org/wiki/Bonding_curve) - Token incentive structures
