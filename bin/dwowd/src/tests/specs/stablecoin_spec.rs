use dwow_contract_test_harness::harness::{ContractHarness, StablecoinHarness};
use dwow_sdk::crypto::BaseBlind;
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;
pub fn stablecoin_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(StablecoinHarness::spawn()));
    let h: &StablecoinHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/stablecoin/dwow_stablecoin_contract.wasm");
    let sk = pallas::Base::from(10u64);
    ContractTestSpec { name: "stablecoin", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![
            mk_ep("OpenPositionV1", true, Box::new(move || {
                let r = h.open_position(sk, 10000, 5000, pallas::Base::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("MintStableV1", true, Box::new(move || {
                let r = h.mint_stable(sk, 10000, 5000, 1000, BaseBlind::from(100u64), BaseBlind::from(200u64), pallas::Base::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("LiquidateV1", true, Box::new(move || {
                let r = h.liquidate(sk, 10000, 5000, 200, 1000, 500, BaseBlind::from(100u64), BaseBlind::from(200u64), pallas::Base::from(1u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("GovernanceReportV1", true, Box::new(move || {
                let r = h.governance_report(sk, 10000, 5000, 10, 3600, 42).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("AccrueInterestV1", true, Box::new(move || {
                let r = h.accrue_interest(sk, 5000, 10, 3600).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
