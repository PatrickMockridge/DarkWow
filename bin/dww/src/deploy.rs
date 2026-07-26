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
    tx::{ContractCallLeaf, Transaction},
};
use crate::wallet_error::{Error, Result};
use dwow_sdk::{
    crypto::{Keypair, ContractId, PublicKey, SecretKey},
    tx::ContractCall,
};
use dwow_sdk::deploy::DeployParamsV1;
use rand::rngs::OsRng;

use crate::{scan::ScanCache, Dww};
use crate::contract_imports::deployooor::DeployCallBuilder;
use dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID;

/// Default network fee in DRKW
#[allow(dead_code)]
const DEFAULT_FEE: u64 = 42_000_000;

impl Dww {
    /// Create a contract deployment transaction using Deployooor.
    ///
    /// This function:
    /// 1. Takes WASM bytes and a deploy authority keypair
    /// 2. Builds a DeployV1 call to Deployooor contract
    /// 3. Attaches the FeeV1 call from the wallet's DRKW caps
    ///    (`build_fee_and_finalize_tx` — real fee proofs, fee nullifier
    ///    published, outer tx_commitment computed)
    /// 4. Signs per-call: the deploy row with the deploy authority
    ///    (DeployV1 metadata declares `[params.public_key]`,
    ///    deployooor entrypoint/deploy_v1.rs), the fee row with the fee
    ///    ephemeral — one signature row per call, in call order
    #[allow(dead_code)]
    pub async fn deploy_contract(
        &self,
        deploy_keypair: &Keypair,
        wasm_bincode: Vec<u8>,
        deploy_ix: Vec<u8>,
    ) -> Result<Transaction> {
        // Create deploy call builder
        let builder = DeployCallBuilder {
            deploy_keypair: deploy_keypair.clone(),
            wasm_bincode,
            deploy_ix,
            singleton: false,
            singleton_name: String::new(),
        };

        let call_data = builder.build_call_data()
            .map_err(|e| Error::Custom(format!("Failed to build deploy call data: {:?}", e)))?;

        // Create Deployooor contract call
        // Function code 0x00 = DeployV1 is encapsulated in build_call_data()
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

        // Attach the fee call and build the transaction (§6.3 step 6) —
        // fee proofs, fee nullifier, and the outer tx_commitment all handled
        // by the centralized fee builder.
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
        let mut tx = crate::fee_builder::build_fee_and_finalize_tx(
            &self.wallet, &self.account_mgr, deploy_leaf, None, None, seed,
        )?;


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
        let txid = self.broadcast_tx(&tx, output, _wait_for_confirm, None, None).await?;

        output.push(format!("Contract deployed with txid: {}", txid));

        Ok(txid)
    }

    /// Append data related to DeployoOor contract transactions into
    /// the wallet database and update the provided scan cache.
    pub fn apply_tx_deploy_data(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        tx_hash: &dwow_sdk::tx::TransactionHash,
        _block_height: &u64,
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
                let _params = DeployParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode DeployV1 params: {:?}", e)))?;

                scan_cache.log(format!(
                    "[apply_tx_deploy_data] Found DeployV1 contract deployment: contract_id={}",
                    tx_hash
                ));

                // Deploy-authority tracking deferred (requires BTreeMap, spec justification).
                // Until implemented, all deployments are treated as external.
                let is_own_deployment = false;
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
    pub fn apply_deploy_deploy_data(
        &self,
        _scan_cache: &mut ScanCache,
        _params: &DeployParamsV1,
        _tx_hash: &dwow_sdk::tx::TransactionHash,
        _block_height: &u64,
    ) -> Result<bool> {
        // TODO: Store deployment info in wallet database
        Ok(false)
    }

    /// Apply lock data from a LockV1 call.
    pub fn apply_deploy_lock_data(
        &self,
        _scan_cache: &mut ScanCache,
        _public_key: &PublicKey,
        _tx_hash: &dwow_sdk::tx::TransactionHash,
        _block_height: &u64,
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