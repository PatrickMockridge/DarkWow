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

use dwow_promissory_note_contract::capability::{CAP_COIN, CAP_MINT_AUTHORITY, CAP_RECEIPT};

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
    /// Derives coin capabilities from the wallet's unspent coins and
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

        // Coin capabilities — all unspent coins the wallet holds
        self.derive_coin_capabilities(wallet, &mut capabilities);

        // Per-contract instance resolution — one pass per contract's sled tree
        for desc in self.descriptors.values() {
            match desc.name.as_str() {
                "promissory_note" => {
                    if let Some(cid) = crate::contract_imports::PROMISSORY_NOTE_CONTRACT_ID.get() {
                        self.resolve_promissory_note(*cid, wallet, &mut capabilities, &mut actions);
                    }
                }
                "escrow" => {
                    if let Some(cid) = crate::contract_imports::ESCROW_CONTRACT_ID.get() {
                        self.resolve_escrow(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "darkbet_exchange" => {
                    if let Some(cid) = crate::contract_imports::DARKBET_EXCHANGE_CONTRACT_ID.get() {
                        self.resolve_darkbet_exchange(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "dao_escrow" => {
                    if let Some(cid) = crate::contract_imports::DAO_ESCROW_CONTRACT_ID.get() {
                        self.resolve_dao_escrow(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "betting_stake" => {
                    if let Some(cid) = crate::contract_imports::BETTING_STAKE_CONTRACT_ID.get() {
                        self.resolve_betting_stake(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "pool_stake" => {
                    if let Some(cid) = crate::contract_imports::POOL_STAKE_CONTRACT_ID.get() {
                        self.resolve_pool_stake(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "lottery" => {
                    if let Some(cid) = crate::contract_imports::LOTTERY_CONTRACT_ID.get() {
                        self.resolve_lottery(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "otc_swap" => {
                    if let Some(cid) = crate::contract_imports::OTC_SWAP_CONTRACT_ID.get() {
                        self.resolve_otc_swap(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "baccarat" => {
                    if let Some(cid) = crate::contract_imports::BACCARAT_CONTRACT_ID.get() {
                        self.resolve_baccarat(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "darktoshi_dice" => {
                    if let Some(cid) = crate::contract_imports::DARKTOSHI_DICE_CONTRACT_ID.get() {
                        self.resolve_darktoshi_dice(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "game_room" => {
                    if let Some(cid) = crate::contract_imports::GAME_ROOM_CONTRACT_ID.get() {
                        self.resolve_game_room(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "roulette" => {
                    if let Some(cid) = crate::contract_imports::ROULETTE_CONTRACT_ID.get() {
                        self.resolve_roulette(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "slot" => {
                    if let Some(cid) = crate::contract_imports::SLOT_CONTRACT_ID.get() {
                        self.resolve_slot(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                _ => {
                    // Contracts without resolver methods yet — descriptor is
                    // registered but on-chain scanning is pending.
                }
            }
        }

        PositionResult { capabilities, available_actions: actions }
    }

    // ── Coin capabilities ──────────────────────────────────────────────

    /// Derive coin capabilities from unspent wallet coins.
    fn derive_coin_capabilities(&self, wallet: &WalletDb, held: &mut Vec<Capability>) {
        let pn_cid = match crate::contract_imports::PROMISSORY_NOTE_CONTRACT_ID.get() {
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

            let is_receipt = coin.value == 0 && coin.spend_hook.is_some();
            let (cap_type, description) = if is_receipt {
                (CAP_RECEIPT, format!("Receipt for token {}", &coin.token_id[..8]))
            } else {
                (CAP_COIN, format!("Coin worth {}", coin.value))
            };

            let cap_id = CapabilityId::derive(pn_cid, cap_type, &coin_id_bytes);
            held.push(Capability {
                id: cap_id,
                contract_id: pn_cid,
                description,
                source: CapabilitySource::Coin { coin_id: coin_id_bytes },
                consumable: !is_receipt,
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

            let token_id_bytes = match Self::decode_coin_id(&token.token_id) {
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
                    id: CapabilityId::derive(pn_cid, CAP_COIN, b"output"),
                    description: "Newly minted coin".into(),
                }],
            });
        }
    }

    // ── Escrow resolution (single pass) ────────────────────────────────

    /// Scan the escrow sled tree and derive both capabilities and per-instance
    /// actions in a single pass.
    fn resolve_escrow(
        &self,
        escrow_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
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

            let escrow_id_bytes = escrow.id.to_repr();
            let buyer_pk = escrow.buyer_pubkey.to_string();
            let seller_pk = escrow.seller_pubkey.to_string();
            let instance_id = escrow.instance_seed;
            let is_buyer = user_pubkeys.contains(&buyer_pk)
                || self.matches_derived_key(user_secrets, &escrow_cid, &instance_id, &buyer_pk);
            let is_seller = user_pubkeys.contains(&seller_pk)
                || self.matches_derived_key(user_secrets, &escrow_cid, &instance_id, &seller_pk);

            if !is_buyer && !is_seller {
                continue;
            }

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

    // ── DarkBet Exchange resolution ─────────────────────────────────────

    fn resolve_darkbet_exchange(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_darkbet_exchange_contract::capability::{
            CAP_BACKER, CAP_CREATOR, CAP_LAYER, CAP_LP_PROVIDER, CAP_ORACLE,
        };
        use dwow_darkbet_exchange_contract::model::{
            LpShare, LpShareState, Market, MarketState, Order, OrderState, Position,
            PositionState,
        };
        use dwow_darkbet_exchange_contract::{
            DARKBET_EXCHANGE_BACK_ORDERS_TREE, DARKBET_EXCHANGE_LAY_ORDERS_TREE,
            DARKBET_EXCHANGE_LP_SHARES_TREE, DARKBET_EXCHANGE_MARKETS_TREE,
            DARKBET_EXCHANGE_POSITIONS_TREE,
        };
        use dwow_serial::deserialize;

        // Scan markets
        let tree_name = cid.hash_state_id(DARKBET_EXCHANGE_MARKETS_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let market: Market = match deserialize(&value) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let instance_id = market.instance_seed;
                let market_bytes = market.market_id.to_repr();
                let is_creator = user_pubkeys.contains(&market.creator.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &market.creator.to_string(),
                    );
                if !is_creator {
                    continue;
                }
                let display_id = bs58::encode(&market_bytes).into_string();
                match market.state {
                    MarketState::Open | MarketState::Closed => {
                        let cap_id =
                            CapabilityId::derive(cid, CAP_CREATOR, &market_bytes);
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: cid,
                            description: format!("Creator of market {}", display_id),
                            source: CapabilitySource::Role {
                                state: format!("{:?}", market.state),
                                role: "Creator".into(),
                                instance_id: market_bytes,
                            },
                            consumable: true,
                            expires_at: None,
                        });
                        if market.state == MarketState::Open {
                            actions.push(Action {
                                function_id: 0x04,
                                name: "ResolveMarket".into(),
                                contract_id: cid,
                                description: format!("Resolve market {}", display_id),
                                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                                    cid, CAP_ORACLE, &market_bytes,
                                )]),
                                consumes: vec![],
                                produces: vec![],
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // Scan positions (AMM mode)
        let tree_name = cid.hash_state_id(DARKBET_EXCHANGE_POSITIONS_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let pos: Position = match deserialize(&value) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let instance_id = pos.instance_seed;
                let pos_bytes = pos.position_id.to_repr();
                let is_owner = user_pubkeys.contains(&pos.owner.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &pos.owner.to_string(),
                    );
                if !is_owner {
                    continue;
                }
                if pos.state == PositionState::Active {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_BACKER, &pos_bytes);
                    let display_id = bs58::encode(&pos_bytes).into_string();
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Position holder {}", display_id),
                        source: CapabilitySource::Role {
                            state: "Active".into(),
                            role: "PositionOwner".into(),
                            instance_id: pos_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                    actions.push(Action {
                        function_id: 0x0A,
                        name: "ClaimWinnings".into(),
                        contract_id: cid,
                        description: format!("Claim winnings for position {}", display_id),
                        requires: CapabilityExpression::All(vec![CapabilityId::derive(
                            cid, CAP_BACKER, &pos_bytes,
                        )]),
                        consumes: vec![CapabilityId::derive(cid, CAP_BACKER, &pos_bytes)],
                        produces: vec![],
                    });
                }
            }
        }

        // Scan LP shares
        let tree_name = cid.hash_state_id(DARKBET_EXCHANGE_LP_SHARES_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let lp: LpShare = match deserialize(&value) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                let instance_id = lp.instance_seed;
                let lp_bytes = lp.lp_share_id.to_repr();
                let is_provider = user_pubkeys.contains(&lp.provider.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &lp.provider.to_string(),
                    );
                if !is_provider {
                    continue;
                }
                if lp.state == LpShareState::Active {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_LP_PROVIDER, &lp_bytes);
                    let display_id = bs58::encode(&lp_bytes).into_string();
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("LP provider {}", display_id),
                        source: CapabilitySource::Role {
                            state: "Active".into(),
                            role: "LpProvider".into(),
                            instance_id: lp_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                    actions.push(Action {
                        function_id: 0x09,
                        name: "RemoveLiquidity".into(),
                        contract_id: cid,
                        description: format!("Remove liquidity {}", display_id),
                        requires: CapabilityExpression::All(vec![CapabilityId::derive(
                            cid, CAP_LP_PROVIDER, &lp_bytes,
                        )]),
                        consumes: vec![CapabilityId::derive(cid, CAP_LP_PROVIDER, &lp_bytes)],
                        produces: vec![],
                    });
                }
            }
        }

        // Scan back orders
        let tree_name = cid.hash_state_id(DARKBET_EXCHANGE_BACK_ORDERS_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let order: Order = match deserialize(&value) {
                    Ok(o) => o,
                    Err(_) => continue,
                };
                let instance_id = order.instance_seed;
                let order_bytes = order.order_id.to_repr();
                let is_user = user_pubkeys.contains(&order.user_pub.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &order.user_pub.to_string(),
                    );
                if !is_user {
                    continue;
                }
                if order.state == OrderState::Open {
                    let display_id = bs58::encode(&order_bytes).into_string();
                    let cap_id =
                        CapabilityId::derive(cid, CAP_BACKER, &order_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Back order {}", display_id),
                        source: CapabilitySource::Role {
                            state: "Open".into(),
                            role: "Backer".into(),
                            instance_id: order_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                    actions.push(Action {
                        function_id: 0x06,
                        name: "CancelOrder".into(),
                        contract_id: cid,
                        description: format!("Cancel back order {}", display_id),
                        requires: CapabilityExpression::All(vec![CapabilityId::derive(
                            cid, CAP_BACKER, &order_bytes,
                        )]),
                        consumes: vec![CapabilityId::derive(cid, CAP_BACKER, &order_bytes)],
                        produces: vec![],
                    });
                }
            }
        }

        // Scan lay orders
        let tree_name = cid.hash_state_id(DARKBET_EXCHANGE_LAY_ORDERS_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let order: Order = match deserialize(&value) {
                    Ok(o) => o,
                    Err(_) => continue,
                };
                let instance_id = order.instance_seed;
                let order_bytes = order.order_id.to_repr();
                let is_user = user_pubkeys.contains(&order.user_pub.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &order.user_pub.to_string(),
                    );
                if !is_user {
                    continue;
                }
                if order.state == OrderState::Open {
                    let display_id = bs58::encode(&order_bytes).into_string();
                    let cap_id =
                        CapabilityId::derive(cid, CAP_LAYER, &order_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Lay order {}", display_id),
                        source: CapabilitySource::Role {
                            state: "Open".into(),
                            role: "Layer".into(),
                            instance_id: order_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                    actions.push(Action {
                        function_id: 0x06,
                        name: "CancelOrder".into(),
                        contract_id: cid,
                        description: format!("Cancel lay order {}", display_id),
                        requires: CapabilityExpression::All(vec![CapabilityId::derive(
                            cid, CAP_LAYER, &order_bytes,
                        )]),
                        consumes: vec![CapabilityId::derive(cid, CAP_LAYER, &order_bytes)],
                        produces: vec![],
                    });
                }
            }
        }
    }

    // ── DAO-Escrow resolution ───────────────────────────────────────────

    fn resolve_dao_escrow(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_dao_escrow_contract::capability::{CAP_OWNER, CAP_TREASURY_GOV};
        use dwow_dao_escrow_contract::model::DaoEscrow;
        use dwow_dao_escrow_contract::DAO_ESCROW_CONTRACT_BULLAS_TREE;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(DAO_ESCROW_CONTRACT_BULLAS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let dao: DaoEscrow = match deserialize(&value) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let instance_id = dao.instance_seed;
            let is_owner = user_pubkeys.contains(&dao.owner_pubkey.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &dao.owner_pubkey.to_string(),
                );
            if !is_owner {
                continue;
            }
            let display_id = bs58::encode(&instance_id).into_string();
            let cap_id = CapabilityId::derive(cid, CAP_OWNER, &instance_id);
            capabilities.push(Capability {
                id: cap_id,
                contract_id: cid,
                description: format!("Owner of DAO escrow {}", display_id),
                source: CapabilitySource::Role {
                    state: "Active".into(),
                    role: "Owner".into(),
                    instance_id,
                },
                consumable: true,
                expires_at: None,
            });
            // Owner can pay premium and propose claims
            actions.push(Action {
                function_id: 0x02,
                name: "PayPremium".into(),
                contract_id: cid,
                description: format!("Pay premium to DAO {}", display_id),
                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                    cid, CAP_OWNER, &instance_id,
                )]),
                consumes: vec![],
                produces: vec![],
            });
            actions.push(Action {
                function_id: 0x07,
                name: "ProposeClaim".into(),
                contract_id: cid,
                description: format!("Propose claim to DAO {}", display_id),
                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                    cid, CAP_OWNER, &instance_id,
                )]),
                consumes: vec![],
                produces: vec![],
            });
        }
    }

    // ── BettingStake resolution ──────────────────────────────────────────

    fn resolve_betting_stake(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_betting_stake_contract::capability::CAP_STAKER;
        use dwow_betting_stake_contract::model::Stake;
        use dwow_betting_stake_contract::BETTING_STAKE_STAKES_TREE;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(BETTING_STAKE_STAKES_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let stake: Stake = match deserialize(&value) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let instance_id = stake.instance_seed;
            let stake_bytes = stake.stake_id.to_repr();
            let is_staker = user_pubkeys.contains(&stake.staker_pub.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &stake.staker_pub.to_string(),
                );
            if !is_staker {
                continue;
            }
            let display_id = bs58::encode(&stake_bytes).into_string();
            let cap_id = CapabilityId::derive(cid, CAP_STAKER, &stake_bytes);
            capabilities.push(Capability {
                id: cap_id,
                contract_id: cid,
                description: format!("Staker of stake {}", display_id),
                source: CapabilitySource::Role {
                    state: if stake.is_active { "Active".into() } else { "Inactive".into() },
                    role: "Staker".into(),
                    instance_id: stake_bytes,
                },
                consumable: true,
                expires_at: None,
            });
            actions.push(Action {
                function_id: 0x04,
                name: "Unstake".into(),
                contract_id: cid,
                description: format!("Unstake {}", display_id),
                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                    cid, CAP_STAKER, &stake_bytes,
                )]),
                consumes: vec![CapabilityId::derive(cid, CAP_STAKER, &stake_bytes)],
                produces: vec![],
            });
            actions.push(Action {
                function_id: 0x05,
                name: "ClaimEarnings".into(),
                contract_id: cid,
                description: format!("Claim earnings from stake {}", display_id),
                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                    cid, CAP_STAKER, &stake_bytes,
                )]),
                consumes: vec![],
                produces: vec![],
            });
        }
    }

    // ── PoolStake resolution ────────────────────────────────────────────

    fn resolve_pool_stake(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_pool_stake_contract::capability::CAP_POOL_MEMBER;
        use dwow_pool_stake_contract::model::PoolMemberStake;
        use dwow_pool_stake_contract::POOL_STAKE_MEMBERS_TREE;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(POOL_STAKE_MEMBERS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let member: PoolMemberStake = match deserialize(&value) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let instance_id = member.instance_seed;
            let pool_bytes = member.pool_id.to_repr();
            let is_member = user_pubkeys.contains(&member.member_pub.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &member.member_pub.to_string(),
                );
            if !is_member {
                continue;
            }
            let display_id = bs58::encode(&pool_bytes).into_string();
            let cap_id =
                CapabilityId::derive(cid, CAP_POOL_MEMBER, &pool_bytes);
            capabilities.push(Capability {
                id: cap_id,
                contract_id: cid,
                description: format!("Member of pool {}", display_id),
                source: CapabilitySource::Role {
                    state: "Active".into(),
                    role: "PoolMember".into(),
                    instance_id: pool_bytes,
                },
                consumable: true,
                expires_at: None,
            });
            actions.push(Action {
                function_id: 0x04,
                name: "LeavePool".into(),
                contract_id: cid,
                description: format!("Leave pool {}", display_id),
                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                    cid, CAP_POOL_MEMBER, &pool_bytes,
                )]),
                consumes: vec![CapabilityId::derive(cid, CAP_POOL_MEMBER, &pool_bytes)],
                produces: vec![],
            });
        }
    }

    // ── Lottery resolution ──────────────────────────────────────────────

    fn resolve_lottery(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_lottery_contract::capability::{CAP_HOUSE, CAP_PLAYER};
        use dwow_lottery_contract::model::{Lottery, Ticket};
        use dwow_lottery_contract::{
            LOTTERY_CONTRACT_LOTTERIES_TREE, LOTTERY_CONTRACT_TICKETS_TREE,
        };
        use dwow_serial::deserialize;

        // Scan lotteries
        let tree_name = cid.hash_state_id(LOTTERY_CONTRACT_LOTTERIES_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let lottery: Lottery = match deserialize(&value) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                let instance_id = lottery.instance_seed;
                let lot_bytes = lottery.id.to_repr();
                let is_house = user_pubkeys.contains(&lottery.house_pub.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &lottery.house_pub.to_string(),
                    );
                if is_house {
                    let display_id = bs58::encode(&lot_bytes).into_string();
                    let cap_id =
                        CapabilityId::derive(cid, CAP_HOUSE, &lot_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("House of lottery {}", display_id),
                        source: CapabilitySource::Role {
                            state: format!("{:?}", lottery.state),
                            role: "House".into(),
                            instance_id: lot_bytes,
                        },
                        consumable: false,
                        expires_at: None,
                    });
                }
            }
        }

        // Scan tickets
        let tree_name = cid.hash_state_id(LOTTERY_CONTRACT_TICKETS_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let ticket: Ticket = match deserialize(&value) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let instance_id = ticket.instance_seed;
                let tick_bytes = ticket.id.to_repr();
                let is_player = user_pubkeys.contains(&ticket.player_pub.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &ticket.player_pub.to_string(),
                    );
                if !is_player {
                    continue;
                }
                let display_id = bs58::encode(&tick_bytes).into_string();
                let cap_id =
                    CapabilityId::derive(cid, CAP_PLAYER, &tick_bytes);
                capabilities.push(Capability {
                    id: cap_id,
                    contract_id: cid,
                    description: format!("Player with ticket {}", display_id),
                    source: CapabilitySource::Role {
                        state: "Active".into(),
                        role: "Player".into(),
                        instance_id: tick_bytes,
                    },
                    consumable: true,
                    expires_at: None,
                });
            }
        }
    }

    // ── Baccarat resolution ─────────────────────────────────────────────

    fn resolve_baccarat(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_baccarat_contract::capability::{CAP_PLAYER_CARDS_DRAWN, CAP_PLAYER_COMMITTED};
        use dwow_baccarat_contract::model::{Bet, BetState};
        use dwow_baccarat_contract::BACCARAT_CONTRACT_BETS_TREE;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(BACCARAT_CONTRACT_BETS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let bet: Bet = match deserialize(&value) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let instance_id = bet.instance_seed;
            let is_player = user_pubkeys.contains(&bet.player_pub.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &bet.player_pub.to_string(),
                );
            if !is_player {
                continue;
            }
            let bet_bytes = bet.id.to_repr();
            let display_id = bs58::encode(&bet_bytes).into_string();
            match bet.state {
                BetState::Committed => {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_PLAYER_COMMITTED, &bet_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Player of baccarat bet {} (Committed)", display_id),
                        source: CapabilitySource::Role {
                            state: "Committed".into(),
                            role: "Player".into(),
                            instance_id: bet_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                }
                BetState::CardsDrawn => {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_PLAYER_CARDS_DRAWN, &bet_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Player of baccarat bet {} (CardsDrawn)", display_id),
                        source: CapabilitySource::Role {
                            state: "CardsDrawn".into(),
                            role: "Player".into(),
                            instance_id: bet_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                }
                _ => {} // Settled or Cancelled — no capabilities
            }
        }
    }

    // ── DarkToshi Dice resolution ───────────────────────────────────────

    fn resolve_darktoshi_dice(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_darktoshi_dice_contract::capability::{CAP_PLAYER_COMMITTED, CAP_PLAYER_REVEALED};
        use dwow_darktoshi_dice_contract::model::{Bet, BetState};
        use dwow_darktoshi_dice_contract::DICE_CONTRACT_BETS_TREE;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(DICE_CONTRACT_BETS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let bet: Bet = match deserialize(&value) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let instance_id = bet.instance_seed;
            let is_player = user_pubkeys.contains(&bet.player_pub.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &bet.player_pub.to_string(),
                );
            if !is_player {
                continue;
            }
            let bet_bytes = bet.id.to_repr();
            let display_id = bs58::encode(&bet_bytes).into_string();
            match bet.state {
                BetState::Committed => {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_PLAYER_COMMITTED, &bet_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Player of dice bet {} (Committed)", display_id),
                        source: CapabilitySource::Role {
                            state: "Committed".into(),
                            role: "Player".into(),
                            instance_id: bet_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                }
                BetState::Revealed => {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_PLAYER_REVEALED, &bet_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Player of dice bet {} (Revealed)", display_id),
                        source: CapabilitySource::Role {
                            state: "Revealed".into(),
                            role: "Player".into(),
                            instance_id: bet_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                }
                _ => {} // Settled or Cancelled — no capabilities
            }
        }
    }

    // ── Game Room resolution ────────────────────────────────────────────

    fn resolve_game_room(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_game_room_contract::capability::CAP_PLAYER;
        use dwow_game_room_contract::model::PlayerAccount;
        use dwow_game_room_contract::GAME_ROOM_ACCOUNTS_TREE;
        use dwow_sdk::crypto::poseidon_hash;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(GAME_ROOM_ACCOUNTS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let account: PlayerAccount = match deserialize(&value) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let instance_id = account.instance_seed;
            let is_player = user_pubkeys.contains(&account.pubkey.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &account.pubkey.to_string(),
                );
            if !is_player {
                continue;
            }
            let account_hash = poseidon_hash([account.pubkey.x(), account.pubkey.y()]);
            let account_bytes = account_hash.to_repr();
            let display_id = bs58::encode(&account_bytes).into_string();
            let cap_id = CapabilityId::derive(cid, CAP_PLAYER, &account_bytes);
            capabilities.push(Capability {
                id: cap_id,
                contract_id: cid,
                description: format!("Player account {}", display_id),
                source: CapabilitySource::Role {
                    state: "Active".into(),
                    role: "Player".into(),
                    instance_id: account_bytes,
                },
                consumable: true,
                expires_at: None,
            });
        }
    }

    // ── Roulette resolution ─────────────────────────────────────────────

    fn resolve_roulette(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_roulette_contract::capability::{CAP_HOUSE, CAP_PLAYER};
        use dwow_roulette_contract::model::{Bet, RouletteTable};
        use dwow_roulette_contract::{ROULETTE_CONTRACT_BETS_TREE, ROULETTE_CONTRACT_TABLES_TREE};
        use dwow_serial::deserialize;

        // Scan tables for house role
        let tree_name = cid.hash_state_id(ROULETTE_CONTRACT_TABLES_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let table: RouletteTable = match deserialize(&value) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                let instance_id = table.instance_seed;
                let is_house = user_pubkeys.contains(&table.house_pub.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &table.house_pub.to_string(),
                    );
                if !is_house {
                    continue;
                }
                let table_bytes = table.table_id.to_repr();
                let display_id = bs58::encode(&table_bytes).into_string();
                let cap_id = CapabilityId::derive(cid, CAP_HOUSE, &table_bytes);
                capabilities.push(Capability {
                    id: cap_id,
                    contract_id: cid,
                    description: format!("House of roulette table {}", display_id),
                    source: CapabilitySource::Role {
                        state: format!("{:?}", table.state),
                        role: "House".into(),
                        instance_id: table_bytes,
                    },
                    consumable: false,
                    expires_at: None,
                });
            }
        }

        // Scan bets for player role
        let tree_name = cid.hash_state_id(ROULETTE_CONTRACT_BETS_TREE);
        if let Ok(tree) = cache.db.open_tree(tree_name) {
            for entry in tree.iter() {
                let (_, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let bet: Bet = match deserialize(&value) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                // Only active bets (not yet settled)
                if bet.won.is_some() {
                    continue;
                }
                let instance_id = bet.instance_seed;
                let is_player = user_pubkeys.contains(&bet.player_pub.to_string())
                    || self.matches_derived_key(
                        user_secrets, &cid, &instance_id, &bet.player_pub.to_string(),
                    );
                if !is_player {
                    continue;
                }
                let bet_bytes = bet.bet_id.to_repr();
                let display_id = bs58::encode(&bet_bytes).into_string();
                let cap_id = CapabilityId::derive(cid, CAP_PLAYER, &bet_bytes);
                capabilities.push(Capability {
                    id: cap_id,
                    contract_id: cid,
                    description: format!("Player with roulette bet {}", display_id),
                    source: CapabilitySource::Role {
                        state: "Active".into(),
                        role: "Player".into(),
                        instance_id: bet_bytes,
                    },
                    consumable: true,
                    expires_at: None,
                });
            }
        }
    }

    // ── Slot resolution ─────────────────────────────────────────────────

    fn resolve_slot(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_slot_contract::capability::{CAP_PLAYER_COMMITTED, CAP_PLAYER_REVEALED};
        use dwow_slot_contract::model::{Spin, SpinState};
        use dwow_slot_contract::SLOT_CONTRACT_SPINS_TREE;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(SLOT_CONTRACT_SPINS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let spin: Spin = match deserialize(&value) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let instance_id = spin.instance_seed;
            let is_player = user_pubkeys.contains(&spin.player_pub.to_string())
                || self.matches_derived_key(
                    user_secrets, &cid, &instance_id, &spin.player_pub.to_string(),
                );
            if !is_player {
                continue;
            }
            let spin_bytes = spin.id.to_repr();
            let display_id = bs58::encode(&spin_bytes).into_string();
            match spin.state {
                SpinState::Committed => {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_PLAYER_COMMITTED, &spin_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Player of slot spin {} (Committed)", display_id),
                        source: CapabilitySource::Role {
                            state: "Committed".into(),
                            role: "Player".into(),
                            instance_id: spin_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                }
                SpinState::Revealed => {
                    let cap_id =
                        CapabilityId::derive(cid, CAP_PLAYER_REVEALED, &spin_bytes);
                    capabilities.push(Capability {
                        id: cap_id,
                        contract_id: cid,
                        description: format!("Player of slot spin {} (Revealed)", display_id),
                        source: CapabilitySource::Role {
                            state: "Revealed".into(),
                            role: "Player".into(),
                            instance_id: spin_bytes,
                        },
                        consumable: true,
                        expires_at: None,
                    });
                }
                _ => {} // Settled or Cancelled — no capabilities
            }
        }
    }

    // ── OTC Swap resolution ──────────────────────────────────────────

    /// Scan the otc_swap sled tree and derive both capabilities and per-instance
    /// actions in a single pass.
    fn resolve_otc_swap(
        &self,
        otc_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_otc_swap_contract::capability::{
            CAP_ALICE_CREATED, CAP_ALICE_FUNDED,
            CAP_BOB_CREATED, CAP_BOB_FUNDED,
        };
        use dwow_otc_swap_contract::model::{OtcSwap, SwapState};
        use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_SWAPS_TREE;
        use dwow_serial::deserialize;

        let tree_name = otc_cid.hash_state_id(OTC_SWAP_CONTRACT_SWAPS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let swap: OtcSwap = match deserialize(&value) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let swap_id_bytes = swap.id.to_repr();
            let alice_pk = swap.alice_pubkey.to_string();
            let bob_pk = swap.bob_pubkey.to_string();
            let instance_id = swap.instance_seed;
            let is_alice = user_pubkeys.contains(&alice_pk)
                || self.matches_derived_key(user_secrets, &otc_cid, &instance_id, &alice_pk);
            let is_bob = user_pubkeys.contains(&bob_pk)
                || self.matches_derived_key(user_secrets, &otc_cid, &instance_id, &bob_pk);

            if !is_alice && !is_bob {
                continue;
            }

            let display_id = bs58::encode(&swap_id_bytes).into_string();

            match swap.state {
                SwapState::Created => {
                    if is_alice {
                        let cap_id = CapabilityId::derive(
                            otc_cid, CAP_ALICE_CREATED, &swap_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: otc_cid,
                            description: format!(
                                "Alice of swap {} (Created)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Created".into(),
                                role: "Alice".into(),
                                instance_id: swap_id_bytes,
                            },
                            consumable: true,
                            expires_at: None,
                        });

                        // Alice can fund or cancel from Created
                        actions.push(Action {
                            function_id: 0x02,
                            name: "FundSwap".into(),
                            contract_id: otc_cid,
                            description: format!(
                                "Fund swap {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_CREATED,
                                    &swap_id_bytes,
                                ),
                            ]),
                            consumes: vec![],
                            produces: vec![
                                CapabilityOutput {
                                    id: CapabilityId::derive(
                                        otc_cid, CAP_ALICE_FUNDED,
                                        &swap_id_bytes,
                                    ),
                                    description: "Alice of funded swap".into(),
                                },
                                CapabilityOutput {
                                    id: CapabilityId::derive(
                                        otc_cid, CAP_BOB_FUNDED,
                                        &swap_id_bytes,
                                    ),
                                    description: "Bob of funded swap".into(),
                                },
                            ],
                        });

                        actions.push(Action {
                            function_id: 0x04,
                            name: "CancelSwap".into(),
                            contract_id: otc_cid,
                            description: format!(
                                "Cancel swap {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_CREATED,
                                    &swap_id_bytes,
                                ),
                            ]),
                            consumes: vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_CREATED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_BOB_CREATED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_FUNDED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_BOB_FUNDED,
                                    &swap_id_bytes,
                                ),
                            ],
                            produces: vec![],
                        });
                    }
                    if is_bob {
                        let cap_id = CapabilityId::derive(
                            otc_cid, CAP_BOB_CREATED, &swap_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: otc_cid,
                            description: format!(
                                "Bob of swap {} (Created)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Created".into(),
                                role: "Bob".into(),
                                instance_id: swap_id_bytes,
                            },
                            consumable: true,
                            expires_at: None,
                        });
                        // Bob cannot act from Created — waits for Alice to fund
                    }
                }
                SwapState::Funded => {
                    if is_alice {
                        let cap_id = CapabilityId::derive(
                            otc_cid, CAP_ALICE_FUNDED, &swap_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: otc_cid,
                            description: format!(
                                "Alice of swap {} (Funded)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Funded".into(),
                                role: "Alice".into(),
                                instance_id: swap_id_bytes,
                            },
                            consumable: true,
                            expires_at: Some(swap.timeout),
                        });

                        actions.push(Action {
                            function_id: 0x04,
                            name: "CancelSwap".into(),
                            contract_id: otc_cid,
                            description: format!(
                                "Cancel swap {} (timeout refund)", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_CREATED,
                                    &swap_id_bytes,
                                ),
                            ]),
                            consumes: vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_CREATED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_BOB_CREATED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_FUNDED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_BOB_FUNDED,
                                    &swap_id_bytes,
                                ),
                            ],
                            produces: vec![],
                        });
                    }
                    if is_bob {
                        let cap_id = CapabilityId::derive(
                            otc_cid, CAP_BOB_FUNDED, &swap_id_bytes,
                        );
                        capabilities.push(Capability {
                            id: cap_id,
                            contract_id: otc_cid,
                            description: format!(
                                "Bob of swap {} (Funded)", display_id,
                            ),
                            source: CapabilitySource::Role {
                                state: "Funded".into(),
                                role: "Bob".into(),
                                instance_id: swap_id_bytes,
                            },
                            consumable: true,
                            expires_at: Some(swap.timeout),
                        });

                        actions.push(Action {
                            function_id: 0x03,
                            name: "ExecuteSwap".into(),
                            contract_id: otc_cid,
                            description: format!(
                                "Execute swap {}", display_id,
                            ),
                            requires: CapabilityExpression::All(vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_BOB_FUNDED,
                                    &swap_id_bytes,
                                ),
                            ]),
                            consumes: vec![
                                CapabilityId::derive(
                                    otc_cid, CAP_ALICE_FUNDED,
                                    &swap_id_bytes,
                                ),
                                CapabilityId::derive(
                                    otc_cid, CAP_BOB_FUNDED,
                                    &swap_id_bytes,
                                ),
                            ],
                            produces: vec![],
                        });
                    }
                }
                _ => {
                    // Executed and Cancelled are terminal — no capabilities or actions
                }
            }
        }
    }

    /// Check whether an on-chain pubkey string matches any wallet's derived instance key.
    ///
    /// For each wallet secret, derives the instance key for (contract_id, instance_id)
    /// and checks if the resulting public key matches the on-chain key.
    fn matches_derived_key(
        &self,
        user_secrets: &[SecretKey],
        contract_id: &ContractId,
        instance_id: &[u8],
        on_chain_pubkey_str: &str,
    ) -> bool {
        user_secrets.iter().any(|secret| {
            let derived_sk = secret.derive_instance(contract_id, instance_id);
            let derived_pk = PublicKey::from_secret(derived_sk);
            derived_pk.to_string() == on_chain_pubkey_str
        })
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
            "promissory_note",
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
        let mut seed = [0u8; 32];
        seed[0..8].copy_from_slice(&id_val.to_le_bytes());
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
            instance_seed: seed,
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
        // Only register promissory_note, NOT escrow — so ESCROW_CONTRACT_ID is unset
        let _ = contract_imports::register_contract_id(
            "promissory_note",
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

    #[test]
    fn test_derived_key_matching_with_instance_seed() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let escrow_cid = *ESCROW_CONTRACT_ID.get().unwrap();
        let instance_seed: [u8; 32] = {
            let mut s = [0u8; 32];
            s[0..8].copy_from_slice(&99u64.to_le_bytes());
            s
        };

        // Create a wallet secret and derive the instance key
        let wallet_sk = SecretKey::from(pallas::Base::from(100));
        let wallet_pk = PublicKey::from_secret(wallet_sk);
        let instance_sk = wallet_sk.derive_instance(&escrow_cid, &instance_seed);
        let instance_pk = PublicKey::from_secret(instance_sk);

        // Store the RAW wallet pubkey, NOT the derived instance key.
        // This forces the resolver to use matches_derived_key path.
        wallet.insert_address(
            &wallet_pk.to_string(),
            &wallet_sk.to_string(),
            true,
            0,
        ).unwrap();

        // Create an escrow with the DERIVED pubkey on-chain
        let other = pk(999);
        let mut escrow = make_escrow(1, instance_pk, other, EscrowState::Created);
        escrow.instance_seed = instance_seed;
        insert_escrow_to_sled(&db, escrow_cid, &escrow);

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Should match via derived key (direct pubkey comparison would fail)
        assert!(
            result.capabilities.iter().any(|c| c.description.contains("Creator")),
            "derived key should match via matches_derived_key"
        );
    }
}
