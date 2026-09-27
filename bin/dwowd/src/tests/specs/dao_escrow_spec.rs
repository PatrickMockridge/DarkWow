//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE.
//! Ten endpoints: this spec covers all of them except `PayPremiumV1`, which is deferred on a circuit
//! bug its own header records — the funding path's proof has never been made to verify, so there is no
//! honest row to write for it and `min_premium` has no reader in this fixture.
//!
//! # The lifecycle, which this spec is the first to drive at all
//!
//! `ProposeClaimV1_Lifecycle` → `VoteClaimV1_Approved` → `ExecuteClaimV1_Approved` runs the claim
//! lifecycle end to end, and it is the positive control two fixes owe (`OBL-C159`, `OBL-C160`). Before
//! them the lifecycle was **unreachable**, and in two independent ways: nothing in the crate wrote
//! `ProposalState::Approved`, so `verify_proposal_approved` could not pass for any input; and a vote's
//! approval was over the bare claim id, which every vote on a claim shares, while a MultiSig approval is
//! spend-once — so a group of *n* at threshold *t* could finalise it `floor(n/t)` times, one for the
//! usual case. The rows that asserted the old behaviour asserted a refusal (`Custom(38)`) and could not
//! assert its opposite, because there was no opposite to reach.
//!
//! They run on their own claim (`CLAIM_ID_LIFECYCLE`) because the cancellation row above moves the first
//! claim to `Cancelled`, which is terminal.
//!
//! # Governance (`OBL-C151`)
//!
//! The governance gates used to be unreachable: `endowment.multisig_group_id` was written once, to
//! zero, and nothing could set it — so `propose_claim` and `vote_claim` refused with
//! `GovernanceNotActive`, while `withdraw_v1`'s governance branch was an empty body that would have
//! failed open.
//!
//! **Order is the fixture's whole design.** Activating governance changes every gated endpoint's
//! acceptance — including `withdraw_v1`, which switches from its owner path to the group's — so the
//! rows that exercise the *inactive* path run first and the setter sits between them and the rows that
//! exercise the active one. The runner aborts at the first failing endpoint, so a setter placed first
//! would have made every earlier row unreachable and the failures unreadable.
//!
//! The approvals are cast in `setup` because the finalize child *names* them rather than re-deriving
//! them, and each is a **distinct message**: a MultiSig approval is spend-once, so one approval cannot
//! authorise two actions. These declarations and the copy below used to carry `IntentNullifier::ZERO`
//! in a fabricated `CapabilityProof`, which the type refuses — so `ProposeClaimParamsV1::decode` failed
//! and the contract's metadata arm returned the bare `vec![]` the host reports as "rejected by design",
//! which is why this contract's red read only `metadata-decode-zkp … EMPTY metadata` for a week.
//!
//! # Children, and why every row here builds them (`OBL-C154`)
//!
//! Every row used to pass `children: vec![]` and assert either a bare `Rejection` or a rejection that
//! named a *child-count* error — because no child was ever built. A child-count error is the first check
//! every one of these endpoints performs, so the checks behind it were never executed, and that is how
//! `endowment_withdraw_v1` and `treasury_spend_v1` came to read the MultiSig approval from **slot 0**
//! while the check two lines above pinned slot 0 to the payment's selector `0x04`: a child cannot carry
//! both, so their governance paths could not be built by any caller and a green run said nothing.
//!
//! So each row below supplies the children its endpoint demands and names the check it expects to fire.
//! The five endpoints this fixture used to drive — `VerifyMemberCapabilityV1`,
//! `EnableDrainProtectionV1`, `RegisterCapabilityRequirementV1`,
//! `DeactivateCapabilityRequirementV1` and the `ResolveDispute` builder — are retired from the contract,
//! and their rows, their approvals and the whole Identity fixture are removed here rather than left
//! dormant: a row for a selector the contract now answers with `InvalidFunction` proves nothing.
use dwow_contract_test_harness::harness::{
    DaoEscrowHarness, MultiSigHarness, PromissoryNoteHarness,
};
use dwow_dao_escrow_contract::model::{
    governance_message, governance_role, DaoEscrowMode,
};
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    // `Group` for `pallas::Point::identity()` — the vote's two commitment points are circuit fields the
    // `VoteClaimV2` circuit never reads, so they are passed as the identity rather than built.
    // `PrimeField` for `pallas::Base::to_repr()`, which the vote row needs to put the capability secret
    // in the params as the 32 bytes the contract feeds to `from_repr`.
    pasta_prelude::{Group, PrimeField},
    poseidon_hash, util::fp_mod_fv, Blind, MerkleNode,
    MerkleTree, Nullifier, PublicKey, SecretKey, MULTISIG_CONTRACT_ID,
    PROMISSORY_NOTE_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use std::sync::{Arc, Mutex};
use crate::tests::uniform_runner::*;
use super::helpers::{mk_ep, mk_ep_rejecting};

/// `(commitment, leaf position, merkle path, asset id, commitment blind)`. Copied shape
/// (`escrow_spec.rs`, `insurance_market_spec.rs`).
type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// The secret every note is issued under, and the secret `pn_transfer_child` spends with. They must be
/// the same value: the transfer proof rebuilds the spent leaf as `poseidon_hash([7, secret])`, so any
/// other secret yields a leaf the tree does not hold. Every working spec in this tree keeps
/// `issue_secret == 100` for exactly this reason.
const PN_ISSUE_SECRET: pallas::Base = pallas::Base::from_raw([100, 0, 0, 0]);

/// One value per spending row, because a note is not reusable: `pn_transfer_child` spends the note it is
/// given, and the note's own amount must equal the input's. Insurance_market's spec states the rule this
/// obeys — a shared note would make the second row fail on a PN double-spend and bury the diagnostic.
const PN_VALUE_WITHDRAW_OWNER: u64 = 50_000_000;
const PN_VALUE_ENDOWMENT_NO_AUTH: u64 = 25_000_000;
const PN_VALUE_TREASURY_SPEND: u64 = 10_000_000;
const PN_VALUE_ENDOWMENT_APPROVED: u64 = 20_000_000;
// No `PN_VALUE_WITHDRAW_APPROVED`: the row that spent it exercised `withdraw_v1`'s group branch, and
// that branch retired with `WITHDRAW` role 6. `withdraw` now has one path — the owner's — and one row.
/// Its own note as well: the `_NoAuthorization` row spends the endowment-withdraw note, so lending it
/// here would surface as a PN double-spend instead of as the missing approval.
const PN_VALUE_WITHDRAW_NO_APPROVAL: u64 = 30_000_000;
/// Equal to the proposal's value, because `verify_proposal_approved` requires the executed call's value
/// to match the proposal's before it looks at anything else.
const PN_VALUE_EXECUTE_CLAIM: u64 = 10_000;
/// Its own note, and deliberately the **same amount** as `PN_VALUE_EXECUTE_CLAIM`: the lifecycle rows
/// execute a proposal whose value is `PN_VALUE_EXECUTE_CLAIM`, and the first of those notes is spent by
/// the `_ProposalNotApproved` row before the proposal is ever approved. A note is not reusable, so the
/// approved execute needs one of its own — and it cannot differ in amount, because the contract checks
/// the executed call's value against the proposal's.
const PN_VALUE_EXECUTE_APPROVED: u64 = PN_VALUE_EXECUTE_CLAIM;

/// The claim the **lifecycle rows** run on, which is deliberately not `CLAIM_ID`.
///
/// `CancelClaimV1_Approved` moves `CLAIM_ID`'s proposal to `Cancelled`, which is terminal, and the
/// negative controls after it are written against that state. A vote on a cancelled proposal refuses
/// with `ClaimAlreadyCancelled`, which is a true statement about the contract and no statement at all
/// about whether a vote can ever succeed. So the propose → vote → execute rows run on a second claim,
/// against a second set of approvals.
const CLAIM_ID_LIFECYCLE: pallas::Base = pallas::Base::from_raw([200, 0, 0, 0]);

/// Build a `promissory_note::transfer_v1` (0x04) child spending an issued note.
///
/// Copied from `escrow_spec.rs:21-59` — the working example — with one deliberate change: the PN harness
/// is **passed in** rather than spawned per call. Every `spawn` rebuilds proving keys, this spec spends a
/// note in seven rows across two chains, and that wall clock is the same reason the multisig harness
/// below is leaked once.
///
/// `blind_seed` must be exactly the parent's own derivation: `dao_escrow` computes
/// `poseidon_hash([Base::from(value), dao_escrow_bulla])` and checks the child's *output* commitment
/// against `pedersen_commitment_u64(value, value_blind)`
/// (`promissory_note/src/validation.rs:46-72` scans outputs, never inputs). The helper applies
/// `fp_mod_fv` once, matching the parent.
fn pn_transfer_child(
    pn: &PromissoryNoteHarness,
    note: &PnNote,
    value: u64,
    blind_seed: pallas::Base,
) -> dwow_core::Result<ChildCall> {
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
        secret: PN_ISSUE_SECRET,
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
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
    })
}

/// The approvals the governance group cast in `setup`, per case. Each is a set of signature nullifiers,
/// captured from the `sign` calls that produced them because the finalize child *names* them.
#[derive(Default, Clone)]
struct Governance {
    /// The endowment's group on the proposal message — the approvals that must be accepted.
    propose: Vec<Nullifier>,
    /// The endowment's group on a *different* message — valid approvals of the wrong thing.
    wrong_message: Vec<Nullifier>,
    /// The endowment's group on the endowment-withdraw action — role 4 over the
    /// `(bulla, value, recipient_x)` triple its row uses. Distinct from every other message, including
    /// the other money endpoints' triples: the role tag is what keeps them apart.
    endowment_withdraw: Vec<Nullifier>,
    /// The endowment's group on the cancellation action — role 10 over the claim id. The same id the
    /// proposal's approval names, which is exactly why the role tag exists (`OBL-C151`).
    cancel_claim: Vec<Nullifier>,
    /// The endowment's group on the *second* proposal — role 1 over `CLAIM_ID_LIFECYCLE`. Distinct from
    /// `propose` because an approval is spend-once and the two name different action ids.
    propose_2: Vec<Nullifier>,
    /// The endowment's group on the vote — role 2 over
    /// `poseidon_hash([claim_id, voter_x, voter_y, direction])`, direction 0 for Yes.
    ///
    /// This is the action id `OBL-C160` fixed. It used to be the bare claim id, which every vote on a
    /// claim shared, and a MultiSig approval is spend-once — so a group of *n* at threshold *t* could
    /// finalise it `floor(n/t)` times, which for the common *t = n* case is **once**: a claim could
    /// record at most one counted vote, and the group-authorised vote this row drives was impossible.
    vote: Vec<Nullifier>,
    /// A second group's id and its approvals of the proposal — valid approvals by the wrong group.
    foreign_group: pallas::Base,
    foreign: Vec<Nullifier>,
}

/// What `setup` publishes to the rows: one note per spending row. `setup` runs twice, once per chain, so
/// this is written behind a mutex and read by the row closures.
#[derive(Default, Clone)]
struct Shared {
    withdraw_owner: Option<PnNote>,
    endowment_no_auth: Option<PnNote>,
    treasury_spend: Option<PnNote>,
    endowment_approved: Option<PnNote>,
    execute_claim: Option<PnNote>,
    withdraw_no_approval: Option<PnNote>,
    execute_approved: Option<PnNote>,
}

pub fn dao_escrow_test_spec() -> ContractTestSpec<'static> {
    let harness = Box::leak(Box::new(DaoEscrowHarness::spawn()));
    let h: &DaoEscrowHarness = harness;
    // Leaked like the contract's harness, and for a reason that shows up in the wall clock: every
    // `spawn` rebuilds the multisig contract's proving keys, and this spec calls `create_group`, `sign`
    // and `finalize` from a setup that runs twice plus several endpoints.
    let ms: &'static MultiSigHarness = Box::leak(Box::new(MultiSigHarness::spawn()));
    // Same reason, and now a bigger share of it: seven rows spend a note, each on both chains.
    let pn: &'static PromissoryNoteHarness = Box::leak(Box::new(PromissoryNoteHarness::spawn()));
    let wasm = include_bytes!("../../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
    let owner_secret = pallas::Base::from(12345u64);
    let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
    let dao_bulla = pallas::Base::from(1u64);
    let claim_id = pallas::Base::from(100u64);
    // Circuit witnesses for `ProposeClaimV2` only — `ProposeClaimParamsV1` carries neither (`OBL-C160`'s
    // class: a `capability_secret` named a secret and travelled as public call data).
    let capability_id = pallas::Base::from(999u64);
    let capability_secret = pallas::Base::from(888u64);
    let nullifier_k = pallas::Scalar::from(1u64);
    let endowment_asset_id = pallas::Base::from(42u64);
    let bulla_blind = pallas::Base::from(9999u64);
    let proposer_secret = pallas::Base::from(777u64);

    // The endowment is stored under `derive_bulla(dao_bulla, owner, asset, blind)` — `initialize_v1`
    // derives it — while every endpoint that touches the endowment looks it up by its own
    // `dao_escrow_bulla` field. So a caller passes the DERIVED value in that field, and `initialize` is
    // the only call that takes the DAO's own bulla and derives.
    let endowment_bulla = dwow_dao_escrow_contract::model::DaoEscrow::derive_bulla(
        dwow_dao_escrow_contract::model::DaoEscrowBulla(dao_bulla),
        &owner_pub,
        dwow_sdk::crypto::AssetId::from_base(endowment_asset_id),
        dwow_sdk::crypto::Blind(bulla_blind),
    )
    .inner();

    // The messages, computed with the CONTRACT'S OWN derivation so that the message the group signs and
    // the message the contract checks cannot drift — the reason `MultiSigHarness::group_id` delegates to
    // the multisig contract's `derive_group_id` rather than re-implementing it.
    //
    // No approval set is cast for `vote_claim_v1` (role 2): it has **no row here**, because a vote row
    // needs its own circuit fixture. When it gets one its message must be a *distinct* one — an approval
    // is spend-once, and `propose_claim` and `vote_claim` both key on the same `claim_id`, which is
    // exactly why the message carries a role tag in the first place, and why `vote_claim`'s action id is
    // the `(claim_id, voter_x, voter_y, direction)` quadruple rather than the bare id (`OBL-C160`).
    let msg_propose = governance_message(governance_role::PROPOSE_CLAIM, claim_id);
    let msg_wrong = governance_message(governance_role::PROPOSE_CLAIM, pallas::Base::from(9999u64));
    // The money endpoints' action ids: the contract's own `(bulla, value, recipient_x)` triple, not the
    // claim/proposal id — and not each other's, because the role tag separates them. Computed here with
    // the contract's own functions so the signed message and the checked one cannot disagree.
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let owner_x = owner_pub.xy().expect("pk not identity").0;
    let action_endowment_withdraw = governance_message(
        governance_role::ENDOWMENT_WITHDRAW,
        poseidon_hash([
            endowment_bulla,
            pallas::Base::from(PN_VALUE_ENDOWMENT_APPROVED),
            owner_x,
        ]),
    );
    // There is no `action_withdraw`: `withdraw_v1` has no group branch and therefore no governed action.
    // Its authority is the ownership proof, which is not a message a group signs.
    let action_cancel_claim = governance_message(governance_role::CANCEL_CLAIM, claim_id);
    let msg_propose_2 = governance_message(governance_role::PROPOSE_CLAIM, CLAIM_ID_LIFECYCLE);

    // The voter, and the vote's own action id. `VOTE_CLAIM`'s id is the
    // `(claim_id, voter_x, voter_y, direction)` quadruple — direction 0 for Yes, matching
    // `VoteType::Yes = 0` — and all four are load-bearing (`OBL-C160`):
    //
    //   * Without the **voter**, every vote on a claim shares one message, and since a member signs a
    //     given message once and the approval is spend-once, only `floor(n/t)` votes can ever be
    //     finalised — one, for the usual `t = n`.
    //   * Without the **direction**, one approval for "somebody votes on claim X" authorises a Yes and
    //     a No alike.
    //
    // This fixture is the first caller of that form, so it is also the first evidence that the form is
    // *buildable*: the contract computes the same quadruple with its own `poseidon_hash`, and if the two
    // derivations disagreed the call would refuse `GovernanceApprovalWrongMessage` and this row would be
    // a listener for exactly that.
    let voter_secret = pallas::Base::from(555u64);
    let voter_pub = PublicKey::from_secret(SecretKey::from_base(voter_secret));
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (voter_x, voter_y) = voter_pub.xy().expect("pk not identity");
    let action_vote = governance_message(
        governance_role::VOTE_CLAIM,
        poseidon_hash([CLAIM_ID_LIFECYCLE, voter_x, voter_y, pallas::Base::zero()]),
    );

    let gov: Arc<Mutex<Governance>> = Arc::new(Mutex::new(Governance::default()));
    let notes: Arc<Mutex<Shared>> = Arc::new(Mutex::new(Shared::default()));

    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: true,
        initialize: Some(Box::new(move || {
            // **Escrow mode, chosen here rather than imposed.** `mode` is an `InitializeParamsV1` field
            // and `initialize_apply_v1` writes what it is given; it used to be a constant `Escrow` inside
            // the contract, which made the other two variants unreachable and `treasury_spend_v1`'s mode
            // gate impossible to pass (`OBL-C154`). This fixture wants that gate to refuse — its rows draw
            // claims from the endowment, which is the `Escrow` path — so it states Escrow.
            //
            // The floor is zero: no row here pays a premium, so `min_premium` has no reader in this
            // fixture (`PayPremiumV1` is the deferred endpoint).
            let r = h.initialize(nullifier_k, dao_bulla, owner_secret, endowment_asset_id, bulla_blind, DaoEscrowMode::Escrow, 0).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
            Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
        })),
        needs_coinbase_coordination: false,
        setup: Some(Box::new({
            let gov = gov.clone();
            let notes = notes.clone();
            move |chain| {
                let ms_cid = *MULTISIG_CONTRACT_ID;
                let group_id = DaoEscrowHarness::governance_group();
                let created = ms
                    .create_group(
                        DaoEscrowHarness::GOVERNANCE_THRESHOLD,
                        DaoEscrowHarness::governance_member_commitments(),
                    )
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                // The harness derives the id with the multisig contract's own function and the contract
                // derives it again; if they ever disagreed the endowment would store a group no
                // signature could satisfy, and it would look like a contract refusal.
                assert_eq!(
                    created.group_id, group_id,
                    "the created group's id must be the one the endowment will register",
                );
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &created.call_data, vec![created.proof])?.submit(),
                )?;

                // One approval set per message, each by `GOVERNANCE_THRESHOLD` of the three members —
                // real signatures, because the multisig contract counts the threshold itself.
                let sign_for = |chain: &crate::tests::blockchain::HeavyweightPipeline,
                                    message: pallas::Base|
                 -> dwow_core::Result<Vec<Nullifier>> {
                    let mut out = Vec::new();
                    for secret in DaoEscrowHarness::GOVERNANCE_MEMBERS
                        .iter()
                        .take(DaoEscrowHarness::GOVERNANCE_THRESHOLD as usize)
                    {
                        let s = ms
                            .sign(group_id, message, *secret)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        smol::block_on(
                            chain.block()?.with_call(ms_cid, ms, &s.call_data, vec![s.proof])?.submit(),
                        )?;
                        out.push(s.nullifier);
                    }
                    Ok(out)
                };
                let propose = sign_for(chain, msg_propose)?;
                let wrong_message = sign_for(chain, msg_wrong)?;
                let endowment_withdraw = sign_for(chain, action_endowment_withdraw)?;
                let cancel_claim = sign_for(chain, action_cancel_claim)?;
                let propose_2 = sign_for(chain, msg_propose_2)?;
                let vote = sign_for(chain, action_vote)?;

                // A second group — one member, threshold one — approves the proposal. Its approval is
                // valid; what is wrong is who gave it.
                const FOREIGN_MEMBER: pallas::Base = pallas::Base::from_raw([9876, 0, 0, 0]);
                let foreign = ms
                    .create_group(1, vec![MultiSigHarness::member_commitment(FOREIGN_MEMBER)])
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &foreign.call_data, vec![foreign.proof])?.submit(),
                )?;
                let f = ms
                    .sign(foreign.group_id, msg_propose, FOREIGN_MEMBER)
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(
                    chain.block()?.with_call(ms_cid, ms, &f.call_data, vec![f.proof])?.submit(),
                )?;

                *gov.lock().unwrap() = Governance {
                    propose,
                    wrong_message,
                    endowment_withdraw,
                    cancel_claim,
                    propose_2,
                    vote,
                    foreign_group: foreign.group_id,
                    foreign: vec![f.nullifier],
                };

                // ── Promissory note: one type, one note per spending row ──
                let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
                let owner_addr = poseidon_hash([pallas::Base::from(7u64), PN_ISSUE_SECRET]);
                // The type's own amount is arbitrary; only the issued notes' amounts matter.
                let token0 = pn
                    .register_type(PN_ISSUE_SECRET, pallas::Base::from(2u64), pallas::Base::from(3u64), owner_addr, PN_VALUE_WITHDRAW_OWNER, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(6u64))
                    .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                smol::block_on(chain.block()?.with_call(pn_cid, pn, &token0.call_data, token0.token_proofs.clone())?.submit())?;
                let tid = token0.asset_id;

                // The guard leaf at 0 and the asset leaf at 1 are what `issue`'s own proof assumes; the
                // issued notes append above them, and each note's merkle path is taken from the tree
                // *after* its own append.
                let mut issued: Vec<PnNote> = Vec::new();
                let mut tree = MerkleTree::new(1);
                tree.append(MerkleNode::from_base(pallas::Base::zero()));
                tree.append(MerkleNode::from_base(token0.commitment.inner()));
                for (idx, value) in [
                    PN_VALUE_WITHDRAW_OWNER,
                    PN_VALUE_ENDOWMENT_NO_AUTH,
                    PN_VALUE_TREASURY_SPEND,
                    PN_VALUE_ENDOWMENT_APPROVED,
                    PN_VALUE_EXECUTE_CLAIM,
                    PN_VALUE_WITHDRAW_NO_APPROVAL,
                    PN_VALUE_EXECUTE_APPROVED,
                ]
                .iter()
                .enumerate()
                {
                    let n = pn
                        .issue(PN_ISSUE_SECRET, tid, owner_addr, *value, pallas::Base::zero(), pallas::Base::zero(), pallas::Base::from(8u64 + idx as u64))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    smol::block_on(chain.block()?.with_call(pn_cid, pn, &n.call_data, n.proofs.clone())?.submit())?;
                    tree.append(MerkleNode::from_base(n.commitment.inner()));
                    let mark = tree.mark().unwrap();
                    issued.push((n.commitment.inner(), u64::from(mark), tree.witness(mark, 0).expect("note witness"), tid, pallas::Base::from(8u64 + idx as u64)));
                }
                let mut n = issued.into_iter();
                let shared = Shared {
                    withdraw_owner: n.next(),
                    endowment_no_auth: n.next(),
                    treasury_spend: n.next(),
                    endowment_approved: n.next(),
                    execute_claim: n.next(),
                    withdraw_no_approval: n.next(),
                    execute_approved: n.next(),
                };

                // The Identity fixture lived here — an issuer, one credential, one capability and its
                // issuance — feeding `VerifyMemberCapabilityV1`'s possession row. That endpoint, its
                // circuit and its ZKAS namespace are retired, and no surviving endpoint of this contract
                // addresses the Identity contract at all, so the fixture went with the row rather than
                // staying as twelve chain submissions nothing reads.

                *notes.lock().unwrap() = shared;

                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            // ── Governance INACTIVE. These run first: the group id is still zero, so they exercise the
            //    paths that existed before `OBL-C151` and the setter below changes their meaning.
            //
            // **`OBL-C152`'s last instance, now repaired.** `withdraw_v1` takes its owner path, and the
            // check that decides it — `record.owner_pubkey == params.recipient_pubkey` — is between two
            // public values, so on its own it admitted anyone who knew the owner's address. What makes it
            // an authorisation is the `SetGovernanceConfigV2` proof this call now carries: the circuit
            // constrains the *payee's* coordinates to `ec_mul_base(owner_secret, NULLIFIER_K)`, so a
            // caller who passes the comparison has demonstrated knowledge of the owner's secret.
            //
            // `is_zk: true` because it is one: the runner verifies the proof against the metadata arm
            // `withdraw_get_metadata` publishes, which is the pair that must agree or every withdrawal
            // fails as an invalid proof (`OBL-C156`'s class, one endpoint over).
            mk_ep("WithdrawV1_OwnerPath", true, Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().withdraw_owner.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_WITHDRAW_OWNER, poseidon_hash([pallas::Base::from(PN_VALUE_WITHDRAW_OWNER), endowment_bulla]))?;
                    let r = h.withdraw(endowment_bulla, owner_pub, PN_VALUE_WITHDRAW_OWNER, owner_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // **The refusal that the old check could not make.** A caller who is not the owner can still
            // build a perfectly valid ownership proof — for *their own* key — and the endpoint refuses
            // because the payee it names is not this endowment's owner. `Custom(21)`,
            // `NotAuthorizedToWithdraw`.
            //
            // The row is the reader for the comparison: an endpoint whose authority had been reduced to
            // the group approval again would refuse here with a different code, and one whose proof had
            // been dropped would *accept* the stranger — because `owner_secret` is what the proof is
            // built from, and this row's caller passes their own.
            mk_ep_rejecting("WithdrawV1_PayeeIsNotTheOwner", true, &["ContractError(Custom(21))"], Box::new({
                let notes = notes.clone();
                move || {
                    let stranger_secret = pallas::Base::from(4321u64);
                    let stranger_pub = PublicKey::from_secret(SecretKey::from_base(stranger_secret));
                    let note = notes.lock().unwrap().withdraw_no_approval.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_WITHDRAW_NO_APPROVAL, poseidon_hash([pallas::Base::from(PN_VALUE_WITHDRAW_NO_APPROVAL), endowment_bulla]))?;
                    let r = h.withdraw(endowment_bulla, stranger_pub, PN_VALUE_WITHDRAW_NO_APPROVAL, stranger_secret).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // **The no-group refusal, stated once** — and the row that carries it is this one, because
            // `EnableDrainProtectionV1_NoGroup` (the row that used to state it) went with its endpoint.
            //
            // The child is valid and `require_governance_child` fails closed on a zero group, so the
            // refusal is `GovernanceNotActive` (`Custom(43)`) and nothing else: the mode gate below it
            // is not reached, and neither is any authorization this endpoint used to take from
            // `capability_proof`/`proposal_id` — both of those fields are gone from the params, and the
            // endpoint's only authority is the group whose approval the call does not carry.
            mk_ep_rejecting("EndowmentWithdrawV1_NoAuthorization", false, &["ContractError(Custom(43))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().endowment_no_auth.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_ENDOWMENT_NO_AUTH, poseidon_hash([pallas::Base::from(PN_VALUE_ENDOWMENT_NO_AUTH), endowment_bulla]))?;
                    let r = h.endowment_withdraw(endowment_bulla, claim_id, owner_pub, PN_VALUE_ENDOWMENT_NO_AUTH).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // **The mode gate, and it is now a real gate rather than a wall.** `mode` is an
            // `InitializeParamsV1` field, so the creator chooses it and all three `DaoEscrowMode`
            // variants are reachable; this row's endowment chose `Escrow` at initialize, and
            // `treasury_spend_v1` refuses a non-treasury endowment with `InvalidState` (`Custom(4)`).
            //
            // The mode check sits *before* the approval check in that handler, which is why this row
            // reaches it with no approval child at all: the mode is what refuses, not the missing group.
            // It does not prove `TreasurySpendV1` is unreachable — it is reachable, for a
            // `Treasury`/`TreasuryEndowment` endowment, which this fixture deliberately is not (its rows
            // draw claims from the endowment, the `Escrow` path).
            mk_ep_rejecting("TreasurySpendV1_ModeGate", false, &["ContractError(Custom(4))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().treasury_spend.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_TREASURY_SPEND, poseidon_hash([pallas::Base::from(PN_VALUE_TREASURY_SPEND), endowment_bulla]))?;
                    let r = h.treasury_spend(endowment_bulla, owner_pub, PN_VALUE_TREASURY_SPEND).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // **The negative control `OBL-C161` owes, and it has to run *here*.** `UpdateV1` with no group
            // used to write nothing and refuse nothing, while its `apply` recorded the owner's
            // `owner_nullifier` regardless — and that nullifier is deterministic in
            // `(owner_secret, dao_escrow_bulla)`, so one no-op update spent the only credential that can
            // ever install a group and left the endowment **permanently ungovernable**.
            //
            // The row must come before the setter, and that is the whole reason it is first: the replay
            // check sits above the `None` arm, so after a successful update the same proof is refused with
            // `OwnershipProofReplayed` (`Custom(56)`) and the `None` arm is never reached. `UpdateV1_ReplaysTheProof`
            // below passes `None` for exactly that reason and asserts `Custom(56)`.
            //
            // **The pair is the control.** This row proves the refusal; the `UpdateV1_SetGovernanceGroup`
            // row immediately after proves the nullifier was *not* spent — before the fix it would have
            // been, and that row would have failed `Custom(56)`. A value-substitution fix with no control
            // is a fix nothing distinguishes from a no-op, which is what `OBL-C161` records as owed.
            EndpointSpec {
                name: "UpdateV1_NoGroup",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(57))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update(endowment_bulla, owner_secret, owner_pub, None)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            // ── THE SETTER. Everything below runs with governance ACTIVE.
            //
            // `UpdateV1` proves ownership: the `SetGovernanceConfigV2` circuit derives `owner_pub` from
            // `owner_secret` and constrains the exposed coordinates to it, so this is not a public key
            // compared against a public key.
            EndpointSpec {
                name: "UpdateV1_SetGovernanceGroup",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update(endowment_bulla, owner_secret, owner_pub, Some(DaoEscrowHarness::governance_group()))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_Approved",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let approvals = gov.lock().unwrap().propose.clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, pallas::Base::from(10u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_propose, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // ── `OBL-C152`'s repair on the surviving endpoints: the endowment's group authorises each
            //    call. Each approval is over the action the call performs, not over the endpoint, and
            //    each row would fail if the gate were removed again.
            //
            // Three rows that stood here are removed with their endpoints — `EnableDrainProtectionV1`
            // (0x06), `RegisterCapabilityRequirementV1` (0x0a) and `DeactivateCapabilityRequirementV1`
            // (0x10). The third of them also pinned an ordering ("deactivation needs the record the row
            // above registered"); the capability registry those two addresses is gone, so there is no
            // record to order against.
            //
            // **The row that proves the `OBL-C154` fix.** Two children: the payment at slot 0, which the
            // endpoint's own check pins to selector `0x04`, and the group's approval at slot 1. Before the
            // fix the approval was read from slot 0 — the same slot the payment must occupy — so no caller
            // could build a call this endpoint would accept, and a green run could not tell.
            //
            // The `capability_proof` this row used to pass is gone from the params: it was a *path
            // selector*, tested with a bare `is_some()` whose contents nothing read.
            mk_ep("EndowmentWithdrawV1_Approved", false, Box::new({
                let gov = gov.clone();
                let notes = notes.clone();
                move || {
                    let approvals = gov.lock().unwrap().endowment_withdraw.clone();
                    let note = notes.lock().unwrap().endowment_approved.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_ENDOWMENT_APPROVED, poseidon_hash([pallas::Base::from(PN_VALUE_ENDOWMENT_APPROVED), endowment_bulla]))?;
                    let r = h.endowment_withdraw(endowment_bulla, claim_id, owner_pub, PN_VALUE_ENDOWMENT_APPROVED)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_endowment_withdraw, approvals)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![
                            child,
                            ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] },
                        ],
                        call_data: r.call_data, proofs: vec![],
                    })
                }
            })),
            // `WithdrawV1_Approved` stood here, under role 6. The endpoint's group branch — and role 6
            // with it — is removed: one `requires_proof` declaration cannot describe both a path that
            // carries a proof and one that must not, and the group's spend is `EndowmentWithdrawV1`
            // (escrow modes) and `TreasurySpendV1` (treasury modes), each of which says which mode makes
            // it legal. `withdraw` is the owner's, and its row is `WithdrawV1_OwnerPath` above.
            //
            // **The reader for `OBL-C159`/`OBL-C160`, and it is a rejection.** The call is complete and
            // well-formed — the proposal below is found by id, its value and recipient match — and it
            // refuses at the state check with `ProposalNotPending` (`Custom(38)`) because the proposal is
            // still `Pending`.
            //
            // That the state is `Pending` is now the *fixture's* doing rather than the contract's
            // impossibility: `vote_claim_v1` writes `ProposalState::Approved` or `Rejected` on a
            // successful vote, so the lifecycle does have a successful exit — this claim simply never
            // received one, because no row here votes. A vote row would be the positive control; it needs
            // `VoteClaimV2`'s witness fixture and is not this unit's.
            mk_ep_rejecting("ExecuteClaimV1_ProposalNotApproved", false, &["ContractError(Custom(38))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().execute_claim.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_EXECUTE_CLAIM, poseidon_hash([pallas::Base::from(PN_VALUE_EXECUTE_CLAIM), endowment_bulla]))?;
                    // `proposal_id` is the claim's own id: `propose_claim_v1` files the proposal under
                    // `claim_id` and `execute_claim_v1` looks it up by `proposal_id`, so a fixture that
                    // passed a distinct id would record `ProposalNotFound` and prove nothing about the
                    // state check.
                    let r = h.execute_claim(endowment_bulla, claim_id, owner_pub, PN_VALUE_EXECUTE_CLAIM).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // **`OBL-C152`'s most consequential instance, now repaired.** `cancel_claim_v1` used to refuse
            // a cancellation when `proposal.proposer_pubkey != params.proposer_pubkey` — two public values,
            // so any caller who knew the proposer's key cancelled any pending claim. The row that asserted
            // the resulting `Success` (while passing exactly that key) is what caught the repair.
            //
            // `params.proposer_pubkey` is now gone from the struct rather than merely unread, and the
            // approval is over the claim id — role 10, the same id `propose_claim`'s approval names, which
            // is why the two are distinct messages.
            mk_ep("CancelClaimV1_Approved", false, Box::new({
                let gov = gov.clone();
                move || {
                    let approvals = gov.lock().unwrap().cancel_claim.clone();
                    let r = h.cancel_claim(endowment_bulla, claim_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_cancel_claim, approvals)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                        call_data: r.call_data, proofs: vec![],
                    })
                }
            })),
            // ── THE LIFECYCLE, END TO END: propose → vote → execute.
            //
            // These three rows are the positive control `OBL-C159` owes, and they are the first callers
            // of two things this contract could not previously do.
            //
            // (1) **A vote can succeed at all.** `VoteClaimV1` is now the sole writer of
            // `ProposalState::Approved`; before it, nothing in the crate wrote that state, so
            // `verify_proposal_approved` could not pass for any input and `ExecuteClaimV1` was
            // unreachable — the row above (`ExecuteClaimV1_ProposalNotApproved`) asserted the symptom
            // (`Custom(38)`) and nothing could assert the opposite. And the vote itself was
            // unfinalisable: its approval was over the bare claim id, which every vote on a claim shares,
            // while a MultiSig approval is spend-once (`OBL-C160`).
            //
            // (2) **The execute path completes.** `verify_proposal_approved` checks the proposal's state,
            // its deadline, its bulla, its value and its recipient, and this row matches all five — so it
            // is also the reader for four checks that had no reader, because no call ever got past the
            // first one.
            //
            // On `CLAIM_ID_LIFECYCLE` rather than `CLAIM_ID` because the cancellation row above moved the
            // first claim to `Cancelled`, which is terminal; see that constant's note.
            EndpointSpec {
                name: "ProposeClaimV1_Lifecycle",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let approvals = gov.lock().unwrap().propose_2.clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, CLAIM_ID_LIFECYCLE, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_APPROVED, pallas::Base::from(50u64), owner_pub, pallas::Base::from(11u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_propose_2, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // **The vote, and the `OBL-C160` fix's positive control.** The approval is over
            // `(CLAIM_ID_LIFECYCLE, voter_x, voter_y, 0)` — the quadruple the contract recomputes with its
            // own `poseidon_hash` — and the parameters carry the *same* secret twice: as the 32 bytes
            // `CapabilityProof.capability_secret`, which the contract feeds to `from_repr` to derive the
            // tree key, and as the field element `VoteClaimV2` constrains the published instance to. If
            // those two disagreed the proof would be rejected; if the message disagreed the approval would
            // be (`Custom(54)`). Nothing else in the tree couples them, which is why the row is the check.
            //
            // `is_zk: true` because `VoteClaimV2` is a real proof — the harness builds it, so this row
            // also exercises the client's witness assembly against the contract's instance order.
            EndpointSpec {
                name: "VoteClaimV1_Approved",
                is_zk: true,
                expectation: EndpointExpectation::Success,
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let approvals = gov.lock().unwrap().vote.clone();
                        let cap_proof = dwow_dao_escrow_contract::model::CapabilityProof {
                            capability_id: [1u8; 32],
                            // The circuit's witness, as bytes. Non-canonical bytes would make the
                            // contract's `from_repr` fall back to zero — silently, since it uses
                            // `unwrap_or` — and the tree key would then be a value the proof never
                            // published. `capability_secret` is a small literal, so it is canonical.
                            capability_secret: capability_secret.to_repr(),
                            // Not zero: `IntentNullifier` refuses the all-zero encoding, and the field is
                            // unread by `vote_claim_v1` (it derives its own key), so any legal value does.
                            nullifier: dwow_sdk::crypto::IntentNullifier::from_base(pallas::Base::from(4u64)),
                            issuer_pub: [0u8; 32],
                            predicate_result: [0u8; 32],
                            proof: vec![],
                        };
                        let r = h.vote_claim(
                            nullifier_k,
                            pallas::Point::identity(),
                            pallas::Point::identity(),
                            CLAIM_ID_LIFECYCLE,
                            capability_id,
                            capability_secret,
                            voter_secret,
                            true,
                            pallas::Base::from(12u64),
                            endowment_bulla,
                            CLAIM_ID_LIFECYCLE,
                            voter_pub,
                            cap_proof,
                        ).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), action_vote, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // **The lifecycle's terminal step, and the reader for the vote above.** It succeeds only if
            // the vote moved the proposal to `Approved`, so this row is wrong in exactly the case
            // `OBL-C159` describes — a terminal state with no writer — and correct only now.
            //
            // One child and no approval: `execute_claim_v1` validates a `promissory_note::transfer_v1`
            // at slot 0 and takes its authority from the *proposal's* state, not from a fresh approval.
            // That is the difference between this endpoint and `endowment_withdraw_v1`, and the reason
            // the lifecycle exists at all.
            mk_ep("ExecuteClaimV1_Approved", false, Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().execute_approved.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_EXECUTE_APPROVED, poseidon_hash([pallas::Base::from(PN_VALUE_EXECUTE_APPROVED), endowment_bulla]))?;
                    let r = h.execute_claim(endowment_bulla, CLAIM_ID_LIFECYCLE, owner_pub, PN_VALUE_EXECUTE_APPROVED).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // `VerifyMemberCapabilityV1`'s possession row stood here. The endpoint (0x0b), its circuit
            // (`verify_member_capability.zk`) and its identity fixture are retired: a selector absent from
            // `DaoEscrowFunction` reaches `InvalidFunction`, and a row asserting that would be a row about
            // dispatch rather than about this contract's behaviour.
            // ── Negative controls, each naming the check it exercises rather than accepting any
            //    rejection. A bare `Rejection` is satisfied by an earlier failure in the frame, which is
            //    how a control that cannot fail gets written.
            EndpointSpec {
                name: "UpdateV1_ByAStranger",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(20))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    // A different secret, whose public key is therefore not the endpoint's owner. The
                    // *proof* still proves knowledge of that secret — which is the point: ownership of
                    // the value does not make you this endowment's owner.
                    let stranger_secret = pallas::Base::from(4321u64);
                    let stranger_pub = PublicKey::from_secret(SecretKey::from_base(stranger_secret));
                    let r = h.update(endowment_bulla, stranger_secret, stranger_pub, Some(DaoEscrowHarness::governance_group()))
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "UpdateV1_ReplaysTheProof",
                is_zk: true,
                // The same ownership proof twice: `owner_nullifier` is deterministic in
                // `(owner_secret, bulla)`, so the second call must be refused rather than replayed. This
                // row passes `None` for the group — no rotation is attempted — so the only check it can
                // reach is the one-shot nullifier, and `OwnershipProofReplayed` (`Custom(56)`) is
                // therefore the code that must appear.
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(56))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.update(endowment_bulla, owner_secret, owner_pub, None)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_NoChild",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, pallas::Base::from(10u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![r.proof] })
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_ForeignGroup",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(53))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let g = gov.lock().unwrap().clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, pallas::Base::from(10u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(g.foreign_group, msg_propose, g.foreign)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            EndpointSpec {
                name: "ProposeClaimV1_WrongMessage",
                is_zk: true,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(54))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let gov = gov.clone();
                    move || {
                        let approvals = gov.lock().unwrap().wrong_message.clone();
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, pallas::Base::from(10u64)).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_wrong, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // ── The readers for `OBL-C152`'s repair: a real call to a guarded endpoint carrying no
            //    approval child at all. Without them the gates would be asserted only by their positive
            //    paths, and a gate that silently admitted everyone would still pass those.
            //
            // `EnableDrainProtectionV1_NoApproval` stood here with its endpoint. What it covered — the
            // slot-0 approval read — is covered for `endowment_withdraw` and `withdraw` by the rows
            // below and above: slot 0 is checked against the payment's selector `0x04`, so an approval
            // cannot stand there.
            EndpointSpec {
                name: "CancelClaimV1_NoApproval",
                is_zk: false,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    // The proposer's own key used to be the whole authorization. It is not one now — it is
                    // not even a field of the params any more — and that is the point of the row.
                    let r = h.cancel_claim(endowment_bulla, claim_id).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
        ],
    }
}
