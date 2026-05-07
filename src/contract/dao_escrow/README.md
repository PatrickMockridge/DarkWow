# DarkWow DAO-Escrow Contract

A flexible contract supporting three operating modes: **Escrow-Only**, **Treasury-Only**, and **Treasury+Endowment**.

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
│  │  Members ──pay fees──► Treasury Pool                                  │    │
│  │                                  │                                    │    │
│  │                    ┌─────────────┼─────────────┐                   │    │
│  │                    │             │             │                   │    │
│  │                    │   Propose   │   Vote     │   Exec           │    │
│  │                    │             │             │                   │    │
│  │                    └─────────────┴─────────────┘                   │    │
│  │                                  │                                    │    │
│  │                                  ▼                                    │    │
│  │                          Treasury spent                               │    │
│  │                                                                       │    │
│  │  Grants, development, operational costs. No insurance.               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  MODE_TREASURY_ENDOWMENT (0x02) ────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Treasury + Endowment (Full-Featured)                                │    │
│  │                                                                       │    │
│  │  Members ──pay premiums──► ┌──────────┬──────────┐                 │    │
│  │                            │          │          │                 │    │
│  │              treasury_share %          % endowment_share             │    │
│  │                            │          │          │                 │    │
│  │                            ▼          ▼          │                 │    │
│  │                      Treasury      Endowment      │                 │    │
│  │                         │             │          │                 │    │
│  │                         │             │ DAO vote │                 │    │
│  │                         ▼             ▼          │                 │    │
│  │                    Operational    Claims/        │                 │    │
│  │                    (grants etc)  Refunds        │                 │    │
│  │                                                                       │    │
│  │  Best of both: DAO-funded operations + insurance backing.          │    │
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
| Insurance payouts | ✅ | ❌ | ✅ |
| Operational funding | ❌ | ✅ | ✅ |
| Fee split | N/A | N/A | ✅ (configurable) |

## Implementation Status

### ZK Circuits ✅
- `init_v1.zk` - ✅ Working
- `pay_premium_v1.zk` - ✅ Working

### Entrypoints
| Function | Opcode | Status |
|----------|--------|--------|
| `InitializeV1` | `0x00` | ✅ Implemented |
| `UpdateV1` | `0x01` | ✅ Implemented |
| `PayPremiumV1` | `0x02` | ✅ Implemented |
| `WithdrawV1` | `0x03` | ✅ Implemented |
| `EndowmentWithdrawV1` | `0x04` | ✅ Implemented |
| `TreasurySpendV1` | `0x05` | ✅ Implemented |
| `EnableDrainProtectionV1` | `0x06` | ✅ Implemented |

### Test Status
- Heavyweight pipeline test: ✅ PASSING
- Integration tests: ✅ PASSING

## Initialize

Choose your mode when initializing:

```rust
// MODE_ESCROW: Pure insurance pool
let escrow = InitializeBuilder::new()
    .mode(DaoEscrowMode::Escrow)  // 0x00
    .owner_secret(owner_secret)
    .pool_token_id(DRK_TOKEN)
    .min_premium(100)
    .max_members(1000)
    .build()?;

// MODE_TREASURY: Same as DarkWow DAO
let treasury = InitializeBuilder::new()
    .mode(DaoEscrowMode::Treasury)  // 0x01
    .owner_secret(owner_secret)
    .pool_token_id(DRK_TOKEN)
    .build()?;

// MODE_TREASURY_ENDOWMENT: Full-featured
let full = InitializeBuilder::new()
    .mode(DaoEscrowMode::TreasuryEndowment)  // 0x02
    .owner_secret(owner_secret)
    .pool_token_id(DRK_TOKEN)
    .fee_config(FeeConfig {
        treasury_share: 7000,  // 70%
        endowment_share: 3000,  // 30%
    })
    .min_premium(100)
    .max_members(1000)
    .build()?;
```

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Create new DAO-Escrow (mode selected) |
| `UpdateV1` | `0x01` | Update parameters |
| `PayPremiumV1` | `0x02` | Pay premium, get membership note |
| `WithdrawV1` | `0x03` | Withdraw from treasury |
| `EndowmentWithdrawV1` | `0x04` | Withdraw from endowment (insurance) |
| `TreasurySpendV1` | `0x05` | Treasury spending (standard governance) |
| `EnableDrainProtectionV1` | `0x06` | Enable DrainProtection on existing DAO-Escrow |

## ZK Circuits

| Circuit | Public Inputs | Status |
|---------|--------------|--------|
| `init_v1.zk` | `dao_bulla`, `endowment_bulla` | ✅ Implemented |
| `pay_premium_v1.zk` | `dao_escrow_bulla`, `membership_note`, `value_commit.x`, `value_commit.y` | ✅ Implemented |

### InitV1 Circuit

Creates the endowment bulla proving ownership:

```
public inputs: dao_bulla, endowment_bulla
private inputs: nullifier_k, owner_secret, owner_pub, endowment_token_id, bulla_blind
```

### PayPremiumV1 Circuit

Proves membership premium payment with MPC commit-reveal bulla:

```
public inputs: dao_escrow_bulla, membership_note, value_commit.x, value_commit.y
private inputs: nullifier_k, dao_escrow_bulla, current_block, member_secret,
               value, token_id, expiry, membership_blind, value_blind,
               mpc_secret_1, mpc_secret_2, mpc_secret_3,
               max_membership_blocks, max_expiry, member_pub.x, member_pub.y
```

Uses Pedersen commitment for value: `value_commit = value * G1 + value_blind * G2`

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
    │  (operational)      │         │    (insurance)       │
    │                     │         │                     │
    │  - Grants           │         │  - Refunds          │
    │  - Development      │         │  - Claims           │
    │  - Operations       │         │  - Emergency        │
    └─────────────────────┘         └─────────────────────┘
```

The split is configurable at initialization. Circuit enforces:
- `treasury_share + endowment_share = 10000` (100%)

## Block-Based Time Locks

Membership notes use block-based expiry (no oracle needed):

```rust
// In pay_premium_v1.zk circuit
less_than_strict(current_block, expiry);  // Membership still valid
```

This replaces timestamp-based locks which can be manipulated by miners.

### Maximum Membership Period

To prevent members from self-issuing excessively long memberships, the circuit enforces a maximum:

```rust
// Maximum: ~1 year (52560 blocks at 5min/block)
max_membership_blocks = 52560;
max_expiry = add(current_block, max_membership_blocks);
less_than_strict(expiry, max_expiry);
```

This prevents:
- Members choosing 100-year memberships
- Long-term lockup that bypasses DAO governance

## MPC Commit-Reveal for Bulla Generation

**⚠️ Privacy Fix Applied**: The bulla is now generated via MPC commit-reveal ceremony to ensure unpredictability.

### The Problem (Fixed)

Previously, the DAO alone chose bullae, which could be predictable. A malicious DAO could:
- Assign sequential bullae to track members
- Choose low-entropy bullae to weaken privacy

### The Fix: MPC Commit-Reveal

**Setup Phase (off-chain MPC ceremony):**
```
Party 1: secret_1 → commitment_1 = secret_1 * G
Party 2: secret_2 → commitment_2 = secret_2 * G
Party 3: secret_3 → commitment_3 = secret_3 * G
```

**Issuance Phase:**
```
1. All parties reveal secrets to the user
2. User verifies: secret_i * G == commitment_i
3. Final bulla = H(member_pub, secret_1, secret_2, secret_3)
```

**Privacy Guarantee:**
- As long as ONE MPC party is honest, the bulla is unpredictable
- Even if n-1 parties collude, they cannot predict the final bulla
- Same security model as Zcash's Powers of Tau ceremony

See [security-analysis.md Issue #4](../../doc/src/arch/security-analysis.md) for full details.

## Trust Model

| Aspect | How It's Protected |
|--------|-------------------|
| Treasury funds | DAO governance (propose/vote/exec) |
| Endowment | Cannot be used for treasury items, only insurance |
| Membership notes | Block-based expiry enforced in circuit |
| Double-spend | Nullifiers prevent redemption twice |

## Use Cases

### MODE_ESCROW: Pure Mutual Insurance
- No operational overhead
- All funds go to insurance pool
- Claims voted by members

### MODE_TREASURY: Protocol Treasury
- Same as existing DarkWow DAO
- For grants, development, operations

### MODE_TREASURY_ENDOWMENT: Full-Featured DAO
- Insurance + operational funding
- Subscription services can integrate
- Best for sustainable DAOs

---

## DrainProtection Integration

DAO-Escrow integrates with the [DrainProtection contract](../drain_protection/README.md) for governance-level fund protections.

### Composability Pattern

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    DAO-Escrow + DrainProtection                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │     DAO-Escrow       │         │  DrainProtection      │              │
│  │                      │         │                       │              │
│  │  pay_premium()       │         │  exit()               │              │
│  │  └─► Membership      │────────▶│  └─► Merkle proof    │              │
│  │       (Merkle tree)  │         │       verification   │              │
│  │                      │         │                       │              │
│  │  State:              │         │  State:              │              │
│  │  - Merkle root       │         │  - Rate limits       │              │
│  │  - Bulla             │         │  - Vote tracking     │              │
│  │  - Membership notes  │         │  - Exit records      │              │
│  │                      │         │                       │              │
│  └──────────────────────┘         └──────────────────────┘              │
│                                                                          │
│  KEY: No direct state sharing. DrainProtection verifies membership        │
│  from DAO-Escrow's Merkle tree. Each has its own nullifier namespace.    │
└─────────────────────────────────────────────────────────────────────────┘
```

### Enabling DrainProtection

```rust
// At initialization
let dao_escrow = InitializeBuilder::new()
    .mode(DaoEscrowMode::TreasuryEndowment)
    .enable_drain_protection(true)  // Enable protections
    // ...
    .build()?;

// Or enable later via governance
let enable_dp = EnableDrainProtectionBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .drain_protection_bulla(drain_protection.bulla)
    .build()?;
```

### What DrainProtection Provides

| Protection | Description |
|------------|-------------|
| **Rate Limiting** | Transfers within base rate allowed; exceeding requires 2/3 vote |
| **Vote Thresholds** | Large withdrawals need 2/3 approval + 50% quorum |
| **Emergency Lock** | Lock funds with 2/3 vote (max 7 days, renewable) |
| **Member Exit** | Any member exits with 1/3 haircut (anti-griefing) |
| **Authority Controls** | Spend authority changes need 2/3 vote + 48hr timelock |
| **Graduated Tiers** | Multi-tier approval requirements based on withdrawal size |
| **Exit Queue** | FCFS processing prevents bank-run cascades |
| **Circuit Breaker** | Auto-pause if anomalous drain detected |
| **Guardian Pause** | Multisig emergency stop capability |
| **Observation Period** | 48h delay before large withdrawals visible |
| **Split Proposals** | Large withdrawals must be chunked |
| **No-Loss Reserve** | 20% reserve never available for DAO governance |
| **Dead Man's Switch** | Auto-protocol if DAO inactive for 30 days |

### How It Works

1. **Membership Verification**: DrainProtection's `exit()` function verifies the member exists in DAO-Escrow's Merkle tree via ZK proof
2. **Rate Tracking**: DrainProtection tracks all transfers and enforces rate limits per block
3. **Vote Execution**: Large withdrawals require DAO vote, recorded in DrainProtection
4. **Exit Calculation**: Member's exit value = (weight / total) × funds × 0.666

---

## Provisional Endowment Drain Protection (Further Work Required)

**⚠️ WARNING**: The endowment/treasury funds are protected by provisional governance controls listed below. These require implementation in the DAO governance layer and have not yet been audited.

### Protections

| Action | Threshold | Notes |
|--------|-----------|-------|
| Fund transfers (within rate limit) | None | Base rate per block |
| Fund transfers (exceeds rate) | 2/3 total vote | Configurable rate limit |
| Lock endowment funds | 2/3 total vote | Max 7 days, renewable |
| Unlock funds | 2/3 total vote | + 24hr timelock |
| Change spend authority | 2/3 total vote | + 48hr timelock |
| Member exit | 1/3 haircut | Any time, block-height-weighted |

### Haircut Formula for Member Exit

```
exit_value = (member_contribution_weight / total_endowment) × current_funds × 0.666
```

- Contribution weight is block-height-adjusted (longer deposits = more weight)
- 1/3 withheld goes to insurance reserve
- Protects against sudden mass exit attacks

### Rate Limit Specification

```
base_rate = total_funds × 0.01 / 1000_blocks  # Suggested 1% per 1000 blocks
```

Exceeding this rate triggers mandatory 2/3 vote requirement.

### Implementation Status

- [ ] Implement block-increment rate limiting in DAO governance
- [ ] Add 2/3 vote threshold enforcement for large withdrawals
- [ ] Implement lock/unlock emergency controls with timelocks
- [ ] Add spend authority change restrictions
- [ ] Implement member exit with haircut mechanism
- [ ] Add endowment health metrics dashboard

See [security-analysis.md](../../doc/src/arch/security-analysis.md#issue-10-endowment-fund-has-no-drain-protection-major--provisional-fix-applied) for full details.

---

## Integration with Subscription Contract

DAO-Escrow membership enables Subscription discounts:

```rust
// Subscription verifies DAO-Escrow membership
let proof = SubscribeBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .dao_membership_note(membership.note)
    .dao_escrow_merkle_root(merkle_root)
    // ... other params
    .build()?;
```

## See Also

- [DAO-Escrow Architecture Doc](../../doc/src/arch/dao_escrow.md)
- [Subscription Contract](../subscription/README.md)
- [DarkWow DAO Contract](../../doc/src/arch/dao.md)
