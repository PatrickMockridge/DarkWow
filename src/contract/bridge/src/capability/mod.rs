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

//! Object Capability Module
//!
//! Implements the Object Capability model for the universal bridge.
//!
//! ## OCap Principles
//!
//! 1. **Capabilities are derived, never assigned**
//! 2. **No threshold signing / VSS**
//! 3. **User alone authorizes via secret knowledge**
//!
//! ## Types
//!
//! - `ReceiveCapability`: Authority to receive on a destination chain
//! - `SendCapability`: Authority to send, bound to a receive capability
//! - `SendCapabilityNullifier`: Revealed on spend, proves ownership
//!
//! ## Derivation
//!
//! ```ignore
//! ReceiveCap = poseidon_hash(recipient_pub, dest_chain, nonce)
//! SendCap = poseidon_hash(secret, ReceiveCap.hash, amount)
//! SendCapabilityNullifier = poseidon_hash(SendCap.hash)
//! ```
//!
//! ## Security
//!
//! - Knowing ReceiveCap doesn't reveal the secret
//! - Knowing SendCap proves knowledge of secret
//! - Revealing SendCapabilityNullifier proves SendCap ownership without revealing secret

pub mod derive;

pub use derive::{
    derive_commitment, derive_nullifier, derive_receive_capability, derive_send_capability,
    ReceiveCapability, SendCapability, SendCapabilityNullifier,
};
