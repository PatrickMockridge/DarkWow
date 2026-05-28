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

//! Oracle Contract Data Structures
//!
//! This contract demonstrates the "push model" for oracles in DarkWow.
//! Oracles create attestations for external data that other contracts
//! can then verify and consume.

use dwow_sdk::pasta::pallas;
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Oracle unique identifier
pub type OracleId = pallas::Base;

/// Attestation ID (references attestation contract)
pub type AttestationId = pallas::Base;

/// Represents an oracle data feed
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Oracle {
    pub version: u8,
    /// Oracle identifier
    pub id: OracleId,
    /// Oracle operator's public key x coordinate
    pub oracle_pub_x: pallas::Base,
    /// Oracle operator's public key y coordinate
    pub oracle_pub_y: pallas::Base,
    /// Name/description of the data feed
    pub name: String,
    /// Type of data (e.g., "price", "weather", "score")
    pub data_type: String,
    /// Current value (updated by oracle)
    pub value: pallas::Base,
    /// Block when value was last updated
    pub updated_at: u64,
    /// Whether oracle is active
    pub is_active: bool,
}

/// Parameters for registering a new oracle
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterOracleParamsV1 {
    /// ZK proof for oracle registration
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Oracle operator's public key x coordinate
    pub oracle_pub_x: pallas::Base,
    /// Oracle operator's public key y coordinate
    pub oracle_pub_y: pallas::Base,
    /// Name of the data feed
    pub name: String,
    /// Type of data
    pub data_type: String,
}

/// Parameters for pushing a new value
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PushValueParamsV1 {
    /// ZK proof for value push
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// New value
    pub value: pallas::Base,
}

/// Parameters for creating an attestation for external data
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AttestValueParamsV1 {
    /// ZK proof for attestation
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Attestation ID (to be created)
    pub attestation_id: AttestationId,
    /// Predicate type (0=Matches, 1=GreaterOrEqual, 2=LessOrEqual)
    pub predicate: u8,
    /// Threshold value for comparison predicates
    pub threshold: pallas::Base,
}

/// Parameters for pushing a commitment to a data point (private value submission)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PushValueCommitmentParamsV1 {
    /// ZK proof for commitment push
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Commitment (Poseidon hash of value and nonce)
    pub commitment: pallas::Base,
    /// Merkle root of the data tree (public input)
    pub data_root: pallas::Base,
    /// Position in Merkle tree
    pub pos: pallas::Base,
    /// Sparse Merkle path
    pub path: Vec<pallas::Base>,
}

/// Parameters for aggregating multiple data points
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AggregateParamsV1 {
    /// ZK proof for aggregation
    pub proof: Vec<u8>,
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Computed weighted average result
    pub result: pallas::Base,
    /// Minimum acceptable result
    pub min_result: pallas::Base,
    /// Maximum acceptable result
    pub max_result: pallas::Base,
}

// ============================================================================
// UPDATE TYPES (for process_update)
// ============================================================================

/// Update for RegisterOracleV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterOracleUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
}

/// Update for PushValueV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PushValueUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
    /// New value pushed by oracle
    pub value: pallas::Base,
}

/// Update for AttestValueV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AttestValueUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Attestation ID
    pub attestation_id: AttestationId,
}

/// Update for PushValueCommitmentV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PushValueCommitmentUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Commitment hash
    pub commitment: pallas::Base,
}

/// Update for AggregateV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AggregateUpdateV1 {
    /// Oracle ID
    pub oracle_id: OracleId,
    /// Aggregated result
    pub result: pallas::Base,
}

/// Parameters for `SetOracleActiveV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SetOracleActiveParamsV1 {
    pub oracle_pub_x: pallas::Base,
    pub oracle_pub_y: pallas::Base,
    pub is_active: bool,
}

/// Update for `SetOracleActiveV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SetOracleActiveUpdateV1 {
    pub oracle_id: OracleId,
    pub is_active: bool,
}
