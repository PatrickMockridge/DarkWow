//! ContractTestSpec for tender. Tier: HARVESTABLE.
//!
//! # The capability gate, and why the last three rows exist
//!
//! `submit_bid_with_capability_v1` (0x08) requires exactly one child: an
//! `Identity::VerifyCapabilityV1`. The parent then decodes that child's params and compares
//! `capability_proof.capability_id` against `Tender.required_capability`, read from the **stored
//! tender record**.
//!
//! That comparison is the gate, and until this file it could not run. `Tender.required_capability` is
//! written by `create_tender_with_capability_v1` alone (`entrypoint.rs:985`), and that endpoint had
//! no client, no harness method and no row here — so every tender's field was `None`, the
//! `if let Some(required)` branch never executed, and the endpoint enforced only the child's *shape*.
//! A row asserting `Rejection` cannot see the difference: every rejection is satisfied by the
//! comparison being absent *or* by an earlier failure in the frame.
//!
//! So `CreateTenderWithCapabilityV1` is declared below and the two rows after it are a pair, read as
//! one instrument:
//!
//! * `SubmitBidWithCapabilityV1_CorrectCapability` — the child proves the capability the tender
//!   requires, and the call must **succeed**.
//! * `SubmitBidWithCapabilityV1_WrongCapability` — the identical frame, except the child proves a
//!   *different* capability, and the call must be rejected with `Custom(30)` (`InvalidCapability`),
//!   asserted **by name**.
//!
//! The first is the second's control: it proves children, selectors, contract ids, the parent's own
//! proof and its state guards are all satisfied in a byte-identical frame — so `Custom(30)` cannot be
//! a rejection for some other reason. And the negative row is the guard's control: without it the
//! comparison could be inverted with every test still green.
//!
//! **The two capabilities differ by SCHEMA, not by name**, copied from `insurance_market_spec.rs:30`:
//! `compute_capability_id` hashes the requirement's first eight bytes `++ name`, and the requirement
//! begins with `schema_hash` — so two capabilities registered with the same schema are the **same
//! capability** however they are named. A "wrong capability" built from a second name would be the
//! *same* id, the guard would accept it, and the row would pass while testing nothing.
//!
//! # What `CreateTenderWithCapabilityV1` is, and what that costs
//!
//! It is **proof-less by design**: `manifest.toml:47-48` gives it no `requires_proof` and no
//! `proof_circuit`, and its metadata arm returns an *encoded empty* `zk_public_inputs` with the
//! comment *"No circuit exists yet — deferred to v1.1"* (`entrypoint.rs:175-180`) — encoded rather
//! than bare because a zero-byte buffer is the host's documented rejection signal (`OBL-C77`). So a
//! caller builds the call data directly and there is no proof to build; that is why this row is
//! hand-encoded exactly as `CloseTenderV1` is, and why no `client/` module should be added for it.
//!
//! **And because it is proof-less, its `requester_pub_x/y` are unauthenticated.** `create_tender`
//! proves the requester; this endpoint takes the coordinates as params, so any caller can create a
//! tender attributed to any requester. The `tender_id` is unauthenticated too, which is why this
//! fixture may choose one freely. Both are recorded in the register rather than fixed here.
use dwow_contract_test_harness::harness::{ContractHarness, IdentityHarness, TenderHarness};
use dwow_identity_contract::model::{CapabilityId, CredentialRequirement};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, poseidon_hash, IntentNullifier, PublicKey, SecretKey,
    IDENTITY_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting_naming};
use std::cell::RefCell; use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// The identity material the two capability rows need. A struct rather than a tuple because the rows
/// read six fields by name, and a positional tuple of six is exactly how a wrong-argument bug hides.
#[derive(Clone)]
struct CapSetup {
    cap_a: CapabilityId,
    cap_b: CapabilityId,
    schema_a: pallas::Base,
    schema_b: pallas::Base,
    credential_secret_a: pallas::Base,
    credential_secret_b: pallas::Base,
    capability_secret: pallas::Base,
    attribute_blind: pallas::Base,
    issuer_secret: pallas::Base,
}

/// Far future, as `insurance_market_spec.rs:139` has it: an expiry the fixture never reaches, so no
/// row can fail for having crossed it.
const EXPIRES_AT: u64 = 1_000_000;

pub fn tender_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(TenderHarness::spawn()));
    let h: &TenderHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/tender/dwow_tender_contract.wasm");
    let r_sk = pallas::Base::from(10u64); let r_pk = PublicKey::from_secret(SecretKey::from_base(r_sk));
    let b_sk = pallas::Base::from(20u64); let b_pk = PublicKey::from_secret(SecretKey::from_base(b_sk));
    // The tender the three capability rows share. Chosen rather than derived — see the header note:
    // the endpoint takes the id as a param and never checks its derivation, so the fixture may pick
    // one. `CreateTenderV1` derives its own, and the two do not collide.
    let cap_tender_id = pallas::Base::from(0x0ca9_ab1eu64);

    // The identity material, written by `setup` and read by the two capability rows.
    let shared: Arc<Mutex<Option<CapSetup>>> = Arc::new(Mutex::new(None));
    let read_setup = |shared: &Arc<Mutex<Option<CapSetup>>>| -> dwow_core::Result<CapSetup> {
        shared.lock().map_err(|_| dwow_core::Error::Custom("capability setup cell poisoned".into()))?
            .clone().ok_or_else(|| dwow_core::Error::Custom("capability setup did not run".into()))
    };

    // **The derived tender id, carried from the row that creates it to the rows that use it.**
    //
    // Every row in this spec used to pass `pallas::Base::from(1u64)` as the tender id, while
    // `TenderHarness::create_tender` derives a real one (`Tender::derive_id(...)`, returned as
    // `CreateTenderResult::tender_id`). Measured on the fixture's own log before this change: block 2
    // created the tender as `0x225ff829…` and block 3's `SubmitBidV1` asked for `0x134f374c…`, so the
    // handler answered `Tender not found` and the block was rejected. **`test_heavyweight_tender` had
    // therefore never exercised a row past `CreateTenderV1`** — three of its four rows were
    // unreachable and the fixture was red for that reason, not for a contract defect.
    //
    // A cell rather than a `setup`: the id is produced *by* a row (the runner builds each row's call
    // data and submits it in order), so it cannot be known when the spec is constructed. The rows run
    // in declaration order, and on the determinism replay in the same order, so the value is written
    // before it is read.
    let tender_id: Rc<RefCell<Option<pallas::Base>>> = Rc::new(RefCell::new(None));
    let read_id = |cell: &Rc<RefCell<Option<pallas::Base>>>| -> pallas::Base {
        cell.borrow().expect("CreateTenderV1 is declared first and runs first")
    };
    // **And the bid id, for the same reason and by the same route.** `RevealBidV1` and
    // `SelectWinnerV1` were both passed a hardcoded `pallas::Base::from(1u64)` as the bid id while the
    // bid `SubmitBidV1` submits is a Poseidon commitment over the tender, the bidder's key, the amount
    // and a nonce — so the reveal asked for a bid that does not exist. Measured on the fixture's log:
    // `[tender::reveal_bid_get_metadata_v1] ERROR: Bid not found`. The bid id is the harness's
    // `SubmitBidResult::public_inputs.bid_id`, which the submitting row now carries forward.
    let bid_id: Rc<RefCell<Option<pallas::Base>>> = Rc::new(RefCell::new(None));

    ContractTestSpec { name: "tender", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        // **The identity setup, copied from `insurance_market_spec.rs:156-240` minus its
        // promissory-note half** — tender's capability child needs no PN payment, so only the issuer,
        // the two credentials and the two capabilities are built. The five submits it makes shift
        // every row's height (the runner re-reads `height_before` *after* the setup), which is why
        // the deadlines on `CreateTenderV1` are larger than they once were.
        setup: Some(Box::new({
            let shared = shared.clone();
            move |chain| {
                let id_cid = *IDENTITY_CONTRACT_ID;
                let issuer_secret = pallas::Base::from(10u64);
                let credential_secret_a = pallas::Base::from(20u64);
                let credential_secret_b = pallas::Base::from(21u64);
                let schema_a = pallas::Base::from(30u64);
                let schema_b = pallas::Base::from(31u64);
                let attribute_blind = pallas::Base::from(300u64);
                let capability_secret = pallas::Base::from(777u64);
                let issuer_pub = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                let oh = |e: String| dwow_core::Error::Custom(e);

                let id = IdentityHarness::spawn();

                let issuer = id
                    .register_issuer(issuer_pub, b"tenderer".to_vec(), vec![])
                    .map_err(|e| oh(format!("register_issuer: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &issuer.call_data, vec![])?.submit())?;

                // The commitment is fixed by these parts, and `verify_capability` below must supply
                // the same ones or the proof is about a different credential.
                let cred_a = id
                    .issue_credential(issuer_secret, credential_secret_a,
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        attribute_blind, schema_a, 0, EXPIRES_AT)
                    .map_err(|e| oh(format!("issue_credential A: {e}")))?;
                let cred_b = id
                    .issue_credential(issuer_secret, credential_secret_b,
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        attribute_blind, schema_b, 0, EXPIRES_AT)
                    .map_err(|e| oh(format!("issue_credential B: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &cred_a.call_data, vec![cred_a.proof.clone()])?.submit())?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &cred_b.call_data, vec![cred_b.proof.clone()])?.submit())?;

                // Two capabilities, differing only in schema — which is what makes their ids differ.
                // A second *name* would not: the id hashes the requirement's first eight bytes, and
                // the requirement starts with the schema hash.
                let reg_a = id
                    .register_capability(b"bidder_licence".to_vec(),
                        CredentialRequirement {
                            schema_hash: schema_a.to_repr(), issuer_pub,
                            min_threshold: 1, attribute_name: b"role".to_vec(),
                        }, None)
                    .map_err(|e| oh(format!("register_capability A: {e}")))?;
                let reg_b = id
                    .register_capability(b"bidder_licence".to_vec(),
                        CredentialRequirement {
                            schema_hash: schema_b.to_repr(), issuer_pub,
                            min_threshold: 1, attribute_name: b"role".to_vec(),
                        }, None)
                    .map_err(|e| oh(format!("register_capability B: {e}")))?;
                let cap_a = reg_a.capability_id;
                let cap_b = reg_b.capability_id;
                if cap_a.inner() == cap_b.inner() {
                    return Err(oh("the two capabilities share an id, so this fixture cannot \
                        distinguish them — the schemas must differ".into()))
                }
                smol::block_on(chain.block()?.with_call(id_cid, &id, &reg_a.call_data, vec![])?.submit())?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &reg_b.call_data, vec![])?.submit())?;

                // Issuance. `verify_capability` loads the capability *definition* rather than this
                // record, so these are fidelity rather than a precondition — stated because a reader
                // would otherwise assume they are load-bearing.
                for (cid_cap, secret, commitment) in [
                    (cap_a, credential_secret_a, cred_a.public_inputs.commitment),
                    (cap_b, credential_secret_b, cred_b.public_inputs.commitment),
                ] {
                    let nf = IntentNullifier::from_base(poseidon_hash([
                        pallas::Base::from(1u64), secret, commitment,
                    ]));
                    let iss = id.issue_capability(cid_cap, issuer_pub, nf)
                        .map_err(|e| oh(format!("issue_capability: {e}")))?;
                    smol::block_on(chain.block()?.with_call(id_cid, &id, &iss.call_data, vec![])?.submit())?;
                }

                *shared.lock().map_err(|_| oh("capability setup cell poisoned".into()))? = Some(CapSetup {
                    cap_a, cap_b, schema_a, schema_b,
                    credential_secret_a, credential_secret_b,
                    capability_secret, attribute_blind, issuer_secret,
                });
                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            mk_ep("CreateTenderV1", true, Box::new({ let cell = tender_id.clone(); move || {
                // **The deadlines are small because the fixture is.** They were 500/1000/2000 while
                // the runner submits one row per block, so `bid_deadline` at 500 was unreachable and
                // every later row was refused *"Tender not in reveal state"* — the state simply never
                // advanced.
                //
                // They are also **absolute block heights**, so the capability `setup` above, which
                // occupies five blocks before the first row, moved every one of them. Against a
                // seven-row fixture the choice has to satisfy `bid_deadline` *between* the bid and the
                // close, and the fixture cannot tell whether a rejected row advances the height — so
                // these are the values that work under **both** readings. Rejected: 11 (bid at 10 or
                // 11, close at 11 or 12).
                let r = h.create_tender(r_pk, r_sk, "Test Tender".to_string(), pallas::Base::from(1u64), pallas::Base::from(2u64), 100, 10000, 11, 13, 15)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                *cell.borrow_mut() = Some(r.tender_id);
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            // **The negative control for the child-shape half of the binding, and it is red against
            // the code that preceded it.** `SubmitBidWithCapabilityV1` had no row here at all, so
            // nothing exercised the endpoint — and before the fix it accepted a bid with **no children
            // of any kind**: its only checks were `params.required_capability_id` against tender's own
            // config (a caller-supplied param, satisfied by naming the right id) and
            // `params.capability_predicate_result != ONE`, which required the proof to equal the
            // constant its own circuit pins that witness to (`submit_bid_with_capability.zk:91-92`,
            // `:118`). A call with `children: vec![]` and a predicate of one therefore reached
            // bid creation and succeeded. It is refused now, by name: `TenderError::CapabilityRequired`
            // is `Custom(28)`. The needle is the error code rather than a bare `Rejection`, because a
            // row asserting only `Rejection` is satisfied by any earlier failure in the frame
            // (`OBL-C163`).
            //
            // **It is declared here, before `SubmitBidV1`, because it needs a tender that is still
            // accepting bids.** The handler refuses with a state error before it reaches the child
            // checks, so a row declared after `CloseTenderV1` would be rejected for the wrong reason
            // and the needle would fail — which is how the ordering was found.
            //
            // **What it covers is the child-*shape* half only**: a bid with no child is refused. The
            // `capability_id == required_capability` half is what the last two rows below test, and it
            // needs a tender that *has* a requirement — which is what the two rows before them create.
            mk_ep_rejecting_naming("SubmitBidWithCapabilityV1_NoChild", true, &["ContractError(Custom(28))"],
                Box::new({ let cell = tender_id.clone(); move || {
                    let r = h.submit_bid_with_capability(
                        read_id(&cell), b_pk, b_sk, 5000,
                        pallas::Base::from(5u64),
                        pallas::Base::from(7u64),
                        // The constant the circuit pins the witness to, or the proof itself fails and
                        // the frame is refused for a reason this row is not testing.
                        pallas::Base::one(),
                        pallas::Base::from(9u64),
                        b"encrypted".to_vec(),
                    ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }})),
            mk_ep("SubmitBidV1", true, Box::new({ let cell = tender_id.clone(); let bids = bid_id.clone(); move || {
                let r = h.submit_bid(read_id(&cell), b_pk, b_sk, 5000, pallas::Base::from(3u64), pallas::Base::from(4u64), b"encrypted".to_vec())
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                *bids.borrow_mut() = Some(r.public_inputs.bid_id);
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            // **`CloseTenderV1` was missing, and without it nothing past this point was reachable.**
            // It is the only transition to `TenderState::Revealed` (`entrypoint.rs:748`), and
            // `reveal_bid` and `select_winner` both refuse unless the tender is `Revealed`
            // (`:364`, `:452`). So before this row the fixture could never present a tender in that
            // state, and its last two rows were unreachable for a reason the fixture could not
            // express. **No proof is needed**: the manifest gives `close_tender` no
            // `requires_proof` and no `proof_circuit`, and `client/` has no `close_tender.rs` — which
            // is why it looked unreachable and is not: the endpoint takes params and nothing else, so
            // a caller builds the call data directly, and so does this row.
            mk_ep("CloseTenderV1", false, Box::new({ let cell = tender_id.clone(); move || {
                let (rx, ry) = r_pk.xy().expect("requester pk is not identity");
                let params = dwow_tender_contract::model::CloseTenderParamsV1 {
                    tender_id: read_id(&cell),
                    requester_pub_x: rx,
                    requester_pub_y: ry,
                };
                let mut call_data = vec![0x03u8];
                call_data.extend_from_slice(&params.encode());
                Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
            }})),
            mk_ep("RevealBidV1", true, Box::new({ let cell = tender_id.clone(); let bids = bid_id.clone(); move || {
                let r = h.reveal_bid(read_id(&cell), read_id(&bids), b_pk, b_sk, 5000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            mk_ep("SelectWinnerV1", true, Box::new({ let cell = tender_id.clone(); let bids = bid_id.clone(); move || {
                let r = h.select_winner(read_id(&cell), read_id(&bids), r_pk, r_sk, b_pk, 5000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            // ── The writer: the only endpoint that can give a tender a capability requirement ──
            //
            // Until this row existed, `Tender.required_capability` was always `None` and the gate
            // above was dormant. It is proof-less (see the header), so the call is the selector plus
            // `Params::encode()`, exactly as `CloseTenderV1` is built.
            //
            // The deadlines are generous and unrelated to the fixture's other tender: nothing closes
            // this one, so the only requirement is that they are after the two bid rows below.
            mk_ep("CreateTenderWithCapabilityV1", false, Box::new({
                let shared = shared.clone();
                move || {
                    let s = read_setup(&shared)?;
                    let (rx, ry) = r_pk.xy().expect("requester pk is not identity");
                    let params = dwow_tender_contract::model::CreateTenderWithCapabilityParamsV1 {
                        proof: vec![],
                        tender_id: cap_tender_id,
                        requester_pub_x: rx,
                        requester_pub_y: ry,
                        title: "Capability Tender".to_string(),
                        specification: pallas::Base::from(2u64),
                        attestation_id: pallas::Base::from(1u64),
                        min_bid: 100,
                        max_bid: 10000,
                        bid_deadline: 24,
                        reveal_deadline: 30,
                        delivery_deadline: 40,
                        required_capability: Some(s.cap_a.to_bytes()),
                        required_dag_id: None,
                    };
                    let mut call_data = vec![0x07u8];
                    call_data.extend_from_slice(&params.encode()
                        .map_err(|e| dwow_core::Error::Custom(format!("encode: {e}")))?);
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }
            })),
            // ── THE NEGATIVE CASE. Identical to the positive row below except in which capability the
            // child proves, so a rejection names the guard rather than the frame. ──
            mk_ep_rejecting_naming("SubmitBidWithCapabilityV1_WrongCapability", true, &["ContractError(Custom(30))"],
                Box::new({
                    let shared = shared.clone();
                    let id = Box::leak(Box::new(IdentityHarness::spawn()));
                    move || {
                        let s = read_setup(&shared)?;
                        // The child proves capability B; the tender requires A.
                        let holder_b = PublicKey::from_secret(SecretKey::from_base(s.credential_secret_b));
                        let v = id
                            .verify_capability(s.credential_secret_b, s.cap_b.inner(),
                                pallas::Base::from(50u64),
                                b"role", pallas::Base::from(100u64),
                                b"tenure", pallas::Base::from(200u64),
                                s.attribute_blind, s.capability_secret,
                                PublicKey::from_secret(SecretKey::from_base(s.issuer_secret)),
                                holder_b, s.schema_b, 0, EXPIRES_AT, true)
                            .map_err(|e| dwow_core::Error::Custom(format!("verify_capability B: {e}")))?;
                        let child = ChildCall {
                            contract_id: *IDENTITY_CONTRACT_ID,
                            call_data: v.call_data,
                            proofs: vec![v.proof],
                            children: vec![],
                        };
                        let r = h.submit_bid_with_capability(
                            cap_tender_id, b_pk, b_sk, 5000,
                            pallas::Base::from(21u64),
                            s.cap_a.inner(),
                            pallas::Base::one(),
                            pallas::Base::from(31u64),
                            b"encrypted".to_vec(),
                        ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                    }
                })),
            // ── THE POSITIVE CASE, and the control for the row above: the same frame with the
            // capability the tender actually requires must be ACCEPTED. ──
            mk_ep("SubmitBidWithCapabilityV1_CorrectCapability", true, Box::new({
                let shared = shared.clone();
                let id = Box::leak(Box::new(IdentityHarness::spawn()));
                move || {
                    let s = read_setup(&shared)?;
                    let holder_a = PublicKey::from_secret(SecretKey::from_base(s.credential_secret_a));
                    let v = id
                        .verify_capability(s.credential_secret_a, s.cap_a.inner(),
                            pallas::Base::from(50u64),
                            b"role", pallas::Base::from(100u64),
                            b"tenure", pallas::Base::from(200u64),
                            s.attribute_blind, s.capability_secret,
                            PublicKey::from_secret(SecretKey::from_base(s.issuer_secret)),
                            holder_a, s.schema_a, 0, EXPIRES_AT, true)
                        .map_err(|e| dwow_core::Error::Custom(format!("verify_capability A: {e}")))?;
                    let child = ChildCall {
                        contract_id: *IDENTITY_CONTRACT_ID,
                        call_data: v.call_data,
                        proofs: vec![v.proof],
                        children: vec![],
                    };
                    let r = h.submit_bid_with_capability(
                        cap_tender_id, b_pk, b_sk, 5000,
                        pallas::Base::from(22u64),
                        s.cap_a.inner(),
                        pallas::Base::one(),
                        pallas::Base::from(32u64),
                        b"encrypted".to_vec(),
                    ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
        ],
    }
}
