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

use dwow_serial::{SerialDecodable, SerialEncodable};
use lazy_static::lazy_static;
use pasta_curves::{group::ff::PrimeField, pallas};

use super::{poseidon_hash, PublicKey, SecretKey};
use crate::error::ContractError;

/// The hardcoded db name for the zkas circuits database tree
pub const SMART_CONTRACT_ZKAS_DB_NAME: &str = "_zkas";

/// The hardcoded db name for the monotree database tree
pub const SMART_CONTRACT_MONOTREE_DB_NAME: &str = "_monotree";

lazy_static! {
    // The idea here is that 0 is not a valid x coordinate for any pallas point,
    // therefore a signature cannot be produced for such IDs. This allows us to
    // avoid hardcoding contract IDs for arbitrary contract deployments, because
    // the contracts with 0 as their x coordinate can never have a valid signature.

    /// Derivation prefix for `ContractId`
    pub static ref CONTRACT_ID_PREFIX: pallas::Base = pallas::Base::from(42);

    /// Contract ID for the native Deployooor contract
    ///
    /// `EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN`
    pub static ref DEPLOYOOOR_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(2)]));

    /// Contract ID for the Promissory Note contract (hardcoded at genesis).
    ///
    /// Promissory Note is included in genesis as a universal DeFi dependency —
    /// every bridge, stablecoin, DEX, escrow, and bearer bond references its
    /// contract ID. A canonical, well-known ID prevents ecosystem fragmentation
    /// from replica deployments.
    ///
    /// PN plays ZERO role in chain consensus. It is not native_token. It does
    /// not affect block validation, fee payment, or coinbase rewards. It is in
    /// genesis purely as ecosystem infrastructure, like ERC-20 pre-deploys on
    /// Ethereum testnets or the bank module in Cosmos SDK.
    pub static ref PROMISSORY_NOTE_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(3)]));

    /// Legacy alias — Promissory Note was previously called "Money" in upstream.
    pub static ref MONEY_TOKEN_CONTRACT_ID: ContractId = *PROMISSORY_NOTE_CONTRACT_ID;

    /// Contract ID for the Native Token contract (hardcoded at genesis).
    /// Native Token handles ONLY consensus-critical operations: block rewards and fees.
    pub static ref NATIVE_TOKEN_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(4)]));

    /// Contract ID for the Identity contract (hardcoded at genesis).
    ///
    /// Identity provides credential issuance, selective disclosure, and capability proofs.
    /// It is a core dependency of the contract manifest trust model (Layer 3: Attestation).
    /// As genesis infrastructure, it has a canonical well-known ContractId that every
    /// node can rely on from block 1.
    pub static ref IDENTITY_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(5)]));

    /// Contract ID for the Oracle contract (hardcoded at genesis).
    ///
    /// Oracle provides external data feeds (price, weather, randomness) via a push model.
    /// It is a core dependency of the contract manifest trust model — attestations
    /// depend on oracle data for predicate verification.
    pub static ref ORACLE_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(6)]));

    /// Contract ID for the Attestation contract (hardcoded at genesis).
    ///
    /// Attestation provides claim verification, predicates, delegation, and slashing.
    /// It is the core of Layer 3 of the contract manifest trust model — without it,
    /// contracts cannot verify that other contracts' binaries match their claims.
    pub static ref ATTESTATION_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(7)]));

    /// Contract ID for the Purse contract (hardcoded at genesis).
    ///
    /// Purse is the ZK fungible asset container — the DarkWow equivalent of Agoric's ERTP Purse.
    /// It provides deposit, withdraw, and balance operations with hidden balances (Pedersen)
    /// and hidden token types (Poseidon). It is the primitive that PN token balances are
    /// measured in, and every wallet depends on it for balance tracking.
    pub static ref PURSE_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(8)]));

    /// Contract ID for the Box contract (hardcoded at genesis).
    ///
    /// Box is the ZK capability container — the DarkWow equivalent of capability delegation.
    /// Put a capability into a Box; whoever Takes it receives it. The Box is consumed on open
    /// (linear use via nullifier). Contents are hidden — the chain sees only that SOMETHING
    /// was transferred, not what.
    pub static ref BOX_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(9)]));

    /// Consensus-critical native contract IDs (Deployooor + NativeToken only).
    /// Promissory Note is deliberately excluded — it is ecosystem infrastructure,
    /// not a consensus dependency.
    pub static ref NATIVE_CONTRACT_IDS_BYTES: [[u8; 32]; 2] =
        [DEPLOYOOOR_CONTRACT_ID.to_bytes(), NATIVE_TOKEN_CONTRACT_ID.to_bytes()];

    /// Native contract zkas circuits database trees
    pub static ref NATIVE_CONTRACT_ZKAS_DB_NAMES: [[u8; 32]; 2] = [
        DEPLOYOOOR_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
        NATIVE_TOKEN_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    ];

    /// All genesis-deployed contract IDs (consensus-critical + ecosystem infrastructure).
    /// 8 contracts: 2 consensus-critical (Deployooor, NativeToken) + 6 ecosystem.
    pub static ref GENESIS_CONTRACT_IDS_BYTES: [[u8; 32]; 8] = [
        DEPLOYOOOR_CONTRACT_ID.to_bytes(),
        PROMISSORY_NOTE_CONTRACT_ID.to_bytes(),
        NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
        IDENTITY_CONTRACT_ID.to_bytes(),
        ORACLE_CONTRACT_ID.to_bytes(),
        ATTESTATION_CONTRACT_ID.to_bytes(),
        PURSE_CONTRACT_ID.to_bytes(),
        BOX_CONTRACT_ID.to_bytes(),
    ];
}

/// ContractId represents an on-chain identifier for a certain smart contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, SerialEncodable, SerialDecodable)]
pub struct ContractId(pallas::Base);

impl ContractId {
    /// Derives a `ContractId` from a `SecretKey` (deploy key)
    pub fn derive(deploy_key: SecretKey) -> Self {
        let public_key = PublicKey::from_secret(deploy_key);
        let (x, y) = public_key.xy();
        let hash = poseidon_hash([*CONTRACT_ID_PREFIX, x, y]);
        Self(hash)
    }

    /// Derive a contract ID from a `PublicKey`
    pub fn derive_public(public_key: PublicKey) -> Self {
        let (x, y) = public_key.xy();
        let hash = poseidon_hash([*CONTRACT_ID_PREFIX, x, y]);
        Self(hash)
    }

    /// Get the inner `pallas::Base` element.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create a `ContractId` object from given bytes.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError(
                "Failed to instantiate ContractId from bytes".to_string(),
            )),
        }
    }

    /// Convert a `ContractId` object to its byte representation
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    /// `blake3(self || tree_name)` is used in databases to have a
    /// fixed-size name for a contract's state db.
    pub fn hash_state_id(&self, tree_name: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.to_bytes());
        hasher.update(tree_name.as_bytes());
        let id = hasher.finalize();
        *id.as_bytes()
    }
}

use core::str::FromStr;
crate::fp_from_bs58!(ContractId);
crate::fp_to_bs58!(ContractId);
crate::ty_from_fp!(ContractId);
