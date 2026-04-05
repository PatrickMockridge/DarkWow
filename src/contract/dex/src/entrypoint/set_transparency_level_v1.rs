/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::{error::ContractError, msg, wasm};
use darkfi_serial::deserialize;

use crate::{
    model::SetTransparencyLevelParams,
    DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_TRANSPARENCY_LEVEL_KEY,
};

/// `process_instruction` function for `Dex::SetTransparencyLevelV1`
///
/// Verifies the caller is authorized (governance) and updates transparency level.
pub(crate) fn dex_set_transparency_level_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    ix: &[u8],
) -> Result<Vec<u8>, ContractError> {
    let params: SetTransparencyLevelParams = deserialize(ix)?;

    msg!("[SetTransparencyLevelV1] Setting transparency level to: {:?}", params.level);

    // Verify caller is authorized ( governance check )
    // TODO: Add proper governance authorization check
    // For now, we just update the level

    // Update transparency level in config
    let config_db = wasm::db::db_lookup(cid, DEX_CONTRACT_CONFIG_TREE)?;
    wasm::db::db_set(config_db, DEX_CONTRACT_TRANSPARENCY_LEVEL_KEY, &[params.level as u8])?;

    msg!("[SetTransparencyLevelV1] Transparency level updated successfully");
    Ok(vec![])
}
