//! ContractTestSpec for subscription. Tier: HARVESTABLE — 5 harness methods.
//! 5/5 endpoints active (2 empty_witnesses, 3 real ZK including 25-param subscribe).
//!
//! **`VerifyAccessV1`'s capability is derived from the record** (`OBL-C84`). It used to be the
//! constant `4`, which the circuit accepted — the capability was a witness the prover picked and
//! nothing exposed it, so the proof said only "some subscription exists that this caller can open".
//! The circuit now instances the capability and the host recomputes it from the stored record, so
//! the fixture's value has to be the derivation for the subscription the `SubscribeV1` before it
//! wrote: plan 1, expiry 200, this subscriber key, nonce 1 (see the two calls below — those are the
//! same values, and they have to stay the same values).
use dwow_contract_test_harness::harness::{SubscriptionHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey, MerkleNode};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// The nonce both verify_access endpoints bind (`OBL-C84`), so the capability is one value per call.
const ACCESS_NONCE: pallas::Base = pallas::Base::from_raw([1, 0, 0, 0]);

pub fn subscription_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SubscriptionHarness::spawn()));
    let h: &SubscriptionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/subscription/dwow_subscription_contract.wasm");
    let sub_secret = pallas::Base::from(10u64);
    let sub_pub = PublicKey::from_secret(SecretKey::from_base(sub_secret));
    let subscription_id = pallas::Base::from(1u64);
    // The record's own fields, as the `SubscribeV1` call below writes them: plan 1, expiry 200.
    let record_plan_id = 1u32;
    let record_lock_until_block = 200u64;
    ContractTestSpec {
        name: "subscription", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            mk_ep("SubscribeV1", true, Box::new(move || {
                let r = h.subscribe(sub_secret, pallas::Base::from(1u64), vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Scalar::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), 1000, pallas::Base::from(4u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64))], 0, vec![MerkleNode::new(pallas::Base::from(0u64))], subscription_id, sub_pub, 1, 5000, pallas::Base::from(5u64), 200, pallas::Base::from(6u64), 100, pallas::Base::from(7u64), pallas::Base::from(8u64), pallas::Base::from(9u64), pallas::Base::from(10u64), pallas::Base::from(11u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("VerifyAccessV1", true, Box::new(move || {
                #[expect(clippy::unwrap_used, reason = "PublicKey rejects identity, so x()/y() is always Some")]
                let (px, py) = (sub_pub.x().unwrap(), sub_pub.y().unwrap());
                let capability = SubscriptionHarness::access_capability(px, py, record_plan_id, subscription_id, record_lock_until_block, ACCESS_NONCE);
                let r = h.verify_access(sub_secret, ACCESS_NONCE, 1, 0, vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Base::from(2u64), pallas::Base::from(3u64), capability, subscription_id, 100, px, py, record_plan_id, record_lock_until_block, 10, 3600, 5, 100, 5, pallas::Base::from(6u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            // NEGATIVE — `OBL-C84`'s control, and it is the one the old fixture could not have: the
            // capability here is a **real** one, derived exactly as `verify_access.zk` derives it,
            // with every witness agreeing with every other — for plan 2 instead of the record's plan
            // 1. So the proof is satisfiable and verifies; what refuses it is the host comparing the
            // published capability against the capability the *record* derives. Remove that
            // comparison and this endpoint starts granting access.
            EndpointSpec {
                name: "verify_access_wrong_plan", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    #[expect(clippy::unwrap_used, reason = "PublicKey rejects identity, so x()/y() is always Some")]
                    let (px, py) = (sub_pub.x().unwrap(), sub_pub.y().unwrap());
                    let capability = SubscriptionHarness::access_capability(px, py, 2, subscription_id, record_lock_until_block, ACCESS_NONCE);
                    let r = h.verify_access(sub_secret, ACCESS_NONCE, 1, 0, vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Base::from(2u64), pallas::Base::from(3u64), capability, subscription_id, 100, px, py, 2, record_lock_until_block, 10, 3600, 5, 100, 5, pallas::Base::from(6u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            mk_ep("UpdateUsageV1", true, Box::new(move || {
                let r = h.update_usage(subscription_id, pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(3u64), pallas::Base::from(4u64), sub_secret, 100, pallas::Base::from(99u64), vec![pallas::Base::from(0u64)]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("CancelV1", true, Box::new(move || {
                let r = h.cancel(subscription_id, sub_secret, pallas::Base::from(99u64), 100, sub_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("RenewV1", true, Box::new(move || {
                let r = h.renew(subscription_id, sub_secret, 200, pallas::Base::from(99u64), pallas::Point::default(), vec![pallas::Base::from(0u64)]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
