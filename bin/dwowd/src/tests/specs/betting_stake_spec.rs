//! ContractTestSpec for betting_stake. Spec: heavyweight-spec.md §5.9.
//! Harness: COMPLETE (5/5, real proofs). Tier: READY.

use dwow_contract_test_harness::harness::{BettingStakeHarness, ClaimStakeInfo, ContractHarness, UnstakeStakeInfo};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn betting_stake_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BettingStakeHarness::spawn()));
    let h: &BettingStakeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/betting_stake/dwow_betting_stake_contract.wasm");
    let sk = SecretKey::from_bytes([1u8; 32]).unwrap();
    let pk = PublicKey::from_secret(sk.clone());
    let table_id = pallas::Base::from(1u64);

    ContractTestSpec {
        name: "betting_stake", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "InitializeV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.initialize(table_id, 200, 1)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "StakeV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let s = SecretKey::from_bytes([1u8; 32]).unwrap();
                    let r = h.stake(table_id, pk, s, 1000,
                        pallas::Base::zero(), pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "UnstakeV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let s = SecretKey::from_bytes([1u8; 32]).unwrap();
                    let info = UnstakeStakeInfo { table_id, staker_pub: pk,
                        original_amount: 1000, current_amount: 1000,
                        accumulated_earnings: 0,
                        token_id: pallas::Base::from(1u64), nonce: 0u64 };
                    let r = h.unstake(pallas::Base::from(1u64), &info, s,
                        pallas::Base::zero(), pallas::Base::zero())?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ClaimEarningsV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let s = SecretKey::from_bytes([1u8; 32]).unwrap();
                    let info = ClaimStakeInfo { table_id, staker_pub: pk,
                        current_amount: 1000, accumulated_earnings: 100,
                        token_id: pallas::Base::from(1u64), nonce: 0u64 };
                    let r = h.claim_earnings(pallas::Base::from(1u64), &info, s)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "UpdateRiskV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.update_risk(table_id, pallas::Base::from(1u64),
                        5000, 100, 200, 1)?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
