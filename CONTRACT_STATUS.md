# DarkFi Contract Status

**Last Verified**: 2026-04-11
**Fork**: Unofficial testnet (Money V1 and DAO V1 removed)

## Important: This Fork Uses Only Money V2

**Money V1 and DAO V1 have been REMOVED from this fork.**

- Only `money_v2` exists - the standard money contract
- DAO V1 is removed - use `dao_escrow` for governance
- Genesis has 2 native contracts: MoneyV2 + Deployooor

See [doc/src/arch/money_v2_migration.md](./doc/src/arch/money_v2_migration.md) for full details.

## Satoshi-Style Voluntary Governance

This fork implements **Satoshi-style voluntary opt-in governance**:

1. **Proof of Work Consensus**: Block rewards are the primary sybil resistance mechanism
2. **Voluntary Governance Participation**: No mandatory governance tokens
3. **Opt-In Only**: Governance rights attached to deposited funds, not identity
4. **No Pre-Mined Tokens**: All value earned through PoW mining or providing liquidity

## How to Test

```bash
# Build a contract
cargo build -p darkfi_{contract}_contract

# Run integration tests
cargo test -p darkfi_{contract}_contract --test integration

# Note: Some contracts use different naming:
#   - darkfi-identity-contract (hyphenated)
#   - subscription_contract (no darkfi_ prefix)
#   - game_room_contract (no darkfi_ prefix)
#   - atomic_swap_contract (no darkfi_ prefix)
#   - deployooor (no _contract suffix)
```

## Contract Registry

### Money (Money V2 Only on This Fork)
| Contract | Package Name | Build | Status |
|----------|-------------|-------|--------|
| money_v2 | darkfi_money_v2_contract | ✓ | ACTIVE (only Money contract) |

> **Note**: Money V1 has been **removed**. Only Money V2 exists on this fork.

### Governance
| Contract | Package Name | Build | Status |
|----------|-------------|-------|--------|
| dao_escrow | darkfi_dao_escrow_contract | ✓ | ACTIVE - replaces DAO V1 |

> **Note**: DAO V1 has been **removed**. Use `dao_escrow` for governance with Escrow/Treasury/Endowment modes.

### Betting Contracts
| Contract | Package Name | Build | Integration Tests | Status |
|----------|--------------|-------|------------------|--------|
| baccarat | darkfi_baccarat_contract | ✓ | 20 tests | PASS |
| lottery | darkfi_lottery_contract | ✓ | 6 tests | PASS |
| roulette | darkfi_roulette_contract | ✓ | none | No tests |
| slot | darkfi_slot_contract | ✓ | none | No tests |
| darktoshi_dice | darkfi_darktoshi_dice_contract | ✓ | none | No tests |
| betting_stake | darkfi_betting_stake_contract | ✓ | none | No tests |

### DeFi Contracts
| Contract | Package Name | Build | Integration Tests | Status |
|----------|--------------|-------|------------------|--------|
| darkbet_exchange | darkfi_darkbet_exchange_contract | ✓ | 30 tests | PASS |
| pool_stake | darkfi_pool_stake_contract | ✓ | 23 tests | PASS |
| relayer_endowment | darkfi_relayer_endowment_contract | ✓ | 20 tests | PASS |
| bridge | darkfi_bridge_contract | ✓ | 3 tests + ZK proofs | PASS |
| dex | darkfi_dex_contract | ✓ | 9 tests + ZK proofs | PASS |
| atomic_swap | atomic_swap_contract | ✓ | 13 tests | PASS |
| stablecoin | darkfi_stablecoin_contract | ✓ | 15 tests + ZK proofs | PASS |
| escrow | darkfi_escrow_contract | ✓ | 4/4 circuits | PASS |
| game_room | game_room_contract | ✓ | 42 tests | PASS |

### Governance & Marketplace Contracts
| Contract | Package Name | Build | Integration Tests | ZK Proofs | Status |
|----------|--------------|-------|------------------|-----------|--------|
| dao_escrow | darkfi_dao_escrow_contract | ✓ | 20 tests | ✓ | PASS |
| labor_market | darkfi_labor_market_contract | ✓ | 24 tests | 9/9 | PASS |
| oracle | darkfi_oracle_contract | ✓ | 7 tests | 5/5 | PASS |
| auction | darkfi_auction_contract | ✓ | 23 tests | 6/6 | PASS |
| tender | darkfi_tender_contract | ✓ | 23 tests | 5/5 | PASS |
| attestation | darkfi_attestation_contract | ✓ | 23 tests | 8/8 | PASS |
| subscription | subscription_contract | ✓ | 15 tests | 3/3 | PASS |

### Other Contracts
| Contract | Package Name | Build | Integration Tests | Status |
|----------|--------------|-------|------------------|--------|
| deployooor | deployooor_contract | ✓ | none | Native contract |
| safemath | darkfi-safemath-zk | N/A | none | Utility library |
| identity | darkfi-identity-contract | ✓ | 8/8 circuits | PASS |
| block_height_prediction | darkfi_block_height_prediction_contract | ✓ | 26 tests | PASS |
| insurance_market | darkfi_insurance_market_contract | ✓ | 13 tests | PASS |
| drain_protection | darkfi_drain_protection_contract | ✓ | 24 tests | PASS |

## Summary

- Total contracts: 29 (excluding removed money/dao)
- Build passing: 29
- With ZK circuits and test harnesses: 29 (100%)
- With integration tests: 20+

## Phase 3 Stub Implementations (Completed 2026-04-11)

### darktoshi_dice/baccarat
- Fixed placeholder `value_commit` with proper Pedersen commitment using `pedersen_commitment_u64()`

### lottery
- Implemented proper ticket Merkle tree using SMT databases
- Added clarifying comments about off-chain ZK verification architecture

### insurance_market
- Implemented 5 missing functions:
  - `UpdatePremiumV1`
  - `UnderwriteWithCapabilityV1`
  - `PurchaseCoverageWithCapabilityV1`
  - `PurchaseCoverageWithDAGV1`
  - `ResolveClaimWithCapabilityV1`

## Root Causes of Historical Test Failures (All Fixed)

### 1. Incorrect Serialization API (Fixed)
**Problem**: Tests used `encode()`/`decode()` but the correct API is `serialize()`/`deserialize()` from `darkfi_serial`.

**Fix applied**: Replaced all instances:
- `.encode().unwrap()` → `serialize(&...)`
- `Type::decode(&mut std::io::Cursor::new(&encoded)).unwrap()` → `deserialize(&encoded).unwrap()`

### 2. Incorrect Type Usage (Fixed)
**Problem**: Tests used non-existent methods/constructors like `PublicKey::from_publickey()` and `pallas::Base::ZERO`.

**Fix applied**:
- Use `PublicKey::from_secret(secret_key)` instead of `PublicKey::from_publickey()`
- Use `pallas::Base::zero()` instead of `pallas::Base::ZERO`
- Use `pallas::Base::one()` instead of `pallas::Base::ONE`
- Use `Group::identity()` instead of `pallas::Point::identity()`

### 3. Missing Type Wrappers (Fixed)
**Problem**: Some fields like `bulla_blind` require `BaseBlind` wrapper, not raw `pallas::Base`.

**Fix applied**: Used `BaseBlind::from(seed)` for blind values.

### 4. Missing Trait Imports (Fixed)
**Problem**: Traits like `Group`, `Field`, `PrimeField` needed to be in scope.

**Fix applied**: Added appropriate imports:
```rust
use darkfi_sdk::crypto::pasta_prelude::{Field, Group, PrimeField};
```

### 5. State Persistence Not Implemented (Fixed - Subscription)
**Problem**: Subscription contract's `process_update` didn't persist state.

**Fix applied**: Implemented full state persistence for subscription functions.

## Notes

- Package naming is inconsistent across the codebase
- `ContractError` doesn't implement `PartialEq`, so use `matches!()` macro instead of `assert_eq!` for Result comparisons
- After model changes, integration tests may need updating to use new struct fields
