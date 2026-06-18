/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! Capability resolver for the wallet.
//!
//! Scans the local chain (full node) to derive the user's current capabilities
//! and compute available actions. Each contract gets a single-pass resolver
//! that scans its sled tree, derives capabilities, and builds per-instance
//! actions in one traversal.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use dwow_sdk::capability::{
    Action, Capability, CapabilityDescriptor, CapabilityExpression, CapabilityId,
    CapabilityOutput, CapabilitySource,
};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, ContractId, PublicKey, SecretKey};
use tracing::warn;

use dwow_promissory_note_contract::capability::{CAP_NOTE, CAP_MINT_AUTHORITY, CAP_RECEIPT};

use crate::cache::Cache;
use crate::walletdb::{AddressRecord, WalletDb};

/// Result of a capability resolution: what the user holds and what they can do.
#[derive(Clone, Debug)]
pub struct PositionResult {
    /// Capabilities the user currently holds.
    pub capabilities: Vec<Capability>,
    /// Actions available to the user based on their capabilities.
    pub available_actions: Vec<Action>,
}

/// Resolves user capabilities and available actions from local chain state.
pub struct CapabilityResolver {
    /// Loaded contract descriptors, keyed by contract name.
    descriptors: HashMap<String, CapabilityDescriptor>,
}

impl CapabilityResolver {
    /// Create a new resolver with no descriptors loaded.
    pub fn new() -> Self {
        CapabilityResolver { descriptors: HashMap::new() }
    }

    /// Register a contract capability descriptor.
    pub fn register_descriptor(&mut self, desc: CapabilityDescriptor) {
        self.descriptors.insert(desc.name.clone(), desc);
    }

    /// Return all registered descriptors.
    pub fn descriptors(&self) -> &HashMap<String, CapabilityDescriptor> {
        &self.descriptors
    }

    /// Resolve the user's current position by scanning local chain state.
    ///
    /// Derives note capabilities from the wallet's retained capabilities and
    /// per-contract capabilities + actions by scanning contract sled trees.
    pub fn resolve(&self, wallet: &WalletDb, cache: &Cache) -> PositionResult {
        let addresses: Vec<AddressRecord> = match wallet.get_addresses() {
            Ok(addrs) => addrs,
            Err(e) => {
                warn!(target: "capability::resolve",
                      "Failed to fetch addresses: {}", e);
                return PositionResult {
                    capabilities: vec![],
                    available_actions: vec![],
                };
            }
        };

        let user_pubkeys: HashSet<String> =
            addresses.iter().map(|a| a.public_key.clone()).collect();

        let user_secrets: Vec<SecretKey> = addresses
            .iter()
            .filter_map(|a| {
                SecretKey::from_str(&a.secret)
                    .map_err(|e| {
                        warn!(target: "capability::resolve",
                              "Failed to parse secret: {}", e);
                    })
                    .ok()
            })
            .collect();

        let mut capabilities = Vec::new();
        let mut actions = Vec::new();

        // Note capabilities — all retained capabilities the wallet holds
        self.derive_held_capabilities(wallet, &mut capabilities);

        // Generic capabilities — queried once. Surfaced for ALL contracts
        // regardless of whether a descriptor is registered. Contract-specific
        // resolvers add structured interpretation on top.
        let generic_caps = wallet.get_capabilities().unwrap_or_default();

        // Per-contract instance resolution — one pass per contract's sled tree
        // Track descriptors without named resolver arms for Step 1 below.
        let mut no_resolver_descriptors: Vec<&CapabilityDescriptor> = Vec::new();

        for desc in self.descriptors.values() {
            match desc.name.as_str() {
                "promissory_note" => {
                    let cid = *crate::contract_imports::PROMISSORY_NOTE_CONTRACT_ID;
                    self.resolve_promissory_note(cid, wallet, &mut capabilities, &mut actions);
                }
                _ => {
                    // Descriptor registered but no named resolver — track for
                    // per-contract generic surfacing below (Step 1).
                    no_resolver_descriptors.push(desc);
                }
            }
        }

        // Surface generic capabilities for contracts WITH registered descriptors
        // that have no named resolver arm (they hit the _ => above).
        // Only iterates descriptors that were tracked as having no named arm,
        // matching the Python model where _resolve_generic() is only called
        // from the `else` branch.
        for desc in &no_resolver_descriptors {
            if let Some(cid) = crate::contract_imports::get_contract_id(&desc.name) {
                let target_bytes = cid.to_bytes();
                for cap in &generic_caps {
                    if let Ok(cid_bytes) = bs58::decode(&cap.contract_id).into_vec() {
                        if cid_bytes.len() == 32 && cid_bytes == target_bytes {
                            let nullifier_bytes = bs58::decode(&cap.nullifier)
                                .into_vec()
                                .unwrap_or_default();
                            let cap_id = CapabilityId::derive(
                                cid, 0x00, &nullifier_bytes,
                            );
                            capabilities.push(Capability {
                                id: cap_id,
                                contract_id: cid,
                                description: format!(
                                    "Capability from {} at block {} ({})",
                                    &cap.contract_id[..8],
                                    cap.block_height,
                                    cap.note_type,
                                ),
                                source: CapabilitySource::Generic {
                                    note_type: cap.note_type.clone(),
                                    block_height: cap.block_height,
                                },
                                consumable: false,
                                expires_at: None,
                            });
                        }
                    }
                }
            }
        }

        // Manifest-based capability resolution — contracts with a stored manifest
        // get typed capabilities from their manifest declarations instead of
        // appearing as opaque generic capabilities.
        // This is the bridge between on-chain manifests and the wallet UX.
        use crate::manifest_resolver::ManifestResolver;
        for cap in &generic_caps {
            if let Ok(cid_bytes) = bs58::decode(&cap.contract_id).into_vec() {
                if cid_bytes.len() == 32 {
                    let cid_bytes_for_derive = cid_bytes.clone();
                    let cid_arr: [u8; 32] = cid_bytes.try_into().unwrap_or([0u8; 32]);
                    if let Ok(Some(manifest)) = wallet.get_contract_manifest(&cap.contract_id) {
                        let resolver = ManifestResolver::new(&manifest);
                        if let Ok(cid) = ContractId::from_bytes(cid_arr) {
                            for cap_name in resolver.list_capabilities() {
                                if let Some(mcap) = resolver.get_capability(cap_name) {
                                    let cap_id = CapabilityId::derive(
                                        cid, mcap.discriminant, &cid_bytes_for_derive,
                                    );
                                    let source = CapabilitySource::Generic {
                                        note_type: format!("manifest:{}", cap_name),
                                        block_height: cap.block_height as u32,
                                    };
                                    capabilities.push(Capability {
                                        id: cap_id,
                                        contract_id: cid,
                                        description: format!(
                                            "{} ({}) — {}",
                                            cap_name, manifest.name, mcap.description
                                        ),
                                        source,
                                        consumable: !cap_name.contains("authority") && !cap_name.contains("receipt"),
                                        expires_at: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Surface orphan capabilities — contracts with NO registered descriptor
        // AND no stored manifest. These are discovered via Path 2 AEAD scan and
        // stored in the capabilities table, but neither a descriptor nor a manifest
        // directs their resolution. They are surfaced as opaque generic capabilities
        // so the user can see SOMETHING exists.
        // This is what makes kernel Property 4 hold: "New contracts work with
        // zero wallet code changes — even without a manifest."
        let described_contracts: HashSet<[u8; 32]> = self
            .descriptors
            .values()
            .filter_map(|d| crate::contract_imports::get_contract_id(&d.name))
            .map(|cid| cid.to_bytes())
            .collect();

        for cap in &generic_caps {
            if let Ok(cid_bytes) = bs58::decode(&cap.contract_id).into_vec() {
                if cid_bytes.len() == 32 {
                    let cid_arr: [u8; 32] = cid_bytes.try_into().unwrap_or([0u8; 32]);
                    if !described_contracts.contains(&cid_arr) {
                        if let Ok(cid) = ContractId::from_bytes(cid_arr) {
                            let nullifier_bytes = bs58::decode(&cap.nullifier)
                                .into_vec()
                                .unwrap_or_default();
                            let cap_id = CapabilityId::derive(
                                cid, 0x00, &nullifier_bytes,
                            );
                            capabilities.push(Capability {
                                id: cap_id,
                                contract_id: cid,
                                description: format!(
                                    "Capability from {} at block {} ({})",
                                    &cap.contract_id[..8],
                                    cap.block_height,
                                    cap.note_type,
                                ),
                                source: CapabilitySource::Generic {
                                    note_type: cap.note_type.clone(),
                                    block_height: cap.block_height,
                                },
                                consumable: false,
                                expires_at: None,
                            });
                        }
                    }
                }
            }
        }

        PositionResult { capabilities, available_actions: actions }
    }

    // ── Note capabilities ──────────────────────────────────────────────

    /// Derive note capabilities from retained wallet capabilities.
    fn derive_held_capabilities(&self, wallet: &WalletDb, held: &mut Vec<Capability>) {
        let pn_cid = *crate::contract_imports::PROMISSORY_NOTE_CONTRACT_ID;

        let coins = match wallet.get_held_capabilities(Some(false)) {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "capability::derive_held_capabilities",
                      "Failed to fetch coins: {}", e);
                return;
            }
        };

        for coin in &coins {
            let coin_id_bytes = match Self::decode_cap_id(&coin.cap_id) {
                Some(b) => b,
                None => continue,
            };

            let is_receipt = coin.value == 0 && coin.spend_hook.is_some();
            let (cap_type, description) = if is_receipt {
                (CAP_RECEIPT, format!("Receipt for token {}", &coin.token_id[..8]))
            } else {
                (CAP_NOTE, format!("Coin worth {}", coin.value))
            };

            let cap_id = CapabilityId::derive(pn_cid, cap_type, &coin_id_bytes);
            held.push(Capability {
                id: cap_id,
                contract_id: pn_cid,
                description,
                source: CapabilitySource::Note { note_id: coin_id_bytes },
                consumable: !is_receipt,
                expires_at: None,
            });
        }
    }

    /// Decode a cap_id string (bs58) to a 32-byte array.
    fn decode_cap_id(cap_id: &str) -> Option<[u8; 32]> {
        let bytes = bs58::decode(cap_id).into_vec().ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(arr)
    }

    // ── Promissory Note resolution ──────────────────────────────────────

    /// Scan the wallet token registry for mint authorities the user controls.
    ///
    /// For each token where the user holds the mint authority, derive a
    /// `CAP_MINT_AUTHORITY` capability with corresponding `MintV1` actions.
    fn resolve_promissory_note(
        &self,
        pn_cid: ContractId,
        wallet: &WalletDb,
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        let tokens = match wallet.get_all_tokens() {
            Ok(t) => t,
            Err(e) => {
                warn!(target: "capability::resolve_promissory_note",
                      "Failed to fetch tokens: {}", e);
                return;
            }
        };

        for token in &tokens {
            // If the token has a mint_authority, the user controls minting
            if token.mint_authority.is_none() {
                continue;
            }

            let token_id_bytes = match Self::decode_cap_id(&token.token_id) {
                Some(b) => b,
                None => continue,
            };

            let cap_id = CapabilityId::derive(pn_cid, CAP_MINT_AUTHORITY, &token_id_bytes);
            let token_label = token
                .symbol
                .as_deref()
                .unwrap_or(&token.token_id[..8]);

            capabilities.push(Capability {
                id: cap_id,
                contract_id: pn_cid,
                description: format!("Mint authority for {}", token_label),
                source: CapabilitySource::Role {
                    state: Default::default(),
                    role: "mint_authority".into(),
                    instance_id: token_id_bytes,
                },
                consumable: false,
                expires_at: if token.is_frozen {
                    token.freeze_height.map(|h| h as u64)
                } else {
                    None
                },
            });

            // MintV1 — mint more coins of this token type
            actions.push(Action {
                function_id: 0x02,
                name: "MintV1".into(),
                contract_id: pn_cid,
                description: format!("Mint new coins of {}", token_label),
                requires: CapabilityExpression::All(vec![cap_id]),
                consumes: vec![],
                produces: vec![CapabilityOutput {
                    id: CapabilityId::derive(pn_cid, CAP_NOTE, b"output"),
                    description: "Newly minted coin".into(),
                }],
            });
        }
    }

}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self::new()
    }
}

// All per-contract capability resolvers removed. The wallet now discovers
// contract interfaces from on-chain manifests via the generic fallback path
// and the orphan capability handler. Only Promissory Note (genesis) retains
// a hardcoded resolver. See: doc/src/arch/manifest.md, doc/src/arch/wallet.md
//
// The old per-contract resolver code (resolve_escrow, resolve_darkbet_exchange,
// resolve_dao_escrow, resolve_betting_stake, resolve_bearer_bond, resolve_pool_stake,
// resolve_lottery, resolve_baccarat, resolve_darktoshi_dice, resolve_game_room,
// resolve_roulette, resolve_slot, resolve_otc_swap, resolve_auction, resolve_dex,
// resolve_subscription, resolve_relayer_endowment) and their test code have been
// removed. These now belong in their contract crates as optional Rust client code
// or are replaced by manifest-based discovery.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_new_is_empty() {
        let resolver = CapabilityResolver::new();
        assert!(resolver.descriptors().is_empty());
    }
}
