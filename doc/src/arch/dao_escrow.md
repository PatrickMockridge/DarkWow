# DAO-Escrow Contract

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
│  │  Treasury-Only (Same as DarkFi DAO)                                  │    │
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
| Insurance payouts | ✅ | ❌ | ✅ |
| Operational funding | ❌ | ✅ | ✅ |
| Fee split | N/A | N/A | ✅ (configurable) |

## How It Works

### Initialize with Mode Selection

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

// MODE_TREASURY: Same as DarkFi DAO
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

### Pay Premium (Membership Notes)

Members pay premiums and receive time-limited membership notes:

```rust
let premium = PayPremiumBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .member_secret(member_secret)
    .value(100)
    .token_id(DRK_TOKEN)
    .expiry(current_block + 52500)  // ~1 year
    .build()?;
```

The circuit verifies block-based expiry via `less_than_strict(current_block, expiry)`.

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

## Trust Model

| Aspect | How It's Protected |
|--------|-------------------|
| Treasury funds | DAO governance (propose/vote/exec) |
| Endowment | Cannot be used for treasury items, only insurance |
| Membership notes | Block-based expiry enforced in circuit |
| Double-spend | Nullifiers prevent redemption twice |
| Mass exit / drain | Optional DrainProtection with rate limiting and exit queue |

## Trust Model

| Aspect | How It's Protected |
|--------|-------------------|
| Treasury funds | DAO governance (propose/vote/exec) |
| Endowment | Cannot be used for treasury items, only insurance |
| Membership notes | Block-based expiry enforced in circuit |
| Double-spend | Nullifiers prevent redemption twice |

## DrainProtection Integration

DAO-Escrow can integrate with the [DrainProtection contract](./drain_protection.md) to provide governance-level protections against malicious DAO actions or mass exit attacks.

### How It Works

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    DAO-Escrow + DrainProtection                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │     DAO-Escrow       │         │  DrainProtection      │              │
│  │                      │         │                       │              │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │              │
│  │  │ pay_premium()  │──┼───┐     │  │  exit()        │  │              │
│  │  └────────────────┘  │   │     │  │  transfer()    │  │              │
│  │                      │   │     │  │  lock/unlock   │  │              │
│  │  State: Merklized    │   │     │  └───────┬────────┘  │              │
│  │  Membership tree     │   │     │          │            │              │
│  │                      │   │     │  Verifies via:        │              │
│  │                      │   ├────▶│  ┌────────▼────────┐ │              │
│  │                      │   │     │  │ Merkle proof   │ │              │
│  │                      │   │     │  │ from DAO-Escrow│ │              │
│  └──────────────────────┘   │     │  └────────────────┘ │              │
│                             │     │                       │              │
└─────────────────────────────┴─────┴───────────────────────┘              │
       Cross-Contract         │                                              │
       Merkle Proof          │                                              │
                              ▼                                              │
┌─────────────────────────────────────────────────────────────────────────┐
│                    No Direct State Sharing!                                │
│                                                                          │
│  DrainProtection verifies DAO-Escrow membership via Merkle proof.        │
│  DAO-Escrow does NOT read DrainProtection state.                         │
│  Each contract maintains its own nullifier namespace.                    │
└─────────────────────────────────────────────────────────────────────────┘
```

### DrainProtection Features

When enabled, the following protections are available:

| Feature | Description |
|---------|-------------|
| Rate Limiting | Transfers exceeding base rate require 2/3 vote |
| Vote Thresholds | Large withdrawals need 2/3 approval + 50% quorum |
| Emergency Lock | Lock funds with 2/3 vote (max 7 days) |
| Exit Queue | Members queue for exit during high-risk periods |
| Graduated Tiers | Larger exits require more governance approval |
| Guardian Pause | Emergency pause capability for guardians |
| Observation Period | New proposals face a delay before voting |
| Split Proposals | Large proposals split into smaller ones |
| No-Loss Reserve | Endowment maintains minimum reserve |
| Dead Man's Switch | Auto-disable if governance becomes unresponsive |

### Enabling DrainProtection

DrainProtection can be enabled during initialization or on an existing DAO-Escrow:

```rust
// During initialization
let dao_escrow = InitializeBuilder::new()
    .mode(DaoEscrowMode::TreasuryEndowment)
    .enable_drain_protection(true)  // Enable at setup
    .build()?;

// Or enable later via governance
let enable_dp = EnableDrainProtectionBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .drain_protection_bulla(dp_instance.bulla)
    .build()?;
```

## Composability with Subscription

DAO-Escrow membership integrates with Subscription for tiered pricing:

```
┌──────────────────────┐         ┌──────────────────────┐
│     DAO-Escrow       │         │    Subscription       │
│  MODE_TREASURY_      │         │                       │
│  ENDOWMENT           │         │  Verifies via:        │
│                       │         │  ┌────────────────┐  │
│  pay_premium() ──────┼─────────┼─►│ Merkle proof    │  │
│                       │  Merkle │  │ expiry check    │  │
│  State: Merklized    │  Proof  │  │ pubkey link    │  │
│                       │         │  └────────────────┘  │
└──────────────────────┘         └──────────────────────┘
```

## ZK Circuits

| Circuit | Status | Notes |
|--------|--------|-------|
| `init_v1.zk` | Complete | Mode selection, bulla derivation |
| `pay_premium_v1.zk` | Complete | Block-based expiry enforced |

## See Also

- [DAO-Escrow Contract README](../../src/contract/dao_escrow/README.md)
- [Subscription Contract](subscription.md)
- [DarkFi DAO Contract](dao.md)
- [Opcodes Reference](opcodes.md)
- [Opcode Universe](opcode_universe.md)
