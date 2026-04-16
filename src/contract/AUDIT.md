# Contract Token Integration Audit

> **Date**: 2026-04-16
> **Auditor**: Claude Code
> **Scope**: All contracts in `src/contract/` for NativeToken and MoneyV3/MoneyV2 integration
> **Status**: Phase 1 (migration) COMPLETED | Phase 2 (audit) COMPLETED

---

## Executive Summary

### Phase 1: Migration Complete ✅

Successfully migrated 4 contracts from deprecated `money_v2` to `money_v3`:

| Contract | Old Usage | New Usage | Status |
|----------|-----------|-----------|--------|
| **dao_escrow** | `money::TransferV2` | `money_v3::transfer_v1` | ✅ Migrated |
| **game_room** | `money::TransferV2` | `money_v3::transfer_v1` | ✅ Migrated |
| **subscription** | `money::TransferV2` | `money_v3::transfer_v1` | ✅ Migrated |
| **dex** | `money::OtcSwapV2` | `money_v3::otc_swap_v1` | ✅ Migrated |

### Phase 2: Audit Complete ✅

**No contracts found using money_v2** in the remaining 18 contracts audited.

---

## Token Architecture

### NativeToken (Consensus Layer)
- **Purpose**: PoW rewards, network fees, burns
- **Deployment**: Genesis (automatic, mandatory)
- **No contracts link to it directly** - consensus handles it

### MoneyV3 (DeFi Layer)
- **Purpose**: ERC-20 style tokens, stablecoins, wrapped assets
- **Design**: Poseidon-only (no EC operations = no heap bugs)
- **Status**: **RECOMMENDED** for all token operations

### MoneyV2 (DEPRECATED)
- **Status**: DEPRECATED - contains EC heap bugs
- **Bug Details**: 4 of 5 circuits have EC heap bugs (Fee_V2, Mint_V2, Burn_V2, AuthTokenMint_V2)
- **Safe Circuits**: Only TokenMint_V2 is safe (Poseidon-only)
- **Migration**: All dependent contracts have been migrated to MoneyV3

---

## Phase 2 Audit Results

### Gaming Contracts

| Contract | money_v2 | money_v3 | native_token | Risk | Notes |
|----------|----------|----------|-------------|------|-------|
| **baccarat** | ❌ | ❌ | ❌ | Medium | Generic token_id, no hard dependency |
| **darktoshi_dice** | ❌ | ❌ | ❌ | Medium | Generic token_id, no hard dependency |
| **roulette** | ❌ | ❌ | ❌ | **HIGH** | No token integration found, appears incomplete |
| **lottery** | ❌ | ❌ | ❌ | Medium | Generic token_id, TokenIdMismatch error exists |
| **slot** | ❌ | ❌ | ❌ | Medium | Generic token_id, documented Money::Burn |
| **block_height_prediction** | ❌ | ❌ | ❌ | Low-Medium | Protocol fee in basis points |
| **darkbet_exchange** | ❌ | ❌ | ❌ | Low | Own fee handling, no money calls |

#### roulette ⚠️ INCOMPLETE
- **Issue**: No token integration found in source code
- **Files checked**: `src/contract/roulette/src/`
- **Findings**: No `token_id`, no money contract references, no fee handling
- **Verdict**: Appears incomplete - needs implementation

### Labor/Insurance/Identity

| Contract | money_v2 | money_v3 | native_token | Risk | Notes |
|----------|----------|----------|-------------|------|-------|
| **labor_market** | ❌ | ❌ | ❌ | Low | payment_token metadata only, no money calls |
| **insurance_market** | ❌ | ❌ | ❌ | **MEDIUM** | Documents Money::TokenMint but NOT implemented |
| **identity** | ❌ | ❌ | ❌ | Low | Credential contract, fee fields unused |

#### insurance_market ⚠️ INCOMPLETE
- **Documentation** (`lib.rs`): Mentions "Money::Burn for premium payments" and "Money::TokenMint for claim payouts"
- **Implementation** (`withdraw_premium_v1.rs` line 97): Only a comment `// In production: trigger Money::TokenMint to transfer the premium to underwriter`
- **Error exists but unused**: `InsuranceMarketError::TransferFailed` defined but never returned
- **Verdict**: Token integration documented but not implemented

### Other Contracts

| Contract | money_v2 | money_v3 | native_token | Risk | Notes |
|----------|----------|----------|-------------|------|-------|
| **bridge** | ❌ | ❌ | ❌ | Low | Own fee mechanism (deposit_fee/withdrawal_fee) |
| **oracle** | ❌ | ❌ | ❌ | Low | Data feeds only, no token handling |
| **escrow** | ❌ | ❌ | ❌ | Low | Own HTLC model, Phase 2 planned per README |
| **atomic_swap** | ❌ | ❌ | ❌ | Low | Own HTLC model, no money contracts |
| **drain_protection** | ❌ | ❌ | ❌ | Low | Own TransferV1 mechanism |
| **tender** | ❌ | ❌ | ❌ | Low | O-Cap authorization only |
| **attestation** | ❌ | ❌ | ❌ | Low | Claims system only |
| **auction** | ❌ | ❌ | ❌ | Low | Integrates with Escrow, not money contracts |

---

## Phase 1 Migrated Contracts (Completed)

### stablecoin ✅ CORRECT
- **Cargo.toml**: `darkfi_money_v3_contract` (optional, client feature)
- **pipeline.toml**: `dependencies = ["money_v3"]`
- **Source**: Uses `darkfi_money_v3_contract::client::token_mint_v1::TokenMintCallInput`
- **Verdict**: CORRECT - Uses MoneyV3 as intended

### dao_escrow ✅ MIGRATED
- **Cargo.toml**: Added `darkfi_money_v3_contract` dependency
- **pipeline.toml**: `dependencies = []` (explicit empty - standalone)
- **Source**: 3 functions updated (WithdrawV1, EndowmentWithdrawV1, TreasurySpendV1)
- **Change**: `money::TransferV2` (0x03) → `money_v3::transfer_v1` (0x04)
- **Verdict**: ✅ Migrated successfully

### game_room ✅ MIGRATED
- **Cargo.toml**: No token dependencies (game-specific)
- **Source**: Claim function updated
- **Change**: `money::TransferV2` (0x03) → `money_v3::transfer_v1` (0x04)
- **Verdict**: ✅ Migrated successfully

### subscription ✅ MIGRATED
- **Cargo.toml**: No token dependencies
- **Source**: DaoControlV1 endowment withdrawal updated
- **Change**: `money::TransferV2` (0x03) → `money_v3::transfer_v1` (0x04)
- **Verdict**: ✅ Migrated successfully

### dex ✅ MIGRATED
- **Cargo.toml**: Added `darkfi_money_v3_contract` dependency
- **pipeline.toml**: `dependencies = ["money_v3"]`
- **Source**: execute_swap_v1 updated
- **Change**: `money::OtcSwapV2` (0x04) → `money_v3::otc_swap_v1` (0x05)
- **New in money_v3**: Added `OtcSwapV1` function (0x05) for atomic token swaps
- **Verdict**: ✅ Migrated successfully

---

## Issues Requiring Attention

### HIGH Priority
1. **roulette** - No token integration found, appears incomplete
   - No token_id fields
   - No fee handling
   - No client/ directory
   - May need complete implementation

### MEDIUM Priority
2. **insurance_market** - Documents Money::TokenMint but code only has comments
   - `withdraw_premium_v1.rs` line 97: `// In production: trigger Money::TokenMint...`
   - `InsuranceMarketError::TransferFailed` exists but never used
   - Need to implement actual token transfers

3. **Gaming contracts (baccarat, darktoshi_dice, lottery, slot)** - Generic token_id
   - Use `pallas::Base` as token identifier without validating against MoneyV3
   - May need to integrate with MoneyV3 for proper token validation

---

## Verification Commands

```bash
# Check for any remaining money_v2 references
grep -r "money_v2\|money::TransferV2\|money::OtcSwapV2" src/contract/*/src/

# Check for hardcoded money dependencies
grep -r "darkfi_money_v2_contract\|darkfi_money_v3_contract" src/contract/*/Cargo.toml

# Build all migrated contracts
cargo build --release -p darkfi_dao_escrow_contract
cargo build --release -p darkfi_game_room_contract
cargo build --release -p darkfi_subscription_contract
cargo build --release -p darkfi_dex_contract
cargo build --release -p darkfi_stablecoin_contract

# Run pipeline tests
CONTRACT_NAME=dao_escrow cargo test --package darkfid --release test_pipeline
CONTRACT_NAME=dex cargo test --package darkfid --release test_pipeline
CONTRACT_NAME=stablecoin cargo test --package darkfid --release test_pipeline
```

---

## Appendix: money_v2 EC Heap Bug Details

Per `money_v2/README.md`:

> Money V2 contains EC heap bugs in 4 of its 5 circuits (Fee_V2, Mint_V2, Burn_V2, AuthTokenMint_V2). Only TokenMint_V2 is safe (Poseidon-only).

The buggy circuits use EC operations that can be exploited via malicious inputs.
