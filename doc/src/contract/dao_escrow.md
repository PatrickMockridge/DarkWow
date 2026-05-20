# DAO-Escrow Contract

A flexible contract supporting three operating modes: **Escrow-Only**, **Treasury-Only**, and **Treasury+Endowment**, with **OCap-based governance** for proposal, voting, execution, and multi-oracle dispute resolution.

## Three Operating Modes

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              DAO-Escrow Modes                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  MODE_ESCROW (0x00) ────────────────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Escrow-Only (Pure Insurance Pool)                                   │    │
│  │                                                                       │    │
│  │  Members ──pay premiums──► Endowment Pool                             │    │
│  │                                    │                                 │    │
│  │                                    │ DAO votes                      │    │
│  │                                    ▼                                 │    │
│  │                            Claims paid out                           │    │
│  │                                                                       │    │
│  │  No treasury. No operational costs. Pure mutual insurance.          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  MODE_TREASURY (0x01) ───────────────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Treasury-Only (Same as DarkWow DAO)                                  │    │
│  │                                                                       │    │
│  │  Members ──pay fees──► Treasury Pool                                 │    │
│  │                                  │                                   │    │
│  │                    ┌─────────────┼─────────────┐                    │    │
│  │                    │   Propose   │   Vote     │   Exec              │    │
│  │                    └─────────────┴─────────────┘                    │    │
│  │                                  │                                   │    │
│  │                                  ▼                                   │    │
│  │                          Treasury spent                              │    │
│  │                                                                       │    │
│  │  Grants, development, operational costs. No insurance.               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  MODE_TREASURY_ENDOWMENT (0x02) ────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Treasury + Endowment (Full-Featured)                                │    │
│  │                                                                       │    │
│  │  Members ──pay premiums──► ┌──────────┬──────────┐                 │    │
│  │                            │          │          │                     │    │
│  │              treasury_share %          % endowment_share                 │    │
│  │                            │          │          │                     │    │
│  │                            ▼          ▼          │                     │    │
│  │                      Treasury      Endowment      │                     │    │
│  │                         │             │          │                     │    │
│  │                         │             │ DAO vote │                     │    │
│  │                         ▼             ▼          │                     │    │
│  │                    Operational    Claims/          │                     │    │
│  │                    (grants etc)  Refunds          │                     │    │
│  │                                                                       │    │
│  │  Best of both: DAO-funded operations + insurance backing.           │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Mode Comparison

| Feature | MODE_ESCROW | MODE_TREASURY | MODE_TREASURY_ENDOWMENT |
|---------|-------------|---------------|------------------------|
| Membership notes | ✅ | ❌ | ✅ |
| Endowment pool | ✅ | ❌ | ✅ |
| Treasury pool | ❌ | ✅ | ✅ |
| DAO governance | ✅ | ✅ | ✅ |
| OCap governance | ✅ | ✅ | ✅ |
| Insurance payouts | ✅ | ❌ | ✅ |
| Operational funding | ❌ | ✅ | ✅ |
| Fee split | N/A | N/A | ✅ (configurable) |

## Entrypoints

### Core Functions (0x00-0x06)

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Create new DAO-Escrow (mode selected) |
| `UpdateV1` | `0x01` | Update parameters |
| `PayPremiumV1` | `0x02` | Pay premium, get membership note |
| `WithdrawV1` | `0x03` | Withdraw with capability-gated authorization |
| `EndowmentWithdrawV1` | `0x04` | Endowment withdrawal (capability or proposal) |
| `TreasurySpendV1` | `0x05` | Treasury spending (capability or proposal) |
| `EnableDrainProtectionV1` | `0x06` | Enable DrainProtection on existing DAO-Escrow |

### OCap Governance Functions (0x07-0x0e)

| Function | Opcode | Capability Required | Description |
|----------|--------|--------------------|-------------|
| `ProposeClaimV1` | `0x07` | `member_vote` | Propose claim (endowment/treasury/dispute) |
| `VoteClaimV1` | `0x08` | `member_vote` | Vote on pending proposal |
| `ExecuteClaimV1` | `0x09` | None (quorum is authority) | Execute approved proposal |
| `RegisterCapabilityRequirementV1` | `0x0a` | `board_treasury` | Map role to Identity contract capability |
| `VerifyMemberCapabilityV1` | `0x0b` | None (this IS verification) | Verify holder possesses a capability |
| `ResolveDisputeV1` | `0x0c` | `dispute_arbitrator` | Multi-oracle dispute resolution |
| `CancelClaimV1` | `0x0d` | Proposer identity match | Cancel pending claim |
| `SetGovernanceConfigV1` | `0x0e` | `board_treasury` | Update governance configuration |

## OCap Governance Model

DAO-Escrow uses **capability-based governance** via cross-contract verification with the [Identity contract](../../src/contract/identity/README.md). Authority is proven ("I hold capability X") rather than asserted ("I am user Y").

### Capability Types

| Capability | Issued By | Purpose |
|-----------|-----------|---------|
| `member_vote` | Identity contract | Basic voting on proposals |
| `board_treasury` | Identity contract | Treasury release control |
| `board_endowment` | Identity contract | Endowment release control |
| `dispute_arbitrator` | Identity contract | Dispute resolution via oracle attestation |

### Governance Lifecycle

```
1. Identity contract issues capabilities to holders
2. dao_escrow registers capability requirements (0x0a)
3. Member proposes claim with member_vote capability proof (0x07)
4. Members vote with member_vote capability proof (0x08)
5. When quorum + approval ratio met → execute claim (0x09)
6. Disputes resolved by arbitrator with multi-oracle attestation (0x0c)
```

### Backward Compatibility

A `governance_active: bool` flag acts as a feature toggle:
- **`false`** (default): Capability checks bypassed, existing owner-pubkey behavior preserved
- **`true`**: Capability proofs mandatory for all protected operations

### Access Control Comparison

| Action | Governance Inactive | Governance Active |
|--------|--------------------|--------------------|
| Withdraw | Owner pubkey | `board_treasury` capability |
| Endowment withdraw | Open (any caller) | `board_endowment` capability or approved proposal |
| Treasury spend | Open (any caller) | `board_treasury` capability or approved proposal |
| Propose claim | Open | `member_vote` capability + ZK proof |
| Vote | Open | `member_vote` capability + ZK proof |
| Dispute resolution | N/A | `dispute_arbitrator` capability + multi-oracle attestation |

## Dispute Resolution Flow

Disputes are resolved via multi-oracle attestation with an arbitrator:

```
1. Off-chain event → Oracle(s) push values (Oracle::PushValueV1)
2. Oracle(s) create attestations (Attestation::CreateAttestationV1)
3. Arbitrator calls ResolveDisputeV1 with:
   - Multiple oracle attestation references
   - dispute_arbitrator capability proof (ZK)
   - Payout amount + recipient
4. Contract verifies:
   a. Arbitrator capability via Identity::VerifyCapabilityV1 (child call)
   b. Multi-oracle threshold met (e.g., 3/5 oracles attested)
   c. Consumes attestations to prevent replay
   d. Transfers funds via money_v3::transfer_v1 (child call)
```

## Fee Split (TreasuryEndowment Mode)

When members pay premiums:

```
┌──────────────────────────────────────────────────────────────┐
│                        Premium Payment                        │
│                        (e.g., 1000)                           │
└──────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
    treasury_share (70%)             endowment_share (30%)
              │                               │
              ▼                               ▼
    ┌─────────────────────┐         ┌─────────────────────┐
    │      Treasury       │         │      Endowment      │
    │  (operational)      │         │    (insurance)      │
    │                     │         │                     │
    │  - Grants           │         │  - Refunds          │
    │  - Development      │         │  - Claims           │
    │  - Operations       │         │  - Emergency        │
    └─────────────────────┘         └─────────────────────┘
```

## ZK Circuits

| Circuit | Public Inputs | Status |
|---------|--------------|--------|
| `init_v1.zk` | `dao_bulla`, `endowment_bulla` | Compiled |
| `pay_premium_v1.zk` | `dao_escrow_bulla`, `membership_note`, `value_commit.x`, `value_commit.y` | Compiled |
| `propose_claim_v1.zk` | `dao_escrow_bulla`, `claim_id`, `capability_id`, `proposal_nullifier`, `claim_commit` | Source complete |
| `vote_claim_v1.zk` | `proposal_id`, `capability_id`, `vote_nullifier`, `vote_commit.x`, `vote_commit.y` | Source complete |
| `verify_member_capability_v1.zk` | `capability_id`, `dao_escrow_bulla`, `holder_commit` | Source complete |
| `resolve_dispute_v1.zk` | `capability_id`, `dao_escrow_bulla`, `dispute_id`, `attestation_root`, `resolution_commit`, `dispute_nullifier` | Source complete |

## Database Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract version and configuration |
| `bullas` | Endowment instances |
| `membership` | Time-limited membership notes |
| `endowment` | Endowment pool funds |
| `proposals` | Governance proposals/claims |
| `votes` | Vote records per proposal |
| `capability_requirements` | Required capability IDs per role |
| `disputes` | Dispute resolution records |
| `nullifiers` | Prevents double-vote, double-propose |
| `governance` | Governance configuration |

## Trust Model

| Aspect | How It's Protected |
|--------|-------------------|
| Treasury funds | OCap governance (propose/vote/exec) or DrainProtection |
| Endowment | Capability-gated withdrawal or approved proposal required |
| Membership notes | Block-based expiry enforced in circuit |
| Double-spend | Nullifiers prevent redemption twice |
| Double-vote | Vote nullifier = H(capability_secret, proposal_id) |
| Dispute replay | Dispute nullifier = H(capability_secret, dispute_id) |
| Mass exit / drain | Optional DrainProtection with rate limiting and exit queue |

## DrainProtection Integration

DAO-Escrow can integrate with the [DrainProtection contract](drain_protection.md) for governance-level fund protections.

```rust
// During initialization
let dao_escrow = InitializeBuilder::new()
    .mode(DaoEscrowMode::TreasuryEndowment)
    .enable_drain_protection(true)
    .build()?;

// Or enable later via governance
let enable_dp = EnableDrainProtectionBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .drain_protection_bulla(dp_instance.bulla)
    .build()?;
```

## Build & Test Status

```
Build:    ✅ Clean (zero warnings)
Tests:    ✅ 20/20 integration tests passing
Circuits: 2 compiled, 4 source complete (needs zkas compilation)
```

## See Also

- [DAO-Escrow Contract README](../../src/contract/dao_escrow/README.md)
- [Identity Contract README](../../src/contract/identity/README.md)
- [O-Cap Architecture](../arch/ocap.md)
- [Identity Architecture](../arch/identity.md)
- [Subscription Contract](subscription.md)
- [DrainProtection Contract](drain_protection.md)
