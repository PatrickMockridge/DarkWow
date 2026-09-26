//! ContractTestSpec for insurance_market. Tier: UNDERPOWERED — 6 harness methods.
//! 4 real proofs (underwrite, purchase_coverage ×2, create_market's siblings), 12 functions have NO
//! harness methods.
//!
//! # The capability guard, and why the two rows at the end of this file exist
//!
//! `underwrite_with_capability` (0x09) requires exactly two children: a `PN::TransferV1` paying the
//! bond, and an `Identity::VerifyCapabilityV1`. The parent then decodes the child's input and compares
//! `capability_proof.capability_id` against `market.required_underwriter_capability`, which it reads
//! from the **stored market record**.
//!
//! That comparison is the gate. Without it the parent would check only the child's *shape* — selector
//! 0x06, addressed to the configured Identity contract — and any valid capability would pass, which is
//! where `labor_market` still is. A test that asserts `Rejection` cannot see the difference: every
//! rejection is satisfied by the comparison being absent *or* by an earlier failure in the frame.
//!
//! So the two rows below are a pair, and they must be read as one instrument:
//!
//! * `UnderwriteWithCapabilityV1_CorrectCapability` — the child proves the capability the market
//!   requires, and the call must **succeed**, mutating the underwriter and market records.
//! * `UnderwriteWithCapabilityV1_WrongCapability` — the identical frame, except the child proves a
//!   *different* capability, and the call must be rejected with `Custom(29)` (`CapabilityNotMet`),
//!   asserted **by name**.
//!
//! The first is the second's control: it proves that children, selectors, contract ids, the PN
//! payment binding, the bond arithmetic and the parent's own proof are all satisfied in a byte-identical
//! frame — so `Custom(29)` cannot be a rejection for some other reason. And the negative row is the
//! guard's control: without it the comparison could be inverted with every test still green.
//!
//! **The two capabilities differ by SCHEMA, not by name.** `compute_capability_id` hashes
//! `requirement.encode()[..8] ++ name`'s first eight bytes, and `requirement` begins with
//! `schema_hash` — so two capabilities registered with the same schema are the **same capability**
//! however they are named. A "wrong capability" built from a second name would be the *same* id, the
//! guard would accept it, and the row would pass while testing nothing. `schema_a`/`schema_b` are what
//! make `cap_a != cap_b`.
//!
//! # What the earlier rows in this file got wrong, stated rather than left to mislead
//!
//! `PurchaseCoverageWithCapabilityV1` (0x0a) is still rejected at `metadata-decode-zkp`, **not**
//! because its call carries no children: the harness encodes a smaller params type into its selector
//! than the function decodes, so `get_metadata` cannot decode it and the empty vector it returns is the
//! documented rejection signal. It needs its own corrected harness method; that is owed and is named at
//! the row rather than papered over with a needle that would fail.
//!
//! `PurchaseCoverageDirectV1` (0x04) and `PurchaseCoverageWithDAGV1` (0x0b) were in that state and no
//! longer are. Two defects had kept them there, and neither was test scaffolding: the harness built both
//! proofs from `dwow_core::zk::empty_witnesses` against circuits with real witnesses, and
//! `PurchaseCoverageParamsV1::decode` demanded 160 bytes while its `encode` wrote 168 — so 0x04 could
//! not be called end to end by any client, and an earlier version of this comment claimed it already
//! named `Custom(33)`, which was false. Both are fixed, so both rows now reach the child-count check
//! and name it.
use dwow_contract_test_harness::harness::{
    IdentityHarness, InsuranceMarketHarness, PromissoryNoteHarness,
};
use dwow_identity_contract::model::{CapabilityId, CredentialRequirement};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, poseidon_hash, util::fp_mod_fv, Blind, ContractId, IntentNullifier,
    MerkleNode, MerkleTree, PublicKey, SecretKey, IDENTITY_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};

use crate::tests::uniform_runner::{
    ChildCall, ContractTestSpec, EndpointResult, EndpointSpec, EndpointExpectation,
};

/// `(commitment, leaf position, merkle path, asset id, commitment blind)`. Copied shape.
type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// Build a `promissory_note::transfer_v1` (0x04) child spending an issued note.
///
/// Copied verbatim from `escrow_spec.rs`, which is the working example — including the output whose
/// `value` and `commitment_blind` are the ones the parent re-derives, because
/// `validate_child_value_commit` checks the *output's* commitment against
/// `pedersen_commitment_u64(bond_amount, value_blind)`.
fn pn_transfer_child(note: &PnNote, value: u64, blind_seed: pallas::Base) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, asset_id, commitment_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());
    let input = TransferCallInput {
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: *commitment_blind,
        leaf_position: *pos,
        merkle_path: path.clone(),
        secret: pallas::Base::from(100u64),
        ephemeral_signature_secret: pallas::Base::from(9u64),
        tx_commitment: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };
    let output = TransferCallOutput {
        recipient: poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(200u64)]),
        recipient_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(200u64))),
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: blind_seed,
    };
    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
    })
}

/// What the two capability rows need from the fixture, published because the market id is derived
/// from the *verifying block height* and so cannot be known when the spec is built.
#[derive(Clone)]
struct Shared {
    market_id: pallas::Base,
    underwriter_id: pallas::Base,
    /// One note per row. If the artifact were stale and the guard absent, both rows would be
    /// *accepted*; a shared note would then make the second fail on a PN double-spend, burying the
    /// diagnostic these rows exist to produce.
    note_correct: PnNote,
    note_wrong: PnNote,
    cap_a: CapabilityId,
    cap_b: CapabilityId,
    schema_a: pallas::Base,
    schema_b: pallas::Base,
    credential_secret_a: pallas::Base,
    credential_secret_b: pallas::Base,
    capability_secret: pallas::Base,
    attribute_blind: pallas::Base,
    issuer_secret: pallas::Base,
}

const BOND_AMOUNT: u64 = 10_000;
const COVERAGE_LIMIT: u64 = 50_000;
const TOTAL_COVERAGE: u64 = 1_000_000;
const COVERAGE_PERIOD: u64 = 1000;
const EXPIRES_AT: u64 = 1_000_000;

pub fn insurance_market_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(InsuranceMarketHarness::spawn()));
    let h: &InsuranceMarketHarness = harness;
    let wasm = include_bytes!("../../../../../src/contract/insurance_market/dwow_insurance_market_contract.wasm");
    let pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(10u64)));
    // A distinct key from `pk`, so the underwriter is not also the credential's issuer.
    let underwriter_pk = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(11u64)));
    let shared: Arc<Mutex<Option<Shared>>> = Arc::new(Mutex::new(None));

    ContractTestSpec {
        name: "insurance_market", is_genesis: false,
        contract_id: ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: false, initialize: None,
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let shared = shared.clone();
            move |chain| {
                let cid = crate::tests::blockchain::derive_contract_id_from_name("insurance_market");
                let id_cid = *IDENTITY_CONTRACT_ID;
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;

                let issuer_secret = pallas::Base::from(10u64);
                let credential_secret_a = pallas::Base::from(20u64);
                let credential_secret_b = pallas::Base::from(21u64);
                let schema_a = pallas::Base::from(30u64);
                let schema_b = pallas::Base::from(31u64);
                let attribute_blind = pallas::Base::from(300u64);
                let capability_secret = pallas::Base::from(777u64);
                let issuer_pub = PublicKey::from_secret(SecretKey::from_base(issuer_secret));
                // Each row derives its own holder from its credential secret; only B's is needed here,
                // for the standalone control below.
                let holder_b = PublicKey::from_secret(SecretKey::from_base(credential_secret_b));

                let oh = |e: String| dwow_core::Error::Custom(e);

                // ── Identity: an issuer, two credentials (two schemas), two capabilities ──
                let id = IdentityHarness::spawn();
                let issuer = id
                    .register_issuer(issuer_pub, b"insurer".to_vec(), vec![])
                    .map_err(|e| oh(format!("register_issuer: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &issuer.call_data, vec![])?.submit())?;

                // The commitment is fixed by these parts, and `verify_capability` below must supply
                // the same ones or the proof is about a different credential.
                let cred_a = id
                    .issue_credential(issuer_secret, credential_secret_a,
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        attribute_blind, schema_a, 0, EXPIRES_AT)
                    .map_err(|e| oh(format!("issue_credential A: {e}")))?;
                let cred_b = id
                    .issue_credential(issuer_secret, credential_secret_b,
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        attribute_blind, schema_b, 0, EXPIRES_AT)
                    .map_err(|e| oh(format!("issue_credential B: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &cred_a.call_data, vec![cred_a.proof.clone()])?.submit())?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &cred_b.call_data, vec![cred_b.proof.clone()])?.submit())?;

                // Two capabilities, differing only in schema — which is what makes their ids differ.
                // A second *name* would not: the id hashes the requirement's first eight bytes, and
                // the requirement starts with the schema hash.
                let reg_a = id
                    .register_capability(b"underwriter_licence".to_vec(),
                        CredentialRequirement {
                            schema_hash: schema_a.to_repr(), issuer_pub,
                            min_threshold: 1, attribute_name: b"role".to_vec(),
                        }, None)
                    .map_err(|e| oh(format!("register_capability A: {e}")))?;
                let reg_b = id
                    .register_capability(b"underwriter_licence".to_vec(),
                        CredentialRequirement {
                            schema_hash: schema_b.to_repr(), issuer_pub,
                            min_threshold: 1, attribute_name: b"role".to_vec(),
                        }, None)
                    .map_err(|e| oh(format!("register_capability B: {e}")))?;
                let cap_a = reg_a.capability_id;
                let cap_b = reg_b.capability_id;
                if cap_a.inner() == cap_b.inner() {
                    return Err(oh("the two capabilities share an id, so this fixture cannot \
                        distinguish them — the schemas must differ".into()))
                }
                smol::block_on(chain.block()?.with_call(id_cid, &id, &reg_a.call_data, vec![])?.submit())?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &reg_b.call_data, vec![])?.submit())?;

                // Issuance. `verify_capability` loads the capability *definition* rather than this
                // record, so these are fidelity rather than a precondition — stated because a reader
                // would otherwise assume they are load-bearing.
                for (cid_cap, secret, commitment) in [
                    (cap_a, credential_secret_a, cred_a.public_inputs.commitment),
                    (cap_b, credential_secret_b, cred_b.public_inputs.commitment),
                ] {
                    let nf = IntentNullifier::from_base(poseidon_hash([
                        pallas::Base::from(1u64), secret, commitment,
                    ]));
                    let iss = id.issue_capability(cid_cap, issuer_pub, nf)
                        .map_err(|e| oh(format!("issue_capability: {e}")))?;
                    smol::block_on(chain.block()?.with_call(id_cid, &id, &iss.call_data, vec![])?.submit())?;
                }

                // ── Promissory note: one type, two notes, each worth exactly the bond ──
                // **The PN secret must be the one `pn_transfer_child` spends with, and that is 100.**
                // The invariant every working spec in this tree preserves (`escrow_spec.rs:86` sets
                // `issue_secret = 100` for exactly this reason): the transfer proof rebuilds the leaf as
                // `public_key = poseidon_hash([7, secret])`
                // (`promissory_note/src/client/transfer.rs:353`), so issuing under a different secret
                // makes the recomputed root a key in no recorded root, and `transfer_v1` rejects the
                // child with `Custom(13)` before the parent's guard is reached. Deliberately a
                // *different* variable from the identity issuer's secret above; the two are unrelated.
                let pn_issue_secret = pallas::Base::from(100u64);
                let pn = PromissoryNoteHarness::spawn();
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), pn_issue_secret]);
                let token0 = pn
                    .register_type(pn_issue_secret, pallas::Base::from(2u64), pallas::Base::from(3u64),
                        owner_addr, BOND_AMOUNT, pallas::Base::zero(), pallas::Base::zero(),
                        pallas::Base::from(6u64))
                    .map_err(|e| oh(format!("register_type: {e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let asset_id = token0.asset_id;

                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));            // guard leaf @ 0
                tree.append(MerkleNode::from_base(token0.commitment.inner()));       // token leaf @ 1
                let mark_token = tree.mark().unwrap();

                let n1 = pn
                    .issue(pn_issue_secret, asset_id, owner_addr, BOND_AMOUNT,
                        pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64))
                    .map_err(|e| oh(format!("issue: {e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, &pn, &n1.call_data, n1.proofs.clone())?.submit())?;
                tree.append(MerkleNode::from_base(n1.commitment.inner()));
                let mark_n1 = tree.mark().unwrap();

                // Both witnesses are taken after the last append. **That is tidiness, not the fix, and
                // an earlier version of this comment claimed otherwise — a run refuted it.** The
                // failure it blamed on a "stale" path was `Custom(13)`; it persisted unchanged after
                // this reordering; and mechanically it could not have helped, because
                // `commitment_roots` keeps *every* historical root, so a path captured at append time
                // still yields a root that was recorded then. The real cause was the secret above.
                let note_correct: PnNote = (
                    token0.commitment.inner(), u64::from(mark_token),
                    tree.witness(mark_token, 0).expect("witness"), asset_id, pallas::Base::from(6u64),
                );
                let note_wrong: PnNote = (
                    n1.commitment.inner(), u64::from(mark_n1),
                    tree.witness(mark_n1, 0).expect("witness"), asset_id, pallas::Base::from(8u64),
                );

                // ── The risk type and the market, the latter requiring capability A ──
                let rt_params = dwow_insurance_market_contract::model::RegisterRiskTypeParamsV1 {
                    category: dwow_insurance_market_contract::model::RiskCategory::SmartContractHack,
                    description: b"underwriter-liability".to_vec(),
                    base_premium_rate: 500,
                    min_bond_rate: 1000,
                    oracle_pubkey: issuer_pub,
                };
                let rt = h.register_risk_type(&rt_params)
                    .map_err(|e| oh(format!("register_risk_type: {e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &rt.call_data, vec![])?.submit())?;

                // `CreateMarketV1` derives the id from the *verifying* height, so take the next one
                // immediately before submitting — the height the call executes at is the one hashed.
                let mk_params = dwow_insurance_market_contract::model::CreateMarketParamsV1 {
                    risk_type_id: rt.risk_type_id,
                    initial_premium_rate: 500,
                    total_coverage: TOTAL_COVERAGE,
                    coverage_period: COVERAGE_PERIOD,
                    deductible: 0,
                    max_coverage_per_buyer: TOTAL_COVERAGE,
                    closes_at: 0,
                    required_underwriter_capability: Some(cap_a.to_bytes()),
                    required_buyer_capability: None,
                    required_dag_id: None,
                };
                let verifying_height = chain.height().succ().get();
                let market_id = poseidon_hash([
                    rt.risk_type_id,
                    pallas::Base::from(TOTAL_COVERAGE),
                    pallas::Base::from(COVERAGE_PERIOD),
                    pallas::Base::from(verifying_height),
                ]);
                let mk = h.create_market(&mk_params)
                    .map_err(|e| oh(format!("create_market: {e}")))?;
                smol::block_on(chain.block()?.with_call(cid, h, &mk.call_data, vec![])?.submit())?;

                // ── The control that makes the negative row a falsifier rather than a hope ──
                // Capability B's proof is submitted STANDALONE and required to land, so that when the
                // negative row is rejected it cannot be because B's proof was invalid or B unknown:
                // the child is provably good on-chain, and the only thing left for the parent to
                // reject is the mismatch.
                let b_check = id
                    .verify_capability(credential_secret_b, cap_b.inner(),
                        pallas::Base::from(50u64),
                        b"role", pallas::Base::from(100u64),
                        b"tenure", pallas::Base::from(200u64),
                        attribute_blind, capability_secret,
                        issuer_pub, holder_b, schema_b, 0, EXPIRES_AT, true)
                    .map_err(|e| oh(format!("verify_capability B (control): {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, &id, &b_check.call_data, vec![b_check.proof.clone()])?.submit())?;

                *shared.lock().unwrap() = Some(Shared {
                    market_id,
                    underwriter_id: dwow_insurance_market_contract::model::derive_underwriter_id(
                        market_id, &underwriter_pk, BOND_AMOUNT,
                    ),
                    note_correct, note_wrong, cap_a, cap_b, schema_a, schema_b,
                    credential_secret_a, credential_secret_b, capability_secret,
                    attribute_blind, issuer_secret,
                });
                Ok(())
            }
        })),
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
                        bond_amount: BOND_AMOUNT, coverage_limit: COVERAGE_LIMIT,
                        underwriter: pk,
                        capability_proof: vec![],
                        capability_secret: pallas::Base::from(1u64).to_repr(),
                    };
                    // Well-formed for its selector, so the rejection comes from the guard's own
                    // child-count check and not from an undecodable payload.
                    let r = h.underwrite_with_capability(&params, pallas::Base::from(10u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // ── THE NEGATIVE CASE. Identical to the positive row except in which capability the
            // child proves, so a rejection names the guard rather than the frame.
            EndpointSpec {
                name: "UnderwriteWithCapabilityV1_WrongCapability",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&[
                    // The PARENT failed, not a child. `call_idx=2` is what says so: the two children
                    // are calls 0 and 1 and the parent is submitted last, so it is always the third.
                    //
                    // **Not `fn_code=0x09`** — an earlier version of this needle used the selector and
                    // failed against a correct rejection, because `fn_code` in this message is the
                    // call-tree length prefix rather than the function selector (the same trap the
                    // attestation work records against `DelegateAttestationParamsV1::decode`). The
                    // parent's prefix is `0x03`. The needle was wrong; the guard was not.
                    "call_idx=2",
                    "ContractError(Custom(29))",     // == CapabilityNotMet
                ]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let shared = shared.clone();
                    let id = Box::leak(Box::new(IdentityHarness::spawn()));
                    move || {
                        use dwow_insurance_market_contract::model::UnderwriteWithCapabilityParamsV1;
                        let s = shared.lock().unwrap().clone().ok_or_else(|| {
                            dwow_core::Error::Custom("setup did not run".to_string())
                        })?;
                        let child_pn = pn_transfer_child(
                            &s.note_wrong, BOND_AMOUNT,
                            poseidon_hash([pallas::Base::from(BOND_AMOUNT), s.underwriter_id]),
                        )?;
                        // The child proves capability B; the market requires A.
                        let holder_b = PublicKey::from_secret(SecretKey::from_base(s.credential_secret_b));
                        let v = id
                            .verify_capability(s.credential_secret_b, s.cap_b.inner(),
                                pallas::Base::from(50u64),
                                b"role", pallas::Base::from(100u64),
                                b"tenure", pallas::Base::from(200u64),
                                s.attribute_blind, s.capability_secret,
                                PublicKey::from_secret(SecretKey::from_base(s.issuer_secret)),
                                holder_b, s.schema_b, 0, EXPIRES_AT, true)
                            .map_err(|e| dwow_core::Error::Custom(format!("verify_capability B: {e}")))?;
                        let child_id = ChildCall {
                            contract_id: *IDENTITY_CONTRACT_ID,
                            call_data: v.call_data,
                            proofs: vec![v.proof],
                        };
                        let params = UnderwriteWithCapabilityParamsV1 {
                            market_id: s.market_id,
                            bond_amount: BOND_AMOUNT,
                            coverage_limit: COVERAGE_LIMIT,
                            underwriter: underwriter_pk,
                            capability_proof: vec![],
                            capability_secret: s.capability_secret.to_repr(),
                        };
                        let r = h.underwrite_with_capability(&params, pallas::Base::from(11u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![child_pn, child_id],
                            call_data: r.call_data,
                            proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // ── THE POSITIVE CASE, and the control for the row above: the same frame with the
            // capability the market actually requires must be ACCEPTED.
            EndpointSpec {
                name: "UnderwriteWithCapabilityV1_CorrectCapability",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: Some(Box::new({
                    let shared = shared.clone();
                    move |chain: &crate::tests::blockchain::HeavyweightPipeline| {
                        let s = shared.lock().unwrap().clone()
                            .ok_or_else(|| dwow_core::Error::Custom("setup did not run".to_string()))?;
                        let cid = crate::tests::blockchain::derive_contract_id_from_name("insurance_market");
                        // Both halves of the update, because `process_update` writes both and a fix
                        // that carried only one would pass an assertion that checked one.
                        let uw_key = s.underwriter_id.to_repr();
                        let raw_uw = chain.query_contract_state(cid, "underwriters", &uw_key)?
                            .ok_or_else(|| dwow_core::Error::Custom("underwriter must be stored".to_string()))?;
                        let uw = dwow_insurance_market_contract::model::Underwriter::decode(&raw_uw)
                            .map_err(|e| dwow_core::Error::Custom(format!("underwriter decode: {e}")))?;
                        if uw.bond_amount != BOND_AMOUNT {
                            return Err(dwow_core::Error::Custom(format!(
                                "underwriter bond_amount is {} not {}", uw.bond_amount, BOND_AMOUNT)))
                        }
                        let raw_m = chain.query_contract_state(cid, "markets", &s.market_id.to_repr())?
                            .ok_or_else(|| dwow_core::Error::Custom("market must be rewritten".to_string()))?;
                        let m = dwow_insurance_market_contract::model::InsuranceMarket::decode(&raw_m)
                            .map_err(|e| dwow_core::Error::Custom(format!("market decode: {e}")))?;
                        if m.coverage_sold != COVERAGE_LIMIT {
                            return Err(dwow_core::Error::Custom(format!(
                                "market coverage_sold is {} not {}", m.coverage_sold, COVERAGE_LIMIT)))
                        }
                        Ok(())
                    }
                })),
                generate: Box::new({
                    let shared = shared.clone();
                    let id = Box::leak(Box::new(IdentityHarness::spawn()));
                    move || {
                        use dwow_insurance_market_contract::model::UnderwriteWithCapabilityParamsV1;
                        let s = shared.lock().unwrap().clone().ok_or_else(|| {
                            dwow_core::Error::Custom("setup did not run".to_string())
                        })?;
                        let child_pn = pn_transfer_child(
                            &s.note_correct, BOND_AMOUNT,
                            poseidon_hash([pallas::Base::from(BOND_AMOUNT), s.underwriter_id]),
                        )?;
                        // The child proves capability A — the one the market requires.
                        let holder_a = PublicKey::from_secret(SecretKey::from_base(s.credential_secret_a));
                        let v = id
                            .verify_capability(s.credential_secret_a, s.cap_a.inner(),
                                pallas::Base::from(50u64),
                                b"role", pallas::Base::from(100u64),
                                b"tenure", pallas::Base::from(200u64),
                                s.attribute_blind, s.capability_secret,
                                PublicKey::from_secret(SecretKey::from_base(s.issuer_secret)),
                                holder_a, s.schema_a, 0, EXPIRES_AT, true)
                            .map_err(|e| dwow_core::Error::Custom(format!("verify_capability A: {e}")))?;
                        let child_id = ChildCall {
                            contract_id: *IDENTITY_CONTRACT_ID,
                            call_data: v.call_data,
                            proofs: vec![v.proof],
                        };
                        let params = UnderwriteWithCapabilityParamsV1 {
                            market_id: s.market_id,
                            bond_amount: BOND_AMOUNT,
                            coverage_limit: COVERAGE_LIMIT,
                            underwriter: underwriter_pk,
                            capability_proof: vec![],
                            capability_secret: s.capability_secret.to_repr(),
                        };
                        let r = h.underwrite_with_capability(&params, pallas::Base::from(11u64))
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![child_pn, child_id],
                            call_data: r.call_data,
                            proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // 0x04 is the one selector whose params type the harness gets right, and
            // `purchase_coverage.rs` requires exactly one child while this carries none, so the
            // contract rejects it with `Custom(33)` — which this row now names. It used to assert
            // `Success`, i.e. it asserted a call the contract rejects.
            //
            // LAST IN THIS VEC, and the position is deliberate rather than arbitrary: its `generate`
            // cannot succeed. `purchase_coverage_v1` builds its proof from `empty_witnesses`
            // (`harness/insurance_market.rs`), while the circuit has real witnesses — it constrains
            // `computed_nullifier` to `buyer_nullifier` — so `Proof::create` fails and the endpoint
            // dies in the *generate* phase, where the runner's per-endpoint assertions never apply.
            // Its `generate` failed until two things were fixed, and both were defects rather than
            // test scaffolding: the harness built the proof from `empty_witnesses` against a circuit
            // with real witnesses, and `PurchaseCoverageParamsV1`'s `decode` demanded 160 bytes while
            // its `encode` wrote 168 — so the row could not reach exec at all, and an earlier version
            // of this comment claimed it already named `Custom(33)`. Both are now fixed, so the
            // expectation below is a real one. **Both needles are load-bearing**: the
            // `metadata-decode-zkp` text also contains `fn_code=0x04`, so `at exec` is what separates
            // exec from the metadata stage, and `Custom(33)` is emitted by nothing else in this
            // contract.
            //
            // `buyer_nullifier` is set to zero here and is NOT read: the harness derives it from the
            // circuit's preimage and overwrites the field. `buyer_secret` must be the discrete log of
            // `pk`, which it is — `pk = PublicKey::from_secret(SecretKey::from_base(10))`.
            EndpointSpec {
                name: "PurchaseCoverageDirectV1",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&[
                    "at exec",
                    "ContractError(Custom(33))",
                ]),
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
                        buyer_nullifier: pallas::Base::zero(),
                    };
                    let r = h.purchase_coverage_v1(
                        &params, pallas::Base::from(10u64), pallas::Base::zero(),
                    ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // `PurchaseCoverageWithCapabilityV1` (0x0a) is still rejected for the WRONG reason, and that
            // is owed rather than intended: the harness's `purchase_coverage` encodes
            // `PurchaseCoverageParamsV1` (168 B) into a selector that decodes
            // `PurchaseCoverageWithCapabilityParamsV1` (>= 204 B) — measured, the contract's own `msg!`
            // says `Failed to decode PurchaseCoverageWithCapabilityParamsV1: IoError("… too short")` —
            // so the cause is `metadata-decode-zkp` and NOT the omitted children. No needle here on
            // purpose: the honest one names that stage, and would stop being true the moment the harness
            // method is corrected. Owed: `h.purchase_coverage_with_capability`, which is the same recipe
            // `purchase_coverage` and `purchase_coverage_dag` have just been given.
            //
            // Its position at the end of the vec no longer matters: the two rows that could not
            // `generate` now can, so the runner's abort-on-first-error masks nothing.
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
            // Not DAG-gated here — what this row exercises is the DAG function's own child-count
            // check. It needs one payment child and carries none, so it is rejected with `Custom(33)`,
            // and it can only reach that check because the params type is now the right one.
            EndpointSpec {
                name: "PurchaseCoverageWithDAGV1",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&[
                    "at exec",
                    "ContractError(Custom(33))",
                ]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    use dwow_insurance_market_contract::model::PurchaseCoverageWithDAGParamsV1;
                    let params = PurchaseCoverageWithDAGParamsV1 {
                        market_id: pallas::Base::from(1u64),
                        underwriter_id: pallas::Base::from(1u64),
                        buyer: pk,
                        coverage_amount: 5000,
                        value_commit: pallas::Point::default(),
                        // Not read; the harness derives it and overwrites the field.
                        buyer_nullifier: pallas::Base::zero(),
                        dag_proof: vec![],
                        dag_path_index: 0,
                        required_dag_id: [0u8; 32],
                    };
                    let r = h.purchase_coverage_dag(&params, pallas::Base::from(10u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
        ],
    }
}
