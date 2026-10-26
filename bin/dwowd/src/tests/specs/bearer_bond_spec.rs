//! ContractTestSpec for bearer_bond — refusal controls after the authority-model disable.
//!
//! `IssueStakeV1`, `RequestInterestV1` and `ProveCoverageV1` are refused before any host call or
//! state read (`AuthorityModelUnavailable`, `Custom(35)`) at
//! `src/contract/bearer_bond/src/entrypoint/mod.rs:597-607`. The host unit control is
//! `disabled_authority_arms_are_refused_before_any_state_read` in the contract crate.
//!
//! The former success flow (issue → burn/transfer/request/unstake/emergency/pay) is unreachable
//! through the chain because no bond can be created. The consuming arms are kept **only** as
//! negative twins that reach their own state check and name their own error; a fabricated success
//! would be a false green, and a `Custom(35)` here would test the wrong gate.
//!
//! The failing-before evidence for this rewrite: the old spec failed at block 3,
//! `fn_code=0x01`, `ContractError(Custom(35))` (`/tmp/heavy-bearer-before.out`, 734.59s).
//!
//! **A refusal row must carry the arm's real params, or it tests the wrong gate.** The first
//! version of this rewrite used a selector-only call (the shape `stablecoin_spec.rs` uses for its
//! dispatch pre-check). Here that is refused earlier: `get_metadata` decodes the arm's params, a
//! one-byte call fails that decode, and the host rejects the call as `metadata-decode-zkp` /
//! `EMPTY metadata` before `exec` is ever entered — so the run named the metadata gate, not
//! `Custom(35)` (`/tmp/p2-bearer.out`, 84.34s). The params are therefore built by each arm's own
//! `prepare` (no proving), which is what makes the metadata arm answer and leaves the exec
//! pre-check as the first refusal.
//!
//! Proofs are empty on these rows because the host verifies them only after every call in the
//! transaction has executed (`src/linear/src/execution.rs` — `verify_core_tx_with_tables` runs
//! post-loop); a call refused at `exec` never reaches verification. The negative twins below need
//! real proofs for the same reason: they must survive to `apply` to reach their state check.
//!
//! The ZK inputs carry a well-formed but arbitrary Merkle path: the circuits fold it into
//! `merkle_root` and instance it, but no arm of this contract compares that root to anything, so
//! the proof must be *satisfiable*, not *anchored*.
use dwow_contract_test_harness::harness::BearerBondHarness;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::crypto::{ContractId, MerkleNode};
use dwow_sdk::pasta::pallas;
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting_naming};

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
            // First and still enabled: it creates the `BondSeriesInfo` record the consuming arms
            // read. Plaintext, so no proof.
            mk_ep("RegisterSeriesV1", false, Box::new(move || {
                let r = h.register_series(
                    pallas::Base::from(1u64),
                    500u64,
                    100_000u64,
                    ContractId::from_bytes([1u8; 32]).unwrap(),
                ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
            // The three issuer-authority arms, refused at the dispatch pre-check. The params are
            // real (built by each arm's `prepare`) so the metadata gate answers and `exec` is the
            // first refusal; a selector-only call dies earlier at `metadata-decode-zkp`.
            mk_ep_rejecting_naming(
                "IssueStakeV1_refused", false, &["ContractError(Custom(35))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::issue_stake::IssueStakeCallInput;
                    let input = IssueStakeCallInput {
                        principal: 10_000, maturity_block: 1_000, min_claim: 1,
                        issuer_contract: ContractId::from_bytes([1u8; 32]).unwrap(),
                        asset_id: pallas::Base::from(1u64), staker: pallas::Base::from(42u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(3u64),
                        tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                    };
                    let plan = h.issue_stake_prepare(input)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let mut call_data = vec![0x00u8];
                    call_data.extend_from_slice(&plan.params().encode());
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }),
            ),
            mk_ep_rejecting_naming(
                "RequestInterestV1_refused", false, &["ContractError(Custom(35))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::request_interest::RequestInterestCallInput;
                    let input = RequestInterestCallInput {
                        principal: 10_000, asset_id: pallas::Base::from(1u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(3u64), last_claim_block: 10,
                        maturity_block: 1_000, claim_block: 20, min_claim: 1, leaf_position: 0,
                        merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                        secret: pallas::Base::from(42u64),
                        payment_key: pallas::Base::from(11u64),
                        tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                    };
                    let plan = h.request_interest_prepare(input)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let mut call_data = vec![0x02u8];
                    call_data.extend_from_slice(&plan.params().encode());
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }),
            ),
            mk_ep_rejecting_naming(
                "ProveCoverageV1_refused", false, &["ContractError(Custom(35))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::prove_coverage::ProveCoverageCallInput;
                    let input = ProveCoverageCallInput {
                        series_asset_id: pallas::Base::from(1u64),
                        total_outstanding: 500, total_interest_obligation: 50,
                        reserve_amount: 100, report_block: 10,
                        tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                    };
                    let plan = h.prove_coverage_prepare(input)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let mut call_data = vec![0x06u8];
                    call_data.extend_from_slice(&plan.params().encode()
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }),
            ),
            // The read-only arm is enabled; with no report it must answer `CoverageNotVerified`
            // (code 24), never the authority refusal.
            mk_ep_rejecting_naming(
                "VerifyCoverageV1_no_report", false, &["ContractError(Custom(24))"],
                Box::new(|| {
                    let mut call_data = vec![0x07u8];
                    call_data.extend_from_slice(&pallas::Base::from(1u64).to_repr());
                    Ok(EndpointResult { children: vec![], call_data, proofs: vec![] })
                }),
            ),
            // Consuming arms without an issued bond: each reaches its own state check and names
            // its own error. These prove the enabled paths fail closed on missing state.
            mk_ep_rejecting_naming(
                "BurnStakeV1_without_issue", true, &["ContractError(Custom(1))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::burn_stake::BurnStakeCallInput;
                    let input = BurnStakeCallInput {
                        principal: 10000, asset_id: pallas::Base::from(1u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(3u64), maturity_block: 1000,
                        leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                        secret: pallas::Base::from(42u64),
                        tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                    };
                    let r = h.burn_stake_solo(vec![input]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
                }),
            ),
            mk_ep_rejecting_naming(
                "TransferStakeV1_without_issue", true, &["ContractError(Custom(1))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::transfer_stake::{TransferStakeCallInput, TransferStakeCallOutput};
                    let input = TransferStakeCallInput {
                        principal: 10000, asset_id: pallas::Base::from(1u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(4u64), last_claim_block: 10,
                        maturity_block: 1000, leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                        secret: pallas::Base::from(42u64),
                        issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                        tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                    };
                    let output = TransferStakeCallOutput {
                        recipient: pallas::Base::from(10u64), principal: 10000,
                        asset_id: pallas::Base::from(1u64), spend_hook: pallas::Base::zero(),
                        user_data: pallas::Base::zero(), commitment_blind: pallas::Base::from(6u64),
                        last_claim_block: 10, maturity_block: 1000,
                        issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                    };
                    let r = h.transfer_stake_solo(vec![input], vec![output]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
                }),
            ),
            mk_ep_rejecting_naming(
                "UnstakeV1_without_issue", true, &["ContractError(Custom(1))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::unstake::{UnstakeCallInput, UnstakeCallOutput};
                    let input = UnstakeCallInput {
                        principal: 10000, asset_id: pallas::Base::from(1u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(6u64), maturity_block: 1000,
                        leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                        secret: pallas::Base::from(42u64),
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
                }),
            ),
            // The coverage gate runs *before* the stake lookup (`entrypoint/mod.rs:1032`), so an
            // adequate report names `EmergencyUnstakeNotAllowed` with no bond at all. A voided
            // report reaches the lookup instead and answers `StakeNotFound` — measured 2026-10-26,
            // the first version of this row expected 29 from a voided report and got 1.
            mk_ep_rejecting_naming(
                "EmergencyUnstakeV1_coverage_adequate", true, &["ContractError(Custom(29))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::emergency_unstake::{EmergencyUnstakeCallInput, EmergencyUnstakeCallOutput};
                    use dwow_bearer_bond_contract::model::CoverageReport;
                    let report = CoverageReport {
                        series_asset_id: pallas::Base::from(1u64),
                        total_outstanding: 500, total_interest_obligation: 50,
                        reserve_amount: 10_000, coverage_ratio_bps: 181_818, report_block: 10,
                    };
                    let input = EmergencyUnstakeCallInput {
                        principal: 10000, asset_id: pallas::Base::from(1u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(7u64), maturity_block: 1000,
                        leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                        secret: pallas::Base::from(42u64),
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
                }),
            ),
            mk_ep_rejecting_naming(
                "PayInterestV1_without_claim", true, &["ContractError(Custom(30))"],
                Box::new(move || {
                    use dwow_bearer_bond_contract::client::pay_interest::PayInterestCallInput;
                    let input = PayInterestCallInput {
                        bond_commitment: pallas::Base::from(1u64),
                        claim_block: 100, interest_amount: 50,
                        asset_id: pallas::Base::from(1u64),
                        payment_key: pallas::Base::from(42u64),
                        spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                        commitment_blind: pallas::Base::from(3u64),
                        tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                    };
                    let r = h.pay_interest_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
                }),
            ),
        ],
    }
}
