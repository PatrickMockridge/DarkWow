/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 * ... (license header)
 */

//! ZK Audit Test — verifies every contract harness has complete ZK circuit coverage.

use dwow_contract_test_harness::harness::ContractHarness;

macro_rules! zk_check {
    ($harness:ty, $name:expr) => {{
        let h = <$harness>::spawn();
        if let Err(e) = h.verify_zk_coverage() {
            panic!("{} ZK coverage FAILED: {}", $name, e);
        }
        assert!(!h.circuits().is_empty(), "{}: circuits() must be non-empty", $name);
        assert_eq!(h.name(), $name, "{}: name() must match", $name);
    }};
}

#[test]
fn test_all_harnesses_zk_coverage() {
    use dwow_contract_test_harness::harness::*;

    zk_check!(AttestationHarness, "attestation");
    zk_check!(AuctionHarness, "auction");
    zk_check!(BaccaratHarness, "baccarat");
    zk_check!(BearerBondHarness, "bearer_bond");
    zk_check!(BettingStakeHarness, "betting_stake");
    zk_check!(BoxHarness, "box");
    zk_check!(BridgeHarness, "bridge");
    zk_check!(DaoEscrowHarness, "dao_escrow");
    zk_check!(DarkbetExchangeHarness, "darkbet_exchange");
    zk_check!(DarkToshiDiceHarness, "darktoshi_dice");
    // Deployooor has NO ZK circuits — pure WASM contract. Skip circuits() check.
    zk_check!(DexHarness, "dex");
    zk_check!(DrainProtectionHarness, "drain_protection");
    zk_check!(EscrowHarness, "escrow");
    zk_check!(GameRoomHarness, "game_room");
    zk_check!(IdentityHarness, "identity");
    zk_check!(InsuranceMarketHarness, "insurance_market");
    zk_check!(LaborMarketHarness, "labor_market");
    zk_check!(LotteryHarness, "lottery");
    zk_check!(MultiSigHarness, "multisig");
    zk_check!(NativeTokenHarness, "native_token");
    zk_check!(OracleHarness, "oracle");
    zk_check!(OtcSwapHarness, "otc_swap");
    zk_check!(PoolStakeHarness, "pool_stake");
    zk_check!(PromissoryNoteHarness, "promissory_note");
    zk_check!(PurseHarness, "purse");
    zk_check!(RelayerEndowmentHarness, "relayer_endowment");
    zk_check!(RouletteHarness, "roulette");
    zk_check!(SlotHarness, "slot");
    zk_check!(StablecoinHarness, "stablecoin");
    zk_check!(SubscriptionHarness, "subscription");
    zk_check!(TenderHarness, "tender");
}
