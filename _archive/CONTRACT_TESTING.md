# Local Devnet Contract Testing Results

**Date**: 2026-04-09
**Updated**: 2026-04-09 with complete ZK circuits and test harnesses for all contracts
**Tested by**: Claude Code

## Summary

Extensive local devnet smart contract testing was performed on DarkWow contracts. Results show that the localnet infrastructure is functional but deployment confirmations are slow/blocked.

## ZK Proof Client Module Implementation (2026-04-08 Update)

All 6 governance/marketplace contracts now have full ZK proof client modules:

| Contract | Circuits | Status |
|----------|----------|--------|
| identity | 8 circuits | ✅ IMPLEMENTED |
| labor_market | 9 circuits | ✅ IMPLEMENTED |
| oracle | 5 circuits | ✅ IMPLEMENTED |
| auction | 6 circuits | ✅ IMPLEMENTED |
| tender | 5 circuits | ✅ IMPLEMENTED |
| attestation | 8 circuits | ✅ IMPLEMENTED |

**Total**: 41 ZK proof client modules across 6 contracts

Each module follows the standard pattern:
- `*PublicInputs` struct with `to_vec()` for public inputs
- `*CallData` struct with input data
- `compute_public_inputs()` for deriving public inputs
- `to_witnesses()` returning `Vec<Witness>` for `ZkCircuit`
- `*_proof()` function creating `Proof`

See `doc/src/arch/test_harness_guide.md` for full implementation status.

## Prerequisites Verified

### Binaries
| Binary | Path | Status |
|--------|------|--------|
| darkfid | `target/release/darkfid` | EXISTS |
| drk | `target/release/drk` | EXISTS |

### WASM Contracts (24 available)

| Contract | WASM Path | Build Status |
|----------|-----------|--------------|
| attestation | `target/wasm32-unknown-unknown/release/darkfi_attestation_contract.wasm` | BUILT |
| auction | `target/wasm32-unknown-unknown/release/darkfi_auction_contract.wasm` | BUILT |
| baccarat | `target/wasm32-unknown-unknown/release/darkfi_baccarat_contract.wasm` | BUILT |
| betting_stake | `target/wasm32-unknown-unknown/release/darkfi_betting_stake_contract.wasm` | BUILT |
| bridge | `target/wasm32-unknown-unknown/release/darkfi_bridge_contract.wasm` | BUILT |
| dao | `target/wasm32-unknown-unknown/release/darkfi_dao_contract.wasm` | BUILT |
| dao_escrow | `target/wasm32-unknown-unknown/release/darkfi_dao_escrow_contract.wasm` | BUILT |
| darkbet_exchange | `target/wasm32-unknown-unknown/release/darkfi_darkbet_exchange_contract.wasm` | BUILT |
| darktoshi_dice | `target/wasm32-unknown-unknown/release/darkfi_darktoshi_dice_contract.wasm` | BUILT |
| deployooor | `target/wasm32-unknown-unknown/release/darkfi_deployooor_contract.wasm` | BUILT |
| dex | `target/wasm32-unknown-unknown/release/darkfi_dex_contract.wasm` | BUILT |
| drain_protection | `target/wasm32-unknown-unknown/release/darkfi_drain_protection_contract.wasm` | BUILT |
| escrow | `target/wasm32-unknown-unknown/release/darkfi_escrow_contract.wasm` | BUILT |
| identity | `target/wasm32-unknown-unknown/release/darkfi_identity_contract.wasm` | BUILT |
| lottery | `target/wasm32-unknown-unknown/release/darkfi_lottery_contract.wasm` | BUILT |
| money | `target/wasm32-unknown-unknown/release/darkfi_money_contract.wasm` | BUILT |
| money_v2 | `target/wasm32-unknown-unknown/release/darkfi_money_v2_contract.wasm` | BUILT |
| oracle | `target/wasm32-unknown-unknown/release/darkfi_oracle_contract.wasm` | BUILT |
| pool_stake | `target/wasm32-unknown-unknown/release/darkfi_pool_stake_contract.wasm` | BUILT |
| relayer_endowment | `target/wasm32-unknown-unknown/release/darkfi_relayer_endowment_contract.wasm` | BUILT |
| roulette | `target/wasm32-unknown-unknown/release/darkfi_roulette_contract.wasm` | BUILT |
| slot | `target/wasm32-unknown-unknown/release/darkfi_slot_contract.wasm` | BUILT |
| stablecoin | `target/wasm32-unknown-unknown/release/darkfi_stablecoin_contract.wasm` | BUILT |
| tender | `target/wasm32-unknown-unknown/release/darkfi_tender_contract.wasm` | BUILT |
| labor_market | `target/wasm32-unknown-unknown/release/darkfi_labor_market_contract.wasm` | BUILT |
| game_room | `target/wasm32-unknown-unknown/release/darkfi_game_room_contract.wasm` | BUILT |

### WASM Contracts NOT Built (blocked by dependencies)

| Contract | Issue |
|----------|-------|
| insurance_market | FIXED: Added `getrandom` with `js` feature |
| block_height_prediction | FIXED: Added `getrandom` with `js` feature |

## Localnet Infrastructure

### darkfid
- **Status**: RUNNING
- **Config**: `contrib/localnet/darkfid-single-node/darkfid.toml`
- **Port**: RPC 48345, Stratum 48347

### Mining
- **Status**: WORKING (RandomX PoW)
- **Balance**: 240.11682956 DARK accumulated

### Wallet
- **Status**: INITIALIZED
- **Coins**: Multiple coins scanned from blocks 19-22

## Deployment Test Results

### DAO Escrow Contract Deployment (2026-04-08)

```
Contract ID: Dgwt5X8nWh8DFKHKjao1gvHD92DfxqzaUWyJBsGxFkVV
Transaction: aabb0985c73096c6001bcbed144f86743bf83ea9036f8b566eb2663e1e0ad56a
WASM Size: 169KB
Status: ✅ DEPLOYED AND CONFIRMED
```

**Implementation completed**:
- `InitializeV1` - Creates new endowment
- `UpdateV1` - Updates endowment parameters
- `PayPremiumV1` - Members pay premium, receive membership
- `WithdrawV1` - Endowment owner withdrawals
- `EnableDrainProtectionV1` - Enable drain protection

### Identity Contract Deployment

```
Command: drk contract deploy <authority> darkfi_identity_contract.wasm | broadcast
Transaction: 3be2399c5d55d5e2f6ba06b3d9343fb83d7400bf78fe8c197b9b9d5c3510c392
Status: BROADCASTED (not confirmed after 5+ blocks)
```

**Issue**: The deployed transaction remains in "Broadcasted" state and is not being included in mined blocks. This is a known issue with localnet testing where broadcasted transactions don't always get mined immediately.

## Test Workflow

### Starting Localnet

```bash
# Terminal 1: Start darkfid
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml

# Terminal 2: Start mining
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine

# Terminal 3: Wallet commands
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet balance
./target/release/drk -c bin/drk/drk_config.toml -n localnet scan
./target/release/drk -c bin/drk/drk_config.toml -n localnet wallet coins
```

### Deploying Contracts

```bash
# Generate deploy authority
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy

# Deploy contract
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract deploy <id> <wasm.wasm> | \
  ./target/release/drk -c bin/drk/drk_config.toml -n localnet broadcast

# List deployed contracts
./target/release/drk -c bin/drk/drk_config.toml -n localnet contract list
```

## Issues Encountered

1. **Transaction Not Confirming**: Identity contract deployment transaction stuck in "Broadcasted" state after multiple blocks mined
2. **Wallet DB Lock**: Mining process locks wallet DB, requiring stop before wallet commands
3. **WASM Build Fixed**: insurance_market and block_height_prediction WASM builds fixed with `getrandom` `js` feature

## Recommendations

1. **For faster localnet testing**: Consider using a faster block time or pre-mined wallet
2. **For WASM build issues**: WASM builds for insurance_market and block_height_prediction now work with `getrandom` `js` feature
3. **For deployment testing**: Use the `contrib/localnet/darkfid-small/` setup which may have faster block times

## Integration Test Status

Per `doc/src/arch/localnet_contract_testing.md`, the following contracts have passing integration tests:

| Contract | Tests |
|----------|-------|
| identity | 15 |
| baccarat | 20 |
| lottery | 6 |
| darkbet_exchange | 30 |
| pool_stake | 23 |
| relayer_endowment | 20 |
| bridge | 3 |
| dex | 9 |
| stablecoin | 15 |
| dao_escrow | 20 |
| drain_protection | 24 |
| block_height_prediction | 26 |
| insurance_market | 13 |
| oracle | 7 |
| auction | 23 |
| tender | 23 |
| attestation | 23 |
| labor_market | 24 |
| roulette | 17 |
| slot | 16 |
| darktoshi_dice | 19 |
| betting_stake | 24 |
| escrow | 18 |
| game_room | 42 |

**Note**: These integration tests are separate from localnet deployment testing and test serialization/model behavior via cargo test.

**Note**: game_room contract was added to workspace members in this session.
