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

//! Promissory Note Client API
//!
//! This module implements the client-side API for Promissory Note contract interaction.
//!
//! Key design: Value commitments use Pedersen (additively homomorphic)
//! for cross-proof value conservation in the entrypoint.

use dwow_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, Blind, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::model::{Coin, Output};

/// `PromissoryNote::TokenMintV1` API - create new token type
pub mod token_mint_v1;

/// `PromissoryNote::MintV1` API - mint tokens of existing type
pub mod mint_v1;

/// `PromissoryNote::BurnV1` API
pub mod burn_v1;

/// `PromissoryNote::TransferV1` API
pub mod transfer_v1;

/// PromissoryNote holds the inner attributes of a Coin.
///
/// Note that value_blind is pallas::Scalar (Pedersen blinding), not pallas::Base.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct PromissoryNote {
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: pallas::Base,
    /// Spend hook used for protocol-owned liquidity
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value (Pedersen commitment)
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID
    pub token_blind: pallas::Base,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}

/// Verify a received coin by decrypting the AEAD note and checking all commitments.
///
/// This is the recipient-side verification path: given an Output from a TransferV1
/// or OtcSwapV1 transaction, the recipient uses their `SecretKey` to:
///
/// 1. **Decrypt** the AEAD note (only the intended recipient can do this — the
///    Diffie-Hellman shared secret requires the recipient's secret key).
/// 2. **Verify the coin commitment** matches the decrypted attributes.
/// 3. **Verify the value commitment** matches the decrypted value and blind.
///
/// On success, returns the verified `PromissoryNote` with all coin attributes.
/// On failure (wrong recipient, corrupted data, mismatched commitments), returns an error.
pub fn verify_received_coin(output: &Output, secret: &SecretKey) -> Result<PromissoryNote, dwow_sdk::error::ContractError> {
    // 1. Decrypt the AEAD note. Only the intended recipient can do this —
    //    the AEAD encryption uses Diffie-Hellman with the recipient's public key.
    let note: PromissoryNote = output.note.decrypt(secret)?;

    // 2. Derive the recipient's address (field element) from their public key.
    //    The coin commitment uses poseidon_hash([public_key_x]) as the "public_key" field,
    //    not the EC point itself — promissory_note keeps public keys as Poseidon-derived elements
    //    for ZK circuit simplicity.
    let recipient_pub = PublicKey::from_secret(*secret);
    let recipient_address = poseidon_hash([recipient_pub.x()]);

    // 3. Verify coin commitment matches the decrypted attributes.
    //    This proves the coin was correctly formed and the note wasn't tampered with.
    let expected_coin = Coin::from_attributes(
        recipient_address,
        note.value,
        note.token_id,
        note.spend_hook,
        note.user_data,
        note.coin_blind,
    );
    if expected_coin != output.coin {
        return Err(dwow_sdk::error::ContractError::Custom(
            crate::error::PromissoryNoteError::ValueMismatch as u32,
        ));
    }

    // 4. Verify Pedersen value_commit matches the decrypted value and blind.
    let value_blind = Blind(note.value_blind);
    let expected_value_commit = pedersen_commitment_u64(note.value, value_blind);
    if expected_value_commit != output.value_commit {
        return Err(dwow_sdk::error::ContractError::Custom(
            crate::error::PromissoryNoteError::ValueMismatch as u32,
        ));
    }

    Ok(note)
}
