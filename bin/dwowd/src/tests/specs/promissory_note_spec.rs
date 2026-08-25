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
    // recipient is the field-element owner address H(7, owner_secret); the spend endpoints use
    // secret = auth_parent, so recipient MUST equal H(7, auth_parent) for the commitment to be spendable.
    let recipient = poseidon_hash([pallas::Base::from(7u64), auth_parent]);
    // spend_hook = 0: no cross-contract burn callback for these self-contained endpoint tests.
    let spend_hook = pallas::Base::zero();
    let commitment_blind = pallas::Base::from(6u64);

    // Pre-compute asset_id = poseidon_hash(2, token_auth_parent, token_user_data, token_blind)
    // where token_auth_parent = poseidon_hash(7, issue_secret) — matching register_type.
    let token_auth_parent = poseidon_hash([pallas::Base::from(7), auth_parent]);
    let asset_id = poseidon_hash([
        pallas::Base::from(2), token_auth_parent, user_data, blind,
    ]);
    let asset_id_key = asset_id.to_repr().to_vec();
    let tkk = asset_id_key.clone();

    // Pre-compute the minted commitments. The commitment tree is append-only: [ZERO @ 0, commitment @ 1, commitment @ 2, ...].
    // Each mint (register/issue/transfer output/otc output/redeem receipt) appends a leaf; the
    // burn side marks the nullifier spent but does not remove the leaf.
    let register_result = h.register_type(auth_parent, user_data, blind, recipient,
        1000, spend_hook, user_data, commitment_blind)
        .expect("pre-compute register_type");
    let issue_result = h.issue(auth_parent, asset_id, recipient,
        500, spend_hook, user_data, commitment_blind)
        .expect("pre-compute issue");

    // CapCommitment = H(4, public_key, value, asset_id, spend_hook, user_data, blind).
    let cap = |public_key: pallas::Base, value: u64, commitment_blind: pallas::Base| {
        poseidon_hash([
            pallas::Base::from(4u64), public_key, pallas::Base::from(value),
            asset_id, spend_hook, user_data, commitment_blind,
        ])
    };

    let coin_a = register_result.commitment.inner();   // register commitment @ pos 1
    let coin_b = issue_result.commitment.inner();      // issue commitment @ pos 2
    let coin_c = cap(recipient, 500, pallas::Base::from(7u64));      // transfer output @ pos 3
    let coin_a_prime = cap(recipient, 1000, pallas::Base::from(8u64)); // otc output 0 @ pos 4 (Alice's A)
    let coin_b_prime = cap(recipient, 500, pallas::Base::from(9u64));  // otc output 1 @ pos 5 (Bob's C)

    // Build prefix commitment trees — the Merkle witness for a commitment depends on how many leaves were
    // on-chain when it was SPENT (the tree is append-only: guard@0, then commitments in mint order).
    // Each spend endpoint witnesses against the tree prefix that existed at that point.

    // TransferV1 spends B (pos 2) in [guard, A, B]:
    let mut tree_3 = MerkleTree::new(1);
    tree_3.append(MerkleNode::from_base(pallas::Base::zero()));
    tree_3.append(MerkleNode::from_base(coin_a));
    tree_3.append(MerkleNode::from_base(coin_b));
    let mark_b = tree_3.mark().expect("tree.mark b");
    let path_b: Vec<MerkleNode> = tree_3.witness(mark_b, 0).expect("witness b");
    let pos_b = u64::from(mark_b);

    // OtcSwapV1 spends A (pos 1) and C (pos 3) in [guard, A, B, C]:
    let mut tree_4 = MerkleTree::new(1);
    tree_4.append(MerkleNode::from_base(pallas::Base::zero()));
    tree_4.append(MerkleNode::from_base(coin_a));
    let mark_a = tree_4.mark().expect("tree.mark a");
    tree_4.append(MerkleNode::from_base(coin_b));
    tree_4.append(MerkleNode::from_base(coin_c));
    let mark_c = tree_4.mark().expect("tree.mark c");
    let path_a: Vec<MerkleNode> = tree_4.witness(mark_a, 0).expect("witness a");
    let path_c: Vec<MerkleNode> = tree_4.witness(mark_c, 0).expect("witness c");
    let pos_a = u64::from(mark_a);
    let pos_c = u64::from(mark_c);

    // RevokeV1 spends A' (pos 4) and RedeemV1 spends B' (pos 5) in [guard, A, B, C, A', B']:
    let mut tree_6 = MerkleTree::new(1);
    tree_6.append(MerkleNode::from_base(pallas::Base::zero()));
    tree_6.append(MerkleNode::from_base(coin_a));
    tree_6.append(MerkleNode::from_base(coin_b));
    tree_6.append(MerkleNode::from_base(coin_c));
    tree_6.append(MerkleNode::from_base(coin_a_prime));
    let mark_a_prime = tree_6.mark().expect("tree.mark a'");
    tree_6.append(MerkleNode::from_base(coin_b_prime));
    let mark_b_prime = tree_6.mark().expect("tree.mark b'");
    let path_a_prime: Vec<MerkleNode> = tree_6.witness(mark_a_prime, 0).expect("witness a'");
    let path_b_prime: Vec<MerkleNode> = tree_6.witness(mark_b_prime, 0).expect("witness b'");
    let pos_a_prime = u64::from(mark_a_prime);
    let pos_b_prime = u64::from(mark_b_prime);

    ContractTestSpec {
        name: "promissory_note",
        is_genesis: true,
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "RegisterTypeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let k = tkk.clone(); let c = *PROMISSORY_NOTE_CONTRACT_ID; move |chain: &HeavyweightPipeline| { let r = chain.query_contract_state(c, "token_registry", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("token must be stored".into())); } Ok(()) } })),
                generate: Box::new(move || {
                    let r = h.register_type(auth_parent, user_data, blind, recipient,
                        1000, spend_hook, user_data, commitment_blind)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.token_proofs })
                }),
            },
            EndpointSpec {
                name: "IssueV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({ let c = *PROMISSORY_NOTE_CONTRACT_ID; let issue_commitment = issue_commitment.clone(); move |chain: &HeavyweightPipeline| { let k = issue_commitment.lock().unwrap().clone().ok_or_else(|| dwow_core::Error::Custom("IssueV1 commitment not captured".into()))?; let r = chain.query_contract_state(c, "commitment_set", &k)?; if r.is_none() { return Err(dwow_core::Error::Custom("minted commitment must exist".into())); } Ok(()) } })),
                generate: Box::new({
                    let issue_commitment = issue_commitment.clone();
                    move || {
                        let r = h.issue(auth_parent, asset_id, recipient,
                            500, spend_hook, user_data, commitment_blind)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *issue_commitment.lock().unwrap() = Some(r.commitment.to_bytes().to_vec());
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
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
                    let path_b = path_b.clone();
                    move || {
                        use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_base(recipient));
                        let input = TransferCallInput {
                            value: 500, asset_id, spend_hook, user_data, commitment_blind,
                            leaf_position: pos_b, merkle_path: path_b.clone(),
                            secret: auth_parent,
                            ephemeral_signature_secret: pallas::Base::from(9u64),
                            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                        };
                        let output = TransferCallOutput {
                            recipient, recipient_pub, value: 500, asset_id, spend_hook, user_data,
                            commitment_blind: pallas::Base::from(7u64),
                        };
                        let r = h.transfer(vec![input], vec![output])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *transfer_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
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
                    let path_a = path_a.clone();
                    let path_c = path_c.clone();
                    move || {
                        use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
                        let recipient_pub = PublicKey::from_secret(SecretKey::from_base(recipient));
                        // Alice burns commitment A (asset_id) -> Bob receives output[1]; Bob burns commitment C
                        // (asset_id) -> Alice receives output[0]. OtcSwapV1 requires 2 in / 2 out.
                        let alice_input = TransferCallInput {
                            value: 1000, asset_id, spend_hook, user_data, commitment_blind,
                            leaf_position: pos_a, merkle_path: path_a.clone(),
                            secret: auth_parent,
                            ephemeral_signature_secret: pallas::Base::from(9u64),
                            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                        };
                        let bob_input = TransferCallInput {
                            value: 500, asset_id, spend_hook, user_data,
                            commitment_blind: pallas::Base::from(7u64),
                            leaf_position: pos_c, merkle_path: path_c.clone(),
                            secret: auth_parent,
                            ephemeral_signature_secret: pallas::Base::from(10u64),
                            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                        };
                        let bob_output = TransferCallOutput {
                            recipient, recipient_pub: recipient_pub.clone(), value: 1000, asset_id,
                            spend_hook, user_data, commitment_blind: pallas::Base::from(8u64),
                        };
                        let alice_output = TransferCallOutput {
                            recipient, recipient_pub, value: 500, asset_id,
                            spend_hook, user_data, commitment_blind: pallas::Base::from(9u64),
                        };
                        let r = h.otc_swap(vec![alice_input, bob_input], vec![bob_output, alice_output])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *otcswap_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
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
                    let path_a_prime = path_a_prime.clone();
                    move || {
                        let r = h.revoke(1000, asset_id, spend_hook, user_data,
                            pallas::Base::from(8u64), auth_parent, pos_a_prime, path_a_prime.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *revoke_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
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
                    let path_b_prime = path_b_prime.clone();
                    move || {
                        let r = h.redeem(500, asset_id, spend_hook, user_data,
                            pallas::Base::from(9u64), auth_parent, recipient, pos_b_prime, path_b_prime.clone())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        *redeem_nf.lock().unwrap() = Some(r.nullifier.to_bytes().to_vec());
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
                    }
                }),
            },
        ],
    }
}
