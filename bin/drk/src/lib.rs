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

use std::{collections::HashMap, fs::create_dir_all, sync::Arc};

use bs58;

use smol::lock::RwLock;
use url::Url;

use darkfi::{
    system::ExecutorPtr,
    tx::Transaction,
    util::path::expand_path,
    zk::{proof::ProvingKey, Proof},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_serial::deserialize_partial;
use darkfi_sdk::{
    crypto::{
        keypair::{Address, Keypair, Network, PublicKey, SecretKey},
        pasta_prelude::PrimeField,
        poseidon_hash, ContractId, FuncId, MerkleTree,
    },
    pasta::pallas,
    tx::{ContractCall, TransactionHash},
};
use darkfi_money_v3_contract::client::MoneyV3Note;
use darkfi_money_v3_contract::model::{Coin, TransferParamsV1, BurnParamsV1, Input as MoneyV3Input};
use crate::contract_imports::{money::TokenId, MONEY_V3_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
use crate::swap::PartialSwapData;
use crate::walletdb::{CoinRecord, MerkleProof, TokenInfo};
use darkfi_sdk::crypto::util::FieldElemAsStr;

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

/// Wallet functionality related to transactions history
pub mod txs_history;

/// Wallet functionality related to scanned blocks
pub mod scanned_blocks;

/// Contract import graph - maps stale imports to actual crates
pub mod contract_imports;

/// Generic contract registry for dependency resolution and transaction building
pub mod contract_registry;

/// Money module (re-export from contract_imports for backwards compatibility)
pub mod money {
    pub use crate::contract_imports::money::*;
}

/// DAO module (re-export from contract_imports for backwards compatibility)
/// Note: DAO functionality is currently disabled due to contract bugs
pub mod dao {
    // pub use crate::contract_imports::dao_escrow::*;  // Disabled - needs darkfi_dao_escrow_contract

    // Stub types for backwards compatibility - DAO is disabled on this fork
    use darkfi_sdk::pasta::pallas;

    /// Stub for DaoBulla - DAO is disabled
    #[derive(Debug, Clone, Copy)]
    pub struct DaoBulla(pallas::Base);

    impl DaoBulla {
        pub fn new(_inner: pallas::Base) -> Self {
            Self(_inner)
        }
    }

    /// Stub for DaoProposalBulla - DAO is disabled
    #[derive(Debug, Clone, Copy)]
    pub struct DaoProposalBulla(pallas::Base);

    impl DaoProposalBulla {
        pub fn new(_inner: pallas::Base) -> Self {
            Self(_inner)
        }
        pub fn from_str(_s: &str) -> Result<Self, ()> {
            Err(())
        }
    }

    /// Stub for DaoParams - DAO is disabled
    #[derive(Debug, Clone)]
    pub struct DaoParams {
        pub name: String,
    }

    impl DaoParams {
        pub fn new(_name: &str) -> Self {
            Self { name: _name.to_string() }
        }
        pub fn from_toml_str(_s: &str) -> Result<Self, ()> {
            Err(())
        }
    }

    /// Stub for ProposalRecord - DAO is disabled
    #[derive(Debug, Clone)]
    pub struct ProposalRecord {
        pub proposal_bulla: pallas::Base,
    }

    /// Stub for DaoFunction - DAO is disabled
    #[derive(Debug, Clone, Copy)]
    #[repr(u8)]
    pub enum DaoFunction {
        CreateDao = 0x00,
        UpdateDao = 0x01,
        Propose = 0x02,
        Vote = 0x03,
        Exec = 0x04,
        AuthMoneyTransfer = 0x05,
    }

    /// Stub for blockwindow - DAO is disabled
    pub fn blockwindow() {
        // DAO is disabled
    }
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
        let sled_db = sled_overlay::sled::open(&db_path)?;
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
                // Create an empty Merkle tree for linear-testnet (no previous state)
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
                let std_addr = darkfi_sdk::crypto::keypair::StandardAddress::from_public(self.network, public);
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
    /// Note: This is a stub. Full implementation requires NativeToken::FeeV1.
    pub async fn attach_fee(&self, _tx: &mut Transaction, _fee: u64) -> Result<()> {
        Err(Error::Custom(
            "attach_fee not yet implemented for Money V3. \
             Fee payment requires NativeToken::FeeV1 integration.".to_string(),
        ))
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
        // For linear-testnet, we don't need special money initialization
        output.push("Money V3 initialized".to_string());
        Ok(())
    }

    /// Money keygen
    pub async fn money_keygen(&self, output: &mut Vec<String>) -> Result<Keypair> {
        use darkfi_sdk::crypto::Keypair;
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

    // =============================================================================================
    // DAO STUB METHODS - DAO is disabled on this fork
    // =============================================================================================

    /// Initialize DAO (disabled)
    pub async fn initialize_dao(&self) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Get DAO trees (disabled)
    pub async fn get_dao_trees(&self) -> Result<(MerkleTree, MerkleTree)> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Get DAO by bulla (disabled)
    pub async fn get_dao_by_bulla(&self, _bulla: &pallas::Base) -> Result<Option<DaoStub>> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Get DAO proposals (disabled)
    pub async fn get_dao_proposals(&self, _name: &str) -> Result<Vec<ProposalStub>> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Get DAO proposal by bulla (disabled)
    pub async fn get_dao_proposal_by_bulla(&self, _bulla: &str) -> Result<Option<ProposalStub>> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Get DAO proposal votes (disabled)
    pub async fn get_dao_proposal_votes(&self, _bulla: &str) -> Result<Vec<VoteStub>> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Import DAO (disabled)
    pub async fn import_dao(&self, _name: &str, _params: &DaoStub, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Remove DAO (disabled)
    pub async fn remove_dao(&self, _name: &str, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO list (disabled)
    pub async fn dao_list(&self, _name: &str, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO balance (disabled)
    pub async fn dao_balance(&self, _name: &str) -> Result<HashMap<String, u64>> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO mint (disabled)
    pub async fn dao_mint(&self, _name: &str) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO propose transfer (disabled)
    pub async fn dao_propose_transfer(&self, _name: &str) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO propose generic (disabled)
    pub async fn dao_propose_generic(&self, _name: &str, _duration: u64, _user_data: pallas::Base) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO vote (disabled)
    pub async fn dao_vote(&self, _bulla: &str, _vote: u8, _weight: u64) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO exec transfer (disabled)
    pub async fn dao_exec_transfer(&self, _proposal: &ProposalStub, _early: bool) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO exec generic (disabled)
    pub async fn dao_exec_generic(&self, _proposal: &ProposalStub, _early: bool) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO mining config (disabled)
    pub async fn dao_mining_config(&self, _name: &str, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO transfer proposal tx (disabled)
    pub async fn dao_transfer_proposal_tx(&self, _proposal: &ProposalStub) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// DAO generic proposal tx (disabled)
    pub async fn dao_generic_proposal_tx(&self, _proposal: &ProposalStub) -> Result<Transaction> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Put DAO proposal (disabled)
    pub async fn put_dao_proposal(&self, _proposal: &ProposalStub) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    // =============================================================================================
    // ADDITIONAL STUB METHODS
    // =============================================================================================

    /// Unconfirm DAOs after height (disabled)
    pub async fn unconfirm_daos_after(&self, _height: u32) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Unconfirm DAO proposals after height (disabled)
    pub async fn unconfirm_dao_proposals_after(&self, _height: u32) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Unexec DAO proposals after height (disabled)
    pub async fn unexec_dao_proposals_after(&self, _height: u32) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
    }

    /// Remove DAO votes after height (disabled)
    pub async fn remove_dao_votes_after(&self, _height: u32) -> Result<()> {
        Err(Error::Custom("DAO is disabled on this fork".to_string()))
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
    pub async fn remove_alias(&self, alias: String, _output: &mut Vec<String>) -> Result<()> {
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

    /// Reset deploy authorities (stub)
    pub fn reset_deploy_authorities(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        // Stub - DAO is disabled on this fork
        Ok(())
    }

    /// Reset deploy history (stub)
    pub fn reset_deploy_history(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        // Stub - DAO is disabled on this fork
        Ok(())
    }

    /// Initialize deployooor (stub - DAO is disabled)
    pub async fn initialize_deployooor(&self, _output: &mut Vec<String>) -> Result<()> {
        Err(Error::Custom("Deployooor is disabled on this fork".to_string()))
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

    /// Deploy auth keygen (stub)
    pub async fn deploy_auth_keygen(&self, _output: &mut Vec<String>) -> Result<SecretKey> {
        Err(Error::Custom("deploy_auth_keygen not yet implemented".to_string()))
    }

    /// List deploy auth (stub)
    pub async fn list_deploy_auth(&self) -> Result<Vec<(ContractId, SecretKey, bool, Option<u32>)>> {
        Err(Error::Custom("list_deploy_auth not yet implemented".to_string()))
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
}

// =============================================================================================
// HELPER FUNCTIONS
// =============================================================================================

/// Convert CoinRecord database records to MoneyV3Note structs.
fn coin_records_to_money_notes(records: &[CoinRecord]) -> Result<Vec<MoneyV3Note>> {
    use darkfi_sdk::pasta::pallas;

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

// =============================================================================================
// STUB TYPES for DAO
// =============================================================================================

#[derive(Debug, Clone)]
pub struct DaoStub {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ProposalStub {
    pub bulla: pallas::Base,
}

#[derive(Debug, Clone)]
pub struct VoteStub {
    pub vote: u8,
}
