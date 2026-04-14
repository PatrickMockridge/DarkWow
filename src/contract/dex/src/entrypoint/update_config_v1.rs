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

//! UpdateConfigV1 entrypoint functions
//!
//! Allows governance to update DEX configuration parameters.

use darkfi_sdk::{crypto::PublicKey, crypto::schnorr::SchnorrPublic, error::ContractError, msg, wasm};
use darkfi_serial::{deserialize, serialize};

use crate::{
    error::DexError,
    model::UpdateConfigParams,
    DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_FEE, DEX_CONTRACT_GOVERNANCE_PUBKEY_KEY,
    DEX_CONTRACT_TIMEOUT,
};

/// `process_instruction` function for `Dex::UpdateConfigV1`
///
/// Verifies the caller is authorized (governance) and updates configuration.
pub(crate) fn dex_update_config_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: UpdateConfigParams = deserialize(&self_.data[1..])?;

    msg!("[UpdateConfigV1] Updating config: timeout={}, fee={}", params.timeout, params.fee);

    // Verify caller is authorized (governance check)
    let config_db = wasm::db::db_lookup(cid, DEX_CONTRACT_CONFIG_TREE)?;
    let governance_pubkey_data = wasm::db::db_get(config_db, DEX_CONTRACT_GOVERNANCE_PUBKEY_KEY)?
        .ok_or(DexError::GovernanceNotSet)?;

    // Verify signature - params must be signed by governance key
    // Encode the config changes for signature verification
    let config_data = serialize(&(params.timeout, params.fee));
    let governance_pubkey = PublicKey::from_bytes(governance_pubkey_data.as_slice().try_into().map_err(|_| DexError::InvalidGovernanceKey)?)
        .map_err(|_| DexError::InvalidGovernanceKey)?;
    if !governance_pubkey.verify(&config_data, &params.signature) {
        return Err(DexError::NotAuthorized.into());
    }

    // Update timeout in config
    wasm::db::db_set(config_db, DEX_CONTRACT_TIMEOUT, &params.timeout.to_le_bytes())?;

    // Update fee in config
    wasm::db::db_set(config_db, DEX_CONTRACT_FEE, &params.fee.to_le_bytes())?;

    msg!("[UpdateConfigV1] Configuration updated successfully");
    Ok(vec![])
}
