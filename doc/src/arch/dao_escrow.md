# DAO-Escrow Contract (Simplified MVP)

Community insurance via DAO-controlled endowment. **Claims are handled by DAO treasury — no parallel voting needed.**

## The Problem: Community Insurance Needs Governance + Trustlessness

Traditional insurance:
- **Centralized**: Single company controls everything
- **Opaque**: Premium calculations and claims decisions are opaque
- **Counterparty risk**: Company can deny claims or go bankrupt

Pure escrow:
- **Trustless**: No single party controls funds
- **Inflexible**: No voting/discretion on edge cases
- **Timeout-based**: Refund is automatic, not discretionary

**What if the DAO controlled the escrow, using its existing voting mechanism?**

## Simplified Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                  DAO-Escrow Architecture (Simplified)               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   ┌─────────────┐                                                    │
│   │  Members     │ ──pay premiums──> ┌─────────────────┐            │
│   │              │                   │  Endowment Pool  │            │
│   │  (pay annual │                   │                 │            │
│   │   premium)   │                   │  (membership    │            │
│   └─────────────┘                   │   notes issued)  │            │
│                                      └────────┬────────┘            │
│                                              │                       │
│                           ┌─────────────────┼─────────────────┐    │
│                           │                 │                   │    │
│                           │    DAO Treasury │ Management       │    │
│                           │    (propose/   │ (via existing    │    │
│                           │     vote/exec) │  DAO governance) │    │
│                           │                 │                   │    │
│                           └─────────────────┴─────────────────┘    │
│                                               │                       │
│                              Claims against endowment:               │
│                              DAO votes → treasury releases funds     │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

**Key simplification**: Claims are NOT handled by DAO-Escrow. They're handled by the DAO's existing treasury management. DAO-Escrow is just:
1. An endowment pool linked to a DAO
2. Issues membership notes when premiums are paid

## How It Works

### 1. Create DAO-Escrow Endowment

Owner creates an endowment linked to an existing DAO:
```rust
// Endowment is controlled by a DAO
let endowment = InitializeBuilder::new()
    .dao_bulla(existing_dao.bulla)  // Links to existing DAO
    .owner_secret(owner_secret)
    .endowment_token_id(DRK_TOKEN)
    .build()?;
```

### 2. Members Pay Premiums → Get Membership Note

Members pay annual premiums and receive a time-limited membership note:
```rust
// Pay premium, get membership note valid for ~1 year
let premium = PayPremiumBuilder::new()
    .dao_escrow_bulla(endowment.bulla)
    .member_secret(member_secret)
    .value(100)                    // Premium amount
    .token_id(DRK_TOKEN)
    .expiry(current_block + 52500) // ~1 year
    .build()?;
```

The membership note has a `spend_hook` that checks `current_block < expiry`.

### 3. Claims → DAO Treasury (NOT DAO-Escrow)

Claims against the endowment are handled by the DAO's existing treasury:
- **Propose**: Someone proposes a disbursement (DAO::Propose)
- **Vote**: DAO members vote (DAO::Vote)
- **Exec**: If approved, funds released (DAO::Exec + AuthMoneyTransfer)

The DAO-Escrow endowment is just a pool of funds. The DAO votes on how to allocate it.

## Why This Simplification?

**Original design had problems**:
- Building parallel voting mechanism = lots of new circuits
- Vote aggregation is complex (SMT, nullifiers, tallying)
- Merkle proofs for premium tracking

**Simplified design reuses**:
- DAO's existing `Propose`/`Vote`/`Exec` flow
- DAO's existing `AuthMoneyTransfer` for fund release
- No new voting circuits needed

**Result**: DAO-Escrow is now just:
1. Initialize endowment + link to DAO
2. Pay premium + get membership note
3. Claims handled by DAO treasury (existing code)

## Trust Model: DAO as Escrow Oracle

| Aspect | Traditional Insurance | Pure Escrow | DAO-Escrow (Simplified) |
|--------|---------------------|-------------|------------------------|
| **Who decides claims** | Company alone | Algorithm alone | DAO vote |
| **Funds control** | Company | Smart contract | Smart contract |
| **Premium → membership** | Underwriting | None | Time-limited note |
| **Claims handling** | Company | Pre-defined | DAO treasury |
| **Edge cases** | Company discretion | None | DAO discretion |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Create endowment linked to a DAO |
| UpdateV1 | 0x01 | Update endowment params |
| PayPremiumV1 | 0x02 | Pay premium, get membership note |
| WithdrawV1 | 0x03 | Owner withdraws fees |

**Claims are NOT handled here** — they're handled by the DAO's treasury management.

## ZK Circuits (Simplified)

### init_v1.zk

Proves endowment is linked to a DAO:
- **Public inputs**: `dao_bulla`, `endowment_bulla`
- **Private inputs**: `owner_secret`, `endowment_token_id`, `bulla_blind`
- **Verification**: `endowment_bulla = H(dao_bulla, owner_pub, token_id, blind)`

### pay_premium_v1.zk

Proves premium payment and creates membership note:
- **Public inputs**: `dao_escrow_bulla`, `membership_note`, `value_commit.x`, `value_commit.y`
- **Private inputs**: `member_secret`, `value`, `token_id`, `expiry`, `membership_blind`, `value_blind`
- **Verification**:
  - Member key derivation
  - Membership note commitment
  - Value commitment (Pedersen)

**No new opcodes needed!** Both circuits use only proven opcodes.

## Opcode Requirements

| Circuit | Opcodes Used | Status |
|---------|-------------|--------|
| `init_v1.zk` | `poseidon_hash`, `ec_mul_base`, `ec_get_x`, `ec_get_y` | Proven |
| `pay_premium_v1.zk` | `poseidon_hash`, `ec_mul_base`, `ec_mul_short`, `ec_mul`, `ec_add`, `ec_get_x`, `ec_get_y` | Proven |

**No experimental opcodes. No grey-market concerns.**

## What DAO-Escrow IS NOT

This is NOT a standalone voting system. It does NOT have:
- Its own voting mechanism
- Its own proposal/execute flow
- Vote aggregation circuits

Instead, it:
- Holds an endowment pool
- Issues membership notes
- Lets the DAO control fund release

## Integration

### With Money Contract
- Premiums paid via Money::Transfer
- Membership notes have `spend_hook` checking expiry
- Endowment funds managed via Money contract

### With DAO Contract
- Endowment is controlled by a DAO
- Claims go through DAO::Propose/Vote/Exec
- Funds released via DAO::AuthMoneyTransfer

## MVP Status

**Simplified MVP** — Delegated voting to DAO treasury.

## Roadmap: Extended Insurance Models

The MVP delegates voting to DAO treasury. Future extensions can add more democratic rights and cooperative governance, subject to opcode availability.

### Level 1: Endowment DAO (Current MVP)

- Premium → membership note (annual)
- Claims handled by DAO treasury
- No parallel voting in DAO-Escrow

### Level 2: Claims DAO (Requires Vote Aggregation)

Add dedicated claim voting within DAO-Escrow:

```
Claims ──> Vote ──> Approved/Rejected
```

**Opcode requirements**:
- Vote aggregation via cross-multiplication (exists in DAO `exec.zk`)
- SMT for vote nullifiers (exists in DAO `vote-input.zk`)

**Opcode barriers**: None for basic voting. `LessThanOrEqual` may be needed for advanced ratio checks.

### Level 3: Multi-tier Governance

Add differentiated rights:

| Role | Rights |
|------|--------|
| Members | Pay premiums, vote on claims |
| Delegates | Vote on policy changes |
| Trustees | Emergency interventions, appeals |

**Opcode requirements**:
- Role-based access via Merkle membership proofs
- Weighted voting (delegates have more weight)

**Opcode barriers**: None identified.

### Level 4: Democratic Policy Changes

Allow members to vote on:

- Premium rates
- Coverage scope
- Claim criteria
- Investment strategy

**Opcode requirements**:
- Policy commitment via poseidon_hash
- Policy change requires quorum + approval ratio

**Opcode barriers**: None identified.

### Level 5: Mutual Insurance (Full Cooperative)

Complete mutual insurance model:

- Premiums based on risk pool
- Claims voted by peers (not company)
- Profit shared as dividends
- Democratic governance of all parameters

**Opcode requirements**:
- Advanced ratio checks for risk-adjusted premiums
- Cross-chain price oracles for risk assessment
- Reputation/identity integration

**Opcode barriers**:
- **`BaseDiv`** may be needed for complex ratio calculations (e.g., `risk_premium = claims / total_premiums`)
- Cross-chain light client verification not yet implemented
- `LessThanOrEqual` delta-invert soundness must be resolved for production

### Opcode Dependency Summary

| Feature | Current Status | Barrier |
|---------|---------------|---------|
| Cross-multiplication for ratios | Proven | None |
| Vote aggregation | Proven (DAO) | None |
| Merkle proofs | Proven | None |
| `LessThanOrEqual` returning 0/1 | Grey-market | Delta-invert soundness |
| `BaseDiv` | Not implemented | Division circuit complexity |
| Cross-chain verification | Not implemented | Light client needed |

**Key insight**: Most cooperative features can be built with existing opcodes. The main gaps are:

1. **`LessThanOrEqual` soundness** — blocks production deployment of comparison-based governance
2. **`BaseDiv`** — would simplify some ratio calculations but cross-multiplication works around it
3. **Cross-chain oracles** — requires external light client infrastructure

**See**: [zkVM Primitive Layer](./zkvm_primitives.md) for the full opcode analysis.

| Circuit | Status | Notes |
|---------|--------|-------|
| `init_v1.zk` | Complete | Links endowment to DAO |
| `pay_premium_v1.zk` | Complete | Creates membership note |

### What Remains

1. **Entry point wiring**: Connect ZK proofs to `process_instruction()`
2. **Membership note integration**: `spend_hook` to check expiry at spend time
3. **Money integration**: Connect endowment to actual token holdings

### Key Blocker: Membership Note Spend Hook

The membership note needs a `spend_hook` that:
- Verifies `current_block < expiry` before allowing spend
- This is Money contract integration, not DAO-Escrow

**Claims are handled by DAO treasury** — no DAO-Escrow-specific work needed there.

## Comparison

| Feature | Insurance | DAO | Escrow | DAO-Escrow (New) |
|---------|----------|-----|--------|-------------------|
| Premium collection | Yes | No | No | Yes (annual note) |
| Membership | Underwriting | N/A | N/A | Time-limited |
| Claims handling | Company | N/A | Pre-defined | DAO treasury |
| Funds release | Company | N/A | Secret/timeout | DAO vote |

## References

- [DarkFi DAO-Escrow README](../../src/contract/dao_escrow/README.md)
- [DarkFi DAO Contract](./dao.md)
- [DarkFi Escrow Contract](./escrow.md)
- [DarkFi Money Contract](../spec/contract/money/money.md)
- [Contract MVP Status](./mvp_status.md)
- [Field Arithmetic Constraints](./field_arithmetic.md)
