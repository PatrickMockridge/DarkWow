//! ContractTestSpec for promissory_note contract. Spec: heavyweight-spec.md §5.7.

use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness};
use dwow_sdk::crypto::{PROMISSORY_NOTE_CONTRACT_ID, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::pasta::pallas;

use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn promissory_note_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(PromissoryNoteHarness::spawn()));
    let state_trees = harness.state_trees();
    let h: &PromissoryNoteHarness = harness;

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

    ContractTestSpec {
        name: "promissory_note",
        is_genesis: true,
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        harness: h,
        wasm_bytes: None,
        has_initialize: false,
        initialize: None,
        needs_coinbase_coordination: false,
        state_trees,
        endpoints: vec![
            EndpointSpec {
                name: "RegisterTypeV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                state_tree: "token_registry",
                state_key_fn: Box::new(move || token_id_key.clone()),
                generate: Box::new(move || {
                    let r = h.register_type(auth_parent, user_data, blind, recipient,
                        1000, spend_hook, user_data, coin_blind)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { call_data: r.call_data, proofs: r.token_proofs })
                }),
            },
        ],
    }
}
