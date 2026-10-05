//! ContractTestSpec for bearer_bond. Tier: READY.
//!
//! Every ZK input carries a **well-formed but arbitrary** Merkle path
//! (`vec![MerkleNode::new(pallas::Base::from(0u64)); 32]`, the shape `otc_swap_spec.rs:186` uses).
//! `vec![]` panicked the client — `Witness::MerklePath` converts the path to a fixed-depth array
//! and the empty vec cannot be one — and 32 is that depth. The values are arbitrary *and that is
//! sound here*, which is worth stating rather than leaving a reader to infer: the circuits fold the
//! path into `merkle_root` and instance it, but **no arm of this contract compares that root to
//! anything** — `burn_stake_v1`'s exec checks the commitment set and the nullifier set and never
//! the root, and `apply_issue_stake` writes the commitment set without touching a Merkle tree. So
//! the proof must be *satisfiable*, not *anchored*. If a root comparison is added, this fixture
//! becomes wrong and must build a real tree in the same change.
use dwow_contract_test_harness::harness::{BearerBondHarness, ContractHarness};
use dwow_sdk::crypto::{ContractId, MerkleNode};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::mk_ep;

pub fn bearer_bond_test_spec() -> ContractTestSpec<'static> {
    // `OBL-C198`: the proof binds to a commitment derived over the call set, and a call carries the
    // contract it addresses — so the harness must be given the deployed id rather than a
    // placeholder, exactly as the node's `get_metadata` arm derives over the same bytes.
    let bearer_bond_cid = crate::tests::blockchain::derive_contract_id_from_name("bearer_bond");
    let harness = Box::leak(Box::new(BearerBondHarness::spawn(bearer_bond_cid)));
    let h: &BearerBondHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/bearer_bond/dwow_bearer_bond_contract.wasm");

    ContractTestSpec {
        name: "bearer_bond", is_genesis: false,
        contract_id: bearer_bond_cid,
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: None,
        deploy_ix: None,
        endpoints: vec![
            mk_ep("RegisterSeriesV1", false, Box::new(move || {
                // **First, and it has to be.** `IssueStakeV1` requires the `BondSeriesInfo` record
                // this creates and nothing else creates one, so without this row the contract's own
                // entry point answers `StakeNotFound` (code 1) and no endpoint after it is
                // reachable. Plaintext, so no proof: the runner is told `is_zk: false`.
                //
                // The three values are the ones `IssueStakeV1` below depends on: the series is keyed
                // by `asset_id` (`1`), and `issue_stake_v1` authorises against the stored
                // `issuer_contract`, so it names the same `[1u8; 32]` contract id that call passes.
                let r = h.register_series(
                    pallas::Base::from(1u64),
                    500u64,
                    100_000u64,
                    ContractId::from_bytes([1u8; 32]).unwrap(),
                ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            mk_ep("IssueStakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::issue_stake::IssueStakeCallInput;
                let input = IssueStakeCallInput {
                    principal: 10000, maturity_block: 1000, min_claim: 1,
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                    // `OBL-C199`: the holder's public key, computed the way every *spending* client
                    // computes it (`poseidon_hash([7, secret])`) with the same secret the bond is
                    // later spent with. `issue_stake` takes `staker` literally while the burn family
                    // derives it, so a fixture that passed an unrelated constant here minted a bond
                    // whose note commitment no later call could reproduce — and the commitment is
                    // now the bond's identity, so the two must be the same value.
                    asset_id: pallas::Base::from(1u64),
                    staker: dwow_sdk::crypto::poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(42u64)]),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.issue_stake_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("BurnStakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::burn_stake::BurnStakeCallInput;
                let input = BurnStakeCallInput {
                    principal: 10000, asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64), maturity_block: 1000,
                    leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(8u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.burn_stake_solo(vec![input]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("TransferStakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::transfer_stake::{TransferStakeCallInput, TransferStakeCallOutput};
                let input = TransferStakeCallInput {
                    principal: 10000, asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64), last_claim_block: 10,
                    maturity_block: 1000, leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(9u64),
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let output = TransferStakeCallOutput {
                    recipient: pallas::Base::from(10u64), principal: 500,
                    asset_id: pallas::Base::from(1u64), spend_hook: pallas::Base::zero(),
                    user_data: pallas::Base::zero(), commitment_blind: pallas::Base::from(6u64),
                    last_claim_block: 10, maturity_block: 1000,
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                };
                let r = h.transfer_stake_solo(vec![input], vec![output]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("RequestInterestV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::request_interest::RequestInterestCallInput;
                let input = RequestInterestCallInput {
                    principal: 10000, asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64), last_claim_block: 10,
                    maturity_block: 1000, claim_block: 100, min_claim: 1,
                    leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(10u64),
                    payment_key: pallas::Base::from(42u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.request_interest_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("UnstakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::unstake::{UnstakeCallInput, UnstakeCallOutput};
                let input = UnstakeCallInput {
                    principal: 10000, asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64), maturity_block: 1000,
                    leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(11u64),
                    current_block: 1001,
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let output = UnstakeCallOutput {
                    recipient: pallas::Base::from(10u64), asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(6u64),
                };
                let r = h.unstake_solo(input, output).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("EmergencyUnstakeV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::emergency_unstake::{EmergencyUnstakeCallInput, EmergencyUnstakeCallOutput};
                use dwow_bearer_bond_contract::model::CoverageReport;
                let report = CoverageReport {
                    series_asset_id: pallas::Base::from(1u64),
                    total_outstanding: 500, total_interest_obligation: 50,
                    reserve_amount: 100, coverage_ratio_bps: 1818, report_block: 500,
                };
                let input = EmergencyUnstakeCallInput {
                    principal: 10000, asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64), maturity_block: 1000,
                    leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                    secret: pallas::Base::from(42u64),
                    ephemeral_signature_secret: pallas::Base::from(12u64),
                    coverage_report: report,
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let output = EmergencyUnstakeCallOutput {
                    recipient: pallas::Base::from(10u64), asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(6u64),
                };
                let r = h.emergency_unstake_solo(input, output).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("PayInterestV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::pay_interest::PayInterestCallInput;
                let input = PayInterestCallInput {
                    bond_commitment: pallas::Base::from(99u64),
                    claim_block: 100, interest_amount: 50,
                    asset_id: pallas::Base::from(1u64),
                    payment_key: pallas::Base::from(42u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(3u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.pay_interest_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("ProveCoverageV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::prove_coverage::ProveCoverageCallInput;
                // reserve 100 over obligation 500 + 50 = 550 → 1818 bps, derived by the builder.
                let input = ProveCoverageCallInput {
                    series_asset_id: pallas::Base::from(1u64),
                    total_outstanding: 500, total_interest_obligation: 50,
                    reserve_amount: 100, report_block: 500,
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.prove_coverage_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
        ],
    }
}

