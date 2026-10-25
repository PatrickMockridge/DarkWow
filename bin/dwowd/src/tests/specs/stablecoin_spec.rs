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
//! The former success-flow endpoints (open, mint, repay, liquidate, governance, collateral
//! add/remove) were removed with the arms they exercised. The rows they controlled were
//! pre-migration `settled` entries. The live row that cites this spec (`OBL-C206`) needs the
//! heavyweight harness to run deterministically, which this refusal list preserves. The disabled
//! exec functions remain in the contract for the custody-model redesign.
use dwow_contract_test_harness::harness::StablecoinHarness;
use dwow_stablecoin_contract::StablecoinFunction;

use super::helpers::mk_ep_rejecting_naming;
use crate::tests::uniform_runner::*;

/// A disabled custody arm, called with the selector and no params. The refusal is selector-level
/// and happens before any parameter decode, so the empty tail is the honest minimal call.
fn refusal(name: &'static str, func: StablecoinFunction) -> EndpointSpec<'static> {
    let selector = func as u8;
    mk_ep_rejecting_naming(
        name,
        false,
        &["ContractError(Custom(32))"],
        Box::new(move || {
            Ok(EndpointResult { children: vec![], call_data: vec![selector], proofs: vec![] })
        }),
    )
}

pub fn stablecoin_test_spec() -> ContractTestSpec<'static> {
    let cid = crate::tests::blockchain::derive_contract_id_from_name("stablecoin");
    let harness = Box::leak(Box::new(StablecoinHarness::spawn(cid)));
    let wasm = include_bytes!("../../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm");
    ContractTestSpec {
        name: "stablecoin",
        is_genesis: false,
        contract_id: cid,
        harness,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            refusal("open_position_disabled", StablecoinFunction::OpenPositionV1),
            refusal("add_collateral_disabled", StablecoinFunction::AddCollateralV1),
            refusal("mint_stable_disabled", StablecoinFunction::MintStableV1),
            refusal("liquidate_disabled", StablecoinFunction::LiquidateV1),
            refusal("redeem_disabled", StablecoinFunction::RedeemStableV1),
            // The negative twin at chain level: `GovernanceReportV1` is enabled, so it must pass
            // the custody guard and fail on its own rule — with no params its decoder refuses —
            // never with `Custom(32)`. The needle is the decoder's own error, not the guard's.
            mk_ep_rejecting_naming(
                "governance_report_not_caught_by_the_custody_guard",
                false,
                &["ContractError(IoError"],
                Box::new(|| {
                    Ok(EndpointResult {
                        children: vec![],
                        call_data: vec![StablecoinFunction::GovernanceReportV1 as u8],
                        proofs: vec![],
                    })
                }),
            ),
        ],
    }
}
