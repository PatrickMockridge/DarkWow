//! ContractTestSpec for relayer_endowment. Spec: heavyweight-spec.md §5.9.
//! Harness: PARTIAL (3/8, real proofs). Tier: UNDERPOWERED.

use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness, RelayerEndowmentHarness};
use dwow_relayer_endowment_contract::model::FeeAllocation;
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

/// The blind seed `deploy_capital_v1` **requires** its child to have used.
///
/// The parent reproduces the child's value commitment rather than trusting it —
/// `validate_child_value_commit(&child_call.data, params.amount, value_blind)` with
/// `value_blind = poseidon_hash([amount, relayer_base])`, where `relayer_base` is the first 32
/// bytes of `compute_relayer_key(relayer_pub)` read as a base field element. So the child's
/// `blind_seed` is not a free choice: any other value is `PromissoryNoteError::ValueMismatch`
/// (code 20) raised from inside the parent's exec, naming neither the child nor the blind.
///
/// Reproduced here rather than called: `compute_relayer_key` is `pub(crate)` inside the
/// (private) entrypoint module, so it is not reachable from a spec. This is the whole of it —
/// chunk the pubkey's bytes into four little-endian `u64`s, hash them, take the repr.
fn deploy_child_blind_seed(relayer_pub: &PublicKey, amount: u64) -> pallas::Base {
    let pubkey_bytes = relayer_pub.to_bytes();
    let mut chunks = [0u64; 4];
    for i in 0..4 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&pubkey_bytes[i * 8..(i + 1) * 8]);
        chunks[i] = u64::from_le_bytes(bytes);
    }
    let hash = poseidon_hash([
        pallas::Base::from(chunks[0]),
        pallas::Base::from(chunks[1]),
        pallas::Base::from(chunks[2]),
        pallas::Base::from(chunks[3]),
    ]);
    let relayer_base = pallas::Base::from_repr(hash.to_repr()).expect("poseidon output is canonical");
    // …and then the outer hash the parent actually uses. `compute_relayer_key` gives
    // `relayer_base`; the blind is `poseidon_hash([amount, relayer_base])`. Returning
    // `relayer_base` alone — which this helper did on its first run — is a different value, and the
    // failure is the same `ValueMismatch` the coupling was written down to prevent.
    poseidon_hash([pallas::Base::from(amount), relayer_base])
}

/// What the child transfers and the parent deploys.
///
/// `RELAYER_ENDOWMENT_MIN_DEPLOY` is `1_000_000` (`relayer_endowment/src/lib.rs:156`) and
/// `deploy_capital_v1` refuses anything below it with `InsufficientDeploy` (code 3), so the note
/// the child spends has to be worth at least that. `1000` was the second thing this deployment's
/// own floor refused, after the value commitment matched.
const DEPLOY_VALUE: u64 = 1_000_000;

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
    // The relayer is `pk`, the same key `InitializeV1` creates the endowment for — a second key
    // stood here and made the deployment answer `EndowmentNotFound`.

    // The note `DeployCapitalV1`'s child spends. `deploy_capital_v1` requires exactly one
    // `promissory_note::transfer_v1` child, so without an issued note the row cannot pass at all.
    // **100, and it is not a free choice.** `child_calls::pn_transfer_prepare` hardcodes the child
    // input's `secret` as `pallas::Base::from(100u64)`, and a `transfer_v1` input derives its
    // commitment — and therefore its nullifier and its merkle root — from that secret. A note
    // issued under any other key is a **different** note, so the contract answers
    // `[transfer_v1] Error: Merkle root not found for input 0` (code 13): the root the child folds
    // is for a commitment the tree does not hold. `otc_swap_spec.rs` uses 100 for the same reason.
    let issue_secret = pallas::Base::from(100u64);
    let notes: Arc<Mutex<Option<Vec<child_calls::PnNote>>>> = Arc::new(Mutex::new(None));
    // The deployment id `ClaimFeesV1` has to name. It is **derived** by the deploy and cannot be
    // recomputed by the spec — `derive_deployment_id(relayer_pub, signature_public, backer_cut_bp,
    // amount, nonce)` — so the row that creates it records it for the row that claims against it.
    // `pallas::Base::from(1u64)` stood here, which matches no deployment ever created:
    // `DeploymentNotFound` (code 2). The same shape `bearer_bond_spec.rs` uses for its bond.
    let deployment_id: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));

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
                    .issue(issue_secret, asset_id, owner_addr, DEPLOY_VALUE, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(7u64))
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
                    let deployment_id = deployment_id.clone();
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
                        // The blind the *parent* will reproduce — see `deploy_child_blind_seed`. A
                        // free choice here is refused `ValueMismatch` inside the parent's exec,
                        // which is where the last run died.
                        // **`pk`, not `r_pk`**: `deploy_capital_v1` looks the endowment up by the
                        // relayer's key, and the endowment that exists is the one `InitializeV1`
                        // created — for `pk`. Deploying to `r_pk` is `EndowmentNotFound` (code 1).
                        let blind_seed = deploy_child_blind_seed(&pk, DEPLOY_VALUE);
                        let (child_call, child_plan, child_nonce) = child_calls::pn_transfer_prepare(
                            &n[0], DEPLOY_VALUE, blind_seed, pallas::Base::zero(),
                        )?;

                        let plan = h.deploy_capital_prepare(
                            pk, DEPLOY_VALUE, pallas::Base::from(1u64),
                            pallas::Scalar::from(100u64), pk, 1000u32,
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
                        *deployment_id.lock().unwrap_or_else(|e| e.into_inner()) = Some(r.public_inputs.derived_deployment_id);
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            EndpointSpec {
                // **Before `ClaimFeesV1`, and it is the claim's own precondition.**
                // `deployment.accumulated_fees` is written only inside
                // `process_settle_fees_instruction`, so without this row the claim answers `NoFees`
                // (code 5) — permanently, and naming neither the settlement nor the deployment.
                // Plaintext: the manifest marks `settle_fees` so, and there is no `settle_fees.zk`.
                name: "SettleFeesV1", is_zk: false, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new({
                    let deployment_id = deployment_id.clone();
                    move || {
                        let id = deployment_id
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .expect("DeployCapitalV1 must run before SettleFeesV1 and record its id");
                        // The allocation is the deployment and the fee; `total_fees` must equal the
                        // sum of the allocations or the exec refuses it
                        // (`InvalidParams("allocation sum != total_fees")`).
                        let r = h.settle_fees(pk, 100, vec![FeeAllocation { deployment_id: id, fee_amount: 100 }])
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            EndpointSpec {
                name: "ClaimFeesV1", is_zk: true, expectation: EndpointExpectation::Success,
                generate_with_coinbase: None, verify_state: None,
                generate: Box::new({
                    let deployment_id = deployment_id.clone();
                    move || {
                        let id = deployment_id
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .expect("DeployCapitalV1 must run before ClaimFeesV1 and record its id");
                        let r = h.claim_fees(id, pk, 100)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
        ],
    }
}
