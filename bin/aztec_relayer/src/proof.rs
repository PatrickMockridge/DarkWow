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

//! Aztec Deposit Proof Construction
//!
//! Constructs the ZK proof data for Aztec rollup deposits.
//! This module creates the proof that demonstrates:
//! 1. The note exists in the Aztec note tree
//! 2. The prover knows the note secret (without revealing it)
//! 3. The commitment is correctly formed

use anyhow::Result;

use crate::{aztec_rpc::AztecNote, Config};

/// Submit an Aztec deposit to the DarkWow bridge
///
/// Constructs the AztecDepositProof and submits it via the DarkWow RPC.
pub async fn submit_deposit(note: &AztecNote, config: &Config) -> Result<()> {
    // TODO: Implement actual proof construction and submission
    //
    // The AztecDepositProof requires:
    // 1. nullifier: The note's nullifier (derived from secret + asset)
    // 2. commitment: Pedersen commitment to the value
    // 3. anchor: Merkle root at the rollup height
    // 4. merkle_path: Authentication path proving inclusion
    // 5. proof_bytes: ZK proof of note ownership (stubbed for MVP)
    // 6. value: Value in wei
    // 7. asset_id: ETH = 0, DAI = 1
    // 8. rollup_height: Aztec rollup block
    // 9. eth_block_height: Ethereum block of rollup commitment
    // 10. confirmations: Number of Ethereum block confirmations
    // 11. rollup_tx_hash: Ethereum tx hash of rollup

    let asset_name = if note.asset_id == 0 { "ETH" } else { "DAI" };
    println!("[aztec_relayer::proof] Submitting deposit proof:");
    println!("  rollup_tx_hash: {}", note.rollup_tx_hash);
    println!("  value: {} wei", note.value);
    println!("  asset_id: {} ({})", note.asset_id, asset_name);
    println!("  rollup_height: {}", note.rollup_height);
    println!("  eth_block_height: {}", note.eth_block_height);
    println!("  nullifier: {:?}", hex::encode(&note.nullifier));
    println!("  commitment: {:?}", hex::encode(&note.commitment));
    println!("  anchor: {:?}", hex::encode(&note.anchor));
    println!("  confirmations: {}", note.confirmations);

    // TODO: Actually submit to DarkWow via JSON-RPC
    // POST to config.darkfid_url
    // Method: bridge_deposit
    // Params: AztecDepositProof structure

    Ok(())
}

/// Derive the nullifier for an Aztec note
///
/// In Aztec, the nullifier is derived as:
///   nf = pedersen_hash(note_secret, asset_id)
/// where note_secret is derived from the user's spending key.
pub fn derive_nullifier(note_secret: &[u8; 32], asset_id: u32) -> Result<[u8; 32]> {
    use blake3::Hasher;

    let mut h = Hasher::new();
    h.update(b"aztec_nullifier");
    h.update(note_secret);
    h.update(&asset_id.to_le_bytes());

    let hash = h.finalize();
    let mut nullifier = [0u8; 32];
    nullifier.copy_from_slice(hash.as_bytes());

    Ok(nullifier)
}

/// Derive the commitment for an Aztec note
///
/// In Aztec, the commitment is:
///   cm = pedersen_hash(value, secret, asset_id, blinding)
pub fn derive_commitment(
    value: u64,
    secret: &[u8; 32],
    asset_id: u32,
    blinding: &[u8; 32],
) -> Result<[u8; 32]> {
    use blake3::Hasher;

    let mut h = Hasher::new();
    h.update(b"aztec_commitment");
    h.update(&value.to_le_bytes());
    h.update(secret);
    h.update(&asset_id.to_le_bytes());
    h.update(blinding);

    let hash = h.finalize();
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(hash.as_bytes());

    Ok(commitment)
}

/// Verify the merkle proof for an Aztec note
///
/// The Aztec note tree uses a Merkle tree with Pedersen hashing.
/// For DarkWow bridge compatibility, we use blake3 for verification.
pub fn verify_merkle_path(
    commitment: &[u8; 32],
    position: u32,
    path: &[[u8; 32]],
    anchor: &[u8; 32],
) -> Result<bool> {
    // TODO: Implement actual Aztec Merkle path verification
    // Using Pedersen hash for the tree hashing
    Ok(true)
}