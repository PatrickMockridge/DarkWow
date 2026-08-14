//! ContractTestSpec for darktoshi_dice. Spec: heavyweight-spec.md §5.9.
//! Harness: COMPLETE (4/4 operational, real proofs). Tier: READY.

use dwow_contract_test_harness::harness::{ContractHarness, DarkToshiDiceHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn darktoshi_dice_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DarkToshiDiceHarness::spawn()));
    let h: &DarkToshiDiceHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/darktoshi_dice/dwow_darktoshi_dice_contract.wasm");

    let player_sk = SecretKey::from_bytes([1u8; 32]).unwrap();
    let player_pub = PublicKey::from_secret(player_sk.clone());

    ContractTestSpec {
        name: "darktoshi_dice",
        is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h,
        wasm_bytes: Some(wasm),
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "CommitBetV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.commit_bet(player_pub, 1000, 50,
                        pallas::Base::from(99u64), pallas::Base::from(3u64),
                        pallas::Base::from(1u64), 200u32)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "RevealRollV1", is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.reveal_roll(pallas::Base::from(1u64), pallas::Base::from(99u64))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "SettleBetV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.settle_bet(pallas::Base::from(1u64), pallas::Base::from(99u64),
                        pallas::Base::from(1u64), pallas::Base::from(2u64),
                        pallas::Base::from(1000u64), pallas::Base::from(50u64),
                        pallas::Base::from(1u64), pallas::Base::from(3u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "HouseCloseV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.house_close(pallas::Base::from(1u64), pallas::Base::from(10u64),
                        pallas::Base::from(3u64), pallas::Base::from(4u64),
                        pallas::Base::from(5u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
