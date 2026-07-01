/* This file is part of DarkWow
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

//! Account Manager — unified key management for mining nodes.
//!
//! Used by `dwowd` for mining key management. Wallet integration via
//! pipeline phase_10 (imports miner's key) or direct sled access.
//! Keys can be declared in `keys.toml` or auto-generated.
//! Designed for future: BIP39 seed phrases, BIP32 HD derivation, hardware keys.

use std::path::Path;

use dwow_sdk::crypto::keypair::{Keypair, PublicKey, SecretKey};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

/// A single account — one keypair with optional metadata.
#[derive(Debug, Clone)]
pub struct Account {
    pub keypair: Keypair,
    /// Human-readable label (future: set by user)
    pub label: Option<String>,
    /// BIP32 derivation path (future: HD wallets)
    pub derivation_path: Option<String>,
}

impl Account {
    pub fn address(&self) -> String {
        use dwow_sdk::crypto::keypair::{Address, StandardAddress};
        let addr: Address = StandardAddress::from_public(
            dwow_sdk::crypto::keypair::Network::Testnet, self.keypair.public,
        ).into();
        addr.to_string()
    }

    pub fn secret_hex(&self) -> String {
        hex::encode(self.keypair.secret.inner().to_repr())
    }
}

/// Manages a collection of accounts. Both mining nodes and wallets use this.
pub struct AccountManager {
    accounts: Vec<Account>,
    default_index: usize,
    db: Option<sled::Db>,
}

impl AccountManager {
    // ========================================================================
    // Construction
    // ========================================================================

    /// Load accounts from `keys.toml` if it exists, otherwise auto-generate.
    /// The file is sled-backed: reads existing state, writes new state.
    ///
    /// Resolution order:
    ///   1. Sled cache (restart) — accounts previously persisted
    ///   2. keys.toml declaration — operator-specified keys (single source of truth)
    ///   3. Auto-generate (localnet only) — random key for dev/testing
    ///   4. Hard error (non-localnet, no keys declared) — never mine to random keys
    pub fn open(
        db: &sled::Db,
        localnet: bool,
        keys_toml: Option<&Path>,
    ) -> Result<Self, String> {
        let tree = db.open_tree("accounts")
            .map_err(|e| format!("sled open_tree: {e}"))?;

        // 1. Sled cache — restart path
        if let Some(stored) = tree.get("accounts_json")
            .map_err(|e| format!("sled get: {e}"))?
        {
            return Self::from_json(&stored, db.clone());
        }

        // 2. keys.toml declaration — operator-specified keys
        if let Some(path) = keys_toml {
            if path.exists() {
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| format!("read keys.toml: {e}"))?;
                let cfg: toml::Value = toml::from_str(&contents)
                    .map_err(|e| format!("parse keys.toml: {e}"))?;

                // Determine which section to use.
                // NODE_NAME env var selects the section, default "node0".
                let node_name = std::env::var("NODE_NAME").unwrap_or_else(|_| "node0".into());
                let hex_secret = cfg.get(&node_name)
                    .and_then(|s| s.get("wallet_secret"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!(
                        "keys.toml: section [{}] with wallet_secret not found", node_name
                    ))?;

                if hex_secret.len() != 64 {
                    return Err(format!(
                        "keys.toml: [{}].wallet_secret must be 64 hex chars, got {}",
                        node_name, hex_secret.len()
                    ));
                }

                // Build AccountManager with the declared key.
                // We create the struct directly rather than using open()+import_hex()
                // to avoid the orphan auto-generated key problem (F9).
                let bytes = hex::decode(hex_secret)
                    .map_err(|e| format!("keys.toml hex decode: {e}"))?;
                let arr = <[u8; 32]>::try_from(bytes)
                    .map_err(|_| "keys.toml: expected 32 bytes".to_string())?;
                let secret = SecretKey::from_bytes(arr)
                    .map_err(|_| "keys.toml: invalid secret key".to_string())?;
                let keypair = Keypair::new(secret);

                let manager = AccountManager {
                    accounts: vec![Account {
                        keypair,
                        label: Some(format!("{}-declared", node_name)),
                        derivation_path: None,
                    }],
                    default_index: 0,
                    db: Some(db.clone()),
                };

                // Cache in sled for fast restart
                manager.save(&tree)?;

                return Ok(manager);
            }
        }

        // 3. Auto-generate (localnet only) — no keys declared, no sled state
        if localnet {
            let account = Account {
                keypair: Keypair::random(&mut rand::rngs::OsRng),
                label: Some("default".into()),
                derivation_path: None,
            };

            let manager = AccountManager {
                accounts: vec![account],
                default_index: 0,
                db: Some(db.clone()),
            };

            manager.save(&tree)?;

            return Ok(manager);
        }

        // 4. Hard error — non-localnet, no keys declared
        Err("No keys declared and no cached keys found. \
             Provide a keys.toml with --keys or set LOCALNET=true for auto-generation."
            .into())
    }

    /// Import an account from a hex secret (from keys.toml or env var).
    pub fn import_hex(&mut self, hex_secret: &str) -> Result<usize, String> {
        let hex_secret = hex_secret.trim();
        let bytes = hex::decode(hex_secret).map_err(|e| format!("hex decode: {e}"))?;
        let arr = <[u8; 32]>::try_from(bytes)
            .map_err(|_| "expected 32 bytes".to_string())?;
        let secret = SecretKey::from_bytes(arr)
            .map_err(|_| "invalid secret key".to_string())?;

        // Check for duplicate (case-insensitive hex comparison)
        let hex_lower = hex_secret.to_lowercase();
        if let Some(idx) = self.accounts.iter().position(|a| a.secret_hex().to_lowercase() == hex_lower) {
            return Err(format!(
                "Secret already imported at index {} (label: {})",
                idx,
                self.accounts[idx].label.as_deref().unwrap_or("unnamed")
            ));
        }

        let keypair = Keypair::new(secret);
        let account = Account {
            keypair,
            label: Some(format!("imported-{}", self.accounts.len())),
            derivation_path: None,
        };
        self.accounts.push(account);
        Ok(self.accounts.len() - 1)
    }

    /// Generate a new random account.
    pub fn generate(&mut self) -> usize {
        let account = Account {
            keypair: Keypair::random(&mut rand::rngs::OsRng),
            label: Some(format!("generated-{}", self.accounts.len())),
            derivation_path: None,
        };
        self.accounts.push(account);
        self.accounts.len() - 1
    }

    // ========================================================================
    // Access
    // ========================================================================

    pub fn default_account(&self) -> Result<&Account, String> {
        if self.accounts.is_empty() {
            return Err("No accounts in AccountManager".into());
        }
        Ok(&self.accounts[self.default_index])
    }

    pub fn default_public_key(&self) -> Result<PublicKey, String> {
        Ok(self.default_account()?.keypair.public)
    }

    pub fn default_index(&self) -> usize {
        self.default_index
    }

    pub fn set_default(&mut self, index: usize) -> Result<(), String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range", index));
        }
        self.default_index = index;
        Ok(())
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn secrets(&self) -> Vec<SecretKey> {
        self.accounts.iter().map(|a| a.keypair.secret).collect()
    }

    // ========================================================================
    // Persistence (sled)
    // ========================================================================

    fn save(&self, tree: &sled::Tree) -> Result<(), String> {
        let json = self.to_json()?;
        tree.insert("accounts_json", json.as_bytes()).map_err(|e| format!("sled write: {e}"))?;
        tree.flush().map_err(|e| format!("sled flush: {e}"))?;
        Ok(())
    }

    /// Save current state using stored db reference. Call after import or generate.
    pub fn persist(&self) -> Result<(), String> {
        match &self.db {
            Some(db) => {
                let tree = db.open_tree("accounts")
                    .map_err(|e| format!("sled open: {e}"))?;
                self.save(&tree)
            }
            None => Err("AccountManager: no db reference — cannot persist".into()),
        }
    }

    // ========================================================================
    // Serialization (JSON — simple, inspectable)
    // ========================================================================

    fn to_json(&self) -> Result<String, String> {
        let entries: Vec<serde_json::Value> = self.accounts.iter().map(|a| {
            serde_json::json!({
                "secret_hex": a.secret_hex(),
                "address": a.address(),
                "label": a.label,
                "derivation_path": a.derivation_path,
            })
        }).collect();
        let json = serde_json::json!({
            "default_index": self.default_index,
            "accounts": entries,
        });
        serde_json::to_string_pretty(&json).map_err(|e| format!("json serialize: {e}"))
    }

    fn from_json(data: &[u8], db: sled::Db) -> Result<Self, String> {
        let json: serde_json::Value = serde_json::from_slice(data).map_err(|e| format!("json parse: {e}"))?;
        let default_index = json["default_index"].as_u64()
            .ok_or("missing default_index field")? as usize;
        let entries = json["accounts"].as_array()
            .ok_or("missing accounts array")?;
        let mut accounts = Vec::new();
        for entry in entries {
            let hex_str = entry["secret_hex"].as_str()
                .ok_or("missing secret_hex")?;
            let bytes = hex::decode(hex_str)
                .map_err(|e| format!("hex decode: {e}"))?;
            let arr = <[u8; 32]>::try_from(bytes)
                .map_err(|_| "expected 32 bytes".to_string())?;
            let secret = SecretKey::from_bytes(arr)
                .map_err(|_| "invalid secret key".to_string())?;
            let keypair = Keypair::new(secret);
            accounts.push(Account {
                keypair,
                label: entry["label"].as_str().map(|s| s.to_string()),
                derivation_path: entry["derivation_path"].as_str().map(|s| s.to_string()),
            });
        }
        Ok(AccountManager { accounts, default_index, db: Some(db) })
    }

    // ========================================================================
    // Future API (structure only — not yet implemented)
    // ========================================================================

    /*
    /// Import from BIP39 seed phrase.
    pub fn from_seed_phrase(_phrase: &str, _passphrase: &str) -> Result<Self, String> {
        unimplemented!("BIP39 seed phrase import")
    }

    /// Derive from BIP32 extended key + derivation path.
    pub fn from_hd_path(_xpriv: &str, _path: &str) -> Result<Self, String> {
        unimplemented!("BIP32 HD derivation")
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_manager_generate() {
        let config = sled::Config::new().temporary(true);
        let db = config.open().unwrap();
        let mut mgr = AccountManager::open(&db, true, None).unwrap();
        assert_eq!(mgr.accounts().len(), 1);

        mgr.generate();
        assert_eq!(mgr.accounts().len(), 2);

        mgr.set_default(1).unwrap();
        assert_eq!(mgr.default_account().unwrap().secret_hex(), mgr.accounts()[1].secret_hex());
    }

    #[test]
    fn test_account_manager_import_hex() {
        let config = sled::Config::new().temporary(true);
        let db = config.open().unwrap();
        let mut mgr = AccountManager::open(&db, true, None).unwrap();
        let initial_count = mgr.accounts().len();

        // Test key: 0000...0001
        let hex_key = "0000000000000000000000000000000000000000000000000000000000000001";
        mgr.import_hex(hex_key).unwrap();
        assert_eq!(mgr.accounts().len(), initial_count + 1);
    }

    #[test]
    fn test_persist_roundtrip() {
        let config = sled::Config::new().temporary(true);
        let db = config.open().unwrap();
        let mut mgr = AccountManager::open(&db, true, None).unwrap();
        mgr.generate();
        mgr.persist().unwrap();

        let mgr2 = AccountManager::open(&db, true, None).unwrap();
        assert_eq!(mgr2.accounts().len(), 2);
    }
}
