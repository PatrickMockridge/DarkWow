//! ContractTestSpec for insurance_market. Tier: UNDERPOWERED — 4 harness methods.
//! 2 real proofs (underwrite, purchase_coverage), 2 empty_witnesses.
//! 12 functions have NO harness methods.
//!
//! # What the child-call repair did, and what this file's previous comments got wrong
//!
//! The OBL-Z16 repair made `underwrite_with_capability` (0x09) and
//! `purchase_coverage_with_capability` (0x0a) require child calls they did not, and
//! `purchase_coverage_with_dag` (0x0b) one that it did not. This file's header used to say those
//! endpoints are rejected *because their calls carry `children: vec![]`*. **That was wrong, and the
//! correction is the point of the note:** all three harness methods encode the WRONG params type into
//! their selector — `h.underwrite` and `h.purchase_coverage` and `h.purchase_coverage_dag` each take a
//! `…ParamsV1` that is smaller than the type the selector actually decodes — so those calls are turned
//! away at `metadata-decode-zkp` (the contract's `get_metadata` cannot decode them, it returns an empty
//! vector, and `execution.rs` reads an empty buffer as the documented rejection signal) **before the
//! child-count check is ever reached**. A rejection assertion is satisfied by any earlier failure, which
//! is why the wrong explanation survived.
//!
//! The rows below are therefore split by what they can actually name:
//!
//! * `UnderwriteWithCapabilityV1_NoChildren` now uses `h.underwrite_with_capability`, which fixes the
//!   params type, so it genuinely reaches the guard and is rejected for the reason it claims —
//!   `Custom(33)` (`InvalidChildrenIndexes`). It asserts that by name.
//! * `PurchaseCoverageWithCapabilityV1` and `PurchaseCoverageWithDAGV1` still use mis-typed harness
//!   methods, so they remain bare `Rejection` rows with the cause stated rather than a named one. Each
//!   needs its own corrected harness method first; that is owed and is named here rather than papered
//!   over with a needle that would fail.
//! * `PurchaseCoverageDirectV1` (0x04) was asserting `Success` while `purchase_coverage.rs` requires
//!   exactly one child. **That row was red at HEAD** — it expected a call to be accepted that the
//!   contract rejects. It now names `Custom(33)`.
//!
//! # The guard's own coverage, and the obligation this file still carries
//!
//! The parent's capability gate has two parts: the child's *shape* (selector 0x06, addressed to the
//! configured Identity contract) and the *binding* — it decodes `VerifyCapabilityParams` and compares
//! `capability_proof.capability_id` against `market.required_underwriter_capability`. The second is what
//! makes it a gate rather than a shape check, and a `Rejection` row cannot test it: reaching the
//! comparison needs a well-formed frame carrying a valid PN payment child and a real Identity capability
//! proof. Those are the two rows at the end of this vec, and the fixture that feeds them is in `setup`.
//!
//! Also noted because it misleads a reader of the test report: the endpoint *named* `UnderwriteV1`
//! exercises selector 0x09, not 0x03. The names are the spec's.
use dwow_contract_test_harness::harness::{ContractHarness, InsuranceMarketHarness};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, ContractId, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;

pub fn insurance_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(InsuranceMarketHarness::spawn()));
    let h: &InsuranceMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/insurance_market/dwow_insurance_market_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(10u64)));

    ContractTestSpec {
        name: "insurance_market", is_genesis: false,
        contract_id: ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            EndpointSpec {
                name: "UnderwriteWithCapabilityV1_NoChildren",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    use dwow_insurance_market_contract::model::UnderwriteWithCapabilityParamsV1;
                    let params = UnderwriteWithCapabilityParamsV1 {
                        market_id: pallas::Base::from(1u64),
                        bond_amount: 10_000, coverage_limit: 50_000,
                        underwriter: pk,
                        capability_proof: vec![],
                        capability_secret: pallas::Base::from(1u64).to_repr(),
                    };
                    // The call is well-formed for its selector, so the rejection comes from the
                    // guard's own child-count check — `InvalidChildrenIndexes` — and not from an
                    // undecodable payload. That is the difference this row exists to pin.
                    let r = h.underwrite_with_capability(&params)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // Rejected because the harness encodes `PurchaseCoverageParamsV1` (160 B) into selector
            // 0x0a, which decodes `PurchaseCoverageWithCapabilityParamsV1` (>= 204 B). The cause is
            // therefore `metadata-decode-zkp`, NOT the omitted children; a needle is omitted here
            // because the honest one would name that stage and would stop being true the moment the
            // harness method is corrected. Owed: `h.purchase_coverage_with_capability`.
            EndpointSpec {
                name: "PurchaseCoverageWithCapabilityV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    use dwow_insurance_market_contract::model::PurchaseCoverageParamsV1;
                    let params = PurchaseCoverageParamsV1 {
                        market_id: pallas::Base::from(1u64),
                        underwriter_id: pallas::Base::from(1u64),
                        buyer: pk,
                        coverage_amount: 5000,
                        value_commit: pallas::Point::default(),
                        buyer_nullifier: pallas::Base::from(99u64),
                    };
                    let r = h.purchase_coverage(&params).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // 0x04 is the one selector whose params type the harness gets right, and
            // `purchase_coverage.rs` requires exactly one child while this call carries none. This row
            // asserted `Success` at HEAD, i.e. it asserted a call the contract rejects.
            EndpointSpec {
                name: "PurchaseCoverageDirectV1",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    use dwow_insurance_market_contract::model::PurchaseCoverageParamsV1;
                    let params = PurchaseCoverageParamsV1 {
                        market_id: pallas::Base::from(1u64),
                        underwriter_id: pallas::Base::from(1u64),
                        buyer: pk,
                        coverage_amount: 5000,
                        value_commit: pallas::Point::default(),
                        buyer_nullifier: pallas::Base::from(99u64),
                    };
                    let r = h.purchase_coverage_v1(&params).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // Rejected for the same mis-typing as `PurchaseCoverageWithCapabilityV1` above: 0x0b
            // decodes `PurchaseCoverageWithDAGParamsV1` (>= 204 B) and this encodes the 160-byte type.
            // Owed: `h.purchase_coverage_with_dag`.
            EndpointSpec {
                name: "PurchaseCoverageWithDAGV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    use dwow_insurance_market_contract::model::PurchaseCoverageParamsV1;
                    let params = PurchaseCoverageParamsV1 {
                        market_id: pallas::Base::from(1u64),
                        underwriter_id: pallas::Base::from(1u64),
                        buyer: pk,
                        coverage_amount: 5000,
                        value_commit: pallas::Point::default(),
                        buyer_nullifier: pallas::Base::from(99u64),
                    };
                    let r = h.purchase_coverage_dag(&params).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
