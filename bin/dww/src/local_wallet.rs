// Lightweight wallet handle — SQLite only, no sled, no P2P.
//
// Used by CLI commands that only need local wallet state (keys, addresses,
// capabilities). The daemon owns sled exclusively; this skips sled entirely
// so CLI processes don't hit WouldBlock lock contention.
//
// SQLite-only wallet handle — used by CLI commands that don't need sled/P2P.
// Complements the full Dww (daemon path). The identity is derived on boot
// from keys.toml [section] — no key store.

use std::sync::Arc;
use dwow_sdk::crypto::keypair::{Network, PublicKey};
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use crate::wallet_error::{Error, Result};
use crate::walletdb::WalletDb;

pub struct LocalWallet {
    pub wallet: Arc<WalletDb>,
    /// The wallet's declared identity, derived on boot (same as the daemon's Dww).
    /// Single source of address/secret display — no addresses table.
    pub account_mgr: dwow_accounts::AccountManager,
}

impl LocalWallet {
    pub fn open(
        wallet_path: &str,
        wallet_pass: &str,
        keys_toml: Option<&std::path::Path>,
        network: Network,
        section: &str,
    ) -> Result<Self> {
        let path = crate::wallet_util::expand_path(wallet_path)
            .map_err(|e| Error::Custom(format!("expand wallet path: {}", e)))?;
        let wallet = WalletDb::new(Some(path), Some(wallet_pass), false)
            .map_err(|_| Error::Custom("Failed to open wallet database".into()))?;

        // Verify schema exists — probe a surviving table (held_capabilities).
        if wallet.get_held_capabilities(Some(false)).is_err() {
            return Err(Error::Custom(
                "Wallet not initialized. Run 'wallet initialize' first.".into()
            ));
        }

        // Derive the declared identity (same path as the daemon). keys.toml is
        // required — the wallet must declare its key; nothing is stored.
        let keys_path = keys_toml.ok_or_else(|| Error::Custom(
            "no keys.toml provided (--keys or KEYS_FILE env): the wallet must declare its key".into()))?;
        let mut account_mgr = dwow_accounts::AccountManager::open(keys_path, network, section)
            .map_err(|e| Error::Custom(format!("AccountManager::open: {e}")))?;

        // F5-fix: hydrate lifecycle keys so the CLIs `secrets`/`addresses` match
        // the daemon's view. Soft-fail on corrupt/missing blob.
        if let Some(blob) = wallet.load_key_lifecycle() {
            let _ = account_mgr.load_lifecycle(blob.as_bytes());
        }

        Ok(Self { wallet, account_mgr })
    }

    pub fn default_address(&self) -> Result<String> {
        // Raw bs58 of the declared identity's public key (matches the prior
        // addresses.public_key format the transfer path consumes).
        let public = self.account_mgr.default_public_key()
            .map_err(|e| Error::Custom(format!("AccountManager: {e}")))?;
        Ok(bs58::encode(public.to_bytes()).into_string())
    }

    pub fn addresses(&self) -> Result<Vec<String>> {
        Ok(self.account_mgr.secrets().into_iter()
            .map(|s| bs58::encode(PublicKey::from_secret(s).to_bytes()).into_string())
            .collect())
    }

    /// Get secrets (bs58) derived from the declared identity.
    pub fn secrets(&self) -> Result<Vec<String>> {
        Ok(self.account_mgr.secrets().into_iter()
            .map(|s| bs58::encode(s.inner().to_repr()).into_string())
            .collect())
    }

    pub fn capabilities(&self) -> Result<Vec<crate::walletdb::CapRecord>> {
        self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    pub fn capability_balance(&self) -> Result<std::collections::HashMap<String, u64>> {
        let caps = self.capabilities()?;
        let mut balances = std::collections::HashMap::new();
        for cap in caps {
            // token_id is now [u8; 32] — encode as bs58 for HashMap key (display boundary)
            let token_key = bs58::encode(&cap.token_id.to_bytes()).into_string();
            *balances.entry(token_key).or_insert(0) += cap.value;
        }
        Ok(balances)
    }
}
