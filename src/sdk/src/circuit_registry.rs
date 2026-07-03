//! ZK circuit builder registry — keyed by circuit name from manifest.toml.
//!
//! Contract crates self-register their ZK proof builders here at startup.
//! The SDK provides this registry so crates don't need to depend on the wallet.
//!
//! Lifecycle:
//!   1. Contract crate calls `register("burn_v1", my_builder)` at startup
//!   2. ManifestContractClient reads `proof_circuit` from the manifest
//!   3. Routes to the registered builder by circuit name
//!
//! Builder signature: fn(params_json, wallet_state) -> Result<(call_data, proofs)>

use std::collections::HashMap;
use std::sync::Mutex;
use crate::contract_client::WalletStateProvider;

pub type CircuitBuilder = fn(
    params: &str,
    wallet_state: &dyn WalletStateProvider,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), String>;

static REGISTRY: std::sync::LazyLock<Mutex<HashMap<String, CircuitBuilder>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a ZK proof builder by circuit name.
/// Circuit name must match the `proof_circuit` field in manifest.toml.
/// Called at startup by contract crates. Panics on mutex poison (startup bug).
pub fn register(circuit_name: &str, builder: CircuitBuilder) {
    REGISTRY.lock()
        .expect("circuit registry mutex poisoned during registration")
        .insert(circuit_name.to_string(), builder);
}

/// Check whether a builder is registered for a circuit name.
pub fn is_registered(circuit_name: &str) -> bool {
    REGISTRY.lock()
        .map(|r| r.contains_key(circuit_name))
        .unwrap_or(false)
}

/// Build a ZK proof for the given circuit.
/// Returns Err if no builder is registered or the registry lock is poisoned.
pub fn build(
    circuit_name: &str,
    params: &str,
    wallet_state: &dyn WalletStateProvider,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
    let registry = REGISTRY.lock()
        .map_err(|e| format!("circuit registry lock poisoned: {e}"))?;
    let builder = registry.get(circuit_name)
        .ok_or_else(|| format!(
            "no ZK builder registered for circuit '{}'", circuit_name
        ))?;
    builder(params, wallet_state)
}
