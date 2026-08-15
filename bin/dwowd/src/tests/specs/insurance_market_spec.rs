//! ContractTestSpec for insurance_market. Tier: UNDERPOWERED — 4 harness methods.
//! 2 real proofs (underwrite, purchase_coverage), 2 empty_witnesses.
//! 12 functions have NO harness methods.
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
            mk_ep("UnderwriteV1", true, Box::new(move || {
                use dwow_insurance_market_contract::model::UnderwriteParamsV1;
                let params = UnderwriteParamsV1 {
                    market_id: pallas::Base::from(1u64),
                    bond_amount: 10000, coverage_limit: 50000,
                    underwriter: pk,
                };
                let r = h.underwrite(&params).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("PurchaseCoverageV1", true, Box::new(move || {
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
            })),
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
            mk_ep("PurchaseCoverageWithDAGV1", true, Box::new(move || {
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
            })),
        ],
    }
}
