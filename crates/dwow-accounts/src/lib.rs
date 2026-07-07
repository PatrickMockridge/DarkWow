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

//! Account Manager — unified key management for mining nodes and wallets.
//!
//! Single source of truth for all key material. Used by both `dwowd` (mining)
//! and `dwow_wallet` (wallet daemon) — both binaries resolve their identity
//! through the same `AccountManager::open()` entry point.
//!
//! Keys are DECLARED in `keys.toml` and re-derived deterministically on every
//! boot. There is no cache, no auto-generation, and no random/`Default` identity:
//! `open()` reads exactly one declared secret and is the sole constructor. A
//! missing file or section is a hard error — a key is never synthesised.
//!
//! The `section` parameter selects which `[section]` of `keys.toml` is the
//! identity (e.g. `node0`, `observer`, `wallet-1`). It is required — there is no
//! default. Callers resolve it explicitly (dwowd from `NODE_NAME`).

use std::path::Path;

use dwow_sdk::crypto::keypair::{Keypair, Network, PublicKey, SecretKey};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

/// A single account — one declared keypair.
#[derive(Debug, Clone)]
pub struct Account {
    pub keypair: Keypair,
}

impl Account {
    pub fn address(&self, network: Network) -> String {
        use dwow_sdk::crypto::keypair::{Address, StandardAddress};
        let addr: Address = StandardAddress::from_public(network, self.keypair.public).into();
        addr.to_string()
    }
}

/// Holds the node/wallet's single declared identity, derived on boot from the
/// owner's declaration (keys.toml section). Nothing is persisted or cached.
pub struct AccountManager {
    accounts: Vec<Account>,
    default_index: usize,
    /// Network for address generation (testnet=0xaf, mainnet=0x39)
    pub network: Network,
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
            accounts: vec![Account { keypair }],
            default_index: 0,
            network,
        })
    }

    // Mutating / alt-constructor methods REMOVED (all callerless after the
    // key-management refactor): `generate` (random identity), `import_hex`,
    // `import_base58`, `export_hex`, `remove`, `set_default`. Identity is
    // owner-declared and deterministic: `open()` (via `OwnedSecretKey::from_declared_bytes`)
    // is the SOLE constructor; there is no path that mints, imports, or mutates an
    // identity at runtime. `export_base58` (below) is the one live export, used by
    // `dwowd --export-secret`.

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
    // encrypt/decrypt_secret, resolve_passphrase, DEVNET_PASSPHRASE) and BIP39/BIP32
    // seed derivation (from_seed_phrase/from_seed/derive_account/derive_key) were all
    // REMOVED — callerless after the refactor. The wallet/miner derive their identity
    // on boot from the declaration; nothing is persisted, cached, or seed-derived.
}

// BIP39_WORDS wordlist REMOVED — callerless after the BIP39/seed-derivation sweep.

// ── BIP39 Mnemonic → Seed (PBKDF2-HMAC-SHA512) ──────────────────────────

// BIP39/BIP32 seed derivation + PBKDF2 helpers REMOVED (callerless dead code —
// the wallet/miner use owner-declared keys only, never seed-derived).


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
    /// Deterministic; rejects non-canonical bytes. This is the SOLE identity
    /// constructor — no random ctor exists, and `open()` is its only caller.
    pub fn from_declared_bytes(bytes: [u8; 32]) -> Result<Self, String> {
        SecretKey::from_bytes(bytes)
            .map(Self)
            .map_err(|_| "invalid declared secret key bytes".to_string())
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
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MiningRecipient(PublicKey);

impl MiningRecipient {
    /// The node's own declared identity as the reward recipient.
    pub fn from_account(mgr: &AccountManager) -> Result<Self, String> {
        Ok(Self(mgr.default_public_key()?))
    }

    /// The recipient public key.
    pub fn public(&self) -> PublicKey {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let path = write_temp_keys("recip", DECL);
        let mgr = AccountManager::open(&path, Network::Testnet, "node0").unwrap();
        let r = MiningRecipient::from_account(&mgr).unwrap();
        assert_eq!(
            r.public().to_bytes(),
            mgr.default_public_key().unwrap().to_bytes(),
            "MiningRecipient::from_account must be the node's own declared key"
        );
        std::fs::remove_file(&path).ok();
    }
}
