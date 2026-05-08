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

//! DAO-Escrow module - WASM contract interaction
//!
//! This module handles DAO-Escrow contract interactions including:
//! - InitializeV1: Create a new DAO-Escrow endowment
//! - EnableDrainProtectionV1: Link DrainProtection to an endowment
//! - PayPremiumV1: Join as a member by paying premium
//! - WithdrawV1: Withdraw from endowment (with MoneyV3 child call)
//!
//! DAO-Escrow is a WASM contract that requires client-side ZK proof generation.

use dwow::{tx::{ContractCallLeaf, Transaction}, Error, Result};
use dwow_sdk::{
    crypto::pasta_prelude::PrimeField,
    crypto::poseidon_hash,
    crypto::PublicKey,
    pasta::{pallas, group::Group},
    tx::ContractCall,
};
use dwow_serial::Encodable;
use rand::{rngs::OsRng, Rng};

use crate::contract_imports::dao_escrow::{
    DAO_ESCROW_ZKAS_INIT_V1_BIN, DAO_ESCROW_ZKAS_PAY_PREMIUM_V1_BIN, DaoEscrowFunction,
    EnableDrainProtectionParamsV1, InitializeParamsV1, PayPremiumParamsV1,
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
        let init_zkbin = dwow::zkas::ZkBinary::decode(DAO_ESCROW_ZKAS_INIT_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode Init ZK binary: {:?}", e)))?;

        // Create Init circuit with empty witnesses (they'll be set during proof generation)
        let init_wits = dwow::zk::vm_heap::empty_witnesses(&init_zkbin)?;
        let init_circuit = dwow::zk::vm::ZkCircuit::new(init_wits, &init_zkbin);
        let init_pk = dwow::zk::proof::ProvingKey::build(init_zkbin.k, &init_circuit);

        // Build InitV1CallData for ZK proof
        // Note: nullifier_k is a CONSTANT in the circuit - it's baked into the .zk.bin
        // and NOT passed as a witness. We pass 0 here but it's ignored.
        let init_input = dwow_dao_escrow_contract::client::init_v1::InitV1CallData::new(
            pallas::Scalar::zero(), // nullifier_k constant - ignored, embedded in circuit
            dao_bulla,
            owner_secret_base,
            endowment_token_id,
            bulla_blind,
        );

        // Generate ZK proof
        let (proof, _public_inputs) =
            dwow_dao_escrow_contract::client::init_v1::init_v1_proof(
                &init_zkbin,
                &init_pk,
                &init_input,
            )?;

        // Build InitializeParamsV1
        let params = InitializeParamsV1 {
            dao_bulla,
            owner_pubkey: owner_pub,
            endowment_token_id,
            bulla_blind: dwow_sdk::crypto::Blind(bulla_blind),
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

    /// Pay premium to join a DAO-Escrow as a member
    ///
    /// This function allows a user to join a DAO-Escrow endowment by paying
    /// a premium. The transaction includes a ZK proof for the PayPremiumV1 function.
    ///
    /// # Arguments
    /// * `dao_escrow_bulla` - The DAO-Escrow endowment's bulla
    /// * `value` - Premium amount to pay
    /// * `token_id` - Token ID being paid (use DRKW_TOKEN_ID for DARK)
    /// * `expiry` - Membership expiry block height
    pub async fn dao_escrow_pay_premium(
        &self,
        dao_escrow_bulla: pallas::Base,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
    ) -> Result<Transaction> {
        // Get member's secret from wallet
        let member_secret = self.default_secret().await?;
        let member_secret_base = member_secret.inner();

        // Get current block height from blockchain
        let current_block = self.get_next_block_height().await? as u64;

        // Generate random blinds
        let mut membership_blind_bytes = [0u8; 32];
        let mut value_blind_bytes = [0u8; 32];
        let mut mpc1_bytes = [0u8; 32];
        let mut mpc2_bytes = [0u8; 32];
        let mut mpc3_bytes = [0u8; 32];
        OsRng.fill(&mut membership_blind_bytes);
        OsRng.fill(&mut value_blind_bytes);
        OsRng.fill(&mut mpc1_bytes);
        OsRng.fill(&mut mpc2_bytes);
        OsRng.fill(&mut mpc3_bytes);

        let membership_blind = pallas::Base::from_repr(membership_blind_bytes).unwrap();
        let value_blind = pallas::Scalar::from_repr(value_blind_bytes).unwrap();
        let mpc_secret_1 = pallas::Scalar::from_repr(mpc1_bytes).unwrap();
        let mpc_secret_2 = pallas::Scalar::from_repr(mpc2_bytes).unwrap();
        let mpc_secret_3 = pallas::Scalar::from_repr(mpc3_bytes).unwrap();

        // Max membership blocks (roughly 1 year at 5min blocks)
        let max_membership_blocks: u64 = 525600;
        let max_expiry = current_block + max_membership_blocks;

        // Load PayPremium ZK binary
        let premium_zkbin =
            dwow::zkas::ZkBinary::decode(DAO_ESCROW_ZKAS_PAY_PREMIUM_V1_BIN, false)
                .map_err(|e| Error::Custom(format!("Failed to decode PayPremium ZK binary: {:?}", e)))?;

        // Create PayPremium circuit with empty witnesses
        let premium_wits = dwow::zk::vm_heap::empty_witnesses(&premium_zkbin)?;
        let premium_circuit = dwow::zk::vm::ZkCircuit::new(premium_wits, &premium_zkbin);
        let premium_pk = dwow::zk::proof::ProvingKey::build(premium_zkbin.k, &premium_circuit);

        // Build PayPremiumV1CallData for ZK proof
        let call_data = dwow_dao_escrow_contract::client::pay_premium_v1::PayPremiumV1CallData::new(
            pallas::Scalar::zero(), // nullifier_k constant - ignored, embedded in circuit
            dao_escrow_bulla,
            current_block,
            member_secret_base,
            value,
            token_id,
            expiry,
            membership_blind,
            value_blind,
            mpc_secret_1,
            mpc_secret_2,
            mpc_secret_3,
            max_membership_blocks,
            max_expiry,
        );

        // Generate ZK proof
        let (proof, _public_inputs) =
            dwow_dao_escrow_contract::client::pay_premium_v1::pay_premium_v1_proof(
                &premium_zkbin,
                &premium_pk,
                &call_data,
            )?;

        // Get member public key for membership note
        let member_pubkey = PublicKey::from_secret(member_secret);
        let (mx, my) = member_pubkey.xy();

        // Derive membership note using poseidon_hash
        let membership_note = poseidon_hash([
            dao_escrow_bulla,
            mx,
            my,
            pallas::Base::from(value),
            token_id,
            pallas::Base::from(expiry),
            membership_blind,
        ]);

        // Build PayPremiumParamsV1 matching the contract model
        let params = PayPremiumParamsV1 {
            dao_escrow_bulla,
            membership_note,
            value_commit: pallas::Point::identity(),
            value,
            token_id,
            expiry,
            membership_blind: dwow_sdk::crypto::Blind(membership_blind),
            value_blind: dwow_sdk::crypto::Blind(value_blind),
            member_pubkey,
        };

        // Create function call data
        let function = DaoEscrowFunction::PayPremiumV1 as u8;
        let mut call_data_buf = vec![function];
        params.encode(&mut call_data_buf)
            .map_err(|e| Error::Custom(format!("Failed to encode params: {:?}", e)))?;

        // Get DAO-Escrow contract ID
        let dao_escrow_id = crate::contract_imports::DAO_ESCROW_CONTRACT_ID.get()
            .copied()
            .ok_or_else(|| Error::Custom("DAO-Escrow contract ID not initialized".to_string()))?;

        // Create contract call
        let dao_call = ContractCall {
            contract_id: dao_escrow_id,
            data: call_data_buf,
        };

        // Create contract call leaf with proof
        let dao_leaf = ContractCallLeaf { call: dao_call, proofs: vec![proof] };

        // Add fee payment
        let tx = build_fee_and_finalize_tx(&self.wallet, dao_leaf).await?;

        Ok(tx)
    }
}