//! ContractTestSpec for subscription. Tier: HARVESTABLE — 5 harness methods.
//! 2 active (cancel, renew — empty_witnesses circuits), 3 pending.
use dwow_contract_test_harness::harness::{SubscriptionHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn subscription_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SubscriptionHarness::spawn()));
    let h: &SubscriptionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/subscription/dwow_subscription_contract.wasm");
    let sub_secret = pallas::Base::from(10u64);
    let sub_pub = PublicKey::from_secret(SecretKey::from_base(sub_secret));
    let subscription_id = pallas::Base::from(1u64);

    ContractTestSpec {
        name: "subscription", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![
            mk_ep("CancelV1", true, Box::new(move || {
                let r = h.cancel(subscription_id, sub_secret, pallas::Base::from(99u64), 100, sub_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RenewV1", true, Box::new(move || {
                let r = h.renew(subscription_id, sub_secret, 200, pallas::Base::from(99u64), pallas::Point::default(), vec![pallas::Base::from(0u64)]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
