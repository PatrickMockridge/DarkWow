//! ContractTestSpec for dao_escrow. Tier: HARVESTABLE — 13 harness methods.
//! 12 endpoints active, 1 deferred (pay_premium: circuit bug).
//!
//! # Governance (`OBL-C151`)
//!
//! The four governance gates used to be unreachable: `endowment.multisig_group_id` was written once, to
//! zero, and nothing could set it — so `propose_claim`, `vote_claim`, `resolve_dispute` and the
//! capability path of the two spend endpoints all refused with `GovernanceNotActive`, while
//! `withdraw_v1`'s governance branch was an empty body that would have failed open.
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
//! So each row below now supplies the children its endpoint demands and names the check it expects to
//! fire. Two of them assert **Success over a missing authorization** — `RegisterCapabilityRequirementV1`
//! and `CancelClaimV1` — and that is deliberate: they pin what the contract does today, with the defect
//! and its register row named in the comment, so that adding a gate later fails these rows loudly
//! instead of silently changing what they mean. The `finality-widget` campaign's rule.
use dwow_contract_test_harness::harness::{
    DaoEscrowHarness, IdentityHarness, MultiSigHarness, PromissoryNoteHarness,
};
use dwow_dao_escrow_contract::model::{
    governance_message, governance_role, CapabilityProof, ClaimType,
};
use dwow_identity_contract::model::CredentialRequirement;
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::crypto::{
    pasta_prelude::PrimeField, poseidon_hash, util::fp_mod_fv, Blind, IntentNullifier, MerkleNode,
    MerkleTree, Nullifier, PublicKey, SecretKey, IDENTITY_CONTRACT_ID, MULTISIG_CONTRACT_ID,
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
const PN_VALUE_WRONG_SELECTOR: u64 = 1_000;
const PN_VALUE_WITHDRAW_OWNER: u64 = 50_000_000;
const PN_VALUE_ENDOWMENT_NO_AUTH: u64 = 25_000_000;
const PN_VALUE_TREASURY_SPEND: u64 = 10_000_000;
const PN_VALUE_ENDOWMENT_APPROVED: u64 = 20_000_000;
const PN_VALUE_WITHDRAW_APPROVED: u64 = 40_000_000;
/// Its own note as well: the `_NoAuthorization` row spends the endowment-withdraw note, so lending it
/// here would surface as a PN double-spend instead of as the missing approval.
const PN_VALUE_WITHDRAW_NO_APPROVAL: u64 = 30_000_000;
/// Equal to the proposal's value, because `verify_proposal_approved` requires the executed call's value
/// to match the proposal's before it looks at anything else.
const PN_VALUE_EXECUTE_CLAIM: u64 = 10_000;

/// The drain-protection instance the `0x06` row associates. Its value is arbitrary — the contract
/// neither contacts the DrainProtection contract nor checks that this bulla names anything — but it is a
/// *field*, and the action id binds it, so the approval is over the association rather than over the
/// endpoint.
const DRAIN_PROTECTION_BULLA: pallas::Base = pallas::Base::from_raw([4242, 0, 0, 0]);

/// The capability id the `0x0a` row registers. Zero is a legal field element and a legal capability id
/// here — nothing in this contract validates it against the Identity contract — and the fixture picks it
/// because the field is otherwise opaque to this contract.
const REGISTERED_CAPABILITY_ID: [u8; 32] = [0u8; 32];

/// The identity fixture's parts, matching `insurance_market_spec.rs`'s in shape and nothing else.
const EXPIRES_AT: u64 = 1_000_000;
const ATTR_BLIND: u64 = 300;
const CAPABILITY_SECRET: u64 = 777;
const CREDENTIAL_SECRET: u64 = 20;
const ISSUER_SECRET: u64 = 10;
const SCHEMA_HASH: u64 = 30;
const THRESHOLD: u64 = 50;
const ATTR_ROLE: u64 = 100;
const ATTR_TENURE: u64 = 200;

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
    /// the other two money endpoints' triples: the role tag is what keeps them apart.
    endowment_withdraw: Vec<Nullifier>,
    /// The endowment's group on the withdraw action — role 6.
    withdraw: Vec<Nullifier>,
    /// The four endpoints `OBL-C152` records as having no authorization at all, one approval set each.
    /// Four and not one: an approval is spend-once and each of these is a distinct role tag over a
    /// distinct action id, so a single set could not authorise two of them even if the roles matched.
    enable_drain_protection: Vec<Nullifier>,
    register_capability: Vec<Nullifier>,
    deactivate_capability: Vec<Nullifier>,
    cancel_claim: Vec<Nullifier>,
    /// A second group's id and its approvals of the proposal — valid approvals by the wrong group.
    foreign_group: pallas::Base,
    foreign: Vec<Nullifier>,
}

/// What `setup` publishes to the rows: one note per spending row, plus the capability id the identity
/// registration produced. `setup` runs twice, once per chain, so this is written behind a mutex and read
/// by the row closures. The capability id cannot be known when the spec is built — it is derived on
/// chain from the registered requirement — which is the same reason `insurance_market` publishes its
/// `market_id` this way.
#[derive(Default, Clone)]
struct Shared {
    wrong_selector: Option<PnNote>,
    withdraw_owner: Option<PnNote>,
    endowment_no_auth: Option<PnNote>,
    treasury_spend: Option<PnNote>,
    endowment_approved: Option<PnNote>,
    withdraw_approved: Option<PnNote>,
    execute_claim: Option<PnNote>,
    withdraw_no_approval: Option<PnNote>,
    /// The identity capability `setup` registered. The possession row proves this one; passing any
    /// other id would produce a proof about a capability the Identity contract has no requirement for.
    identity_capability_id: Option<pallas::Base>,
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
    let id: &'static IdentityHarness = Box::leak(Box::new(IdentityHarness::spawn()));
    let wasm = include_bytes!("../../../../../src/contract/dao_escrow/dwow_dao_escrow_contract.wasm");
    let owner_secret = pallas::Base::from(12345u64);
    let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
    let dao_bulla = pallas::Base::from(1u64);
    let claim_id = pallas::Base::from(100u64);
    let proposal_id = pallas::Base::from(200u64);
    let capability_id = pallas::Base::from(999u64);
    let identity_contract_bulla = pallas::Base::from(300u64);
    let nullifier_k = pallas::Scalar::from(1u64);
    let endowment_asset_id = pallas::Base::from(42u64);
    let bulla_blind = pallas::Base::from(9999u64);
    let proposer_secret = pallas::Base::from(777u64);
    let proposer_pub = PublicKey::from_secret(SecretKey::from_base(proposer_secret));
    let holder_secret = pallas::Base::from(111u64);
    let holder_pub = PublicKey::from_secret(SecretKey::from_base(holder_secret));
    let capability_secret = pallas::Base::from(888u64);
    let cp_id = capability_id.to_repr();
    let cp_secret = capability_secret.to_repr();

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
    // Two approval sets this fixture used to cast are gone, and the reason is the rule rather than
    // tidiness: `vote_claim_v1` (role 2) and `resolve_dispute_v1` (role 3) have **no row here**, so the
    // signatures were submitted and dropped — twelve chain submissions per run that nothing read. When
    // either endpoint gets a row, its message must be a *distinct* one: an approval is spend-once, and
    // `propose_claim` and `vote_claim` both key on the same `claim_id`, which is exactly why the message
    // carries a role tag in the first place.
    let msg_propose = governance_message(governance_role::PROPOSE_CLAIM, claim_id);
    let msg_wrong = governance_message(governance_role::PROPOSE_CLAIM, pallas::Base::from(9999u64));
    // The two money endpoints' action ids: the contract's own `(bulla, value, recipient_x)` triple, not
    // the claim/proposal id — and not each other's, because the role tag separates them. Computed here
    // with the contract's own functions so the signed message and the checked one cannot disagree.
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
    let action_withdraw = governance_message(
        governance_role::WITHDRAW,
        poseidon_hash([endowment_bulla, pallas::Base::from(PN_VALUE_WITHDRAW_APPROVED), owner_x]),
    );
    // The four actions `OBL-C152`'s repair gates. Each with the contract's own `governance_message` and
    // the contract's own action derivation, so the signed message and the checked one cannot drift.
    let action_enable_dp = governance_message(
        governance_role::ENABLE_DRAIN_PROTECTION,
        poseidon_hash([endowment_bulla, DRAIN_PROTECTION_BULLA]),
    );
    // `0x0a` binds the capability id it registers and `0x10` the capability id of the record it
    // deactivates; the fixture registers `[0u8; 32]`, whose field conversion is zero, so both action ids
    // are `poseidon_hash([bulla, 0])` — and they are still distinct *messages*, which is the role tag
    // doing its work rather than the id.
    let registered_cap_action_id =
        poseidon_hash([endowment_bulla, pallas::Base::from_repr(REGISTERED_CAPABILITY_ID).into_option().unwrap_or(pallas::Base::zero())]);
    let action_register_cap = governance_message(
        governance_role::REGISTER_CAPABILITY_REQUIREMENT, registered_cap_action_id,
    );
    let action_deactivate_cap = governance_message(
        governance_role::DEACTIVATE_CAPABILITY_REQUIREMENT, registered_cap_action_id,
    );
    let action_cancel_claim = governance_message(governance_role::CANCEL_CLAIM, claim_id);

    let gov: Arc<Mutex<Governance>> = Arc::new(Mutex::new(Governance::default()));
    let notes: Arc<Mutex<Shared>> = Arc::new(Mutex::new(Shared::default()));

    ContractTestSpec {
        name: "dao_escrow", is_genesis: false,
        contract_id: dwow_sdk::crypto::ContractId::from_bytes([0u8; 32]).expect("temp"),
        harness: h, wasm_bytes: Some(wasm),
        has_initialize: true,
        initialize: Some(Box::new(move || {
            let r = h.initialize(nullifier_k, dao_bulla, owner_secret, endowment_asset_id, bulla_blind).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
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
                let withdraw = sign_for(chain, action_withdraw)?;
                let enable_drain_protection = sign_for(chain, action_enable_dp)?;
                let register_capability = sign_for(chain, action_register_cap)?;
                let deactivate_capability = sign_for(chain, action_deactivate_cap)?;
                let cancel_claim = sign_for(chain, action_cancel_claim)?;

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
                    withdraw,
                    enable_drain_protection,
                    register_capability,
                    deactivate_capability,
                    cancel_claim,
                    foreign_group: foreign.group_id,
                    foreign: vec![f.nullifier],
                };

                // ── Promissory note: one type, seven notes, one per spending row ──
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
                    PN_VALUE_WRONG_SELECTOR,
                    PN_VALUE_WITHDRAW_OWNER,
                    PN_VALUE_ENDOWMENT_NO_AUTH,
                    PN_VALUE_TREASURY_SPEND,
                    PN_VALUE_ENDOWMENT_APPROVED,
                    PN_VALUE_WITHDRAW_APPROVED,
                    PN_VALUE_EXECUTE_CLAIM,
                    PN_VALUE_WITHDRAW_NO_APPROVAL,
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
                let mut shared = Shared {
                    wrong_selector: n.next(),
                    withdraw_owner: n.next(),
                    endowment_no_auth: n.next(),
                    treasury_spend: n.next(),
                    endowment_approved: n.next(),
                    withdraw_approved: n.next(),
                    execute_claim: n.next(),
                    withdraw_no_approval: n.next(),
                    identity_capability_id: None,
                };

                // ── Identity: an issuer, one credential, one capability ──
                // The possession fixture `OBL-C154` owed, ported from `insurance_market_spec.rs`. Only
                // the capability *registration* is a precondition — `verify_capability` loads the
                // capability definition rather than the issuance record — and it is done here rather
                // than in the row because a row runs twice, once per chain.
                let id_cid = *IDENTITY_CONTRACT_ID;
                let issuer_pub = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(ISSUER_SECRET)));
                let issuer = id
                    .register_issuer(issuer_pub, b"dao_escrow_members".to_vec(), vec![])
                    .map_err(|e| dwow_core::Error::Custom(format!("register_issuer: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, id, &issuer.call_data, vec![])?.submit())?;

                let cred = id
                    .issue_credential(
                        pallas::Base::from(ISSUER_SECRET), pallas::Base::from(CREDENTIAL_SECRET),
                        b"role", pallas::Base::from(ATTR_ROLE),
                        b"tenure", pallas::Base::from(ATTR_TENURE),
                        pallas::Base::from(ATTR_BLIND), pallas::Base::from(SCHEMA_HASH), 0, EXPIRES_AT)
                    .map_err(|e| dwow_core::Error::Custom(format!("issue_credential: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, id, &cred.call_data, vec![cred.proof.clone()])?.submit())?;

                let reg = id
                    .register_capability(b"member_vote".to_vec(),
                        CredentialRequirement {
                            schema_hash: pallas::Base::from(SCHEMA_HASH).to_repr(), issuer_pub,
                            min_threshold: 1, attribute_name: b"role".to_vec(),
                        }, None)
                    .map_err(|e| dwow_core::Error::Custom(format!("register_capability: {e}")))?;
                let identity_capability_id = reg.capability_id;
                smol::block_on(chain.block()?.with_call(id_cid, id, &reg.call_data, vec![])?.submit())?;

                let nf = IntentNullifier::from_base(poseidon_hash([
                    pallas::Base::from(1u64),
                    pallas::Base::from(CREDENTIAL_SECRET),
                    cred.public_inputs.commitment,
                ]));
                let iss = id.issue_capability(identity_capability_id, issuer_pub, nf)
                    .map_err(|e| dwow_core::Error::Custom(format!("issue_capability: {e}")))?;
                smol::block_on(chain.block()?.with_call(id_cid, id, &iss.call_data, vec![])?.submit())?;

                shared.identity_capability_id = Some(identity_capability_id.inner());
                *notes.lock().unwrap() = shared;

                Ok(())
            }
        })),
        deploy_ix: None,
        endpoints: vec![
            // ── Governance INACTIVE. These run first: the group id is still zero, so they exercise the
            //    paths that existed before `OBL-C151` and the setter below changes their meaning.
            //
            // A *valid* child that happens to carry the wrong selector: the PN transfer executes
            // successfully as a child, so the only thing that can refuse this transaction is the
            // parent's own selector check. `Identity::VerifyCapabilityV1` is 0x06 — not 0x0b, which is
            // this contract's own `VerifyMemberCapabilityV1`; the entrypoint comment and the design doc
            // that said 0x0b were wrong.
            mk_ep_rejecting("VerifyMemberCapabilityV1_WrongSelector", true, &["ContractError(Custom(34))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().wrong_selector.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_WRONG_SELECTOR, poseidon_hash([pallas::Base::from(PN_VALUE_WRONG_SELECTOR), endowment_bulla]))?;
                    let r = h.verify_member_capability(nullifier_k, capability_id, endowment_bulla, capability_secret, holder_secret, holder_pub, CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
            // Governance inactive, so `withdraw_v1` takes its owner path: `endowment.owner_pubkey !=
            // params.recipient_pubkey` is false for the owner. Success here is what proves the child
            // validation passed — the check that used to be the only thing any row reached.
            //
            // That comparison is itself vacuous (`OBL-C152`): a public key is public, so this path admits
            // anyone who knows the owner's address. Left as it is and asserted here so the fix, when it
            // comes, fails this row.
            mk_ep("WithdrawV1_OwnerPath", false, Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().withdraw_owner.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_WITHDRAW_OWNER, poseidon_hash([pallas::Base::from(PN_VALUE_WITHDRAW_OWNER), endowment_bulla]))?;
                    let r = h.withdraw(endowment_bulla, owner_pub, PN_VALUE_WITHDRAW_OWNER).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // The child is valid and the auth branch is reached: neither `proposal_id` nor
            // `capability_proof` is set, so the endpoint refuses for that reason and no other. Named,
            // because a bare `Rejection` here was satisfied by the child-count check instead.
            mk_ep_rejecting("EndowmentWithdrawV1_NoAuthorization", false, &["ContractError(Custom(29))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().endowment_no_auth.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_ENDOWMENT_NO_AUTH, poseidon_hash([pallas::Base::from(PN_VALUE_ENDOWMENT_NO_AUTH), endowment_bulla]))?;
                    let r = h.endowment_withdraw(endowment_bulla, claim_id, owner_pub, PN_VALUE_ENDOWMENT_NO_AUTH, None).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // `OBL-C154` instance 2, pinned. The child is valid, so the call reaches the mode gate — and
            // refuses, because `initialize_apply_v1` writes `mode: DaoEscrowMode::Escrow` as a constant
            // while `InitializeParamsV1` carries no mode field at all. Every endowment is an Escrow-mode
            // endowment, so this gate can never pass and `TreasurySpendV1` is unreachable by
            // construction. Making `mode` settable is a decision, not a repair; until it is taken, this
            // row states the fact rather than a bare rejection.
            mk_ep_rejecting("TreasurySpendV1_ModeGate", false, &["ContractError(Custom(4))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().treasury_spend.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_TREASURY_SPEND, poseidon_hash([pallas::Base::from(PN_VALUE_TREASURY_SPEND), endowment_bulla]))?;
                    let r = h.treasury_spend(endowment_bulla, proposal_id, owner_pub, PN_VALUE_TREASURY_SPEND).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // While the group id is zero, `OBL-C152`'s four repaired endpoints refuse for that reason and
            // no other — `require_governance_child` fails closed on a zero group. One row states it,
            // because a *later* row per endpoint would read as four findings rather than one, and because
            // the row that matters is the one after the setter that shows the gate opening.
            mk_ep_rejecting("EnableDrainProtectionV1_NoGroup", false, &["ContractError(Custom(43))"], Box::new(move || {
                let r = h.enable_drain_protection(endowment_bulla, DRAIN_PROTECTION_BULLA).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
            })),
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
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_propose, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // ── `OBL-C152`'s repair: four endpoints that had no authorization at all, now gated by the
            //    endowment's group. Each approval is over the action the call performs, not over the
            //    endpoint, and each row would fail if the gate were removed again.
            //
            // `0x06` was the worst of the four in one respect: it wrote two fields **no code path reads**,
            // so it was inert as well as unauthenticated. Gated now, and the inertness is `OBL-C154`'s
            // neighbourhood rather than this row's subject.
            mk_ep("EnableDrainProtectionV1", false, Box::new({
                let gov = gov.clone();
                move || {
                    let approvals = gov.lock().unwrap().enable_drain_protection.clone();
                    let r = h.enable_drain_protection(endowment_bulla, DRAIN_PROTECTION_BULLA).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_enable_dp, approvals)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                        call_data: r.call_data, proofs: vec![],
                    })
                }
            })),
            // The row that was `Success` over a missing authorization until this unit, and that is exactly
            // how the repair was detected: the row asserted the defect so that adding a gate would fail it.
            // It runs *after* the setter now, because before it the endpoint has no group to check against.
            mk_ep("RegisterCapabilityRequirementV1", false, Box::new({
                let gov = gov.clone();
                move || {
                    let approvals = gov.lock().unwrap().register_capability.clone();
                    let r = h.register_capability_requirement(endowment_bulla, b"member_vote".to_vec(), REGISTERED_CAPABILITY_ID, identity_contract_bulla).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_register_cap, approvals)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                        call_data: r.call_data, proofs: vec![],
                    })
                }
            })),
            // Deactivation needs the record the row above just registered — which makes this pair an
            // ordered test of `0x0a` as well: if registration had not landed, this row would fail
            // `CapabilityRequirementNotRegistered` (`Custom(35)`) before reaching its gate.
            mk_ep("DeactivateCapabilityRequirementV1", false, Box::new({
                let gov = gov.clone();
                move || {
                    let approvals = gov.lock().unwrap().deactivate_capability.clone();
                    let r = h.deactivate_capability_requirement(endowment_bulla, b"member_vote".to_vec()).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_deactivate_cap, approvals)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                        call_data: r.call_data, proofs: vec![],
                    })
                }
            })),
            // **The row that proves the `OBL-C154` fix.** Two children: the payment at slot 0, which the
            // endpoint's own check pins to selector `0x04`, and the group's approval at slot 1. Before the
            // fix the approval was read from slot 0 — the same slot the payment must occupy — so no caller
            // could build a call this endpoint would accept, and a green run could not tell.
            //
            // `capability_proof` is `Some(..)` with fabricated contents: it is a *path selector* here, and
            // the contract tests `is_some()` without reading it. That is its own defect and its own unit.
            mk_ep("EndowmentWithdrawV1_Approved", false, Box::new({
                let gov = gov.clone();
                let notes = notes.clone();
                move || {
                    let approvals = gov.lock().unwrap().endowment_withdraw.clone();
                    let note = notes.lock().unwrap().endowment_approved.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_ENDOWMENT_APPROVED, poseidon_hash([pallas::Base::from(PN_VALUE_ENDOWMENT_APPROVED), endowment_bulla]))?;
                    let r = h.endowment_withdraw(endowment_bulla, claim_id, owner_pub, PN_VALUE_ENDOWMENT_APPROVED, Some(CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}))
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
            // The same fix on the third of the money endpoints, under its own role tag (6) and its own
            // action triple — so it also proves the role tag is doing work: this approval and the one
            // above are over different messages and neither can authorise the other's endpoint.
            mk_ep("WithdrawV1_Approved", false, Box::new({
                let gov = gov.clone();
                let notes = notes.clone();
                move || {
                    let approvals = gov.lock().unwrap().withdraw.clone();
                    let note = notes.lock().unwrap().withdraw_approved.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_WITHDRAW_APPROVED, poseidon_hash([pallas::Base::from(PN_VALUE_WITHDRAW_APPROVED), endowment_bulla]))?;
                    let r = h.withdraw(endowment_bulla, owner_pub, PN_VALUE_WITHDRAW_APPROVED).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_withdraw, approvals)
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
            // `OBL-C154` instance 3, pinned. The call is complete and well-formed — the proposal below is
            // found by id, its value and recipient match — and it refuses at the state check, because
            // NOTHING IN THE CONTRACT WRITES `ProposalState::Approved`. Its only occurrence outside the
            // enum's definition is the read in `verify_proposal_approved`. So the proposal lifecycle has
            // no successful exit, and this row is what says so in a way a later fix will break.
            mk_ep_rejecting("ExecuteClaimV1_ProposalNotApproved", false, &["ContractError(Custom(38))"], Box::new({
                let notes = notes.clone();
                move || {
                    let note = notes.lock().unwrap().execute_claim.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                    let child = pn_transfer_child(pn, &note, PN_VALUE_EXECUTE_CLAIM, poseidon_hash([pallas::Base::from(PN_VALUE_EXECUTE_CLAIM), endowment_bulla]))?;
                    // `proposal_id` is the claim's own id: `propose_claim_v1` files the proposal under
                    // `claim_id` and `execute_claim_v1` looks it up by `proposal_id`, so a fixture that
                    // passed the distinct `proposal_id` above would record `ProposalNotFound` and prove
                    // nothing about the state check.
                    let r = h.execute_claim(endowment_bulla, claim_id, owner_pub, PN_VALUE_EXECUTE_CLAIM).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                }
            })),
            // **`OBL-C152`'s most consequential instance, now repaired.** `cancel_claim_v1` used to refuse
            // a cancellation when `proposal.proposer_pubkey != params.proposer_pubkey` — two public values,
            // so any caller who knew the proposer's key cancelled any pending claim. The row that asserted
            // the resulting `Success` (while passing exactly that key) is what caught the repair. The
            // approval is over the claim id; `params.proposer_pubkey` is now unread by the contract and
            // owes removal in the unit that gives the proposer a real proof.
            mk_ep("CancelClaimV1_Approved", false, Box::new({
                let gov = gov.clone();
                move || {
                    let approvals = gov.lock().unwrap().cancel_claim.clone();
                    let r = h.cancel_claim(endowment_bulla, claim_id, proposer_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    let f = ms.finalize(DaoEscrowHarness::governance_group(), action_cancel_claim, approvals)
                        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult {
                        children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                        call_data: r.call_data, proofs: vec![],
                    })
                }
            })),
            // The possession fixture (`OBL-C154` owed). The parent demands a child whose first byte is
            // `0x06`; the child is a real `identity::VerifyCapabilityV1` over the capability `setup`
            // registered, with its own ZK proof. Note what the parent does *not* do: the child's contract
            // id is compared against `identity_cid`, which `init_contract` seeds as `[0u8; 32]` and whose
            // reader treats zero as "skip the check" — fail-open — so the selector byte is the only
            // binding. That is `OBL-C152`'s neighbourhood and is stated rather than implied.
            mk_ep("VerifyMemberCapabilityV1", true, Box::new({
                let notes = notes.clone();
                move || {
                let identity_capability_id = notes.lock().unwrap().identity_capability_id
                    .ok_or_else(|| dwow_core::Error::Custom("setup did not register the identity capability".into()))?;
                let holder = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(CREDENTIAL_SECRET)));
                let v = id.verify_capability(
                    pallas::Base::from(CREDENTIAL_SECRET),
                    identity_capability_id,
                    pallas::Base::from(THRESHOLD),
                    b"role", pallas::Base::from(ATTR_ROLE),
                    b"tenure", pallas::Base::from(ATTR_TENURE),
                    pallas::Base::from(ATTR_BLIND),
                    pallas::Base::from(CAPABILITY_SECRET),
                    PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(ISSUER_SECRET))),
                    holder,
                    pallas::Base::from(SCHEMA_HASH), 0, EXPIRES_AT, true,
                ).map_err(|e| dwow_core::Error::Custom(format!("verify_capability: {e}")))?;
                let child = ChildCall {
                    contract_id: *IDENTITY_CONTRACT_ID,
                    call_data: v.call_data,
                    proofs: vec![v.proof],
                };
                let r = h.verify_member_capability(nullifier_k, capability_id, endowment_bulla, capability_secret, holder_secret, holder_pub, CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![r.proof] })
                }
            })),
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
                    let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
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
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
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
                        let r = h.propose_claim(nullifier_k, endowment_bulla, claim_id, capability_id, capability_secret, proposer_secret, PN_VALUE_EXECUTE_CLAIM, pallas::Base::from(50u64), owner_pub, proposer_pub, ClaimType::Endowment, pallas::Base::from(10u64), CapabilityProof{capability_id:cp_id,capability_secret:cp_secret,nullifier:IntentNullifier::from_base(pallas::Base::from(42u64)),issuer_pub:[0u8;32],predicate_result:[0u8;32],proof:vec![]}).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        let f = ms.finalize(DaoEscrowHarness::governance_group(), msg_wrong, approvals)
                            .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult {
                            children: vec![ChildCall { contract_id: *MULTISIG_CONTRACT_ID, call_data: f.call_data, proofs: vec![f.proof] }],
                            call_data: r.call_data, proofs: vec![r.proof],
                        })
                    }
                }),
            },
            // ── The reader for `OBL-C152`'s repair: a real call to a repaired endpoint carrying no
            //    approval child at all. Without these rows the four new gates would be asserted only by
            //    their positive paths, and a gate that silently admitted everyone would still pass them.
            EndpointSpec {
                name: "EnableDrainProtectionV1_NoApproval",
                is_zk: false,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    let r = h.enable_drain_protection(endowment_bulla, DRAIN_PROTECTION_BULLA).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "CancelClaimV1_NoApproval",
                is_zk: false,
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new(move || {
                    // The proposer's own key, which used to be the whole authorization. It is not one now,
                    // and that is the point of the row.
                    let r = h.cancel_claim(endowment_bulla, claim_id, proposer_pub).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                    Ok(EndpointResult { children: vec![], call_data: r.call_data, proofs: vec![] })
                }),
            },
            EndpointSpec {
                name: "WithdrawV1_GovernanceActiveWithoutApproval",
                is_zk: false,
                // The reader that `withdraw_v1`'s governance branch never had: before this it was an
                // empty body, so activating governance removed the owner check and left nothing. With the
                // payment child present and no approval beside it, the slot-1 check is what refuses.
                expectation: EndpointExpectation::RejectionNaming(&["ContractError(Custom(33))"]),
                generate_with_coinbase: None,
                verify_state: None,
                generate: Box::new({
                    let notes = notes.clone();
                    move || {
                        // Its own note: the `_NoAuthorization` row spends the endowment-withdraw one, and
                        // lending it here would surface as a PN double-spend rather than as the missing
                        // approval this row is about.
                        let note = notes.lock().unwrap().withdraw_no_approval.clone().ok_or_else(|| dwow_core::Error::Custom("setup did not publish the note".into()))?;
                        let child = pn_transfer_child(pn, &note, PN_VALUE_WITHDRAW_NO_APPROVAL, poseidon_hash([pallas::Base::from(PN_VALUE_WITHDRAW_NO_APPROVAL), endowment_bulla]))?;
                        let r = h.withdraw(endowment_bulla, owner_pub, PN_VALUE_WITHDRAW_NO_APPROVAL).map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
                        Ok(EndpointResult { children: vec![child], call_data: r.call_data, proofs: vec![] })
                    }
                }),
            },
        ],
    }
}
