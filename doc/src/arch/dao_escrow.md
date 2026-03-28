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

## Trust Model

| Aspect | How It's Protected |
|--------|-------------------|
| Treasury funds | DAO governance (propose/vote/exec) |
| Endowment | Cannot be used for treasury items, only insurance |
| Membership notes | Block-based expiry enforced in circuit |
| Double-spend | Nullifiers prevent redemption twice |

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
- [Experimental Opcodes](experimental-opcodes.md)
- [Opcode Universe](opcode_universe.md)
