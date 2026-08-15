//! ContractTestSpec for escrow. Spec: heavyweight-spec.md §5.9.
//! Tier: HARVESTABLE. Gap: fund_escrow requires &mut self (harness merkle tree mutation).
use dwow_contract_test_harness::harness::{ContractHarness, EscrowHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;

pub fn escrow_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(EscrowHarness::spawn()));
    let h: &EscrowHarness = &*harness;
    let wasm = include_bytes!("../../../../../src/contract/escrow/dwow_escrow_contract.wasm");
    let buyer_pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(10u64)));
    let seller_pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(20u64)));
    let buyer_sk_val = pallas::Base::from(10u64);
    let seller_sk_val = pallas::Base::from(20u64);
    let seed = [0u8; 32];

    ContractTestSpec {
        name: "escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        endpoints: vec![
            EndpointSpec { name: "CreateEscrowV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.create_escrow(buyer_sk_val, buyer_pk, seller_pk,
                        5000, pallas::Base::from(1u64), 1000, seed)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof.clone()] })
                }),
            },
            EndpointSpec { name: "ClaimV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.claim_escrow(pallas::Base::from(1u64), seller_sk_val,
                        seller_pk, pallas::Base::from(1u64), seller_pk)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof.clone()] })
                }),
            },
            EndpointSpec { name: "RefundV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let (bx, by) = buyer_pk.xy().expect("pk");
                    let r = h.refund_escrow(pallas::Base::from(1u64), 1000, 1001,
                        buyer_sk_val, buyer_pk, bx, by, buyer_pk)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof.clone()] })
                }),
            },
        ],
    }
}
