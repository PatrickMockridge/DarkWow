/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! WASM entrypoint for the DAO-Escrow contract
//!
//! ## One endowment pool, governed by one MultiSig group
//!
//! The endowment is a pool of promissory notes. Its owner installs a MultiSig group with `UpdateV1`,
//! and from then on **every spend is authorised by that group's `multisig::FinalizeV1` over a message
//! naming the action** — a message the group's threshold has signed, which the multisig contract
//! consumes exactly once (`OBL-C151`). There is no second authority: the capability model this contract
//! was designed around never had a requirement registered, so every one of its gates refused every call
//! (`OBL-C151`).
//!
//! The ten endpoints fall into four groups:
//!
//! 1. **Lifecycle** — `initialize` (create the pool; the mode and the premium floor are the creator's),
//!    `update` (install the group; one-shot, via the owner's `SetGovernanceConfigV2` proof).
//! 2. **Funding** — `pay_premium` issues a membership note and enforces `min_premium`.
//! 3. **Spending**, each ending in a `promissory_note::transfer_v1` child that moves the notes —
//!    `withdraw` (owner or group), `endowment_withdraw` (escrow modes), `treasury_spend` (treasury
//!    modes). The `mode` the creator chose decides which of the last two is legal.
//! 4. **The claim lifecycle** — `propose_claim` → `vote_claim` → `execute_claim`, with `cancel_claim`
//!    alongside. Each step needs the group's approval; the group's own threshold **is** the vote's
//!    quorum, so one approved vote decides the claim (`OBL-C159`, `OBL-C160`).
//!
//! ```text
//! Members pay premiums ──> Endowment Pool ──> claims, by group decision
//!                              ▲
//!                              │
//!                     Membership notes
//!                     (block-based expiry)
//! ```
//!
//! Balances are the Purse contract's to hold and to check: this contract's spend paths publish the
//! child and depend on it, rather than keeping a second copy of the balance that could disagree.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, MULTISIG_CONTRACT_ID},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::DaoEscrowError,
    model,
    DaoEscrowFunction, DAO_ESCROW_CONTRACT_BULLAS_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
    DAO_ESCROW_CONTRACT_INFO_TREE,
    DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE,
    DAO_ESCROW_CONTRACT_PROPOSALS_TREE,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
    MULTISIG_CONTRACT_ID_KEY,
};


dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize DAO-Escrow contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Bullas tree (endowment instances)
/// - Membership tree (membership notes)
/// - Endowment tree (funds pool)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[dao_escrow::init_contract] Initializing DAO-Escrow contract");

    // V2 (V1 loads removed — rc3 migration) circuits (HAZOP RC3: domain separation)
    let init_v2_bincode = include_bytes!("../proof/init.zk.bin");
    wasm::db::zkas_db_set(&init_v2_bincode[..])?;
    let pay_premium_v2_bincode = include_bytes!("../proof/pay_premium.zk.bin");
    wasm::db::zkas_db_set(&pay_premium_v2_bincode[..])?;
    let propose_claim_v2_bincode = include_bytes!("../proof/propose_claim.zk.bin");
    wasm::db::zkas_db_set(&propose_claim_v2_bincode[..])?;
    let vote_claim_v2_bincode = include_bytes!("../proof/vote_claim.zk.bin");
    wasm::db::zkas_db_set(&vote_claim_v2_bincode[..])?;
    let set_governance_config_v2_bincode = include_bytes!("../proof/set_governance_config.zk.bin");
    wasm::db::zkas_db_set(&set_governance_config_v2_bincode[..])?;

    // Initialize info tree. Two entries only, and both are read: the promissory-note id every money
    // endpoint's child check compares against, and the multisig id every governance gate's child check
    // compares against. The `db_version`, `identity_cid`, `box_cid` and `purse_cid` entries that were
    // written here had no reader at all — and `identity_cid` was seeded as `[0u8; 32]`, which its reader
    // treated as "skip the routing check", i.e. fail-open (`OBL-C152`).
    let info_db = wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;
    // The **real** id, not a zero placeholder: the governance helper treats zero as "refuse everything"
    // (HAZOP H-11), so seeding zero would fail every gate closed forever — `OBL-C151` again with a
    // different field name.
    wasm::db::db_set(info_db, MULTISIG_CONTRACT_ID_KEY, &MULTISIG_CONTRACT_ID.to_bytes())?;

    // Initialize bullas tree (endowment instances)
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;

    // Initialize membership tree
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;

    // Initialize endowment tree
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // The proposals and nullifiers trees are the two the surviving endpoints actually use: proposals holds
    // the claim lifecycle's records, and nullifiers holds every spend-once value — approvals, votes and
    // ownership proofs. The `votes`, `capability_requirements`, `disputes` and `governance` trees were
    // initialised here and written by nobody.
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    wasm::db::db_init(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[dao_escrow::init_contract] DAO-Escrow contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoEscrowFunction::try_from(self_.data[0])?;

    msg!("[dao_escrow::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        DaoEscrowFunction::InitializeV1 => initialize_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::PayPremiumV1 => pay_premium_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::ProposeClaimV1 => propose_claim_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::VoteClaimV1 => vote_claim_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::UpdateV1 => update_get_metadata(cid, call_idx, &calls),
        DaoEscrowFunction::WithdrawV1 => withdraw_get_metadata(cid, call_idx, &calls),
        // The non-ZK functions below fall here. They must return an **encoded** empty
        // `zk_public_inputs`, not a bare `vec![]`: the host decodes the metadata as
        // `Vec<(String, Vec<Base>)>` (`execution.rs:423`), so a 0-byte buffer fails that decode and
        // is reported as "contract signalled EMPTY metadata, the documented rejection signal" —
        // which made every one of them uncallable (register OBL-C77). The functions are
        // `WithdrawV1`, `EndowmentWithdrawV1`, `TreasurySpendV1`,
        // `EnableDrainProtectionV1`, `ExecuteClaimV1`, `RegisterCapabilityRequirementV1`,
        // `CancelClaimV1`, `SetGovernanceConfigV1`, `SetGovernanceActiveV1`,
        // `DeactivateCapabilityRequirementV1`. `UpdateV1` left this list on 2026-09-27, when it became
        // the caller of the `SetGovernanceConfigV2` ownership circuit (`OBL-C151`), and
        // `SetGovernanceConfigV1` joined it: a retired no-op has no circuit to publish instances for.
        _ => {
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            let mut m = vec![];
            zk_public_inputs.encode(&mut m)?;
            Ok(m)
        }
    }?;

    wasm::util::set_return_data(&metadata)
}

/// Metadata for InitializeV1 (0x00)
/// The transaction binding every arm publishes — the deriving side of `OBL-C198`.
///
/// The commitment comes from the host (`get_tx_commitment`) and **not** from the call data: the
/// commitment is a derivation over the call data, so a binding carried inside it would be computed
/// from a value that covers it — a cycle with no fixed point, i.e. a proof nothing can satisfy.
/// What stood at the six arms was the constant `poseidon_hash([3, 0, 0])`, which bound every proof
/// to nothing at all: it was identical in every transaction, so a proof lifted from one transaction
/// verified in another. Two arms below share the `SetGovernanceConfigV2` circuit and so share this
/// binding, which is why there are six call sites and five circuits.
///
/// The nonce is zero because these calls carry no nonce field, so every proof in one transaction
/// publishes the same binding — a *linking* of that transaction's own proofs, which
/// `tx-commitment.md` §The Nullifier Scheme exists to avoid. It is still strictly better than the
/// constant, and a per-proof nonce on the wire is owed.
fn dao_escrow_tx_binding(tx_nonce: pallas::Base) -> Result<pallas::Base, ContractError> {
    Ok(poseidon_hash([pallas::Base::from(3u64), wasm::util::get_tx_commitment()?, tx_nonce]))
}

fn initialize_get_metadata(_cid: ContractId, call_idx: usize, calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::InitializeParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (owner_pub_x, owner_pub_y) = params.owner_pubkey.xy().expect("pk not identity");

    // The endowment bulla, using the same formula as the `InitV2` circuit and
    // `DaoEscrow::derive_bulla`:
    //     poseidon_hash(DRK_POSEIDON_DOMAIN_COMMITMENT, dao_bulla, owner_pub_x, owner_pub_y,
    //                   endowment_asset_id, bulla_blind)
    //
    // The domain is named rather than written as its literal `4`. That literal is what `OBL-C156` was:
    // three sites derived this value — the circuit, this metadata arm and `derive_bulla` — and they
    // disagreed about the domain (one had none) with nothing comparing them. Using the same *name* in
    // both host sites does not make them agree with the circuit, which is a separate file, but it does
    // mean a change to the domain cannot move one of the two and leave the other behind.
    let endowment_bulla = dwow_sdk::crypto::poseidon_hash([
        dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_COMMITMENT,
        params.dao_bulla.inner(),
        owner_pub_x,
        owner_pub_y,
        params.endowment_asset_id.inner(),
        params.bulla_blind.inner(),
    ]);

    // `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` with
    // `DOMAIN_TX_BINDING = 3` (`src/sdk/src/crypto/constants.rs:57`). This contract's tx pair is
    // the constant `(0, 0)` — the convention `attestation` and `identity` use, and one of the 21
    // constant bindings the register already records.
    //
    // It was `pallas::Base::zero()`, labelled "Pattern A: pass-through placeholder", which is NOT
    // that hash. The circuit constrains `tx_binding == poseidon(3, tx_commitment, tx_nonce)`, so
    // publishing a literal zero required `poseidon(3, 0, 0) == 0` — a preimage. Every proof for
    // this circuit was unsatisfiable, not merely unbound, and a count check cannot see it because
    // the vector had the right length (register OBL-C78).
    let tx_binding = dao_escrow_tx_binding(pallas::Base::zero())?;
    let tx_nonce_val = pallas::Base::zero();

    // Circuit constrain_instance order: [dao_bulla, endowment_bulla, tx_binding, tx_nonce] — the
    // pair last (`OBL-C198`). It sat at 1,2 of 4 with `endowment_bulla` after it.
    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_INIT_NS_V2.to_string(),
        vec![
            params.dao_bulla.inner(),
            endowment_bulla,
            tx_binding,
            tx_nonce_val,
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for PayPremiumV1 (0x02) — PayPremiumV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce]
fn pay_premium_get_metadata(_cid: ContractId, _call_idx: usize, _calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>]) -> Result<Vec<u8>, ContractError> {
    // `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` with
    // `DOMAIN_TX_BINDING = 3` (`src/sdk/src/crypto/constants.rs:57`). This contract's tx pair is
    // the constant `(0, 0)` — the convention `attestation` and `identity` use, and one of the 21
    // constant bindings the register already records.
    //
    // It was `pallas::Base::zero()`, labelled "Pattern A: pass-through placeholder", which is NOT
    // that hash. The circuit constrains `tx_binding == poseidon(3, tx_commitment, tx_nonce)`, so
    // publishing a literal zero required `poseidon(3, 0, 0) == 0` — a preimage. Every proof for
    // this circuit was unsatisfiable, not merely unbound, and a count check cannot see it because
    // the vector had the right length (register OBL-C78).
    let tx_binding = dao_escrow_tx_binding(pallas::Base::zero())?;
    let tx_nonce_val = pallas::Base::zero();

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_PREMIUM_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DaoEscrowFunction::try_from(self_.data[0])?;

    msg!("[dao_escrow::process_instruction] Processing function: {:?}", func);

    match func {
        DaoEscrowFunction::InitializeV1 => {
            let params = model::InitializeParamsV1::decode(&self_.data[1..])?;
            initialize_v1(cid, params)
        }
        DaoEscrowFunction::UpdateV1 => {
            let params = model::UpdateParamsV1::decode(&self_.data[1..])?;
            update_v1(cid, params)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let params = model::PayPremiumParamsV1::decode(&self_.data[1..])?;
            pay_premium_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let params = model::WithdrawParamsV1::decode(&self_.data[1..])?;
            withdraw_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let params = model::EndowmentWithdrawParamsV1::decode(&self_.data[1..])?;
            endowment_withdraw_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let params = model::TreasurySpendParamsV1::decode(&self_.data[1..])?;
            treasury_spend_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::ProposeClaimV1 => {
            let params = model::ProposeClaimParamsV1::decode(&self_.data[1..])?;
            propose_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let params = model::VoteClaimParamsV1::decode(&self_.data[1..])?;
            vote_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let params = model::ExecuteClaimParamsV1::decode(&self_.data[1..])?;
            execute_claim_v1(cid, call_idx, calls, params)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let params = model::CancelClaimParamsV1::decode(&self_.data[1..])?;
            cancel_claim_v1(cid, call_idx, calls, params)
        }
    }
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = DaoEscrowFunction::try_from(update_data[0])?;

    match func {
        DaoEscrowFunction::InitializeV1 => {
            let update = model::InitializeUpdateV1::decode(&update_data[1..])?;
            initialize_apply_v1(cid, update)
        }
        DaoEscrowFunction::UpdateV1 => {
            let update = model::UpdateUpdateV1::decode(&update_data[1..])?;
            update_apply_v1(cid, update)
        }
        DaoEscrowFunction::PayPremiumV1 => {
            let update = model::PayPremiumUpdateV1::decode(&update_data[1..])?;
            pay_premium_apply_v1(cid, update)
        }
        DaoEscrowFunction::WithdrawV1 => {
            let update = model::WithdrawUpdateV1::decode(&update_data[1..])?;
            withdraw_apply_v1(cid, update)
        }
        DaoEscrowFunction::EndowmentWithdrawV1 => {
            let update = model::EndowmentWithdrawUpdateV1::decode(&update_data[1..])?;
            endowment_withdraw_apply_v1(cid, update)
        }
        DaoEscrowFunction::TreasurySpendV1 => {
            let update = model::TreasurySpendUpdateV1::decode(&update_data[1..])?;
            treasury_spend_apply_v1(cid, update)
        }
        DaoEscrowFunction::ProposeClaimV1 => {
            let update = model::ProposeClaimUpdateV1::decode(&update_data[1..])?;
            propose_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::VoteClaimV1 => {
            let update = model::VoteClaimUpdateV1::decode(&update_data[1..])?;
            vote_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::ExecuteClaimV1 => {
            let update = model::ExecuteClaimUpdateV1::decode(&update_data[1..])?;
            execute_claim_apply_v1(cid, update)
        }
        DaoEscrowFunction::CancelClaimV1 => {
            let update = model::CancelClaimUpdateV1::decode(&update_data[1..])?;
            cancel_claim_apply_v1(cid, update)
        }
    }
}

// ============================================================================
// INSTRUCTION HANDLERS
// ============================================================================

/// InitializeV1 instruction - creates a new DAO-Escrow endowment
fn initialize_v1(cid: ContractId, params: model::InitializeParamsV1) -> ContractResult {
    msg!("[dao_escrow::initialize_v1] Initializing DAO-Escrow endowment");

    // Derive endowment bulla (formula must match init.zk circuit)
    let endowment_bulla = model::DaoEscrow::derive_bulla(
        params.dao_bulla,
        &params.owner_pubkey,
        params.endowment_asset_id,
        params.bulla_blind.clone(),
    );

    // The duplicate check is on the **derived** bulla — the key the record is stored under — and it used
    // to test `params.dao_bulla`, which is the DAO's own bulla and not a key anything is written to. So
    // the check could never fire: two `initialize` calls with the same DAO bulla and different owners
    // derive different endowment bullas and are two legitimate endowments, while two calls with the same
    // DAO bulla *and* the same owner derive the same one — and that second case is the one the check was
    // for. Testing the value that is actually written is what makes it a guard.
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    if wasm::db::db_contains_key(bullas_db, &endowment_bulla.to_bytes())? {
        msg!("[dao_escrow::initialize_v1] ERROR: this endowment already exists");
        return Err(DaoEscrowError::DaoEscrowAlreadyExists("Endowment already exists".to_string()).into())
    }

    let update = model::InitializeUpdateV1 {
        bulla: endowment_bulla,
        owner_pubkey: params.owner_pubkey,
        mode: params.mode,
        min_premium: params.min_premium,
    };

    msg!("[dao_escrow::initialize_v1] Endowment initialized: {:?}", endowment_bulla);
    wasm::util::set_return_data(&[&[DaoEscrowFunction::InitializeV1 as u8], &update.encode()[..]].concat())
}

/// InitializeV1 apply - store new endowment
fn initialize_apply_v1(cid: ContractId, update: model::InitializeUpdateV1) -> ContractResult {
    let bullas_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_BULLAS_TREE)?;
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Store endowment bulla in bullas tree (non-empty marker — empty is
    // invisible to db_contains_key per §9.1, which breaks the duplicate check)
    wasm::db::db_set(bullas_db, &update.bulla.to_bytes(), &[1])?;

    // Four fields, and the mode is the caller's rather than a constant. `multisig_group_id` starts at zero,
    // which every governance gate reads as "no group installed" and refuses on, so the endowment is
    // owner-controlled until the owner installs a group through `update_v1`.
    let endowment = model::DaoEscrow {
        mode: update.mode,
        owner_pubkey: update.owner_pubkey,
        multisig_group_id: pallas::Base::zero(),
        min_premium: update.min_premium,
    };

    wasm::db::db_set(endowments_db, &update.bulla.to_bytes(), &endowment.encode())?;

    msg!("[dao_escrow::initialize_apply_v1] Endowment stored: {:?}", update.bulla);
    Ok(())
}

/// UpdateV1 instruction - update endowment parameters
/// `UpdateV1` (0x01) — register the endowment's governance group (`OBL-C151`).
///
/// **The owner is proved, not asserted.** The accompanying `SetGovernanceConfigV2` proof derives
/// `owner_pub = ec_mul_base(owner_secret, NULLIFIER_K)` and constrains the *exposed* `owner_pub_x/y` to
/// it, so the coordinates read here off the params are known to whoever could build that proof. The
/// comparison below is therefore against a value the caller has demonstrated knowledge of — unlike
/// `withdraw_v1`'s owner path, which compares a public key against a public key and so gates nothing
/// (`OBL-C152`).
///
/// **It is one-shot.** `owner_nullifier` is deterministic in `(owner_secret, dao_escrow_bulla)` because
/// the circuit binds it, so recording it in the nullifiers tree makes a replay of the same proof
/// impossible and a caller cannot mint a fresh nullifier per call. A record that already carries a group
/// refuses a second write: there is no rotation in this design, so the choice is irreversible by
/// construction.
fn update_v1(cid: ContractId, params: model::UpdateParamsV1) -> ContractResult {
    msg!("[dao_escrow::update_v1] Updating DAO-Escrow: {:?}", params.bulla);

    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.bulla.to_bytes())?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;
    let mut endowment = model::DaoEscrow::decode(&endowment_data)?;

    // The proof exposed these coordinates; the record's owner must be the same point.
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (ox, oy) = params.owner_pubkey.xy().expect("pk not identity");
    #[expect(clippy::expect_used, reason = "the stored owner is a PublicKey, so xy() is always Some")]
    let (ex, ey) = endowment.owner_pubkey.xy().expect("pk not identity");
    if ox != ex || oy != ey {
        msg!("[dao_escrow::update_v1] ERROR: the proof's owner is not this endowment's owner");
        return Err(DaoEscrowError::NotOwner.into())
    }

    let nullifiers_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.owner_nullifier.to_repr())? {
        msg!("[dao_escrow::update_v1] ERROR: this ownership proof has already been used");
        return Err(DaoEscrowError::OwnershipProofReplayed.into())
    }

    match params.multisig_group_id {
        // **A call naming no group is refused, and it must be.** The record's mode, owner and premium
        // floor are immutable after `InitializeV1`, so a group-less `UpdateV1` has nothing to do —
        // and `update_apply_v1` below records `owner_nullifier` unconditionally. Because that nullifier
        // is deterministic in `(owner_secret, dao_escrow_bulla)`, letting a no-op through would spend
        // the owner's one-shot proof and make governance **permanently uninstallable** on this
        // endowment: every later `UpdateV1` would fail `OwnershipProofReplayed` (`OBL-C161`). A silent
        // success on a call that does nothing is the same defect as a gate that cannot fail.
        None => {
            msg!("[dao_escrow::update_v1] ERROR: no governance group named, and nothing else to update");
            return Err(DaoEscrowError::NoGovernanceGroup.into())
        }
        Some(gid) => {
            if gid == pallas::Base::zero() {
                msg!("[dao_escrow::update_v1] ERROR: a zero group id does not activate governance");
                return Err(DaoEscrowError::GovernanceNotActive.into())
            }
            if endowment.multisig_group_id != pallas::Base::zero() {
                msg!("[dao_escrow::update_v1] ERROR: this endowment already has a governance group");
                return Err(DaoEscrowError::GovernanceAlreadyActive.into())
            }
            endowment.multisig_group_id = gid;
        }
    }

    let update = model::UpdateUpdateV1 {
        bulla: params.bulla,
        owner_nullifier: params.owner_nullifier,
        endowment_bytes: endowment.encode(),
    };

    msg!("[dao_escrow::update_v1] Endowment update prepared: {:?}", params.bulla);
    wasm::util::set_return_data(&[&[DaoEscrowFunction::UpdateV1 as u8], &update.encode()?[..]].concat())
}

/// UpdateV1 apply - update endowment parameters
/// Blind write of what exec encoded — apply may not read (`OBL-C72`). This replaced a stub whose only
/// statement was a comment claiming the write would happen "in a full implementation", which is the
/// false statement that let the field go unset for as long as it did (`OBL-C151`).
fn update_apply_v1(cid: ContractId, update: model::UpdateUpdateV1) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    wasm::db::db_set(endowments_db, &update.bulla.to_bytes(), &update.endowment_bytes)?;
    // The other half of the one-shot guard (`OBL-C151`): exec *checks* this nullifier, and apply has to
    // *record* it, or the check has nothing to find. This is the write the check's absence proved
    // necessary — `UpdateV1_ReplaysTheProof` passed a second time until it was here.
    let nullifiers_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_set(nullifiers_db, &update.owner_nullifier.to_repr(), &[1])?;
    msg!("[dao_escrow::update_apply_v1] Endowment updated: {:?}", update.bulla);
    Ok(())
}

/// PayPremiumV1 instruction - member pays premium, receives membership
fn pay_premium_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>, params: model::PayPremiumParamsV1) -> ContractResult {
    msg!("[dao_escrow::pay_premium_v1] Processing premium payment");

    // Validate child call is promissory_note::transfer_v1 (0x04) for premium payment
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[pay_premium_v1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[pay_premium_v1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify DAO-Escrow endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    if endowment_data.is_none() {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Endowment not found");
        return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
    }

    // Verify membership note doesn't already exist
    let membership_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;
    if wasm::db::db_contains_key(membership_db, &params.membership_note.to_bytes())? {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Membership already exists");
        return Err(DaoEscrowError::ClaimAlreadyExists("Membership already exists".to_string()).into())
    }

    // Verify ZK proof (skipped - ZK verification happens at validator runtime)
    // wasm::zk::verify_zk_proof(cid, crate::DAO_ESCROW_ZKAS_PREMIUM_NS)?;

    // **The premium floor is enforced here, and this is the only thing that makes `min_premium` a
    // reader rather than a field.** It was previously set to `0` by `initialize_apply_v1` for every
    // endowment and never compared to anything, so a caller could pay `0` and still be issued a
    // membership note. The floor is the creator's, carried in the record.
    let endowment = model::DaoEscrow::decode(
        &endowment_data.ok_or(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?,
    )?;
    if params.value < endowment.min_premium {
        msg!("[dao_escrow::pay_premium_v1] ERROR: Premium {} is below the floor {}", params.value, endowment.min_premium);
        return Err(DaoEscrowError::InsufficientPremium.into())
    }

    // The membership timestamp is read here because apply may not (register OBL-C72).
    let update = model::PayPremiumUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        membership_note: params.membership_note,
        amount: params.value,
        member_pubkey: params.member_pubkey,
        asset_id: params.asset_id,
        expiry: params.expiry,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        endowment_bytes: endowment.encode(),
    };

    msg!("[dao_escrow::pay_premium_v1] Premium processed: {:?}", params.membership_note);
    wasm::util::set_return_data(&[&[DaoEscrowFunction::PayPremiumV1 as u8], &update.encode()?[..]].concat())
}

/// PayPremiumV1 apply - store membership note and update endowment
fn pay_premium_apply_v1(cid: ContractId, update: model::PayPremiumUpdateV1) -> ContractResult {
    let membership_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE)?;
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Create and store membership
    let membership = model::Membership {
        version: 1,
        note: update.membership_note,
        dao_escrow_bulla: update.dao_escrow_bulla,
        member_pubkey: update.member_pubkey,
        value: update.amount,
        asset_id: update.asset_id,
        expiry: update.expiry,
        created_at: update.created_at,
    };

    wasm::db::db_set(membership_db, &update.membership_note.to_bytes(), &membership.encode())?;

    // Blind write. Both values this used to read — the block height for `created_at` and the
    // endowment record — now arrive in the update (register OBL-C72). Purse::DepositV1 child call
    // handles the balance; `endowment_purse_id` is the Purse instance reference, not a counter.
    wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &update.endowment_bytes)?;

    msg!("[dao_escrow::pay_premium_apply_v1] Membership stored: {:?}", update.membership_note);
    Ok(())
}

/// WithdrawV1 instruction - endowment owner withdraws funds
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
/// WithdrawV1 instruction — the owner's withdrawal, authorised by the ownership proof
///
/// **What was wrong, stated precisely.** The check was
/// `endowment.owner_pubkey != params.recipient_pubkey` — two public values, so *anyone* who knew the
/// owner's address could call it (`OBL-C152`, its last instance). The **effect** was narrower than the
/// defect reads: the payee had to be the owner, so the funds could only ever reach the owner and no
/// attacker could redirect them. What an attacker could do is trigger the transfer — force the owner's
/// funds out of the endowment and into the owner's own hands — which is an unauthorised state change,
/// not theft.
///
/// **What fixes it.** The `recipient_pubkey == record.owner_pubkey` comparison stays, and the
/// `SetGovernanceConfigV2` proof (see `withdraw_get_metadata`) turns it into an authorisation: the
/// circuit constrains the exposed coordinates of *that key* to
/// `ec_mul_base(owner_secret, NULLIFIER_K)`, so a caller who passes the comparison has demonstrated
/// knowledge of the owner's secret. No one else can produce that proof, which is the whole difference.
///
/// **The group branch is removed rather than repaired.** It read a `multisig::FinalizeV1` child at slot
/// 1 while the payment occupies slot 0 — the shape `OBL-C154` fixed for its siblings — but the deeper
/// problem was structural: one `requires_proof` declaration has to describe both a path that carries a
/// proof and one that must not, and whichever way it is set a client reading it builds the wrong call
/// for one of them. The group's spend has two better homes, `EndowmentWithdrawV1` and `TreasurySpendV1`,
/// each of which states the mode that makes it legal. `withdraw` is the owner's, and now only the
/// owner's.
///
/// **The proof is the only gate, and there is no balance guard here** — `Purse::WithdrawV1` refuses a
/// withdrawal larger than the balance, and a failing child fails this transaction.
fn withdraw_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::WithdrawParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::withdraw_v1] Processing withdrawal");

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    // Exactly one child, the payment: this endpoint's authority is its own proof, not a governance
    // approval, so there is no slot 1 to reserve. The count was `<= 2` while the group branch existed.
    if self_.children_indexes.len() != 1 {
        msg!(
            "[WithdrawV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[WithdrawV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    // The seed is the record and the amount: `poseidon_hash([params.value, bulla])`. It is *not* a
    // function of the call, and that is deliberate rather than an oversight — **a seed cannot make a
    // repeat impossible, and never could.** `promissory_note::transfer_v1` refuses a duplicate of the
    // child's output *commitment* (the note leaf), and that leaf's blind is a caller-chosen
    // `TransferCallOutput.commitment_blind`, independent of the value blind this seed produces
    // (`OBL-C192`). So a caller that varies that one argument may withdraw the same amount twice
    // whatever this line says, and a caller that reuses it is refused no matter what this line says.
    // `OBL-C191` read this the other way for three units and changed five sites in this contract on
    // that reading; what the measured collisions needed was the *caller's* leaf blind, and that is
    // `OBL-C192`'s unit A.
    //
    // What this endpoint is nonetheless worth knowing about, because it is unusual: **nothing else
    // bounds how many withdrawals an endowment may make.** It records no nullifier (see the note
    // below on why it must not), it changes no state any reader consults, and its authority — the
    // ownership proof — is fresh for every call.
    //
    // The params had nothing to key on either: `owner_nullifier` is deterministic in
    // `(owner_secret, dao_escrow_bulla)` (`model/mod.rs:488-501`), so it is *identical* for two
    // withdrawals by one owner, and `withdraw_get_metadata` publishes the zero-pair `tx_binding`
    // (`:1372-1376`), a constant.
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::withdraw_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // The authority: the payee must be the endowment's owner, and the `SetGovernanceConfigV2` proof
    // that rides with this call is what makes that comparison mean something — see this function's
    // header and `withdraw_get_metadata`. An endowment whose owner has been changed by no one (there is
    // no path to change it) can only be withdrawn from by a caller who knows the owner's secret.
    if endowment.owner_pubkey != params.recipient_pubkey {
        msg!("[dao_escrow::withdraw_v1] ERROR: Not authorized to withdraw");
        return Err(DaoEscrowError::NotAuthorizedToWithdraw.into())
    }

    // `params.owner_nullifier` is deliberately **not** written to the nullifiers tree: it is the value
    // `update_v1` records one-shot to install the group, and recording it here would let a single
    // withdrawal prevent governance from ever being installed. See `WithdrawParamsV1`'s field note.

    // Create update
    let update = model::WithdrawUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        value: params.value,
        amount: params.value, // Purse::WithdrawV1 verifies balance >= amount
        // Carried so apply re-stores it instead of reading it back (OBL-C72).
        endowment_bytes: endowment.encode(),
    };

    msg!("[dao_escrow::withdraw_v1] Withdrawal processed: {}", params.value);
    wasm::util::set_return_data(&[&[DaoEscrowFunction::WithdrawV1 as u8], &update.encode()?[..]].concat())
}

/// WithdrawV1 apply - update endowment totals
fn withdraw_apply_v1(cid: ContractId, update: model::WithdrawUpdateV1) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Blind write — the record was read in exec and carried here (OBL-C72). Purse handles the
    // balance; `endowment_purse_id` is the instance reference.
    wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &update.endowment_bytes)?;

    msg!("[dao_escrow::withdraw_apply_v1] Endowment updated: new total = {}", update.amount);
    Ok(())
}

/// EndowmentWithdrawV1 instruction - executes an approved claim from endowment
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn endowment_withdraw_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::EndowmentWithdrawParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::endowment_withdraw_v1] Processing endowment withdrawal");

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    // One child — the payment — plus, when governance is active, a second: the MultiSig approval at slot
    // 1, exactly as `withdraw_v1` has it. The count was `!= 1` and the approval was read from slot 0,
    // which is unsatisfiable together: the check below pins slot 0 to selector 0x04, and the approval
    // child carries 0x03, so the governance path could never be built by any caller (`OBL-C154`).
    if self_.children_indexes.is_empty() || self_.children_indexes.len() > 2 {
        msg!(
            "[EndowmentWithdrawV1] Error: Expected 1 child call (promissory_note::transfer_v1), plus a MultiSig approval when governance is active; got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[EndowmentWithdrawV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::endowment_withdraw_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Authorisation: the endowment's group, and nothing else.
    //
    // There used to be two other paths and both were phantoms. `params.proposal_id` named a proposal
    // this endpoint loaded and checked with `verify_proposal_approved` — a *second* executor for the
    // lifecycle `ExecuteClaimV1` already executes, and one that could never pass, because nothing in the
    // crate writes `ProposalState::Approved` (`OBL-C159`). `params.capability_proof` was three lines
    // long and read nothing: a bare `Option::is_some()` standing in for "the caller chose the governance
    // path", with a field named for a capability proof carrying one bit of routing (`OBL-C152`).
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (rx, _ry) = params.recipient_pubkey.xy().expect("pk not identity");
    let action = model::governance_message(
        model::governance_role::ENDOWMENT_WITHDRAW,
        poseidon_hash([
            params.dao_escrow_bulla.inner(),
            pallas::Base::from(params.value),
            rx,
        ]),
    );
    require_governance_child(cid, call_idx, &calls, &endowment, 1, action)?;

    // The mode gate, which `treasury_spend_v1` already had and this endpoint did not: `escrow` mode pays
    // claims from the endowment, `treasury` mode spends operational funds, and `TreasuryEndowment` does
    // both. Without it this endpoint was reachable in every mode, which made the `mode` field's first
    // two variants indistinguishable here (`OBL-C154`).
    if endowment.mode == model::DaoEscrowMode::Treasury {
        msg!("[dao_escrow::endowment_withdraw_v1] ERROR: Not an endowment mode DAO-Escrow");
        return Err(DaoEscrowError::InvalidState { expected: "Escrow mode".to_string(), actual: "Treasury mode".to_string() }.into())
    }

    // The endowment's balance is `Purse::WithdrawV1`'s to check — it refuses a withdrawal larger than
    // the balance, and a child that fails fails this transaction. The guard that stood here was
    // `if false { … }`: a refusal no input could reach, left in place while the balance moved to Purse.
    // Removing it rather than deleting the block keeps the fact that a balance guard belongs here, one
    // contract over, rather than leaving a reader to wonder where it went.

    // Create update
    let update = model::EndowmentWithdrawUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        value: params.value,
        amount: params.value, // Purse verifies balance
        endowment_bytes: endowment.encode(),
    };

    msg!(
        "[dao_escrow::endowment_withdraw_v1] Endowment withdrawal processed: {} to {:?}",
        params.value,
        params.recipient_pubkey
    );
    wasm::util::set_return_data(&[&[DaoEscrowFunction::EndowmentWithdrawV1 as u8], &update.encode()?[..]].concat())
}

/// EndowmentWithdrawV1 apply - update endowment totals
fn endowment_withdraw_apply_v1(
    cid: ContractId,
    update: model::EndowmentWithdrawUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Blind write (OBL-C72). Purse handles the balance; `endowment_purse_id` is the instance ref.
    wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &update.endowment_bytes)?;

    msg!(
        "[dao_escrow::endowment_withdraw_apply_v1] Endowment updated: new total = {}",
        update.amount
    );
    Ok(())
}

/// TreasurySpendV1 instruction - executes an approved treasury spend
///
/// Money Integration: This function REQUIRES promissory_note::transfer_v1 child calls to be
/// bundled for the actual token transfer to the recipient.
fn treasury_spend_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::TreasurySpendParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::treasury_spend_v1] Processing treasury spend");

    // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
    let self_ = &calls[call_idx];
    // One child — the payment — plus, when governance is active, a second: the MultiSig approval at slot
    // 1. Same defect as `endowment_withdraw_v1` and the same fix (`OBL-C154`): the count was `!= 1` and
    // the approval was read from slot 0, which this check pins to selector 0x04.
    if self_.children_indexes.is_empty() || self_.children_indexes.len() > 2 {
        msg!(
            "[TreasurySpendV1] Error: Expected 1 child call (promissory_note::transfer_v1), plus a MultiSig approval when governance is active; got {}",
            self_.children_indexes.len()
        );
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }

    // Verify child call is promissory_note::transfer_v1 (function code 0x04)
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!(
            "[TreasurySpendV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]
        );
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify endowment exists and is in treasury mode
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::treasury_spend_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // Verify treasury mode or treasury+endowment mode
    if endowment.mode != model::DaoEscrowMode::Treasury &&
        endowment.mode != model::DaoEscrowMode::TreasuryEndowment
    {
        msg!("[dao_escrow::treasury_spend_v1] ERROR: Not a treasury mode DAO-Escrow");
        return Err(DaoEscrowError::InvalidState { expected: "Treasury mode".to_string(), actual: "Escrow mode".to_string() }.into())
    }

    // Authorisation: the endowment's group, and nothing else — the same two phantoms as
    // `endowment_withdraw_v1` (`OBL-C159` for the proposal path, `OBL-C152` for the path selector).
    //
    // The action id is a `(bulla, value, recipient_x)` triple. It must not be `proposal_id`-shaped:
    // this endpoint's gate is reached exactly when a proposal id is absent, so an id-based message
    // would be zero for every call and one approval of zero would authorise every spend of this
    // endowment forever.
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (rx, _ry) = params.recipient_pubkey.xy().expect("pk not identity");
    let action = model::governance_message(
        model::governance_role::TREASURY_SPEND,
        poseidon_hash([
            params.dao_escrow_bulla.inner(),
            pallas::Base::from(params.value),
            rx,
        ]),
    );
    require_governance_child(cid, call_idx, &calls, &endowment, 1, action)?;

    // The balance is `Purse::WithdrawV1`'s to check; the guard that stood here was `if false { … }`.
    // See the sibling note in `endowment_withdraw_v1`.

    // Create update
    let update = model::TreasurySpendUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        value: params.value,
        amount: params.value, // Purse verifies balance
        endowment_bytes: endowment.encode(),
    };

    msg!(
        "[dao_escrow::treasury_spend_v1] Treasury spend processed: {} to {:?}",
        params.value,
        params.recipient_pubkey
    );
    wasm::util::set_return_data(&[&[DaoEscrowFunction::TreasurySpendV1 as u8], &update.encode()?[..]].concat())
}

/// TreasurySpendV1 apply - update treasury totals
fn treasury_spend_apply_v1(
    cid: ContractId,
    update: model::TreasurySpendUpdateV1,
) -> ContractResult {
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;

    // Blind write (OBL-C72). Purse handles the treasury balance; `treasury_purse_id` is the ref.
    wasm::db::db_set(endowments_db, &update.dao_escrow_bulla.to_bytes(), &update.endowment_bytes)?;

    msg!(
        "[dao_escrow::treasury_spend_apply_v1] Treasury updated: new total = {}",
        update.amount
    );
    Ok(())
}

// ============================================================================
// GOVERNANCE HELPER FUNCTIONS
// ============================================================================

/// Verify a capability proof against the capability requirements registered for this DAO.
/// The approval a governance-gated endpoint requires (`OBL-C151`): the endowment's group's
/// `multisig::FinalizeV1` (0x03) at `child_slot`, over `message`.
///
/// **The threshold is not re-counted here, and deliberately.** The multisig contract's `FinalizeV1`
/// refuses below the group's threshold, and a child that fails fails this transaction — so "the group
/// requires it" is enforced where the group's record lives. A count here would be a second source of
/// truth that could disagree with the first; that is the argument `OBL-C101` records for
/// `drain_protection`.
///
/// This replaces `verify_capability_for_action`, whose capability-requirement lookup could never
/// succeed — nothing in this contract registers a requirement, so every governance call would have
/// failed `CapabilityRequirementNotRegistered("board_endowment")` even once the gates were reachable.
/// A function named for a capability that checks a MultiSig approval is itself a false statement in the
/// tree (R11), which is why it is renamed rather than kept.
///
/// Every step has its own message so a rejection names its cause: this contract's red was recorded for
/// a week as "EMPTY metadata" precisely because a refusal's reason was discarded.
fn require_governance_child(
    cid: ContractId,
    call_idx: usize,
    calls: &[DarkLeaf<ContractCall>],
    endowment: &model::DaoEscrow,
    child_slot: usize,
    message: pallas::Base,
) -> ContractResult {
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }
    if message == pallas::Base::zero() {
        msg!("[dao_escrow::require_governance_child] ERROR: the action id is zero");
        return Err(DaoEscrowError::GovernanceApprovalWrongMessage.into());
    }

    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() <= child_slot {
        msg!("[dao_escrow::require_governance_child] ERROR: expected an approval child at slot {}, got {} child(ren)",
             child_slot, this_call.children_indexes.len());
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[child_slot];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x03 {
        msg!("[dao_escrow::require_governance_child] ERROR: expected multisig::FinalizeV1 (0x03), got 0x{:02x}",
             child_call.data[0]);
        return Err(DaoEscrowError::InvalidChildCall.into())
    }

    // Validate the child targets the multisig contract (prevent cross-contract routing).
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let multisig_bytes = wasm::db::db_get(info_db, MULTISIG_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let multisig_cid: ContractId = deserialize(&multisig_bytes)?;
    // HAZOP H-11: fail closed. Do not copy `identity_cid`'s zero-means-skip, which is fail-open.
    if multisig_cid == ContractId::ZERO {
        msg!("[dao_escrow::require_governance_child] ERROR: multisig contract id is not configured");
        return Err(ContractError::IoError("multisig contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &multisig_cid)?;

    let child = dwow_multisig_contract::model::FinalizeParamsV1::decode(
        child_call.data.get(1..).unwrap_or_default(),
    )
    .map_err(|_| DaoEscrowError::InvalidChildCall)?;
    if child.group_id.inner() != endowment.multisig_group_id {
        msg!("[dao_escrow::require_governance_child] ERROR: the approval belongs to another group");
        return Err(DaoEscrowError::GovernanceApprovalForeignGroup.into())
    }
    if child.message_hash != message {
        msg!("[dao_escrow::require_governance_child] ERROR: the approval names a different action");
        return Err(DaoEscrowError::GovernanceApprovalWrongMessage.into())
    }

    msg!("[dao_escrow::require_governance_child] Governance approval verified");
    Ok(())
}

/// Verify that a governance proposal has met quorum and approval ratio requirements.
fn verify_proposal_approved(
    cid: ContractId,
    proposal_id: pallas::Base,
    dao_escrow_bulla: pallas::Base,
    value: u64,
    recipient_pubkey: &dwow_sdk::crypto::PublicKey,
) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &proposal_id.to_repr())?
        .ok_or_else(|| DaoEscrowError::ProposalNotFound("Proposal not found".to_string()))?;
    let proposal = model::Proposal::decode(&proposal_data)?;

    // Verify proposal state is Approved
    if proposal.state != model::ProposalState::Approved {
        msg!("[dao_escrow::verify_proposal] ERROR: Proposal not approved");
        return Err(DaoEscrowError::ProposalNotPending.into());
    }

    // Verify execution deadline not passed
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block > proposal.execution_deadline {
        msg!("[dao_escrow::verify_proposal] ERROR: Execution deadline passed");
        return Err(DaoEscrowError::ClaimExecutionDeadlinePassed.into());
    }

    // Verify proposal matches (value, recipient, escrow)
    if proposal.dao_escrow_bulla.inner() != dao_escrow_bulla {
        msg!("[dao_escrow::verify_proposal] ERROR: Escrow bulla mismatch");
        return Err(DaoEscrowError::ProposalNotFound("Escrow bulla mismatch".to_string()).into());
    }
    if proposal.value != value {
        msg!("[dao_escrow::verify_proposal] ERROR: Value mismatch");
        return Err(DaoEscrowError::ProposalNotFound("Value mismatch".to_string()).into());
    }
    if proposal.recipient_pubkey != *recipient_pubkey {
        msg!("[dao_escrow::verify_proposal] ERROR: Recipient mismatch");
        return Err(DaoEscrowError::ProposalNotFound("Recipient mismatch".to_string()).into());
    }

    msg!("[dao_escrow::verify_proposal] Proposal approved: {:?}", proposal_id);
    Ok(())
}

// ============================================================================
// METADATA FUNCTIONS (ZK proof public inputs)
// ============================================================================

/// Metadata for ProposeClaimV1 (0x07) — ProposeClaimV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce, claim_commit]
fn propose_claim_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::ProposeClaimParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        // Behaviour is unchanged — the empty vector is still the refusal signal the host reads as "the
        // call is rejected by design" — but the reason is now logged instead of discarded. Without
        // this, a decoder error is indistinguishable from an intended refusal, which is why this
        // contract's red carried no cause: the register could only ever record the symptom.
        Err(e) => {
            msg!("[dao_escrow::propose_claim_get_metadata] ProposeClaimParamsV1::decode failed: {:?}", e);
            return Ok(vec![])
        }
    };

    // claim_commit = poseidon_hash(DOMAIN_COIN_COMMIT, claim_id, claim_amount, claim_blind)
    //
    // The blind is the params' own (`OBL-C153`). This used to substitute
    // `capability_proof.capability_secret`, under a comment calling it a "claim_blind placeholder (needs
    // dedicated field in params)" — so the contract hashed a different preimage from the one the
    // proposer's proof was built over, and the proof could never verify.
    let claim_commit = poseidon_hash([
        pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.claim_id.inner(),
        pallas::Base::from(params.value),
        params.claim_blind,
    ]);

    // `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` with
    // `DOMAIN_TX_BINDING = 3` (`src/sdk/src/crypto/constants.rs:57`). This contract's tx pair is
    // the constant `(0, 0)` — the convention `attestation` and `identity` use, and one of the 21
    // constant bindings the register already records.
    //
    // It was `pallas::Base::zero()`, labelled "Pattern A: pass-through placeholder", which is NOT
    // that hash. The circuit constrains `tx_binding == poseidon(3, tx_commitment, tx_nonce)`, so
    // publishing a literal zero required `poseidon(3, 0, 0) == 0` — a preimage. Every proof for
    // this circuit was unsatisfiable, not merely unbound, and a count check cannot see it because
    // the vector had the right length (register OBL-C78).
    let tx_binding = dao_escrow_tx_binding(pallas::Base::zero())?;
    let tx_nonce_val = pallas::Base::zero();

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_PROPOSE_CLAIM_NS_V2.to_string(),
        // The circuit's order is [claim_commit, tx_binding, tx_nonce] — the pair last
        // (`OBL-C198`). It sat at 0,1 of 3 with `claim_commit` after it.
        vec![claim_commit, tx_binding, tx_nonce_val],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for VoteClaimV1 (0x08) — VoteClaimV2 circuit
/// Circuit constrain_instance order: [tx_binding, tx_nonce, vote_nullifier]
fn vote_claim_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match model::VoteClaimParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    let cap_secret_fp = pallas::Base::from_repr(params.capability_proof.capability_secret)
        .into_option()
        .unwrap_or(pallas::Base::zero());

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (voter_pub_x, voter_pub_y) = params.voter_pubkey.xy().expect("pk not identity");

    // vote_nullifier = poseidon_hash(DOMAIN_NULLIFIER, capability_secret, proposal_id,
    //                                 voter_pub_x, voter_pub_y)
    let vote_nullifier = poseidon_hash([
        pallas::Base::from(1u64), // DOMAIN_NULLIFIER
        cap_secret_fp,
        params.claim_id.inner(),
        voter_pub_x,
        voter_pub_y,
    ]);

    // `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` with
    // `DOMAIN_TX_BINDING = 3` (`src/sdk/src/crypto/constants.rs:57`). This contract's tx pair is
    // the constant `(0, 0)` — the convention `attestation` and `identity` use, and one of the 21
    // constant bindings the register already records.
    //
    // It was `pallas::Base::zero()`, labelled "Pattern A: pass-through placeholder", which is NOT
    // that hash. The circuit constrains `tx_binding == poseidon(3, tx_commitment, tx_nonce)`, so
    // publishing a literal zero required `poseidon(3, 0, 0) == 0` — a preimage. Every proof for
    // this circuit was unsatisfiable, not merely unbound, and a count check cannot see it because
    // the vector had the right length (register OBL-C78).
    let tx_binding = dao_escrow_tx_binding(pallas::Base::zero())?;
    let tx_nonce_val = pallas::Base::zero();

    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_VOTE_CLAIM_NS_V2.to_string(),
        // The circuit's order is [vote_nullifier, tx_binding, tx_nonce] — the pair last
        // (`OBL-C198`). It sat at 0,1 of 3 with `vote_nullifier` after it.
        vec![vote_nullifier, tx_binding, tx_nonce_val],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// Two metadata arms were removed here — `VerifyMemberCapabilityV1`'s and `ResolveDisputeV1`'s — together
// with the two doc lines the retired setter had left above `update_get_metadata`. Both belonged to
// endpoints retired with the OCap model, and both published a `resolution_commit`/`capability_commit`
// built from a param the contract itself called a placeholder ("resolution_blind placeholder (needs
// dedicated field in params)") — the same shape as `OBL-C153`, where a commitment's preimage was
// substituted because the params carried no field for it.

/// Metadata for `UpdateV1` (0x01) — the `SetGovernanceConfigV2` circuit (`OBL-C151`).
///
/// This function used to be `set_governance_config_get_metadata` and publish **five zeros** under the
/// comment "SetGovernanceConfig was migrated to MultiSig; params struct removed" — a placeholder that
/// described nothing, because the function it served is a retired no-op. The circuit was never removed,
/// and it is the one that proves ownership: it derives `owner_pub = ec_mul_base(owner_secret,
/// NULLIFIER_K)` and constrains the exposed `owner_pub_x/y` to it. `UpdateV1` is now its caller, so the
/// vector below is the real one — the circuit's `constrain_instance` order — read from the params that
/// rode with the proof.
fn update_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = model::UpdateParamsV1::decode(&self_.data[1..])?;

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (owner_pub_x, owner_pub_y) = params.owner_pubkey.xy().expect("pk not identity");
    // Constant `(0, 0)`, the convention this contract and its siblings use; the circuit constrains
    // `tx_binding == poseidon_hash(3, tx_commitment, tx_nonce)`, so a literal zero here would require a
    // preimage and make every proof unsatisfiable (the defect `OBL-C78` records for this contract).
    let tx_binding = dao_escrow_tx_binding(pallas::Base::zero())?;

    // The circuit's `constrain_instance` order: owner_pub_x, owner_pub_y, owner_nullifier, tx_binding,
    // tx_nonce.
    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_SET_GOVERNANCE_CONFIG_NS_V2.to_string(),
        vec![owner_pub_x, owner_pub_y, params.owner_nullifier, tx_binding, pallas::Base::zero()],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for `WithdrawV1` (0x03) — `SetGovernanceConfigV2`, the ownership circuit.
///
/// The same five instances `update_get_metadata` publishes, and the same circuit: this endpoint's
/// authority is the owner's proof, which is what turns its `recipient_pubkey == owner` comparison from a
/// public value checked against a public value into a check the caller had to *prove* they could pass
/// (`OBL-C152`'s last instance).
///
/// The published coordinates are the **payee's**, not a separate owner field. On this endpoint the two
/// are the same by construction — `withdraw_v1` refuses a payee that is not the owner — so publishing
/// the payee's coordinates is publishing the owner's, and the circuit's
/// `constrain_equal_base(ec_get_x(owner_pub), owner_pub_x)` then binds them to knowledge of the secret.
fn withdraw_get_metadata(
    _cid: ContractId,
    call_idx: usize,
    calls: &[dwow_sdk::dark_tree::DarkLeaf<ContractCall>],
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = model::WithdrawParamsV1::decode(&self_.data[1..])?;

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (owner_pub_x, owner_pub_y) = params.recipient_pubkey.xy().expect("pk not identity");
    let tx_binding = dao_escrow_tx_binding(pallas::Base::zero())?;

    // The circuit's `constrain_instance` order: owner_pub_x, owner_pub_y, owner_nullifier, tx_binding,
    // tx_nonce.
    let zk_public_inputs = vec![(
        crate::DAO_ESCROW_ZKAS_SET_GOVERNANCE_CONFIG_NS_V2.to_string(),
        vec![owner_pub_x, owner_pub_y, params.owner_nullifier, tx_binding, pallas::Base::zero()],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// PROPOSE CLAIM V1 (0x07)
// ============================================================================

/// ProposeClaimV1 instruction - creates a new governance proposal with capability verification
fn propose_claim_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::ProposeClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::propose_claim_v1] Processing claim proposal");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::propose_claim_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // MultiSig governance: group must be configured
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }
    // ...and the group must have approved *this* claim (`OBL-C151`). The role tag is load-bearing: the
    // vote below keys on the same `claim_id`, and a MultiSig approval is spend-once, so an untagged
    // message would make the vote's approval name nullifiers this one had already spent.
    require_governance_child(
        cid,
        call_idx,
        &calls,
        &endowment,
        0,
        model::governance_message(model::governance_role::PROPOSE_CLAIM, params.claim_id.inner()),
    )?;

    // Verify proposal does not already exist
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    if wasm::db::db_get(proposals_db, &params.claim_id.to_bytes())?.is_some() {
        return Err(DaoEscrowError::ClaimAlreadyExists("Claim already exists".to_string()).into());
    }

    // MultiSig: voting windows and claim limits are group configuration,
    // not contract parameters. Threshold verification via SignV1 + FinalizeV1.
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let voting_ends_at = current_block + 1000; // default window
    let execution_deadline = voting_ends_at + 1000;

    let update = model::ProposeClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        value: params.value,
        voting_ends_at,
        execution_deadline,
        recipient_pubkey: params.recipient_pubkey.clone(),
    };

    msg!("[dao_escrow::propose_claim_v1] Claim proposed: {:?}", params.claim_id);
    wasm::util::set_return_data(&[&[DaoEscrowFunction::ProposeClaimV1 as u8], &update.encode()[..]].concat())
}

/// ProposeClaimV1 apply - store proposal and record nullifier
fn propose_claim_apply_v1(cid: ContractId, update: model::ProposeClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;

    let proposal = model::Proposal {
        dao_escrow_bulla: update.dao_escrow_bulla,
        value: update.value,
        recipient_pubkey: update.recipient_pubkey,
        state: model::ProposalState::Pending,
        voting_ends_at: update.voting_ends_at,
        execution_deadline: update.execution_deadline,
    };

    wasm::db::db_set(proposals_db, &update.claim_id.to_bytes(), &proposal.encode())?;
    msg!("[dao_escrow::propose_claim_apply_v1] Proposal stored: {:?}", update.claim_id);
    Ok(())
}

// ============================================================================
// VOTE CLAIM V1 (0x08)
// ============================================================================

/// VoteClaimV1 instruction - casts a vote on a pending proposal
fn vote_claim_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::VoteClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::vote_claim_v1] Processing vote");

    // Verify endowment exists
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = match endowment_data {
        Some(data) => model::DaoEscrow::decode(&data)?,
        None => {
            msg!("[dao_escrow::vote_claim_v1] ERROR: Endowment not found");
            return Err(DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()).into())
        }
    };

    // MultiSig governance: group must be configured, and it is the sole authority here.
    if endowment.multisig_group_id == pallas::Base::zero() {
        return Err(DaoEscrowError::GovernanceNotActive.into());
    }

    // Load the proposal before the approval, so a refusal can name the state it actually found rather
    // than a generic "not pending". `ClaimAlreadyApproved` and `ClaimAlreadyRejected` were declared and
    // never constructed anywhere in the crate until this became their reader (`OBL-C159`).
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.claim_id.to_bytes())?
        .ok_or_else(|| DaoEscrowError::ClaimNotFound("Claim not found".to_string()))?;
    let mut proposal = model::Proposal::decode(&proposal_data)?;

    match proposal.state {
        model::ProposalState::Pending => {}
        model::ProposalState::Approved => return Err(DaoEscrowError::ClaimAlreadyApproved.into()),
        model::ProposalState::Rejected => return Err(DaoEscrowError::ClaimAlreadyRejected.into()),
        model::ProposalState::Cancelled => return Err(DaoEscrowError::ClaimAlreadyCancelled.into()),
        model::ProposalState::Executed => return Err(DaoEscrowError::ClaimAlreadyExecuted.into()),
        model::ProposalState::Expired => return Err(DaoEscrowError::ClaimExpired.into()),
    }

    // The voting window. If it has closed the proposal expires rather than being decided.
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block > proposal.voting_ends_at {
        msg!("[dao_escrow::vote_claim_v1] Voting window expired, auto-expiring proposal");
        proposal.state = model::ProposalState::Expired;
        let update = model::VoteClaimUpdateV1 {
            dao_escrow_bulla: params.dao_escrow_bulla,
            claim_id: params.claim_id,
            state: model::ProposalState::Expired,
            // No decision was taken, so no nullifier is spent — `apply` gates on the state, and this
            // path is the only one that carries a zero.
            vote_nullifier: pallas::Base::zero(),
            // Carried because apply may not read it (OBL-C72).
            proposal_bytes: proposal.encode(),
        };
        wasm::util::set_return_data(&[&[DaoEscrowFunction::VoteClaimV1 as u8], &update.encode()?[..]].concat())?;
        return Ok(())
    }

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (voter_x, voter_y) = params.voter_pubkey.xy().expect("pk not identity");

    // The approval: the group authorises *this voter* to cast *this direction* on *this claim*.
    //
    // **All three components are load-bearing (`OBL-C160`).** The message used to be
    // `governance_message(VOTE_CLAIM, claim_id)` — the claim and nothing else — while a MultiSig approval
    // is spend-once, each signature's nullifier being `H(1, member_secret, group_id, message_hash)`. Every
    // vote on a claim therefore presented the same message, so a group of `n` at threshold `t` could
    // finalise it `floor(n/t)` times: **at most one counted vote per claim**, since `t = n` is the common
    // case. And the direction was not in the message either, so one approval for "somebody votes on claim
    // X" authorised a Yes and a No alike.
    let direction = match params.vote {
        model::VoteType::Yes => pallas::Base::from(0u64),
        model::VoteType::No => pallas::Base::from(1u64),
    };
    let vote_id = poseidon_hash([params.claim_id.inner(), voter_x, voter_y, direction]);
    require_governance_child(
        cid,
        call_idx,
        &calls,
        &endowment,
        0,
        model::governance_message(model::governance_role::VOTE_CLAIM, vote_id),
    )?;

    // The voter's nullifier, derived here so that it **is** the value the `VoteClaimV2` proof publishes.
    //
    // It used to be `params.capability_proof.nullifier` taken verbatim — a caller-chosen value that the
    // proof never bound, so the tree's double-vote guard keyed on whatever the caller wrote, and
    // `capability_proof.capability_secret`, its only claimed secret, is public call data
    // (`OBL-C160`). The derivation matches `vote_claim_get_metadata` and `proof/vote_claim.zk:37-43`
    // term for term, so the key this writes is the one the proof constrains.
    let cap_secret_fp = pallas::Base::from_repr(params.capability_proof.capability_secret)
        .into_option()
        .unwrap_or(pallas::Base::zero());
    let vote_nullifier = poseidon_hash([
        pallas::Base::from(1u64), // DOMAIN_NULLIFIER
        cap_secret_fp,
        params.claim_id.inner(),
        voter_x,
        voter_y,
    ]);
    // The read stays in exec — `↓nullify` is an `exec` barb (contract-wasm-type-system.md §A.2.1) — and
    // the *write* moves to apply, carried in the update.
    let nullifiers_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &vote_nullifier.to_repr())? {
        msg!("[dao_escrow::vote_claim_v1] ERROR: Already voted");
        return Err(DaoEscrowError::AlreadyVoted.into());
    }

    // The decision. The group's `FinalizeV1` **is** the quorum: it refuses below the group's threshold,
    // and a child that fails fails this transaction, so a successful approval here means the group — not
    // one member — has decided. There is therefore no tally to count, which is why
    // `ProposalState::Approved` had a reader and no writer before this (`OBL-C159`).
    proposal.state = match params.vote {
        model::VoteType::Yes => model::ProposalState::Approved,
        model::VoteType::No => model::ProposalState::Rejected,
    };

    let update = model::VoteClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        state: proposal.state,
        vote_nullifier,
        proposal_bytes: proposal.encode(),
    };

    msg!("[dao_escrow::vote_claim_v1] Vote recorded: {:?}", params.claim_id);
    wasm::util::set_return_data(&[&[DaoEscrowFunction::VoteClaimV1 as u8], &update.encode()?[..]].concat())
}

/// VoteClaimV1 apply - record the decision and spend the voter's nullifier
fn vote_claim_apply_v1(cid: ContractId, update: model::VoteClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;

    // Spend the voter's nullifier here, not in exec. Gated on the state: the auto-expiry path takes no
    // decision and casts no vote, so nothing should be spent.
    if update.state != model::ProposalState::Expired {
        let nullifiers_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_NULLIFIERS_TREE)?;
        wasm::db::db_mark_spent(nullifiers_db, &update.vote_nullifier.to_repr())?;
    } else {
        msg!("[dao_escrow::vote_claim_apply_v1] Expired path: nullifier not spent");
    }

    // Blind write — the state was applied in exec and carried here (OBL-C72), so the record is no
    // longer read back.
    wasm::db::db_set(proposals_db, &update.claim_id.to_bytes(), &update.proposal_bytes)?;

    msg!("[dao_escrow::vote_claim_apply_v1] Proposal state updated");
    Ok(())
}

// ============================================================================
// EXECUTE CLAIM V1 (0x09)
// ============================================================================

/// ExecuteClaimV1 instruction - executes an approved proposal
fn execute_claim_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::ExecuteClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::execute_claim_v1] Executing claim");

    // Validate child call is promissory_note::transfer_v1
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into());
    }
    let child_idx = self_.children_indexes[0];
    if child_idx >= calls.len() {
        return Err(DaoEscrowError::InvalidChildrenIndexes.into())
    }
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        return Err(DaoEscrowError::InvalidChildCall.into());
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(DaoEscrowError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // Only validate if promissory_note_contract_id was configured (non-zero)
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
    let value_blind = poseidon_hash([
        pallas::Base::from(params.value),
        params.dao_escrow_bulla.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.value, value_blind)?;

    // Verify proposal is approved
    verify_proposal_approved(cid, params.proposal_id.inner(), params.dao_escrow_bulla.inner(), params.value, &params.recipient_pubkey)?;

    // Load proposal and verify not already executed
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.proposal_id.inner().to_repr())?
        .ok_or_else(|| DaoEscrowError::ProposalNotFound("Proposal not found".to_string()))?;
    let proposal = model::Proposal::decode(&proposal_data)?;

    if proposal.state == model::ProposalState::Executed {
        return Err(DaoEscrowError::ProposalAlreadyExecuted.into());
    }

    // The endowment the claim names must exist. This load is also what the `if false { … }` balance
    // guard below it used to be attached to; the guard is gone — `Purse::WithdrawV1` checks the balance
    // and a failing child fails this transaction — but the existence check is its own reason to stay: a
    // claim whose proposal names a bulla with no endowment record is a claim against nothing.
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let _endowment: model::DaoEscrow = endowment_data
        .map(|d| model::DaoEscrow::decode(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    let update = model::ExecuteClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        proposal_id: params.proposal_id,
        value: params.value,
        state: model::ProposalState::Executed,
        // Apply the state change here and carry the record — apply may not read it (OBL-C72).
        proposal_bytes: {
            let mut p = proposal;
            p.state = model::ProposalState::Executed;
            p.encode()
        },
    };

    msg!("[dao_escrow::execute_claim_v1] Claim executed");
    wasm::util::set_return_data(&[&[DaoEscrowFunction::ExecuteClaimV1 as u8], &update.encode()?[..]].concat())
}

/// ExecuteClaimV1 apply - mark proposal as executed
fn execute_claim_apply_v1(cid: ContractId, update: model::ExecuteClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    // Blind write — the state was applied in exec and carried here (OBL-C72).
    wasm::db::db_set(proposals_db, &update.proposal_id.inner().to_repr(), &update.proposal_bytes)?;

    msg!("[dao_escrow::execute_claim_apply_v1] Proposal marked as executed");
    Ok(())
}

// ============================================================================
// CANCEL CLAIM V1 (0x0d)
// ============================================================================

/// CancelClaimV1 instruction - cancels a pending proposal, authorised by the endowment's group
///
/// **The check this replaces gated nothing** (`OBL-C152`): it refused a cancellation when
/// `proposal.proposer_pubkey != params.proposer_pubkey`, and both operands are public values — the
/// proposer's key is written into the proposal record at propose time and published to receive funds — so
/// any caller who knew it could cancel any pending claim. The fixture's row now asserts the resulting
/// `Success` deliberately, so that this check fails it.
///
/// **What the old check protected, and what covers it now** (R3): it was meant to be "only the proposer
/// may withdraw their own proposal". That obligation is now carried by `require_governance_child` below —
/// the endowment's group authorises the cancellation, bound to the claim id. That is a *design*
/// consequence and is stated plainly: cancellation is a governance action rather than a proposer's
/// exclusive right, because a check that admits everyone who knows a public key is not one, and building
/// a real proposer proof means a new circuit and a codec change for this params type (its own unit).
/// `params.proposer_pubkey` is now unused by the contract and owes removal in that unit.
fn cancel_claim_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: model::CancelClaimParamsV1,
) -> ContractResult {
    msg!("[dao_escrow::cancel_claim_v1] Cancelling claim");

    // Load the endowment, which the approval check needs, and the proposal.
    let endowments_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE)?;
    let endowment_data = wasm::db::db_get(endowments_db, &params.dao_escrow_bulla.to_bytes())?;
    let endowment: model::DaoEscrow = endowment_data
        .map(|d| model::DaoEscrow::decode(&d))
        .transpose()?
        .ok_or_else(|| DaoEscrowError::DaoEscrowNotFound("Endowment not found".to_string()))?;

    require_governance_child(
        cid,
        call_idx,
        &calls,
        &endowment,
        0,
        model::governance_message(
            model::governance_role::CANCEL_CLAIM,
            params.claim_id.inner(),
        ),
    )?;

    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    let proposal_data = wasm::db::db_get(proposals_db, &params.claim_id.to_bytes())?
        .ok_or_else(|| DaoEscrowError::ClaimNotFound("Claim not found".to_string()))?;
    let proposal = model::Proposal::decode(&proposal_data)?;

    // Verify proposal is still pending
    if proposal.state != model::ProposalState::Pending {
        msg!("[dao_escrow::cancel_claim_v1] ERROR: Claim not pending");
        return Err(DaoEscrowError::ClaimNotPending.into());
    }

    let update = model::CancelClaimUpdateV1 {
        dao_escrow_bulla: params.dao_escrow_bulla,
        claim_id: params.claim_id,
        state: model::ProposalState::Cancelled,
        // Apply the state change here and carry the record — apply may not read it (OBL-C72).
        proposal_bytes: {
            let mut p = proposal;
            p.state = model::ProposalState::Cancelled;
            p.encode()
        },
    };

    msg!("[dao_escrow::cancel_claim_v1] Claim cancelled");
    wasm::util::set_return_data(&[&[DaoEscrowFunction::CancelClaimV1 as u8], &update.encode()?[..]].concat())
}

/// CancelClaimV1 apply - update proposal state to cancelled
fn cancel_claim_apply_v1(cid: ContractId, update: model::CancelClaimUpdateV1) -> ContractResult {
    let proposals_db = wasm::db::db_lookup(cid, DAO_ESCROW_CONTRACT_PROPOSALS_TREE)?;
    // Blind write — the state was applied in exec and carried here (OBL-C72).
    wasm::db::db_set(proposals_db, &update.claim_id.to_bytes(), &update.proposal_bytes)?;

    msg!("[dao_escrow::cancel_claim_apply_v1] Proposal cancelled");
    Ok(())
}

// The `SET GOVERNANCE CONFIG V1 (0x0e)` section and a `DeactivateCapabilityRequirementV1 (0x10)` section
// were removed here. The first had already been reduced to a comment block by an earlier migration — a
// block that described a function whose selector is no longer in `DaoEscrowFunction` at all; the second
// retired with the capability registry. That leaves ten endpoints, and `cancel_claim_v1` is the last of
// them in this file.
