# DarkFi Contract Status

**Last Verified**: 2026-04-08
**Note**: Package names use UNDERSCORE format (e.g., `darkfi_baccarat_contract`) except for `darkfi-identity-contract` (hyphenated) and `subscription_contract` (also underscore but no darkfi_ prefix)

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

### Identity (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| identity | darkfi-identity-contract | ✓ | 8/8 circuits | PASS |

**ZK Circuits implemented**:
- `issue_credential_v1.zk` - Credential issuance with attributes
- `create_claim_v1.zk` - Basic claim creation
- `create_claim_v1_l1.zk` - Level 1 selective disclosure with bounded equation
- `create_claim_v1_l1_v2.zk` - Level 1 V2 using less_than_or_equal
- `create_claim_v1_multi.zk` - Multi-credential AND logic
- `create_claim_v1_ratio.zk` - Ratio-based claims with base_div
- `create_claim_v1_dag.zk` - DAG-based claims
- `verify_capability_v1.zk` - O-Cap capability verification

### Labor Market (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| labor_market | darkfi_labor_market_contract | ✓ | 9/9 circuits | PASS |

**ZK Circuits implemented**:
- `create_job_v1.zk` - Simple employer key derivation
- `accept_job_v1.zk` - Worker accepts job
- `accept_job_with_capability_v1.zk` - Worker with capability verification
- `submit_deliverable_v1.zk` - Generic deliverable submission
- `submit_git_deliverable_v1.zk` - Git deliverable submission
- `confirm_delivery_v1.zk` - Employer confirms delivery
- `milestone_payment_v1.zk` - Time-weighted milestone payment
- `dispute_v1.zk` - Dispute escalation to DAO
- `refund_v1.zk` - HTLC-style timeout refund

### Oracle (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| oracle | darkfi_oracle_contract | ✓ | 5/5 circuits | PASS |

**ZK Circuits implemented**:
- `register_oracle_v1.zk` - Oracle registration
- `push_value_v1.zk` - Value push with public key derivation
- `attest_value_v1.zk` - Value attestation
- `push_value_commitment_v1.zk` - Private data commitment with Merkle proof
- `aggregate_v1.zk` - Weighted average aggregation

### Auction (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| auction | darkfi_auction_contract | ✓ | 6/6 circuits | PASS |

**ZK Circuits implemented**:
- `create_auction_v1.zk` - Auction creation with seller commitment
- `place_bid_v1.zk` - Bid placement with deadline check
- `close_auction_v1.zk` - Auction close by seller
- `claim_winnings_v1.zk` - Winner claims item
- `settle_auction_v1.zk` - Seller settles auction
- `refund_bid_v1.zk` - Outbid bidder refunds

### Tender (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| tender | darkfi_tender_contract | ✓ | 5/5 circuits | PASS |

**ZK Circuits implemented**:
- `create_tender_v1.zk` - Tender creation
- `submit_bid_v1.zk` - Sealed bid submission
- `reveal_bid_v1.zk` - Bid reveal after deadline
- `select_winner_v1.zk` - Winner selection by requester
- `submit_bid_with_capability_v1.zk` - Bid with capability proof

### Attestation (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| attestation | darkfi_attestation_contract | ✓ | 8/8 circuits | PASS |

**ZK Circuits implemented**:
- `create_attestation_v1.zk` - Attestation creation
- `create_claim_v1.zk` - Claim creation against attestation
- `verify_claim_v1.zk` - Claim verification with predicate logic
- `consume_claim_v1.zk` - Claim consumption with nullifier
- `check_not_revoked_v1.zk` - Non-revocation proof
- `delegate_attestation_v1.zk` - Delegation with stake ratio check
- `verify_chain_v1.zk` - Delegation chain verification
- `update_delegation_v1.zk` - Delegation update with ratio enforcement

### Subscription (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | Integration Tests | Status |
|----------|--------------|-------|------------------|--------|
| identity | darkfi-identity-contract | ✓ | 15 tests | PASS |

### Subscription (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| subscription | subscription_contract | ✓ | 3/3 circuits | PASS |

**ZK Circuits implemented**:
- `subscribe_v1.zk` - Subscription creation with DAO-Escrow integration
- `verify_access_v1.zk` - Access verification with rate limiting
- `rate_limit_v1.zk` - Rate limit enforcement

### Stablecoin (ZK PROOFS IMPLEMENTED 2026-04-08)
| Contract | Package Name | Build | ZK Proofs | Status |
|----------|--------------|-------|-----------|--------|
| stablecoin | darkfi_stablecoin_contract | ✓ | 5/5 circuits | PASS |

**ZK Circuits implemented**:
- `open_position_v1.zk` - Position opening with Pedersen commitments
- `mint_stable_v1.zk` - Stablecoin minting
- `liquidate_v1.zk` - CDP liquidation with penalty
- `governance_report_v1.zk` - Precise collateral/debt ratio via BaseDiv
- `accrue_interest_v1.zk` - Interest calculation via BaseDiv

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

**DEX ZK Circuits**:
- `create_swap_v1.zk` - Swap creation with lock commitment
- `accept_swap_v1.zk` - Swap acceptance
- `execute_swap_v1.zk` - Swap execution
- `execute_swap_slippage_v1.zk` - Slippage-protected execution
- `execute_swap_fee_v1.zk` - Fee-on-transfer execution
- `cancel_swap_v1.zk` - Swap cancellation

**Bridge ZK Circuits**:
- `deposit_v1.zk` - Cross-chain deposit with Merkle proof
- `withdraw_v1.zk` - Cross-chain withdrawal
- `ltc_deposit_v1.zk` - Litecoin deposit (with MWEB support)
- `xmr_deposit_v1.zk` - Monero deposit (with DLEq proof)
- `azt_deposit_v1.zk` - Aztec deposit (with note commitment)
- `zec_deposit_v1.zk` - Zcash deposit (with Sapling proof)

| atomic_swap | atomic_swap_contract | ✓ | 13 tests | PASS |

**Atomic Swap ZK Circuits**:
- `create_swap_v1.zk` - HTLC creation with secret hash commitment
- `claim_v1.zk` - Swap claim with secret reveal
- `refund_v1.zk` - Timelock refund after expiration

| stablecoin | darkfi_stablecoin_contract | ✓ | 15 tests + ZK proofs | PASS |

**Stablecoin ZK Circuits**:
- `open_position_v1.zk` - Position opening with Pedersen commitments
- `mint_stable_v1.zk` - Stablecoin minting
- `liquidate_v1.zk` - CDP liquidation with penalty
- `governance_report_v1.zk` - Precise collateral/debt ratio via BaseDiv
- `accrue_interest_v1.zk` - Interest calculation via BaseDiv
| dao_escrow | darkfi_dao_escrow_contract | ✓ | 20 tests | PASS |
| drain_protection | darkfi_drain_protection_contract | ✓ | 24 tests | PASS |
| block_height_prediction | darkfi_block_height_prediction_contract | ✓ | 26 tests | PASS |
| insurance_market | darkfi_insurance_market_contract | ✓ | 13 tests | PASS |
| escrow | darkfi_escrow_contract | ✓ | 4/4 circuits | PASS |

**Escrow ZK Circuits**:
- `create_escrow_v1.zk` - Escrow creation with buyer/seller keys
- `fund_v1.zk` - Funding with Pedersen commitment
- `claim_v1.zk` - Seller claim with H(seller_pub) privacy
- `refund_v1.zk` - Buyer refund after timeout

| game_room | game_room_contract | ✓ | 42 tests | PASS |

### Governance & Marketplace Contracts
| Contract | Package Name | Build | Integration Tests | ZK Proofs | Status |
|----------|--------------|-------|------------------|-----------|--------|
| labor_market | darkfi_labor_market_contract | ✓ | 24 tests | 9/9 circuits | PASS |
| oracle | darkfi_oracle_contract | ✓ | 7 tests | 5/5 circuits | PASS |
| auction | darkfi_auction_contract | ✓ | 23 tests | 6/6 circuits | PASS |
| tender | darkfi_tender_contract | ✓ | 23 tests | 5/5 circuits | PASS |
| attestation | darkfi_attestation_contract | ✓ | 23 tests | 8/8 circuits | PASS |
| dao | darkfi_dao_contract | ✓ | 1 test | N/A | IGNORE (needs darkfid) |

### Money Contracts
| Contract | Package Name | Build | Integration Tests | Status |
|----------|--------------|-------|------------------|--------|
| money | darkfi_money_contract | ✓ | 1 test | IGNORE (needs darkfid) |
| money_v2 | darkfi_money_v2_contract | ✓ | 1 test | IGNORE (needs darkfid) |

### Other Contracts
| Contract | Package Name | Build | Integration Tests | Status |
|----------|--------------|-------|------------------|--------|
| deployooor | deployooor_contract | ✓ | none | No tests |
| safemath | darkfi-safemath-zk | N/A | none | Utility library |

## Summary

- Total contracts: 31
- Build passing: 31 (all have valid Cargo.toml)
- With passing integration tests: 20+
- With tests marked #[ignore] (needs darkfid): 3 (money, money_v2, dao)
- Without tests: 6 (roulette, slot, darktoshi_dice, betting_stake, escrow, deployooor)

## Root Causes of Test Failures (FIXED)

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

### 5. Test Harness Dependency (Documented)
**Problem**: money, money_v2, and dao tests require a running `darkfid` node via `TestHarness`.

**Solution**: Marked these tests with `#[ignore]` attribute. Run manually with:
```
cargo test -p darkfi_money_contract --test integration -- --ignored
```

### 6. State Persistence Not Implemented (Fixed - Subscription)
**Problem**: Subscription contract's `process_update` didn't persist state - subscriptions vanished after creation.

**Fix applied** (2026-04-08): Implemented full state persistence:
- Expanded `SubscribeUpdateV1`, `CancelUpdateV1`, `RenewUpdateV1` to carry full `Subscription` objects
- Implemented `subscribe_v1`, `cancel_v1`, `renew_v1`, `update_usage_v1`, `dao_control_v1` instruction handlers
- Implemented corresponding `*_apply_v1` functions in `process_update`

## Notes

- Package naming is inconsistent across the codebase
- `money`, `money_v2`, and `dao` tests require the full test harness (running darkfid node) - marked with `#[ignore]`
- `ContractError` doesn't implement `PartialEq`, so use `matches!()` macro instead of `assert_eq!` for Result comparisons
- After model changes, integration tests may need updating to use new struct fields
