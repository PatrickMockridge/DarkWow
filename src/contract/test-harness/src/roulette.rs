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

//! Roulette contract test harness
//!
//! This module provides a test harness for the Roulette contract,
//! a WASM-based privacy-preserving roulette game with fixed-odds betting.
//!
//! Flow:
//! 1. House initializes with InitializeV1
//! 2. Player places bet with PlaceBetV1 (on specific numbers/types)
//! 3. House spins wheel with SpinWheelV1 (block entropy)
//! 4. House settles bets with SettleBetsV1
//! 5. House closes table with HouseCloseV1

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, ContractId, PublicKey, schnorr::Signature},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use tracing::debug;
use darkfi_roulette_contract::{
    model::{
        HouseCloseParamsV1, HouseCloseUpdateV1, InitializeParamsV1, InitializeUpdateV1,
        PlaceBetParamsV1, PlaceBetUpdateV1, SettleBetsParamsV1, SettleBetsUpdateV1,
        SpinWheelParamsV1, SpinWheelUpdateV1,
    },
    RouletteFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Roulette WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the roulette contract.
    /// After deployment, use `roulette_initialize()` to set up the table.
    pub async fn deploy_roulette(
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

        // Derive the roulette contract ID from the deploy public key
        let roulette_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed roulette contract: {:?}",
            roulette_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(roulette_contract_id)
    }

    /// Create a `Roulette::InitializeV1` transaction.
    ///
    /// House initializes the roulette table.
    pub async fn roulette_initialize(
        &mut self,
        holder: &Holder,
        roulette_contract_id: ContractId,
        block_height: u32,
    ) -> Result<(Transaction, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let data = vec![RouletteFunction::InitializeV1 as u8];
        let call = ContractCall { contract_id: roulette_contract_id, data };

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

    /// Execute a `Roulette::InitializeV1` transaction.
    pub async fn execute_roulette_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("roulette::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Roulette::PlaceBetV1` transaction.
    ///
    /// Player places a bet on specific numbers or bet types.
    pub async fn roulette_place_bet(
        &mut self,
        holder: &Holder,
        roulette_contract_id: ContractId,
        table_id: pallas::Base,
        player_pub: PublicKey,
        bet_type: darkfi_roulette_contract::model::BetType,
        numbers: Vec<u8>,
        amount: u64,
        block_height: u32,
    ) -> Result<(Transaction, PlaceBetParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = PlaceBetParamsV1 {
            table_id,
            player_pub,
            bet_type,
            numbers,
            amount,
            signature: pallas::Base::zero(),
        };

        // Build contract call data
        let mut data = vec![RouletteFunction::PlaceBetV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: roulette_contract_id, data };

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

    /// Execute a `Roulette::PlaceBetV1` transaction.
    pub async fn execute_roulette_place_bet_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &PlaceBetParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("roulette::place_bet", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Roulette::SpinWheelV1` transaction.
    ///
    /// House spins the wheel using block entropy.
    pub async fn roulette_spin_wheel(
        &mut self,
        holder: &Holder,
        roulette_contract_id: ContractId,
        table_id: pallas::Base,
        nonce: pallas::Base,
        house_pub: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, SpinWheelParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SpinWheelParamsV1 {
            table_id,
            nonce,
            house_pub,
            signature: Signature::dummy(),
        };

        // Build contract call data
        let mut data = vec![RouletteFunction::SpinWheelV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: roulette_contract_id, data };

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

    /// Execute a `Roulette::SpinWheelV1` transaction.
    pub async fn execute_roulette_spin_wheel_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SpinWheelParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("roulette::spin_wheel", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Roulette::SettleBetsV1` transaction.
    ///
    /// House settles all bets for a given table.
    pub async fn roulette_settle_bets(
        &mut self,
        holder: &Holder,
        roulette_contract_id: ContractId,
        table_id: pallas::Base,
        bet_ids: Vec<pallas::Base>,
        block_height: u32,
    ) -> Result<(Transaction, SettleBetsParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SettleBetsParamsV1 { table_id, bet_ids };

        // Build contract call data
        let mut data = vec![RouletteFunction::SettleBetsV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: roulette_contract_id, data };

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

    /// Execute a `Roulette::SettleBetsV1` transaction.
    pub async fn execute_roulette_settle_bets_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SettleBetsParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("roulette::settle_bets", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Roulette::HouseCloseV1` transaction.
    ///
    /// House closes the table.
    pub async fn roulette_house_close(
        &mut self,
        holder: &Holder,
        roulette_contract_id: ContractId,
        table_id: pallas::Base,
        house_pub: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, HouseCloseParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = HouseCloseParamsV1 {
            table_id,
            house_pub,
            signature: Signature::dummy(),
        };

        // Build contract call data
        let mut data = vec![RouletteFunction::HouseCloseV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: roulette_contract_id, data };

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

    /// Execute a `Roulette::HouseCloseV1` transaction.
    pub async fn execute_roulette_house_close_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &HouseCloseParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("roulette::house_close", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}