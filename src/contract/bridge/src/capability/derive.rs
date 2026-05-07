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

//! Capability Derivation
//!
//! Implements the Object Capability model for the bridge.
//!
//! ## OCap Model
//!
//! **Principles:**
//! 1. Capabilities are derived, never assigned
//! 2. No threshold signing / VSS
//! 3. User alone authorizes via secret knowledge
//!
//! ## Derivation
//!
//! ```ignore
//! // For receiving on chain B:
//! ReceiveCap = poseidon_hash(recipient_pub_x, recipient_pub_y, dest_chain, nonce)
//!
//! // For sending from chain A:
//! SendCap = poseidon_hash(sender_secret, ReceiveCap, amount)
//!
//! // Nullifier (revealed on spend):
//! Nullifier = poseidon_hash(SendCap)
//! ```

use dwow_sdk::{crypto::poseidon_hash, pasta::pallas};

use crate::chain_handler::ChainId;

/// A capability for receiving funds on a destination chain
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveCapability {
    /// Hash of the capability
    pub hash: pallas::Base,
    /// Destination chain
    pub chain: ChainId,
    /// Recipient public key X
    pub recipient_x: pallas::Base,
    /// Recipient public key Y
    pub recipient_y: pallas::Base,
    /// Fresh nonce for this capability
    pub nonce: u64,
}

/// A capability for sending funds, bound to a receive capability
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendCapability {
    /// Hash of the capability
    pub hash: pallas::Base,
    /// Chain where funds can be sent
    pub chain: ChainId,
    /// The receive capability this is bound to
    pub receive_cap: ReceiveCapability,
    /// Amount to send
    pub amount: u64,
}

/// A nullifier that proves ownership of a send capability
///
/// Revealed when spending, proves ownership without revealing secret
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nullifier {
    /// Hash of the nullifier
    pub hash: pallas::Base,
    /// The send capability being spent
    pub send_cap_hash: pallas::Base,
}

/// Derive a receive capability for a recipient on a destination chain
///
/// ReceiveCap = poseidon_hash(recipient_pub_x, recipient_pub_y, dest_chain, nonce)
///
/// This capability allows the recipient to receive funds on the
/// destination chain. Only they can derive the corresponding
/// send capability using their secret.
pub fn derive_receive_capability(
    recipient_x: pallas::Base,
    recipient_y: pallas::Base,
    dest_chain: ChainId,
    nonce: u64,
) -> ReceiveCapability {
    let cap_hash = poseidon_hash([
        recipient_x,
        recipient_y,
        pallas::Base::from(dest_chain.as_u8() as u64),
        pallas::Base::from(nonce),
    ]);

    ReceiveCapability { hash: cap_hash, chain: dest_chain, recipient_x, recipient_y, nonce }
}

/// Derive a send capability bound to a receive capability
///
/// SendCap = poseidon_hash(secret, ReceiveCap.hash, amount)
///
/// This binds the secret to the receive capability, creating
/// a send authority. The secret holder can reveal this to
/// send funds.
pub fn derive_send_capability(
    secret: pallas::Base,
    receive_cap: &ReceiveCapability,
    amount: u64,
) -> SendCapability {
    let cap_hash = poseidon_hash([secret, receive_cap.hash, pallas::Base::from(amount)]);

    SendCapability { hash: cap_hash, chain: receive_cap.chain, receive_cap: receive_cap.clone(), amount }
}

/// Derive a nullifier from a send capability
///
/// Nullifier = poseidon_hash(SendCap.hash)
///
/// Revealing the nullifier proves ownership of the send capability
/// without revealing the secret itself.
pub fn derive_nullifier(send_cap: &SendCapability) -> Nullifier {
    Nullifier { hash: poseidon_hash([send_cap.hash]), send_cap_hash: send_cap.hash }
}

/// Derive a commitment for a deposit
///
/// Commitment = poseidon_hash(secret, amount, ReceiveCap.hash)
///
/// This commits to the deposit parameters without revealing them.
pub fn derive_commitment(
    secret: pallas::Base,
    amount: u64,
    receive_cap: &ReceiveCapability,
) -> pallas::Base {
    poseidon_hash([secret, pallas::Base::from(amount), receive_cap.hash])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receive_capability_derivation() {
        let x = pallas::Base::from(1);
        let y = pallas::Base::from(2);
        let nonce = 42;

        let cap1 = derive_receive_capability(x, y, ChainId::Ethereum, nonce);
        let cap2 = derive_receive_capability(x, y, ChainId::Ethereum, nonce);

        // Same inputs should produce same capability
        assert_eq!(cap1.hash, cap2.hash);
        assert_eq!(cap1.chain, cap2.chain);

        // Different nonce should produce different capability
        let cap3 = derive_receive_capability(x, y, ChainId::Ethereum, nonce + 1);
        assert_ne!(cap1.hash, cap3.hash);
    }

    #[test]
    fn test_send_capability_binding() {
        let secret = pallas::Base::from(12345);
        let x = pallas::Base::from(1);
        let y = pallas::Base::from(2);
        let nonce = 42;
        let amount = 1_000_000;

        let receive_cap = derive_receive_capability(x, y, ChainId::Ethereum, nonce);
        let send_cap = derive_send_capability(secret, &receive_cap, amount);

        assert_eq!(send_cap.chain, receive_cap.chain);
        assert_eq!(send_cap.amount, amount);
        assert_eq!(send_cap.receive_cap.hash, receive_cap.hash);
    }

    #[test]
    fn test_nullifier_derivation() {
        let secret = pallas::Base::from(12345);
        let x = pallas::Base::from(1);
        let y = pallas::Base::from(2);
        let nonce = 42;
        let amount = 1_000_000;

        let receive_cap = derive_receive_capability(x, y, ChainId::Ethereum, nonce);
        let send_cap = derive_send_capability(secret, &receive_cap, amount);
        let nullifier = derive_nullifier(&send_cap);

        // Nullifier should be derived from send capability
        assert!(nullifier.hash != send_cap.hash);
        assert_eq!(nullifier.send_cap_hash, send_cap.hash);
    }

    #[test]
    fn test_commitment_derivation() {
        let secret = pallas::Base::from(12345);
        let x = pallas::Base::from(1);
        let y = pallas::Base::from(2);
        let nonce = 42;
        let amount = 1_000_000;

        let receive_cap = derive_receive_capability(x, y, ChainId::Ethereum, nonce);
        let commit1 = derive_commitment(secret, amount, &receive_cap);
        let commit2 = derive_commitment(secret, amount, &receive_cap);

        // Same inputs should produce same commitment
        assert_eq!(commit1, commit2);

        // Different amount should produce different commitment
        let commit3 = derive_commitment(secret, amount + 1, &receive_cap);
        assert_ne!(commit1, commit3);
    }
}
