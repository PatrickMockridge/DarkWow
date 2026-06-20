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

//! SetTransparencyLevelV1 entrypoint functions
//!
//! Allows governance to change the transparency level post-deployment.

use dwow_sdk::{crypto::PublicKey, crypto::schnorr::SchnorrPublic, error::ContractError, msg, wasm};
use dwow_serial::{deserialize, serialize};

use crate::{
    error::DexError,
    model::SetTransparencyLevelParams,
    DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_GOVERNANCE_PUBKEY_KEY,
    DEX_CONTRACT_NULLIFIERS_TREE, DEX_CONTRACT_TRANSPARENCY_LEVEL_KEY,
};

/// `process_instruction` function for `Dex::SetTransparencyLevelV1`
///
/// Verifies the caller is authorized (governance) and updates transparency level.
pub(crate) fn dex_set_transparency_level_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: SetTransparencyLevelParams = deserialize(&self_.data[1..])?;

    msg!("[SetTransparencyLevelV1] Setting transparency level to: {:?}", params.level);

    // Verify ZK proof authorizes this operation (governance key holder)
    let config_db = wasm::db::db_lookup(cid, DEX_CONTRACT_CONFIG_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, DEX_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.gov_nullifier))? {
        return Err(DexError::NotAuthorized.into());
    }

    // Update transparency level in config
    wasm::db::db_set(config_db, DEX_CONTRACT_TRANSPARENCY_LEVEL_KEY, &[params.level as u8])?;

    msg!("[SetTransparencyLevelV1] Transparency level updated successfully");
    Ok(vec![])
}
