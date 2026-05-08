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

    /// Well-known ContractId for Money V3 token derivation (used by vanityaddr).
    /// Money V3 is deployed as a WASM contract via Deployooor — this fixed ID
    /// serves as a stable reference for deriving token FuncIds.
    pub static ref MONEY_V2_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(3)]));

    /// Contract ID for the Native Token contract (hardcoded at genesis).
    /// Native Token handles ONLY consensus-critical operations: block rewards and fees.
    /// All ERC-20 style DeFi functionality lives in Money V3 (WASM, deployed via Deployooor).
    pub static ref NATIVE_TOKEN_CONTRACT_ID: ContractId =
        ContractId::from(poseidon_hash([*CONTRACT_ID_PREFIX, pallas::Base::zero(), pallas::Base::from(4)]));

    /// Native contract IDs bytes (only true native contracts: Deployooor + NativeToken)
    pub static ref NATIVE_CONTRACT_IDS_BYTES: [[u8; 32]; 2] =
        [DEPLOYOOOR_CONTRACT_ID.to_bytes(), NATIVE_TOKEN_CONTRACT_ID.to_bytes()];

    /// Native contract zkas circuits database trees
    pub static ref NATIVE_CONTRACT_ZKAS_DB_NAMES: [[u8; 32]; 2] = [
        DEPLOYOOOR_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
        NATIVE_TOKEN_CONTRACT_ID.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME),
    ];
}

/// ContractId represents an on-chain identifier for a certain smart contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
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
