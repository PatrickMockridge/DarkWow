// Capability Resolver — core of the wallet's capability browser architecture
//
// Per wallet.md + ocap.md: the wallet derives capabilities from on-chain state
// rather than authenticating identities. This module implements:
//
// 1. TypedCapability — a held capability with manifest-derived human-readable names
// 2. AvailableAction — a contract action the user can take given their capabilities
// 3. CapabilityResolver::resolve() — single-pass resolution per contract
//
// Design (wallet.md):
//   For each held capability, cross-reference with stored manifests to produce
//   typed capabilities. For each typed capability, determine available contract
//   actions. Manifest-less contracts fall through to generic AEAD discovery.

use dwow_sdk::capability::{Barb, Primitive};

use crate::{
    wallet_error::{Error, Result},
    walletdb::WalletPtr,
};

/// Capability status — mirrors the transaction lifecycle.
/// NULL = unspent (spendable). Other states exclude the cap from selection.
///
/// Lifecycle: NULL → Pending (broadcast) → Processing (mined, immature)
///           → Spent (confirmed, >= CONFIRMATION_DEPTH blocks)
/// Fallback:  Pending → NULL (never mined, window expired)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapStatus {
    /// Transaction broadcast, cap in mempool. Not yet mined.
    Pending,
    /// Nullifier on-chain but under CONFIRMATION_DEPTH confirmations.
    Processing,
    /// Fully confirmed. Permanently spent.
    Spent,
}

impl CapStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CapStatus::Pending => "pending",
            CapStatus::Processing => "processing",
            CapStatus::Spent => "spent",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(CapStatus::Pending),
            "processing" => Some(CapStatus::Processing),
            "spent" => Some(CapStatus::Spent),
            _ => None,
        }
    }
}

/// Blocks before a mined spend is confirmed.
/// Matches the chain's COINBASE_MATURITY (src/linear/src/lib.rs:53).
pub const CONFIRMATION_DEPTH: u64 = 100;

/// Blocks before a pending cap is considered abandoned (never mined).
/// Matches CONFIRMATION_DEPTH for consistency — both are 100 blocks.
pub const MEMPOOL_WINDOW: u64 = 100;

/// A typed capability — a held CapRecord with manifest-derived metadata.
/// Resolves "unknown" capabilities to their manifest-declared names.
#[derive(Debug, Clone)]
pub struct TypedCapability {
    pub cap_id: String,
    pub value: u64,
    pub asset_id: String,
    pub contract_id: String,
    pub contract_name: String,
    /// Human-readable capability name from manifest (e.g., "creator", "member")
    pub capability_name: String,
    /// Discriminant from manifest [[capabilities]] section
    pub discriminant: Option<u8>,
    /// Typed capability composition (ocap.md §6)
    pub resource: Option<String>,
    pub action: Option<String>,
    pub primitives: Vec<Primitive>,
    pub barbs: Vec<Barb>,
    pub revoked: bool,
}

/// An action the user can take given their current capabilities.
#[derive(Debug, Clone)]
pub struct AvailableAction {
    pub contract_id: String,
    pub contract_name: String,
    pub function_name: String,
    pub function_code: u8,
    pub description: String,
    /// Capabilities required to exercise this action (from manifest [[actions]])
    pub requires_description: String,
}

/// The result of capability resolution — what the user holds and can do.
#[derive(Debug, Clone)]
pub struct CapabilityView {
    pub capabilities: Vec<TypedCapability>,
    pub actions: Vec<AvailableAction>,
}

/// Resolves held capabilities against stored manifests to produce typed
/// capabilities and available actions. Implements the wallet's "capability
/// browser" function per ocap.md.
pub struct CapabilityResolver {
    wallet: WalletPtr,
}

impl CapabilityResolver {
    /// Create a new resolver backed by the given wallet.
    pub fn new(wallet: WalletPtr) -> Self {
        Self { wallet }
    }

    /// Resolve the user's full capability view: what they hold and what they can do.
    /// Queries held_capabilities and capabilities tables, cross-references with
    /// stored contract manifests, and computes available actions.
    pub fn resolve(&self) -> Result<CapabilityView> {
        let typed_caps = self.resolve_typed_capabilities()?;
        let actions = self.resolve_available_actions(&typed_caps)?;
        Ok(CapabilityView {
            capabilities: typed_caps,
            actions,
        })
    }

    /// For each held capability, cross-reference with stored manifests to produce
    /// typed capabilities with human-readable names. Falls back to "unknown" for
    /// contracts without manifests.
    fn resolve_typed_capabilities(&self) -> Result<Vec<TypedCapability>> {
        let held = self.wallet.get_held_capabilities(None) // P1c: include revoked so [EXERCISED] is reachable
            .map_err(|e| Error::Custom(format!("Failed to get held capabilities: {:?}", e)))?;

        let mut typed = Vec::with_capacity(held.len());
        for cap in &held {
            let asset_id_str = bs58::encode(&cap.asset_id.to_bytes()).into_string();
            // Fix the mis-key bug: manifest lookups key on contract_id, not asset_id.
            let contract_id_str = bs58::encode(&cap.contract_id.to_bytes()).into_string();

            let contract_name = self.wallet
                .get_contract_name_by_id(&contract_id_str)
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".to_string());

            // The generic scan now resolves discriminants and persists typed fields;
            // read them off the CapRecord directly — no more "unknown".
            let capability_name = cap.capability_name.clone()
                .unwrap_or_else(|| "unknown".to_string());

            typed.push(TypedCapability {
                cap_id: cap.cap_id.clone(),
                value: cap.value,
                asset_id: asset_id_str,
                contract_id: contract_id_str,
                contract_name,
                capability_name,
                discriminant: cap.capability_discriminant,
                resource: cap.resource.clone(),
                action: cap.action.clone(),
                primitives: cap.primitives.clone(),
                barbs: cap.barbs.clone(),
                revoked: cap.revoked,
            });
        }
        Ok(typed)
    }

    /// For each typed capability, consult the stored manifest's [[actions]]
    /// section and emit actions the user can take.
    fn resolve_available_actions(&self, caps: &[TypedCapability]) -> Result<Vec<AvailableAction>> {
        use std::collections::HashSet;
        let mut actions = Vec::new();
        let mut seen = HashSet::new();
        for cap in caps {
            // Skip duplicate contract_ids (multiple caps for same contract)
            if !seen.insert(cap.contract_id.clone()) { continue; }
            let manifest = match self.wallet.get_contract_manifest(&cap.contract_id) {
                Ok(Some(m)) => m,
                _ => continue,
            };
            for action in &manifest.actions {
                actions.push(AvailableAction {
                    contract_id: cap.contract_id.clone(),
                    contract_name: cap.contract_name.clone(),
                    function_name: action.function.clone(),
                    function_code: manifest.functions.iter()
                        .find(|f| f.name == action.function)
                        .map(|f| f.code).unwrap_or(0),
                    description: format!("{}::{}", manifest.name, action.function),
                    requires_description: format!("{:?}", action.requires.capabilities),
                });
            }
        }
        Ok(actions)
    }
}
