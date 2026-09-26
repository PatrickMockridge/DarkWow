//! ContractTestSpec for insurance_market. Tier: UNDERPOWERED — 4 harness methods.
//! 2 real proofs (underwrite, purchase_coverage), 2 empty_witnesses.
//! 12 functions have NO harness methods.
//!
//! THREE OF THE FOUR ARE NEGATIVE CONTROLS, as of the OBL-Z16 child-call repair.
//! `underwrite_with_capability` (0x09), `purchase_coverage_with_capability` (0x0a) and
//! `purchase_coverage_with_dag` (0x0b) each now require child calls — a payment transfer, plus a
//! capability proof for the first two — where before the repair they required none. Their calls in
//! this spec carry `children: vec![]`, so they must be *rejected*, and each asserts
//! `EndpointExpectation::Rejection` accordingly.
//!
//! That is a real narrowing of coverage and it is stated rather than hidden: these three endpoints
//! no longer exercise a successful call. Before the repair they did — but only because the guard was
//! missing, so what they asserted was "an unpaid bond and an unproven capability are accepted". The
//! happy path needs a pre-issued PN note (`pn_transfer_child` takes a `PnNote`) and, for the first
//! two, an Identity capability proof; neither is in this spec's setup. Recorded as an obligation.
//!
//! `PurchaseCoverageDirectV1` (0x04) is unaffected and still expects Success.
//!
//! Also noted here because it will mislead a reader of the test report: the endpoints named
//! `UnderwriteV1` and `PurchaseCoverageV1` do **not** exercise selectors 0x03/0x04. `h.underwrite`
//! and `h.purchase_coverage` build the *capability* calls (0x09/0x0a). The names are the spec's and
//! correcting them is a separate change.
use dwow_contract_test_harness::harness::{InsuranceMarketHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn insurance_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(InsuranceMarketHarness::spawn()));
    let h: &InsuranceMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/insurance_market/dwow_insurance_market_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(10u64)));

    ContractTestSpec {
        name: "insurance_market", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            // NEGATIVE CONTROL for the `underwrite_with_capability` child-call guard (OBL-Z16).
            //
            // Note the endpoint's name is wrong and is left as it is: `h.underwrite` builds an
            // **UnderwriteWithCapabilityV1** call (selector 0x09), not UnderwriteV1 (0x03). The name
            // is the spec's, and correcting it is a separate change.
            //
            // The call carries no child calls, which is now a rejection by construction: the function
            // requires two — `PN::transfer_v1` to pay the bond, and `Identity::VerifyCapabilityV1`
            // addressed to the configured Identity contract. Before the repair it required neither,
            // so this endpoint passed while accepting an arbitrary, unpaid bond from any caller.
            // That is why it asserts Rejection now: it was asserting the defect.
            //
            // The second child is checked in two parts: its shape (selector 0x06, addressed to the
            // configured Identity contract) and, once the market record is read, that the capability
            // the child verified is the one the market requires — the parent decodes
            // `VerifyCapabilityParams` and compares `capability_proof.capability_id` against
            // `market.required_underwriter_capability`. The second part is what makes the gate real
            // rather than shape-only; without it any valid capability would pass, which is where
            // `labor_market` still is.
            //
            // The happy path — a well-formed call carrying both children — is NOT covered here and is
            // recorded as an obligation. It needs a pre-issued PN note (`pn_transfer_child` takes a
            // `PnNote`, so a mint flow this spec does not have) and an Identity capability proof.
            EndpointSpec {
                name: "UnderwriteV1",
                is_zk: true,
                expectation: EndpointExpectation::Rejection,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    use dwow_insurance_market_contract::model::UnderwriteParamsV1;
                    let params = UnderwriteParamsV1 {
                        market_id: pallas::Base::from(1u64),
                        bond_amount: 10000, coverage_limit: 50000,
                        underwriter: pk,
                    };
                    let r = h.underwrite(&params).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // NEGATIVE CONTROL for the `purchase_coverage_with_capability` guard. Same shape and the
            // same mislabelled name: `h.purchase_coverage` builds PurchaseCoverageWithCapabilityV1
            // (0x0a). The omitted children are `PN::transfer_v1` (premium) and
            // `Identity::VerifyCapabilityV1`.
            EndpointSpec {
                name: "PurchaseCoverageV1",
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
            // empty_witnesses endpoints
            mk_ep("PurchaseCoverageDirectV1", true, Box::new(move || {
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
            })),
            // NEGATIVE CONTROL for the `purchase_coverage_with_dag` payment guard. This path is
            // DAG-gated rather than capability-gated, so it needs one child, not two:
            // `PN::transfer_v1` to pay the premium. It had none, so the function computed
            // `premium`, credited `underwriter.earned_premiums += premium` and wrote
            // `premium_paid: premium` into the update while nothing moved.
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
