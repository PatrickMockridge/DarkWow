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

//! Game Room contract test harness
//!
//! This module provides a test harness for the Game Room contract,
//! a WASM-based generalized betting and pot management contract.
//!
//! Note: Game Room has no ZK circuits - game logic is delegated to app layer.
//!
//! Flow:
//! 1. Room owner creates room with CreateRoomV1
//! 2. Players deposit stake with DepositV1
//! 3. Players place bets with PlaceBetV1, RaiseV1, CallV1, FoldV1
//! 4. Owner closes pot with ClosePotV1
//! 5. Owner settles pot with SettlePotV1
//! 6. Players claim winnings with ClaimV1

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, ContractId, PublicKey},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use tracing::debug;
use darkfi_game_room_contract::{
    model::{
        CallParamsV1, CallUpdateV1, ClaimParamsV1, ClaimUpdateV1, ClosePotParamsV1,
        ClosePotUpdateV1, ContributeEntropyParamsV1, ContributeEntropyUpdateV1,
        CreateRoomParamsV1, CreateRoomUpdateV1, DepositParamsV1, DepositUpdateV1,
        FoldParamsV1, FoldUpdateV1, PlaceBetParamsV1, PlaceBetUpdateV1, RaiseParamsV1,
        RaiseUpdateV1, SettlePotParamsV1, SettlePotUpdateV1, WithdrawParamsV1, WithdrawUpdateV1,
        EntropyMode,
    },
    GameRoomFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Game Room WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the game_room contract.
    /// After deployment, use `game_room_create_room()` to create a room.
    pub async fn deploy_game_room(
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

        // Derive the game_room contract ID from the deploy public key
        let game_room_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed game_room contract: {:?}",
            game_room_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(game_room_contract_id)
    }

    /// Create a `GameRoom::CreateRoomV1` transaction.
    ///
    /// Room owner creates a new game room.
    pub async fn game_room_create_room(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        owner: PublicKey,
        token_id: pallas::Base,
        min_stake: u64,
        max_stake: u64,
        entropy_mode: EntropyMode,
        block_height: u32,
    ) -> Result<(Transaction, CreateRoomParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateRoomParamsV1 {
            owner,
            token_id,
            min_stake,
            max_stake,
            entropy_mode,
            confirmation_depth: 1,
            required_entropy_contributions: 0,
            entropy_contribution_deadline: 0,
            max_players: 10,
            nonce: pallas::Base::zero(),
        };

        // Build contract call data
        let mut data = vec![GameRoomFunction::CreateRoomV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::CreateRoomV1` transaction.
    pub async fn execute_game_room_create_room_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateRoomParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::create_room", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `GameRoom::DepositV1` transaction.
    ///
    /// Player deposits stake into the game room.
    pub async fn game_room_deposit(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        room_id: pallas::Base,
        player: PublicKey,
        amount: u64,
        block_height: u32,
    ) -> Result<(Transaction, DepositParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = DepositParamsV1 { room_id, player, amount };

        // Build contract call data
        let mut data = vec![GameRoomFunction::DepositV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::DepositV1` transaction.
    pub async fn execute_game_room_deposit_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &DepositParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::deposit", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `GameRoom::PlaceBetV1` transaction.
    ///
    /// Player places a bet/ante in the game.
    pub async fn game_room_place_bet(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        room_id: pallas::Base,
        player: PublicKey,
        amount: u64,
        bet_type: darkfi_game_room_contract::model::BetType,
        block_height: u32,
    ) -> Result<(Transaction, PlaceBetParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = PlaceBetParamsV1 {
            room_id,
            player,
            amount,
            bet_type,
            nonce: pallas::Base::zero(),
        };

        // Build contract call data
        let mut data = vec![GameRoomFunction::PlaceBetV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::PlaceBetV1` transaction.
    pub async fn execute_game_room_place_bet_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &PlaceBetParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::place_bet", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `GameRoom::CallV1` transaction.
    ///
    /// Player calls the current bet.
    pub async fn game_room_call(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        room_id: pallas::Base,
        player: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, CallParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CallParamsV1 { room_id, player, nonce: pallas::Base::zero() };

        // Build contract call data
        let mut data = vec![GameRoomFunction::CallV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::CallV1` transaction.
    pub async fn execute_game_room_call_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CallParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::call", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `GameRoom::FoldV1` transaction.
    ///
    /// Player folds (forfeits the hand).
    pub async fn game_room_fold(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        room_id: pallas::Base,
        player: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, FoldParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = FoldParamsV1 { room_id, player };

        // Build contract call data
        let mut data = vec![GameRoomFunction::FoldV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::FoldV1` transaction.
    pub async fn execute_game_room_fold_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &FoldParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::fold", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `GameRoom::SettlePotV1` transaction.
    ///
    /// Owner DAO settles the pot to winners.
    pub async fn game_room_settle_pot(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        caller: PublicKey,
        room_id: pallas::Base,
        pot_id: pallas::Base,
        winners: Vec<(PublicKey, u64)>,
        block_height: u32,
    ) -> Result<(Transaction, SettlePotParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SettlePotParamsV1 {
            caller,
            room_id,
            pot_id,
            winners,
            signature: vec![],
        };

        // Build contract call data
        let mut data = vec![GameRoomFunction::SettlePotV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::SettlePotV1` transaction.
    pub async fn execute_game_room_settle_pot_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SettlePotParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::settle_pot", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `GameRoom::ClaimV1` transaction.
    ///
    /// Player claims winnings from the pot.
    pub async fn game_room_claim(
        &mut self,
        holder: &Holder,
        game_room_contract_id: ContractId,
        room_id: pallas::Base,
        pot_id: pallas::Base,
        winner: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, ClaimParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ClaimParamsV1 { room_id, pot_id, winner };

        // Build contract call data
        let mut data = vec![GameRoomFunction::ClaimV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: game_room_contract_id, data };

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

    /// Execute a `GameRoom::ClaimV1` transaction.
    pub async fn execute_game_room_claim_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ClaimParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("game_room::claim", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}