// Lightweight wallet handle — SQLite only, no sled, no P2P.
//
// Used by CLI commands that only need local wallet state (keys, addresses,
// capabilities). The daemon owns sled exclusively; this skips sled entirely
// so CLI processes don't hit WouldBlock lock contention.
//
// Matches SpecWallet.open_local() in wallet_model.py.

use std::sync::Arc;
use crate::wallet_error::{Error, Result};
use crate::walletdb::WalletDb;

pub struct LocalWallet {
    pub wallet: Arc<WalletDb>,
}

impl LocalWallet {
    pub fn open(wallet_path: &str, wallet_pass: &str) -> Result<Self> {
        let path = crate::wallet_util::expand_path(wallet_path)
            .map_err(|e| Error::Custom(format!("expand wallet path: {}", e)))?;
        let wallet = WalletDb::new(Some(path), Some(wallet_pass), false)
            .map_err(|_| Error::Custom("Failed to open wallet database".into()))?;

        // Verify schema exists — Connection::open() creates an empty file
        // if none exists, but the tables may be missing. Return a clear error
        // instead of failing later with a confusing query error.
        if wallet.get_addresses().is_err() {
            return Err(Error::Custom(
                "Wallet not initialized. Run 'wallet initialize' first.".into()
            ));
        }

        Ok(Self { wallet })
    }

    pub fn default_address(&self) -> Result<String> {
        let addrs = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("{:?}", e)))?;
        addrs.first()
            .map(|a| a.public_key.clone())
            .ok_or_else(|| Error::Custom("No addresses — run 'wallet keygen'".into()))
    }

    pub fn addresses(&self) -> Result<Vec<String>> {
        self.wallet.get_addresses()
            .map(|addrs| addrs.into_iter().map(|a| a.public_key).collect())
            .map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    pub fn secrets(&self) -> Result<Vec<String>> {
        self.wallet.get_secrets()
            .map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    pub fn capabilities(&self) -> Result<Vec<crate::walletdb::CapRecord>> {
        self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    pub fn capability_balance(&self) -> Result<std::collections::HashMap<String, u64>> {
        let caps = self.capabilities()?;
        let mut balances = std::collections::HashMap::new();
        for cap in caps {
            *balances.entry(cap.token_id).or_insert(0) += cap.value;
        }
        Ok(balances)
    }
}
