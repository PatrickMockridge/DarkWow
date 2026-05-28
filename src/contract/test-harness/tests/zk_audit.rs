/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! CI Audit — ZK Binary Integrity and Harness Coverage
//!
//! Two tests:
//!
//! 1. `test_all_zk_binaries_decode` (fast) — decodes every .zk.bin file that
//!    is loaded by a harness via `include_bytes!`. No proving key building.
//!    Catches corrupted files and namespace mismatches at compile time.
//!
//! 2. `test_harness_circuits_match_zkbins` (slow, #[ignore]d) — instantiates
//!    each harness and cross-checks its `circuits()` list against zkbin files
//!    on disk. Catches harnesses that load a zkbin but forget to list it.
//!
//! `deployooor` is the sole contract with no ZK circuits.

use std::collections::BTreeMap;
use dwow_core::zkas::ZkBinary;

fn decode(data: &[u8]) -> String {
    ZkBinary::decode(data, false)
        .expect("Failed to decode zkbin — file may be corrupted or in an unsupported format")
        .namespace
}

/// Return (contract_name, Vec<namespace>) for every zkbin file loaded by a harness.
/// This mirrors the `include_bytes!` calls in each harness's `spawn()` method.
fn harness_zkbin_namespaces() -> BTreeMap<&'static str, Vec<String>> {
    let mut map = BTreeMap::new();

    map.insert("attestation", vec![
        decode(include_bytes!("../../attestation/proof/verify_claim_v1.zk.bin")),
        decode(include_bytes!("../../attestation/proof/create_attestation_v1.zk.bin")),
        decode(include_bytes!("../../attestation/proof/create_claim_v1.zk.bin")),
        decode(include_bytes!("../../attestation/proof/consume_claim_v1.zk.bin")),
        decode(include_bytes!("../../attestation/proof/delegate_attestation_v1.zk.bin")),
    ]);

    map.insert("auction", vec![
        decode(include_bytes!("../../auction/proof/create_auction_v1.zk.bin")),
        decode(include_bytes!("../../auction/proof/place_bid_v1.zk.bin")),
        decode(include_bytes!("../../auction/proof/close_auction_v1.zk.bin")),
        decode(include_bytes!("../../auction/proof/claim_winnings_v1.zk.bin")),
        decode(include_bytes!("../../auction/proof/settle_auction_v1.zk.bin")),
        decode(include_bytes!("../../auction/proof/refund_bid_v1.zk.bin")),
    ]);

    map.insert("baccarat", vec![
        decode(include_bytes!("../../baccarat/proof/commit_bet_v1.zk.bin")),
        decode(include_bytes!("../../baccarat/proof/settle_bet_v1.zk.bin")),
    ]);

    map.insert("betting_stake", vec![
        decode(include_bytes!("../../betting_stake/proof/init_v1.zk.bin")),
        decode(include_bytes!("../../betting_stake/proof/stake_v1.zk.bin")),
        decode(include_bytes!("../../betting_stake/proof/claim_v1.zk.bin")),
        decode(include_bytes!("../../betting_stake/proof/unstake_v1.zk.bin")),
        decode(include_bytes!("../../betting_stake/proof/update_risk_v1.zk.bin")),
    ]);

    map.insert("bridge", vec![
        decode(include_bytes!("../../bridge/proof/deposit_v1.zk.bin")),
        decode(include_bytes!("../../bridge/proof/withdraw_v1.zk.bin")),
    ]);

    map.insert("dao_escrow", vec![
        decode(include_bytes!("../../dao_escrow/proof/init_v1.zk.bin")),
        decode(include_bytes!("../../dao_escrow/proof/pay_premium_v1.zk.bin")),
        decode(include_bytes!("../../dao_escrow/proof/propose_claim_v1.zk.bin")),
        decode(include_bytes!("../../dao_escrow/proof/vote_claim_v1.zk.bin")),
        decode(include_bytes!("../../dao_escrow/proof/resolve_dispute_v1.zk.bin")),
        decode(include_bytes!("../../dao_escrow/proof/verify_member_capability_v1.zk.bin")),
    ]);

    map.insert("darkbet_exchange", vec![
        decode(include_bytes!("../../darkbet_exchange/proof/create_market_v1.zk.bin")),
        decode(include_bytes!("../../darkbet_exchange/proof/buy_position_v1.zk.bin")),
        decode(include_bytes!("../../darkbet_exchange/proof/claim_winnings_v1.zk.bin")),
        decode(include_bytes!("../../darkbet_exchange/proof/add_liquidity_v1.zk.bin")),
    ]);

    map.insert("darktoshi_dice", vec![
        decode(include_bytes!("../../darktoshi_dice/proof/commit_bet_v1.zk.bin")),
        decode(include_bytes!("../../darktoshi_dice/proof/settle_bet_v1.zk.bin")),
    ]);

    map.insert("dex", vec![
        decode(include_bytes!("../../dex/proof/create_swap_v1.zk.bin")),
        decode(include_bytes!("../../dex/proof/accept_swap_v1.zk.bin")),
        decode(include_bytes!("../../dex/proof/execute_swap_v1.zk.bin")),
        decode(include_bytes!("../../dex/proof/cancel_swap_v1.zk.bin")),
    ]);

    map.insert("drain_protection", vec![
        decode(include_bytes!("../../drain_protection/proof/exit_v1.zk.bin")),
    ]);

    map.insert("escrow", vec![
        decode(include_bytes!("../../escrow/proof/create_escrow_v1.zk.bin")),
        decode(include_bytes!("../../escrow/proof/fund_v1.zk.bin")),
        decode(include_bytes!("../../escrow/proof/claim_v1.zk.bin")),
        decode(include_bytes!("../../escrow/proof/refund_v1.zk.bin")),
    ]);

    map.insert("game_room", vec![
        decode(include_bytes!("../../game_room/proof/create_room_v1.zk.bin")),
        decode(include_bytes!("../../game_room/proof/deposit_v1.zk.bin")),
        decode(include_bytes!("../../game_room/proof/place_bet_v1.zk.bin")),
        decode(include_bytes!("../../game_room/proof/settle_pot_v1.zk.bin")),
        decode(include_bytes!("../../game_room/proof/claim_v1.zk.bin")),
    ]);

    map.insert("identity", vec![
        decode(include_bytes!("../../identity/proof/create_claim_v1.zk.bin")),
        decode(include_bytes!("../../identity/proof/create_claim_v1_l1.zk.bin")),
        decode(include_bytes!("../../identity/proof/create_claim_v1_l1_v2.zk.bin")),
        decode(include_bytes!("../../identity/proof/create_claim_v1_multi.zk.bin")),
        decode(include_bytes!("../../identity/proof/create_claim_v1_ratio.zk.bin")),
        decode(include_bytes!("../../identity/proof/create_claim_v1_dag.zk.bin")),
        decode(include_bytes!("../../identity/proof/issue_credential_v1.zk.bin")),
        decode(include_bytes!("../../identity/proof/verify_capability_v1.zk.bin")),
    ]);

    map.insert("insurance_market", vec![
        decode(include_bytes!("../../insurance_market/proof/underwrite_with_capability_v1.zk.bin")),
        decode(include_bytes!("../../insurance_market/proof/purchase_coverage_with_capability_v1.zk.bin")),
    ]);

    map.insert("labor_market", vec![
        decode(include_bytes!("../../labor_market/proof/create_job_v1.zk.bin")),
        decode(include_bytes!("../../labor_market/proof/accept_job_v1.zk.bin")),
        decode(include_bytes!("../../labor_market/proof/submit_deliverable_v1.zk.bin")),
        decode(include_bytes!("../../labor_market/proof/submit_git_deliverable_v1.zk.bin")),
        decode(include_bytes!("../../labor_market/proof/confirm_delivery_v1.zk.bin")),
        decode(include_bytes!("../../labor_market/proof/dispute_v1.zk.bin")),
        decode(include_bytes!("../../labor_market/proof/refund_v1.zk.bin")),
    ]);

    map.insert("lottery", vec![
        decode(include_bytes!("../../lottery/proof/commit_ticket_v1.zk.bin")),
        decode(include_bytes!("../../lottery/proof/reveal_ticket_v1.zk.bin")),
    ]);

    map.insert("promissory_note", vec![
        decode(include_bytes!("../../promissory_note/proof/mint_v1.zk.bin")),
        decode(include_bytes!("../../promissory_note/proof/burn_v1.zk.bin")),
        decode(include_bytes!("../../promissory_note/proof/token_mint_v1.zk.bin")),
    ]);

    map.insert("native_token", vec![
        decode(include_bytes!("../../native_token/proof/mint_v1.zk.bin")),
        decode(include_bytes!("../../native_token/proof/burn_v1.zk.bin")),
        decode(include_bytes!("../../native_token/proof/fee_v1.zk.bin")),
    ]);

    map.insert("oracle", vec![
        decode(include_bytes!("../../oracle/proof/register_oracle_v1.zk.bin")),
    ]);

    map.insert("pool_stake", vec![
        decode(include_bytes!("../../pool_stake/proof/create_pool_v1.zk.bin")),
        decode(include_bytes!("../../pool_stake/proof/join_pool_v1.zk.bin")),
        decode(include_bytes!("../../pool_stake/proof/allocate_coverage_v1.zk.bin")),
        decode(include_bytes!("../../pool_stake/proof/slash_coverage_v1.zk.bin")),
    ]);

    map.insert("relayer_endowment", vec![
        decode(include_bytes!("../../relayer_endowment/proof/initialize_v1.zk.bin")),
        decode(include_bytes!("../../relayer_endowment/proof/deploy_capital_v1.zk.bin")),
        decode(include_bytes!("../../relayer_endowment/proof/claim_fees_v1.zk.bin")),
    ]);

    map.insert("roulette", vec![
        decode(include_bytes!("../../roulette/proof/place_bet_v1.zk.bin")),
        decode(include_bytes!("../../roulette/proof/settle_bet_v1.zk.bin")),
    ]);

    map.insert("slot", vec![
        decode(include_bytes!("../../slot/proof/commit_bet_v1.zk.bin")),
        decode(include_bytes!("../../slot/proof/settle_bet_v1.zk.bin")),
    ]);

    map.insert("stablecoin", vec![
        decode(include_bytes!("../../stablecoin/proof/open_position_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/mint_stable_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/liquidate_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/governance_report_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/accrue_interest_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/add_collateral_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/remove_collateral_v1.zk.bin")),
        decode(include_bytes!("../../stablecoin/proof/repay_stable_v1.zk.bin")),
    ]);

    map.insert("subscription", vec![
        decode(include_bytes!("../../subscription/proof/subscribe_v1.zk.bin")),
        decode(include_bytes!("../../subscription/proof/verify_access_v1.zk.bin")),
        decode(include_bytes!("../../subscription/proof/update_usage_v1.zk.bin")),
    ]);

    map.insert("tender", vec![
        decode(include_bytes!("../../tender/proof/create_tender_v1.zk.bin")),
        decode(include_bytes!("../../tender/proof/submit_bid_v1.zk.bin")),
        decode(include_bytes!("../../tender/proof/reveal_bid_v1.zk.bin")),
        decode(include_bytes!("../../tender/proof/select_winner_v1.zk.bin")),
    ]);

    map
}

// ═══════════════════════════════════════════════════════════════════════
// Test 1: All harness-loaded zkbin files decode successfully
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_all_zk_binaries_decode() {
    let map = harness_zkbin_namespaces();
    let total: usize = map.values().map(|v| v.len()).sum();
    assert!(total > 0, "No zkbin files found — something is wrong");
    println!("All {total} harness-loaded zkbin files decoded successfully across {} contracts", map.len());
}

// ═══════════════════════════════════════════════════════════════════════
// Test 2: Harness circuits() matches zkbin files on disk (slow, nightly)
// ═══════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "slow: builds proving keys for all 27 harnesses; run nightly with RAYON_NUM_THREADS=20"]
fn test_harness_circuits_match_zkbins() {
    use dwow_contract_test_harness::harness::{
        ContractHarness,
        AttestationHarness, AuctionHarness, BaccaratHarness, BettingStakeHarness,
        BridgeHarness, DaoEscrowHarness, DarkbetExchangeHarness, DarkToshiDiceHarness,
        DeployooorHarness, DexHarness, DrainProtectionHarness, EscrowHarness,
        GameRoomHarness, IdentityHarness, InsuranceMarketHarness, LaborMarketHarness,
        LotteryHarness, PromissoryNoteHarness, NativeTokenHarness, OracleHarness,
        PoolStakeHarness, RelayerEndowmentHarness, RouletteHarness, SlotHarness,
        StablecoinHarness, SubscriptionHarness, TenderHarness,
    };

    let zkbin_map = harness_zkbin_namespaces();
    let mut failures: Vec<String> = Vec::new();

    macro_rules! check {
        ($harness:ident, $allow_empty:literal) => {
            let h = $harness::spawn();
            let name = h.name();
            let circuits = h.circuits();

            if circuits.is_empty() && !$allow_empty {
                failures.push(format!(
                    "{name}: circuits() returned empty. \
                     deployooor is the only contract allowed to have \
                     no ZK circuits."
                ));
            } else if let Err(e) = h.verify_zk_coverage() {
                failures.push(format!("{name}: {e}"));
            }

            // Cross-check: every harness-loaded zkbin should be in circuits()
            if let Some(disk_namespaces) = zkbin_map.get(name) {
                for ns in disk_namespaces {
                    if !circuits.contains(&ns.as_str()) {
                        failures.push(format!(
                            "{name}: zkbin for \"{ns}\" is loaded but \
                             NOT in circuits(). Add it to the harness."
                        ));
                    }
                }
                // Reverse: every circuit should have a corresponding zkbin loaded
                for ns in &circuits {
                    if !disk_namespaces.iter().any(|d| d == ns) {
                        failures.push(format!(
                            "{name}: \"{ns}\" is in circuits() but no \
                             matching zkbin loaded in spawn()."
                        ));
                    }
                }
            } else if !circuits.is_empty() {
                failures.push(format!(
                    "{name}: has circuits but no entry in harness_zkbin_namespaces()"
                ));
            }
        };
    }

    check!(AttestationHarness, false);
    check!(AuctionHarness, false);
    check!(BaccaratHarness, false);
    check!(BettingStakeHarness, false);
    check!(BridgeHarness, false);
    check!(DaoEscrowHarness, false);
    check!(DarkbetExchangeHarness, false);
    check!(DarkToshiDiceHarness, false);
    check!(DeployooorHarness, true);
    check!(DexHarness, false);
    check!(DrainProtectionHarness, false);
    check!(EscrowHarness, false);
    check!(GameRoomHarness, false);
    check!(IdentityHarness, false);
    check!(InsuranceMarketHarness, false);
    check!(LaborMarketHarness, false);
    check!(LotteryHarness, false);
    check!(PromissoryNoteHarness, false);
    check!(NativeTokenHarness, false);
    check!(OracleHarness, false);
    check!(PoolStakeHarness, false);
    check!(RelayerEndowmentHarness, false);
    check!(RouletteHarness, false);
    check!(SlotHarness, false);
    check!(StablecoinHarness, false);
    check!(SubscriptionHarness, false);
    check!(TenderHarness, false);

    assert!(
        failures.is_empty(),
        "ZK coverage gaps detected:\n\n{}\n",
        failures.join("\n")
    );
}
