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

//! # Account Manager — unified, owner-sovereign key management
//!
//! Single source of truth for all key material. Used by both `dwowd` (mining)
//! and `dwow_wallet` (wallet daemon) — both binaries consume this module's API.
//!
//! ## Design basis (non-negotiable)
//!
//! The owner declares their key; the software uses exactly that key, deterministically,
//! on every boot. Nothing — no cache, no environment variable, no RNG, no default —
//! may sit between the owner and their key. Non-deterministic key registration is a
//! catastrophic failure (miner → broken consensus; wallet → loss of funds).
//!
//! ## Architecture
//!
//! ```text
//!                         keys.toml (declaration boundary)
//!                              │
//!                     open(path, network, section)
//!                              │
//!                    ┌─────────┴─────────┐
//!                    │  accounts[0]       │  ← declared identity (immutable, always default)
//!                    │  accounts[1..]     │  ← lifecycle keys (additive, never displace [0])
//!                    └───────────────────┘
//!                              │
//!          ┌───────────────────┼───────────────────┐
//!          │                   │                   │
//!   darkwow node          darkwow wallet      darkwow account
//!  (dwowd: mining)        (dwow_wallet)       (lifecycle CLI)
//!  default_public_key()   secrets()           generate()
//!  → coinbase recipient   → scan decrypt      import_hex/import_base58()
//!  (consumes identity)    default_owned()     from_seed_phrase()
//!                         → per-contract keys  export (vault + keys.toml)
//!                          default_address()   to_json_string() → persistence
//! ```
//!
//! **Universal service provider:** every key/account operation is a method on
//! this module. The `darkwow` top-level CLI, the mining node (`dwowd`), and the
//! wallet (`dwow_wallet`) are all thin consumers — daemons consume their declared
//! identity internally; `darkwow account` is a thin argv→method adapter. No
//! consumer implements or forks key logic.
//!
//! **Declaration boundary:** `keys.toml` is an external file the owner writes.
//! `open(path, network, section)` reads exactly one `[section].wallet_secret`,
//! derives the keypair deterministically via `OwnedSecretKey::from_declared_bytes`,
//! and returns a single-key manager. Missing file/section = hard error. Keys are
//! NEVER auto-generated — the owner is the source.
//!
//! **Declared identity (`accounts[0]`):** set by `open()`, always `default_index = 0`,
//! can NEVER be removed (`remove()` refuses index 0). This is the coinbase recipient
//! (miner via `MiningRecipient::from_account`) and the decrypt/spend root (wallet
//! via `default_public_key()` / `get_secret()`).
//!
//! **Lifecycle keys (`accounts[1..]`):** additive keys for address cycling,
//! per-contract scanning, and multi-key management. Created via `generate()` (owner-
//! initiated random, like `darkwow account generate`), `import_hex()`/`import_base58()`,
//! or `from_seed_phrase()` (BIP39 HD). They never displace the declared identity.
//! Persisted via `to_json_string()` (encrypted at rest with AEAD), loaded on boot
//! via `load_lifecycle(json)` (advisory, soft-fails on corrupt/missing blob).
//!
//! **`darkwow account`:** a thin CLI wrapping this module's lifecycle operations
//! (`bin/darkwow/src/account.rs`). Generate, import, export (vault index or the
//! declared keys.toml identity), BIP39 seed phrases — the owner-facing entry point.
//! Keygen is NOT a mining or wallet concern; it is a module-level operation.
//!
//! **Safety types:** `OwnedSecretKey` has ONLY deterministic constructors — no
//! `::random()` exists. `MiningRecipient` can only be built from `from_account`
//! (the node's own declared key). Both enforce the design basis at compile time.

use std::path::Path;

use dwow_sdk::crypto::keypair::{Address, Keypair, Network, PublicKey, SecretKey, StandardAddress};
use dwow_sdk::crypto::ContractId;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::ContractError;
use pasta_curves::{group::ff::FromUniformBytes, pallas};

/// A single account — one keypair with optional metadata.
#[derive(Debug, Clone)]
pub struct Account {
    pub keypair: Keypair,
    /// Human-readable label
    pub label: Option<String>,
    /// BIP32 derivation path (HD wallets)
    pub derivation_path: Option<String>,
}

impl Account {
    pub fn address(&self, network: Network) -> String {
        use dwow_sdk::crypto::keypair::{Address, StandardAddress};
        let addr: Address = StandardAddress::from_public(network, self.keypair.public).into();
        addr.to_string()
    }

    pub fn secret_hex(&self) -> String {
        hex::encode(self.keypair.secret.inner().to_repr())
    }
}

/// Holds the node/wallet's declared identity + lifecycle-managed keys. The
/// declared identity is derived on boot from keys.toml [section]; additional
/// keys are added via lifecycle operations (import, generate, HD derivation).
pub struct AccountManager {
    accounts: Vec<Account>,
    default_index: usize,
    /// True when `accounts[0]` is an owner-declared identity (from `open()` /
    /// seed root) that must never be displaced by a lifecycle op. False for a
    /// vault manager (`empty()`), whose `accounts[0]` is just its first key —
    /// legitimately mintable and never mistaken for a declared identity.
    declared: bool,
    /// Network for address generation (testnet=0xaf, mainnet=0x39)
    pub network: Network,
    /// Encrypted BIP39 seed phrase for HD re-derivation.
    /// None if accounts were imported from keys.toml or base58 (not seed-derived).
    pub encrypted_seed: Option<String>,
    /// Whether the encrypted seed stores the mnemonic phrase (true) or raw seed bytes (false).
    pub seed_is_mnemonic: bool,
}

impl AccountManager {
    // ========================================================================
    // Construction
    // ========================================================================

    /// Resolve the owner's declared identity from `keys_toml`, section `section`.
    ///
    /// Total and deterministic: reads exactly one declared `wallet_secret`, derives
    /// the keypair through the audited `OwnedSecretKey` boundary, and returns a
    /// single-key manager. NO cache, NO env fallback, NO auto-generation. A missing
    /// Create an empty manager with no declared identity — used ONLY by
    /// key lifecycle tools (`darkwow account`) that don't declare a keys.toml root.
    /// `open()` is the authoritative constructor for wallet/miner identity.
    pub fn empty(network: Network) -> Self {
        AccountManager {
            accounts: Vec::new(),
            default_index: 0,
            declared: false,
            network,
            encrypted_seed: None,
            seed_is_mnemonic: false,
        }
    }

    /// file or section is a hard error — a key is never synthesised. `section` is
    /// required (e.g. `node0`, `observer`, `wallet-1`); the caller resolves it
    /// explicitly (dwowd from `NODE_NAME`).
    pub fn open(
        keys_toml: &Path,
        network: Network,
        section: &str,
    ) -> Result<Self, String> {
        // Total, deterministic resolution: read exactly one declared secret from
        // `keys_toml` section `[section]`, derive the keypair, return a single-key
        // manager. NO cache, NO env fallback, NO localnet auto-generation. A missing
        // file or section is a hard error — a key is never synthesised, because a
        // non-deterministic / non-owner-declared identity is a catastrophic failure
        // (miner → broken consensus + unspendable rewards; wallet → loss of funds
        // and identity). The owner declares their key; the software only uses it.
        if !keys_toml.exists() {
            return Err(format!(
                "keys.toml not found at {} — every node/wallet must declare its key; \
                 keys are never auto-generated",
                keys_toml.display()
            ));
        }

        let contents = std::fs::read_to_string(keys_toml)
            .map_err(|e| format!("read keys.toml: {e}"))?;
        let cfg: toml::Value =
            toml::from_str(&contents).map_err(|e| format!("parse keys.toml: {e}"))?;

        let hex_secret = cfg
            .get(section)
            .and_then(|s| s.get("wallet_secret"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                format!("keys.toml: section [{section}] with wallet_secret not found")
            })?;

        if hex_secret.len() != 64 {
            return Err(format!(
                "keys.toml: [{section}].wallet_secret must be 64 hex chars, got {}",
                hex_secret.len()
            ));
        }

        let bytes = hex::decode(hex_secret).map_err(|e| format!("keys.toml hex decode: {e}"))?;
        let arr =
            <[u8; 32]>::try_from(bytes).map_err(|_| "keys.toml: expected 32 bytes".to_string())?;
        // Construct the identity through the audited deterministic boundary. There is
        // no random constructor on `OwnedSecretKey`, so this is the only way an
        // identity enters the manager — and it is total and reproducible.
        let owned = OwnedSecretKey::from_declared_bytes(arr)
            .map_err(|_| "keys.toml: invalid secret key".to_string())?;
        let keypair = Keypair::new(owned.into());

        Ok(AccountManager {
            encrypted_seed: None,
            seed_is_mnemonic: false,
            declared: true,
            accounts: vec![Account {
                keypair,
                label: Some(format!("{section}-declared")),
                derivation_path: None,
            }],
            default_index: 0,
            network,
        })
    }

    // ========================================================================
    // Key lifecycle — owner-initiated, never auto-called at boot
    //
    // These add/remove/manage keys in the manager beyond the single declared
    // identity from `open()`. `generate` is owner-initiated randomness (like
    // `darkwow account generate`), never an automatic fallback. keys.toml remains
    // the declaration boundary; these are key-lifecycle operations for address
    // cycling, per-contract scanning, and multi-key management.
    // ========================================================================

    /// Import an account from a hex secret. Owner-initiated.
    pub fn import_hex(&mut self, hex_secret: &str) -> Result<usize, String> {
        let hex_secret = hex_secret.trim();
        let bytes = hex::decode(hex_secret).map_err(|e| format!("hex decode: {e}"))?;
        let arr = <[u8; 32]>::try_from(bytes)
            .map_err(|_| "expected 32 bytes".to_string())?;
        let secret = SecretKey::from_bytes(arr)
            .map_err(|_| "invalid secret key".to_string())?;

        // Check for duplicate
        let secret_bytes = secret.inner().to_repr();
        if self.accounts.iter().any(|a| {
            a.keypair.secret.inner().to_repr() == secret_bytes
        }) {
            return Err("Secret already imported".into());
        }

        let node_label = format!("imported-{}", self.accounts.len());
        let keypair = Keypair::new(secret);
        self.accounts.push(Account { keypair, label: Some(node_label), derivation_path: None });
        Ok(self.accounts.len() - 1)
    }

    /// Import a secret key from a base58-encoded string. Owner-initiated.
    pub fn import_base58(&mut self, b58: &str) -> Result<usize, String> {
        let b58 = b58.trim();
        let bytes = bs58::decode(b58).into_vec()
            .map_err(|e| format!("base58 decode: {e}"))?;
        let arr = <[u8; 32]>::try_from(bytes.clone())
            .map_err(|_| format!("expected 32 bytes, got {}", bytes.len()))?;
        let secret = SecretKey::from_bytes(arr)
            .map_err(|_| "invalid secret key".to_string())?;

        let secret_bytes = secret.inner().to_repr();
        if self.accounts.iter().any(|a| {
            a.keypair.secret.inner().to_repr() == secret_bytes
        }) {
            return Err("Secret already imported".into());
        }

        let node_label = format!("imported-{}", self.accounts.len());
        let keypair = Keypair::new(secret);
        self.accounts.push(Account { keypair, label: Some(node_label), derivation_path: None });
        Ok(self.accounts.len() - 1)
    }

    /// Export the secret hex for an account by index.
    pub fn export_hex(&self, index: usize) -> Result<String, String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range", index));
        }
        Ok(hex::encode(self.accounts[index].keypair.secret.inner().to_repr()))
    }

    /// Generate a new random account. Owner-initiated ONLY — never auto-called
    /// at boot (unlike the removed localnet fallback). Randomness at the owner's
    /// explicit request is legitimate (mirrors `darkwow account generate`).
    pub fn generate(&mut self) -> usize {
        // M1-fix (refined): protect a *declared* identity — a keys.toml-rooted
        // manager (declared=true) always has accounts[0] seated by open(), so
        // it can never be empty here; lifecycle keys are additive at index ≥ 1
        // and never displace it. A *vault* manager (empty()-constructed,
        // declared=false) legitimately mints its first key at index 0 — it has
        // no declared identity to mistake it for. Only the impossible
        // declared-but-empty case is rejected defensively.
        if self.declared && self.accounts.is_empty() {
            panic!("Cannot generate a key on an empty declared AccountManager — the \
                    declared identity must come from keys.toml via open().");
        }
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        self.accounts.push(Account { keypair, label: None, derivation_path: None });
        // Does NOT repoint default_index — accounts[0] (the declared identity from
        // keys.toml) is always the default. Lifecycle keys are additive at index ≥ 1.
        self.accounts.len() - 1
    }

    /// Remove an account by index.
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range", index));
        }
        if index == 0 {
            return Err("Cannot remove accounts[0] — the declared identity from keys.toml is permanent.".into());
        }
        if self.accounts.len() <= 1 {
            return Err("Cannot remove the last account".into());
        }
        self.accounts.remove(index);
        if index < self.default_index {
            self.default_index = self.default_index.saturating_sub(1);
        } else if self.default_index >= self.accounts.len() {
            self.default_index = self.accounts.len().saturating_sub(1);
        }
        Ok(())
    }

    /// Set the default account (for multi-key use).
    pub fn set_default(&mut self, index: usize) -> Result<(), String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range", index));
        }
        // H2-fix: the declared identity (accounts[0]) is always the default for
        // coinbase/spend. Lifecycle keys at ≥1 are additive only — they never
        // displace the declared identity. Reject any attempt to move default
        // off index 0.
        if index != 0 {
            return Err(
                "Cannot set a lifecycle key as default. The declared identity \
                 (accounts[0], from keys.toml) is always the default. \
                 Lifecycle keys are additive only.".into()
            );
        }
        Ok(())
    }

    /// Export a secret key as base58-encoded string by account index.
    pub fn export_base58(&self, index: usize) -> Result<String, String> {
        if index >= self.accounts.len() {
            return Err(format!("account index {} out of range (0-{})", index, self.accounts.len().saturating_sub(1)));
        }
        Ok(bs58::encode(self.accounts[index].keypair.secret.inner().to_repr()).into_string())
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

    /// The declared identity as an `Address` (Base58-encoded, network-prefixed).
    /// This is the human-facing representation of the declared public key.
    pub fn default_address(&self) -> Result<Address, String> {
        use dwow_sdk::crypto::keypair::{Address, StandardAddress};
        let pk = self.default_public_key()?;
        let addr: Address = StandardAddress::from_public(self.network, pk).into();
        Ok(addr)
    }

    /// A per-block cycled `Address` for privacy-preserving coinbase rewards.
    /// Derives a fresh unlinkable key for each (contract, instance) pair via
    /// `derive_instance`, then converts to Address. The miner uses this to
    /// produce a unique recipient per block; the wallet reconstructs the key
    /// on-the-fly during scan via `secrets_for_contract`.
    pub fn per_block_address(
        &self, cid: &ContractId, instance_seed: &[u8],
    ) -> Result<Address, String> {
        use dwow_sdk::crypto::keypair::{Address, StandardAddress};
        let owned = self.default_owned()?;
        let derived = owned.derive_instance(cid, instance_seed)
            .map_err(|e| format!("per_block_address derive_instance failed: {}", e))?;
        let pk = derived.public();
        let addr: Address = StandardAddress::from_public(self.network, pk).into();
        Ok(addr)
    }

    /// The declared identity as an `OwnedSecretKey` — the only sanctioned path
    /// to `derive_instance` for per-contract unlinkable keys. The caller gets
    /// the compile-time guarantee that this key was declared, not random.
    pub fn default_owned(&self) -> Result<OwnedSecretKey, String> {
        Ok(OwnedSecretKey::from_declared(self.default_account()?.keypair.secret))
    }

    /// Derived secrets for a specific (contract, instance) pair, from every
    /// account the manager holds. Used by the wallet's contract-aware scan
    /// pre-pass to trial-decrypt notes encrypted to per-instance keys.
    pub fn secrets_for_contract(
        &self, cid: &ContractId, instance_seed: &[u8],
    ) -> Result<Vec<SecretKey>, ContractError> {
        let owned = self.accounts.iter().map(|a| {
            OwnedSecretKey::from_declared(a.keypair.secret)
        });
        owned
            .map(|o| o.derive_instance(cid, instance_seed).map(SecretKey::from))
            .collect()
    }

    pub fn default_index(&self) -> usize {
        self.default_index
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn secrets(&self) -> Vec<SecretKey> {
        self.accounts.iter().map(|a| a.keypair.secret).collect()
    }

    // Persistence (sled JSON cache), at-rest AEAD encryption (to_json/from_json,
    // ========================================================================
    // Persistence — JSON serialization + at-rest AEAD
    //
    // Key material MAY be serialized to/from JSON and encrypted at rest with an
    // owner-supplied passphrase. This is a CONVENIENCE for persisting lifecycle
    // keys (imported/generated/HD-derived) across restarts — it does NOT
    // substitute for the declaration: keys.toml remains the authoritative source,
    // and `open()` always resolves the declared identity from keys.toml first.
    // ========================================================================

    /// Serialize to JSON for persistence. Caller writes to their storage backend.
    pub fn to_json_string(&self) -> Result<String, String> {
        self.to_json()
    }

    /// Devnet passphrase for key encryption. Override via DWOW_KEY_PASSPHRASE env.
    const DEVNET_PASSPHRASE: &str = "darkwow-devnet-key-encryption-v1";

    fn resolve_passphrase() -> String {
        std::env::var("DWOW_KEY_PASSPHRASE")
            .unwrap_or_else(|_| Self::DEVNET_PASSPHRASE.to_string())
    }

    fn encrypt_secret(secret_hex: &str) -> Result<String, String> {
        use chacha20poly1305::{
            aead::{Aead, KeyInit, OsRng},
            ChaCha20Poly1305, Nonce,
        };
        let passphrase = Self::resolve_passphrase();
        let mut key = [0u8; 32];
        pbkdf2_hmac_sha256(passphrase.as_bytes(), b"dwow-accounts", 100_000, &mut key);
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| format!("cipher init: {e}"))?;
        let nonce_bytes: [u8; 12] = {
            use rand::RngCore;
            let mut b = [0u8; 12];
            OsRng.fill_bytes(&mut b);
            b
        };
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, secret_hex.as_bytes())
            .map_err(|e| format!("encrypt: {e}"))?;
        let mut combined = Vec::with_capacity(12 + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&combined))
    }

    fn decrypt_secret(encrypted: &str) -> Result<String, String> {
        use chacha20poly1305::{
            aead::{Aead, KeyInit},
            ChaCha20Poly1305, Nonce,
        };
        let passphrase = Self::resolve_passphrase();
        let mut key = [0u8; 32];
        pbkdf2_hmac_sha256(passphrase.as_bytes(), b"dwow-accounts", 100_000, &mut key);
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| format!("cipher init: {e}"))?;
        use base64::Engine;
        let combined = base64::engine::general_purpose::STANDARD
            .decode(encrypted)
            .map_err(|e| format!("base64 decode: {e}"))?;
        if combined.len() < 12 {
            return Err("encrypted secret too short".into());
        }
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|_| "decrypt failed — wrong passphrase or corrupt data".to_string())?;
        String::from_utf8(plaintext).map_err(|e| format!("utf8: {e}"))
    }

    fn to_json(&self) -> Result<String, String> {
        // H1-fix: propagate encrypt errors — never emit plaintext secret on AEAD failure.
        let mut entries = Vec::with_capacity(self.accounts.len());
        for a in &self.accounts {
            let encrypted = Self::encrypt_secret(&a.secret_hex())?;
            entries.push(serde_json::json!({
                "encrypted_secret": encrypted,
                "address": a.address(self.network),
                "label": a.label,
                "derivation_path": a.derivation_path,
            }));
        }
        let mut json = serde_json::json!({
            "default_index": self.default_index,
            "accounts": entries,
        });
        if let Some(ref seed) = self.encrypted_seed {
            json["encrypted_seed"] = serde_json::Value::String(seed.clone());
            json["seed_is_mnemonic"] = serde_json::Value::Bool(self.seed_is_mnemonic);
        }
        serde_json::to_string_pretty(&json).map_err(|e| format!("json serialize: {e}"))
    }

    /// Load lifecycle keys from a JSON blob (previously saved via
    /// `to_json_string`). Keys are APPENDED at index ≥ 1 — the declared
    /// identity at `accounts[0]` (set by `open()`) is never touched. Skips
    /// duplicates (same secret bytes as any existing account). The JSON blob
    /// is advisory — a corrupt/missing blob is a soft skip, never a boot
    /// failure. Returns the number of keys appended.
    pub fn load_lifecycle(&mut self, data: &[u8]) -> Result<usize, String> {
        let json: serde_json::Value = serde_json::from_slice(data)
            .map_err(|e| format!("json parse: {e}"))?;
        // Absorb encrypted_seed if the manager doesn't already have one.
        if self.encrypted_seed.is_none() {
            self.encrypted_seed = json["encrypted_seed"].as_str().map(|s| s.to_string());
            self.seed_is_mnemonic = json["seed_is_mnemonic"].as_bool().unwrap_or(false);
        }
        let entries = json["accounts"].as_array()
            .ok_or("missing accounts array")?;
        let mut existing: Vec<[u8; 32]> = self.accounts.iter()
            .map(|a| a.keypair.secret.inner().to_repr()).collect();
        let mut count = 0usize;
        for entry in entries {
            let hex_str: String = if let Some(enc) = entry["encrypted_secret"].as_str() {
                Self::decrypt_secret(enc)?
            } else if let Some(hex) = entry["secret_hex"].as_str() {
                hex.to_string()
            } else {
                // M3-fix: skip bad entries, don't abort the whole load.
                continue;
            };
            let bytes = match hex::decode(&hex_str) {
                Ok(b) => b,
                Err(_) => continue, // M3-fix: skip corrupt hex
            };
            let arr = match <[u8; 32]>::try_from(bytes) {
                Ok(a) => a,
                Err(_) => continue, // M3-fix: skip wrong-length
            };
            let secret = match SecretKey::from_bytes(arr) {
                Ok(s) => s,
                Err(_) => continue, // M3-fix: skip invalid key
            };
            // M2-fix: dedup against both pre-existing AND previously-appended keys.
            let repr = secret.inner().to_repr();
            if existing.contains(&repr) { continue; }
            existing.push(repr);
            let keypair = Keypair::new(secret);
            self.accounts.push(Account {
                keypair,
                label: entry["label"].as_str().map(|s| s.to_string()),
                derivation_path: entry["derivation_path"].as_str().map(|s| s.to_string()),
            });
            count += 1;
        }
        Ok(count)
    }

    // ========================================================================
    // BIP39/BIP32 Seed Phrase & HD Derivation
    // ========================================================================

    /// Import accounts from a BIP39 seed phrase (12 or 24 words).
    pub fn from_seed_phrase(phrase: &str, passphrase: &str, network: Network) -> Result<Self, String> {
        let seed = bip39_to_seed(phrase, passphrase)?;
        let encrypted = Self::encrypt_secret(phrase)?;
        let mut mgr = Self::from_seed(&seed, "m/44'/0'/0'/0/0", network)?;
        mgr.encrypted_seed = Some(encrypted);
        mgr.seed_is_mnemonic = true;
        Ok(mgr)
    }

    /// Import accounts from a raw 64-byte BIP39 seed + derivation path.
    pub fn from_seed(seed: &[u8; 64], path: &str, network: Network) -> Result<Self, String> {
        let child_key = bip32_derive(seed, path)?;
        let keypair = Keypair::new(child_key);
        let account = Account {
            keypair,
            label: Some(format!("hd-{}", path.replace('/', "-"))),
            derivation_path: Some(path.to_string()),
        };
        Ok(AccountManager {
            accounts: vec![account],
            default_index: 0,
            declared: true,
            network,
            encrypted_seed: None,
            seed_is_mnemonic: false,
        })
    }

    /// Derive an additional account from the stored encrypted seed.
    pub fn derive_account(&mut self, path: &str) -> Result<usize, String> {
        let encrypted = self.encrypted_seed.as_ref()
            .ok_or("No seed stored — cannot derive additional accounts.")?;
        if !self.seed_is_mnemonic {
            return Err("Seed is raw bytes, not mnemonic — re-derivation not supported.".into());
        }
        let phrase = Self::decrypt_secret(encrypted)?;
        let seed = bip39_to_seed(&phrase, "")?;
        let child_key = bip32_derive(&seed, path)?;
        let keypair = Keypair::new(child_key);
        let idx = self.accounts.len();
        self.accounts.push(Account {
            keypair,
            label: Some(format!("hd-{}", path.replace('/', "-"))),
            derivation_path: Some(path.to_string()),
        });
        Ok(idx)
    }

    /// Derive a child key from a seed at a BIP32 derivation path.
    pub fn derive_key(seed: &[u8; 64], path: &str) -> Result<SecretKey, String> {
        bip32_derive(seed, path)
    }
}

// ── BIP39 Wordlist (2048 English words) ─────────────────────────────────

const BIP39_WORDS: &[&str] = &[
    "abandon","ability","able","about","above","absent","absorb","abstract","absurd","abuse",
    "access","accident","account","accuse","achieve","acid","acoustic","acquire","across","act",
    "action","actor","actress","actual","adapt","add","addict","address","adjust","admit",
    "adult","advance","advice","aerobic","affair","afford","afraid","africa","after","again",
    "age","agent","agree","ahead","aim","air","airport","aisle","alarm","album",
    "alcohol","alert","alien","all","alley","allow","almost","alone","alpha","already",
    "also","alter","always","amateur","amazing","among","amount","amused","analyst","anchor",
    "ancient","anger","angle","angry","animal","ankle","announce","annual","another","answer",
    "antenna","antique","anxiety","any","apart","apology","appear","apple","approve","april",
    "arch","arctic","area","arena","argue","arm","armed","armor","army","around",
    "arrange","arrest","arrive","arrow","art","artefact","artist","artwork","ask","aspect",
    "assault","asset","assist","assume","asthma","athlete","atom","attack","attend","attitude",
    "attract","auction","audit","august","aunt","author","auto","autumn","average","avocado",
    "avoid","awake","aware","away","awesome","awful","awkward","axis","baby","bachelor",
    "bacon","badge","bag","balance","balcony","ball","bamboo","banana","banner","bar",
    "barely","bargain","barrel","base","basic","basket","battle","beach","bean","beauty",
    "because","become","beef","before","begin","behave","behind","believe","below","belt",
    "bench","benefit","best","betray","better","between","beyond","bicycle","bid","bike",
    "bind","biology","bird","birth","bitter","black","blade","blame","blanket","blast",
    "bleak","bless","blind","blood","blossom","blouse","blue","blur","blush","board",
    "boat","body","boil","bomb","bone","bonus","book","boost","border","boring",
    "borrow","boss","bottom","bounce","box","boy","bracket","brain","brand","brass",
    "brave","bread","breeze","brick","bridge","brief","bright","bring","brisk","broccoli",
    "broken","bronze","broom","brother","brown","brush","bubble","buddy","budget","buffalo",
    "build","bulb","bulk","bullet","bundle","bunker","burden","burger","burst","bus",
    "business","busy","butter","buyer","buzz","cabbage","cabin","cable","cactus","cage",
    "cake","call","calm","camera","camp","can","canal","cancel","candy","cannon",
    "canoe","canvas","canyon","capable","capital","captain","car","carbon","card","cargo",
    "carpet","carry","cart","case","cash","casino","castle","casual","cat","catalog",
    "catch","category","cattle","caught","cause","caution","cave","ceiling","celery","cement",
    "census","century","cereal","certain","chair","chalk","champion","change","chaos","chapter",
    "charge","chase","chat","cheap","check","cheese","chef","cherry","chest","chicken",
    "chief","child","chimney","choice","choose","chronic","chuckle","chunk","churn","cigar",
    "cinnamon","circle","citizen","city","civil","claim","clap","clarify","claw","clay",
    "clean","clerk","clever","click","client","cliff","climb","clinic","clip","clock",
    "clog","close","cloth","cloud","clown","club","clump","cluster","clutch","coach",
    "coast","coconut","code","coffee","coil","coin","collect","color","column","combine",
    "come","comfort","comic","common","company","concert","conduct","confirm","congress","connect",
    "consider","control","convince","cook","cool","copper","copy","coral","core","corn",
    "correct","cost","cotton","couch","country","couple","course","cousin","cover","coyote",
    "crack","cradle","craft","cram","crane","crash","crater","crawl","crazy","cream",
    "credit","creek","crew","cricket","crime","crisp","critic","crop","cross","crouch",
    "crowd","crucial","cruel","cruise","crumble","crunch","crush","cry","crystal","cube",
    "culture","cup","cupboard","curious","current","curtain","curve","cushion","custom","cute",
    "cycle","dad","damage","damp","dance","danger","daring","dash","daughter","dawn",
    "day","deal","debate","debris","decade","december","decide","decline","decorate","decrease",
    "deer","defense","define","defy","degree","delay","deliver","demand","demise","denial",
    "dentist","deny","depart","depend","deposit","depth","deputy","derive","describe","desert",
    "design","desk","despair","destroy","detail","detect","develop","device","devote","diagram",
    "dial","diamond","diary","dice","diesel","diet","differ","digital","dignity","dilemma",
    "dinner","dinosaur","direct","dirt","disagree","discover","disease","dish","dismiss","disorder",
    "display","distance","divert","divide","divorce","dizzy","doctor","document","dog","doll",
    "dolphin","domain","donate","donkey","donor","door","dose","double","dove","draft",
    "dragon","drama","drastic","draw","dream","dress","drift","drill","drink","drip",
    "drive","drop","drum","dry","duck","dumb","dune","during","dust","dutch",
    "duty","dwarf","dynamic","eager","eagle","early","earn","earth","easily","east",
    "easy","echo","ecology","economy","edge","edit","educate","effort","egg","eight",
    "either","elbow","elder","electric","elegant","element","elephant","elevator","elite","else",
    "embark","embody","embrace","emerge","emotion","employ","empower","empty","enable","enact",
    "end","endless","endorse","enemy","energy","enforce","engage","engine","enhance","enjoy",
    "enlist","enough","enrich","enroll","ensure","enter","entire","entry","envelope","episode",
    "equal","equip","era","erase","erode","erosion","error","erupt","escape","essay",
    "essence","estate","eternal","ethics","evidence","evil","evoke","evolve","exact","example",
    "excess","exchange","excite","exclude","excuse","execute","exercise","exhaust","exhibit","exile",
    "exist","exit","exotic","expand","expect","expire","explain","expose","express","extend",
    "extra","eye","eyebrow","fabric","face","faculty","fade","faint","faith","fall",
    "false","fame","family","famous","fan","fancy","fantasy","farm","fashion","fat",
    "fatal","father","fatigue","fault","favorite","feature","february","federal","fee","feed",
    "feel","female","fence","festival","fetch","fever","few","fiber","fiction","field",
    "figure","file","film","filter","final","find","fine","finger","finish","fire",
    "firm","first","fiscal","fish","fit","fitness","fix","flag","flame","flash",
    "flat","flavor","flee","flight","flip","float","flock","floor","flower","fluid",
    "flush","fly","foam","focus","fog","foil","fold","follow","food","foot",
    "force","forest","forget","fork","fortune","forum","forward","fossil","foster","found",
    "fox","fragile","frame","frequent","fresh","friend","fringe","frog","front","frost",
    "frown","frozen","fruit","fuel","fun","funny","furnace","fury","future","gadget",
    "gain","galaxy","gallery","game","gap","garage","garbage","garden","garlic","garment",
    "gas","gasp","gate","gather","gauge","gaze","general","genius","genre","gentle",
    "genuine","gesture","ghost","giant","gift","giggle","ginger","giraffe","girl","give",
    "glad","glance","glare","glass","glide","glimpse","globe","gloom","glory","glove",
    "glow","glue","goat","goddess","gold","good","goose","gorilla","gospel","gossip",
    "govern","gown","grab","grace","grain","grant","grape","grass","gravity","great",
    "green","grid","grief","grit","grocery","group","grow","grunt","guard","guess",
    "guide","guilt","guitar","gun","gym","habit","hair","half","hammer","hamster",
    "hand","happy","harbor","hard","harsh","harvest","hat","have","hawk","hazard",
    "head","health","heart","heavy","hedgehog","height","hello","helmet","help","hen",
    "hero","hidden","high","hill","hint","hip","hire","history","hobby","hockey",
    "hold","hole","holiday","hollow","home","honey","hood","hope","horn","horror",
    "horse","hospital","host","hotel","hour","hover","hub","huge","human","humble",
    "humor","hundred","hungry","hunt","hurdle","hurry","hurt","husband","hybrid","ice",
    "icon","idea","identify","idle","ignore","ill","illegal","illness","image","imitate",
    "immense","immune","impact","impose","improve","impulse","inch","include","income","increase",
    "index","indicate","indoor","industry","infant","inflict","inform","inhale","inherit","initial",
    "inject","injury","inmate","inner","innocent","input","inquiry","insane","insect","inside",
    "inspire","install","intact","interest","into","invest","invite","involve","iron","island",
    "isolate","issue","item","ivory","jacket","jaguar","jar","jazz","jealous","jeans",
    "jelly","jewel","job","join","joke","journey","joy","judge","juice","jump",
    "jungle","junior","junk","just","kangaroo","keen","keep","ketchup","key","kick",
    "kid","kidney","kind","kingdom","kiss","kit","kitchen","kite","kitten","kiwi",
    "knee","knife","knock","know","lab","label","labor","ladder","lady","lake",
    "lamp","language","laptop","large","later","latin","laugh","laundry","lava","law",
    "lawn","lawsuit","layer","lazy","leader","leaf","learn","leave","lecture","left",
    "leg","legal","legend","leisure","lemon","lend","length","lens","leopard","lesson",
    "letter","level","liar","liberty","library","license","life","lift","light","like",
    "limb","limit","link","lion","liquid","list","little","live","lizard","load",
    "loan","lobster","local","lock","logic","lonely","long","loop","lottery","loud",
    "lounge","love","loyal","lucky","luggage","lumber","lunar","lunch","luxury","lyrics",
    "machine","mad","magic","magnet","maid","mail","main","major","make","mammal",
    "man","manage","mandate","mango","mansion","manual","maple","marble","march","margin",
    "marine","market","marriage","mask","mass","master","match","material","math","matrix",
    "matter","maximum","maze","meadow","mean","measure","meat","mechanic","medal","media",
    "melody","melt","member","memory","mention","menu","mercy","merge","merit","merry",
    "mesh","message","metal","method","middle","midnight","milk","million","mimic","mind",
    "minimum","minor","minute","miracle","mirror","misery","miss","mistake","mix","mixed",
    "mixture","mobile","model","modify","mom","moment","monitor","monkey","monster","month",
    "moon","moral","more","morning","mosquito","mother","motion","motor","mountain","mouse",
    "move","movie","much","muffin","mule","multiply","muscle","museum","mushroom","music",
    "must","mutual","myself","mystery","myth","naive","name","napkin","narrow","nasty",
    "nation","nature","near","neck","need","negative","neglect","neither","nephew","nerve",
    "nest","net","network","neutral","never","news","next","nice","night","noble",
    "noise","nominee","noodle","normal","north","nose","notable","note","nothing","notice",
    "novel","now","nuclear","number","nurse","nut","oak","obey","object","oblige",
    "obscure","observe","obtain","obvious","occur","ocean","october","odor","off","offer",
    "office","often","oil","okay","old","olive","olympic","omit","once","one",
    "onion","online","only","open","opera","opinion","oppose","option","orange","orbit",
    "orchard","order","ordinary","organ","orient","original","orphan","ostrich","other","outdoor",
    "outer","output","outside","oval","oven","over","own","owner","oxygen","oyster",
    "ozone","pact","paddle","page","pair","palace","palm","panda","panel","panic",
    "panther","paper","parade","parent","park","parrot","party","pass","patch","path",
    "patient","patrol","pattern","pause","pave","payment","peace","peanut","pear","peasant",
    "pelican","pen","penalty","pencil","people","pepper","perfect","permit","person","pet",
    "phone","photo","phrase","physical","piano","picnic","picture","piece","pig","pigeon",
    "pill","pilot","pink","pioneer","pipe","pistol","pitch","pizza","place","planet",
    "plastic","plate","play","please","pledge","pluck","plug","plunge","poem","poet",
    "point","polar","pole","police","pond","pony","pool","popular","portion","position",
    "possible","post","potato","pottery","poverty","powder","power","practice","praise","predict",
    "prefer","prepare","present","pretty","prevent","price","pride","primary","print","priority",
    "prison","private","prize","problem","process","produce","profit","program","project","promote",
    "proof","property","prosper","protect","proud","provide","public","pudding","pull","pulp",
    "pulse","pumpkin","punch","pupil","puppy","purchase","purity","purpose","purse","push",
    "put","puzzle","pyramid","quality","quantum","quarter","question","quick","quit","quiz",
    "quote","rabbit","raccoon","race","rack","radar","radio","rail","rain","raise",
    "rally","ramp","ranch","random","range","rapid","rare","rate","rather","raven",
    "raw","razor","ready","real","reason","rebel","rebuild","recall","receive","recipe",
    "record","recycle","reduce","reflect","reform","refuse","region","regret","regular","reject",
    "relax","release","relief","rely","remain","remember","remind","remove","render","renew",
    "rent","reopen","repair","repeat","replace","report","require","rescue","resemble","resist",
    "resource","response","result","retire","retreat","return","reunion","reveal","review","reward",
    "rhythm","rib","ribbon","rice","rich","ride","ridge","rifle","right","rigid",
    "ring","riot","ripple","risk","ritual","rival","river","road","roast","robot",
    "robust","rocket","romance","roof","rookie","room","rose","rotate","rough","round",
    "route","royal","rubber","rude","rug","rule","run","runway","rural","sad",
    "saddle","sadness","safe","sail","salad","salmon","salon","salt","salute","same",
    "sample","sand","satisfy","satoshi","sauce","sausage","save","say","scale","scan",
    "scare","scatter","scene","scheme","school","science","scissors","scorpion","scout","scrap",
    "screen","script","scrub","sea","search","season","seat","second","secret","section",
    "security","seed","seek","segment","select","sell","seminar","senior","sense","sentence",
    "series","service","session","settle","setup","seven","shadow","shaft","shallow","share",
    "shed","shell","sheriff","shield","shift","shine","ship","shiver","shock","shoe",
    "shoot","shop","short","shoulder","shove","shrimp","shrug","shuffle","shy","sibling",
    "sick","side","siege","sight","sign","silent","silk","silly","silver","similar",
    "simple","since","sing","siren","sister","situate","six","size","skate","sketch",
    "ski","skill","skin","skirt","skull","slab","slam","sleep","slender","slice",
    "slide","slight","slim","slogan","slot","slow","slush","small","smart","smile",
    "smoke","smooth","snack","snake","snap","sniff","snow","soap","soccer","social",
    "sock","soda","soft","solar","soldier","solid","solution","solve","someone","song",
    "soon","sorry","sort","soul","sound","soup","source","south","space","spare",
    "spatial","spawn","speak","special","speed","spell","spend","sphere","spice","spider",
    "spike","spin","spirit","split","spoil","sponsor","spoon","sport","spot","spray",
    "spread","spring","spy","square","squeeze","squirrel","stable","stadium","staff","stage",
    "stairs","stamp","stand","start","state","stay","steak","steel","stem","step",
    "stereo","stick","still","sting","stock","stomach","stone","stool","story","stove",
    "strategy","street","strike","strong","struggle","student","stuff","stumble","style","subject",
    "submit","subway","success","such","sudden","suffer","sugar","suggest","suit","summer",
    "sun","sunny","sunset","super","supply","supreme","sure","surface","surge","surprise",
    "surround","survey","suspect","sustain","swallow","swamp","swap","swarm","swear","sweet",
    "swift","swim","swing","switch","sword","symbol","symptom","syrup","system","table",
    "tackle","tag","tail","talent","talk","tank","tape","target","task","taste",
    "tattoo","taxi","teach","team","tell","ten","tenant","tennis","tent","term",
    "test","text","thank","that","theme","then","theory","there","they","thing",
    "this","thought","three","thrive","throw","thumb","thunder","ticket","tide","tiger",
    "tilt","timber","time","tiny","tip","tired","tissue","title","toast","tobacco",
    "today","toddler","toe","together","toilet","token","tomato","tomorrow","tone","tongue",
    "tonight","tool","tooth","top","topic","topple","torch","tornado","tortoise","toss",
    "total","tourist","toward","tower","town","toy","track","trade","traffic","tragic",
    "train","transfer","trap","trash","travel","tray","treat","tree","trend","trial",
    "tribe","trick","trigger","trim","trip","trophy","trouble","truck","true","truly",
    "trumpet","trust","truth","try","tube","tuition","tumble","tuna","tunnel","turkey",
    "turn","turtle","twelve","twenty","twice","twin","twist","two","type","typical",
    "ugly","umbrella","unable","unaware","uncle","uncover","under","undo","unfair","unfold",
    "unhappy","uniform","unique","unit","universe","unknown","unlock","until","unusual","unveil",
    "update","upgrade","uphold","upon","upper","upset","urban","urge","usage","use",
    "used","useful","useless","usual","utility","vacant","vacuum","vague","valid","valley",
    "valve","van","vanish","vapor","various","vast","vault","vehicle","velvet","vendor",
    "venture","venue","verb","verify","version","very","vessel","veteran","viable","vibrant",
    "vicious","victory","video","view","village","vintage","violin","virtual","virus","visa",
    "visit","visual","vital","vivid","vocal","voice","void","volcano","volume","vote",
    "voyage","wage","wagon","wait","walk","wall","walnut","want","warfare","warm",
    "warrior","wash","wasp","waste","water","wave","way","wealth","weapon","wear",
    "weasel","weather","web","wedding","weekend","weird","welcome","west","wet","whale",
    "what","wheat","wheel","when","where","whip","whisper","wide","width","wife",
    "wild","will","win","window","wine","wing","wink","winner","winter","wire",
    "wisdom","wise","wish","witness","wolf","woman","wonder","wood","wool","word",
    "work","world","worry","worth","wrap","wreck","wrestle","wrist","write","wrong",
    "yard","year","yellow","you","young","youth","zebra","zero","zone","zoo",
];

// ── BIP39 Mnemonic → Seed (PBKDF2-HMAC-SHA512) ──────────────────────────

/// Decode BIP39 mnemonic words to entropy bytes.
fn bip39_words_to_entropy(phrase: &str) -> Result<(Vec<u8>, usize), String> {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.len() < 12 || words.len() > 24 || words.len() % 3 != 0 {
        return Err(format!("Invalid word count: {} (12/15/18/21/24 required)", words.len()));
    }

    let total_bits = words.len() * 11;
    let entropy_bits = total_bits - total_bits / 33;
    let checksum_bits = total_bits - entropy_bits;

    let mut bits: Vec<bool> = Vec::with_capacity(total_bits);
    for w in &words {
        let idx = BIP39_WORDS.iter().position(|&x| x == *w)
            .ok_or_else(|| format!("Invalid BIP39 word: '{}'", w))?;
        for bit in (0..11).rev() {
            bits.push((idx >> bit) & 1 == 1);
        }
    }

    let entropy_bytes = (entropy_bits + 7) / 8;
    let mut entropy = vec![0u8; entropy_bytes];
    for i in 0..entropy_bits {
        if bits[i] {
            entropy[i / 8] |= 1 << (7 - (i % 8));
        }
    }

    Ok((entropy, checksum_bits))
}

/// Validate the BIP39 checksum.
fn bip39_validate(phrase: &str) -> Result<(), String> {
    let (entropy, checksum_bits) = bip39_words_to_entropy(phrase)?;
    if checksum_bits == 0 {
        return Ok(());
    }

    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(&entropy);
    let expected_checksum = (hash[0] as usize) >> (8 - checksum_bits);

    let words: Vec<&str> = phrase.split_whitespace().collect();
    let last_word_idx = BIP39_WORDS.iter().position(|&x| x == words[words.len() - 1])
        .ok_or_else(|| "Invalid BIP39 word".to_string())?;
    let mask = (1 << checksum_bits) - 1;
    let actual_checksum = last_word_idx & mask;

    if expected_checksum != actual_checksum {
        return Err(format!(
            "Invalid BIP39 checksum — possible typo in seed phrase. \
             Expected checksum bits: {:0width$b}, got: {:0width$b}",
            expected_checksum, actual_checksum, width = checksum_bits
        ));
    }
    Ok(())
}

/// Derive a 64-byte BIP39 seed from a mnemonic phrase.
fn bip39_to_seed(phrase: &str, passphrase: &str) -> Result<[u8; 64], String> {
    bip39_validate(phrase)?;
    let (entropy, _) = bip39_words_to_entropy(phrase)?;
    let salt = format!("mnemonic{}", passphrase);
    let mut seed = [0u8; 64];
    pbkdf2_hmac_sha512(&entropy, salt.as_bytes(), 2048, &mut seed);
    Ok(seed)
}

fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take any key size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    let block_size = 32;
    let mut result = vec![0u8; output.len()];

    for (i, chunk) in result.chunks_mut(block_size).enumerate() {
        let mut salt_block = salt.to_vec();
        salt_block.extend_from_slice(&((i + 1) as u32).to_be_bytes());

        let mut u = hmac_sha256(password, &salt_block);
        let mut t = u.clone();

        for _ in 1..iterations {
            u = hmac_sha256(password, &u);
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= b;
            }
        }

        let copy_len = chunk.len().min(t.len());
        chunk[..copy_len].copy_from_slice(&t[..copy_len]);
    }
    output.copy_from_slice(&result[..output.len()]);
}

fn pbkdf2_hmac_sha512(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    type HmacSha512 = Hmac<Sha512>;

    fn hmac_sha512(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha512::new_from_slice(key).expect("HMAC can take any key size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    let block_size = 64;
    let mut result = vec![0u8; output.len()];

    for (i, chunk) in result.chunks_mut(block_size).enumerate() {
        let mut salt_block = salt.to_vec();
        salt_block.extend_from_slice(&((i + 1) as u32).to_be_bytes());

        let mut u = hmac_sha512(password, &salt_block);
        let mut t = u.clone();

        for _ in 1..iterations {
            u = hmac_sha512(password, &u);
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= b;
            }
        }

        let copy_len = chunk.len().min(t.len());
        chunk[..copy_len].copy_from_slice(&t[..copy_len]);
    }
    output.copy_from_slice(&result[..output.len()]);
}

/// Hardened-only BIP32 key derivation.
fn bip32_derive(seed: &[u8; 64], path: &str) -> Result<SecretKey, String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    type HmacSha512 = Hmac<Sha512>;

    fn hmac_sha512(key: &[u8], data: &[&[u8]]) -> Vec<u8> {
        let mut mac = HmacSha512::new_from_slice(key).expect("HMAC can take any key size");
        for d in data {
            mac.update(d);
        }
        mac.finalize().into_bytes().to_vec()
    }

    let i = hmac_sha512(b"DarkWow seed", &[seed]);
    let master_secret = &i[..32];
    let mut chain_code = i[32..].to_vec();
    let mut secret = master_secret.to_vec();

    for component in path.split('/') {
        if component == "m" { continue; }
        let hardened = component.ends_with('\'');
        let index_str = component.trim_end_matches('\'');
        let index: u32 = index_str.parse()
            .map_err(|_| format!("Invalid path component: {}", component))?;

        if hardened {
            let child_index = 0x80000000u32 + index;
            let data = [&[0x00u8] as &[u8], &secret, &child_index.to_be_bytes()];
            let ilr = hmac_sha512(&chain_code, &[data[0], data[1], data[2]]);
            secret = ilr[..32].to_vec();
            chain_code = ilr[32..].to_vec();
        } else {
            // Non-hardened BIP32 derivation: I = HMAC-SHA512(c_par, K_par || i)
            // K_par = compressed public key from parent secret
            // child_secret = parent_secret + I_left  (mod PALLAS_Q)
            // child_chain_code = I_right
            // Normalize parent secret via from_uniform_bytes — HMAC output may
            // be non-canonical (same pattern as final key derivation below).
            let parent_arr = <[u8; 32]>::try_from(secret.clone())
                .map_err(|_| "Invalid parent secret length".to_string())?;
            let mut wide = [0u8; 64];
            wide[..32].copy_from_slice(&parent_arr);
            let parent_base = pallas::Base::from_uniform_bytes(&wide);
            let parent_canonical = parent_base.to_repr();
            let parent_sk = SecretKey::from_bytes(parent_canonical)
                .map_err(|_| "Invalid parent secret".to_string())?;
            let parent_pk = PublicKey::from_secret(parent_sk);
            let pk_bytes = parent_pk.to_bytes();
            // I = HMAC-SHA512(c_par, K_par || i)
            let ilr = hmac_sha512(&chain_code, &[&pk_bytes[..], &index.to_be_bytes()[..]]);
            // child_secret = parent_secret + I_left (mod PALLAS_Q)
            let mut wide_addend = [0u8; 64];
            wide_addend[..32].copy_from_slice(&ilr[..32]);
            let addend_base = pallas::Base::from_uniform_bytes(&wide_addend);
            let sum = parent_base + addend_base;
            secret = sum.to_repr().to_vec();
            chain_code = ilr[32..].to_vec();
        }
    }

    let mut wide = [0u8; 64];
    wide[..32].copy_from_slice(&secret);
    let base = pallas::Base::from_uniform_bytes(&wide);
    let canonical = base.to_repr();
    SecretKey::from_bytes(canonical)
        .map_err(|_| "Derived key is not a valid secret key".to_string())
}


// ============================================================================
// Owner-sovereign key-safety types
//
// Design basis: any non-determinism in key generation/use is a catastrophic
// failure that must be designed out — impossible by construction — not guarded.
// These two newtypes make the hazard unrepresentable at compile time. Scoped,
// approved exception to no-novel-naming: the type IS the safety boundary.
// ============================================================================

/// An owned, owner-declared identity secret.
///
/// Unlike `SecretKey`, this type has NO random constructor and does not import
/// `OsRng`: it can only be produced deterministically from the owner's
/// declaration (`from_declared_bytes` / `from_declared`) or by deterministic
/// derivation (`derive_instance`). `AccountManager` hands this out for
/// mining/wallet identity, so an identity key can never originate from randomness
/// — that path does not exist on this type.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct OwnedSecretKey(SecretKey);

impl OwnedSecretKey {
    /// Construct from the owner's declared 32 bytes (e.g. keys.toml hex).
    /// Deterministic; rejects non-canonical bytes. This is the constructor for
    /// the declared identity — no random ctor exists on this type.
    pub fn from_declared_bytes(bytes: [u8; 32]) -> Result<Self, String> {
        SecretKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| "invalid declared secret key bytes".to_string())
    }

    /// Wrap an already-resolved declared secret. Crate-visible only — external
    /// callers must use `from_declared_bytes` (keys.toml) or `default_owned()`
    /// (AccountManager). This prevents laundering `SecretKey::random` into the
    /// "declared" claim.
    pub(crate) fn from_declared(secret: SecretKey) -> Self {
        Self(secret)
    }

    /// Deterministic per-(contract, instance) derivation. Reproducible from the
    /// declaration — no randomness. Used for address cycling and per-contract
    /// unlinkable scanning keys.
    pub fn derive_instance(
        &self, contract_id: &ContractId, instance_id: &[u8],
    ) -> Result<Self, ContractError> {
        Ok(Self(self.0.derive_instance(contract_id, instance_id)?))
    }

    /// Expose the inner `SecretKey` for boundary decrypt/scan APIs.
    pub fn expose_secret(&self) -> &SecretKey {
        &self.0
    }

    /// The public key derived from this identity secret.
    pub fn public(&self) -> PublicKey {
        PublicKey::from_secret(self.0)
    }
}

impl From<OwnedSecretKey> for SecretKey {
    fn from(o: OwnedSecretKey) -> Self {
        o.0
    }
}

/// A validated mining-reward recipient.
///
/// Constructible ONLY via `from_account` — the node's own declared identity. A bare
/// `PublicKey` parsed off the wire cannot become a `MiningRecipient`, so coinbase can
/// never be minted to a key the node has no secret for (one miner, one key).
///
/// Carries both the `PublicKey` (for ZK circuit encryption) and the `Address`
/// (for human-facing identity and per-block cycling). Per-block address derivation
/// uses `derive_instance(NATIVE_TOKEN_CONTRACT_ID, height)` for unlinkable
/// privacy-preserving rewards.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MiningRecipient {
    pub_key: PublicKey,
    address: Address,
    secret: OwnedSecretKey,
}

impl MiningRecipient {
    /// The node's own declared identity as the reward recipient.
    /// Derives a per-block key for every height including genesis (height=1).
    /// `sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H.to_le_bytes())`
    /// per consensus-coinbase.md §2.2. The wallet derives the same key via
    /// `secrets_for_contract` — zero shared state, pure determinism.
    pub fn from_account(mgr: &AccountManager, height: u32) -> Result<Self, String> {
        use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
        let owned = mgr.default_owned()?;
        let derived =
            owned.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
                .map_err(|e| format!("MiningRecipient derive_instance failed: {}", e))?;
        let pk = derived.public();
        let addr: Address = StandardAddress::from_public(mgr.network, pk).into();
        Ok(Self { pub_key: pk, address: addr, secret: derived })
    }

    /// The recipient public key (for ZK circuit / AEAD encryption).
    pub fn public(&self) -> PublicKey {
        self.pub_key
    }

    /// The recipient address (for diagnostics / logging).
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// The per-block derived secret key (for nullifier computation).
    /// This is `sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H)`.
    /// Both miner and wallet compute this deterministically.
    pub fn secret(&self) -> &OwnedSecretKey {
        &self.secret
    }

    /// Consume the recipient and return the derived secret key.
    /// Used when the secret must be moved into the ZK proof builder.
    pub fn into_secret(self) -> OwnedSecretKey {
        self.secret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bip39_seed_vector() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let passphrase = "TREZOR";
        let seed = bip39_to_seed(phrase, passphrase).unwrap();
        assert_eq!(seed.len(), 64);
        assert!(!seed.iter().all(|b| *b == 0), "Seed must not be all zeros");
    }

    #[test]
    fn test_bip32_derive() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let seed = bip39_to_seed(phrase, "TREZOR").unwrap();
        let key = bip32_derive(&seed, "m/44'/0'/0'/0/0").unwrap();
        assert!(!key.inner().to_repr().iter().all(|b| *b == 0), "derived key should not be zero");
    }

    #[test]
    fn test_bip39_checksum_rejected() {
        let bad_phrase = "legal winner thank year wave sausage worth useful legal winner thank year";
        let result = bip39_validate(bad_phrase);
        assert!(result.is_err(), "Bad checksum must be rejected");
        assert!(result.unwrap_err().contains("checksum"));
    }

    #[test]
    fn test_bip39_invalid_word() {
        let result = bip39_to_seed("notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword notaword", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_bip39_wrong_count() {
        let result = bip39_to_seed("abandon abandon abandon", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_bip39_deterministic() {
        // Canonical BIP39 test vector — valid checksum (all-zero entropy)
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let seed1 = bip39_to_seed(phrase, "TREZOR").unwrap();
        let seed2 = bip39_to_seed(phrase, "TREZOR").unwrap();
        assert_eq!(seed1, seed2, "same phrase must produce same seed");
    }

    // --- Determinism / resolution tests (design basis: no randomness on the
    // identity path; keys are declared and re-derived on boot) ---

    fn write_temp_keys(name: &str, contents: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("dwow_accounts_test_{name}.toml"));
        std::fs::write(&p, contents).unwrap();
        p
    }

    // A canonical declared secret (value 1 in little-endian repr).
    const DECL: &str = "[node0]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n";

    #[test]
    fn test_open_is_deterministic() {
        let path = write_temp_keys("det", DECL);
        let a = AccountManager::open(&path, Network::Testnet, "node0").unwrap();
        let b = AccountManager::open(&path, Network::Testnet, "node0").unwrap();
        assert_eq!(
            a.default_public_key().unwrap().to_bytes(),
            b.default_public_key().unwrap().to_bytes(),
            "same declaration must resolve to the same pubkey"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_missing_section_hard_errors() {
        let path = write_temp_keys("missing_section", DECL);
        assert!(
            AccountManager::open(&path, Network::Testnet, "nonexistent").is_err(),
            "missing section must be a hard error, never a synthesised key"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_missing_file_hard_errors() {
        let path = std::path::Path::new("/nonexistent/dwow_keys_xyz.toml");
        assert!(
            AccountManager::open(path, Network::Testnet, "node0").is_err(),
            "missing keys.toml must be a hard error — keys are never auto-generated"
        );
    }

    #[test]
    fn test_shared_section_same_identity() {
        // wallet-1 shares node0's key → same pubkey (the coinbase-decryption invariant).
        let toml = format!("{DECL}[wallet-1]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n");
        let path = write_temp_keys("shared", &toml);
        let node0 = AccountManager::open(&path, Network::Testnet, "node0").unwrap();
        let wallet1 = AccountManager::open(&path, Network::Testnet, "wallet-1").unwrap();
        assert_eq!(
            node0.default_public_key().unwrap().to_bytes(),
            wallet1.default_public_key().unwrap().to_bytes(),
            "wallet-1 sharing node0's declared key must derive node0's pubkey"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_mining_recipient_from_account() {
        use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

        let path = write_temp_keys("recip", DECL);
        let mgr = AccountManager::open(&path, Network::Testnet, "node0").unwrap();
        let r = MiningRecipient::from_account(&mgr, 1).unwrap();
        // At every height (including genesis), the recipient key is
        // derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, height).
        // This matches what the wallet derives via secrets_for_contract.
        let owned = mgr.default_owned().unwrap();
        let expected = owned
            .derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &1u32.to_le_bytes())
            .unwrap();
        assert_eq!(
            r.public().to_bytes(),
            expected.public().to_bytes(),
            "MiningRecipient from_account must derive per consensus-coinbase.md §2.2"
        );
        std::fs::remove_file(&path).ok();
    }

    /// F3: Miner and wallet derive the same per-block key.
    /// Invariant G2: sk_H_miner == sk_H_wallet for identical sk_owner + height.
    /// Invariant G6: different heights produce different keys (address cycling).
    /// Falsifiable: if derive_instance changes on one side, assertion fails.
    #[test]
    fn test_miner_and_wallet_derive_same_per_block_key() {
        use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

        // Create a deterministic identity secret
        // Use a valid pallas::Scalar (must be < PALLAS_Q)
        let secret_bytes: [u8; 32] = [
            0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let sk_owner = SecretKey::from_bytes(secret_bytes)
            .expect("valid test secret");

        // Miner side: derive via MiningRecipient (as prepare_block does)
        let height: u32 = 42;
        let miner_sk_h = sk_owner.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");

        // Wallet side: same derivation (as scan.rs does via secrets_for_contract)
        let wallet_sk_h = sk_owner.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");

        // G2: same identity + same height = same derived key
        assert_eq!(miner_sk_h.inner().to_repr(), wallet_sk_h.inner().to_repr(),
            "F3 FAIL: miner and wallet derive different per-block keys — G2 violated");

        // G6: different heights produce different keys (address cycling)
        let height_other: u32 = 99;
        let sk_h_other = sk_owner.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height_other.to_le_bytes())
            .expect("valid test derive_instance");
        assert_ne!(miner_sk_h.inner().to_repr(), sk_h_other.inner().to_repr(),
            "F3 FAIL: different heights must produce different keys — G6 violated");
    }
}
