//! ContractTestSpec for bearer_bond. Tier: READY.
use dwow_contract_test_harness::harness::{BearerBondHarness, ContractHarness};
use dwow_sdk::crypto::ContractId;
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn bearer_bond_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(BearerBondHarness::spawn()));
    let h: &BearerBondHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/bearer_bond/dwow_bearer_bond_contract.wasm");

    ContractTestSpec {
        name: "bearer_bond", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        endpoints: vec![
            mk_ep("IssueStakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::issue_stake::IssueStakeCallInput;
                let input = IssueStakeCallInput {
                    principal: 10000, maturity_block: 1000, min_claim: 1,
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                    token_id: pallas::Base::from(1u64), staker: pallas::Base::from(2u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(3u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.issue_stake(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("BurnStakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::burn_stake::BurnStakeCallInput;
                let input = BurnStakeCallInput {
                    principal: 500, token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(5u64), maturity_block: 1000,
                    leaf_position: 0, merkle_path: vec![],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(8u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.burn_stake(vec![input]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("TransferStakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::transfer_stake::{TransferStakeCallInput, TransferStakeCallOutput};
                let input = TransferStakeCallInput {
                    principal: 500, token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(5u64), last_claim_block: 10,
                    maturity_block: 1000, leaf_position: 0, merkle_path: vec![],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(9u64),
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let output = TransferStakeCallOutput {
                    recipient: pallas::Base::from(10u64), principal: 500,
                    token_id: pallas::Base::from(1u64), spend_hook: pallas::Base::zero(),
                    user_data: pallas::Base::zero(), coin_blind: pallas::Base::from(6u64),
                    last_claim_block: 10, maturity_block: 1000,
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                };
                let r = h.transfer_stake(vec![input], vec![output]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("RequestInterestV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::request_interest::RequestInterestCallInput;
                let input = RequestInterestCallInput {
                    principal: 500, token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(5u64), last_claim_block: 10,
                    maturity_block: 1000, claim_block: 100, min_claim: 1,
                    leaf_position: 0, merkle_path: vec![],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(10u64),
                    payment_key: pallas::Base::from(42u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.request_interest(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("UnstakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::unstake::{UnstakeCallInput, UnstakeCallOutput};
                let input = UnstakeCallInput {
                    principal: 500, token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(5u64), maturity_block: 1000,
                    leaf_position: 0, merkle_path: vec![],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(11u64),
                    current_block: 1001,
                    payout: 500, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let output = UnstakeCallOutput {
                    recipient: pallas::Base::from(10u64), token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(6u64),
                };
                let r = h.unstake(input, output).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("EmergencyUnstakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::emergency_unstake::{EmergencyUnstakeCallInput, EmergencyUnstakeCallOutput};
                use dwow_bearer_bond_contract::model::CoverageReport;
                let report = CoverageReport {
                    series_token_id: pallas::Base::from(1u64),
                    total_outstanding: 500, total_interest_obligation: 50,
                    reserve_amount: 100, coverage_ratio_bps: 1818, report_block: 500,
                };
                let input = EmergencyUnstakeCallInput {
                    principal: 500, token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(5u64), maturity_block: 1000,
                    leaf_position: 0, merkle_path: vec![],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(12u64),
                    coverage_report: report,
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let output = EmergencyUnstakeCallOutput {
                    recipient: pallas::Base::from(10u64), token_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(6u64),
                };
                let r = h.emergency_unstake(input, output).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("PayInterestV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::pay_interest::PayInterestCallInput;
                let input = PayInterestCallInput {
                    bond_token_commit: pallas::Base::from(99u64),
                    claim_block: 100, interest_amount: 50,
                    token_id: pallas::Base::from(1u64),
                    payment_key: pallas::Base::from(42u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    coin_blind: pallas::Base::from(5u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.pay_interest(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("ProveCoverageV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::prove_coverage::ProveCoverageCallInput;
                let input = ProveCoverageCallInput {
                    series_token_id: pallas::Base::from(1u64),
                    total_outstanding: 500, total_interest_obligation: 50,
                    reserve_amount: 100, coverage_ratio_bps: 1818, report_block: 500,
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.prove_coverage(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
        ],
    }
}

