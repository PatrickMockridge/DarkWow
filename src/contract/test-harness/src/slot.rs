/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Slot contract test harness
//!
//! This module provides a test harness for the Slot contract,
//! a WASM-based privacy-preserving slot machine.
//!
//! Flow:
//! 1. House initializes slot game with InitializeV1
//! 2. Player commits to spin with CommitSpinV1 (hides bet in ZK)
//! 3. Block entropy reveals random positions with RevealSpinV1
//! 4. Winning combinations calculated with SettleSpinV1 (ZK constrained)
//! 5. House closes abandoned spins with CancelSpinV1

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, BaseBlind, ContractId, PublicKey, ScalarBlind},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;
use darkfi_slot_contract::{
    model::{
        CancelSpinParamsV1, CancelSpinUpdateV1, CommitSpinParamsV1, CommitSpinUpdateV1,
        RevealSpinParamsV1, RevealSpinUpdateV1, SettleSpinParamsV1, SettleSpinUpdateV1,
        SpinId,
    },
    SlotFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Slot WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the slot contract.
    /// After deployment, use `slot_initialize()` to set up the game.
    pub async fn deploy_slot(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        // Get the public key for deriving contract ID before mutable borrow
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        // Deploy the WASM contract using deployooor
        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        // Derive the slot contract ID from the deploy public key
        let slot_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed slot contract: {:?}",
            slot_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(slot_contract_id)
    }

    /// Create a `Slot::InitializeV1` transaction.
    ///
    /// House initializes the slot machine game configuration.
    pub async fn slot_initialize(
        &mut self,
        holder: &Holder,
        slot_contract_id: ContractId,
        block_height: u32,
    ) -> Result<(Transaction, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        // InitializeV1 takes no parameters - empty data array
        let data = vec![SlotFunction::InitializeV1 as u8];
        let call = ContractCall { contract_id: slot_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, fee_params))
    }

    /// Execute a `Slot::InitializeV1` transaction.
    pub async fn execute_slot_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("slot::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Slot::CommitSpinV1` transaction.
    ///
    /// Player commits to a spin with hidden bet parameters.
    pub async fn slot_commit_spin(
        &mut self,
        holder: &Holder,
        slot_contract_id: ContractId,
        player_pub: PublicKey,
        bet_value: u64,
        paylines_played: u32,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        house_edge: u32,
        confirmation_depth: u8,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, CommitSpinParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        // Compute value commitment for the bet
        let value_blind = ScalarBlind::random(&mut OsRng);
        let value_commit = pedersen_commitment_u64(bet_value, value_blind);

        let params = CommitSpinParamsV1 {
            player_pub,
            bet_value,
            paylines_played,
            secret_nonce,
            blind,
            house_edge,
            confirmation_depth,
            token_id,
            value_commit,
        };

        // Build contract call data
        let mut data = vec![SlotFunction::CommitSpinV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: slot_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Build and sign the transaction
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `Slot::CommitSpinV1` transaction.
    pub async fn execute_slot_commit_spin_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CommitSpinParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("slot::commit_spin", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Slot::RevealSpinV1` transaction.
    ///
    /// Uses block entropy to reveal random spin positions.
    pub async fn slot_reveal_spin(
        &mut self,
        holder: &Holder,
        slot_contract_id: ContractId,
        spin_id: SpinId,
        secret_nonce: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, RevealSpinParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = RevealSpinParamsV1 { spin_id, secret_nonce };

        // Build contract call data
        let mut data = vec![SlotFunction::RevealSpinV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: slot_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Build and sign the transaction
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `Slot::RevealSpinV1` transaction.
    pub async fn execute_slot_reveal_spin_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &RevealSpinParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("slot::reveal_spin", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Slot::SettleSpinV1` transaction.
    ///
    /// Settles the spin and calculates payout (ZK constrained).
    pub async fn slot_settle_spin(
        &mut self,
        holder: &Holder,
        slot_contract_id: ContractId,
        spin_id: SpinId,
        block_height: u32,
    ) -> Result<(Transaction, SettleSpinParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SettleSpinParamsV1 { spin_id };

        // Build contract call data
        let mut data = vec![SlotFunction::SettleSpinV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: slot_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Build and sign the transaction
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `Slot::SettleSpinV1` transaction.
    pub async fn execute_slot_settle_spin_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SettleSpinParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("slot::settle_spin", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Slot::CancelSpinV1` transaction.
    ///
    /// House closes abandoned spins after timeout.
    pub async fn slot_cancel_spin(
        &mut self,
        holder: &Holder,
        slot_contract_id: ContractId,
        spin_id: SpinId,
        block_height: u32,
    ) -> Result<(Transaction, CancelSpinParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CancelSpinParamsV1 { spin_id };

        // Build contract call data
        let mut data = vec![SlotFunction::CancelSpinV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: slot_contract_id, data };

        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        // If we have tx fees enabled, make an offering
        let mut fee_params = None;
        let mut fee_signature_secrets = None;
        if self.verify_fees {
            let mut tx = tx_builder.build()?;
            let sigs = tx.create_sigs(&[holder_secret])?;
            tx.signatures = vec![sigs];

            let (fee_call, fee_proofs, fee_secrets, _spent_fee_coins, fee_call_params) =
                self.append_fee_call(holder, tx, block_height, &[]).await?;

            tx_builder.append(
                ContractCallLeaf { call: fee_call, proofs: fee_proofs },
                vec![],
            )?;
            fee_signature_secrets = Some(fee_secrets);
            fee_params = Some(fee_call_params);
        }

        // Build and sign the transaction
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];
        if let Some(fee_signature_secrets) = fee_signature_secrets {
            let sigs = tx.create_sigs(&fee_signature_secrets)?;
            tx.signatures.push(sigs);
        }

        Ok((tx, params, fee_params))
    }

    /// Execute a `Slot::CancelSpinV1` transaction.
    pub async fn execute_slot_cancel_spin_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CancelSpinParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("slot::cancel_spin", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}