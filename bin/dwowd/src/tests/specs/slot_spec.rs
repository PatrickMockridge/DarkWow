//! ContractTestSpec for slot. Endpoints use real ZK proofs from harness.
use dwow_contract_test_harness::harness::{SlotHarness, ContractHarness};
use dwow_sdk::crypto::PublicKey;
use dwow_sdk::crypto::SecretKey;
use dwow_sdk::pasta::{group::Group, pallas};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn slot_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SlotHarness::spawn()));
    let h: &SlotHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/slot/dwow_slot_contract.wasm");
    let sk = SecretKey::from_bytes([3u8; 32]).unwrap();
    let player_pub = PublicKey::from_secret(sk);
    ContractTestSpec {
        name: "slot", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        endpoints: vec![
            // initialize — non-ZK, no proof
            mk_ep("initialize", false, Box::new(move || {
                let r = h.initialize().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // commit_spin — ZK proof
            EndpointSpec {
                name: "commit_spin", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let pk = player_pub;
                    move || {
                        let r = h.commit_spin(pk, 100, 5, pallas::Base::from(42u64),
                            pallas::Base::from(7u64), 3, 10,
                            pallas::Base::from(1u64), pallas::Point::identity())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // reveal_spin — ZK proof
            EndpointSpec {
                name: "reveal_spin", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.reveal_spin(pallas::Base::from(42u64), pallas::Base::from(7u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // settle_bet — ZK proof (harness method is settle_bet, function code 0x03 = settle_spin)
            EndpointSpec {
                name: "settle_spin", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let pk = player_pub;
                    move || {
                        let r = h.settle_bet(pk, 100, 5, pallas::Base::from(42u64),
                            pallas::Base::from(7u64), pallas::Base::from(1u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
