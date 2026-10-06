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

    // One `IssueStakeV1` row per consuming endpoint — see the note inside `endpoints`. A closure
    // rather than five near-identical copies: `h` is a shared reference and therefore `Copy`, and
    // the only thing that varies is the blind that makes each minted bond distinct.
    // `OBL-C199`: a bond's note commitment is derived, not a constant, and later rows must **name**
    // it rather than recompute it — `pay_interest_v1` looks a claim up by (`bond_commitment`,
    // `claim_block`) and the claim was written under the commitment `issue_row` minted. Keyed by
    // blind because the issue rows after 5 run before `PayInterestV1` and a single cell would be
    // overwritten. A hardcoded value here answers `ClaimNotFound`.
    let issued: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u64, pallas::Base>>> =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let issued_for_pay = issued.clone();

    let issue_row = move |blind: u64| {
        let issued = issued.clone();
        mk_ep("IssueStakeV1", true, Box::new(move || {
            use dwow_bearer_bond_contract::client::issue_stake::IssueStakeCallInput;
            let input = IssueStakeCallInput {
                principal: 10000, maturity_block: 1000, min_claim: 1,
                issuer_contract: ContractId::from_bytes([1u8; 32]).unwrap(),
                asset_id: pallas::Base::from(1u64),
                // The holder's key as every *spending* client computes it: `issue_stake` takes
                // `staker` literally while the burn family derives `poseidon_hash([7, secret])`, so
                // a different constant here mints a bond whose note commitment nothing can
                // reproduce — and that commitment is the bond's identity.
                staker: dwow_sdk::crypto::poseidon_hash([
                    pallas::Base::from(7u64), pallas::Base::from(42u64),
                ]),
                spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                commitment_blind: pallas::Base::from(blind),
                tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
            };
            let r = h.issue_stake_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
            issued.lock().unwrap_or_else(|e| e.into_inner()).insert(blind, r.commitment);
            Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
        }))
    };

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
            // `OBL-C199`: **every consuming endpoint needs its own bond.** One `IssueStakeV1` mints
            // one commitment, and a nullifier can be spent once — so a spec that issued a single
            // bond and then burned, transferred, requested against, unstaked and emergency-unstaked
            // it in turn fails at the second with `DuplicateNullifier` (code 13). `commitment_blind`
            // is what distinguishes one bond from another here, so the same constant in an issuing
            // row and its consuming row is what makes them the same bond.
            issue_row(3),
            mk_ep("BurnStakeV1", true, Box::new(move || {
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
            })),
            issue_row(4),
            mk_ep("TransferStakeV1", true, Box::new(move || {
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
                    // `transfer_stake_v1` checks `Σ inputs.value_commit == Σ outputs.value_commit`
                    // as a *point* equality, so the output must carry the input's principal — a
                    // fixture with `500` against a `10000` input is a value mismatch by
                    // construction. The blind half is the client's: see the last-output rule in
                    // `transfer_stake.rs`.
                    recipient: pallas::Base::from(10u64), principal: 10000,
                    asset_id: pallas::Base::from(1u64), spend_hook: pallas::Base::zero(),
                    user_data: pallas::Base::zero(), commitment_blind: pallas::Base::from(6u64),
                    last_claim_block: 10, maturity_block: 1000,
                    issuer_contract: ContractId::from_bytes([1u8;32]).unwrap(),
                };
                let r = h.transfer_stake_solo(vec![input], vec![output]).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            issue_row(5),
            mk_ep("RequestInterestV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::request_interest::RequestInterestCallInput;
                let input = RequestInterestCallInput {
                    principal: 10000, asset_id: pallas::Base::from(1u64),
                    spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
                    commitment_blind: pallas::Base::from(5u64), last_claim_block: 10,
                    // `min_claim: 0` because the accrued interest over a 90-block fixture window is
                    // zero at 500 bp/year — `interest < min_claim` is reported as `InterestOverflow`
                    // (code 9), which is a second defect this fixture does not fix: a claim below
                    // the dust floor is not an overflow.
                    maturity_block: 1000, claim_block: 100, min_claim: 0,
                    leaf_position: 0, merkle_path: vec![MerkleNode::new(pallas::Base::from(0u64)); 32],
                    secret: pallas::Base::from(42u64),
                    payment_key: pallas::Base::from(42u64),
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.request_interest_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            issue_row(6),
            mk_ep("UnstakeV1", true, Box::new(move || {
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
            })),
            issue_row(7),
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
            })),
            // **Before `PayInterestV1`, and it has to be.** `pay_interest_v1` is ringfenced: it
            // refuses a claim against a series with no verified coverage (`CoverageNotVerified`,
            // code 24). The row sat after the payer, so the report did not exist yet.
            mk_ep("ProveCoverageV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::prove_coverage::ProveCoverageCallInput;
                // reserve 1000 over obligation 500 + 50 = 550 → 18181 bps, derived by the builder.
                //
                // It was `reserve_amount: 100` — 1818 bps — and `prove_coverage_v1` **voids** a
                // series whose ratio is below 10000 (`is_coverage_voided` is `<`, so exactly
                // 10000 is adequate). So the report the row made voided the very series the next
                // row then paid a claim against, and `PayInterestV1` answered `SeriesVoided` (21).
                // Under-collateralisation is a real state and `EmergencyUnstakeV1` exists for it,
                // but it cannot be the state this row leaves the series in if anything after it is
                // to work.
                let input = ProveCoverageCallInput {
                    series_asset_id: pallas::Base::from(1u64),
                    total_outstanding: 500, total_interest_obligation: 50,
                    reserve_amount: 1000, report_block: 500,
                    tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
                };
                let r = h.prove_coverage_solo(input).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: r.proofs })
            })),
            mk_ep("PayInterestV1", true, Box::new(move || {
                use dwow_bearer_bond_contract::client::pay_interest::PayInterestCallInput;
                let input = PayInterestCallInput {
                    // The bond `RequestInterestV1` declared its claim against — `issue_row(5)`'s,
                    // read from the cell that row wrote. `claim_block` matches the claim's too
                    // (both `100`): the exec looks the record up by the **pair**, so either value
                    // being wrong answers `ClaimNotFound` (code 30).
                    bond_commitment: issued_for_pay
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get(&5)
                        .copied()
                        .expect("issue_row(5) must run before PayInterestV1 and record its bond"),
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
        ],
    }
}

