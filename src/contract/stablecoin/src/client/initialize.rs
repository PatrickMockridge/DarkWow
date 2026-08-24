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
//! This module handles the creation of a PromissoryNote token for the stablecoin.
//! When initializing, stablecoin creates its token type (e.g., "USDx") in PromissoryNote.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{ContractId, PublicKey},
    crypto::{poseidon_hash, BaseBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

use crate::model::{InitializeParams, StablecoinModel};

/// Debris produced by building an Initialize call
pub struct InitializeCallDebris {
    /// The contract call parameters
    pub params: InitializeParams,
    /// PromissoryNote token mint debris (if token creation requested)
    pub token_mint_debris: Option<TokenMintDebris>,
}

/// Debris for creating a PromissoryNote token
pub struct TokenMintDebris {
    /// Token ID (Poseidon hash of auth_parent, user_data, blind)
    pub asset_id: pallas::Base,
    /// Initial coin commitment
    pub coin: pallas::Base,
    /// Value commitment
    pub value_commit: pallas::Base,
    /// Token commitment
    pub token_commit: pallas::Base,
}

/// Builder for InitializeV1 - creates stablecoin and PromissoryNote token
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
    pub token_authority_pub: PublicKey,
    /// Whether to create a PromissoryNote token
    pub create_token: bool,
    /// Token symbol (e.g., "USDx")
    pub token_symbol: Option<String>,
    /// Initial supply to mint (if create_token)
    pub initial_supply: Option<u64>,
    /// Promissory Note contract ID for cross-contract validation
    pub promissory_note_contract_id: ContractId,
    /// Deployer authorization for InitV1 ZK proof
    pub deployer_auth: pallas::Base,
}

impl InitializeCallBuilder {
    /// Build the Initialize call debris
    pub fn build(&self) -> InitializeCallDebris {
        // Generate token mint debris if requested
        let token_mint_debris = if self.create_token {
            // Use random auth parent to prevent asset_id from carrying
            // identity fragments of the authority public key.
            let (token_auth_parent, token_blind, value_blind, commitment_blind) =
                if crate::deterministic_zk_enabled() {
                let mut rng = rand::rngs::StdRng::seed_from_u64(0);
                (BaseBlind::random(&mut rng).inner(), BaseBlind::random(&mut rng).inner(),
                 BaseBlind::random(&mut rng).inner(), BaseBlind::random(&mut rng).inner())
            } else {
                (BaseBlind::random(&mut OsRng).inner(), BaseBlind::random(&mut OsRng).inner(),
                 BaseBlind::random(&mut OsRng).inner(), BaseBlind::random(&mut OsRng).inner())
            };
            let token_user_data = pallas::Base::zero();

            // Derive token ID
            let asset_id = poseidon_hash([token_auth_parent, token_user_data, token_blind]);

            // Initial supply coin (recipient is zero for initial mint)
            let recipient = pallas::Base::zero();
            let initial_value = self.initial_supply.unwrap_or(0);
            let spend_hook = pallas::Base::zero();
            let user_data = pallas::Base::zero();

            // Create coin
            let coin_inner = poseidon_hash([
                recipient,
                pallas::Base::from(initial_value),
                asset_id,
                spend_hook,
                user_data,
                commitment_blind,
            ]);

            // Value commitment
            let value_commit = poseidon_hash([pallas::Base::from(initial_value), value_blind]);

            // Token commitment
            let token_commit = poseidon_hash([asset_id, token_blind]);

            Some(TokenMintDebris {
                asset_id,
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
            deployer_auth: self.deployer_auth,
            promissory_note_contract_id: self.promissory_note_contract_id,
        };

        InitializeCallDebris { params, token_mint_debris }
    }
}

// ============================================================================
// ZK Proof Generation
// ============================================================================

/// InitV1 circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct InitV1PublicInputs {
    /// Transaction binding = poseidon_hash(tx_commitment, tx_nonce)
    pub tx_binding: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
    /// Deployer authorization = poseidon_hash(deployer_secret, contract_salt)
    pub deployer_auth: pallas::Base,
}

impl InitV1PublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in init_v1.zk:
    /// constrain_instance(tx_binding), constrain_instance(tx_nonce), constrain_instance(deployer_auth)
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.deployer_auth]
    }
}

/// Input data for InitV1 proof generation
#[derive(Debug, Clone)]
pub struct InitV1CallData {
    /// Deployer's secret key (as pallas::Base field element)
    pub deployer_secret: pallas::Base,
    /// Contract salt for uniqueness
    pub contract_salt: pallas::Base,
    /// Transaction commitment
    pub tx_commitment: pallas::Base,
    /// Transaction nonce
    pub tx_nonce: pallas::Base,
}

impl InitV1CallData {
    /// Create new call data with default zero tx fields
    pub fn new(deployer_secret: pallas::Base, contract_salt: pallas::Base) -> Self {
        Self {
            deployer_secret,
            contract_salt,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute the deployer authorization hash
    pub fn deployer_auth(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(7u64), self.deployer_secret, self.contract_salt])
    }

    /// Compute the transaction binding hash
    pub fn tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> InitV1PublicInputs {
        InitV1PublicInputs {
            tx_binding: self.tx_binding(),
            tx_nonce: self.tx_nonce,
            deployer_auth: self.deployer_auth(),
        }
    }

    /// Generate prover witnesses for the circuit
    /// Order matches the witness block in init_v1.zk:
    /// deployer_secret, contract_salt, tx_commitment, tx_nonce, tx_binding
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.deployer_secret)),
            Witness::Base(Value::known(self.contract_salt)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.tx_binding())),
        ]
    }
}

/// Create an InitV1 ZK proof
pub fn create_initialize_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &InitV1CallData,
) -> Result<(Proof, InitV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}