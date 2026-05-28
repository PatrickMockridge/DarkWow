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

//! InitializeV1 Client API
//!
//! This module handles the creation of a MoneyV3 token for the stablecoin.
//! When initializing, stablecoin creates its token type (e.g., "USDx") in MoneyV3.

use dwow_sdk::{
    crypto::{poseidon_hash, BaseBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;

use crate::model::{InitializeParams, StablecoinModel};

/// Debris produced by building an Initialize call
pub struct InitializeCallDebris {
    /// The contract call parameters
    pub params: InitializeParams,
    /// MoneyV3 token mint debris (if token creation requested)
    pub token_mint_debris: Option<TokenMintDebris>,
}

/// Debris for creating a MoneyV3 token
pub struct TokenMintDebris {
    /// Token ID (Poseidon hash of auth_parent, user_data, blind)
    pub token_id: pallas::Base,
    /// Initial coin commitment
    pub coin: pallas::Base,
    /// Value commitment
    pub value_commit: pallas::Base,
    /// Token commitment
    pub token_commit: pallas::Base,
}

/// Builder for InitializeV1 - creates stablecoin and MoneyV3 token
pub struct InitializeCallBuilder {
    /// Stablecoin model (PooledDebt, Liquity, etc.)
    pub model: StablecoinModel,
    /// Minimum collateralization ratio (basis points, e.g., 15000 = 150%)
    pub min_collateralization_ratio: u64,
    /// Liquidation threshold (basis points)
    pub liquidation_threshold: u64,
    /// Liquidation penalty (basis points)
    pub liquidation_penalty: u64,
    /// Base stability fee (basis points annually)
    pub base_rate: u64,
    /// PI controller Kp
    pub pi_kp: i64,
    /// PI controller Ki
    pub pi_ki: i64,
    /// TWAP window in seconds
    pub twap_window: u64,
    /// Price deviation threshold (basis points)
    pub price_deviation_threshold: u64,
    /// Token authority public key (for MintV1 backing proof)
    pub token_authority_pub: [u8; 32],
    /// Whether to create a MoneyV3 token
    pub create_token: bool,
    /// Token symbol (e.g., "USDx")
    pub token_symbol: Option<String>,
    /// Initial supply to mint (if create_token)
    pub initial_supply: Option<u64>,
}

impl InitializeCallBuilder {
    /// Build the Initialize call debris
    pub fn build(&self) -> InitializeCallDebris {
        // Generate token mint debris if requested
        let token_mint_debris = if self.create_token {
            // Use random auth parent to prevent token_id from carrying
            // identity fragments of the authority public key.
            let token_auth_parent = BaseBlind::random(&mut OsRng).inner();
            let token_user_data = pallas::Base::zero();
            let token_blind = BaseBlind::random(&mut OsRng).inner();

            // Derive token ID
            let token_id = poseidon_hash([token_auth_parent, token_user_data, token_blind]);

            // Generate value blind for initial mint
            let value_blind = BaseBlind::random(&mut OsRng).inner();

            // Initial supply coin (recipient is zero for initial mint)
            let recipient = pallas::Base::zero();
            let initial_value = self.initial_supply.unwrap_or(0);
            let spend_hook = pallas::Base::zero();
            let user_data = pallas::Base::zero();
            let coin_blind = BaseBlind::random(&mut OsRng).inner();

            // Create coin
            let coin_inner = poseidon_hash([
                recipient,
                pallas::Base::from(initial_value),
                token_id,
                spend_hook,
                user_data,
                coin_blind,
            ]);

            // Value commitment
            let value_commit = poseidon_hash([pallas::Base::from(initial_value), value_blind]);

            // Token commitment
            let token_commit = poseidon_hash([token_id, token_blind]);

            Some(TokenMintDebris {
                token_id,
                coin: coin_inner,
                value_commit,
                token_commit,
            })
        } else {
            None
        };

        // Create initialize params
        let mut token_symbol = [0u8; 32];
        if let Some(ref sym) = self.token_symbol {
            let bytes = sym.as_bytes();
            let len = bytes.len().min(32);
            token_symbol[..len].copy_from_slice(&bytes[..len]);
        }

        let params = InitializeParams {
            model: self.model.clone(),
            min_collateralization_ratio: self.min_collateralization_ratio,
            liquidation_threshold: self.liquidation_threshold,
            liquidation_penalty: self.liquidation_penalty,
            base_rate: self.base_rate,
            pi_kp: self.pi_kp,
            pi_ki: self.pi_ki,
            twap_window: self.twap_window,
            price_deviation_threshold: self.price_deviation_threshold,
            collateral_params: vec![],
            dead_man_switch: crate::model::DeadManSwitchConfig {
                enabled: false,
                timeout_blocks: 43200,
                action: crate::model::DeadManAction::LiquidateAll,
                last_action_block: 0,
            },
            token_authority_pub: self.token_authority_pub,
            create_token: self.create_token,
            token_symbol,
            deployer_auth: pallas::Base::zero(),
        };

        InitializeCallDebris { params, token_mint_debris }
    }
}