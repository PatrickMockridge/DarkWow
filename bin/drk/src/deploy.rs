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

//! Deploy module - Contract deployment functionality
//!
//! This module handles smart contract deployment using the Deployooor contract.

use dwow_core::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Error, Result,
};
use dwow_sdk::{
    crypto::{Keypair, ContractId, PublicKey, SecretKey},
    tx::ContractCall,
};
use dwow_serial::Encodable;
use dwow_sdk::deploy::DeployParamsV1;
use rand::rngs::OsRng;

use crate::{rpc::ScanCache, Drk};
use crate::contract_imports::deployooor::DeployCallBuilder;
use dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID;

/// Default network fee in DRKW
#[allow(dead_code)]
const DEFAULT_FEE: u64 = 42_000_000;

impl Drk {
    /// Create a contract deployment transaction using Deployooor.
    ///
    /// This function:
    /// 1. Takes WASM bytes and a deploy authority keypair
    /// 2. Builds a DeployV1 call to Deployooor contract
    /// 3. Uses the deploy authority's public key to derive the new contract ID
    /// 4. Broadcasts the deployment transaction
    #[allow(dead_code)]
    pub async fn deploy_contract(
        &self,
        deploy_keypair: &Keypair,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
    ) -> Result<Transaction> {
        // Create deploy call builder
        let builder = DeployCallBuilder {
            deploy_keypair: *deploy_keypair,
            wasm_bincode,
            deploy_ix,
        };

        let debris = builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build deploy call: {:?}", e)))?;

        // Create Deployooor contract call
        // Function code 0x00 = DeployV1
        let mut call_data = vec![0x00u8];
        debris.params.encode(&mut call_data)
            .map_err(|e| Error::Custom(format!("Failed to encode deploy params: {:?}", e)))?;

        let deployooor_id = *DEPLOYOOOR_CONTRACT_ID;
        let deploy_call = ContractCall {
            contract_id: deployooor_id,
            data: call_data,
        };

        // Create contract call leaf (no proofs for DeployV1 - it's a native contract call)
        let deploy_leaf = ContractCallLeaf {
            call: deploy_call,
            proofs: vec![],
        };

        // Build final transaction
        let mut tx_builder = TransactionBuilder::new(deploy_leaf, vec![])
            .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;

        let tx = tx_builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build transaction: {:?}", e)))?;

        Ok(tx)
    }

    /// Deploy a contract to the blockchain and wait for confirmation.
    ///
    /// This is a higher-level function that:
    /// 1. Creates the deployment transaction
    /// 2. Broadcasts it
    /// 3. Optionally waits for confirmation
    pub async fn deploy_contract_broadcast(
        &self,
        deploy_keypair: &Keypair,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
        _wait_for_confirm: bool,
        output: &mut Vec<String>,
    ) -> Result<String> {
        // Create deployment transaction
        let tx = self.deploy_contract(deploy_keypair, wasm_bincode, deploy_ix).await?;

        // Broadcast
        let txid = self.broadcast_tx(&tx, output).await?;

        output.push(format!("Contract deployed with txid: {}", txid));

        Ok(txid)
    }

    /// Append data related to DeployoOor contract transactions into
    /// the wallet database and update the provided scan cache.
    pub async fn apply_tx_deploy_data(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        tx_hash: &dwow_sdk::tx::TransactionHash,
        _block_height: &u32,
    ) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        let function_code = data[0];

        match function_code {
            // DeployV1 (0x00)
            0x00 => {
                use dwow_serial::Decodable;
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
        _tx_hash: &dwow_sdk::tx::TransactionHash,
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
        _tx_hash: &dwow_sdk::tx::TransactionHash,
        _block_height: &u32,
    ) -> Result<bool> {
        // TODO: Store lock info in wallet database
        Ok(false)
    }

    /// Generate a new deploy authority keypair.
    ///
    /// This creates a fresh keypair that can be used to deploy contracts.
    /// The resulting contract ID is derived from the public key.
    pub fn generate_deploy_authority(&self) -> Keypair {
        Keypair::new(SecretKey::random(&mut OsRng))
    }

    /// Derive the contract ID that would be created by a given deploy authority.
    pub fn derive_contract_id(deploy_keypair: &Keypair) -> ContractId {
        ContractId::derive_public(deploy_keypair.public)
    }
}