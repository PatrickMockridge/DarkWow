//! ContractTestSpec for promissory_note contract. Spec: heavyweight-spec.md §5.7.

use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{MerkleNode, MerkleTree, PROMISSORY_NOTE_CONTRACT_ID, PublicKey, SecretKey, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn promissory_note_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(PromissoryNoteHarness::spawn()));
    let h: &PromissoryNoteHarness = harness;

    // Shared state: nullifiers/commitment captured during generate, read during verify_state.
    let issue_commitment: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let transfer_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let otcswap_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let revoke_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
    let redeem_nf: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));

    // Deterministic inputs matching old test values
    let auth_parent = pallas::Base::from(1u64);
    let user_data = pallas::Base::from(2u64);
    let blind = pallas::Base::from(3u64);
    let recipient = pallas::Base::from(4u64);
    let spend_hook = pallas::Base::from(5u64);
    let coin_blind = pallas::Base::from(6u64);

    // Pre-compute token_id = poseidon_hash(2, auth_parent, blind, coin_blind)
    // (matches RegisterTypeV1 circuit: DOMAIN_TOK_COMMIT = witness_base(2))
    let token_id = poseidon_hash([
        pallas::Base::from(2), auth_parent, blind, coin_blind,
    ]);
    let token_id_key = token_id.to_repr().to_vec();
    let tkk = token_id_key.clone();

    // Pre-compute the coin-tree Merkle witness for the IssueV1 coin (leaf 2).
    // Coin tree = [ZERO @ 0, register_commitment @ 1, issue_commitment @ 2].
    let register_result = h.register_type(auth_parent, user_data, blind, recipient,
        1000, spend_hook, user_data, coin_blind)
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    let issue_result = h.issue(auth_parent, token_id, recipient,
        500, spend_hook, user_data, coin_blind)
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    let mut coin_tree = MerkleTree::new(1);
    coin_tree.append(MerkleNode::from_base(pallas::Base::zero()));
    coin_tree.append(MerkleNode::from_base(register_result.commitment.inner()));
    coin_tree.append(MerkleNode::from_base(issue_result.commitment.inner()));
    let coin_mark = coin_tree.mark().expect("tree.mark");
    let coin_path: Vec<MerkleNode> = coin_tree.witness(coin_mark, 0).expect("tree.witness");
    let coin_leaf_pos = u64::from(coin_mark);

    ContractTestSpec {
        name: "promissory_note",
        is_genesis: true,
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        endpoints: vec![
            EndpointSpec {
                name: "RegisterTypeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = tkk.clone(); let c = *PROMISSORY_NOTE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "token_registry", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("token must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.register_type(auth_parent, user_data, blind, recipient,
                        1000, spend_hook, user_data, coin_blind)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: r.token_proofs })
                }),
            },
            EndpointSpec {
                name: "IssueV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *PROMISSORY_NOTE_CONTRACT_ID; let issue_commitment = issue_commitment.clone(); move |chain: &HeavyweightPipeline| { let k = issue_commitment.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("IssueV1 commitment not captured".into()))?; let r = chain.query_contract_state(c, "coins", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("minted coin must exist".into())); } Ok(()) } })),
                generate: Box::new({
                    let issue_commitment = issue_commitment.clone();
                    move || {
                        let r = h.issue(auth_parent, token_id, recipient,
                            500, spend_hook, user_data, coin_blind)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *issue_commitment.lock().unwrap() = Some(r.commitment.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            EndpointSpec {
                name: "TransferV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *PROMISSORY_NOTE_CONTRACT_ID; let transfer_nf = transfer_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = transfer_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("TransferV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist".into())); } Ok(()) } })),
                generate: Box::new({
                    let transfer_nf = transfer_nf.clone();
                    let coin_path = coin_path.clone();
                    move || {
                        use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_base(recipient));
                        let input = TransferCallInput {
                            value: 500, token_id, spend_hook, user_data, coin_blind,
                            leaf_position: coin_leaf_pos, merkle_path: coin_path.clone(),
                            secret: auth_parent,
                            ephemeral_signature_secret: pallas::Base::from(9u64),
                            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                        };
                        let output = TransferCallOutput {
                            recipient, recipient_pub, value: 500, token_id, spend_hook, user_data,
                            coin_blind: pallas::Base::from(7u64),
                        };
                        let r = h.transfer(vec![input], vec![output])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *transfer_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            EndpointSpec {
                name: "OtcSwapV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *PROMISSORY_NOTE_CONTRACT_ID; let otcswap_nf = otcswap_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = otcswap_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("OtcSwapV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist".into())); } Ok(()) } })),
                generate: Box::new({
                    let otcswap_nf = otcswap_nf.clone();
                    let coin_path = coin_path.clone();
                    move || {
                        use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_base(recipient));
                        let input = TransferCallInput {
                            value: 500, token_id, spend_hook, user_data, coin_blind,
                            leaf_position: coin_leaf_pos, merkle_path: coin_path.clone(),
                            secret: auth_parent,
                            ephemeral_signature_secret: pallas::Base::from(9u64),
                            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                        };
                        let output = TransferCallOutput {
                            recipient, recipient_pub, value: 500, token_id, spend_hook, user_data,
                            coin_blind: pallas::Base::from(7u64),
                        };
                        let r = h.otc_swap(vec![input], vec![output])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *otcswap_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            EndpointSpec {
                name: "RevokeV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *PROMISSORY_NOTE_CONTRACT_ID; let revoke_nf = revoke_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = revoke_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("RevokeV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist".into())); } Ok(()) } })),
                generate: Box::new({
                    let revoke_nf = revoke_nf.clone();
                    let coin_path = coin_path.clone();
                    move || {
                        let r = h.revoke(500, token_id, spend_hook, user_data,
                            coin_blind, auth_parent, coin_leaf_pos, coin_path.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *revoke_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
            EndpointSpec {
                name: "RedeemV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *PROMISSORY_NOTE_CONTRACT_ID; let redeem_nf = redeem_nf.clone(); move |chain: &HeavyweightPipeline| { let nf = redeem_nf.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("RedeemV1 nullifier not captured".into()))?; let r = chain.query_contract_state(c, "nullifiers", &nf)?; if r.is_none() { return Err(dwow_core::Error::Custom("nullifier must exist".into())); } Ok(()) } })),
                generate: Box::new({
                    let redeem_nf = redeem_nf.clone();
                    let coin_path = coin_path.clone();
                    move || {
                        let r = h.redeem(500, token_id, spend_hook, user_data,
                            coin_blind, auth_parent, recipient, coin_leaf_pos, coin_path.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *redeem_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
        ],
    }
}
