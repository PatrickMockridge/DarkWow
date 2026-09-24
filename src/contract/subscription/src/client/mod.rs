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

//! Subscription contract client module

use dwow_sdk::pasta::pallas;

pub mod zkbins;

pub mod cancel;
pub mod renew;
pub mod subscribe;
pub mod update_usage;
pub mod verify_access;

/// The transaction binding a subscription circuit instances:
/// `poseidon_hash([3, tx_commitment, tx_nonce])` under the named domain constant (`OBL-C78`).
///
/// **Every five of this contract's circuits instance it and its nonce**, and each metadata arm used to
/// publish a literal `Base::zero()` in its place — a value no circuit derives for any input, so no
/// proof could satisfy the instance column and every one of those instructions was unbuildable. The
/// derivation lives here so the client and the params cannot disagree about it: the client passes the
/// result to `compute_public_inputs`, the params carry it to the host, and the host publishes it as
/// the vector the proof is checked against. This is `tender`'s worked template
/// (`tender/src/client/mod.rs:59`), copied rather than re-derived.
pub fn tx_binding_of(tx_commitment: &pallas::Base, tx_nonce: &pallas::Base) -> pallas::Base {
    dwow_sdk::crypto::poseidon_hash([
        dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING,
        *tx_commitment,
        *tx_nonce,
    ])
}