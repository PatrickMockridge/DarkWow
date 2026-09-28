//! ContractTestSpec for tender. Tier: HARVESTABLE.
use dwow_contract_test_harness::harness::{ContractHarness, TenderHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey}; use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting};
use std::cell::RefCell; use std::rc::Rc;

pub fn tender_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(TenderHarness::spawn()));
    let h: &TenderHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/tender/dwow_tender_contract.wasm");
    let r_sk = pallas::Base::from(10u64); let r_pk = PublicKey::from_secret(SecretKey::from_base(r_sk));
    let b_sk = pallas::Base::from(20u64); let b_pk = PublicKey::from_secret(SecretKey::from_base(b_sk));

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

    ContractTestSpec { name: "tender", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm), has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            mk_ep("CreateTenderV1", true, Box::new({ let cell = tender_id.clone(); move || {
                // **The deadlines are small because the fixture is.** They were 500/1000/2000 while
                // the runner submits one row per block from height 2 upwards, so `bid_deadline` at
                // 500 was unreachable and every later row was refused *"Tender not in reveal state"* —
                // the state simply never advanced. These are chosen against the block each row is
                // verified at: create at 2, the capability control at 3, the bid at 4, the close at 5.
                let r = h.create_tender(r_pk, r_sk, "Test Tender".to_string(), pallas::Base::from(1u64), pallas::Base::from(2u64), 100, 10000, 5, 6, 8)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                *cell.borrow_mut() = Some(r.tender_id);
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            // **The negative control for the capability binding, and it is red against the code that
            // preceded it.** `SubmitBidWithCapabilityV1` had no row here at all, so nothing exercised
            // the endpoint — and before the fix it accepted a bid with **no children of any kind**:
            // its only checks were `params.required_capability_id` against tender's own config (a
            // caller-supplied param, satisfied by naming the right id) and
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
            // **What it does not cover, stated rather than implied**: the
            // `capability_id == required_capability` comparison below it. `Tender.required_capability`
            // is written only by `create_tender_with_capability_v1`, which has **no proof path** —
            // `client/` has no `create_tender_with_capability.rs` — so no tender can carry a
            // requirement and that branch never runs. A control for it needs that path first
            // (`OBL-C186`).
            mk_ep_rejecting("SubmitBidWithCapabilityV1_NoChild", true, &["ContractError(Custom(28))"],
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
            mk_ep("SubmitBidV1", true, Box::new({ let cell = tender_id.clone(); move || {
                let r = h.submit_bid(read_id(&cell), b_pk, b_sk, 5000, pallas::Base::from(3u64), pallas::Base::from(4u64), b"encrypted".to_vec())
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
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
            mk_ep("RevealBidV1", true, Box::new({ let cell = tender_id.clone(); move || {
                let r = h.reveal_bid(read_id(&cell), pallas::Base::from(1u64), b_pk, b_sk, 5000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
            mk_ep("SelectWinnerV1", true, Box::new({ let cell = tender_id.clone(); move || {
                let r = h.select_winner(read_id(&cell), pallas::Base::from(1u64), r_pk, r_sk, b_pk, 5000)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            }})),
        ],
    }
}
