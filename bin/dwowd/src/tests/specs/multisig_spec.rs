//! ContractTestSpec for multisig contract.
//!
//! Category: O-Cap Authorization (genesis).
//! Functions: 4 (InitializeV1=0x00 non-ZK, CreateGroupV1=0x01 ZK,
//!                    SignV1=0x02 ZK, FinalizeV1=0x03 ZK).
//! Spec: heavyweight-spec.md §5.6.

use dwow_contract_test_harness::harness::{ContractHarness, MultiSigHarness};
use dwow_sdk::crypto::{MULTISIG_CONTRACT_ID, PublicKey, SecretKey, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn multisig_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(MultiSigHarness::spawn()));
    let state_trees = harness.state_trees();
    // Shared reference for closures — &T: Copy so each closure gets its own
    let h: &MultiSigHarness = harness;

    // Deterministic values matching harness seeds (signer_secret=3u64)
    let signer_secret = pallas::Base::from(3u64);
    let signer_pub = PublicKey::from_secret(SecretKey::from_base(signer_secret));
    let (fx, fy) = signer_pub.xy().expect("pk not identity");
    let group_id = poseidon_hash([fx, fy, pallas::Base::from(1u64), pallas::Base::from(1u64)]);
    let gid_bytes = group_id.to_repr().to_vec();
    let message_hash = pallas::Base::from(2u64);

    ContractTestSpec {
        name: "multisig",
        is_genesis: true,
        contract_id: *MULTISIG_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        state_trees,
        endpoints: vec![
            EndpointSpec {
                name: "CreateGroupV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                    generate_with_coinbase: None,
                state_tree: "groups",
                state_key_fn: { let b = gid_bytes.clone(); Box::new(move || b.clone()) },
                generate: Box::new(move || {
                    let r = h.create_group(1, vec![signer_pub])?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "SignV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                    generate_with_coinbase: None,
                state_tree: "signatures",
                state_key_fn: { let b = gid_bytes.clone(); Box::new(move || b.clone()) },
                generate: Box::new(move || {
                    let r = h.sign(group_id, message_hash, signer_secret)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "FinalizeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                    generate_with_coinbase: None,
                state_tree: "nullifiers",
                state_key_fn: { let b = gid_bytes; Box::new(move || b.clone()) },
                generate: Box::new(move || {
                    let r = h.finalize(group_id, message_hash)?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
