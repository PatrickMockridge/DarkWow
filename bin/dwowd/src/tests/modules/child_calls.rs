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

//! Shared promissory-note child-call builders for heavyweight specs.
//!
//! Centralizes the `promissory_note::transfer_v1` (0x04) child-call construction
//! that was previously duplicated per-spec (`otc_swap`, `auction`, `escrow`,
//! `bridge`, `stablecoin`, `dex`) and adds the multi-output "payout + change"
//! builder the gambling settlement endpoints need (baccarat / roulette / slot /
//! lottery).
//!
//! Used by: gambling + 6 PN-consuming specs.
//! Spec: RG-MODULAR (2+ contracts).

use dwow_contract_test_harness::harness::PromissoryNoteHarness;
use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
use dwow_sdk::{
    crypto::{
        poseidon_hash, util::fp_mod_fv, Blind, MerkleNode, PublicKey, ScalarBlind, SecretKey,
        PROMISSORY_NOTE_CONTRACT_ID,
    },
    pasta::pallas,
};

use crate::tests::uniform_runner::ChildCall;

/// A pre-issued PN capability: (commitment, leaf_pos, merkle_path, asset_id, commitment_blind).
pub type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// Build a `promissory_note::transfer_v1` (0x04) child spending an issued note.
///
/// One input → one output, both value `value`. **Two blinds are in play and they are not
/// interchangeable:**
///
/// * the **value** blind is `fp_mod_fv(blind_seed)`, so the output's value commitment is
///   `pedersen(value, fp_mod_fv(blind_seed))` — the quantity a parent contract reproduces
///   with `validate_child_value_commit(child, value, blind_seed)`. A caller must match the
///   parent's own derivation exactly or the call is refused `ValueMismatch`.
/// * the **leaf** blind is derived here, from the note this child spends, and is
///   deliberately *not* a parameter. `promissory_note::transfer_v1` refuses an output
///   whose commitment the note tree already holds, and that commitment is built from the
///   leaf blind — so a caller that passed `blind_seed` for both got a leaf fixed by
///   `(seed, value)`, and the second same-valued child for the same object collided
///   (`[transfer_v1] Error: Duplicate commitment in output 0`, `Custom(14)`). Deriving
///   from the spent note makes the leaf per-call by construction, since a note is spent
///   once. See `OBL-C192`.
///
/// `output_spend_hook` stays a caller choice: it is a leaf *attribute* a parent contract
/// may read (e.g. `stablecoin` reads the child's `output.spend_hook`).
pub fn pn_transfer_child(
    note: &PnNote,
    value: u64,
    blind_seed: pallas::Base,
    output_spend_hook: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let (note_commitment, pos, path, asset_id, commitment_blind) = note;
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
        spend_hook: output_spend_hook,
        user_data: pallas::Base::zero(),
        commitment_blind: poseidon_hash([blind_seed, *note_commitment]),
    };

    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], vec![output], Some(vec![value_blind]))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
        children: vec![],
    })
}

/// Build a "payout + change" child for an entropy-dependent settlement endpoint.
///
/// The input note has value `locked_value`; the primary output pays `payout`
/// (its value commitment is `pedersen(payout, fp_mod_fv(blind_seed))`, matching
/// the parent's `validate_child_value_commit(child, payout, blind_seed)`), and the
/// remainder `locked_value - payout` is returned as a change output.
///
/// The change output's value blind MUST be zero: `transfer_with_value_blinds` maps
/// `value_blinds` positionally (input `i` and output `i` share `value_blinds[i]`),
/// so Pedersen conservation over the two outputs
/// (`pedersen(locked, b0) == pedersen(payout, b0) + pedersen(change, b1)`) holds only
/// when `b1 = 0`.
///
/// **The *leaf* blinds here are derived, exactly as `pn_transfer_child` derives its own**
/// (`OBL-C192`), from the spent note plus the output's index. Deriving from the value
/// instead — as this helper used to — left two holes: two same-valued payouts from
/// *different* notes met on the leaf, and when `payout == change` the two outputs of a
/// *single* call met on it, which nothing rejects because the contract's duplicate check
/// consults the tree rather than the batch being built. The *value* blinds are untouched:
/// they are the parent's quantity, and the change blind must stay zero.
pub fn pn_transfer_payout_child(
    note: &PnNote,
    locked_value: u64,
    payout: u64,
    blind_seed: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let (note_commitment, pos, path, asset_id, commitment_blind) = note;
    let change = locked_value - payout;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());

    let input = TransferCallInput {
        value: locked_value,
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

    let recipient = poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(200u64)]);
    let recipient_pub = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(200u64)));

    let payout_out = TransferCallOutput {
        recipient,
        recipient_pub,
        value: payout,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        // Distinct from `blind_seed` (which is also the value_blind seed), from the
        // sibling output, and from every other call's child. Two terms do that work:
        // the spent note (per-call, since a note is spent once) and the output
        // *index* — not the value. Keying on the value leaves a hole: when
        // `payout == change` both outputs would carry the same leaf blind AND the
        // same value, i.e. the same leaf, and nothing rejects a within-call
        // duplicate — the contract's duplicate loop consults the tree, not the
        // batch it is building.
        commitment_blind: poseidon_hash([
            blind_seed,
            *note_commitment,
            pallas::Base::from(0u64),
        ]),
    };

    let mut outputs = vec![payout_out];
    let mut blinds = vec![value_blind];
    if change > 0 {
        let change_out = TransferCallOutput {
            recipient,
            recipient_pub,
            value: change,
            asset_id: *asset_id,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            commitment_blind: poseidon_hash([
                blind_seed,
                *note_commitment,
                pallas::Base::from(1u64),
            ]),
        };
        outputs.push(change_out);
        blinds.push(ScalarBlind::from_u64(0));
    }

    let pn = PromissoryNoteHarness::spawn();
    let child = pn
        .transfer_with_value_blinds(vec![input], outputs, Some(blinds))
        .map_err(|e| dwow_core::Error::Custom(format!("{e}")))?;
    Ok(ChildCall {
        contract_id: *PROMISSORY_NOTE_CONTRACT_ID,
        call_data: child.call_data,
        proofs: child.proofs,
        children: vec![],
    })
}
