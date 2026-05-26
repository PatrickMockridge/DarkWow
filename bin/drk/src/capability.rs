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

use dwow_sdk::capability::{
    Action, Capability, CapabilityDescriptor, CapabilityExpression, CapabilityId,
    CapabilityOutput, CapabilitySource,
};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, ContractId};
use tracing::warn;

use crate::cache::Cache;
use crate::walletdb::WalletDb;

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
    /// Derives coin capabilities from the wallet's unspent coins and
    /// per-contract capabilities + actions by scanning contract sled trees.
    pub fn resolve(&self, wallet: &WalletDb, cache: &Cache) -> PositionResult {
        let user_pubkeys: HashSet<String> = wallet
            .get_addresses()
            .map(|addrs| addrs.into_iter().map(|a| a.public_key).collect())
            .unwrap_or_default();

        let mut capabilities = Vec::new();
        let mut actions = Vec::new();

        // Coin capabilities — all unspent coins the wallet holds
        self.derive_coin_capabilities(wallet, &mut capabilities);

        // Per-contract instance resolution — one pass per contract's sled tree
        for desc in self.descriptors.values() {
            if desc.name == "escrow" {
                if let Some(cid) = crate::contract_imports::ESCROW_CONTRACT_ID.get() {
                    self.resolve_escrow(
                        *cid,
                        cache,
                        &user_pubkeys,
                        &mut capabilities,
                        &mut actions,
                    );
                }
            }
        }

        PositionResult { capabilities, available_actions: actions }
    }

    // ── Coin capabilities ──────────────────────────────────────────────

    /// Derive coin capabilities from unspent wallet coins.
    fn derive_coin_capabilities(&self, wallet: &WalletDb, held: &mut Vec<Capability>) {
        let money_cid = match crate::contract_imports::MONEY_V3_CONTRACT_ID.get() {
            Some(cid) => *cid,
            None => return,
        };

        let coins = match wallet.get_coins(false) {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "capability::derive_coin_capabilities",
                      "Failed to fetch coins: {}", e);
                return;
            }
        };

        for coin in &coins {
            let coin_id_bytes = match Self::decode_coin_id(&coin.coin_id) {
                Some(b) => b,
                None => continue,
            };

            let cap_id = CapabilityId::derive(money_cid, 0x00, &coin_id_bytes);
            held.push(Capability {
                id: cap_id,
                contract_id: money_cid,
                description: format!("Coin worth {}", coin.value),
                source: CapabilitySource::Coin { coin_id: coin_id_bytes },
                consumable: true,
                expires_at: None,
            });
        }
    }

    /// Decode a coin_id string (bs58) to a 32-byte array.
    fn decode_coin_id(coin_id: &str) -> Option<[u8; 32]> {
        let bytes = bs58::decode(coin_id).into_vec().ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(arr)
    }

    // ── Escrow resolution (single pass) ────────────────────────────────

    /// Scan the escrow sled tree and derive both capabilities and per-instance
    /// actions in a single pass.
    fn resolve_escrow(
        &self,
        escrow_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_escrow_contract::capability::{
            CAP_COUNTERPARTY_CREATED, CAP_COUNTERPARTY_FUNDED,
            CAP_CREATOR_CREATED, CAP_CREATOR_FUNDED,
        };
        use dwow_escrow_contract::model::{Escrow, EscrowState};
        use dwow_escrow_contract::ESCROW_CONTRACT_ESCROWS_TREE;
        use dwow_serial::deserialize;

        let tree_name = escrow_cid.hash_state_id(ESCROW_CONTRACT_ESCROWS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let escrow: Escrow = match deserialize(&value) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let buyer_pk = escrow.buyer_pubkey.to_string();
            let seller_pk = escrow.seller_pubkey.to_string();
            let is_buyer = user_pubkeys.contains(&buyer_pk);
            let is_seller = user_pubkeys.contains(&seller_pk);

            if !is_buyer && !is_seller {
                continue;
            }

            let escrow_id_bytes = escrow.id.to_repr();
            let display_id = bs58::encode(&escrow_id_bytes).into_string();

            match escrow.state {
                EscrowState::Created => {
                    if is_buyer {
                        let cap_id = CapabilityId::derive(
                            escrow_cid, CAP_CREATOR_CREATED, &escrow_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: escrow_cid,
                            description: format!(
                                "Creator of escrow {} (Created)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Created".into(),
                                role: "Creator".into(),
                                instance_id: escrow_id_bytes,
                            },
                            consumable: true,
                            expires_at: None,
                        });

                        actions.push(Action {
                            function_id: 0x05,
                            name: "CancelEscrow".into(),
                            contract_id: escrow_cid,
                            description: format!(
                                "Cancel escrow {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_CREATOR_CREATED,
                                    &escrow_id_bytes,
                                ),
                            ]),
                            consumes: vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_CREATOR_CREATED,
                                    &escrow_id_bytes,
                                ),
                                CapabilityId::derive(
                                    escrow_cid, CAP_COUNTERPARTY_CREATED,
                                    &escrow_id_bytes,
                                ),
                            ],
                            produces: vec![],
                        });
                    }
                    if is_seller {
                        let cap_id = CapabilityId::derive(
                            escrow_cid, CAP_COUNTERPARTY_CREATED, &escrow_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: escrow_cid,
                            description: format!(
                                "Counterparty of escrow {} (Created)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Created".into(),
                                role: "Counterparty".into(),
                                instance_id: escrow_id_bytes,
                            },
                            consumable: true,
                            expires_at: None,
                        });

                        actions.push(Action {
                            function_id: 0x02,
                            name: "FundEscrow".into(),
                            contract_id: escrow_cid,
                            description: format!(
                                "Fund escrow {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_COUNTERPARTY_CREATED,
                                    &escrow_id_bytes,
                                ),
                            ]),
                            consumes: vec![],
                            produces: vec![
                                CapabilityOutput {
                                    id: CapabilityId::derive(
                                        escrow_cid, CAP_COUNTERPARTY_FUNDED,
                                        &escrow_id_bytes,
                                    ),
                                    description: "Counterparty of funded escrow".into(),
                                },
                                CapabilityOutput {
                                    id: CapabilityId::derive(
                                        escrow_cid, CAP_CREATOR_FUNDED,
                                        &escrow_id_bytes,
                                    ),
                                    description: "Creator of funded escrow".into(),
                                },
                            ],
                        });
                    }
                }
                EscrowState::Funded => {
                    if is_buyer {
                        let cap_id = CapabilityId::derive(
                            escrow_cid, CAP_CREATOR_FUNDED, &escrow_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: escrow_cid,
                            description: format!(
                                "Creator of escrow {} (Funded)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Funded".into(),
                                role: "Creator".into(),
                                instance_id: escrow_id_bytes,
                            },
                            consumable: true,
                            expires_at: Some(escrow.timeout),
                        });

                        actions.push(Action {
                            function_id: 0x04,
                            name: "RefundEscrow".into(),
                            contract_id: escrow_cid,
                            description: format!(
                                "Refund escrow {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_CREATOR_FUNDED,
                                    &escrow_id_bytes,
                                ),
                            ]),
                            consumes: vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_CREATOR_FUNDED,
                                    &escrow_id_bytes,
                                ),
                                CapabilityId::derive(
                                    escrow_cid, CAP_COUNTERPARTY_FUNDED,
                                    &escrow_id_bytes,
                                ),
                            ],
                            produces: vec![],
                        });
                    }
                    if is_seller {
                        let cap_id = CapabilityId::derive(
                            escrow_cid, CAP_COUNTERPARTY_FUNDED, &escrow_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: escrow_cid,
                            description: format!(
                                "Counterparty of escrow {} (Funded)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Funded".into(),
                                role: "Counterparty".into(),
                                instance_id: escrow_id_bytes,
                            },
                            consumable: true,
                            expires_at: Some(escrow.timeout),
                        });

                        actions.push(Action {
                            function_id: 0x03,
                            name: "ClaimEscrow".into(),
                            contract_id: escrow_cid,
                            description: format!(
                                "Claim escrow {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_COUNTERPARTY_FUNDED,
                                    &escrow_id_bytes,
                                ),
                            ]),
                            consumes: vec![
                                CapabilityId::derive(
                                    escrow_cid, CAP_CREATOR_FUNDED,
                                    &escrow_id_bytes,
                                ),
                                CapabilityId::derive(
                                    escrow_cid, CAP_COUNTERPARTY_FUNDED,
                                    &escrow_id_bytes,
                                ),
                            ],
                            produces: vec![],
                        });
                    }
                }
                _ => {
                    // Terminal states — no capabilities or actions
                }
            }
        }
    }

    // ── Expression evaluation ──────────────────────────────────────────

    /// Evaluate whether a set of held capabilities satisfies a capability expression.
    ///
    /// Used for testing and for future instance-aware expression resolution.
    pub fn evaluate_expression(
        held: &[CapabilityId],
        expr: &CapabilityExpression,
    ) -> bool {
        match expr {
            CapabilityExpression::Any(ids) => ids.iter().any(|id| held.contains(id)),
            CapabilityExpression::All(ids) => ids.iter().all(|id| held.contains(id)),
            CapabilityExpression::Not(inner) => !Self::evaluate_expression(held, inner),
            CapabilityExpression::Threshold { capabilities, count: _, total: _ } => {
                capabilities.iter().any(|id| held.contains(id))
            }
        }
    }
}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self::new()
    }
}
