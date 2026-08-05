//! ContractTestSpec for labor_market. Tier: HARVESTABLE — 9 harness methods, all ZK.
//! Cross-contract deps on identity + attestation.
use dwow_contract_test_harness::harness::{LaborMarketHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn labor_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(LaborMarketHarness::spawn()));
    let h: &LaborMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/labor_market/dwow_labor_market_contract.wasm");
    let employer_secret = pallas::Base::from(10u64);
    let employer_pub = PublicKey::from_secret(SecretKey::from_base(employer_secret));
    let worker_secret = pallas::Base::from(20u64);
    let worker_pub = PublicKey::from_secret(SecretKey::from_base(worker_secret));
    let job_id = pallas::Base::from(100u64);

    ContractTestSpec {
        name: "labor_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![
            mk_ep("AcceptJobV1", true, Box::new(move || {
                let r = h.accept_job(worker_secret, worker_pub, job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ConfirmDeliveryV1", true, Box::new(move || {
                let r = h.confirm_delivery(employer_secret, employer_pub, job_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
