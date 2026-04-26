/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! DAO-Escrow module - WASM contract interaction
//!
//! This module handles DAO-Escrow contract interactions including:
//! - InitializeV1: Create a new DAO-Escrow endowment
//! - EnableDrainProtectionV1: Link DrainProtection to an endowment
//! - PayPremiumV1: Join as a member by paying premium
//! - WithdrawV1: Withdraw from endowment (with MoneyV3 child call)
//!
//! DAO-Escrow is a WASM contract that requires client-side ZK proof generation.

use darkfi::{tx::{ContractCallLeaf, Transaction}, Error, Result};
use darkfi_sdk::{
    crypto::pasta_prelude::PrimeField,
    crypto::PublicKey,
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::Encodable;
use rand::{rngs::OsRng, Rng};

use crate::contract_imports::dao_escrow::{
    DAO_ESCROW_ZKAS_INIT_V1_BIN, DaoEscrowFunction, EnableDrainProtectionParamsV1, InitializeParamsV1,
};
use crate::fee_builder::build_fee_and_finalize_tx;
use crate::Drk;

impl Drk {
    /// Initialize a new DAO-Escrow endowment
    ///
    /// This creates a new DAO-Escrow instance with the given configuration.
    /// The transaction includes a ZK proof for the InitializeV1 function.
    ///
    /// # Arguments
    /// * `dao_bulla` - The controlling DAO's bulla (use Base::zero() for standalone)
    /// * `owner_pubkey` - Owner's public key (derived from wallet's secret if None)
    /// * `endowment_token_id` - Token ID held in the endowment
    /// * `bulla_blind` - Blind factor for the bulla (random if None)
    /// * `enable_drain_protection` - Whether to enable DrainProtection
    pub async fn dao_escrow_initialize(
        &self,
        dao_bulla: pallas::Base,
        owner_pubkey: Option<PublicKey>,
        endowment_token_id: pallas::Base,
        bulla_blind: Option<pallas::Base>,
        enable_drain_protection: bool,
    ) -> Result<Transaction> {
        // Get owner's secret from wallet
        let owner_secret = self.default_secret().await?;
        let owner_secret_base = owner_secret.inner();

        // Derive owner's public key if not provided
        let owner_pub = owner_pubkey.unwrap_or_else(|| PublicKey::from_secret(owner_secret));

        // Generate random bulla blind if not provided
        let bulla_blind = bulla_blind.unwrap_or_else(|| {
            let mut bytes = [0u8; 32];
            OsRng.fill(&mut bytes);
            pallas::Base::from_repr(bytes).unwrap()
        });

        // Load DAO-Escrow Init ZK binary
        let init_zkbin = darkfi::zkas::ZkBinary::decode(DAO_ESCROW_ZKAS_INIT_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode Init ZK binary: {:?}", e)))?;

        // Create Init circuit with empty witnesses (they'll be set during proof generation)
        let init_wits = darkfi::zk::vm_heap::empty_witnesses(&init_zkbin)?;
        let init_circuit = darkfi::zk::vm::ZkCircuit::new(init_wits, &init_zkbin);
        let init_pk = darkfi::zk::proof::ProvingKey::build(init_zkbin.k, &init_circuit);

        // Build InitV1CallData for ZK proof
        // Note: nullifier_k is a CONSTANT in the circuit - it's baked into the .zk.bin
        // and NOT passed as a witness. We pass 0 here but it's ignored.
        let init_input = darkfi_dao_escrow_contract::client::init_v1::InitV1CallData::new(
            pallas::Scalar::zero(), // nullifier_k constant - ignored, embedded in circuit
            dao_bulla,
            owner_secret_base,
            endowment_token_id,
            bulla_blind,
        );

        // Generate ZK proof
        let (proof, _public_inputs) =
            darkfi_dao_escrow_contract::client::init_v1::init_v1_proof(
                &init_zkbin,
                &init_pk,
                &init_input,
            )?;

        // Build InitializeParamsV1
        let params = InitializeParamsV1 {
            dao_bulla,
            owner_pubkey: owner_pub,
            endowment_token_id,
            bulla_blind: darkfi_sdk::crypto::Blind(bulla_blind),
            enable_drain_protection,
        };

        // Create function call data
        let function = DaoEscrowFunction::InitializeV1 as u8;
        let mut call_data = vec![function];
        params.encode(&mut call_data)
            .map_err(|e| Error::Custom(format!("Failed to encode params: {:?}", e)))?;

        // Get DAO-Escrow contract ID
        let dao_escrow_id = crate::contract_imports::DAO_ESCROW_CONTRACT_ID.get()
            .copied()
            .ok_or_else(|| Error::Custom("DAO-Escrow contract ID not initialized".to_string()))?;

        // Create contract call
        let dao_call = ContractCall {
            contract_id: dao_escrow_id,
            data: call_data,
        };

        // Create contract call leaf with proof
        let dao_leaf = ContractCallLeaf { call: dao_call, proofs: vec![proof] };

        // Add fee payment
        let tx = build_fee_and_finalize_tx(&self.wallet, dao_leaf).await?;

        Ok(tx)
    }

    /// Enable DrainProtection on an existing DAO-Escrow endowment
    ///
    /// This links a DrainProtection contract instance to an existing DAO-Escrow
    /// endowment. The DrainProtection must be initialized first to get its bulla.
    ///
    /// # Arguments
    /// * `dao_escrow_bulla` - The DAO-Escrow endowment's bulla (returned from InitializeV1)
    /// * `drain_protection_bulla` - The DrainProtection contract's bulla (from DrainProtection::InitializeV1)
    pub async fn dao_escrow_enable_drain_protection(
        &self,
        dao_escrow_bulla: pallas::Base,
        drain_protection_bulla: pallas::Base,
    ) -> Result<Transaction> {
        // Build EnableDrainProtectionParamsV1
        let params = EnableDrainProtectionParamsV1 {
            dao_escrow_bulla,
            drain_protection_bulla,
        };

        // Create function call data
        let function = DaoEscrowFunction::EnableDrainProtectionV1 as u8;
        let mut call_data = vec![function];
        params.encode(&mut call_data)
            .map_err(|e| Error::Custom(format!("Failed to encode params: {:?}", e)))?;

        // Get DAO-Escrow contract ID
        let dao_escrow_id = crate::contract_imports::DAO_ESCROW_CONTRACT_ID.get()
            .copied()
            .ok_or_else(|| Error::Custom("DAO-Escrow contract ID not initialized".to_string()))?;

        // Create contract call (no ZK proof needed for this function)
        let dao_call = ContractCall {
            contract_id: dao_escrow_id,
            data: call_data,
        };

        // Create contract call leaf with no proofs
        let dao_leaf = ContractCallLeaf { call: dao_call, proofs: vec![] };

        // Add fee payment and finalize
        let tx = build_fee_and_finalize_tx(&self.wallet, dao_leaf).await?;

        Ok(tx)
    }
}