# Contract Token Integration Audit

> **Date**: 2026-04-16
> **Auditor**: Claude Code
> **Scope**: All contracts in `src/contract/` for NativeToken and MoneyV3/MoneyV2 integration

---

## Executive Summary

**CRITICAL FINDING**: Multiple contracts depend on `money_v2` which is **DEPRECATED** with known EC heap bugs.

| Contract | money_v2 Usage | Risk |
|----------|---------------|------|
| **dex** | `money::OtcSwapV2` | CRITICAL |
| **subscription** | `money::TransferV2` | CRITICAL |
| **game_room** | `money::TransferV2` | CRITICAL |
| **dao_escrow** | `money::TransferV2` | CRITICAL |
| **escrow** | References money_v2 | MEDIUM |

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
- **Functions Used by Other Contracts**: TransferV2, OtcSwapV2 (these use v1 circuits internally)

---

## Detailed Findings

### Category 1: DeFi Contracts

#### stablecoin ✅ CORRECT
- **Cargo.toml**: `darkfi_money_v3_contract` (optional, client feature)
- **pipeline.toml**: `dependencies = ["money_v3"]`
- **Source**: Uses `darkfi_money_v3_contract::client::token_mint_v1::TokenMintCallInput`
- **Verdict**: CORRECT - Uses MoneyV3 as intended

#### dex ❌ CRITICAL - Uses Deprecated money_v2
- **Cargo.toml**: NO direct money_v3 dependency
- **pipeline.toml**: `dependencies = ["money_v3"]` (deployment only, NOT code)
- **Source**: `src/contract/dex/src/entrypoint/execute_swap_v1.rs`
  - Line 43: `money::OtcSwapV2` child calls
  - Line 250-251: Token swap via money_v2
- **Issue**: Uses `money_v2::OtcSwapV2` which has EC heap bugs
- **Verdict**: CRITICAL - Must migrate to MoneyV3

#### auction ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **pipeline.toml**: No dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### insurance_market ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **pipeline.toml**: No dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### lottery ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **pipeline.toml**: No dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

---

### Category 2: Gaming Contracts

#### baccarat ⚠️ UNKNOWN
- **Token Behavior**: Game coins (internal)
- **README**: References money_v2 for "Value transfer integration"
- **Verdict**: Needs source code review for actual token usage

#### roulette ⚠️ UNKNOWN
- **Token Behavior**: Game coins (internal)
- **README**: References money_v2 for "Value transfer integration"
- **Verdict**: Needs source code review for actual token usage

#### darktoshi_dice ⚠️ UNKNOWN
- **Token Behavior**: Game coins (internal)
- **README**: References money_v2 for "Value transfer integration"
- **Verdict**: Needs source code review for actual token usage

#### block_height_prediction ⚠️ UNKNOWN
- **Token Behavior**: PoW-based
- **Source**: Not reviewed
- **Verdict**: Needs source code review

---

### Category 3: Identity/Reputation

#### identity ⚠️ UNKNOWN
- **Token Behavior**: ZK credential proofs (no tokens)
- **Source**: Not reviewed
- **Verdict**: Likely standalone, no token integration needed

---

### Category 4: Infrastructure

#### deployooor ⚠️ UNKNOWN
- **Function**: Contract deployment
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### oracle ⚠️ UNKNOWN
- **Function**: Data feeds
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### subscription ❌ CRITICAL - Uses Deprecated money_v2
- **Cargo.toml**: No money_v3 dependency
- **pipeline.toml**: No dependencies
- **Source**: `src/contract/subscription/src/entrypoint.rs`
  - Line 518: `money::TransferV2` child call required
  - Line 544: Validates function code 0x03
- **Issue**: Endowment withdrawals require `money::TransferV2`
- **Verdict**: CRITICAL - Must migrate to MoneyV3

---

### Category 5: Escrow/Governance

#### escrow ⚠️ MEDIUM - References Deprecated money_v2
- **Cargo.toml**: No money_v3 dependency
- **README**: References money_v2 in documentation
- **Source**: Not reviewed (but HTLC may use money for transfers)
- **Verdict**: Needs source code review

#### dao_escrow ❌ CRITICAL - Uses Deprecated money_v2
- **Cargo.toml**: No money_v3 dependency
- **pipeline.toml**: `dependencies = []` (explicit empty)
- **Source**: `src/contract/dao_escrow/src/entrypoint.rs`
  - Line 369: `money::TransferV2` required for WithdrawV1
  - Line 494: `money::TransferV2` required for EndowmentWithdrawV1
  - Line 584: `money::TransferV2` required for TreasurySpendV1
- **Issue**: Multiple functions require money_v2::TransferV2
- **Verdict**: CRITICAL - Must migrate to MoneyV3

#### atomic_swap ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review (likely uses money_v2 or money_v3)

#### bridge ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### drain_protection ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### labor_market ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

#### tender ⚠️ UNKNOWN
- **Cargo.toml**: No token dependencies
- **Source**: Not reviewed
- **Verdict**: Needs source code review

---

### game_room ❌ CRITICAL - Uses Deprecated money_v2
- **Cargo.toml**: No money_v3 dependency
- **Source**: `src/contract/game_room/src/entrypoint/claim.rs`
  - Line 63: `money::TransferV2` required for claims
  - Line 77: Validates function code 0x03
- **Issue**: Claim function requires money_v2::TransferV2
- **Verdict**: CRITICAL - Must migrate to MoneyV3

---

## Contracts with Test Harnesses

| Contract | Harness | Token Integration | Status |
|----------|---------|-------------------|--------|
| dex | ✅ DexHarness | MoneyV3 via test harness only | Needs real integration |
| money_v3 | ✅ MoneyV3Harness | Native (MoneyV3) | ✅ CORRECT |
| native_token | ✅ NativeTokenHarness | Native (NativeToken) | ✅ CORRECT |
| stablecoin | ❌ No harness | MoneyV3 (direct) | ✅ CORRECT |
| money_v2 | ❌ No harness | DEPRECATED | N/A |

---

## Migration Path: money_v2 → MoneyV3

### MoneyV3 Equivalent Functions

| money_v2 Function | money_v3 Equivalent | Notes |
|--------------------|---------------------|-------|
| TransferV2 (0x03) | `transfer_v1` | Uses Poseidon-only circuits |
| OtcSwapV2 (0x04) | No direct equivalent | Need to implement atomic swap in MoneyV3 or use different pattern |

### Steps for Migration

1. **For contracts using TransferV2**:
   - Replace `money::TransferV2` child calls with `money_v3::transfer_v1` calls
   - Update function IDs and parameters
   - Test thoroughly

2. **For contracts using OtcSwapV2**:
   - MoneyV3 doesn't have OtcSwapV2 equivalent
   - Options:
     a. Implement `otc_swap_v1` in MoneyV3
     b. Use a different atomic swap pattern (e.g., hashlock + two transfers)

---

## Recommendations

### Immediate Actions (CRITICAL)

1. **dao_escrow**: Migrate from `money::TransferV2` to `money_v3::transfer_v1`
2. **game_room**: Migrate from `money::TransferV2` to `money_v3::transfer_v1`
3. **subscription**: Migrate from `money::TransferV2` to `money_v3::transfer_v1`
4. **dex**: Implement MoneyV3 equivalent for OtcSwapV2 OR use hashlock pattern

### Medium Priority

5. **escrow**: Audit source for money_v2 usage
6. **baccarat, roulette, darktoshi_dice**: Audit source for money_v2 usage
7. **lottery, auction, insurance_market**: Audit source for token handling

### Architecture Recommendation

Consider adding a **token registry** pattern where:
- Tokens are created via MoneyV3
- All contracts reference tokens by ID (not hardcoded)
- Fees always go to NativeToken
- Cross-contract transfers use MoneyV3::transfer_v1

---

## Appendix: money_v2 EC Heap Bug Details

Per `money_v2/README.md`:

> Money V2 contains EC heap bugs in 4 of its 5 circuits (Fee_V2, Mint_V2, Burn_V2, AuthTokenMint_V2). Only TokenMint_V2 is safe (Poseidon-only).

The buggy circuits use EC operations that can be exploited via malicious inputs. The TransferV2 and OtcSwapV2 entrypoints use v1 circuits internally (not the v2 circuits), but the overall money_v2 contract is still deprecated due to the pervasive EC heap bug pattern.

---

## Verification Commands

```bash
# Check for money_v2 references in contracts
grep -r "money_v2\|money::TransferV2\|money::OtcSwapV2" src/contract/*/src/

# Check for money_v3 usage in contracts
grep -r "darkfi_money_v3_contract\|money_v3::" src/contract/*/src/

# Build stablecoin (should work with money_v3)
cargo build --release -p darkfi_stablecoin_contract

# Verify dex pipeline (deployment works, but code uses money_v2)
CONTRACT_NAME=dex cargo test --package darkfid test_pipeline
```
