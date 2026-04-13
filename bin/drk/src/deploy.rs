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

//! Deploy module - Contract deployment functionality
//!
//! This module handles smart contract deployment using the Deployooor contract.

use darkfi::{
    tx::Transaction,
    Error, Result,
};
use darkfi_sdk::{
    crypto::{ContractId, Keypair, PublicKey, SecretKey},
    deploy::DeployParamsV1,
    tx::TransactionHash,
};
use darkfi_serial::Decodable;

use crate::{rpc::ScanCache, Drk};

impl Drk {
    /// Create a feeless contract deployment transaction.
    ///
    /// Note: Contract deployment requires fee payment infrastructure which depends
    /// on full Money V3 integration. This is a placeholder.
    pub async fn deploy_contract(
        &self,
        _deploy_auth: &ContractId,
        _wasm_bincode: Vec<u8>,
        _deploy_ix: Vec<u8>,
    ) -> Result<Transaction> {
        Err(Error::Custom("Contract deployment not yet implemented for Money V3 - requires fee infrastructure".to_string()))
    }

    /// Append data related to DeployoOor contract transactions into
    /// the wallet database and update the provided scan cache.
    pub async fn apply_tx_deploy_data(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        tx_hash: &TransactionHash,
        block_height: &u32,
    ) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        let function_code = data[0];

        match function_code {
            // DeployV1 (0x00)
            0x00 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = DeployParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode DeployV1 params: {:?}", e)))?;

                scan_cache.log(format!(
                    "[apply_tx_deploy_data] Found DeployV1 contract deployment: contract_id={}",
                    tx_hash
                ));

                // Check if this deployment is for one of our deploy authorities
                let mut is_own_deployment = false;
                for (pubkey_bytes, _secret) in &scan_cache.own_deploy_auths {
                    // The public key from params should match one of our deploy auths
                    let pubkey_bytes_check = params.public_key.to_bytes();
                    if pubkey_bytes == &pubkey_bytes_check {
                        scan_cache.log(format!(
                            "[apply_tx_deploy_data] Found deployment with our deploy authority"
                        ));
                        is_own_deployment = true;
                        break
                    }
                }

                Ok(is_own_deployment)
            }
            // LockV1 (0x01)
            0x01 => {
                scan_cache.log(String::from("[apply_tx_deploy_data] Found LockV1 call"));
                Ok(false)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_deploy_data] Unknown deploy function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }

    /// Apply deploy data from a DeployV1 call.
    pub async fn apply_deploy_deploy_data(
        &self,
        _scan_cache: &mut ScanCache,
        _params: &DeployParamsV1,
        _tx_hash: &TransactionHash,
        _block_height: &u32,
    ) -> Result<bool> {
        // TODO: Store deployment info in wallet database
        Ok(false)
    }

    /// Apply lock data from a LockV1 call.
    pub async fn apply_deploy_lock_data(
        &self,
        _scan_cache: &mut ScanCache,
        _public_key: &PublicKey,
        _tx_hash: &TransactionHash,
        _block_height: &u32,
    ) -> Result<bool> {
        // TODO: Store lock info in wallet database
        Ok(false)
    }
}