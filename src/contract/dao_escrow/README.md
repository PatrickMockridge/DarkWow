# DarkFi DAO-Escrow Contract

Simplified MVP: Endowment pool governed by a DAO. The DAO acts as "escrow oracle" for claims.

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

## Our Solution: DAO-Controlled Endowment

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

**Key simplification**: Instead of building a parallel voting mechanism for DAO-Escrow, we delegate voting to the existing DAO. Claims against the endowment are handled via the DAO's existing treasury management (`Propose` → `Vote` → `Exec`).

## How It Works (Simplified MVP)

### 1. Initialize Endowment

Owner creates an endowment linked to a DAO:
```rust
// The endowment is controlled by a DAO
let endowment = InitializeBuilder::new()
    .dao_bulla(existing_dao.bulla)  // Links to existing DAO
    .owner_secret(owner_secret)
    .endowment_token_id(DRK_TOKEN)
    .build()?;
```

### 2. Members Pay Premiums

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

The membership note is spent when claiming DAO-Escrow benefits. The `spend_hook` checks membership hasn't expired.

### 3. Claims → DAO Treasury

Claims against the endowment are handled by the DAO's existing treasury management:
- **Propose**: Someone proposes a disbursement (using DAO's `Propose`)
- **Vote**: DAO members vote (using DAO's `Vote`)
- **Exec**: If approved, funds released (using DAO's `Exec` + `AuthMoneyTransfer`)

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

## Comparison

| Feature | Insurance | DAO | Escrow | DAO-Escrow (New) |
|---------|----------|-----|--------|-------------------|
| Premium collection | Yes | No | No | Yes (annual note) |
| Membership | Underwriting | N/A | N/A | Time-limited |
| Claims handling | Company | N/A | Pre-defined | DAO treasury |
| Funds release | Company | N/A | Secret/timeout | DAO vote |

## What DAO-Escrow IS NOT

This is NOT a standalone voting system. It does NOT have:
- Its own voting mechanism
- Its own proposal/execute flow
- Vote aggregation circuits

Instead, it:
- Holds an endowment pool
- Issues membership notes
- Lets the DAO control fund release

## Building

```bash
# Build WASM contract
cargo build -p darkfi_dao_escrow_contract

# Compile ZK circuits
make proof

# Run tests
cargo test -p darkfi_dao_escrow_contract
```

## Architecture

```
dao_escrow/
├── proof/
│   ├── init_v1.zk          # Endowment initialization
│   └── pay_premium_v1.zk   # Premium → membership note
├── src/
│   ├── client/              # Builder structs
│   ├── entrypoint.rs        # WASM entrypoint
│   ├── error.rs             # Error types
│   ├── lib.rs               # Contract definitions
│   └── model/              # Data structures
├── Cargo.toml
└── README.md
```

## MVP Status

**Simplified MVP** — Delegated voting to DAO treasury.

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

## References

- [DarkFi DAO Contract](../dao/) — Existing treasury management
- [DarkFi Escrow Contract](../escrow/) — Membership note pattern
- [DarkFi Money Contract](../money/) — Spend hook integration
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md)
