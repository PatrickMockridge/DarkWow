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

//! Capability-based wallet architecture types.
//!
//! Every authorization the user holds is modeled as a capability:
//! - Coins
//! - Contract roles (state + role + instance)
//! - Identity credentials
//! - DAO memberships
//!
//! Actions require capabilities, consume some (nullifiers), and produce new ones.

use crate::crypto::ContractId;

/// Unique identifier for a capability instance.
///
/// Derived deterministically from `(contract_id, capability_type, instance_id)`
/// via Poseidon hash so instances can be matched without storing them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CapabilityId(pub [u8; 32]);

impl CapabilityId {
    /// Derive a capability ID from contract, type discriminant, and instance key.
    ///
    /// Uses Poseidon hash over `(contract_id_inner, capability_type, instance_id_elem)`
    /// where `instance_id_elem` is derived from the first 32 bytes of `instance_id`.
    pub fn derive(
        contract_id: ContractId,
        capability_type: u8,
        instance_id: &[u8],
    ) -> Self {
        use crate::crypto::poseidon_hash;
        use crate::pasta::{pallas, group::ff::PrimeField};

        let mut id_bytes = [0u8; 32];
        let len = instance_id.len().min(32);
        id_bytes[..len].copy_from_slice(&instance_id[..len]);
        let instance_elem = pallas::Base::from_repr(id_bytes)
            .into_option()
            .unwrap_or_default();

        let hash = poseidon_hash([
            contract_id.inner(),
            pallas::Base::from(capability_type as u64),
            instance_elem,
        ]);
        CapabilityId(hash.to_repr())
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", bs58::encode(&self.0).into_string())
    }
}

/// How the user holds this capability — determines how the resolver derives it
/// from on-chain facts.
#[derive(Clone, Debug)]
pub enum CapabilitySource {
    /// Spendable coin — user knows the secret key.
    Coin {
        /// On-chain coin identifier (commitment hash).
        coin_id: [u8; 32],
    },
    /// Contract role — user's pubkey matches a stored role pubkey for
    /// a contract instance in a specific state.
    Role {
        /// Contract state name (e.g. "Created", "Funded").
        state: String,
        /// Role name (e.g. "Creator", "Counterparty").
        role: String,
        /// Instance identifier (escrow_id, job_id, tender_id, etc.).
        instance_id: [u8; 32],
    },
    /// Identity credential — user holds a ZK credential that is not revoked.
    ZkCredential {
        /// Credential identifier from the Identity contract.
        credential_id: [u8; 32],
        /// The nullifier bound to this credential issuance.
        nullifier: [u8; 32],
        /// Whether the credential has been revoked on-chain.
        revoked: bool,
    },
    /// DAO-Escrow membership — user paid the premium and it hasn't expired.
    Membership {
        /// Membership note identifier.
        membership_id: [u8; 32],
        /// Block height when membership expires.
        expiry: u64,
    },
    /// Generic capability — discovered via AEAD decryption from any contract.
    /// Auto-resolved by the capability kernel without per-contract code.
    Generic {
        /// Note type (e.g. "NativeToken", "unknown").
        note_type: String,
        /// Block height where discovered.
        block_height: u32,
    },
}

/// A capability the user holds.
#[derive(Clone, Debug)]
pub struct Capability {
    /// Unique identifier for this capability instance.
    pub id: CapabilityId,
    /// Which contract this capability belongs to.
    pub contract_id: ContractId,
    /// Human-readable description for wallet display.
    pub description: String,
    /// Where this capability comes from — how the resolver derives it.
    pub source: CapabilitySource,
    /// True if exercising this capability consumes it (nullifier).
    /// False if reusable (e.g. Identity credential, DAO membership).
    pub consumable: bool,
    /// Block height when this capability expires (None = never).
    pub expires_at: Option<u64>,
}

/// A capability gained by executing an action.
#[derive(Clone, Debug)]
pub struct CapabilityOutput {
    /// Unique identifier for the new capability.
    pub id: CapabilityId,
    /// Human-readable description.
    pub description: String,
}

/// Boolean expression over capabilities required to authorize an action.
#[derive(Clone, Debug)]
pub enum CapabilityExpression {
    /// Any one of these capabilities is sufficient (OR).
    Any(Vec<CapabilityId>),
    /// All of these capabilities are required (AND).
    All(Vec<CapabilityId>),
    /// Must NOT hold this capability (e.g. "not already voted").
    Not(Box<CapabilityExpression>),
    /// A voting threshold — `count` of `capabilities` must be exercised
    /// before this expression is satisfied.
    Threshold {
        /// The capabilities being counted (e.g. member votes).
        capabilities: Vec<CapabilityId>,
        /// Required count of exercised capabilities (e.g. quorum).
        count: u32,
        /// Total number of eligible voters.
        total: u32,
    },
}

/// An action the user can take — a contract function they are authorized to call.
#[derive(Clone, Debug)]
pub struct Action {
    /// Function opcode byte.
    pub function_id: u8,
    /// Human-readable function name (e.g. "FundEscrow").
    pub name: String,
    /// Which contract this action targets.
    pub contract_id: ContractId,
    /// Human-readable description for wallet display.
    pub description: String,
    /// Capabilities required to authorize this action.
    pub requires: CapabilityExpression,
    /// Capabilities consumed when this action executes (nullifiers).
    pub consumes: Vec<CapabilityId>,
    /// Capabilities gained after successful execution.
    pub produces: Vec<CapabilityOutput>,
}

/// A contract's capability descriptor — declares what capabilities its actions
/// require, consume, and produce.
///
/// Each contract provides one descriptor. The wallet's CapabilityResolver
/// loads descriptors, derives the user's current capabilities from on-chain
/// facts, and computes available actions.
#[derive(Clone, Debug)]
pub struct CapabilityDescriptor {
    /// The contract this descriptor belongs to.
    pub contract_id: ContractId,
    /// Human-readable contract name.
    pub name: String,
    /// All actions this contract supports, with their capability requirements.
    pub actions: Vec<Action>,
}

impl CapabilityDescriptor {
    /// Create a new empty descriptor for a contract.
    pub fn new(contract_id: ContractId, name: &str) -> Self {
        CapabilityDescriptor { contract_id, name: name.to_string(), actions: vec![] }
    }
}
