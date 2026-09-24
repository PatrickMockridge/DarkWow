//! ContractTestSpec for subscription. Tier: HARVESTABLE — 5 harness methods.
//! 6 endpoints: four prove, two are **declared placeholders** (`CancelV1`, `RenewV1` — the
//! harness builds their proofs from `empty_witnesses` and no client exists for either circuit,
//! `OBL-C106`). `SubscribeV1`'s own proof fails L2 verification with the fixture's inputs
//! consistent — `OBL-C107`, which is the next thing to localise, not a fixture question.
//!
//! **The endpoints are a flow and the fixture registers what they act on** (`OBL-C104`). Before this
//! the spec had no setup at all and every arm that needed state or a child refused: the run died at
//! its first endpoint with `[subscribe_v1] Error: Expected 1 child call
//! (promissory_note::transfer_v1), got 0`. What the fixture now provides, in the order the endpoints
//! need it:
//!
//!   * the **plan** — `SubscribeV1` looks it up by `plan_id` and refuses if it is absent or inactive,
//!     and the only writer of the plans tree is `DaoControlV1`'s `UpdatePlan`, which is a
//!     **plaintext** function (no circuit, no metadata, no proof). It is therefore an endpoint
//!     rather than part of `setup`: it must be submitted to the contract under test, whose id the
//!     runner resolves at deploy time and the spec never learns.
//!   * the **payment note**, issued in `setup` against the genesis promissory_note contract, worth
//!     the plan's price and spendable by the shared child builder's fixed secret.
//!   * the **id**, which is `Subscription::derive_id(...)` over the same values the call passes
//!     (`OBL-C75`): the circuit instances that derivation and the host keys the record by it.
//!   * the **capability**, `Subscription::access_capability(...)` over the record's own fields
//!     (`OBL-C84`), so `VerifyAccessV1` proves the caller holds the subscription rather than that
//!     some subscription exists.
use dwow_contract_test_harness::harness::{
    ContractHarness, PromissoryNoteHarness, SubscriptionHarness,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Group, PrimeField}, pedersen_commitment_u64, poseidon_hash, util::fp_mod_fv, Blind,
        MerkleNode, MerkleTree, PublicKey, SecretKey, PROMISSORY_NOTE_CONTRACT_ID,
    },
    pasta::pallas,
};
use dwow_subscription_contract::model::{
    DaoControlParamsV1, Plan, Subscription, SubscriptionId,
};
use std::sync::{Arc, Mutex};
use crate::tests::modules::child_calls::{pn_transfer_child, PnNote};
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

/// The plan the fixture registers, and the price it charges — the amount the `subscribe` child pays.
const PLAN_ID: u32 = 1;
const PLAN_PRICE: u64 = 1000;
const PLAN_DURATION: u64 = 200;

/// The nonce both verify_access endpoints bind (`OBL-C84`), so the capability is one value per call.
const ACCESS_NONCE: pallas::Base = pallas::Base::from_raw([1, 0, 0, 0]);

/// The id the record is keyed by, for the values the `SubscribeV1` call below passes.
fn subscription_id(sub_pub: &PublicKey, asset_id: pallas::Base) -> SubscriptionId {
    Subscription::derive_id(sub_pub, PLAN_ID, PLAN_PRICE, asset_id, PLAN_DURATION, SUB_SECRET, ACCESS_NONCE)
}

/// The subscriber's secret. The payment note is issued to a *different* key (the shared child
/// builder's fixed one), which is the point: the payment and the subscription are independent.
const SUB_SECRET: pallas::Base = pallas::Base::from_raw([10, 0, 0, 0]);

/// What `setup` registered and the endpoints act on. Stashed because the proof inputs depend on
/// values only the chain knows: the payment token's asset id is chosen by `register_type`.
#[derive(Default)]
struct Fixture {
    /// The payment note, in the plan's asset.
    note: Option<PnNote>,
    /// The plan's asset id, which the id derivation includes.
    asset_id: Option<pallas::Base>,
    /// The second payment note — `renew` needs one too, and the first is spent by `subscribe`.
    note2: Option<PnNote>,
    /// The height the `SubscribeV1` block reached. The record's `lock_until_block` is
    /// `current_block + plan.duration_blocks` — computed by the host, from a block only the chain
    /// knows — and `verify_access`'s capability is derived from the record, so the fixture has to
    /// learn it after the fact. It does, from the endpoint's own `verify_state` hook.
    height: Option<u64>,
}

pub fn subscription_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(SubscriptionHarness::spawn()));
    let h: &SubscriptionHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/subscription/dwow_subscription_contract.wasm");
    let sub_secret = SUB_SECRET;
    let sub_pub = PublicKey::from_secret(SecretKey::from_base(sub_secret));
    let fixture: Arc<Mutex<Fixture>> = Arc::new(Mutex::new(Fixture::default()));
    ContractTestSpec {
        name: "subscription", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let fixture = fixture.clone();
            move |chain| {
                // The payment token: the plan is priced in it, so it has to exist first. The same
                // shape `drain_protection`'s spec uses for its children.
                let pn = PromissoryNoteHarness::spawn();
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let issue_secret = pallas::Base::from(100u64);
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), issue_secret]);
                let token = pn
                    .register_type(
                        issue_secret,
                        pallas::Base::from(2u64),
                        pallas::Base::from(3u64),
                        owner_addr,
                        PLAN_PRICE,
                        pallas::Base::zero(),
                        pallas::Base::zero(),
                        pallas::Base::from(6u64),
                    )
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(pn_cid, &pn, &token.call_data, token.token_proofs.clone())?.submit(),
                )?;
                let asset_id = token.asset_id;

                // The note the subscribe child spends: worth the plan's price, in the plan's asset.
                let n = pn
                    .issue(issue_secret, asset_id, owner_addr, PLAN_PRICE, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(12u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(pn_cid, &pn, &n.call_data, n.proofs.clone())?.submit(),
                )?;
                // The second note, for `renew`'s child: the first is spent by `subscribe`.
                let n2 = pn
                    .issue(issue_secret, asset_id, owner_addr, PLAN_PRICE, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(13u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(pn_cid, &pn, &n2.call_data, n2.proofs.clone())?.submit(),
                )?;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token.commitment.inner()));
                tree.append(MerkleNode::from_base(n.commitment.inner()));
                let mark = tree.mark().expect("tree.mark");
                let path: Vec<MerkleNode> = tree.witness(mark, 0).expect("tree.witness");
                tree.append(MerkleNode::from_base(n2.commitment.inner()));
                let mark2 = tree.mark().expect("tree.mark");
                let path2: Vec<MerkleNode> = tree.witness(mark2, 0).expect("tree.witness");

                let mut f = fixture.lock().unwrap();
                f.asset_id = Some(asset_id);
                f.note = Some((n.commitment.inner(), u64::from(mark), path, asset_id, pallas::Base::from(12u64)));
                f.note2 = Some((n2.commitment.inner(), u64::from(mark2), path2, asset_id, pallas::Base::from(13u64)));
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            // The plan, first, because `SubscribeV1` refuses without it. Plaintext: `is_zk: false`.
            mk_ep("DaoControlV1", false, Box::new({
                let fixture = fixture.clone();
                move || {
                let asset_id = fixture.lock().unwrap().asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                let plan = Plan {
                    version: 1,
                    id: PLAN_ID,
                    name_hash: pallas::Base::from(7u64),
                    price: PLAN_PRICE,
                    asset_id,
                    duration_blocks: PLAN_DURATION,
                    treasury_share: 0,
                    endowment_share: 0,
                    active: true,
                    dao_escrow_discount: 0,
                    required_dao_escrow: None,
                };
                let mut call_data = vec![0x05u8];
                call_data.extend_from_slice(&DaoControlParamsV1::UpdatePlan(plan).encode());
                Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }
            })),
            // The subscribe, and the one endpoint that records the block it landed in: the record's
            // `lock_until_block` is the host's `current_block + plan.duration_blocks`, so the
            // capability `verify_access` must present is derived from a value only the chain knows.
            // `verify_state` is where the fixture learns it.
            EndpointSpec {
                name: "SubscribeV1", is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let fixture = fixture.clone();
                    move |chain| {
                        fixture.lock().unwrap().height = Some(chain.height().get());
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let fixture = fixture.clone();
                    move || {
                        let f = fixture.lock().unwrap();
                        let note = f.note.as_ref().ok_or_else(|| dwow_core::Error::Custom("note not issued".into()))?;
                        let asset_id = f.asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                        // The id the circuit derives, over the values this call passes.
                        let id = subscription_id(&sub_pub, asset_id);
                        // The payment: the plan's price, with the blind the contract derives
                        // (`poseidon_hash([plan.price, commitment])`) so its value check accepts it.
                        let blind_seed = poseidon_hash([pallas::Base::from(PLAN_PRICE), id.inner()]);
                        let value_blind = Blind(fp_mod_fv(blind_seed).expect("base fits scalar"));
                        let vc = pedersen_commitment_u64(PLAN_PRICE, value_blind.clone());
                        #[expect(clippy::unwrap_used, reason = "a Pedersen commitment is never the identity point")]
                        let (vc_x, vc_y) = { let a = vc.to_affine(); let c = a.coordinates().unwrap(); (*c.x(), *c.y()) };
                        let child = pn_transfer_child(note, PLAN_PRICE, blind_seed, pallas::Base::from(7u64), pallas::Base::zero())?;
                        let r = h.subscribe(sub_secret, ACCESS_NONCE, vec![MerkleNode::new(pallas::Base::from(0u64))], value_blind.inner(), pallas::Base::from(2u64), pallas::Base::from(3u64), 1000, pallas::Base::from(4u64), 0, vec![MerkleNode::new(pallas::Base::from(0u64))], 0, vec![MerkleNode::new(pallas::Base::from(0u64))], id.inner(), sub_pub, PLAN_ID, PLAN_PRICE, asset_id, PLAN_DURATION, pallas::Base::from(6u64), 100, vc_x, vc_y, pallas::Base::from(9u64), pallas::Base::from(10u64), pallas::Base::from(11u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // The access check. The capability is the one the **record** derives (`Subscription::
            // access_capability`), which is what makes it a check rather than a formality: the plan
            // and the expiry are the record's, and the nonce is this call's.
            mk_ep("VerifyAccessV1", true, Box::new({
                let fixture = fixture.clone();
                move || {
                    let f = fixture.lock().unwrap();
                    let asset_id = f.asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                    let id = subscription_id(&sub_pub, asset_id);
                    let lock = f.height.ok_or_else(|| dwow_core::Error::Custom("subscribe height unknown".into()))? + PLAN_DURATION;
                    #[expect(clippy::unwrap_used, reason = "PublicKey rejects identity, so x()/y() is always Some")]
                    let (px, py) = (sub_pub.x().unwrap(), sub_pub.y().unwrap());
                    let capability = SubscriptionHarness::access_capability(px, py, PLAN_ID, id.inner(), lock, ACCESS_NONCE);
                    let r = h.verify_access(sub_secret, ACCESS_NONCE, 1, 0, vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Base::from(2u64), pallas::Base::from(3u64), capability, id.inner(), 100, px, py, PLAN_ID, lock, 10, 3600, 5, 100, 5, pallas::Base::from(6u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // NEGATIVE — `OBL-C84`'s control: a **real** capability, derived exactly as the circuit
            // derives it with every witness agreeing, for plan 2 instead of the record's plan 1. The
            // proof verifies; what refuses it is the host comparing the published capability against
            // the one the record derives. Remove that comparison and this endpoint grants access.
            EndpointSpec {
                name: "verify_access_wrong_plan", is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let fixture = fixture.clone();
                    move || {
                        let f = fixture.lock().unwrap();
                        let asset_id = f.asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                        let id = subscription_id(&sub_pub, asset_id);
                        let lock = f.height.ok_or_else(|| dwow_core::Error::Custom("subscribe height unknown".into()))? + PLAN_DURATION;
                        #[expect(clippy::unwrap_used, reason = "PublicKey rejects identity, so x()/y() is always Some")]
                        let (px, py) = (sub_pub.x().unwrap(), sub_pub.y().unwrap());
                        let capability = SubscriptionHarness::access_capability(px, py, PLAN_ID + 1, id.inner(), lock, ACCESS_NONCE);
                        let r = h.verify_access(sub_secret, ACCESS_NONCE, 1, 0, vec![MerkleNode::new(pallas::Base::from(0u64))], pallas::Base::from(2u64), pallas::Base::from(3u64), capability, id.inner(), 100, px, py, PLAN_ID + 1, lock, 10, 3600, 5, 100, 5, pallas::Base::from(6u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                }),
            },
            // The usage record. Its nullifier is `poseidon_hash([id, secret])` — the same value
            // `cancel` presents, which is why the two cannot both follow one subscription.
            mk_ep("UpdateUsageV1", true, Box::new({
                let fixture = fixture.clone();
                move || {
                    let f = fixture.lock().unwrap();
                    let asset_id = f.asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                    let id = subscription_id(&sub_pub, asset_id);
                    #[expect(clippy::unwrap_used, reason = "PublicKey rejects identity, so x()/y() is always Some")]
                    let (px, py) = (sub_pub.x().unwrap(), sub_pub.y().unwrap());
                    let nullifier = poseidon_hash([id.inner(), sub_secret]);
                    let block = f.height.ok_or_else(|| dwow_core::Error::Custom("subscribe height unknown".into()))?;
                    let r = h.update_usage(id.inner(), px, py, pallas::Base::from(block), pallas::Base::from(7u64), sub_secret, block, nullifier, vec![pallas::Base::from(0u64)]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // `CancelV1` and `RenewV1` build their proofs from `empty_witnesses` — the harness has no
            // client for either circuit — so they are **declared placeholders** (`OBL-C88`'s
            // standard) and are not evidence about this contract until those clients exist
            // (`OBL-C106`). The fixture supplies what their arms need: the record, the nullifier, and
            // for `renew` a payment child.
            mk_ep("CancelV1", true, Box::new({
                let fixture = fixture.clone();
                move || {
                    let f = fixture.lock().unwrap();
                    let asset_id = f.asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                    let id = subscription_id(&sub_pub, asset_id);
                    let nullifier = poseidon_hash([id.inner(), sub_secret]);
                    let block = f.height.ok_or_else(|| dwow_core::Error::Custom("subscribe height unknown".into()))?;
                    let r = h.cancel(id.inner(), sub_secret, nullifier, block, sub_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            mk_ep("RenewV1", true, Box::new({
                let fixture = fixture.clone();
                move || {
                    let f = fixture.lock().unwrap();
                    let asset_id = f.asset_id.ok_or_else(|| dwow_core::Error::Custom("asset not known".into()))?;
                    let id = subscription_id(&sub_pub, asset_id);
                    let lock = f.height.ok_or_else(|| dwow_core::Error::Custom("subscribe height unknown".into()))? + PLAN_DURATION;
                    let nullifier = poseidon_hash([id.inner(), sub_secret]);
                    let note2 = f.note2.as_ref().ok_or_else(|| dwow_core::Error::Custom("second note not issued".into()))?;
                    let blind_seed = poseidon_hash([pallas::Base::from(PLAN_PRICE), id.inner()]);
                    let child = pn_transfer_child(note2, PLAN_PRICE, blind_seed, pallas::Base::from(9u64), pallas::Base::zero())?;
                    let r = h.renew(id.inner(), sub_secret, lock, nullifier, pallas::Point::identity(), vec![pallas::Base::from(0u64)]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),

        ],
    }
}
