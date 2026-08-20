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

/// A pre-issued PN capability: (coin_commitment, leaf_pos, merkle_path, asset_id, coin_blind).
pub type PnNote = (pallas::Base, u64, Vec<MerkleNode>, pallas::Base, pallas::Base);

/// Build a `promissory_note::transfer_v1` (0x04) child spending an issued note.
///
/// One input → one output, both value `value`. The output's value commitment is
/// `pedersen(value, fp_mod_fv(blind_seed))`, which a parent contract reproduces via
/// `validate_child_value_commit(child, value, blind_seed)`.
///
/// `output_coin_blind` and `output_spend_hook` are the only two knobs that varied
/// across the six former per-spec copies; pass `blind_seed` / `zero` for the common
/// case, or a constant (e.g. 7) / a hook where a spec did so.
#[allow(clippy::too_many_arguments)]
pub fn pn_transfer_child(
    note: &PnNote,
    value: u64,
    blind_seed: pallas::Base,
    output_coin_blind: pallas::Base,
    output_spend_hook: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, asset_id, coin_blind) = note;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());

    let input = TransferCallInput {
        value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: *coin_blind,
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
        coin_blind: output_coin_blind,
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
pub fn pn_transfer_payout_child(
    note: &PnNote,
    locked_value: u64,
    payout: u64,
    blind_seed: pallas::Base,
) -> dwow_core::Result<ChildCall> {
    let (_, pos, path, asset_id, coin_blind) = note;
    let change = locked_value - payout;
    let value_blind = Blind(fp_mod_fv(blind_seed).unwrap());

    let input = TransferCallInput {
        value: locked_value,
        asset_id: *asset_id,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: *coin_blind,
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
        // Distinct from `blind_seed` (which is also the value_blind seed): a
        // same-bet lock child with value == payout would otherwise reuse it and
        // collide (PN DuplicateCoin).
        coin_blind: poseidon_hash([blind_seed, pallas::Base::from(payout)]),
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
            coin_blind: poseidon_hash([blind_seed, pallas::Base::from(change)]),
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
    })
}
