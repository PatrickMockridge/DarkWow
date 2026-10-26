//! ContractTestSpec for stablecoin — chain-level controls for the disabled custody arms.
//!
//! The contract does not custody collateral: a deposit is a `promissory_note::transfer_v1` whose
//! recipient the parent cannot check (the wire `Output` carries only commitments and an encrypted
//! note), so a depositor could keep the note and mint against `total_collateral` bookkeeping.
//! `OpenPositionV1`, `AddCollateralV1`, `MintStableV1`, `LiquidateV1` and `RedeemStableV1` are
//! refused with `CustodyModelUnavailable` before any state read (`OBL-C92`/`C272`/`C273`). This
//! spec is the chain-level twin of the host unit control in the contract crate: every disabled
//! selector must be rejected with `ContractError(Custom(32))`, and an enabled arm must not be
//! caught by the guard (it proceeds to its own business logic).
//!
//! **The refusal must be reached at `exec`, so the metadata gate has to answer.** A selector-only
//! call to an arm whose `get_metadata` decodes its params dies at `metadata-decode-zkp` /
//! `EMPTY metadata` and names the wrong gate; measured 2026-10-26 on the first version of this
//! spec, which was selector-only for all five (`/tmp/p2-stable.out`, 101.15s). The four arms that
//! publish proof instances (Open, Add, Mint, Liquidate) therefore carry their arm's real params,
//! built by the arm's own builder (no proving). `RedeemStableV1` is proofless and needs only a
//! selector — but only after `OBL-C278`: its metadata arm used to return a 0-byte buffer, which is
//! the host's rejection signal, so the arm's documented refusal was unreachable through
//! `accept_block` (the `OBL-C77` shape, as `tender`'s `close_tender` was).
//!
//! The former success-flow endpoints (open, mint, repay, liquidate, governance, collateral
//! add/remove) were removed with the arms they exercised. The rows they controlled were
//! pre-migration `settled` entries. The live row that cites this spec (`OBL-C206`) needs the
//! heavyweight harness to run deterministically, which this refusal list preserves. The disabled
//! exec functions remain in the contract for the custody-model redesign.
use dwow_contract_test_harness::harness::StablecoinHarness;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::crypto::{BaseBlind, IntentCommitment, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use dwow_stablecoin_contract::StablecoinFunction;

use super::helpers::mk_ep_rejecting_naming;
use crate::tests::uniform_runner::*;

/// A row that submits `call_data` and requires the custody guard's own refusal.
fn refusal(name: &'static str, call_data: Vec<u8>) -> EndpointSpec<'static> {
    mk_ep_rejecting_naming(
        name,
        false,
        &["ContractError(Custom(32))"],
        Box::new(move || {
            Ok(EndpointResult { children: vec![], call_data: call_data.clone(), proofs: vec![] })
        }),
    )
}

pub fn stablecoin_test_spec() -> ContractTestSpec<'static> {
    let cid = crate::tests::blockchain::derive_contract_id_from_name("stablecoin");
    let harness = Box::leak(Box::new(StablecoinHarness::spawn(cid)));
    let h: &StablecoinHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm");

    // Open and Add share `DepositCollateralParams`; only the selector differs.
    let open_call_data = h
        .open_position_prepare(
            pallas::Base::from(42u64),
            10_000,
            5_000,
            pallas::Base::from(1u64),
            BaseBlind::from_u64(3),
            BaseBlind::from_u64(4),
        )
        .expect("open_position params")
        .call_data;
    let mut add_call_data = open_call_data.clone();
    add_call_data[0] = StablecoinFunction::AddCollateralV1 as u8;

    let mint_call_data = h
        .mint_stable_prepare(
            pallas::Base::from(42u64),
            10_000,
            5_000,
            1_000,
            pallas::Base::from(1u64),
            BaseBlind::from_u64(3),
            BaseBlind::from_u64(4),
            pallas::Base::from(7u64),
        )
        .expect("mint_stable params")
        .call_data;

    // No prepare builder exists for `LiquidateV1`; its metadata arm only decodes, so an
    // internally consistent but arbitrary report is enough to reach `exec`.
    let liquidate_call_data = {
        use dwow_stablecoin_contract::model::LiquidateParams;
        let params = LiquidateParams {
            liquidation_commitment: IntentCommitment::from_bytes(
                pallas::Base::from(1u64).to_repr(),
            )
            .expect("canonical liquidation commitment"),
            total_debt: 5_000,
            total_collateral: 10_000,
            current_price: 1,
            debt_to_cover: 1_000,
            proof: vec![],
            liquidation_reward: 100,
            fee: 0,
            zk_public_inputs: vec![],
        };
        let mut call_data = vec![StablecoinFunction::LiquidateV1 as u8];
        call_data.extend_from_slice(&params.encode().expect("liquidate params encode"));
        call_data
    };

    ContractTestSpec {
        name: "stablecoin",
        is_genesis: false,
        contract_id: cid,
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            // Proofless after `OBL-C278`, so a selector is the whole call.
            refusal("redeem_disabled", vec![StablecoinFunction::RedeemStableV1 as u8]),
            refusal("open_position_disabled", open_call_data),
            refusal("add_collateral_disabled", add_call_data),
            refusal("mint_stable_disabled", mint_call_data),
            refusal("liquidate_disabled", liquidate_call_data),
            // The negative twin at chain level: `GovernanceReportV1` is enabled, so it must pass
            // the custody guard and fail on its own rule — never with `Custom(32)`. It needs
            // decodable params to get past the metadata gate at all (a selector-only call is
            // refused there, which is what the first run of this row measured); with them, the
            // first exec check compares the reporter against the governance authority written at
            // initialization, and this fixture's reporter is not it: `ConfigError`, `Custom(30)`
            // (measured — the first guess, an unconfigured authority, was wrong because the deploy
            // path does write one).
            mk_ep_rejecting_naming(
                "governance_report_not_caught_by_the_custody_guard",
                false,
                &["ContractError(Custom(30))"],
                Box::new(|| {
                    use dwow_stablecoin_contract::model::GovernanceReportParams;
                    let params = GovernanceReportParams {
                        asset_id: pallas::Base::from(1u64),
                        total_collateral: 0,
                        total_debt: 0,
                        total_redeemed: 0,
                        outstanding: 0,
                        collateral_ratio_bps: 10_000,
                        interest_accrued: 0,
                        reporter_pub: PublicKey::from_secret(SecretKey::from_base(
                            pallas::Base::from(1u64),
                        )),
                        proof: vec![],
                        fee: 0,
                    };
                    let mut call_data = vec![StablecoinFunction::GovernanceReportV1 as u8];
                    call_data.extend_from_slice(
                        &params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?,
                    );
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }),
            ),
        ],
    }
}
