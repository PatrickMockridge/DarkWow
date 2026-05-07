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

//! ZK proof verification wrapper for linear blockchain
//!
//! This module provides a wrapper around darkfi's ZK verification
//! infrastructure for use with the linear blockchain.

use dwow::zk::Proof;
use dwow_sdk::pasta::pallas;
use dwow::zk::verifier::verify_zkp;

/// ZK verifier for linear blockchain
pub struct ZkVerifier;

impl ZkVerifier {
    /// Create a new ZkVerifier
    pub fn new() -> Self {
        Self
    }

    /// Verify a ZK proof
    ///
    /// Returns true if the proof is valid, false otherwise.
    pub fn verify(
        &self,
        proof: &Proof,
        zkbin_bytes: &[u8],
        instances: &[pallas::Base],
    ) -> bool {
        verify_zkp(proof, zkbin_bytes, instances) == dwow::zk::verifier::ZkVerifyResult::Ok
    }
}

impl Default for ZkVerifier {
    fn default() -> Self {
        Self::new()
    }
}