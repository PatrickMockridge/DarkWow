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
//! - Notes (native tokens + promissory notes)
//! - Contract roles (state + role + instance)
//! - Identity credentials
//! - DAO memberships
//!
//! Actions require capabilities, consume some (nullifiers), and produce new ones.

use crate::crypto::ContractId;
use crate::error::ContractError;

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
    ///
    /// Returns an error if `instance_id` encodes a non-canonical field element
    /// (value >= Pallas base field modulus). For typical callers using small
    /// instance IDs this is unreachable — same guard as SecretKey::derive_instance.
    pub fn derive(
        contract_id: ContractId,
        capability_type: u8,
        instance_id: &[u8],
    ) -> Result<Self, ContractError> {
        use crate::crypto::poseidon_hash;
        use crate::pasta::{pallas, group::ff::PrimeField};

        let mut id_bytes = [0u8; 32];
        let len = instance_id.len().min(32);
        id_bytes[..len].copy_from_slice(&instance_id[..len]);
        let instance_elem = match pallas::Base::from_repr(id_bytes).into_option() {
            Some(e) => e,
            None => return Err(ContractError::IoError(
                "Non-canonical instance_id in CapabilityId::derive".into()
            )),
        };

        let hash = poseidon_hash([
            contract_id.inner(),
            pallas::Base::from(capability_type as u64),
            instance_elem,
        ]);
        Ok(CapabilityId(hash.to_repr()))
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
    /// Spendable note — user knows the secret key.
    Note {
        /// On-chain note identifier (commitment hash).
        note_id: [u8; 32],
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

// ============================================================================
// Capability Type Construction — Rust mirror of proofs/lean/Capability/
// ============================================================================
// The types below correspond directly to the Lean4 calculus of constructions:
//   Barb          ↔ Types.lean (inductive Barb)
//   PrimitiveType ↔ Types.lean (primitive type definitions)
//   CapabilityType ↔ Composition.lean (CapabilityType r s)
//   wallet_construct ↔ Wallet.lean (walletConstruct)
//
// This is the anti-scope-drift foundation. Any capability the wallet
// constructs must type-check against these definitions.

/// Observable actions a process can exhibit.
/// Mirrors the Lean4 `inductive Barb` in Types.lean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Barb {
    /// ↓spend — can authorize value transfer
    Spend,
    /// ↓nullify — can prevent replay
    Nullify,
    /// ↓commit — can create a capability (Poseidon commitment)
    Commit,
    /// ↓prove — can satisfy a ZK predicate
    Prove,
    /// ↓verify — can check a ZK proof or signature
    Verify,
    /// ↓dispatch — can route a contract call
    Dispatch,
    /// ↓gate — can authorize a spend hook
    Gate,
    /// ↓denominate — can identify an asset type
    Denominate,
    /// ↓prove-inclusion — can prove set membership (Merkle proof)
    ProveInclusion,
    /// ↓encrypt — can produce ciphertext for a recipient
    Encrypt,
    /// ↓derive — can produce scoped sub-keys
    Derive,
    /// ↓discover — can detect own outputs via AEAD
    Discover,
    /// ↓mine — can produce a valid coinbase
    Mine,
    /// ↓view — can decrypt notes (possesses ViewingKey)
    View,
}

/// Cryptographic primitive types per type-system.md §8.1.
/// Each variant has a fixed barb set matching the Lean4 Types.lean definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Primitive {
    SecretKey,
    PublicKey,
    Nullifier,
    Coin,
    ContractId,
    FuncId,
    TokenId,
    MerkleNode,
    OwnedSecretKey,
    MiningRecipient,
}

impl Primitive {
    /// The barbs this primitive type exhibits.
    /// Must match type-system.md §8.1 and Lean4 Types.lean exactly.
    pub fn barbs(self) -> &'static [Barb] {
        match self {
            Primitive::SecretKey       => &[Barb::Spend, Barb::Derive],
            Primitive::PublicKey       => &[Barb::Verify, Barb::Encrypt],
            Primitive::Nullifier       => &[Barb::Nullify],
            Primitive::Coin            => &[Barb::Commit],
            Primitive::ContractId      => &[Barb::Dispatch],
            Primitive::FuncId          => &[Barb::Gate],
            Primitive::TokenId         => &[Barb::Denominate],
            Primitive::MerkleNode      => &[Barb::ProveInclusion],
            Primitive::OwnedSecretKey  => &[Barb::Spend],
            Primitive::MiningRecipient => &[Barb::Spend, Barb::Mine],
        }
    }

    /// Canonical name for manifest declaration and display.
    /// Matches the enum variant name exactly.
    pub fn name(self) -> &'static str {
        match self {
            Primitive::SecretKey       => "SecretKey",
            Primitive::PublicKey       => "PublicKey",
            Primitive::Nullifier       => "Nullifier",
            Primitive::Coin            => "Coin",
            Primitive::ContractId      => "ContractId",
            Primitive::FuncId          => "FuncId",
            Primitive::TokenId         => "TokenId",
            Primitive::MerkleNode      => "MerkleNode",
            Primitive::OwnedSecretKey  => "OwnedSecretKey",
            Primitive::MiningRecipient => "MiningRecipient",
        }
    }

    /// Parse a primitive from its canonical name (as declared in a manifest).
    ///
    /// Unknown names return `None` — non-fatal by design: a manifest that names
    /// a future primitive this SDK version does not know simply does not
    /// contribute that primitive, rather than failing the scan (forward-compat).
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "SecretKey"       => Primitive::SecretKey,
            "PublicKey"       => Primitive::PublicKey,
            "Nullifier"       => Primitive::Nullifier,
            "Coin"            => Primitive::Coin,
            "ContractId"      => Primitive::ContractId,
            "FuncId"          => Primitive::FuncId,
            "TokenId"         => Primitive::TokenId,
            "MerkleNode"      => Primitive::MerkleNode,
            "OwnedSecretKey"  => Primitive::OwnedSecretKey,
            "MiningRecipient" => Primitive::MiningRecipient,
            _ => return None,
        })
    }
}

impl Barb {
    /// Canonical name for manifest declaration and display.
    /// Matches the enum variant name exactly.
    pub fn name(self) -> &'static str {
        match self {
            Barb::Spend          => "Spend",
            Barb::Nullify        => "Nullify",
            Barb::Commit         => "Commit",
            Barb::Prove          => "Prove",
            Barb::Verify         => "Verify",
            Barb::Dispatch       => "Dispatch",
            Barb::Gate           => "Gate",
            Barb::Denominate     => "Denominate",
            Barb::ProveInclusion => "ProveInclusion",
            Barb::Encrypt        => "Encrypt",
            Barb::Derive         => "Derive",
            Barb::Discover       => "Discover",
            Barb::Mine           => "Mine",
            Barb::View           => "View",
        }
    }

    /// Parse a barb from its canonical name (as declared in a manifest action).
    /// Unknown names return `None` — non-fatal (forward-compat), same rationale
    /// as [`Primitive::from_name`].
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "Spend"          => Barb::Spend,
            "Nullify"        => Barb::Nullify,
            "Commit"         => Barb::Commit,
            "Prove"          => Barb::Prove,
            "Verify"         => Barb::Verify,
            "Dispatch"       => Barb::Dispatch,
            "Gate"           => Barb::Gate,
            "Denominate"     => Barb::Denominate,
            "ProveInclusion" => Barb::ProveInclusion,
            "Encrypt"        => Barb::Encrypt,
            "Derive"         => Barb::Derive,
            "Discover"       => Barb::Discover,
            "Mine"           => Barb::Mine,
            "View"           => Barb::View,
            _ => return None,
        })
    }
}

/// Serialize a primitive list to canonical CSV (comma-separated canonical names).
pub fn primitives_to_csv(primitives: &[Primitive]) -> String {
    primitives.iter().map(|p| p.name()).collect::<Vec<_>>().join(",")
}

/// Parse a primitive list from CSV. **Fail closed**: any unknown name yields
/// `None` (matching `resolve_capability`'s contract, so persistence never
/// silently drops or mis-types a primitive). An empty string is `Some(vec![])`.
pub fn primitives_from_csv(csv: &str) -> Option<Vec<Primitive>> {
    if csv.is_empty() {
        return Some(Vec::new())
    }
    csv.split(',').map(Primitive::from_name).collect()
}

/// Serialize a barb list to canonical CSV.
pub fn barbs_to_csv(barbs: &[Barb]) -> String {
    barbs.iter().map(|b| b.name()).collect::<Vec<_>>().join(",")
}

/// Parse a barb list from CSV. Fail closed, same contract as [`primitives_from_csv`].
pub fn barbs_from_csv(csv: &str) -> Option<Vec<Barb>> {
    if csv.is_empty() {
        return Some(Vec::new())
    }
    csv.split(',').map(Barb::from_name).collect()
}

/// A capability type — the Rust equivalent of Lean4 `CapabilityType r s`.
/// Composes primitive types and verifies they cover the resource's barbs.
///
/// `primitives` and `barbs` are held in canonical (sorted, deduplicated-for-barbs)
/// order by the manifest adapter so that composition order is irrelevant
/// (composition.md §1.2) and two equal compositions compare/hash equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedCapability {
    /// The resource this capability operates on (e.g., "native_token", "dao_governance")
    pub resource: String,
    /// The action this capability performs (e.g., "transfer", "vote", "claim_coinbase")
    pub action: String,
    /// The primitive types that compose this capability
    pub primitives: Vec<Primitive>,
    /// The barbs this composed capability exhibits (union of primitive barbs)
    pub barbs: Vec<Barb>,
}

impl TypedCapability {
    /// Verify that this capability's composed barbs cover the required set.
    /// This is the Rust equivalent of the `coversBarbs` proof in Lean4.
    pub fn covers(&self, required: &[Barb]) -> bool {
        required.iter().all(|b| self.barbs.contains(b))
    }

    /// All primitive types that compose this capability, deduplicated.
    pub fn unique_primitives(&self) -> Vec<Primitive> {
        let mut seen = std::collections::BTreeSet::new();
        let mut result = Vec::new();
        for p in &self.primitives {
            if seen.insert(*p) {
                result.push(*p);
            }
        }
        result
    }
}

/// Construct a capability type from primitives and required barbs.
///
/// This is the Rust equivalent of `Wallet.lean`'s `walletConstruct` function.
/// Given a list of primitive types and a set of required barbs, constructs
/// a `TypedCapability` if the primitives cover the barbs.
///
/// Returns `None` if the primitives do not cover all required barbs — the
/// composition is not a valid capability type.
pub fn wallet_construct(
    resource: &str,
    action: &str,
    primitives: Vec<Primitive>,
    required_barbs: &[Barb],
) -> Option<TypedCapability> {
    let composed: Vec<Barb> = primitives.iter()
        .flat_map(|p| p.barbs())
        .copied()
        .collect();
    // Deduplicate while preserving order
    let mut seen = std::collections::BTreeSet::new();
    let barbs: Vec<Barb> = composed.into_iter()
        .filter(|b| seen.insert(*b))
        .collect();

    if required_barbs.iter().all(|b| barbs.contains(b)) {
        Some(TypedCapability {
            resource: resource.to_string(),
            action: action.to_string(),
            primitives,
            barbs,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod type_construction_tests {
    use super::*;

    // =========================================================================
    // Cross-validation against Lean4 Composition.lean capability types.
    // Every construction that is proved in Lean4 must also succeed here.
    // =========================================================================

    #[test]
    fn test_native_token_coinbase_constructible() {
        let ct = wallet_construct(
            "native_token_coinbase", "claim_coinbase",
            vec![Primitive::SecretKey, Primitive::Coin, Primitive::Nullifier,
                 Primitive::ContractId, Primitive::FuncId, Primitive::TokenId,
                 Primitive::MiningRecipient],
            &[Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Dispatch,
              Barb::Gate, Barb::Denominate, Barb::Mine],
        );
        assert!(ct.is_some(), "nativeTokenCoinbaseType must be constructible");
        let ct = ct.unwrap();
        assert!(ct.covers(&[Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Mine]));
    }

    #[test]
    fn test_native_token_transfer_constructible() {
        let ct = wallet_construct(
            "native_token", "transfer",
            vec![Primitive::SecretKey, Primitive::Coin, Primitive::Nullifier,
                 Primitive::ContractId, Primitive::FuncId, Primitive::TokenId,
                 Primitive::MerkleNode],
            &[Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Dispatch,
              Barb::Gate, Barb::Denominate],
        );
        assert!(ct.is_some(), "nativeTokenTransferType must be constructible");
    }

    #[test]
    fn test_dao_vote_constructible() {
        let ct = wallet_construct(
            "dao_governance", "vote",
            vec![Primitive::SecretKey, Primitive::Coin, Primitive::Nullifier,
                 Primitive::ContractId, Primitive::FuncId, Primitive::TokenId,
                 Primitive::MerkleNode],
            &[Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Dispatch,
              Barb::Gate, Barb::Denominate, Barb::ProveInclusion],
        );
        assert!(ct.is_some(), "daoVoteType must be constructible");
    }

    #[test]
    fn test_purse_balance_constructible() {
        let ct = wallet_construct(
            "purse_balance", "balance",
            vec![Primitive::SecretKey, Primitive::Coin, Primitive::ContractId,
                 Primitive::TokenId],
            &[Barb::Spend, Barb::Commit, Barb::Dispatch, Barb::Denominate],
        );
        assert!(ct.is_some(), "purseBalanceType must be constructible");
    }

    #[test]
    fn test_purse_withdraw_constructible() {
        let ct = wallet_construct(
            "purse_withdrawal", "withdraw",
            vec![Primitive::SecretKey, Primitive::Coin, Primitive::Nullifier,
                 Primitive::ContractId, Primitive::TokenId],
            &[Barb::Spend, Barb::Commit, Barb::Nullify, Barb::Dispatch, Barb::Denominate],
        );
        assert!(ct.is_some(), "purseWithdrawType must be constructible");
    }

    #[test]
    fn test_box_capability_constructible() {
        let ct = wallet_construct(
            "box_capability", "take",
            vec![Primitive::SecretKey, Primitive::Nullifier, Primitive::ContractId,
                 Primitive::FuncId, Primitive::MerkleNode],
            &[Barb::Spend, Barb::Nullify, Barb::Dispatch, Barb::Gate, Barb::ProveInclusion],
        );
        assert!(ct.is_some(), "boxCapType must be constructible");
    }

    #[test]
    fn test_identity_credential_constructible() {
        // ↓prove is emergent from the ZK circuit (LTE gate), not a primitive barb.
        // The primitives compose: SecretKey(↓spend,↓derive) + FuncId(↓gate) +
        // ContractId(↓dispatch) + MerkleNode(↓prove-inclusion).
        let ct = wallet_construct(
            "identity_credential", "verify_credential",
            vec![Primitive::SecretKey, Primitive::FuncId, Primitive::ContractId,
                 Primitive::MerkleNode],
            &[Barb::Spend, Barb::Dispatch, Barb::Gate, Barb::ProveInclusion],
        );
        assert!(ct.is_some(), "identityCredentialType must be constructible");
    }

    #[test]
    fn test_multisig_approval_constructible() {
        let ct = wallet_construct(
            "multisig_approval", "finalize",
            vec![Primitive::PublicKey, Primitive::Nullifier, Primitive::ContractId,
                 Primitive::FuncId],
            &[Barb::Verify, Barb::Nullify, Barb::Dispatch, Barb::Gate],
        );
        assert!(ct.is_some(), "multisigApprovalType must be constructible");
    }

    #[test]
    fn test_attestation_constructible() {
        let ct = wallet_construct(
            "attestation", "verify_attestation",
            vec![Primitive::PublicKey, Primitive::ContractId, Primitive::FuncId,
                 Primitive::MerkleNode],
            &[Barb::Verify, Barb::Dispatch, Barb::Gate, Barb::ProveInclusion],
        );
        assert!(ct.is_some(), "attestationType must be constructible");
    }

    #[test]
    fn test_empty_primitives_rejected() {
        let ct = wallet_construct(
            "test", "test",
            vec![],
            &[Barb::Spend],
        );
        assert!(ct.is_none(), "empty primitives must not cover Spend barb");
    }

    #[test]
    fn test_wrong_primitives_rejected() {
        let ct = wallet_construct(
            "native_token", "transfer",
            vec![Primitive::SecretKey, Primitive::Coin],
            &[Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Dispatch],
        );
        assert!(ct.is_none(), "missing Nullifier must cause construction to fail");
    }

    #[test]
    fn test_primitive_barb_sets_match_spec() {
        // Cross-validate that every primitive's barb set matches type-system.md §8.1
        assert_eq!(Primitive::SecretKey.barbs(), &[Barb::Spend, Barb::Derive]);
        assert_eq!(Primitive::PublicKey.barbs(), &[Barb::Verify, Barb::Encrypt]);
        assert_eq!(Primitive::Nullifier.barbs(), &[Barb::Nullify]);
        assert_eq!(Primitive::Coin.barbs(), &[Barb::Commit]);
        assert_eq!(Primitive::ContractId.barbs(), &[Barb::Dispatch]);
        assert_eq!(Primitive::FuncId.barbs(), &[Barb::Gate]);
        assert_eq!(Primitive::TokenId.barbs(), &[Barb::Denominate]);
        assert_eq!(Primitive::MerkleNode.barbs(), &[Barb::ProveInclusion]);
        assert_eq!(Primitive::OwnedSecretKey.barbs(), &[Barb::Spend]);
        assert_eq!(Primitive::MiningRecipient.barbs(), &[Barb::Spend, Barb::Mine]);
    }

    #[test]
    fn test_csv_codec_roundtrip_and_fail_closed() {
        let prims = vec![Primitive::SecretKey, Primitive::Coin, Primitive::TokenId];
        let csv = primitives_to_csv(&prims);
        assert_eq!(csv, "SecretKey,Coin,TokenId");
        assert_eq!(primitives_from_csv(&csv), Some(prims));
        assert_eq!(primitives_from_csv(""), Some(vec![]));
        // Fail closed on an unknown name.
        assert_eq!(primitives_from_csv("SecretKey,Bogus"), None);

        let barbs = vec![Barb::Spend, Barb::Commit, Barb::View];
        let bcsv = barbs_to_csv(&barbs);
        assert_eq!(bcsv, "Spend,Commit,View");
        assert_eq!(barbs_from_csv(&bcsv), Some(barbs));
        assert_eq!(barbs_from_csv("Spend,Nope"), None);
    }

    #[test]
    fn test_primitive_and_barb_name_roundtrip() {
        // Every primitive/barb round-trips through its canonical name, and
        // unknown names parse to None (forward-compat).
        let prims = [
            Primitive::SecretKey, Primitive::PublicKey, Primitive::Nullifier,
            Primitive::Coin, Primitive::ContractId, Primitive::FuncId,
            Primitive::TokenId, Primitive::MerkleNode,
            Primitive::OwnedSecretKey, Primitive::MiningRecipient,
        ];
        for p in prims {
            assert_eq!(Primitive::from_name(p.name()), Some(p));
        }
        assert_eq!(Primitive::from_name("NotAPrimitive"), None);

        let barbs = [
            Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Prove, Barb::Verify,
            Barb::Dispatch, Barb::Gate, Barb::Denominate, Barb::ProveInclusion,
            Barb::Encrypt, Barb::Derive, Barb::Discover, Barb::Mine, Barb::View,
        ];
        for b in barbs {
            assert_eq!(Barb::from_name(b.name()), Some(b));
        }
        assert_eq!(Barb::from_name("NotABarb"), None);
    }

    #[test]
    fn test_all_primitives_have_distinct_barb_sets() {
        // Pareto-efficiency: no two primitives share identical barb sets
        let all = [
            Primitive::SecretKey, Primitive::PublicKey, Primitive::Nullifier,
            Primitive::Coin, Primitive::ContractId, Primitive::FuncId,
            Primitive::TokenId, Primitive::MerkleNode,
            Primitive::OwnedSecretKey, Primitive::MiningRecipient,
        ];
        for i in 0..all.len() {
            for j in (i+1)..all.len() {
                assert_ne!(all[i].barbs(), all[j].barbs(),
                    "Primitives {:?} and {:?} have identical barb sets — pareto-efficiency violated",
                    all[i], all[j]);
            }
        }
    }
}
