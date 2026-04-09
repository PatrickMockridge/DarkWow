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

//! Baccarat contract test harness
//!
//! This module provides a test harness for the Baccarat contract,
//! a WASM-based privacy-preserving Baccarat (Punto Banco) game.
//!
//! Flow:
//! 1. House initializes with InitializeV1
//! 2. Player commits to bet with CommitBetV1 (bet on Player/Banker/Tie)
//! 3. Cards dealt using block entropy with DrawCardsV1
//! 4. Payout calculated with SettleBetV1
//! 5. House closes abandoned bets with HouseCloseV1

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, BaseBlind, ContractId, PublicKey, ScalarBlind},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;
use darkfi_baccarat_contract::{
    model::{
        BetId, CommitBetParamsV1, CommitBetUpdateV1, DrawCardsParamsV1, DrawCardsUpdateV1,
        HouseCloseParamsV1, HouseCloseUpdateV1, SettleBetParamsV1, SettleBetUpdateV1,
    },
    BaccaratFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Baccarat WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the baccarat contract.
    /// After deployment, use `baccarat_initialize()` to set up the game.
    pub async fn deploy_baccarat(
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

        // Derive the baccarat contract ID from the deploy public key
        let baccarat_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed baccarat contract: {:?}",
            baccarat_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(baccarat_contract_id)
    }

    /// Create a `Baccarat::InitializeV1` transaction.
    ///
    /// House initializes the baccarat table.
    pub async fn baccarat_initialize(
        &mut self,
        holder: &Holder,
        baccarat_contract_id: ContractId,
        block_height: u32,
    ) -> Result<(Transaction, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let data = vec![BaccaratFunction::InitializeV1 as u8];
        let call = ContractCall { contract_id: baccarat_contract_id, data };

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

    /// Execute a `Baccarat::InitializeV1` transaction.
    pub async fn execute_baccarat_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("baccarat::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Baccarat::CommitBetV1` transaction.
    ///
    /// Player commits to a bet on Player/Banker/Tie.
    pub async fn baccarat_commit_bet(
        &mut self,
        holder: &Holder,
        baccarat_contract_id: ContractId,
        player_pub: PublicKey,
        bet_type: u8,
        bet_value: u64,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        house_edge: u32,
        confirmation_depth: u8,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, CommitBetParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        // Compute value commitment for the bet
        let value_blind = ScalarBlind::random(&mut OsRng);
        let value_commit = pedersen_commitment_u64(bet_value, value_blind);

        let params = CommitBetParamsV1 {
            player_pub,
            bet_type,
            bet_value,
            secret_nonce,
            blind,
            house_edge,
            confirmation_depth,
            token_id,
            value_commit,
        };

        // Build contract call data
        let mut data = vec![BaccaratFunction::CommitBetV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: baccarat_contract_id, data };

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

    /// Execute a `Baccarat::CommitBetV1` transaction.
    pub async fn execute_baccarat_commit_bet_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CommitBetParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("baccarat::commit_bet", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Baccarat::DrawCardsV1` transaction.
    ///
    /// Uses block entropy to deal cards.
    pub async fn baccarat_draw_cards(
        &mut self,
        holder: &Holder,
        baccarat_contract_id: ContractId,
        bet_id: BetId,
        secret_nonce: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, DrawCardsParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = DrawCardsParamsV1 { bet_id, secret_nonce };

        // Build contract call data
        let mut data = vec![BaccaratFunction::DrawCardsV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: baccarat_contract_id, data };

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

    /// Execute a `Baccarat::DrawCardsV1` transaction.
    pub async fn execute_baccarat_draw_cards_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &DrawCardsParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("baccarat::draw_cards", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Baccarat::SettleBetV1` transaction.
    ///
    /// Settles the bet and calculates payout.
    pub async fn baccarat_settle_bet(
        &mut self,
        holder: &Holder,
        baccarat_contract_id: ContractId,
        bet_id: BetId,
        block_height: u32,
    ) -> Result<(Transaction, SettleBetParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SettleBetParamsV1 { bet_id };

        // Build contract call data
        let mut data = vec![BaccaratFunction::SettleBetV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: baccarat_contract_id, data };

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

    /// Execute a `Baccarat::SettleBetV1` transaction.
    pub async fn execute_baccarat_settle_bet_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SettleBetParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("baccarat::settle_bet", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Baccarat::HouseCloseV1` transaction.
    ///
    /// House closes abandoned bets after timeout.
    pub async fn baccarat_house_close(
        &mut self,
        holder: &Holder,
        baccarat_contract_id: ContractId,
        bet_id: BetId,
        block_height: u32,
    ) -> Result<(Transaction, HouseCloseParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = HouseCloseParamsV1 { bet_id };

        // Build contract call data
        let mut data = vec![BaccaratFunction::HouseCloseV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: baccarat_contract_id, data };

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

    /// Execute a `Baccarat::HouseCloseV1` transaction.
    pub async fn execute_baccarat_house_close_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &HouseCloseParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("baccarat::house_close", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}