//! ContractTestSpec for subscription. Tier: HARVESTABLE — 5 harness methods.
//! 5/5 endpoints active (2 empty_witnesses, 3 real ZK including 25-param subscribe).
use dwow_contract_test_harness::harness::{SubscriptionHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey, MerkleNode};
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
            mk_ep("SubscribeV1", true, Box::new(move || {
                let r = h.subscribe(sub_secret, pallas::Base::from(1u64), vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Scalar::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), 1000, pallas::Base::from(4u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64))], 0, vec![MerkleNode::new(pallas::Base::from(0u64))], subscription_id, sub_pub, 1, 5000, pallas::Base::from(5u64), 200, pallas::Base::from(6u64), 100, pallas::Base::from(7u64), pallas::Base::from(8u64), pallas::Base::from(9u64), pallas::Base::from(10u64), pallas::Base::from(11u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("VerifyAccessV1", true, Box::new(move || {
                let r = h.verify_access(sub_secret, pallas::Base::from(1u64), 1, 0, vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), subscription_id, 100, sub_pub.x().unwrap(), sub_pub.y().unwrap(), 1, 200, 10, 3600, 5, 100, 5, pallas::Base::from(6u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("UpdateUsageV1", true, Box::new(move || {
                let r = h.update_usage(subscription_id, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), sub_secret, 100, pallas::Base::from(99u64), vec![pallas::Base::from(0u64)]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
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
