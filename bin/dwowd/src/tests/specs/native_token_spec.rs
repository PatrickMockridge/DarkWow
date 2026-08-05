//! ContractTestSpec for native_token. Spec: heavyweight-spec.md §5.1.

use dwow_contract_test_harness::harness::{ContractHarness, NativeTokenHarness};
use dwow_sdk::crypto::{NATIVE_TOKEN_CONTRACT_ID, Keypair, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::modules;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn native_token_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(NativeTokenHarness::spawn()));
    let h: &NativeTokenHarness = harness;
    let secret = SecretKey::from_bytes([2u8; 32]).unwrap();

    ContractTestSpec {
        name: "native_token",
        is_genesis: true,
        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: true,
        endpoints: vec![
            EndpointSpec {
                name: "FeeV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: Some(Box::new({
                    let sk = secret.clone();
                    let ephem = SecretKey::from_bytes([9u8; 32]).unwrap();
                    move |coinbase| {
                        let r = h.fee(
                            coinbase.coin_value, pallas::Base::zero(),
                            pallas::Base::from(0u64), pallas::Base::from(0u64),
                            coinbase.coin_blind, 0,
                            vec![dwow_sdk::crypto::MerkleNode::new(pallas::Base::from(0u64)); 32],
                            sk.clone(), sk.clone(),
                            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap()),
                            pallas::Base::from(0u64), pallas::Base::from(0u64), 10,
                        ).map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                })),
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "nullifiers", &[])?; assert!(r.is_some()); Ok(()) } })),
                generate: Box::new(|| unreachable!("FeeV1 uses generate_with_coinbase")),
            },
            EndpointSpec {
                name: "BurnV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: Some(Box::new({
                    let sk = secret.clone();
                    let ephem = SecretKey::from_bytes([9u8; 32]).unwrap();
                    move |coinbase| {
                        let input = dwow_native_token_contract::client::burn::BurnCallInput {
                            value: coinbase.coin_value / 2,
                            token_id: pallas::Base::from(1u64),
                            spend_hook: pallas::Base::from(0u64),
                            user_data: pallas::Base::from(0u64),
                            coin_blind: coinbase.coin_blind,
                            leaf_position: 0u64,
                            merkle_path: vec![dwow_sdk::crypto::MerkleNode::new(pallas::Base::from(0u64)); 32],
                            secret: sk.clone(),
                            ephemeral_signature_secret: ephem.clone(),
                            tx_commitment: pallas::Base::zero(),
                            tx_nonce: pallas::Base::zero(),
                        };
                        let r = h.burn(vec![input])
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                })),
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "nullifiers", &[])?; assert!(r.is_some()); Ok(()) } })),
                generate: Box::new(|| unreachable!("BurnV1 uses generate_with_coinbase")),
            },
            EndpointSpec {
                name: "TransferV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "nullifiers", &[])?; assert!(r.is_some()); Ok(()) } })),
                generate: Box::new({
                    let sk = secret.clone();
                    move || {
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap());
                        let r = h.transfer(500, pallas::Base::from(1u64), sk.clone(),
                            pallas::Base::from(6u64), recipient_pub)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            EndpointSpec {
                name: "SpendV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "nullifiers", &[])?; assert!(r.is_some()); Ok(()) } })),
                generate: Box::new({
                    let sk = secret.clone();
                    move || {
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap());
                        let r = h.spend(500, pallas::Base::from(1u64), sk.clone(),
                            pallas::Base::from(6u64), recipient_pub)
                            .map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            // MintV1 (0x01) — walled off, returns FunctionDisabled
            EndpointSpec {
                name: "MintV1", is_zk: false,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(|| {
                    Ok(EndpointResult { call_data: vec![0x01], proofs: vec![] })
                }),
            },
        ],
    }
}
