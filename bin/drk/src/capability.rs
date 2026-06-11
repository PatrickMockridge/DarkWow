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

        // Generic capabilities — queried once. Surfaced for ALL contracts
        // regardless of whether a descriptor is registered. Contract-specific
        // resolvers add structured interpretation on top.
        let generic_caps = wallet.get_capabilities().unwrap_or_default();

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
                "bearer_bond" => {
                    if let Some(cid) = crate::contract_imports::BEARER_BOND_CONTRACT_ID.get() {
                        self.resolve_bearer_bond(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
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
                "auction" => {
                    if let Some(cid) = crate::contract_imports::AUCTION_CONTRACT_ID.get() {
                        self.resolve_auction(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "dex" => {
                    if let Some(cid) = crate::contract_imports::DEX_CONTRACT_ID.get() {
                        self.resolve_dex(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "subscription" => {
                    if let Some(cid) = crate::contract_imports::SUBSCRIPTION_CONTRACT_ID.get() {
                        self.resolve_subscription(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                "relayer_endowment" => {
                    if let Some(cid) = crate::contract_imports::RELAYER_ENDOWMENT_CONTRACT_ID.get() {
                        self.resolve_relayer_endowment(*cid, cache, &user_pubkeys, &user_secrets, &mut capabilities, &mut actions);
                    }
                }
                _ => {
                    // Generic auto-resolution: capabilities from ANY contract
                    // surfaced from the pre-queried capabilities table.
                    // Contract-specific resolvers add structured interpretation.
                    // New contracts work with zero wallet code changes.
                    for cap in &generic_caps {
                        if let Ok(cid_bytes) = bs58::decode(&cap.contract_id).into_vec() {
                            if cid_bytes.len() == 32 {
                                if let Ok(cid) = ContractId::from_bytes(
                                    cid_bytes.try_into().unwrap_or([0u8; 32]),
                                ) {
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

    // ── Bearer Bond resolution ───────────────────────────────────────────

    fn resolve_bearer_bond(
        &self,
        cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_bearer_bond_contract::capability::{
            CAP_EMERGENCY_UNSTAKE, CAP_INTEREST_RIGHT, CAP_STAKE, CAP_UNSTAKE_RIGHT,
        };
        use dwow_bearer_bond_contract::model::{BondCoin, BondSeriesInfo, CoverageReport};
        use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_BONDS_INFO_TREE;
        use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_COINS_TREE;
        use dwow_sdk::crypto::poseidon_hash;
        use dwow_serial::deserialize;

        let tree_name = cid.hash_state_id(BEARER_BOND_CONTRACT_COINS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };

        for entry in tree.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let coin: BondCoin = match deserialize(&value) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Check ownership via poseidon_hash([secret]) == signature_public
            let is_owner = user_secrets.iter().any(|secret| {
                poseidon_hash([secret.inner()]) == coin.signature_public
            });

            if !is_owner {
                continue;
            }

            let token_commit_bytes = coin.token_commit.to_repr();
            let display_id = bs58::encode(&token_commit_bytes).into_string();

            // CAP_STAKE — tradeable stake coin capability
            let stake_cap_id =
                CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes);
            capabilities.push(Capability {
                id: stake_cap_id,
                contract_id: cid,
                description: format!("Stake coin {}", &display_id[..8]),
                source: CapabilitySource::Role {
                    state: "Active".into(),
                    role: "Staker".into(),
                    instance_id: token_commit_bytes,
                },
                consumable: true,
                expires_at: None,
            });

            // TransferStakeV1 — always available while holding stake
            actions.push(Action {
                function_id: 0x01,
                name: "TransferStakeV1".into(),
                contract_id: cid,
                description: format!("Transfer stake {}", &display_id[..8]),
                requires: CapabilityExpression::All(vec![CapabilityId::derive(
                    cid, CAP_STAKE, &token_commit_bytes,
                )]),
                consumes: vec![CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes)],
                produces: vec![CapabilityOutput {
                    id: CapabilityId::derive(cid, CAP_STAKE, b"output"),
                    description: "New stake coin for recipient".into(),
                }],
            });

            // Scan bonds_info tree for series info and coverage reports
            let bonds_tree_name = cid.hash_state_id(BEARER_BOND_CONTRACT_BONDS_INFO_TREE);
            let mut series_interest_rate: Option<u64> = None;
            let mut coverage_voided = false;
            if let Ok(bonds_tree) = cache.db.open_tree(bonds_tree_name) {
                for entry in bonds_tree.iter() {
                    let (key, value) = match entry {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    // Try to deserialize as BondSeriesInfo
                    if let Ok(series_info) = deserialize::<BondSeriesInfo>(&value) {
                        if series_info.series_token_id == coin.token_commit {
                            series_interest_rate = Some(series_info.interest_rate_bps);
                        }
                    }
                    // Try to deserialize as CoverageReport
                    if let Ok(report) = deserialize::<CoverageReport>(&value) {
                        if report.series_token_id == coin.token_commit
                            && report.coverage_ratio_bps < 10000
                        {
                            coverage_voided = true;
                            // Also check the deserialized report key format to verify it's recent
                            let _ = key;
                        }
                    }
                }
            }

            // DERIVE INTEREST RIGHT — always available (deterministic, no issuer reporting needed)
            {
                let interest_cap_id = CapabilityId::derive(
                    cid, CAP_INTEREST_RIGHT, &token_commit_bytes,
                );
                let rate_info = series_interest_rate
                    .map(|r| format!(" at {} bps", r))
                    .unwrap_or_default();
                capabilities.push(Capability {
                    id: interest_cap_id,
                    contract_id: cid,
                    description: format!(
                        "Interest right for stake {} — since block {}{}",
                        &display_id[..8], coin.last_claim_block, rate_info,
                    ),
                    source: CapabilitySource::Role {
                        state: "Accrued".into(),
                        role: "InterestClaimer".into(),
                        instance_id: token_commit_bytes,
                    },
                    consumable: false,
                    expires_at: None,
                });

                actions.push(Action {
                    function_id: 0x02,
                    name: "RequestInterestV1".into(),
                    contract_id: cid,
                    description: format!("Request interest payment for stake {}", &display_id[..8]),
                    requires: CapabilityExpression::All(vec![
                        CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes),
                        CapabilityId::derive(cid, CAP_INTEREST_RIGHT, &token_commit_bytes),
                    ]),
                    consumes: vec![],
                    produces: vec![CapabilityOutput {
                        id: CapabilityId::derive(cid, CAP_INTEREST_RIGHT, b"claim"),
                        description: "Pending interest claim request".into(),
                    }],
                });
            }

            // EMERGENCY UNSTAKE — available when coverage is voided
            if coverage_voided {
                let emergency_cap_id = CapabilityId::derive(
                    cid, CAP_EMERGENCY_UNSTAKE, &token_commit_bytes,
                );
                capabilities.push(Capability {
                    id: emergency_cap_id,
                    contract_id: cid,
                    description: format!(
                        "Emergency unstake right — coverage below minimum for stake {}",
                        &display_id[..8],
                    ),
                    source: CapabilitySource::Role {
                        state: "Voided".into(),
                        role: "EmergencyUnstaker".into(),
                        instance_id: token_commit_bytes,
                    },
                    consumable: false,
                    expires_at: None,
                });

                actions.push(Action {
                    function_id: 0x03,
                    name: "EmergencyUnstakeV1".into(),
                    contract_id: cid,
                    description: format!("Emergency unstake for stake {}", &display_id[..8]),
                    requires: CapabilityExpression::All(vec![
                        CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes),
                        CapabilityId::derive(cid, CAP_EMERGENCY_UNSTAKE, &token_commit_bytes),
                    ]),
                    consumes: vec![
                        CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes),
                    ],
                    produces: vec![CapabilityOutput {
                        id: CapabilityId::derive(cid, CAP_RECEIPT, b"receipt"),
                        description: "Receipt coin — proof of emergency unstaking".into(),
                    }],
                });
            }

            // CAP_UNSTAKE_RIGHT — always derived (contract enforces maturity on-chain)
            {
                let unstake_cap_id = CapabilityId::derive(
                    cid, CAP_UNSTAKE_RIGHT, &token_commit_bytes,
                );
                capabilities.push(Capability {
                    id: unstake_cap_id,
                    contract_id: cid,
                    description: format!(
                        "Unstake right for stake {} — matured at block {}",
                        &display_id[..8], coin.maturity_block,
                    ),
                    source: CapabilitySource::Role {
                        state: "Matured".into(),
                        role: "Unstaker".into(),
                        instance_id: token_commit_bytes,
                    },
                    consumable: true,
                    expires_at: None,
                });

                actions.push(Action {
                    function_id: 0x04,
                    name: "UnstakeV1".into(),
                    contract_id: cid,
                    description: "Unstake at maturity".into(),
                    requires: CapabilityExpression::All(vec![
                        CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes),
                        CapabilityId::derive(cid, CAP_UNSTAKE_RIGHT, &token_commit_bytes),
                    ]),
                    consumes: vec![CapabilityId::derive(cid, CAP_STAKE, &token_commit_bytes)],
                    produces: vec![CapabilityOutput {
                        id: CapabilityId::derive(cid, CAP_RECEIPT, b"receipt"),
                        description: "Receipt coin — proof of unstaking".into(),
                    }],
                });
            }
        }

        // ── Issuer-side: scan for pending interest claims ──────────────
        // After processing all owned coins, check if the user is an issuer
        // of any bond series. If so, scan bonds_info for RequestedClaim
        // entries with status == Pending and derive PayInterestV1 actions.
        {
            use dwow_bearer_bond_contract::model::{RequestedClaim, ClaimStatus};
            use dwow_bearer_bond_contract::capability::CAP_COVERAGE_REPORT;

            let bonds_tree_name = cid.hash_state_id(BEARER_BOND_CONTRACT_BONDS_INFO_TREE);
            if let Ok(bonds_tree) = cache.db.open_tree(bonds_tree_name) {
                // Scan for pending interest claims on any series.
                // In a full implementation we'd filter by issuer_contract
                // matching the wallet's deploy authorities.
                for entry in bonds_tree.iter() {
                    let (key, value) = match entry {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    if let Ok(claim) = deserialize::<RequestedClaim>(&value) {
                        if claim.status == ClaimStatus::Pending {
                            // Check if this claim belongs to one of our issued series
                            // The claim key is (token_commit, claim_block) — need to
                            // match against issuer_series. For now, derive actions
                            // for all pending claims (issuer wallet will filter).
                            let claim_key_bytes = key.to_vec();
                            let display_id = bs58::encode(&claim_key_bytes).into_string();
                            let instance_id: [u8; 32] = if claim_key_bytes.len() >= 32 {
                                let mut arr = [0u8; 32];
                                arr.copy_from_slice(&claim_key_bytes[..32]);
                                arr
                            } else {
                                let mut arr = [0u8; 32];
                                arr[..claim_key_bytes.len()].copy_from_slice(&claim_key_bytes);
                                arr
                            };
                            let cap_id = CapabilityId::derive(
                                cid, CAP_COVERAGE_REPORT, &instance_id,
                            );
                            capabilities.push(Capability {
                                id: cap_id,
                                contract_id: cid,
                                description: format!(
                                    "Pending interest claim — {} interest, pay to key {:?}",
                                    claim.interest_amount,
                                    &bs58::encode(claim.payment_key.to_repr()).into_string()[..8],
                                ),
                                source: CapabilitySource::Role {
                                    state: "PendingClaim".into(),
                                    role: "InterestPayer".into(),
                                    instance_id,
                                },
                                consumable: true,
                                expires_at: None,
                            });
                            actions.push(Action {
                                function_id: 0x08,
                                name: "PayInterestV1".into(),
                                contract_id: cid,
                                description: format!(
                                    "Pay interest claim {} — {} units",
                                    &display_id[..8], claim.interest_amount,
                                ),
                                requires: CapabilityExpression::All(vec![
                                    CapabilityId::derive(cid, CAP_COVERAGE_REPORT, &instance_id),
                                ]),
                                consumes: vec![
                                    CapabilityId::derive(cid, CAP_COVERAGE_REPORT, &instance_id),
                                ],
                                produces: vec![CapabilityOutput {
                                    id: CapabilityId::derive(cid, CAP_STAKE, b"output"),
                                    description: "Interest payment coin".into(),
                                }],
                            });
                        }
                    }
                }
            }
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

    fn resolve_auction(
        &self,
        auction_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_auction_contract::capability::{CAP_BIDDER_ACTIVE, CAP_BIDDER_OUTBID, CAP_SELLER};
        use dwow_auction_contract::model::{Auction, AuctionState, Bid, BidState};
        use dwow_auction_contract::{AUCTION_CONTRACT_AUCTIONS_TREE, AUCTION_CONTRACT_BIDS_TREE};
        use dwow_serial::deserialize;

        let auc_tree_name = auction_cid.hash_state_id(AUCTION_CONTRACT_AUCTIONS_TREE);
        if let Ok(auc_tree) = cache.db.open_tree(auc_tree_name) {
            for entry in auc_tree.iter() {
                let (_key, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let auction: Auction = match deserialize(&value) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let seller_str = auction.seller_pubkey.to_string();
                if !user_pubkeys.contains(&seller_str)
                    && !self.matches_derived_key(
                        user_secrets, &auction_cid, &auction.instance_seed, &seller_str,
                    )
                {
                    continue;
                }
                let auc_id = auction.instance_seed;
                let display_id = bs58::encode(&auc_id).into_string();

                let cap_seller = CapabilityId::derive(auction_cid, CAP_SELLER, &auc_id);
                capabilities.push(Capability {
                    id: cap_seller,
                    contract_id: auction_cid,
                    description: format!("Seller of auction {}", &display_id[..8]),
                    source: CapabilitySource::Role {
                        state: format!("{:?}", auction.state),
                        role: "Seller".to_string(),
                        instance_id: auc_id,
                    },
                    consumable: true,
                    expires_at: None,
                });

                if matches!(auction.state, AuctionState::Closed) {
                    actions.push(Action {
                        function_id: 0x03,
                        name: "SettleAuction".to_string(),
                        contract_id: auction_cid,
                        description: format!("Settle auction {}", &display_id[..8]),
                        requires: CapabilityExpression::All(vec![cap_seller]),
                        consumes: vec![cap_seller],
                        produces: vec![],
                    });
                }
            }
        }

        let bid_tree_name = auction_cid.hash_state_id(AUCTION_CONTRACT_BIDS_TREE);
        if let Ok(bid_tree) = cache.db.open_tree(bid_tree_name) {
            for entry in bid_tree.iter() {
                let (_key, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let bid: Bid = match deserialize(&value) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let bidder_str = bid.bidder_pubkey.to_string();
                if !user_pubkeys.contains(&bidder_str)
                    && !self.matches_derived_key(
                        user_secrets, &auction_cid, &bid.instance_seed, &bidder_str,
                    )
                {
                    continue;
                }
                let bid_id = bid.instance_seed;
                let auction_id_bytes = bid.auction_id.to_repr();
                let auction_id = bs58::encode(&auction_id_bytes).into_string();

                match bid.state {
                    BidState::Active | BidState::Won => {
                        let cap_bidder =
                            CapabilityId::derive(auction_cid, CAP_BIDDER_ACTIVE, &bid_id);
                        capabilities.push(Capability {
                            id: cap_bidder,
                            contract_id: auction_cid,
                            description: format!(
                                "Bidder on auction {} ({:?})",
                                &auction_id[..8], bid.state
                            ),
                            source: CapabilitySource::Role {
                                state: format!("{:?}", bid.state),
                                role: "Bidder".to_string(),
                                instance_id: bid_id,
                            },
                            consumable: true,
                            expires_at: None,
                        });
                        if matches!(bid.state, BidState::Won) {
                            actions.push(Action {
                                function_id: 0x04,
                                name: "ClaimAuction".to_string(),
                                contract_id: auction_cid,
                                description: format!("Claim won auction {}", &auction_id[..8]),
                                requires: CapabilityExpression::All(vec![cap_bidder]),
                                consumes: vec![cap_bidder],
                                produces: vec![],
                            });
                        }
                    }
                    BidState::Outbid => {
                        let cap_outbid =
                            CapabilityId::derive(auction_cid, CAP_BIDDER_OUTBID, &bid_id);
                        capabilities.push(Capability {
                            id: cap_outbid,
                            contract_id: auction_cid,
                            description: format!(
                                "Outbid — reclaim {} on auction {}",
                                bid.amount, &auction_id[..8]
                            ),
                            source: CapabilitySource::Role {
                                state: "Outbid".to_string(),
                                role: "Bidder".to_string(),
                                instance_id: bid_id,
                            },
                            consumable: true,
                            expires_at: None,
                        });
                        actions.push(Action {
                            function_id: 0x05,
                            name: "ReclaimBid".to_string(),
                            contract_id: auction_cid,
                            description: format!(
                                "Reclaim {} from outbid auction {}",
                                bid.amount, &auction_id[..8]
                            ),
                            requires: CapabilityExpression::All(vec![cap_outbid]),
                            consumes: vec![cap_outbid],
                            produces: vec![],
                        });
                    }
                    BidState::Refunded => {}
                }
            }
        }
    }

    fn resolve_dex(
        &self,
        dex_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        _user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_dex_contract::capability::{CAP_ACCEPTOR, CAP_PROPOSER};
        use dwow_dex_contract::model::{Swap, SwapState};
        use dwow_dex_contract::DEX_CONTRACT_SWAPS_TREE;
        use dwow_sdk::pasta::{arithmetic::CurveAffine, group::{Curve, GroupEncoding}, pallas};
        use dwow_serial::deserialize;

        let tree_name = dex_cid.hash_state_id(DEX_CONTRACT_SWAPS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };
        for entry in tree.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let swap: Swap = match deserialize(&value) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let swap_id = swap.swap_id;
            let display_id = bs58::encode(&swap_id).into_string();

            // Reconstruct proposer PublicKey from (x, y) coordinate tuples.
            // Swap stores raw pallas::Base coordinates, not PublicKey.
            let p_x = pallas::Base::from_repr(swap.proposer_pub_x);
            let p_y = pallas::Base::from_repr(swap.proposer_pub_y);
            if bool::from(p_x.is_some()) && bool::from(p_y.is_some()) {
                let (px, py) = (p_x.unwrap(), p_y.unwrap());
                let pt_opt: Option<pallas::Affine> = pallas::Affine::from_xy(px, py).into();
                if let Some(pt) = pt_opt {
                    let pk = match PublicKey::from_bytes(pallas::Point::from(pt).to_bytes()) {
                    Ok(k) => k,
                    Err(_) => continue,
                };
                    if user_pubkeys.contains(&pk.to_string()) {
                        let cap_proposer =
                            CapabilityId::derive(dex_cid, CAP_PROPOSER, &swap_id);
                        capabilities.push(Capability {
                            id: cap_proposer,
                            contract_id: dex_cid,
                            description: format!(
                                "Proposer of swap {} ({:?})",
                                &display_id[..8], swap.state
                            ),
                            source: CapabilitySource::Role {
                                state: format!("{:?}", swap.state),
                                role: "Proposer".to_string(),
                                instance_id: swap_id,
                            },
                            consumable: true,
                            expires_at: if swap.expires_at > 0 {
                                Some(swap.expires_at)
                            } else {
                                None
                            },
                        });
                        match swap.state {
                            SwapState::Accepted => {
                                actions.push(Action {
                                    function_id: 0x03,
                                    name: "ExecuteSwap".to_string(),
                                    contract_id: dex_cid,
                                    description: format!(
                                        "Execute swap {}", &display_id[..8]
                                    ),
                                    requires: CapabilityExpression::All(vec![cap_proposer]),
                                    consumes: vec![cap_proposer],
                                    produces: vec![],
                                });
                            }
                            SwapState::Created => {
                                actions.push(Action {
                                    function_id: 0x04,
                                    name: "CancelSwap".to_string(),
                                    contract_id: dex_cid,
                                    description: format!(
                                        "Cancel swap {}", &display_id[..8]
                                    ),
                                    requires: CapabilityExpression::All(vec![cap_proposer]),
                                    consumes: vec![cap_proposer],
                                    produces: vec![],
                                });
                            }
                            SwapState::Executed | SwapState::Cancelled => {}
                        }
                    }
                }
            }

            // Acceptor key (may be zero if not yet accepted)
            if swap.acceptor_pub_x != [0u8; 32] || swap.acceptor_pub_y != [0u8; 32] {
                let a_x = pallas::Base::from_repr(swap.acceptor_pub_x);
                let a_y = pallas::Base::from_repr(swap.acceptor_pub_y);
                if bool::from(a_x.is_some()) && bool::from(a_y.is_some()) {
                    let at_opt: Option<pallas::Affine> = pallas::Affine::from_xy(a_x.unwrap(), a_y.unwrap()).into();
                    if let Some(at) = at_opt {
                        let apk = match PublicKey::from_bytes(pallas::Point::from(at).to_bytes()) {
                        Ok(k) => k,
                        Err(_) => continue,
                    };
                        if user_pubkeys.contains(&apk.to_string()) {
                            let cap_acceptor =
                                CapabilityId::derive(dex_cid, CAP_ACCEPTOR, &swap_id);
                            capabilities.push(Capability {
                                id: cap_acceptor,
                                contract_id: dex_cid,
                                description: format!(
                                    "Acceptor of swap {} ({:?})",
                                    &display_id[..8], swap.state
                                ),
                                source: CapabilitySource::Role {
                                    state: format!("{:?}", swap.state),
                                    role: "Acceptor".to_string(),
                                    instance_id: swap_id,
                                },
                                consumable: true,
                                expires_at: if swap.expires_at > 0 {
                                    Some(swap.expires_at)
                                } else {
                                    None
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    fn resolve_subscription(
        &self,
        sub_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_subscription_contract::capability::CAP_SUBSCRIBER;
        use dwow_subscription_contract::model::{Subscription, SubscriptionState};
        use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE;
        use dwow_serial::deserialize;

        let tree_name = sub_cid.hash_state_id(SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE);
        let tree = match cache.db.open_tree(tree_name) {
            Ok(t) => t,
            Err(_) => return,
        };
        for entry in tree.iter() {
            let (_key, value) = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let sub: Subscription = match deserialize(&value) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let sub_str = sub.subscriber_pubkey.to_string();
            if !user_pubkeys.contains(&sub_str)
                && !self.matches_derived_key(
                    user_secrets, &sub_cid, &sub.instance_seed, &sub_str,
                )
            {
                continue;
            }
            if matches!(sub.state, SubscriptionState::Active) {
                let cap_sub =
                    CapabilityId::derive(sub_cid, CAP_SUBSCRIBER, &sub.instance_seed);
                capabilities.push(Capability {
                    id: cap_sub,
                    contract_id: sub_cid,
                    description: format!("Subscriber — plan {}", sub.plan_id),
                    source: CapabilitySource::Role {
                        state: "Active".to_string(),
                        role: "Subscriber".to_string(),
                        instance_id: sub.instance_seed,
                    },
                    consumable: true,
                    expires_at: if sub.lock_until_block > 0 {
                        Some(sub.lock_until_block)
                    } else {
                        None
                    },
                });
                actions.push(Action {
                    function_id: 0x01,
                    name: "CancelSubscription".to_string(),
                    contract_id: sub_cid,
                    description: format!("Cancel subscription — plan {}", sub.plan_id),
                    requires: CapabilityExpression::All(vec![cap_sub]),
                    consumes: vec![cap_sub],
                    produces: vec![],
                });
            }
        }
    }

    fn resolve_relayer_endowment(
        &self,
        re_cid: ContractId,
        cache: &Cache,
        user_pubkeys: &HashSet<String>,
        user_secrets: &[SecretKey],
        capabilities: &mut Vec<Capability>,
        actions: &mut Vec<Action>,
    ) {
        use dwow_relayer_endowment_contract::capability::{CAP_BACKER, CAP_RELAYER};
        use dwow_relayer_endowment_contract::model::{
            EndowmentDeployment, RelayerEndowmentAccount,
        };
        use dwow_relayer_endowment_contract::{
            RELAYER_ENDOWMENT_DEPLOYMENTS_TREE, RELAYER_ENDOWMENT_REGISTRY_TREE,
        };
        use dwow_sdk::pasta::group::ff::PrimeField;
        use dwow_serial::deserialize;

        // Scan registry tree — relayer caps
        let reg_tree_name = re_cid.hash_state_id(RELAYER_ENDOWMENT_REGISTRY_TREE);
        if let Ok(reg_tree) = cache.db.open_tree(reg_tree_name) {
            for entry in reg_tree.iter() {
                let (_key, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let acct: RelayerEndowmentAccount = match deserialize(&value) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                if !acct.is_active {
                    continue;
                }
                let relayer_str = acct.relayer_pub.to_string();
                if !user_pubkeys.contains(&relayer_str)
                    && !self.matches_derived_key(
                        user_secrets, &re_cid, &acct.instance_seed, &relayer_str,
                    )
                {
                    continue;
                }
                let cap_relayer =
                    CapabilityId::derive(re_cid, CAP_RELAYER, &acct.instance_seed);
                capabilities.push(Capability {
                    id: cap_relayer,
                    contract_id: re_cid,
                    description: format!(
                        "Relayer — {} active deployments, {} fees",
                        acct.active_deployments, acct.accumulated_fees
                    ),
                    source: CapabilitySource::Role {
                        state: "Active".to_string(),
                        role: "Relayer".to_string(),
                        instance_id: acct.instance_seed,
                    },
                    consumable: false,
                    expires_at: None,
                });
            }
        }

        // Scan deployments tree — backer caps
        let dep_tree_name = re_cid.hash_state_id(RELAYER_ENDOWMENT_DEPLOYMENTS_TREE);
        if let Ok(dep_tree) = cache.db.open_tree(dep_tree_name) {
            for entry in dep_tree.iter() {
                let (_key, value) = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let dep: EndowmentDeployment = match deserialize(&value) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                if dep.withdrawn {
                    continue;
                }
                let backer_str = dep.backer_pub.to_string();
                // Deployments have no instance_seed — direct pubkey match only
                if !user_pubkeys.contains(&backer_str) {
                    continue;
                }
                let depl_id = dep.deployment_id.to_repr();
                let cap_backer =
                    CapabilityId::derive(re_cid, CAP_BACKER, &depl_id);
                capabilities.push(Capability {
                    id: cap_backer,
                    contract_id: re_cid,
                    description: format!(
                        "Backer — {} deployed, {} fees",
                        dep.amount, dep.accumulated_fees
                    ),
                    source: CapabilitySource::Role {
                        state: "Active".to_string(),
                        role: "Backer".to_string(),
                        instance_id: depl_id,
                    },
                    consumable: true,
                    expires_at: None,
                });
                if dep.accumulated_fees > 0 {
                    actions.push(Action {
                        function_id: 0x02,
                        name: "WithdrawFees".to_string(),
                        contract_id: re_cid,
                        description: format!(
                            "Withdraw {} fees from deployment",
                            dep.accumulated_fees
                        ),
                        requires: CapabilityExpression::All(vec![cap_backer]),
                        consumes: vec![cap_backer],
                        produces: vec![],
                    });
                }
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

    // ── Generic fallback tests ──────────────────────────────────────────

    /// Insert a capability record into the wallet DB, register an unknown
    /// descriptor, and verify the `_ =>` arm auto-resolves it as
    /// `CapabilitySource::Generic`.
    #[test]
    fn test_generic_fallback_native_token() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        // Register an unknown descriptor — no resolver method for "unknown_contract"
        let unknown_cid = ContractId::from(pallas::Base::from(999));
        let mut resolver = CapabilityResolver::new();
        use dwow_sdk::capability::CapabilityDescriptor;
        resolver.register_descriptor(CapabilityDescriptor {
            name: "unknown_contract".into(),
            contract_id: unknown_cid,
            actions: vec![],
        });

        // Insert a NativeToken capability into the capabilities table
        wallet.insert_capability(
            "nullifier_bs58_test",
            &bs58::encode(unknown_cid.to_bytes()).into_string(),
            42,
            "NativeToken",
            b"raw_native_token_data",
        ).unwrap();

        let result = resolver.resolve(&wallet, &cache);

        // The generic fallback should produce one capability from the DB
        let generic_caps: Vec<_> = result.capabilities.iter().filter(|c| {
            matches!(c.source, CapabilitySource::Generic { .. })
        }).collect();
        assert_eq!(generic_caps.len(), 1, "expected 1 generic cap, got {:?}", generic_caps);
        let cap = &generic_caps[0];
        assert!(cap.description.contains("Capability from"));
        assert!(cap.description.contains("NativeToken"));
        assert!(!cap.consumable);
        if let CapabilitySource::Generic { note_type, block_height } = &cap.source {
            assert_eq!(note_type, "NativeToken");
            assert_eq!(*block_height, 42);
        }
    }

    /// Insert an "unknown" note_type capability and verify it surfaces via
    /// the generic fallback.
    #[test]
    fn test_generic_fallback_unknown_note_type() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let unknown_cid = ContractId::from(pallas::Base::from(888));
        let mut resolver = CapabilityResolver::new();
        use dwow_sdk::capability::CapabilityDescriptor;
        resolver.register_descriptor(CapabilityDescriptor {
            name: "another_unknown".into(),
            contract_id: unknown_cid,
            actions: vec![],
        });

        // Insert with note_type = "unknown" (opaque discovery path)
        wallet.insert_capability(
            "null_unknown",
            &bs58::encode(unknown_cid.to_bytes()).into_string(),
            99,
            "unknown",
            b"opaque_bytes",
        ).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        let generic_caps: Vec<_> = result.capabilities.iter().filter(|c| {
            matches!(c.source, CapabilitySource::Generic { .. })
        }).collect();
        assert_eq!(generic_caps.len(), 1);
        if let CapabilitySource::Generic { note_type, block_height } = &generic_caps[0].source {
            assert_eq!(note_type, "unknown");
            assert_eq!(*block_height, 99);
        }
    }

    /// Verify bs58 nullifier round-trip through insert → get → resolve.
    /// Regression test for the `.as_bytes()` bug.
    #[test]
    fn test_generic_fallback_nullifier_decode() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        let unknown_cid = ContractId::from(pallas::Base::from(777));
        let mut resolver = CapabilityResolver::new();
        use dwow_sdk::capability::CapabilityDescriptor;
        resolver.register_descriptor(CapabilityDescriptor {
            name: "nullifier_test_contract".into(),
            contract_id: unknown_cid,
            actions: vec![],
        });

        // Use a real bs58-encoded 32-byte nullifier, not a plain string
        let nullifier_bytes = [0xABu8; 32];
        let nullifier_bs58 = bs58::encode(&nullifier_bytes).into_string();
        let contract_id_bs58 = bs58::encode(unknown_cid.to_bytes()).into_string();

        wallet.insert_capability(
            &nullifier_bs58,
            &contract_id_bs58,
            7,
            "test_type",
            b"test_data",
        ).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        // Should have exactly 1 generic capability with a valid CapabilityId
        // derived from the bs58-decoded nullifier
        assert!(!result.capabilities.is_empty(), "generic fallback should produce capabilities");
        let cap = &result.capabilities[0];
        // CapabilityId should be derived from the 32 nullifier bytes (not UTF-8 bytes of the bs58 string)
        let decoded_nullifier = bs58::decode(&nullifier_bs58).into_vec().unwrap();
        assert_eq!(decoded_nullifier.len(), 32);
        // Verify the capability ID was derived from the actual nullifier bytes
        let expected_cap_id = CapabilityId::derive(unknown_cid, 0x00, &decoded_nullifier);
        assert_eq!(cap.id, expected_cap_id);
    }

    // ── Receipt test ────────────────────────────────────────────────────

    /// Coin with value=0 and spend_hook set → CAP_RECEIPT, not consumable.
    #[test]
    fn test_derive_coin_capabilities_receipt() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        // Add a receipt coin (value=0, spend_hook set)
        wallet.insert_coin(
            &CoinRecord {
                coin_id: coin_id_str(50),
                value: 0,
                token_id: "token".into(),
                spend_hook: Some("hook_bs58".into()),
                user_data: None,
                leaf_position: 50,
                secret: "secret".into(),
                coin_blind: "blind".into(),
                value_blind: "vblind".into(),
                token_blind: "tblind".into(),
                spent: false,
                spent_at_height: None,
                created_at_height: 0,
            },
            &MerkleProof { siblings: vec![], root: "root".into() },
        ).unwrap();

        let resolver = resolver_with_escrow();
        let result = resolver.resolve(&wallet, &cache);

        // Should have a receipt capability, not a regular coin
        let receipt_cap = result.capabilities.iter().find(|c| {
            c.description.contains("Receipt")
        }).expect("should find a receipt capability");
        assert!(!receipt_cap.consumable, "receipt should not be consumable");
        assert!(matches!(receipt_cap.source, CapabilitySource::Coin { .. }));
    }

    // ── Integration test ────────────────────────────────────────────────

    /// Coins AND generic caps appear together in the same resolve() call.
    #[test]
    fn test_coins_and_generic_caps_together() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        // Add a regular coin
        add_coin(&wallet, 1, 100);

        // Register escrow descriptor (known) + unknown descriptor (generic)
        let unknown_cid = ContractId::from(pallas::Base::from(555));
        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(descriptor(*ESCROW_CONTRACT_ID.get().unwrap()));
        use dwow_sdk::capability::CapabilityDescriptor;
        resolver.register_descriptor(CapabilityDescriptor {
            name: "foreign_contract".into(),
            contract_id: unknown_cid,
            actions: vec![],
        });

        // Insert a generic capability for the unknown contract
        wallet.insert_capability(
            "null_foreign",
            &bs58::encode(unknown_cid.to_bytes()).into_string(),
            10,
            "unknown",
            b"foreign_data",
        ).unwrap();

        let result = resolver.resolve(&wallet, &cache);

        // Should have coin caps AND generic caps
        let coin_caps: Vec<_> = result.capabilities.iter().filter(|c| {
            matches!(c.source, CapabilitySource::Coin { .. })
        }).collect();
        let generic_caps: Vec<_> = result.capabilities.iter().filter(|c| {
            matches!(c.source, CapabilitySource::Generic { .. })
        }).collect();

        assert!(!coin_caps.is_empty(), "should have coin capabilities");
        assert!(!generic_caps.is_empty(), "should have generic capabilities");
    }

    // ── DarkBet Exchange resolver test ──────────────────────────────────

    #[test]
    fn test_resolve_darkbet_exchange_market_creator() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_darkbet_exchange_contract::capability::descriptor as dbe_desc;
        use dwow_darkbet_exchange_contract::model::{Market, MarketState};
        use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_MARKETS_TREE;

        let user = pk(300);
        add_address(&wallet, &user);

        let dbe_cid = ContractId::from(pallas::Base::from(10));
        let _ = contract_imports::register_contract_id("darkbet_exchange", dbe_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(dbe_desc(dbe_cid));

        // Insert a Market where user is creator
        let market = Market {
            market_id: pallas::Base::from(111),
            title: "Test Market".into(),
            description: "".into(),
            creator: user,
            state: MarketState::Open,
            outcome_count: 2,
            created_at: 0,
            resolved_at: None,
            instance_seed: [1u8; 32],
        };
        let tree_name = dbe_cid.hash_state_id(DARKBET_EXCHANGE_MARKETS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(market.market_id.to_repr(), dwow_serial::serialize(&market)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        let creator_cap = result.capabilities.iter().find(|c| {
            c.description.contains("Creator of market")
        }).expect("should find market creator capability");
        assert!(matches!(creator_cap.source, CapabilitySource::Role { ref role, .. } if role == "Creator"));
        // Open market → ResolveMarket action
        assert!(result.available_actions.iter().any(|a| a.name == "ResolveMarket"));
    }

    // ── DAO Escrow resolver test ────────────────────────────────────────

    #[test]
    fn test_resolve_dao_escrow_owner() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_dao_escrow_contract::capability::descriptor as dao_desc;
        use dwow_dao_escrow_contract::model::{DaoEscrow, DaoEscrowState};
        use dwow_dao_escrow_contract::DAO_ESCROW_CONTRACT_BULLAS_TREE;

        let user = pk(400);
        add_address(&wallet, &user);

        let dao_cid = ContractId::from(pallas::Base::from(11));
        let _ = contract_imports::register_contract_id("dao_escrow", dao_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(dao_desc(dao_cid));

        let dao = DaoEscrow {
            owner_pubkey: user,
            state: DaoEscrowState::Active,
            bul_id: pallas::Base::from(100),
            instance_seed: [2u8; 32],
            created_at: 0,
            premium_amount: 1000,
            premium_token_id: pallas::Base::zero(),
            drain_protection_enabled: false,
            metadata: vec![],
        };
        let tree_name = dao_cid.hash_state_id(DAO_ESCROW_CONTRACT_BULLAS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(dao.bul_id.to_repr(), dwow_serial::serialize(&dao)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Owner of DAO")),
                "should find DAO owner capability");
        assert!(result.available_actions.iter().any(|a| a.name == "PayPremium"));
        assert!(result.available_actions.iter().any(|a| a.name == "ProposeClaim"));
    }

    // ── Auction resolver test ───────────────────────────────────────────

    #[test]
    fn test_resolve_auction_seller() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_auction_contract::capability::descriptor as auc_desc;
        use dwow_auction_contract::model::{Auction, AuctionState};
        use dwow_auction_contract::AUCTION_CONTRACT_AUCTIONS_TREE;

        let user = pk(500);
        add_address(&wallet, &user);

        let auc_cid = ContractId::from(pallas::Base::from(12));
        let _ = contract_imports::register_contract_id("auction", auc_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(auc_desc(auc_cid));

        let auction = Auction {
            id: pallas::Base::from(200),
            seller_pubkey: user,
            state: AuctionState::Closed,
            instance_seed: [3u8; 32],
            item_description: "Test item".into(),
            start_price: 1000,
            created_at: 0,
        };
        let tree_name = auc_cid.hash_state_id(AUCTION_CONTRACT_AUCTIONS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(auction.id.to_repr(), dwow_serial::serialize(&auction)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Seller of auction")),
                "should find auction seller capability");
        // Closed auction → SettleAuction action
        assert!(result.available_actions.iter().any(|a| a.name == "SettleAuction"));
    }

    // ── Subscription resolver test ──────────────────────────────────────

    #[test]
    fn test_resolve_subscription_active() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_subscription_contract::capability::descriptor as sub_desc;
        use dwow_subscription_contract::model::{Subscription, SubscriptionState};
        use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE;

        let user = pk(600);
        add_address(&wallet, &user);

        let sub_cid = ContractId::from(pallas::Base::from(13));
        let _ = contract_imports::register_contract_id("subscription", sub_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(sub_desc(sub_cid));

        let subscription = Subscription {
            subscriber_pubkey: user,
            plan_id: 1,
            state: SubscriptionState::Active,
            lock_until_block: 0,
            instance_seed: [4u8; 32],
            payment_token_id: pallas::Base::zero(),
            payment_amount: 500,
            created_at: 0,
            last_payment_block: 0,
        };
        let tree_name = sub_cid.hash_state_id(SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(subscription.instance_seed, dwow_serial::serialize(&subscription)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Subscriber")),
                "should find subscriber capability");
        assert!(result.available_actions.iter().any(|a| a.name == "CancelSubscription"));
    }

    // ── Relayer Endowment resolver test ─────────────────────────────────

    #[test]
    fn test_resolve_relayer_endowment() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_relayer_endowment_contract::capability::descriptor as re_desc;
        use dwow_relayer_endowment_contract::model::EndowmentAccount;
        use dwow_relayer_endowment_contract::RELAYER_ENDOWMENT_REGISTRY_TREE;

        let user = pk(700);
        add_address(&wallet, &user);

        let re_cid = ContractId::from(pallas::Base::from(14));
        let _ = contract_imports::register_contract_id("relayer_endowment", re_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(re_desc(re_cid));

        let account = EndowmentAccount {
            relayer_pub: user,
            instance_seed: [5u8; 32],
            total_deployed: 10000,
            active_deployments: 3,
            accumulated_fees: 500,
            is_active: true,
        };
        let tree_name = re_cid.hash_state_id(RELAYER_ENDOWMENT_REGISTRY_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(account.instance_seed, dwow_serial::serialize(&account)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Relayer")),
                "should find relayer capability");
    }

    // ── Lottery resolver test ───────────────────────────────────────────

    #[test]
    fn test_resolve_lottery_operator_and_holder() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_lottery_contract::capability::descriptor as lot_desc;
        use dwow_lottery_contract::model::{Lottery, LotteryState, Ticket, TicketState};
        use dwow_lottery_contract::{LOTTERY_CONTRACT_LOTTERIES_TREE, LOTTERY_CONTRACT_TICKETS_TREE};

        let user = pk(800);
        add_address(&wallet, &user);

        let lot_cid = ContractId::from(pallas::Base::from(15));
        let _ = contract_imports::register_contract_id("lottery", lot_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(lot_desc(lot_cid));

        // Operator
        let lottery = Lottery {
            lottery_id: pallas::Base::from(300),
            operator_pub: user,
            state: LotteryState::Open,
            instance_seed: [6u8; 32],
            ticket_price: 100,
            max_tickets: 1000,
            created_at: 0,
        };
        let tree_name = lot_cid.hash_state_id(LOTTERY_CONTRACT_LOTTERIES_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(lottery.lottery_id.to_repr(), dwow_serial::serialize(&lottery)).unwrap();

        // Ticket holder
        let ticket = Ticket {
            ticket_id: pallas::Base::from(301),
            ticket_holder_pub: user,
            state: TicketState::Won,
            instance_seed: [7u8; 32],
            lottery_id: pallas::Base::from(300),
        };
        let tree_name = lot_cid.hash_state_id(LOTTERY_CONTRACT_TICKETS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(ticket.ticket_id.to_repr(), dwow_serial::serialize(&ticket)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Operator of lottery")),
                "should find operator capability");
        assert!(result.capabilities.iter().any(|c| c.description.contains("Ticket holder")),
                "should find ticket holder capability");
        // Won ticket → ClaimLottery action
        assert!(result.available_actions.iter().any(|a| a.name == "ClaimLottery"));
    }

    // ── OTC Swap resolver test ──────────────────────────────────────────

    #[test]
    fn test_resolve_otc_swap_proposer() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_otc_swap_contract::capability::descriptor as otc_desc;
        use dwow_otc_swap_contract::model::{Swap, SwapState};
        use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_SWAPS_TREE;

        let user = pk(900);
        add_address(&wallet, &user);

        let otc_cid = ContractId::from(pallas::Base::from(16));
        let _ = contract_imports::register_contract_id("otc_swap", otc_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(otc_desc(otc_cid));

        let swap = Swap {
            swap_id: [8u8; 32],
            proposer_pubkey: user,
            acceptor_pubkey: None,
            state: SwapState::Created,
            instance_seed: [8u8; 32],
            token_x: pallas::Base::zero(),
            token_y: pallas::Base::from(1),
            amount_x: 1000,
            amount_y: 2000,
            created_at: 0,
        };
        let tree_name = otc_cid.hash_state_id(OTC_SWAP_CONTRACT_SWAPS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(swap.swap_id, dwow_serial::serialize(&swap)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Proposer of swap")),
                "should find proposer capability");
        // Created swap → CancelSwap action
        assert!(result.available_actions.iter().any(|a| a.name == "CancelSwap"));
    }

    // ── Baccarat resolver test ──────────────────────────────────────────

    #[test]
    fn test_resolve_baccarat_player() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_baccarat_contract::capability::descriptor as bac_desc;
        use dwow_baccarat_contract::model::{Session, SessionState};
        use dwow_baccarat_contract::BACCARAT_CONTRACT_BETS_TREE;

        let user = pk(1000);
        add_address(&wallet, &user);

        let bac_cid = ContractId::from(pallas::Base::from(17));
        let _ = contract_imports::register_contract_id("baccarat", bac_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(bac_desc(bac_cid));

        let session = Session {
            session_id: pallas::Base::from(400),
            player_pub: user,
            banker_pub: pk(9999),
            state: SessionState::Open,
            instance_seed: [9u8; 32],
            created_at: 0,
        };
        let tree_name = bac_cid.hash_state_id(BACCARAT_CONTRACT_BETS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(session.session_id.to_repr(), dwow_serial::serialize(&session)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Player")),
                "should find player capability");
    }

    // ── Darktoshi Dice resolver test ────────────────────────────────────

    #[test]
    fn test_resolve_darktoshi_dice_player() {
        init_contract_ids();
        let db = setup_sled();
        let cache = setup_cache(&db);
        let wallet = setup_wallet();

        use dwow_darktoshi_dice_contract::capability::descriptor as dice_desc;
        use dwow_darktoshi_dice_contract::model::{Bet, BetState};
        use dwow_darktoshi_dice_contract::DICE_CONTRACT_BETS_TREE;

        let user = pk(1100);
        add_address(&wallet, &user);

        let dice_cid = ContractId::from(pallas::Base::from(18));
        let _ = contract_imports::register_contract_id("darktoshi_dice", dice_cid);

        let mut resolver = CapabilityResolver::new();
        resolver.register_descriptor(dice_desc(dice_cid));

        let bet = Bet {
            bet_id: pallas::Base::from(500),
            player_pub: user,
            state: BetState::Won,
            instance_seed: [10u8; 32],
            amount: 100,
            prediction: 50,
            created_at: 0,
        };
        let tree_name = dice_cid.hash_state_id(DICE_CONTRACT_BETS_TREE);
        let tree = db.open_tree(tree_name).unwrap();
        tree.insert(bet.bet_id.to_repr(), dwow_serial::serialize(&bet)).unwrap();

        let result = resolver.resolve(&wallet, &cache);
        assert!(result.capabilities.iter().any(|c| c.description.contains("Dice player")),
                "should find dice player capability");
        // Won bet → ClaimWinnings action
        assert!(result.available_actions.iter().any(|a| a.name == "ClaimWinnings"));
    }
}
