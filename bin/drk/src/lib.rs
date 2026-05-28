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

use std::{collections::HashMap, fs::create_dir_all, sync::Arc};

use bs58;
use hex;

use smol::lock::RwLock;
use url::Url;

use dwow_core::{
    system::ExecutorPtr,
    tx::{ContractCallLeaf, Transaction},
    util::path::expand_path,
    zk::{proof::ProvingKey, Proof},
    zkas::ZkBinary,
    Error, Result,
};
use dwow_serial::deserialize_partial;
use dwow_sdk::{
    crypto::{
        keypair::{Address, Keypair, Network, PublicKey, SecretKey},
        pasta_prelude::PrimeField,
        poseidon_hash, ContractId, FuncId, MerkleTree,
    },
    pasta::pallas,
    tx::ContractCall,
};
use dwow_money_v3_contract::client::MoneyV3Note;
use dwow_money_v3_contract::model::TransferParamsV1;
use crate::contract_imports::{money::TokenId, MONEY_V3_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
use crate::swap::PartialSwapData;
use crate::walletdb::CoinRecord;
use dwow_sdk::crypto::util::FieldElemAsStr;

/// Error codes
pub mod error;
use error::{WalletDbError, WalletDbResult};

/// Common shared functions
pub mod common;

/// darkfid JSON-RPC related methods
pub mod rpc;
use rpc::DarkfidRpcClient;

/// Payment methods
pub mod transfer;

/// Swap methods
pub mod swap;

/// Token methods
pub mod token;

/// CLI utility functions
pub mod cli_util;

/// Drk interactive shell (TEMPORARILY DISABLED - DAO removal in progress)
// pub mod interactive;

/// Wallet functionality related to Deployooor
pub mod deploy;

/// Wallet functionality related to DAO-Escrow WASM contract
pub mod dao_escrow;

/// Wallet functionality related to DrainProtection WASM contract
pub mod drain_protection;

/// Fee builder helper for contract transactions
pub mod fee_builder;

/// Wallet functionality related to transactions history
pub mod txs_history;

/// Wallet functionality related to scanned blocks
pub mod scanned_blocks;

/// Contract import graph - maps stale imports to actual crates
pub mod contract_imports;

/// Generic contract registry for dependency resolution and transaction building
pub mod contract_registry;

/// Contract metadata registry for universal contract interaction
pub mod contract_metadata;

/// Capability-based wallet state resolution
pub mod capability;

/// Money module (re-export from contract_imports for backwards compatibility)
pub mod money {
    pub use crate::contract_imports::money::*;
}

/// Wallet database operations handler
pub mod walletdb;
use walletdb::{WalletDb, WalletPtr};

/// Blockchain cache database operations handler
pub mod cache;
use cache::Cache;

/// Atomic pointer to a `Drk` structure.
pub type DrkPtr = Arc<RwLock<Drk>>;

/// CLI-util structure
pub struct Drk {
    /// Blockchain network
    pub network: Network,
    /// Blockchain cache database operations handler
    pub cache: Cache,
    /// Wallet database operations handler
    pub wallet: WalletPtr,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: Option<RwLock<DarkfidRpcClient>>,
    /// Flag indicating if fun stuff are enabled
    pub fun: bool,
}

impl Drk {
    pub async fn new(
        network: Network,
        cache_path: String,
        wallet_path: String,
        wallet_pass: String,
        endpoint: Option<Url>,
        ex: &ExecutorPtr,
        fun: bool,
    ) -> Result<Self> {
        // Initialize blockchain cache database
        let db_path = expand_path(&cache_path)?;
        let sled_db = sled::open(&db_path)?;
        let Ok(cache) = Cache::new(&sled_db) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Initialize wallet
        let wallet_path = expand_path(&wallet_path)?;
        if !wallet_path.exists() {
            if let Some(parent) = wallet_path.parent() {
                create_dir_all(parent)?;
            }
        }
        let Ok(wallet) = WalletDb::new(Some(wallet_path), Some(&wallet_pass)) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Auto-load persisted contract registry into OnceLock values
        if let Ok(registry) = wallet.get_contract_registry() {
            for (name, cid_str) in registry {
                let cid_bytes: [u8; 32] = match bs58::decode(&cid_str).into_vec() {
                    Ok(v) => match v.try_into() {
                        Ok(b) => b,
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                let cid = match ContractId::from_bytes(cid_bytes) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let _ = crate::contract_imports::register_contract_id(&name, cid);
            }
        }

        // Initialize rpc client
        let rpc_client = if let Some(endpoint) = endpoint {
            Some(RwLock::new(DarkfidRpcClient::new(endpoint, ex.clone(), network).await))
        } else {
            None
        };

        Ok(Self { network, cache, wallet, rpc_client, fun })
    }

    pub fn into_ptr(self) -> DrkPtr {
        Arc::new(RwLock::new(self))
    }

    /// Initialize wallet with tables for `Drk`.
    pub async fn initialize_wallet(&self) -> WalletDbResult<()> {
        // Initialize wallet schema
        self.wallet.exec_batch_sql(include_str!("../wallet.sql"))?;

        Ok(())
    }

    /// Auxiliary function to completely reset wallet state.
    pub fn reset(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting full wallet state"));
        self.reset_scanned_blocks(output)?;
        self.reset_deploy_authorities(output)?;
        self.reset_deploy_history(output)?;
        self.reset_tx_history(output)?;
        output.push(String::from("Successfully reset full wallet state"));
        Ok(())
    }

    // =============================================================================================
    // STUB METHODS - These are stubs that need proper implementation
    // =============================================================================================

    /// Get the Money contract Merkle tree from cache
    pub async fn get_money_tree(&self) -> Result<MerkleTree> {
        match self.cache.get_merkle_tree(b"money_merkle_trees") {
            Some(tree) => Ok(tree),
            None => {
                // Create an empty Merkle tree for darkwow-devnet (no previous state)
                let tree = MerkleTree::new(1);
                Ok(tree)
            }
        }
    }

    /// Get money secrets from wallet
    pub async fn get_money_secrets(&self) -> Result<Vec<SecretKey>> {
        let secret_strings = self.wallet.get_secrets().map_err(|e| Error::Custom(format!("{:?}", e)))?;
        let mut secrets = vec![];
        for s in secret_strings {
            let bytes = bs58::decode(&s).into_vec().map_err(|e| Error::Custom(e.to_string()))?;
            let key_array: [u8; 32] = bytes.try_into().map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
            let secret = SecretKey::from_bytes(key_array)
                .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;
            secrets.push(secret);
        }
        Ok(secrets)
    }

    /// Get coins from wallet
    pub async fn get_coins(&self, spent: bool) -> Result<Vec<MoneyV3Note>> {
        let coin_records = self.wallet.get_coins(spent).map_err(|e| Error::Custom(format!("{:?}", e)))?;
        coin_records_to_money_notes(&coin_records)
    }

    /// Get coins for a specific token
    pub async fn get_token_coins(&self, token_id: &TokenId) -> Result<Vec<MoneyV3Note>> {
        let token_id_str = token_id.to_string();
        let coin_records = self.wallet.get_token_coins(&token_id_str, false).map_err(|e| Error::Custom(format!("{:?}", e)))?;
        coin_records_to_money_notes(&coin_records)
    }

    /// Get token by token ID or alias.
    ///
    /// The identifier can be:
    /// - A bs58-encoded token ID (pallas::Base)
    /// - A token name/alias stored in the wallet
    pub async fn get_token(&self, identifier: String) -> Result<TokenId> {
        // First, try to parse identifier as a direct bs58-encoded token ID
        if let Ok(token_bytes) = bs58::decode(&identifier).into_vec() {
            if token_bytes.len() == 32 {
                let token_array: [u8; 32] = match token_bytes.clone().try_into() {
                    Ok(a) => a,
                    Err(_) => return Err(Error::Custom("Invalid token_id bytes".to_string())),
                };
                let token_id_opt = pallas::Base::from_repr(token_array);
                if bool::from(token_id_opt.is_some()) {
                    return Ok(token_id_opt.unwrap());
                }
            }
        }

        // Try to look up in database by token_id or name
        match self.wallet.get_token(&identifier) {
            Ok(Some(token_info)) => {
                // Parse the token_id from bs58
                let token_bytes = bs58::decode(&token_info.token_id)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid token_id in database: {}", e)))?;
                if token_bytes.len() == 32 {
                    let token_array: [u8; 32] = match token_bytes.try_into() {
                        Ok(a) => a,
                        Err(_) => return Err(Error::Custom("Invalid token_id length".to_string())),
                    };
                    let token_id_opt = pallas::Base::from_repr(token_array);
                    if bool::from(token_id_opt.is_some()) {
                        return Ok(token_id_opt.unwrap());
                    }
                    return Err(Error::Custom("Invalid token_id in database".to_string()));
                }
                Err(Error::Custom(format!("Invalid token_id length in database")))
            }
            Ok(None) => Err(Error::Custom(format!("Token not found: {}", identifier))),
            Err(e) => Err(Error::Custom(format!("Database error: {:?}", e))),
        }
    }

    /// Get aliases mapped by token
    pub async fn get_aliases_mapped_by_token(&self) -> Result<HashMap<String, String>> {
        let aliases = self.wallet.get_aliases()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        let mut map = HashMap::new();
        for alias in aliases {
            map.insert(alias.token_id, alias.alias);
        }
        Ok(map)
    }

    /// Get default address
    pub async fn default_address(&self) -> Result<Address> {
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        match addresses.first() {
            Some(addr) => {
                let secret_bytes: [u8; 32] = bs58::decode(&addr.secret)
                    .into_vec()
                    .expect("Invalid secret encoding")
                    .as_slice().try_into().unwrap();
                let secret = SecretKey::from_bytes(secret_bytes).unwrap();
                let public = PublicKey::from_secret(secret);
                let std_addr = dwow_sdk::crypto::keypair::StandardAddress::from_public(self.network, public);
                Ok(std_addr.into())
            }
            None => Err(Error::Custom("No addresses in wallet".to_string())),
        }
    }

    /// Get all addresses
    pub async fn addresses(&self) -> Result<Vec<(u64, PublicKey, SecretKey, u64)>> {
        let addrs = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        let mut result: Vec<(u64, PublicKey, SecretKey, u64)> = vec![];
        for a in addrs {
            let secret_bytes: [u8; 32] = bs58::decode(&a.secret)
                .into_vec()
                .expect("Invalid secret encoding")
                .as_slice().try_into().unwrap();
            let secret = SecretKey::from_bytes(secret_bytes).unwrap();
            let public = PublicKey::from_secret(secret);
            result.push((a.id as u64, public, secret, a.created_at_height as u64));
        }

        Ok(result)
    }

    /// Get default secret key for the wallet
    pub async fn default_secret(&self) -> Result<SecretKey> {
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        match addresses.first() {
            Some(addr) => {
                let secret_bytes: [u8; 32] = bs58::decode(&addr.secret)
                    .into_vec()
                    .expect("Invalid secret encoding")
                    .as_slice().try_into().unwrap();
                Ok(SecretKey::from_bytes(secret_bytes).unwrap())
            }
            None => Err(Error::Custom("No addresses in wallet".to_string())),
        }
    }

    /// Append fee call to transaction using NativeToken::FeeV1
    ///
    /// Note: This requires DARK coins in the wallet and proper fee circuit integration.
    pub async fn append_fee_call(
        &self,
        _tx: &Transaction,
        _tree: &MerkleTree,
        _fee_pk: &ProvingKey,
        _fee_zkbin: &ZkBinary,
        _spent_coins: Option<&[MoneyV3Note]>,
    ) -> Result<(ContractCall, Vec<Proof>, Vec<SecretKey>)> {
        Err(Error::Custom(
            "append_fee_call not yet implemented for Money V3. \
             Fee payment requires NativeToken::FeeV1 integration with DARK coins.".to_string(),
        ))
    }

    /// Attach fee to transaction
    ///
    /// Builds a NativeToken::FeeV1 call using the wallet's first DARK coin
    /// and appends it as a root-level call in the transaction.
    pub async fn attach_fee(&self, tx: &mut Transaction, _fee: u64) -> Result<()> {
        use crate::contract_imports::native_token::{
            DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
            NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
        };
        use dwow_core::zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses};
        use dwow_sdk::crypto::{BaseBlind, MerkleNode};
        use dwow_serial::Encodable;
        use rand::rngs::OsRng;

        const DEFAULT_FEE: u64 = 42_000_000;

        // Get DARK coin for fee
        let dark_token_id_str = format!("{:?}", DRKW_TOKEN_ID);
        let dark_coin_records = self.wallet.get_token_coins(&dark_token_id_str, false)
            .map_err(|e| Error::Custom(format!("Failed to get DARK coins: {:?}", e)))?;

        if dark_coin_records.is_empty() {
            return Err(Error::Custom(
                "No DARK coins available for fee payment.".to_string(),
            ));
        }

        let dark_coin = &dark_coin_records[0];
        let dark_secret_bytes = bs58::decode(&dark_coin.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid DARK secret key length".to_string()))?;
        let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse DARK secret key".to_string()))?;

        let dark_merkle_proof = self.wallet.get_merkle_proof(&dark_coin.coin_id)
            .map_err(|e| Error::Custom(format!("Failed to get DARK Merkle proof: {:?}", e)))?;

        let dark_merkle_path: Vec<MerkleNode> = dark_merkle_proof
            .siblings
            .iter()
            .map(|s| {
                let bytes: [u8; 32] = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
                Ok(MerkleNode::from_bytes(bytes)
                    .ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        let dark_coin_blind_bytes = bs58::decode(&dark_coin.coin_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid coin blind length".to_string()))?;
        let dark_coin_blind = pallas::Base::from_repr(dark_coin_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid coin blind".to_string()))?;

        // Load fee ZK binary and build fee proof
        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(0, &fee_circuit);

        let fee_input = FeeCallInput {
            value: dark_coin.value,
            token_id: DRKW_TOKEN_ID,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: dark_coin_blind,
            leaf_position: dark_coin.leaf_position,
            merkle_path: dark_merkle_path,
            secret: dark_secret,
            ephemeral_signature_secret: SecretKey::random(&mut OsRng),
        };

        let dark_public_key = PublicKey::from_secret(dark_secret);
        let change_blind = BaseBlind::random(&mut OsRng);
        let fee_output = FeeCallOutput {
            recipient: dark_public_key,
            value: dark_coin.value.saturating_sub(DEFAULT_FEE),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: change_blind.inner(),
        };

        let fee_builder = FeeCallBuilder {
            input: fee_input,
            output: fee_output,
            fee_zkbin,
            fee_pk,
            fee: DEFAULT_FEE,
        };

        let fee_debris = fee_builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build fee: {:?}", e)))?;

        // Encode fee params into call data (FeeV1 = 0x00)
        let mut fee_call_data = vec![0x00u8];
        fee_debris.params.encode(&mut fee_call_data)
            .map_err(|e| Error::Custom(format!("Failed to encode fee params: {:?}", e)))?;

        let fee_call = ContractCall {
            contract_id: *NATIVE_TOKEN_CONTRACT_ID,
            data: fee_call_data,
        };

        // Append fee as root-level call (no parent, no children)
        tx.calls.push(dwow_core::tx::DarkLeaf {
            data: fee_call,
            parent_index: None,
            children_indexes: vec![],
        });
        tx.proofs.push(vec![]);

        Ok(())
    }

    /// Mark coins from a transaction as spent in the wallet database.
    pub async fn mark_tx_spend(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<()> {
        // Get all unspent coins from wallet to match against
        let unspent_coins = self.wallet.get_coins(false)
            .map_err(|e| Error::Custom(format!("Failed to get coins: {:?}", e)))?;

        // For each call in the transaction that uses Money V3
        for call in &tx.calls {
            let contract_id = call.data.contract_id;
            if contract_id != *MONEY_V3_CONTRACT_ID.get().unwrap() {
                continue;
            }

            // Parse the function code to determine if this is a burn/transfer
            let Some(function_code) = call.data.data.first() else {
                continue;
            };

            // Skip function code byte and decode params
            let params_data = &call.data.data[1..];

            // Match on function code to decode appropriate params type
            match function_code {
                // BurnV1 (0x03) or TransferV1 (0x04) - both spend coins
                0x03 | 0x04 => {
                    // Decode TransferParamsV1 or BurnParamsV1
                    let params = if *function_code == 0x04 {
                        // TransferV1
                        match deserialize_partial::<TransferParamsV1>(params_data) {
                            Ok((p, _)) => p,
                            Err(e) => {
                                output.push(format!(
                                    "Failed to decode TransferParamsV1: {:?}",
                                    e
                                ));
                                continue;
                            }
                        }
                    } else {
                        // BurnV1 - decode as TransferParamsV1 for Input extraction
                        match deserialize_partial::<TransferParamsV1>(params_data) {
                            Ok((p, _)) => p,
                            Err(e) => {
                                output.push(format!(
                                    "Failed to decode Burn params: {:?}",
                                    e
                                ));
                                continue;
                            }
                        }
                    };

                    // For each input in the params, find matching coin in wallet
                    for input in &params.inputs {
                        // Find matching coin by comparing signature_public
                        // signature_public in Input is poseidon_hash(secret)
                        for coin in &unspent_coins {
                            // Decode the secret from wallet
                            if let Ok(secret_bytes) = bs58::decode(&coin.secret).into_vec() {
                                if secret_bytes.len() == 32 {
                                    let secret_array: [u8; 32] = match secret_bytes.clone().try_into() {
                                        Ok(a) => a,
                                        Err(_) => continue,
                                    };
                                    // Use SecretKey which has inner() returning pallas::Base
                                    if let Ok(secret) = SecretKey::from_bytes(secret_array) {
                                        let pub_key = poseidon_hash([secret.inner()]);
                                        if pub_key == input.signature_public {
                                            // Found matching coin - mark as spent
                                            if let Err(e) = self.wallet.mark_coin_spent(&coin.coin_id, 0) {
                                                output.push(format!(
                                                    "Failed to mark coin {} as spent: {:?}",
                                                    coin.coin_id, e
                                                ));
                                            } else {
                                                output.push(format!(
                                                    "Marked coin {} as spent (value: {}, token: {})",
                                                    coin.coin_id, coin.value, coin.token_id
                                                ));
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Unspent money coins after block height
    pub fn unspent_money_coins_after(
        &self,
        height: &u32,
        _output: &mut Vec<String>,
    ) -> WalletDbResult<Vec<MoneyV3Note>> {
        // Get all coins spent_at_height > height
        // We need to get unspent coins (spent=0) that were created at or after height
        // Actually the semantics are: unspent coins that were created after height
        // But our wallet only stores created_at_height, not spent_at_height for unspent coins
        // For simplicity, return coins created after the given height
        let all_coins = self.wallet.get_coins(false)?;

        // Filter coins where created_at_height > *height
        let filtered: Vec<CoinRecord> = all_coins
            .into_iter()
            .filter(|c| c.created_at_height > *height)
            .collect();

        // Convert to MoneyV3Note - this requires secret data which we may not have
        // For now, return empty since we can't reconstruct full notes without secrets
        let _ = filtered;
        Ok(vec![])
    }

    /// Remove money coins after block height
    pub fn remove_money_coins_after(
        &self,
        height: &u32,
        _output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        self.wallet.remove_coins_after(*height)?;
        Ok(())
    }

    /// Check if call is a money fee (NativeToken::FeeV1)
    pub fn is_money_fee(&self, call: &ContractCall) -> bool {
        call.contract_id == *NATIVE_TOKEN_CONTRACT_ID &&
            call.data.first() == Some(&0x00) // FeeV1 function code
    }

    /// Initialize money functionality
    pub async fn initialize_money(&self, output: &mut Vec<String>) -> Result<()> {
        // Wallet database is already initialized with tables via initialize_wallet()
        // For darkwow-devnet, we don't need special money initialization
        output.push("Money V3 initialized".to_string());
        Ok(())
    }

    /// Money keygen
    pub async fn money_keygen(&self, output: &mut Vec<String>) -> Result<Keypair> {
        use dwow_sdk::crypto::Keypair;
        use rand::rngs::OsRng;

        // Generate new keypair
        let keypair = Keypair::random(&mut OsRng);

        // Encode to base58
        let public_str = bs58::encode(keypair.public.to_bytes()).into_string();
        let secret_bytes: [u8; 32] = keypair.secret.inner().to_repr();
        let secret_str = bs58::encode(secret_bytes).into_string();

        // Check if this is the first address (set as default)
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;
        let is_default = addresses.is_empty();

        // Store in database
        self.wallet.insert_address(&public_str, &secret_str, is_default, 0)
            .map_err(|e| Error::Custom(format!("Failed to store address: {:?}", e)))?;

        // Also insert secret into coin_secrets so wallet can decrypt notes sent to this address
        // Use empty string "" for coin_id (same pattern as import_money_secrets)
        self.wallet.insert_secret(&secret_str, "")
            .map_err(|e| Error::Custom(format!("Failed to insert secret: {:?}", e)))?;

        output.push(format!("Generated new address: {}", &public_str[..16]));
        output.push(format!("Address (bs58): {public_str}"));
        output.push(format!("Secret (hex): {}", hex::encode(secret_bytes)));

        Ok(keypair)
    }

    /// Money balance
    pub async fn money_balance(&self) -> Result<HashMap<String, u64>> {
        let mut balances: HashMap<String, u64> = HashMap::new();

        // Get all unspent coins
        let coin_records = self.wallet.get_coins(false).map_err(|e| Error::Custom(format!("{:?}", e)))?;

        for record in coin_records {
            *balances.entry(record.token_id).or_insert(0) += record.value;
        }

        Ok(balances)
    }

    /// Set default address
    pub async fn set_default_address(&self, _key_id: usize) -> Result<()> {
        // TODO: Implement properly
        Err(Error::Custom("Not implemented".to_string()))
    }

    /// Mining config
    pub async fn mining_config(
        &self,
        _index: usize,
        _spend_hook: Option<FuncId>,
        _user_data: Option<pallas::Base>,
        _output: &mut Vec<String>,
    ) -> Result<()> {
        // TODO: Implement properly
        Err(Error::Custom("Not implemented".to_string()))
    }

    /// Import money secrets
    pub async fn import_money_secrets(&self, secrets: Vec<SecretKey>, output: &mut Vec<String>) -> Result<Vec<SecretKey>> {
        for secret in &secrets {
            let secret_str = bs58::encode(secret.inner().to_repr()).into_string();
            self.wallet.insert_secret(&secret_str, "").map_err(|e| Error::Custom(format!("Failed to insert secret: {:?}", e)))?;
            output.push(format!("Imported secret: {}", &secret_str[..8]));
        }
        Ok(secrets)
    }

    /// Unspent coin
    pub async fn unspend_coin(&self, coin: &pallas::Base) -> Result<()> {
        let coin_id = bs58::encode(coin.to_repr()).into_string();
        self.wallet.mark_coin_unspent(&coin_id).map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    /// Get aliases
    pub async fn get_aliases(
        &self,
        _alias: Option<String>,
        _token_id: Option<TokenId>,
    ) -> Result<HashMap<String, TokenId>> {
        let aliases = self.wallet.get_aliases()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        // If specific alias requested, return just that one
        if let Some(alias_str) = _alias {
            for a in aliases {
                if a.alias == alias_str {
                    if let Ok(tid) = TokenId::from_str(&a.token_id) {
                        let mut result = HashMap::new();
                        result.insert(alias_str, tid);
                        return Ok(result)
                    }
                }
            }
            return Ok(HashMap::new())
        }

        // If specific token_id requested, find its alias
        if let Some(token_id) = _token_id {
            for a in aliases {
                if a.token_id == token_id.to_string() {
                    let mut result = HashMap::new();
                    result.insert(a.alias, token_id);
                    return Ok(result)
                }
            }
            return Ok(HashMap::new())
        }

        // Return all aliases
        let mut result = HashMap::new();
        for a in aliases {
            // Parse the token_id string to TokenId
            if let Ok(tid) = TokenId::from_str(&a.token_id) {
                result.insert(a.alias, tid);
            }
        }
        Ok(result)
    }

    /// Remove alias
    pub async fn remove_alias(&self, _alias: String, _output: &mut Vec<String>) -> Result<()> {
        // Note: Would need a remove_alias method in walletdb to implement fully
        Err(Error::Custom("remove_alias not yet implemented".to_string()))
    }

    /// Add alias
    pub async fn add_alias(
        &self,
        alias: String,
        token_id: TokenId,
        output: &mut Vec<String>,
    ) -> Result<()> {
        self.wallet.insert_alias(&alias, &token_id.to_string(), 0)
            .map_err(|e| Error::Custom(format!("Failed to store alias: {:?}", e)))?;
        output.push(format!("Added alias {} for token (stored)", alias));
        Ok(())
    }

    /// Unfreeze mint authorities after height (stub)
    pub fn unfreeze_mint_authorities_after(&self, _height: &u32, _output: &mut Vec<String>) -> WalletDbResult<()> {
        Err(WalletDbError::GenericError)
    }

    /// Unlock deploy authorities after height (stub)
    pub fn unlock_deploy_authorities_after(&self, _height: &u32, _output: &mut Vec<String>) -> WalletDbResult<()> {
        Err(WalletDbError::GenericError)
    }

    /// Remove deploy history after height (stub)
    pub fn remove_deploy_history_after(&self, _height: &u32, _output: &mut Vec<String>) -> WalletDbResult<()> {
        Err(WalletDbError::GenericError)
    }

    /// Reset deploy authorities
    pub fn reset_deploy_authorities(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        self.wallet.remove_deploy_authorities()
    }

    /// Reset deploy history (stub)
    pub fn reset_deploy_history(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        // Stub - deploy history not yet implemented
        Ok(())
    }

    /// Initialize deployooor
    pub async fn initialize_deployooor(&self, output: &mut Vec<String>) -> Result<()> {
        output.push("Deployooor initialized".to_string());
        Ok(())
    }

    /// Get mint authorities (stub)
    pub async fn get_mint_authorities(&self) -> Result<Vec<(pallas::Base, SecretKey, pallas::Base, bool, Option<u32>)>> {
        Err(Error::Custom("get_mint_authorities not yet implemented".to_string()))
    }

    /// Mint token (stub)
    pub async fn mint_token(&self, _token_id: TokenId, _amount: u64, _recipient: Option<PublicKey>) -> Result<Transaction> {
        Err(Error::Custom("mint_token not yet implemented for Money V3".to_string()))
    }

    /// Freeze token (stub)
    pub async fn freeze_token(&self, _token_id: TokenId, _freeze: bool, _height: Option<u32>) -> Result<Transaction> {
        Err(Error::Custom("freeze_token not yet implemented for Money V3".to_string()))
    }

    /// Deploy auth keygen — generates a new deploy authority keypair
    /// and persists it to the wallet database.
    pub async fn deploy_auth_keygen(&self, output: &mut Vec<String>) -> Result<SecretKey> {
        let keypair = self.generate_deploy_authority();
        let contract_id = Drk::derive_contract_id(&keypair);
        let secret = keypair.secret;
        let secret_hex = hex::encode(secret.inner().to_repr());
        let contract_id_str = bs58::encode(contract_id.to_bytes()).into_string();
        let secret_str = bs58::encode(secret.inner().to_repr()).into_string();

        self.wallet.insert_deploy_auth(&contract_id_str, &secret_str)
            .map_err(|e| Error::Custom(format!("Failed to persist deploy authority: {:?}", e)))?;

        output.push(format!("Contract ID: {}", contract_id_str));
        output.push(format!("Secret (hex): {}", secret_hex));
        output.push(format!("Public Key: {}", bs58::encode(keypair.public.to_bytes()).into_string()));

        Ok(secret)
    }

    /// List deploy authorities stored in the wallet database.
    pub async fn list_deploy_auth(&self) -> Result<Vec<(ContractId, SecretKey, bool, Option<u32>)>> {
        let rows = self.wallet.get_deploy_authorities()
            .map_err(|e| Error::Custom(format!("Failed to get deploy authorities: {:?}", e)))?;

        let mut result = vec![];
        for (cid_str, secret_str, is_locked, created_at_height) in rows {
            let cid_bytes: [u8; 32] = bs58::decode(&cid_str)
                .into_vec()
                .map_err(|e| Error::Custom(format!("Invalid contract_id: {}", e)))?
                .try_into()
                .map_err(|_| Error::Custom("Invalid contract_id length".to_string()))?;
            let contract_id = ContractId::from_bytes(cid_bytes)
                .map_err(|_| Error::Custom("Invalid contract_id bytes".to_string()))?;

            let secret_bytes: [u8; 32] = bs58::decode(&secret_str)
                .into_vec()
                .map_err(|e| Error::Custom(format!("Invalid secret: {}", e)))?
                .try_into()
                .map_err(|_| Error::Custom("Invalid secret length".to_string()))?;
            let secret = SecretKey::from_bytes(secret_bytes)
                .map_err(|_| Error::Custom("Invalid secret bytes".to_string()))?;

            result.push((contract_id, secret, is_locked, created_at_height));
        }

        Ok(result)
    }

    /// Register a contract ID for runtime use. Persists to wallet DB so
    /// subsequent `drk` invocations automatically load it.
    pub fn register_contract_id(&self, name: &str, cid: ContractId) -> Result<()> {
        let cid_str = bs58::encode(cid.to_bytes()).into_string();
        // Persist to wallet DB
        self.wallet.register_contract(name, &cid_str)
            .map_err(|e| Error::Custom(format!("Failed to persist contract registry: {:?}", e)))?;
        // Also set the in-process OnceLock immediately
        crate::contract_imports::register_contract_id(name, cid)
            .map_err(|e| Error::Custom(e))
    }

    /// Lock contract (stub)
    pub async fn lock_contract(&self, _contract_id: ContractId, _lock_height: u32, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("lock_contract not yet implemented".to_string()))
    }

    /// Get deploy auth history (stub)
    pub async fn get_deploy_auth_history(&self) -> Result<Vec<(String, String, u32)>> {
        Err(Error::Custom("get_deploy_auth_history not yet implemented".to_string()))
    }

    /// Get deploy history record data (stub)
    pub async fn get_deploy_history_record_data(&self, _tx_hash: &String) -> Result<Option<Vec<u8>>> {
        Err(Error::Custom("get_deploy_history_record_data not yet implemented".to_string()))
    }

    /// Init swap (stub)
    pub async fn init_swap(
        &self,
        _value_pair: (u64, u64),
        _token_pair: (TokenId, TokenId),
        _secret0: Option<&pallas::Base>,
        _secret1: Option<&pallas::Base>,
        _other_swap_data: Option<&PartialSwapData>,
    ) -> Result<PartialSwapData> {
        Err(Error::Custom("init_swap not yet implemented for Money V3".to_string()))
    }

    /// Join swap (stub)
    pub async fn join_swap(
        &self,
        _partial_swap_data: PartialSwapData,
        _secret0: Option<&pallas::Base>,
        _secret1: Option<&pallas::Base>,
        _other_swap_data: Option<&PartialSwapData>,
    ) -> Result<Transaction> {
        Err(Error::Custom("join_swap not yet implemented for Money V3".to_string()))
    }

    /// Inspect swap (stub)
    pub async fn inspect_swap(&self, _data: Vec<u8>, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("inspect_swap not yet implemented for Money V3".to_string()))
    }

    /// Sign swap (stub)
    pub async fn sign_swap(&self, _tx: &mut Transaction) -> Result<()> {
        Err(Error::Custom("sign_swap not yet implemented for Money V3".to_string()))
    }

    /// Invoke a smart contract function
    ///
    /// This is a universal contract invocation method that can call any function
    /// on any deployed contract without needing contract-specific CLI code.
    ///
    /// # Arguments
    /// * `contract_id_or_name` - Contract ID (Base58 encoded) or name (e.g., "dao_escrow")
    /// * `function` - Function name to call (e.g., "enable_drain_protection")
    /// * `params` - JSON string with function parameters
    /// * `proofs` - ZK proofs for functions that require them; use `vec![]` for non-ZK functions
    pub async fn invoke_contract(
        &self,
        contract_id_or_name: &str,
        function: &str,
        params: Option<&str>,
        proofs: Vec<Vec<u8>>,
    ) -> Result<Transaction> {
        use dwow_serial::Encodable;
        use crate::contract_imports::dao_escrow::EnableDrainProtectionParamsV1;
        use dwow_drain_protection_contract::model::InitializeParamsV1 as DrainInitParamsV1;
        use dwow_drain_protection_contract::model::DrainConfig;

        // First try to look up as contract name (e.g., "dao_escrow")
        // If not found, try to parse as Base58 contract ID
        let metadata = crate::contract_metadata::CONTRACT_METADATA_REGISTRY
            .get(contract_id_or_name)
            .or_else(|| {
                // Try to parse as contract ID (Base58)
                let result: Option<ContractId> = bs58::decode(contract_id_or_name)
                    .into_vec()
                    .ok()
                    .and_then(|v| v.try_into().ok())
                    .map(|bytes: [u8; 32]| ContractId::from_bytes(bytes).ok())
                    .flatten();
                // For now, we can't look up by ID since registry stores by name
                // This would need runtime registration
                result.and_then(|_| None)
            })
            .ok_or_else(|| Error::Custom(format!("Unknown contract: {}", contract_id_or_name)))?;

        let func_sig = metadata.get_function(function)
            .ok_or_else(|| Error::Custom(format!("Unknown function: {} on contract {}", function, metadata.name)))?;

        // Get the actual contract ID from the runtime registry
        let contract_id = match metadata.name {
            "dao_escrow" => *crate::contract_imports::DAO_ESCROW_CONTRACT_ID.get()
                .ok_or_else(|| Error::Custom("DAO-Escrow contract not initialized".to_string()))?,
            "drain_protection" => *crate::contract_imports::DRAIN_PROTECTION_CONTRACT_ID.get()
                .ok_or_else(|| Error::Custom("DrainProtection contract not initialized".to_string()))?,
            "money_v3" => *crate::contract_imports::MONEY_V3_CONTRACT_ID.get()
                .ok_or_else(|| Error::Custom("MoneyV3 contract not initialized".to_string()))?,
            _ => return Err(Error::Custom(format!("Contract {} not registered in runtime", metadata.name))),
        };

        // Build call data based on contract and function
        let mut call_data = vec![func_sig.code];

        match (metadata.name, function) {
            // DAO-Escrow: enable_drain_protection
            ("dao_escrow", "enable_drain_protection") => {
                #[derive(serde::Deserialize)]
                struct EnableDrainProtectionJson {
                    dao_escrow_bulla: String,
                    drain_protection_bulla: String,
                }

                let json_params = params.ok_or_else(|| Error::Custom("enable_drain_protection requires params".to_string()))?;
                let json: EnableDrainProtectionJson = serde_json::from_str(json_params)
                    .map_err(|e| Error::Custom(format!("Invalid JSON params: {}", e)))?;

                let dao_escrow_bulla_bytes = bs58::decode(&json.dao_escrow_bulla)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid dao_escrow_bulla Base58: {}", e)))?;
                let dao_escrow_bulla_bytes: [u8; 32] = dao_escrow_bulla_bytes
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid dao_escrow_bulla length".to_string()))?;
                let dao_escrow_bulla = pallas::Base::from_repr(dao_escrow_bulla_bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid dao_escrow_bulla".to_string()))?;

                let drain_protection_bulla_bytes = bs58::decode(&json.drain_protection_bulla)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid drain_protection_bulla Base58: {}", e)))?;
                let drain_protection_bulla_bytes: [u8; 32] = drain_protection_bulla_bytes
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid drain_protection_bulla length".to_string()))?;
                let drain_protection_bulla = pallas::Base::from_repr(drain_protection_bulla_bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid drain_protection_bulla".to_string()))?;

                let params = EnableDrainProtectionParamsV1 {
                    dao_escrow_bulla,
                    drain_protection_bulla,
                };
                params.encode(&mut call_data)
                    .map_err(|e| Error::Custom(format!("Failed to encode params: {}", e)))?;
            }

            // DrainProtection: initialize
            ("drain_protection", "initialize") => {
                #[derive(serde::Deserialize)]
                struct DrainProtectionInitJson {
                    fund_id: String,
                    spend_authority: String,
                    dao_escrow_bulla: String,
                }

                let json_params = params.ok_or_else(|| Error::Custom("initialize requires params".to_string()))?;
                let json: DrainProtectionInitJson = serde_json::from_str(json_params)
                    .map_err(|e| Error::Custom(format!("Invalid JSON params: {}", e)))?;

                let fund_id_bytes = bs58::decode(&json.fund_id)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid fund_id Base58: {}", e)))?;
                let fund_id_bytes: [u8; 32] = fund_id_bytes
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid fund_id length".to_string()))?;
                let fund_id = pallas::Base::from_repr(fund_id_bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid fund_id".to_string()))?;

                let spend_authority_bytes = bs58::decode(&json.spend_authority)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid spend_authority Base58: {}", e)))?;
                let spend_authority_pubkey_bytes: [u8; 32] = spend_authority_bytes
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid spend_authority length".to_string()))?;
                let spend_authority = PublicKey::from_bytes(spend_authority_pubkey_bytes)
                    .map_err(|_| Error::Custom("Invalid spend_authority".to_string()))?;

                let dao_escrow_bulla_bytes = bs58::decode(&json.dao_escrow_bulla)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid dao_escrow_bulla Base58: {}", e)))?;
                let dao_escrow_bulla_bytes: [u8; 32] = dao_escrow_bulla_bytes
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid dao_escrow_bulla length".to_string()))?;
                let dao_escrow_bulla = pallas::Base::from_repr(dao_escrow_bulla_bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid dao_escrow_bulla".to_string()))?;

                let params = DrainInitParamsV1 {
                    fund_id,
                    spend_authority,
                    dao_escrow_bulla,
                    drain_config: DrainConfig {
                        graduated_tiers: None,
                        exit_queue: None,
                        circuit_breaker: None,
                        guardian_pause: None,
                        observation_period: None,
                        split_proposals: None,
                        no_loss_reserve: None,
                        dead_mans_switch: None,
                    },
                    instance_seed: [0u8; 32],
                };
                params.encode(&mut call_data)
                    .map_err(|e| Error::Custom(format!("Failed to encode params: {}", e)))?;
            }

            // Pre-flight ZK proof validation: ensure ZK-requiring functions
            // have proofs attached before building the transaction. This catches
            // the common mistake of calling a ZK function without generating a
            // proof — which would fail consensus silently.
            _ if func_sig.requires_proof => {
                if proofs.is_empty() {
                    return Err(Error::Custom(format!(
                        "ZK proof required: {} function '{}' requires a ZK proof \
                         (circuit: {:?}) but no proofs were provided. \
                         Generate a proof using the contract's client module \
                         before calling this function.",
                        metadata.name,
                        function,
                        func_sig.proof_circuit,
                    )));
                }
                for (i, proof) in proofs.iter().enumerate() {
                    if proof.is_empty() {
                        return Err(Error::Custom(format!(
                            "ZK proof {i} for {}::{} is empty. \
                             Each proof must contain valid Halo2 proof bytes.",
                            metadata.name, function,
                        )));
                    }
                }
            }

            // Unknown function
            _ => {
                return Err(Error::Custom(format!(
                    "Unsupported function: {} on contract: {}",
                    function, metadata.name
                )));
            }
        }

        // Create contract call
        let contract_call = ContractCall {
            contract_id,
            data: call_data,
        };

        // Create contract call leaf with ZK proofs
        let leaf = ContractCallLeaf {
            call: contract_call,
            proofs: proofs.into_iter().map(Proof::from).collect(),
        };

        // Build transaction with fee
        let tx = crate::fee_builder::build_fee_and_finalize_tx(&self.wallet, leaf).await?;

        Ok(tx)
    }
}

// =============================================================================================
// HELPER FUNCTIONS
// =============================================================================================

/// Convert CoinRecord database records to MoneyV3Note structs.
fn coin_records_to_money_notes(records: &[CoinRecord]) -> Result<Vec<MoneyV3Note>> {
    use dwow_sdk::pasta::pallas;

    let mut notes = vec![];
    for record in records {
        let token_id_bytes = bs58::decode(&record.token_id)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid token_id length".to_string()))?;
        let token_id = pallas::Base::from_repr(token_id_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid token_id".to_string()))?;

        let spend_hook = match &record.spend_hook {
            Some(s) if s != "11111111111111111111111111111" => {
                let bytes = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid spend_hook length".to_string()))?;
                pallas::Base::from_repr(bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid spend_hook".to_string()))?
            }
            _ => pallas::Base::zero(),
        };

        let user_data = match &record.user_data {
            Some(s) if !s.is_empty() => {
                let bytes = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid user_data length".to_string()))?;
                pallas::Base::from_repr(bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid user_data".to_string()))?
            }
            _ => pallas::Base::zero(),
        };

        let coin_blind_bytes = bs58::decode(&record.coin_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid coin_blind length".to_string()))?;
        let coin_blind = pallas::Base::from_repr(coin_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid coin_blind".to_string()))?;

        let value_blind_bytes = bs58::decode(&record.value_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid value_blind length".to_string()))?;
        let value_blind = pallas::Base::from_repr(value_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid value_blind".to_string()))?;

        let token_blind_bytes = bs58::decode(&record.token_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid token_blind length".to_string()))?;
        let token_blind = pallas::Base::from_repr(token_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid token_blind".to_string()))?;

        notes.push(MoneyV3Note {
            value: record.value,
            token_id,
            spend_hook,
            user_data,
            coin_blind,
            value_blind,
            token_blind,
            memo: vec![],
        });
    }
    Ok(notes)
}
