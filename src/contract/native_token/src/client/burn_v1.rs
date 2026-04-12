/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! NativeToken BurnV1 Client API
//!
//! This module provides the ability to build Burn calls to destroy coins.

use darkfi::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, Keypair, SecretKey},
    error::ContractError,
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::BurnParamsV1;

/// Struct holding necessary information to build a `NativeToken::BurnV1`
/// contract call.
pub struct BurnCallBuilder {
    /// Anonymous inputs
    pub inputs: Vec<BurnCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

/// Input for building a burn call
pub struct BurnCallInput {
    /// Value of the coin being burned
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<darkfi_sdk::crypto::MerkleNode>,
    /// Caller's keypair for signing
    pub keypair: Keypair,
}

/// Debris produced by building a Burn call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct BurnCallDebris {
    /// The contract call parameters
    pub params: BurnParamsV1,
    /// The ZK proofs for the burn operation
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,
}

impl BurnCallBuilder {
    /// Build the Burn call debris
    /// Note: This is a stub - actual burn proof generation requires
    /// proper merkle proof verification and ZK circuit setup.
    pub fn build(self) -> Result<BurnCallDebris> {
        debug!(target: "contract::native_token::client::burn", "Building NativeToken::BurnV1 contract call (stub)");

        if self.inputs.is_empty() {
            return Err(ContractError::Custom(1).into());
        }

        let mut signature_secrets = vec![];
        for input in self.inputs.iter() {
            let signature_secret = SecretKey::random(&mut OsRng);
            signature_secrets.push(signature_secret);
        }

        Ok(BurnCallDebris {
            params: BurnParamsV1 { inputs: vec![] },
            proofs: vec![],
            signature_secrets,
        })
    }
}