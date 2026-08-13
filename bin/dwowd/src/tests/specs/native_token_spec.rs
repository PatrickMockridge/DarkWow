//! ContractTestSpec for native_token. Spec: heavyweight-spec.md §5.1.

use dwow_contract_test_harness::harness::{ContractHarness, NativeTokenHarness};
use dwow_sdk::crypto::{NATIVE_TOKEN_CONTRACT_ID, Keypair, PublicKey, SecretKey};
use dwow_sdk::pasta::{group::{Group, GroupEncoding}, pallas};
use std::sync::{Arc, Mutex};

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::modules;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn native_token_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(NativeTokenHarness::spawn()));
    let h: &NativeTokenHarness = harness;
    let secret = SecretKey::from_bytes([2u8; 32]).unwrap();

    // Shared state: nullifiers captured during generate, read during verify_state.
    let burn_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let transfer_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let spend_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));

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
                name: "FeeV2", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: Some(Box::new({
                    let sk = secret.clone();
                    let ephem = SecretKey::from_bytes([9u8; 32]).unwrap();
                    move |coinbase| {
                        // Build merkle tree from production module to derive path + root.
                        // FeeV2 receives merkle_root from the tree — never recomputed manually.
                        use dwow_sdk::crypto::{MerkleNode, MerkleTree};
                        let mut tree = MerkleTree::new(1);
                        tree.append(MerkleNode::from_base(pallas::Base::zero()));  // zero guard (pos 0)
                        tree.append(MerkleNode::from_base(coinbase.coin_commitment.inner()));
                        let coin_pos = tree.mark().expect("tree.mark");
                        let path: Vec<MerkleNode> = tree.witness(coin_pos, 0).expect("tree.witness");
                        let root = tree.root(0).expect("tree.root");

                        let r = h.fee_v2(
                            coinbase.coin_value, pallas::Base::zero(),
                            pallas::Base::from(0u64), pallas::Base::from(0u64),
                            coinbase.coin_blind,
                            u64::from(coin_pos),
                            path,
                            root,
                            sk.clone(),
                            ephem.clone(),
                            PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap()),
                            pallas::Base::from(0u64), pallas::Base::from(0u64),
                            1,  // fee_amount
                            1,  // threshold (premium)
                        ).map_err(modules::error_bridge::bridge)?;
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                })),
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; move |chain: &HeavyweightPipeline| {
                    let acc_key: &[u8] = b"fee_commit_acc";
                    let data = chain.query_contract_state(c, "info", acc_key)?
                        .ok_or_else(|| dwow_core::Error::Custom(
                            "TEST-FAIL [native_token::FeeV2]: fee_commit_accumulator not found after fee block".into()
                        ))?;
                    // Verify accumulator was reset to Identity by FeeCollectV1.
                    // If FeeV2 accumulation is broken, the accumulator would already
                    // be Identity before FeeCollectV1, making this a weaker check.
                    // The standalone test_heavyweight_fee_v2 does a three-point check.
                    let acc_point: dwow_sdk::pasta::pallas::Point =
                        Option::from(dwow_sdk::pasta::pallas::Point::from_bytes(
                            &data[..32].try_into().map_err(|_|
                                dwow_core::Error::Custom("accumulator wrong size".into())
                            )?
                        )).ok_or_else(|| dwow_core::Error::Custom(
                            "TEST-FAIL [native_token::FeeV2]: invalid accumulator point".into()
                        ))?;
                    if acc_point != dwow_sdk::pasta::pallas::Point::identity() {
                        return Err(dwow_core::Error::Custom(format!(
                            "TEST-FAIL [native_token::FeeV2]: accumulator not reset (expected Identity)"
                        )));
                    }
                    Ok(())
                } })),
                generate: Box::new(|| Err(dwow_core::Error::Custom("TEST-FAIL [native_token]: FeeV2 must use generate_with_coinbase path".into()))),
            },
            EndpointSpec {
                name: "FeeCollectV1", is_zk: true,
                expectation: EndpointExpectation::Rejection, // exercised structurally by with_fee_collect()
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; move |chain: &HeavyweightPipeline| {
                    let acc_key: &[u8] = b"fee_commit_acc";
                    match chain.query_contract_state(c, "info", acc_key)? {
                        Some(data) => {
                            // After FeeCollectV1, accumulator must be Identity (reset).
                            // 32 zero bytes = pallas::Point::identity() compressed.
                            if data.len() == 32 && data.iter().all(|b| *b == 0) {
                                Ok(())
                            } else {
                                Err(dwow_core::Error::Custom(
                                    "WARN [FeeCollectV1]: accumulator not reset to Identity".into()
                                ))
                            }
                        }
                        None => Err(dwow_core::Error::Custom(
                            "WARN [FeeCollectV1]: fee_commit_accumulator not found".into()
                        )),
                    }
                } })),
                generate: Box::new(|| Err(dwow_core::Error::Custom(
                    "TEST-FAIL [native_token]: FeeCollectV1 exercised structurally by with_fee_collect()".into()
                ))),
            },
            EndpointSpec {
                name: "BurnV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: Some(Box::new({
                    let sk = secret.clone();
                    let ephem = SecretKey::from_bytes([9u8; 32]).unwrap();
                    let burn_nf = burn_nf.clone();
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
                        *burn_nf.lock().unwrap() = Some(r.inputs[0].nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                })),
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; let burn_nf = burn_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = burn_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("BurnV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist after burn".into())); } Ok(()) } })),
                generate: Box::new(|| Err(dwow_core::Error::Custom("TEST-FAIL [native_token]: BurnV1 must use generate_with_coinbase path".into()))),
            },
            EndpointSpec {
                name: "TransferV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; let transfer_nf = transfer_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = transfer_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("TransferV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist after transfer".into())); } Ok(()) } })),
                generate: Box::new({
                    let sk = secret.clone();
                    let transfer_nf = transfer_nf.clone();
                    move || {
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap());
                        let r = h.transfer(500, pallas::Base::from(1u64), sk.clone(),
                            pallas::Base::from(6u64), recipient_pub)
                            .map_err(modules::error_bridge::bridge)?;
                        *transfer_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            EndpointSpec {
                name: "SpendV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *NATIVE_TOKEN_CONTRACT_ID; let spend_nf = spend_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = spend_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("SpendV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist after spend".into())); } Ok(()) } })),
                generate: Box::new({
                    let sk = secret.clone();
                    let spend_nf = spend_nf.clone();
                    move || {
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_bytes([5u8; 32]).unwrap());
                        let r = h.spend(500, pallas::Base::from(1u64), sk.clone(),
                            pallas::Base::from(6u64), recipient_pub)
                            .map_err(modules::error_bridge::bridge)?;
                        *spend_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
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
