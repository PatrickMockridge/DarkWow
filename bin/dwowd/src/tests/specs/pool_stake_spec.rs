//! ContractTestSpec for pool_stake.
//!
//! Money flow: CreatePoolV1 seeds the registry (no child); JoinPoolV1 locks capital with a 1:1
//! promissory-note child; LeavePoolV1 pays the stake back out (child, and see the note below);
//! AllocateCoverageV1 / SlashCoverageV1 are registry arithmetic (no child).
//!
//! `JoinPoolV1` and `LeavePoolV1` both validate a `promissory_note::transfer_v1` (0x04) child
//! against the contract id configured at deploy — `validate_child_contract_id` +
//! `validate_child_value_commit` (`src/contract/promissory_note/src/validation.rs:46,81`). This
//! spec used to pass `children: vec![]` while the exec has required one child since the
//! validation landed, so `JoinPoolV1` failed with `InvalidChildrenIndexes` (Custom 18) the moment
//! the proof above it started verifying.
//!
//! The child is built by the **shared** `modules::child_calls::pn_transfer_child`, not a local
//! copy — six specs had their own before it was factored out. The template is `betting_stake_spec`
//! (setup issues the notes, the endpoint returns the child).

use dwow_contract_test_harness::harness::PoolStakeHarness;
use dwow_sdk::crypto::{
    poseidon_hash, MerkleNode, MerkleTree, PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::modules::child_calls::{pn_transfer_child, PnNote};
use crate::tests::uniform_runner::{
    ContractTestSpec, EndpointExpectation, EndpointResult, EndpointSpec,
};
use super::helpers::mk_ep;

pub fn pool_stake_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(PoolStakeHarness::spawn()));
    let h: &PoolStakeHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/pool_stake/dwow_pool_stake_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(10u64)));
    let mpk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(20u64)));

    // The pool is keyed by its **derived** id — `CreatePoolV1` computes
    // `poseidon_hash(4, creator_pub_x, creator_pub_y, pool_config_hash, nonce)` and stores under
    // that — so every later endpoint has to address the pool by the value the create call
    // returned, not by a literal. This spec passed `Base::from(1)`, which `join_pool` reported as
    // `PoolNotFound`.
    let created_pool: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    // The stake id is the proof-bound `derived_member_id` returned by JoinPoolV1, captured here so
    // LeavePoolV1 can address the stake the join created.
    let created_stake: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    // Likewise the allocation id: SlashCoverageV1 addresses an allocation, and the spec used to
    // pass the *pool* id there.
    let created_allocation: Arc<Mutex<Option<pallas::Base>>> = Arc::new(Mutex::new(None));
    // `POOL_STAKE_MIN_STAKE` is 1_000_000 (`pool_stake/src/lib.rs:147`) and `join_pool` rejects
    // anything below it. This spec joined with 10_000, a hundred times under the floor — which the
    // suite never surfaced because it died at the proof before reaching the check.
    let amount: u64 = 1_000_000;
    let issue_secret = pallas::Base::from(100u64);
    let notes: Arc<Mutex<Option<Vec<PnNote>>>> = Arc::new(Mutex::new(None));

    ContractTestSpec { name: "pool_stake", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let notes = notes.clone();
            move |chain| {
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let pn = dwow_contract_test_harness::harness::PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);

                // The note JoinPoolV1 spends. Its value is the join amount: the contract
                // reproduces the child's value commitment as
                // `pedersen_commitment_u64(amount, Blind(fp_mod_fv(poseidon_hash([amount, pool_id]))))`
                // and rejects any mismatch.
                let token0 = pn
                    .register_type(issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, amount, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let asset_id = token0.asset_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                let mark0 = tree.mark().unwrap();
                let path0: Vec<MerkleNode> = tree.witness(mark0, 0).expect("w0");
                let mut issued = vec![
                    (token0.commitment.inner(), u64::from(mark0), path0, asset_id, pallas::Base::from(6u64)),
                ];

                // note 1: the LeavePoolV1 payout. JoinPoolV1 consumes note 0, so leaving the pool
                // needs a note of its own — the spec previously issued one note and then called
                // LeavePoolV1 with `children: vec![]`.
                let n1 = pn
                    .issue(issue_secret, asset_id, owner_addr, amount, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n1.call_data, n1.proofs.clone())?.submit())?;
                tree.append(MerkleNode::from_base(n1.commitment.inner()));
                let mark1 = tree.mark().unwrap();
                let path1: Vec<MerkleNode> = tree.witness(mark1, 0).expect("w1");
                issued.push((n1.commitment.inner(), u64::from(mark1), path1, asset_id, pallas::Base::from(8u64)));

                *notes.lock().unwrap() = Some(issued);
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            mk_ep("CreatePoolV1", true, Box::new({
                let created_pool = created_pool.clone();
                move || {
                    let r = h.create_pool(pk, 200, 100).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    *created_pool.lock().unwrap() = Some(r.pool_id);
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            EndpointSpec {
                name: "JoinPoolV1",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let created_pool = created_pool.clone();
                    let created_stake = created_stake.clone();
                    move || {
                        let pool_id = created_pool.lock().unwrap()
                            .ok_or_else(|| dwow_core::Error::Custom("pool not created".into()))?;
                        let r = h.join_pool(pool_id, amount, [0u8; 32], mpk)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        // The stake id is the proof-bound `derived_member_id`, which the harness
                        // surfaces as `JoinPoolResult::stake_id`. The contract now stores the stake
                        // under that same value, so this is the id LeavePoolV1 must address — the
                        // old spec passed the *pool* id as the stake id and got `StakeNotFound`.
                        *created_stake.lock().unwrap() = Some(r.stake_id);
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        // The contract's own formula, not a constant: `join_pool` computes
                        // `value_blind = poseidon_hash([amount, pool_id])` and
                        // `pn_transfer_child` runs the seed through `Blind(fp_mod_fv(..))`.
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), pool_id]);
                        let child = pn_transfer_child(&n[0], amount, blind_seed, blind_seed, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // LeavePoolV1's first call starts the leave cooldown, and that now *succeeds*. The
            // write of `leave_requested_at` used to happen in exec on a path that then returned
            // `Err(StakeLocked)` — a write-in-exec on a failing path, register OBL-C73. The write
            // now travels through the update and apply performs it, so the caller learns the
            // cooldown began from a successful call. A child call is still required: exec validates
            // the promissory_note transfer before it reaches the cooldown branch. The payout's
            // value commitment is checked on the *second* call, after the cooldown elapses — which
            // is why note 1 exists.
            EndpointSpec {
                name: "LeavePoolV1",
                is_zk: false,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    let created_pool = created_pool.clone();
                    let created_stake = created_stake.clone();
                    move || {
                        let pool_id = created_pool.lock().unwrap()
                            .ok_or_else(|| dwow_core::Error::Custom("pool not created".into()))?;
                        let stake_id = created_stake.lock().unwrap()
                            .ok_or_else(|| dwow_core::Error::Custom("stake not created".into()))?;
                        let r = h.leave_pool(stake_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let n = notes.lock().unwrap();
                        let n = n.as_ref().ok_or_else(|| dwow_core::Error::Custom("notes not issued".into()))?;
                        let blind_seed = poseidon_hash([pallas::Base::from(amount), pool_id]);
                        // A distinct output commitment blind from the join's. Join and leave both
                        // transfer `amount` under the same contract-derived value blind, so reusing
                        // it here reproduces the join's output commitment and `transfer_v1` rejects
                        // it: "[transfer_v1] Error: Duplicate commitment in output 0" (Custom 14).
                        let output_blind = pallas::Base::from(9u64);
                        let child = pn_transfer_child(&n[1], amount, blind_seed, output_blind, pallas::Base::zero())?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
            mk_ep("AllocateCoverageV1", true, Box::new({
                let created_pool = created_pool.clone();
                let created_allocation = created_allocation.clone();
                move || {
                    let pool_id = created_pool.lock().unwrap()
                        .ok_or_else(|| dwow_core::Error::Custom("pool not created".into()))?;
                    let r = h.allocate_coverage(pool_id, mpk, 5000, pallas::Base::from(1u64), [0u8; 32], 1000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    // The proof-bound `derived_allocation_id`; the contract stores the allocation
                    // under this value, and SlashCoverageV1 must address it by the same one.
                    *created_allocation.lock().unwrap() = Some(r.allocation_id);
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("SlashCoverageV1", true, Box::new({
                let created_allocation = created_allocation.clone();
                move || {
                    let allocation_id = created_allocation.lock().unwrap()
                        .ok_or_else(|| dwow_core::Error::Custom("allocation not created".into()))?;
                    let r = h.slash_coverage(allocation_id, 2000, pk, mpk).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
        ],
    }
}
