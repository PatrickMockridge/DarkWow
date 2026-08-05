//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE — 13 harness methods.
//! 6 non-ZK endpoints active, 7 ZK pending. pay_premium has circuit bug.
use dwow_contract_test_harness::harness::{DaoEscrowHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn dao_escrow_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DaoEscrowHarness::spawn()));
    let h: &DaoEscrowHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
    let owner_secret = pallas::Base::from(12345u64);
    let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
    let dao_bulla = pallas::Base::from(1u64);
    let claim_id = pallas::Base::from(100u64);
    let proposal_id = pallas::Base::from(200u64);
    let capability_id = pallas::Base::from(999u64);
    let identity_contract_bulla = pallas::Base::from(300u64);

    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false, state_trees: harness.state_trees(),
        endpoints: vec![
            mk_ep("WithdrawV1", false, Box::new(move || {
                let r = h.withdraw(dao_bulla, owner_pub, 50_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("EndowmentWithdrawV1", false, Box::new(move || {
                let r = h.endowment_withdraw(dao_bulla, claim_id, owner_pub, 25_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("TreasurySpendV1", false, Box::new(move || {
                let r = h.treasury_spend(dao_bulla, proposal_id, owner_pub, 10_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("ExecuteClaimV1", false, Box::new(move || {
                let r = h.execute_claim(dao_bulla, proposal_id, owner_pub, 75_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("RegisterCapabilityRequirementV1", false, Box::new(move || {
                let r = h.register_capability_requirement(dao_bulla, b"member_vote".to_vec(), [0u8; 32], identity_contract_bulla).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("CancelClaimV1", false, Box::new(move || {
                let r = h.cancel_claim(dao_bulla, claim_id, owner_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![] })
            })),
        ],
    }
}
