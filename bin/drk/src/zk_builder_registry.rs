//! ZK proof builder registry — keyed by circuit name from manifest.toml.
//!
//! Each contract registers its ZK proof builders by the circuit name
//! declared in its manifest's `[zk_circuits]` section (e.g., "Burn_V1",
//! "Mint_V1"). ManifestContractClient looks up a function's `proof_circuit`
//! field and routes to the registered builder.
//!
//! Builder signature: fn(params_json, wallet_state) -> Result<(call_data, proofs)>

use std::collections::HashMap;
use std::sync::Mutex;
use dwow_sdk::contract_client::WalletStateProvider;

/// Type for a ZK proof builder registered by circuit name.
pub type ZkBuilder = fn(
    params: &str,
    wallet_state: &dyn WalletStateProvider,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), String>;

static REGISTRY: std::sync::LazyLock<Mutex<HashMap<String, ZkBuilder>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a ZK proof builder by circuit name.
/// Circuit name must match the `proof_circuit` field in manifest.toml.
pub fn register(circuit_name: &str, builder: ZkBuilder) {
    REGISTRY.lock().unwrap().insert(circuit_name.to_string(), builder);
}

/// Call the registered ZK builder for a circuit name.
/// Returns Err if no builder is registered for this circuit.
pub fn build(
    circuit_name: &str,
    params: &str,
    wallet_state: &dyn WalletStateProvider,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
    let registry = REGISTRY.lock()
        .map_err(|e| format!("ZK builder registry poisoned: {e}"))?;
    let builder = registry.get(circuit_name)
        .ok_or_else(|| format!(
            "No ZK builder registered for circuit '{}'", circuit_name
        ))?;
    builder(params, wallet_state)
}
