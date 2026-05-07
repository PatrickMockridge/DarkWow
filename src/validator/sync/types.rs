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

//! Types for the clean sync module.

use darkfi_sdk::{crypto::ContractId, pasta::pallas};

use crate::blockchain::BlockInfo;

/// Sync block type - reuses existing BlockInfo
pub type SyncBlock = BlockInfo;

/// ZK data entry from ExtendedProposalMessage
/// Format: (contract_id, zkas_ns, zkbin_bytes, instances)
pub type ZkBinEntry = (ContractId, String, Vec<u8>, Vec<pallas::Base>);

/// Verification result
#[derive(Debug, Clone)]
pub enum VerifyResult {
    Ok,
    InvalidSignature,
    InvalidZkProof,
    InvalidHeader,
}

/// Sync state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    Idle,
    Syncing,
    Verifying,
    Applying,
    Synced,
}