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

//! Lottery contract test harness
//!
//! This module provides a test harness for the Lottery contract,
//! a WASM-based privacy-preserving pooled lottery.
//!
//! Flow:
//! 1. House initializes lottery with DrawWinnersV1
//! 2. Players buy tickets with BuyTicketV1 (commit numbers)
//! 3. House draws winning numbers with DrawWinnersV1
//! 4. Players reveal numbers with RevealTicketV1
//! 5. Winners claim prizes with ClaimPrizeV1
//! 6. House expires lottery with ExpireLotteryV1 to claim unclaimed

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{poseidon_hash, pasta_prelude::*, BaseBlind, ContractId, PublicKey, Keypair},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;
use darkfi_lottery_contract::{
    model::{
        BuyTicketParamsV1, BuyTicketUpdateV1, ClaimPrizeParamsV1, ClaimPrizeUpdateV1,
        DrawWinnersParamsV1, DrawWinnersUpdateV1, ExpireLotteryParamsV1, ExpireLotteryUpdateV1,
        InitializeParamsV1, InitializeUpdateV1, LotteryConfig, LotteryId, RevealTicketParamsV1,
        RevealTicketUpdateV1, TicketId,
    },
    LotteryFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Lottery WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the lottery contract.
    /// After deployment, use `lottery_initialize()` to start a lottery round.
    pub async fn deploy_lottery(
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

        // Derive the lottery contract ID from the deploy public key
        let lottery_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed lottery contract: {:?}",
            lottery_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(lottery_contract_id)
    }

    /// Create a `Lottery::InitializeV1` transaction.
    ///
    /// Initializes a new lottery round. The house sets parameters
    /// and players can then buy tickets.
    pub async fn lottery_initialize(
        &mut self,
        holder: &Holder,
        lottery_contract_id: ContractId,
        house_pub: PublicKey,
        config: LotteryConfig,
        duration: u64,
        claim_duration: u64,
        rolled_over: u64,
        block_height: u32,
    ) -> Result<(Transaction, InitializeParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = InitializeParamsV1 {
            house_pub,
            config,
            duration,
            claim_duration,
            rolled_over,
        };

        // Build contract call data
        let mut data = vec![LotteryFunction::InitializeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: lottery_contract_id, data };

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

    /// Execute a `Lottery::InitializeV1` transaction.
    pub async fn execute_lottery_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &InitializeParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("lottery::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Lottery::BuyTicketV1` transaction.
    ///
    /// Player commits to ticket numbers without revealing them.
    pub async fn lottery_buy_ticket(
        &mut self,
        holder: &Holder,
        lottery_contract_id: ContractId,
        player_pub: PublicKey,
        commitment: pallas::Base,
        token_id: pallas::Base,
        value: u64,
        block_height: u32,
    ) -> Result<(Transaction, BuyTicketParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = BuyTicketParamsV1 {
            player_pub,
            commitment,
            token_id,
            value,
            signature: pallas::Base::zero(),
        };

        // Build contract call data
        let mut data = vec![LotteryFunction::BuyTicketV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: lottery_contract_id, data };

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

    /// Execute a `Lottery::BuyTicketV1` transaction.
    pub async fn execute_lottery_buy_ticket_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &BuyTicketParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("lottery::buy_ticket", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Lottery::DrawWinnersV1` transaction.
    ///
    /// House draws winning numbers using block entropy.
    pub async fn lottery_draw_winners(
        &mut self,
        holder: &Holder,
        lottery_contract_id: ContractId,
        lottery_id: LotteryId,
        nonce: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, DrawWinnersParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = DrawWinnersParamsV1 { lottery_id, nonce };

        // Build contract call data
        let mut data = vec![LotteryFunction::DrawWinnersV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: lottery_contract_id, data };

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

    /// Execute a `Lottery::DrawWinnersV1` transaction.
    pub async fn execute_lottery_draw_winners_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &DrawWinnersParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("lottery::draw_winners", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Lottery::RevealTicketV1` transaction.
    ///
    /// Player reveals their numbers to claim a prize.
    pub async fn lottery_reveal_ticket(
        &mut self,
        holder: &Holder,
        lottery_contract_id: ContractId,
        ticket_id: TicketId,
        numbers: Vec<u8>,
        nonce: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, RevealTicketParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = RevealTicketParamsV1 { ticket_id, numbers, nonce };

        // Build contract call data
        let mut data = vec![LotteryFunction::RevealTicketV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: lottery_contract_id, data };

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

    /// Execute a `Lottery::RevealTicketV1` transaction.
    pub async fn execute_lottery_reveal_ticket_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &RevealTicketParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("lottery::reveal_ticket", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Lottery::ClaimPrizeV1` transaction.
    ///
    /// Winner claims their prize share.
    pub async fn lottery_claim_prize(
        &mut self,
        holder: &Holder,
        lottery_contract_id: ContractId,
        ticket_id: TicketId,
        block_height: u32,
    ) -> Result<(Transaction, ClaimPrizeParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ClaimPrizeParamsV1 { ticket_id, proof: vec![] };

        // Build contract call data
        let mut data = vec![LotteryFunction::ClaimPrizeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: lottery_contract_id, data };

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

    /// Execute a `Lottery::ClaimPrizeV1` transaction.
    pub async fn execute_lottery_claim_prize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ClaimPrizeParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("lottery::claim_prize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Lottery::ExpireLotteryV1` transaction.
    ///
    /// House expires lottery to claim unclaimed prizes.
    pub async fn lottery_expire(
        &mut self,
        holder: &Holder,
        lottery_contract_id: ContractId,
        lottery_id: LotteryId,
        block_height: u32,
    ) -> Result<(Transaction, ExpireLotteryParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ExpireLotteryParamsV1 { lottery_id };

        // Build contract call data
        let mut data = vec![LotteryFunction::ExpireLotteryV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: lottery_contract_id, data };

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

    /// Execute a `Lottery::ExpireLotteryV1` transaction.
    pub async fn execute_lottery_expire_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ExpireLotteryParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("lottery::expire", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}