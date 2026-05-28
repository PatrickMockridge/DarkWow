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

//! Escrow cancel_v1 client builder.
//!
//! CancelV1 uses on-chain Schnorr signature verification (not ZK proofs).
//! The buyer signs `poseidon_hash([escrow_id, Base::zero()])` to prove
//! knowledge of the buyer's secret key.

use dwow_sdk::{
    crypto::{poseidon_hash, schnorr::SchnorrSecret, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_serial::serialize;

use crate::model::CancelEscrowParamsV1;

/// Builder for constructing a CancelEscrowV1 call.
pub struct CancelEscrowV1Builder {
    escrow_id: pallas::Base,
    buyer_pubkey: PublicKey,
    buyer_secret: SecretKey,
}

impl CancelEscrowV1Builder {
    /// Create a new cancel escrow builder.
    pub fn new(
        escrow_id: pallas::Base,
        buyer_pubkey: PublicKey,
        buyer_secret: SecretKey,
    ) -> Self {
        Self { escrow_id, buyer_pubkey, buyer_secret }
    }

    /// Build the params with a Schnorr signature over (escrow_id, domain_separator).
    pub fn build(self) -> CancelEscrowParamsV1 {
        let signature_msg =
            serialize(&poseidon_hash([self.escrow_id, pallas::Base::zero()]));
        let signature = self.buyer_secret.sign(&signature_msg);

        CancelEscrowParamsV1 {
            escrow_id: self.escrow_id,
            buyer_pubkey: self.buyer_pubkey,
            buyer_secret: self.buyer_secret.inner(),
            signature,
        }
    }
}
