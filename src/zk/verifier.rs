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

//! Pure ZK proof verification module.
//!
//! Design principles:
//! - Stateless: no sled, no WASM, no side effects
//! - Deterministic: same inputs always produce same output
//! - Separated: independent from sync, consensus, and block production

use darkfi_sdk::pasta::pallas;

use crate::{zk::ZkCircuit, zk::empty_witnesses, zk::Proof, zk::VerifyingKey, zkas::ZkBinary};

/// ZK Verification result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkVerifyResult {
    /// Proof is valid
    Ok,
    /// Proof verification failed
    InvalidProof,
    /// Could not derive VK from circuit bytes
    InvalidVk,
}

/// Verify a ZK proof given the circuit bytes and public instances.
///
/// This is a pure function - same inputs always produce same output.
/// No sled, no WASM, no side effects.
pub fn verify_zkp(
    proof: &Proof,
    zkbin_bytes: &[u8],
    instances: &[pallas::Base],
) -> ZkVerifyResult {
    // 1. Decode ZkBinary from bytes
    let Ok(zkbin) = ZkBinary::decode(zkbin_bytes, false) else {
        return ZkVerifyResult::InvalidVk
    };

    // 2. Create circuit with empty witnesses (for VK derivation only)
    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(_) => return ZkVerifyResult::InvalidVk,
    };
    let circuit = ZkCircuit::new(witnesses, &zkbin);

    // 3. Derive VK from circuit
    let vk = VerifyingKey::build(zkbin.k, &circuit);

    // 4. Verify proof with derived VK
    match proof.verify(&vk, instances) {
        Ok(()) => ZkVerifyResult::Ok,
        Err(_) => ZkVerifyResult::InvalidProof,
    }
}