//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE — 13 harness methods.
//! 12 endpoints active, 1 deferred (pay_premium: circuit bug).
use dwow_contract_test_harness::harness::{DaoEscrowHarness, ContractHarness};
use dwow_dao_escrow_contract::model::{CapabilityProof, ClaimType};
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
    let proposer_secret = pallas::Base::from(777u64);
    let proposer_pub = PublicKey::from_secret(SecretKey::from_base(proposer_secret));
    let holder_secret = pallas::Base::from(111u64);
    let holder_pub = PublicKey::from_secret(SecretKey::from_base(holder_secret));
    let arbitrator_secret = pallas::Base::from(600u64);
    let arbitrator_pub = PublicKey::from_secret(SecretKey::from_base(arbitrator_secret));
    let capability_secret = pallas::Base::from(888u64);
    let dispute_id = pallas::Base::from(500u64);
    let cp_id = capability_id.to_repr();
    let cp_secret = capability_secret.to_repr();

    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize(nullifier_k, dao_bulla, owner_secret, endowment_token_id, bulla_blind).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
            Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
        })),
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            mk_ep("ProposeClaimV1", true, Box::new(move || {
                let r = h.propose_claim(nullifier_k, dao_bulla, claim_id, capability_id, capability_secret, proposer_secret, 10_000, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::ZERO,issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("VoteClaimV1", true, Box::new(move || {
                let r = h.vote_claim(nullifier_k, pallas::Point::default(), pallas::Point::default(), proposal_id, capability_id, capability_secret, voter_secret, true, pallas::Scalar::from(1u64), dao_bulla, claim_id, voter_pub, CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::ZERO,issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("VerifyMemberCapabilityV1", true, Box::new(move || {
                let r = h.verify_member_capability(nullifier_k, capability_id, dao_bulla, capability_secret, holder_secret, holder_pub, CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::ZERO,issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("ResolveDisputeV1", true, Box::new(move || {
                let r = h.resolve_dispute(nullifier_k, capability_id, dao_bulla, dispute_id, capability_secret, arbitrator_secret, vec![], pallas::Base::from(700u64), true, 5000, arbitrator_pub, proposal_id, CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::ZERO,issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
            })),
            mk_ep("WithdrawV1", false, Box::new(move || {
                let r = h.withdraw(dao_bulla, owner_pub, 50_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("EndowmentWithdrawV1", false, Box::new(move || {
                let r = h.endowment_withdraw(dao_bulla, claim_id, owner_pub, 25_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("TreasurySpendV1", false, Box::new(move || {
                let r = h.treasury_spend(dao_bulla, proposal_id, owner_pub, 10_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("ExecuteClaimV1", false, Box::new(move || {
                let r = h.execute_claim(dao_bulla, proposal_id, owner_pub, 75_000_000).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("RegisterCapabilityRequirementV1", false, Box::new(move || {
                let r = h.register_capability_requirement(dao_bulla, b"member_vote".to_vec(), [0u8; 32], identity_contract_bulla).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("CancelClaimV1", false, Box::new(move || {
                let r = h.cancel_claim(dao_bulla, claim_id, owner_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
        ],
    }
}
