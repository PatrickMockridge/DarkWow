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

//! UpdateConfigV1 entrypoint functions
//!
//! Allows governance to update DEX configuration parameters.

use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::{error::ContractError, msg, pasta::pallas, wasm};
use dwow_serial::Encodable;

use crate::{
    error::DexError,
    model::UpdateConfigParams,
    DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_FEE,
    DEX_CONTRACT_NULLIFIERS_TREE, DEX_CONTRACT_TIMEOUT,
    DEX_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2,
};

/// `process_instruction` function for `Dex::UpdateConfigV1`
///
/// Verifies the caller is authorized (governance) and updates configuration.
pub(crate) fn dex_update_config_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= UpdateConfigParams::decode(&self_.data[1..])?;

    msg!("[UpdateConfigV1] Updating config: timeout={}, fee={}", params.timeout, params.fee);

    // Verify ZK proof authorizes this config update (governance key holder)
    let config_db = wasm::db::db_lookup(cid, DEX_CONTRACT_CONFIG_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, DEX_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.gov_nullifier.to_repr())? {
        return Err(DexError::NotAuthorized.into());
    }
    // Record nullifier for replay protection
    wasm::db::db_set(nullifiers_db, &params.gov_nullifier.to_repr(), &[])?;

    // Update timeout in config
    wasm::db::db_set(config_db, DEX_CONTRACT_TIMEOUT, &params.timeout.to_le_bytes())?;

    // Update fee in config
    wasm::db::db_set(config_db, DEX_CONTRACT_FEE, &params.fee.to_le_bytes())?;

    msg!("[UpdateConfigV1] Configuration updated successfully");
    Ok(vec![])
}

/// Get metadata for UpdateConfigV1 ZK proof verification
pub(crate) fn dex_update_config_get_metadata_v1(
    params: UpdateConfigParams,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2.to_string(),
        vec![params.gov_pub_x, params.gov_pub_y, params.gov_nullifier, pallas::Base::zero(), pallas::Base::zero()],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}
