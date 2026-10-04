/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 * ... (license header)
 */

//! ZK Audit Test — verifies every contract harness has complete ZK circuit coverage.

use dwow_contract_test_harness::harness::ContractHarness;

/// The coverage audit for a harness whose `spawn` takes no contract id.
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

/// The same audit for a harness migrated to `OBL-C198`, whose `spawn` takes the contract id.
///
/// The id is a placeholder and never reaches a commitment: this audit calls
/// `verify_zk_coverage`, `circuits()` and `name()`, none of which builds a proof. It is passed
/// explicitly rather than defaulted because `spawn` takes it for the builders that *do* prove —
/// the transaction commitment is a derivation over the call set, and a call carries the contract
/// it addresses, so a harness that proves must be told which contract it is proving for.
///
/// A separate macro rather than changing every harness at once: contracts migrate one at a time,
/// so their audit lines move to this one as they do. When the last has migrated, the two collapse
/// back into one.
macro_rules! zk_check_with_id {
    ($harness:ty, $name:expr) => {{
        let h = <$harness>::spawn(
            dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("zero is canonical"),
        );
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

    zk_check_with_id!(AttestationHarness, "attestation");
    zk_check!(AuctionHarness, "auction");
    zk_check!(BaccaratHarness, "baccarat");
    zk_check!(BearerBondHarness, "bearer_bond");
    zk_check_with_id!(BettingStakeHarness, "betting_stake");
    zk_check!(BoxHarness, "box");
    zk_check!(BridgeHarness, "bridge");
    zk_check!(DaoEscrowHarness, "dao_escrow");
    zk_check!(DarkbetExchangeHarness, "darkbet_exchange");
    zk_check_with_id!(DarkToshiDiceHarness, "darktoshi_dice");
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
    zk_check_with_id!(OtcSwapHarness, "otc_swap");
    zk_check!(PoolStakeHarness, "pool_stake");
    zk_check!(PromissoryNoteHarness, "promissory_note");
    zk_check_with_id!(PurseHarness, "purse");
    zk_check!(RelayerEndowmentHarness, "relayer_endowment");
    zk_check!(RouletteHarness, "roulette");
    zk_check!(SlotHarness, "slot");
    zk_check!(StablecoinHarness, "stablecoin");
    zk_check!(SubscriptionHarness, "subscription");
    zk_check!(TenderHarness, "tender");
}
