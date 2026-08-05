//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE — 13 harness methods.
//! 8 endpoints active (6 non-ZK + 2 ZK), 5 pending.
use dwow_contract_test_harness::harness::{DaoEscrowHarness, ContractHarness};
use dwow_sdk::crypto::{PublicKey, SecretKey, IntentNullifier, pasta_prelude::PrimeField};
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
    let nullifier_k = pallas::Scalar::from(1u64);
    let endowment_token_id = pallas::Base::from(42u64);
    let bulla_blind = pallas::Base::from(9999u64);
    let voter_secret = pallas::Base::from(333u64);
    let voter_pub = PublicKey::from_secret(SecretKey::from_base(voter_secret));
    let capability_secret = pallas::Base::from(888u64);
    let cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
        capability_id: capability_id.to_repr(),
        capability_secret: capability_secret.to_repr(),
        nullifier: IntentNullifier::from_bytes([0u8; 32]).unwrap(),
        issuer_pub: [0u8; 32],
        predicate_result: [0u8; 32],
        proof: vec![],
    };

    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize(nullifier_k, dao_bulla, owner_secret, endowment_token_id, bulla_blind).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
            Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
        })),
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
            mk_ep("VoteClaimV1", true, Box::new(move || {
                let r = h.vote_claim(nullifier_k, pallas::Point::default(), pallas::Point::default(), proposal_id, capability_id, capability_secret, voter_secret, true, pallas::Scalar::from(1u64), dao_bulla, claim_id, voter_pub, cap_proof.clone()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { call_data: r.call_data, proofs: vec![r.proof] })
            })),
        ],
    }
}
