//! Manifest-driven ContractClient — the single implementation for all
//! contracts except NativeToken and Deployooor (hardcoded infrastructure).
//!
//! Reads a contract's manifest.toml and derives function dispatch, opcodes,
//! and ZK proof routing from it. No hardcoded per-contract dispatch.
//!
//! Architecture:
//!   manifest.toml [functions] → function_selector(), supported_functions()
//!   manifest.toml [functions].proof_circuit → ZK builder registry → build()

use dwow_sdk::contract_client::{ContractClient, WalletStateProvider};
use dwow_sdk::manifest::ContractManifest;

/// A ContractClient derived entirely from a contract's manifest.toml.
/// Works for any contract that has a manifest — no per-contract code needed.
pub struct ManifestContractClient {
    /// Owned manifest — the single source of truth for this contract.
    manifest: ContractManifest,
    /// Contract name.
    name: &'static str,
}

impl ManifestContractClient {
    /// Create a new manifest-driven client from raw TOML bytes.
    /// The TOML is parsed into a ContractManifest at construction time.
    pub fn new(name: &'static str, manifest_toml: &str) -> Result<Self, String> {
        let manifest: ContractManifest = toml::from_str(manifest_toml)
            .map_err(|e| format!("ManifestContractClient({name}): parse manifest: {e}"))?;
        Ok(Self { manifest, name })
    }
}

impl ContractClient for ManifestContractClient {
    fn contract_name(&self) -> &'static str {
        self.name
    }

    fn function_selector(&self, function: &str) -> Option<u8> {
        self.manifest.functions.iter()
            .find(|f| f.name == function)
            .map(|f| f.code)
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        self.manifest.functions.iter()
            .map(|f| {
                // Leak to get &'static str — acceptable for fixed set
                // registered at startup and never dropped.
                Box::leak(f.name.clone().into_boxed_str()) as &'static str
            })
            .collect()
    }

    fn build(
        &self,
        function: &str,
        params: &str,
        wallet_state: &dyn WalletStateProvider,
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        let func = self.manifest.functions.iter()
            .find(|f| f.name == function)
            .ok_or_else(|| format!(
                "{}: unsupported function '{}'", self.name, function
            ))?;

        if let Some(ref circuit_name) = func.proof_circuit {
            // Function requires a ZK proof — route through the builder registry
            crate::zk_builder_registry::build(circuit_name, params, wallet_state)
        } else {
            // No proof required — return empty call data and proofs
            Ok((vec![], vec![]))
        }
    }
}
