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
        let user_pubkeys: HashSet<String> = match wallet.get_addresses() {
            Ok(addrs) => addrs.into_iter().map(|a| a.public_key).collect(),
            Err(e) => {
                warn!(target: "capability::resolve",
                      "Failed to fetch addresses: {}", e);
                HashSet::new()
            }
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_escrow_contract::capability::descriptor;
    use dwow_escrow_contract::model::{Escrow, EscrowState};
    use dwow_escrow_contract::ESCROW_CONTRACT_ESCROWS_TREE;
    use dwow_sdk::crypto::keypair::{PublicKey, SecretKey};
    use dwow_sdk::crypto::pasta_prelude::{Group, PrimeField};
    use dwow_sdk::crypto::ContractId;
    use dwow_sdk::pasta::pallas;
    use dwow_serial::serialize;

    use crate::cache::Cache;
    use crate::contract_imports::{self, ESCROW_CONTRACT_ID};
    use crate::walletdb::{CoinRecord, MerkleProof, WalletDb};

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Create a deterministic test public key from a u64 seed.
    fn pk(val: u64) -> PublicKey {
        PublicKey::from_secret(SecretKey::from(pallas::Base::from(val)))
    }

    /// Create a valid bs58-encoded 32-byte coin_id from a u64 seed.
    fn coin_id_str(seed: u64) -> String {
        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&seed.to_le_bytes());
        bs58::encode(&bytes).into_string()
    }

    /// Ensure contract IDs are registered (idempotent across parallel tests).
    fn init_contract_ids() {
        let _ = contract_imports::register_contract_id(
            "money_v3",
            ContractId::from(pallas::Base::from(1)),
        );
        let _ = contract_imports::register_contract_id(
            "escrow",
            ContractId::from(pallas::Base::from(2)),
        );
    }

    fn setup_sled() -> sled::Db {
        sled::Config::new().temporary(true).open().unwrap()
    }

    fn setup_cache(db: &sled::Db) -> Cache {
        Cache::new(db).unwrap()
    }

    fn setup_wallet() -> std::sync::Arc<WalletDb> {
        let wallet = WalletDb::new(None, Some("pw")).unwrap();
        wallet.exec_batch_sql(include_str!("../wallet.sql")).unwrap();
        wallet
    }

    fn add_address(wallet: &WalletDb, pk: &PublicKey) {
        wallet.insert_address(&pk.to_string(), "secret", true, 0).unwrap();
    }

    fn add_coin(wallet: &WalletDb, seed: u64, value: u64) {
        let coin = CoinRecord {
            coin_id: coin_id_str(seed),
            value,
            token_id: "token".into(),
            spend_hook: None,
            user_data: None,
            leaf_position: seed,
            secret: "secret".into(),
            coin_blind: "blind".into(),
            value_blind: "vblind".into(),
            token_blind: "tblind".into(),
            spent: false,
            spent_at_height: None,
            created_at_height: 0,
        };
        let proof = MerkleProof { siblings: vec![], root: "root".into() };
        wallet.insert_coin(&coin, &proof).unwrap();
    }

    fn insert_escrow_to_sled(db: &sled::Db, cid: ContractId, escrow: &Escrow) {
        let tree_name = cid.hash_state_id(ESCROW_CONTRACT_ESCROWS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(escrow.id.to_repr(), serialize(escrow)).unwrap();
    }

    /// Insert arbitrary bytes into the escrow sled tree (for corrupt entry tests).
    fn insert_corrupt_entry(db: &sled::Db, cid: ContractId, key: &[u8], bytes: &[u8]) {
        let tree_name = cid.hash_state_id(ESCROW_CONTRACT_ESCROWS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(key, bytes).unwrap();
    }

    fn make_escrow(
        id_val: u64,
        buyer_pk: PublicKey,
        seller_pk: PublicKey,
        state: EscrowState,
    ) -> Escrow {
        Escrow {
            id: pallas::Base::from(id_val),
            buyer_pubkey: buyer_pk,
            seller_pubkey: seller_pk,
            value: 1000,
            token_id: pallas::Base::zero(),
            timeout: 50000,
            state,
            value_commit: pallas::Point::identity(),
            value_blind: pallas::Scalar::zero(),
            spent_nullifier: pallas::Base::zero(),
            created_at: 100,
            funded_at: if matches!(state, EscrowState::Funded | EscrowState::Claimed | EscrowState::Refunded) {
                Some(200)
            } else {
                None
            },
        }
    }

    fn resolver_with_escrow() -> CapabilityResolver {
        let mut resolver = CapabilityResolver::new();
        let cid = ESCROW_CONTRACT_ID.get().unwrap();
        resolver.register_descriptor(descriptor(*cid));
        resolver
    }

    // ── Tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_empty_wallet() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        assert!(result.capabilities.is_empty(), "expected no capabilities, got {:?}", result.capabilities);
        assert!(result.available_actions.is_empty(), "expected no actions, got {:?}", result.available_actions);
    }

    #[test]
    fn test_coins_only() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        add_coin(&wallet, 1, 100);
        add_coin(&wallet, 2, 200);
        add_coin(&wallet, 3, 300);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        assert_eq!(result.capabilities.len(), 3);
        for cap in &result.capabilities {
            assert!(cap.description.contains("Coin worth"));
            assert!(cap.consumable);
            assert!(matches!(cap.source, CapabilitySource::Coin { .. }));
        }
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_escrow_created_buyer() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &buyer);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Created);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let creator_cap = result.capabilities.iter().find(|c| {
            c.description.contains("Creator") && c.description.contains("Created")
        }).unwrap();
        assert!(creator_cap.consumable);
        assert!(matches!(creator_cap.source, CapabilitySource::Role { ref state, ref role, .. } if state == "Created" && role == "Creator"));

        let cancel = result.available_actions.iter().find(|a| a.name == "CancelEscrow").unwrap();
        assert_eq!(cancel.function_id, 0x05);
        assert_eq!(cancel.contract_id, escrow_cid);
    }

    #[test]
    fn test_escrow_created_seller() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &seller);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Created);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let cp_cap = result.capabilities.iter().find(|c| {
            c.description.contains("Counterparty") && c.description.contains("Created")
        }).unwrap();
        assert!(cp_cap.consumable);
        assert!(matches!(cp_cap.source, CapabilitySource::Role { ref state, ref role, .. } if state == "Created" && role == "Counterparty"));

        let fund = result.available_actions.iter().find(|a| a.name == "FundEscrow").unwrap();
        assert_eq!(fund.function_id, 0x02);
    }

    #[test]
    fn test_escrow_funded_buyer() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &buyer);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Funded);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let creator_cap = result.capabilities.iter().find(|c| {
            c.description.contains("Creator") && c.description.contains("Funded")
        }).unwrap();
        assert!(creator_cap.consumable);
        assert_eq!(creator_cap.expires_at, Some(50000));

        let refund = result.available_actions.iter().find(|a| a.name == "RefundEscrow").unwrap();
        assert_eq!(refund.function_id, 0x04);
    }

    #[test]
    fn test_escrow_funded_seller() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &seller);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Funded);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let cp_cap = result.capabilities.iter().find(|c| {
            c.description.contains("Counterparty") && c.description.contains("Funded")
        }).unwrap();
        assert!(cp_cap.consumable);
        assert_eq!(cp_cap.expires_at, Some(50000));

        let claim = result.available_actions.iter().find(|a| a.name == "ClaimEscrow").unwrap();
        assert_eq!(claim.function_id, 0x03);
    }

    #[test]
    fn test_terminal_claimed() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &buyer);
        add_address(&wallet, &seller);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Claimed);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // No escrow capabilities or actions for terminal states
        let escrow_caps: Vec<_> = result.capabilities.iter()
            .filter(|c| matches!(c.source, CapabilitySource::Role { .. }))
            .collect();
        assert!(escrow_caps.is_empty(), "expected no role caps, got {:?}", escrow_caps);
        assert!(result.available_actions.is_empty(), "expected no actions, got {:?}", result.available_actions);
    }

    #[test]
    fn test_terminal_refunded() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &buyer);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Refunded);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let escrow_caps: Vec<_> = result.capabilities.iter()
            .filter(|c| matches!(c.source, CapabilitySource::Role { .. }))
            .collect();
        assert!(escrow_caps.is_empty());
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_terminal_cancelled() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let buyer = pk(100);
        let seller = pk(200);
        add_address(&wallet, &buyer);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let escrow = make_escrow(1, buyer, seller, EscrowState::Cancelled);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let escrow_caps: Vec<_> = result.capabilities.iter()
            .filter(|c| matches!(c.source, CapabilitySource::Role { .. }))
            .collect();
        assert!(escrow_caps.is_empty());
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_multi_instance_mixed_states() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        let other = pk(999);
        add_address(&wallet, &user);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();

        // Escrow 1: Created, user is buyer
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(1, user, other, EscrowState::Created));
        // Escrow 2: Funded, user is buyer
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(2, user, other, EscrowState::Funded));
        // Escrow 3: Claimed (terminal), user is buyer
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(3, user, other, EscrowState::Claimed));

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Should have Creator+Created (from escrow 1) and Creator+Funded (from escrow 2)
        assert!(result.capabilities.iter().any(|c| c.description.contains("Creator") && c.description.contains("Created")));
        assert!(result.capabilities.iter().any(|c| c.description.contains("Creator") && c.description.contains("Funded")));

        // Should have CancelEscrow (from escrow 1) and RefundEscrow (from escrow 2)
        assert!(result.available_actions.iter().any(|a| a.name == "CancelEscrow"));
        assert!(result.available_actions.iter().any(|a| a.name == "RefundEscrow"));

        // Escrow 3 (Claimed) should produce nothing
        let claimed_caps: Vec<_> = result.capabilities.iter()
            .filter(|c| c.description.contains("Claimed"))
            .collect();
        assert!(claimed_caps.is_empty());
    }

    #[test]
    fn test_multi_role_same_user() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        let other1 = pk(201);
        let other2 = pk(202);
        add_address(&wallet, &user);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();

        // Escrow A: user is buyer
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(1, user, other1, EscrowState::Created));
        // Escrow B: user is seller
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(2, other2, user, EscrowState::Funded));

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Should hold Creator+Created (escrow A) AND Counterparty+Funded (escrow B)
        assert!(result.capabilities.iter().any(|c| c.description.contains("Creator") && c.description.contains("Created")));
        assert!(result.capabilities.iter().any(|c| c.description.contains("Counterparty") && c.description.contains("Funded")));

        assert!(result.available_actions.iter().any(|a| a.name == "CancelEscrow"));
        assert!(result.available_actions.iter().any(|a| a.name == "ClaimEscrow"));
    }

    #[test]
    fn test_both_roles_same_instance() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        add_address(&wallet, &user);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();

        // User is BOTH buyer and seller of the same escrow
        let escrow = make_escrow(1, user, user, EscrowState::Created);
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Should hold BOTH Creator+Created AND Counterparty+Created
        assert!(result.capabilities.iter().any(|c| c.description.contains("Creator") && c.description.contains("Created")));
        assert!(result.capabilities.iter().any(|c| c.description.contains("Counterparty") && c.description.contains("Created")));

        // Should have BOTH CancelEscrow (as buyer) AND FundEscrow (as seller)
        assert!(result.available_actions.iter().any(|a| a.name == "CancelEscrow"));
        assert!(result.available_actions.iter().any(|a| a.name == "FundEscrow"));
    }

    #[test]
    fn test_null_empty_sled_tree() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        add_address(&wallet, &user);

        // No escrow tree created — resolver should not panic, just produce nothing
        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let escrow_caps: Vec<_> = result.capabilities.iter()
            .filter(|c| matches!(c.source, CapabilitySource::Role { .. }))
            .collect();
        assert!(escrow_caps.is_empty());
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_null_corrupt_entry() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        let other = pk(200);
        add_address(&wallet, &user);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();

        // Insert a corrupt entry (garbage bytes)
        insert_corrupt_entry(&db, escrow_cid, b"corrupt_key", b"not-valid-escrow-data");

        // Insert a valid entry
        let valid = make_escrow(1, user, other, EscrowState::Created);
        insert_escrow_to_sled(&db, escrow_cid, &valid);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Valid entry should still produce capabilities despite the corrupt one
        assert!(result.capabilities.iter().any(|c| c.description.contains("Creator") && c.description.contains("Created")));
        assert!(result.available_actions.iter().any(|a| a.name == "CancelEscrow"));
    }

    #[test]
    fn test_null_missing_contract_id() {
        // Only register money, NOT escrow — so ESCROW_CONTRACT_ID is unset
        let _ = contract_imports::register_contract_id(
            "money_v3",
            ContractId::from(pallas::Base::from(1)),
        );

        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        add_address(&wallet, &user);
        add_coin(&wallet, 1, 100);

        // Create a resolver WITHOUT escrow descriptor (since escrow not registered)
        let resolver = CapabilityResolver::new();
        let result = resolver.resolve(&wallet, &cache);

        // Coin caps should still be present, but no role caps
        assert!(result.capabilities.iter().any(|c| matches!(c.source, CapabilitySource::Coin { .. })));
        let role_caps: Vec<_> = result.capabilities.iter()
            .filter(|c| matches!(c.source, CapabilitySource::Role { .. }))
            .collect();
        assert!(role_caps.is_empty());
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_null_coin_id_decode_failure() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        // Insert a coin with a garbage coin_id that's not valid bs58
        wallet.exec_sql(
            "INSERT INTO coins (coin_id, value, token_id, leaf_position, secret, coin_blind, value_blind, token_blind, spent, created_at_height) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            &[
                &"!!!not-valid-bs58!!!" as &dyn rusqlite::types::ToSql,
                &100u64,
                &"token",
                &0u64,
                &"secret",
                &"blind",
                &"vblind",
                &"tblind",
                &0i32,
                &0u32,
            ],
        ).unwrap();

        // Insert a valid coin too
        add_coin(&wallet, 1, 200);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Only the valid coin should produce a capability
        assert_eq!(result.capabilities.len(), 1);
        assert!(result.capabilities[0].description.contains("Coin worth 200"));
    }

    #[test]
    fn test_unknown_descriptor_skipped() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        add_coin(&wallet, 1, 100);

        // Register a descriptor with a name that has no resolver method
        let mut resolver = CapabilityResolver::new();
        let fake_cid = ContractId::from(pallas::Base::from(99));
        resolver.register_descriptor(CapabilityDescriptor::new(fake_cid, "unknown_contract"));

        let result = resolver.resolve(&wallet, &cache);

        // Only coin caps — unknown descriptor skipped silently
        assert_eq!(result.capabilities.len(), 1);
        assert!(matches!(result.capabilities[0].source, CapabilitySource::Coin { .. }));
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_no_descriptors_registered() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        add_coin(&wallet, 1, 100);

        let resolver = CapabilityResolver::new();
        let result = resolver.resolve(&wallet, &cache);

        // Coin caps should still appear even with zero descriptors
        assert_eq!(result.capabilities.len(), 1);
        assert!(result.available_actions.is_empty());
    }

    #[test]
    fn test_multiple_addresses_same_wallet() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let addr1 = pk(100);
        let addr2 = pk(200);
        let addr3 = pk(300);
        add_address(&wallet, &addr1);
        add_address(&wallet, &addr2);
        add_address(&wallet, &addr3);

        let other = pk(999);
        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();

        // Each address participates in a different escrow
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(1, addr1, other, EscrowState::Created));
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(2, other, addr2, EscrowState::Funded));
        insert_escrow_to_sled(&db, escrow_cid, &make_escrow(3, addr3, other, EscrowState::Created));

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Should have caps from all 3 addresses
        let role_cap_count = result.capabilities.iter()
            .filter(|c| matches!(c.source, CapabilitySource::Role { .. }))
            .count();
        assert_eq!(role_cap_count, 3, "expected 3 role caps from 3 addresses");

        // addr1=Creator+Created → CancelEscrow
        assert!(result.available_actions.iter().any(|a| a.name == "CancelEscrow"));
        // addr2=Counterparty+Funded → ClaimEscrow
        assert!(result.available_actions.iter().any(|a| a.name == "ClaimEscrow"));
    }

    #[test]
    fn test_escrow_timeout_on_capability() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let user = pk(100);
        let other = pk(200);
        add_address(&wallet, &user);

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();

        let mut escrow = make_escrow(1, user, other, EscrowState::Funded);
        escrow.timeout = 99999;
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        let cap = result.capabilities.iter().find(|c| {
            matches!(c.source, CapabilitySource::Role { ref state, .. } if state == "Funded")
        }).unwrap();
        assert_eq!(cap.expires_at, Some(99999));
    }
}
