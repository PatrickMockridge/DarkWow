//! ContractTestSpec for relayer_endowment. Spec: heavyweight-spec.md §5.9.
//! Harness: PARTIAL (3/8, real proofs). Tier: UNDERPOWERED.

use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness, RelayerEndowmentHarness};
use dwow_sdk::crypto::{
    poseidon_hash, pasta_prelude::PrimeField, MerkleNode, MerkleTree, PublicKey, SecretKey,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls;
use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointSpec, EndpointResult, EndpointExpectation,
};

pub fn relayer_endowment_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the proof binds to a commitment derived over the call set, and a call carries the
    // contract it addresses — so the harness must be given the deployed id rather than a
    // placeholder, exactly as the node's `get_metadata` arm derives over the same bytes.
    let relayer_endowment_cid =
        crate::tests::blockchain::derive_contract_id_from_name("relayer_endowment");
    let harness = Box::leak(Box::new(RelayerEndowmentHarness::spawn(relayer_endowment_cid)));
    let h: &RelayerEndowmentHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/relayer_endowment/dwow_relayer_endowment_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_bytes([1u8; 32]).unwrap());
    let r_pk = PublicKey::from_secret(SecretKey::from_bytes([2u8; 32]).unwrap());

    // The note `DeployCapitalV1`'s child spends. `deploy_capital_v1` requires exactly one
    // `promissory_note::transfer_v1` child, so without an issued note the row cannot pass at all.
    let issue_secret = pallas::Base::from(46u64);
    let notes: Arc<Mutex<Option<Vec<child_calls::PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "relayer_endowment", is_genesis: false,
        contract_id: relayer_endowment_cid,
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        // Wiring the child's provenance, ported from `otc_swap_spec.rs` — register a type, issue one
        // note, and mirror the tree the contract built so the witness path is the real one.
        setup: Some(Box::new({
            let notes = notes.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, 1000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let asset_id = token0.asset_id;

                let n1 = pn
                    .issue(issue_secret, asset_id, owner_addr, 1000, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(7u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n1.call_data, n1.proofs.clone())?.submit())?;

                // The contract appends the empty leaf at initialize, then the type, then the note —
                // so the mirror has to make the same three appends for the witness path to be the
                // one the contract will recompute the root from.
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                tree.append(MerkleNode::from_base(n1.commitment.inner()));
                let mark = tree.mark().unwrap();
                let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("witness");
                *notes.lock().unwrap_or_else(|e| e.into_inner()) = Some(vec![(
                    n1.commitment.inner(),
                    u64::from(mark),
                    path,
                    asset_id,
                    pallas::Base::from(7u64),
                )]);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "InitializeV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    // No height argument: the harness takes the verifying block height from the
                    // runner (`ContractHarness::set_next_block_height`), because it is the one
                    // input to `InitializeV2` that a caller cannot choose.
                    let r = h.initialize(pk, 1000u32)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "DeployCapitalV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        let n = notes.lock().unwrap_or_else(|e| e.into_inner());
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;

                        // `OBL-C198`: **the child's call data is built first and is not proved**, then
                        // the parent's is built, and only then is one commitment taken over the
                        // ordered set (child first, parent last — DFS post-order). Each proof binds
                        // to that single value, which is what makes the parent's `Σ` check and the
                        // child's transfer describe the same transaction.
                        //
                        // This is the two-phase form rather than the `children` convenience because
                        // the child is proven against a commitment that includes the **parent's**
                        // bytes, which do not exist until the parent's data does — the shape the
                        // nested-child finding describes, and the reason `deploy_capital_solo` is
                        // wrong here even though it compiles.
                        let blind_seed = poseidon_hash([pallas::Base::from(1u64), pallas::Base::from(1u64)]);
                        let (child_call, child_plan, child_nonce) = child_calls::pn_transfer_prepare(
                            &n[0], 1000u64, blind_seed, pallas::Base::zero(),
                        )?;

                        let plan = h.deploy_capital_prepare(
                            pk, 1000, pallas::Base::from(1u64),
                            pallas::Scalar::from(100u64), r_pk, 1000u32,
                        ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;

                        let parent_call = dwow_sdk::tx::ContractCall {
                            contract_id: relayer_endowment_cid,
                            data: plan.call_data.clone(),
                        };
                        let commitment = dwow_sdk::crypto::util::tx_commitment([&child_call, &parent_call]);

                        let child_debris = child_plan.prove(commitment, child_nonce)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let r = h.deploy_capital_prove(plan, commitment, pallas::Base::zero())
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;

                        let child = ChildCall {
                            contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
                            call_data: child_call.data,
                            proofs: child_debris.proofs,
                            children: vec![],
                        };
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                name: "ClaimFeesV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new(move || {
                    let r = h.claim_fees(pallas::Base::from(1u64), pk, 100)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
