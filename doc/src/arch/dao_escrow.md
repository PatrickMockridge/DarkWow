# DAO-Escrow Contract

Community insurance / collective escrow governed by DAO voting.

## The Problem: Centralized Insurance or Trustless but Inflexible

Traditional insurance:
- **Centralized**: Single company controls everything
- **Opaque**: Premium calculations and claims decisions are opaque
- **Counterparty risk**: Company can deny claims or go bankrupt

Smart contract escrow:
- **Trustless**: No single party controls funds
- **Inflexible**: No voting/discretion on edge cases
- **Timeout-based**: Refund is automatic, not discretionary

**What if you could have democratic governance with trustless execution?**

## Our Solution: DAO-Governed Escrow

```
┌─────────────────────────────────────────────────────────────────────┐
│                  DAO-Escrow Architecture                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   ┌─────────────┐                                                    │
│   │  Members     │ ──pay premiums──> ┌─────────────────┐            │
│   │  (token      │                   │  Endowment Pool  │            │
│   │   holders)   │                   │                 │            │
│   └─────────────┘                   └────────┬────────┘            │
│                                              │                       │
│                           ┌─────────────────┼─────────────────┐    │
│                           │                 │                   │    │
│                           ▼                 ▼                   ▼    │
│                    ┌──────────┐      ┌──────────┐       ┌──────────┐│
│                    │  Claims   │      │  Claims   │       │  Claims   ││
│                    │  Pending  │ ──> │ Approved │ ──>  │ Executed ││
│                    └──────────┘      └──────────┘       └──────────┘│
│                         │                  ▲                            │
│                         │                  │                            │
│                         └────── DAO Vote ──┘                            │
│                                                                       │
│   Endowment released like escrow claim IF DAO approves                 │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## How It Works

### 1. Create DAO-Escrow

An owner creates a DAO-Escrow with governance parameters:
- **Quorum**: Minimum vote participation needed
- **Approval ratio**: Yes votes / total votes needed to pass
- **Premium rate**: What members pay into the endowment
- **Max claim ratio**: Maximum claim as % of total endowment

### 2. Members Pay Premiums

Members pay premiums into the endowment pool:
```
premium = member_balance * premium_rate
```

### 3. Propose a Claim

Anyone with sufficient governance tokens can propose a claim:
```
claim = {
    value: u64,
    description_hash: hash,
    recipient: public_key
}
```

### 4. DAO Votes

Token holders vote on the claim:
- **Yes votes**: Approve the claim
- **No votes**: Reject the claim

Vote aggregation happens on-chain. When quorum is reached:
- `yes_votes / total_votes >= approval_ratio` → **Approved**
- Otherwise → **Rejected**

### 5. Execute Claim (Like Escrow)

If approved, the endowment releases funds like an escrow claim:
- Verified via ZK proof
- Funds released to specified recipient
- Claim marked as executed

## Trust Model: Democratic + Algorithmic

| Aspect | Traditional Insurance | Pure Escrow | DAO-Escrow |
|--------|----------------------|-------------|-------------|
| **Who decides claims** | Company alone | Algorithm alone | DAO vote |
| **Funds control** | Company | Smart contract | Smart contract |
| **Edge case handling** | Company discretion | None | DAO discretion |
| **Appeals** | None | None | Vote again |
| **Transparency** | Opaque | Full | Full |

## Comparison: DAO-Escrow vs Plain Escrow

| Feature | Plain Escrow | DAO-Escrow |
|---------|--------------|-------------|
| **Timeout refund** | Yes (automatic) | No (DAO decides) |
| **Claim conditions** | Pre-defined | Voted on |
| **Dispute resolution** | None | DAO vote |
| **Premium collection** | Not built-in | Built-in |
| **Governance** | None | Full DAO |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Create new DAO-Escrow |
| UpdateV1 | 0x01 | Update governance params |
| PayPremiumV1 | 0x02 | Member pays premium |
| ProposeClaimV1 | 0x03 | Propose a claim |
| VoteClaimV1 | 0x04 | Vote on a claim |
| ExecuteClaimV1 | 0x05 | Execute approved claim |
| CancelClaimV1 | 0x06 | Cancel pending claim |
| WithdrawV1 | 0x07 | Owner withdraws fees |

## State Machine

### Claim State

```
Pending ──[quorum + approval]──> Approved ──[execute]──> Executed
   │
   ├──[voting window expires]──> Expired
   │
   └──[proposer cancels]──> Cancelled

Approved ──[execution deadline passes]──> Expired
```

### Endowment Flow

```
Members pay premiums ──> Endowment Pool ──> Claims payout
                              │
                              └──> Owner withdrawal (fees)
```

## ZK Circuits

### init_v1.zk

Proves owner knows secret key and governance params are committed.

### pay_premium_v1.zk

Proves premium payment is valid and member has funds.

### propose_claim_v1.zk

Proves proposer has sufficient governance tokens and claim is valid.

### vote_claim_v1.zk

Proves voter has tokens and hasn't already voted.

### execute_claim_v1.zk

Proves claim was approved and executes payout.

## Opcode Requirements

| Circuit | Opcodes Used | Status |
|---------|-------------|--------|
| `init_v1.zk` | `poseidon_hash`, `ec_mul_base` | Existing |
| `pay_premium_v1.zk` | `ec_mul_short`, `ec_mul`, `ec_add` | Existing |
| `propose_claim_v1.zk` | `poseidon_hash`, `ec_mul_base` | Existing |
| `vote_claim_v1.zk` | `poseidon_hash`, `ec_mul_base` | Existing |
| `execute_claim_v1.zk` | `ec_mul_base` | Existing |

**No new opcodes needed!** All required functionality exists in the zkVM.

## Use Cases

### Community Insurance
```rust
// Create DAO-Escrow for community health insurance
let dao_escrow = InitializeBuilder::new()
    .owner_pubkey(community_treasury)
    .gov_token_id(COMMUNITY_TOKEN)
    .proposer_limit(100)        // 100 tokens to propose
    .quorum(1000)               // 1000 token votes needed
    .approval_ratio(51, 100)    // 51% approval
    .premium_rate(1, 100)       // 1% of balance per period
    .max_claim_ratio(10, 100)  // Max 10% of pool per claim
    .build()?;

// Members pay premiums
premium_payment = PayPremiumBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .value(100)
    .build()?;

// Someone needs medical coverage - propose claim
claim = ProposeClaimBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .value(5000)
    .description("Emergency surgery")
    .recipient_pubkey(claimant)
    .build()?;

// DAO votes on the claim
vote = VoteClaimBuilder::new()
    .claim_id(claim.id)
    .yes()
    .build()?;

// If approved, funds released
execute = ExecuteClaimBuilder::new()
    .claim_id(claim.id)
    .build()?;
```

### Protocol-Owned Liquidity
```rust
// DAO manages a liquidity endowment
// Members contribute tokens
// DAO votes on strategic allocations
```

### Treasury Management
```rust
// DAO manages treasury
// Grants require voting
// Execute is like escrow claim
```

## Architecture

The DAO-Escrow contract source is in `src/contract/dao_escrow/`. See the contract [README](../../src/contract/dao_escrow/README.md) for the full architecture.

```
src/contract/dao_escrow/
├── proof/                    # ZK proof circuits (.zk files)
│   ├── init_v1.zk          # DAO-Escrow initialization
│   ├── pay_premium_v1.zk   # Premium payment
│   ├── propose_claim_v1.zk # Claim proposal
│   ├── vote_claim_v1.zk    # Vote on claim
│   └── execute_claim_v1.zk # Execute approved claim
├── src/
│   ├── client/             # Builder structs
│   ├── entrypoint.rs       # WASM entrypoint
│   ├── error.rs            # Error types
│   ├── lib.rs              # Contract definitions
│   └── model/              # Data structures
└── README.md
```

## Integration

### With Money Contract
- Premiums paid via Money::Transfer
- Claims release funds via Money::Mint (from endowment pool)
- Uses same coin/nullifier infrastructure

### With DAO Contract
- Similar voting mechanism to DAO::Propose/Vote/Exec
- But specifically for endowment claims
- Simplified: no arbitrary auth calls, just value transfer

## Security Considerations

### Vote Manipulation
- One token = one vote (no delegation)
- Nullifiers prevent double-voting
- Quorum prevents low-participation decisions

### Endowment Safety
- Max claim ratio prevents drain via single claim
- Execution deadline prevents indefinite pending claims
- Owner withdrawal limited to accumulated fees

### Privacy
- Premium payments are pseudonymous (public key)
- Claims linkable via description hash (if disclosed)
- Vote amounts hidden via Pedersen commitments

## MVP Status

**Placeholder MVP** — Core structure exists, ZK circuits are stubs.

| Circuit | Status | Notes |
|---------|--------|-------|
| `init_v1.zk` | Placeholder | Uses existing opcodes |
| `pay_premium_v1.zk` | Placeholder | Pedersen commitment |
| `propose_claim_v1.zk` | Placeholder | Merkle proof is TODO |
| `vote_claim_v1.zk` | Placeholder | Nullifier check is TODO |
| `execute_claim_v1.zk` | Placeholder | Vote aggregation is TODO |

### What It Needs

1. **ZK Circuit Compilation**: Convert `.zk` files to `.zk.bin`
2. **Entry Point Implementation**: Wire ZK proof verification
3. **Vote Aggregation**: On-chain vote counting with cross-multiplication
4. **Money Integration**: Connect endowment to actual token pool

### No Blockers

All required opcodes exist. Vote aggregation uses the same cross-multiplication pattern as DAO.

## Comparison

| Feature | Insurance | DAO | Escrow | DAO-Escrow |
|---------|----------|-----|--------|------------|
| Premium collection | Yes | No | No | Yes |
| Claims process | Underwriting | Voting | Conditions | Voting |
| Funds release | Company | N/A | Timeout/Secret | DAO approved |
| Transparency | Low | High | Full | High |
| Edge cases | Company discretion | DAO discretion | None | DAO discretion |

## References

- [DarkFi DAO-Escrow README](../../src/contract/dao_escrow/README.md)
- [DarkFi DAO Contract](./dao.md)
- [DarkFi Escrow Contract](./escrow.md)
- [DarkFi Money Contract](../spec/contract/money/money.md)
- [zkVM Primitive Layer](./zkvm_primitives.md)
- [Contract MVP Status](./mvp_status.md)
- [Field Arithmetic Constraints](./field_arithmetic.md)
