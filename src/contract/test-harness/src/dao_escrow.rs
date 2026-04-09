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

//! DAO-Escrow contract test harness
//!
//! This module provides a test harness for the DAO-Escrow contract,
//! a WASM-based escrow contract with DAO governance.

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_dao_escrow_contract::{
    model::{
        DaoEscrowBulla, EnableDrainProtectionParamsV1, EnableDrainProtectionUpdateV1,
        InitializeParamsV1, InitializeUpdateV1, MembershipNote, PayPremiumParamsV1,
        PayPremiumUpdateV1, UpdateParamsV1, UpdateUpdateV1, WithdrawParamsV1, WithdrawUpdateV1,
    },
    DaoEscrowFunction,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, pasta_prelude::*, BaseBlind, ContractId, PublicKey, ScalarBlind},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the DAO-Escrow WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the dao_escrow contract.
    /// After deployment, use `dao_escrow_initialize()` to initialize an endowment.
    pub async fn deploy_dao_escrow(
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

        // Derive the dao_escrow contract ID from the deploy public key
        let dao_escrow_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed dao_escrow contract: {:?}",
            dao_escrow_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(dao_escrow_contract_id)
    }

    /// Create a `DaoEscrow::InitializeV1` transaction.
    ///
    /// Initializes a new DAO-Escrow endowment.
    pub async fn dao_escrow_initialize(
        &mut self,
        holder: &Holder,
        dao_escrow_contract_id: ContractId,
        dao_bulla: DaoEscrowBulla,
        endowment_token_id: pallas::Base,
        enable_drain_protection: bool,
        block_height: u32,
    ) -> Result<(Transaction, InitializeParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;
        let holder_public = wallet.keypair.public;
        let bulla_blind = BaseBlind::random(&mut OsRng);

        let params = InitializeParamsV1 {
            dao_bulla,
            owner_pubkey: holder_public,
            endowment_token_id,
            bulla_blind,
            enable_drain_protection,
        };

        // Build contract call data
        let mut data = vec![DaoEscrowFunction::InitializeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dao_escrow_contract_id, data };

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

    /// Execute a `DaoEscrow::InitializeV1` transaction.
    pub async fn execute_dao_escrow_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &InitializeParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao_escrow::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `DaoEscrow::UpdateV1` transaction.
    ///
    /// Updates endowment parameters.
    pub async fn dao_escrow_update(
        &mut self,
        holder: &Holder,
        dao_escrow_contract_id: ContractId,
        bulla: DaoEscrowBulla,
        block_height: u32,
    ) -> Result<(Transaction, UpdateParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = UpdateParamsV1 { bulla };

        // Build contract call data
        let mut data = vec![DaoEscrowFunction::UpdateV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dao_escrow_contract_id, data };

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

    /// Execute a `DaoEscrow::UpdateV1` transaction.
    pub async fn execute_dao_escrow_update_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &UpdateParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao_escrow::update", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `DaoEscrow::PayPremiumV1` transaction.
    ///
    /// Member pays premium to join the DAO-Escrow and receive membership.
    pub async fn dao_escrow_pay_premium(
        &mut self,
        holder: &Holder,
        dao_escrow_contract_id: ContractId,
        dao_escrow_bulla: DaoEscrowBulla,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
        block_height: u32,
    ) -> Result<(Transaction, PayPremiumParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;
        let holder_public = wallet.keypair.public;

        let value_blind = ScalarBlind::random(&mut OsRng);
        let membership_blind = BaseBlind::random(&mut OsRng);

        // Create value commitment
        let value_commit = pedersen_commitment_u64(value, value_blind);
        let value_coords = value_commit.to_affine().coordinates().unwrap();

        // Derive membership note
        let membership_note = poseidon_hash([
            dao_escrow_bulla,
            holder_public.x(),
            holder_public.y(),
            pallas::Base::from(value),
            token_id,
            pallas::Base::from(expiry),
            membership_blind.inner(),
        ]);

        let params = PayPremiumParamsV1 {
            dao_escrow_bulla,
            membership_note,
            value_commit,
            value,
            token_id,
            expiry,
            membership_blind,
            value_blind,
            member_pubkey: holder_public,
        };

        // Build contract call data
        let mut data = vec![DaoEscrowFunction::PayPremiumV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dao_escrow_contract_id, data };

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

    /// Execute a `DaoEscrow::PayPremiumV1` transaction.
    pub async fn execute_dao_escrow_pay_premium_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &PayPremiumParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao_escrow::pay_premium", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `DaoEscrow::WithdrawV1` transaction.
    ///
    /// Endowment owner withdraws funds from the pool.
    pub async fn dao_escrow_withdraw(
        &mut self,
        holder: &Holder,
        dao_escrow_contract_id: ContractId,
        dao_escrow_bulla: DaoEscrowBulla,
        value: u64,
        recipient_pubkey: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, WithdrawParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = WithdrawParamsV1 {
            dao_escrow_bulla,
            value,
            recipient_pubkey,
        };

        // Build contract call data
        let mut data = vec![DaoEscrowFunction::WithdrawV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dao_escrow_contract_id, data };

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

    /// Execute a `DaoEscrow::WithdrawV1` transaction.
    pub async fn execute_dao_escrow_withdraw_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &WithdrawParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao_escrow::withdraw", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `DaoEscrow::EnableDrainProtectionV1` transaction.
    ///
    /// Enables DrainProtection on an existing DAO-Escrow endowment.
    pub async fn dao_escrow_enable_drain_protection(
        &mut self,
        holder: &Holder,
        dao_escrow_contract_id: ContractId,
        dao_escrow_bulla: DaoEscrowBulla,
        drain_protection_bulla: DaoEscrowBulla,
        block_height: u32,
    ) -> Result<(Transaction, EnableDrainProtectionParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = EnableDrainProtectionParamsV1 {
            dao_escrow_bulla,
            drain_protection_bulla,
        };

        // Build contract call data
        let mut data = vec![DaoEscrowFunction::EnableDrainProtectionV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dao_escrow_contract_id, data };

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

    /// Execute a `DaoEscrow::EnableDrainProtectionV1` transaction.
    pub async fn execute_dao_escrow_enable_drain_protection_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &EnableDrainProtectionParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dao_escrow::enable_drain_protection", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}