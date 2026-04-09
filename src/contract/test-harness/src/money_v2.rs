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

//! MoneyV2 contract test harness
//!
//! This module provides a test harness for the MoneyV2 contract,
//! a SECURE version with self-contained ZK circuit design.
//!
//! Functions:
//! 0x00 FeeV2, 0x01 GenesisMintV2, 0x02 PoWRewardV2, 0x03 TransferV2
//! 0x04 OtcSwapV2, 0x05 AuthTokenMintV2, 0x06 AuthTokenFreezeV2
//! 0x07 TokenMintV2, 0x08 BurnV2

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
use darkfi_money_v2_contract::{
    model::{
        MoneyAuthTokenFreezeParamsV1, MoneyAuthTokenMintParamsV1, MoneyBurnParamsV1,
        MoneyFeeParamsV1 as MoneyFeeParamsV2, MoneyGenesisMintParamsV1, MoneyPoWRewardParamsV1,
        MoneyTokenMintParamsV1, MoneyTransferParamsV1,
    },
    MoneyFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the MoneyV2 WASM contract using the Deployooor.
    pub async fn deploy_money_v2(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let money_v2_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed money_v2 contract: {:?}",
            money_v2_contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(money_v2_contract_id)
    }

    /// Create a `MoneyV2::TransferV2` transaction.
    pub async fn money_v2_transfer(
        &mut self,
        holder: &Holder,
        money_v2_contract_id: ContractId,
        sender: PublicKey,
        recipient: PublicKey,
        amount: u64,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, MoneyTransferParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let value_blind = ScalarBlind::random(&mut OsRng);
        let value_commit = pedersen_commitment_u64(amount, value_blind);

        let params = MoneyTransferParamsV1 {
            sender,
            recipient,
            amount,
            token_id,
            value_commit,
            value_blind,
            fee: 0,
        };

        let mut data = vec![MoneyFunction::TransferV2 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: money_v2_contract_id, data };

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

        Ok((tx, params, fee_params))
    }

    /// Execute a `MoneyV2::TransferV2` transaction.
    pub async fn execute_money_v2_transfer_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &MoneyTransferParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("money_v2::transfer", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `MoneyV2::TokenMintV2` transaction.
    pub async fn money_v2_token_mint(
        &mut self,
        holder: &Holder,
        money_v2_contract_id: ContractId,
        recipient: PublicKey,
        amount: u64,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, MoneyTokenMintParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let mint_blind = ScalarBlind::random(&mut OsRng);
        let token_blind = BaseBlind::random(&mut OsRng);

        let params = MoneyTokenMintParamsV1 {
            recipient,
            amount,
            token_id,
            mint_blind,
            token_blind,
            fee: 0,
        };

        let mut data = vec![MoneyFunction::TokenMintV2 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: money_v2_contract_id, data };

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

        Ok((tx, params, fee_params))
    }

    /// Execute a `MoneyV2::TokenMintV2` transaction.
    pub async fn execute_money_v2_token_mint_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &MoneyTokenMintParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("money_v2::token_mint", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `MoneyV2::BurnV2` transaction.
    pub async fn money_v2_burn(
        &mut self,
        holder: &Holder,
        money_v2_contract_id: ContractId,
        coin: pallas::Base,
        value: u64,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, MoneyBurnParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = MoneyBurnParamsV1 { coin, value, token_id, fee: 0 };

        let mut data = vec![MoneyFunction::BurnV2 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: money_v2_contract_id, data };

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

        Ok((tx, params, fee_params))
    }

    /// Execute a `MoneyV2::BurnV2` transaction.
    pub async fn execute_money_v2_burn_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &MoneyBurnParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("money_v2::burn", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `MoneyV2::AuthTokenMintV2` transaction.
    pub async fn money_v2_auth_token_mint(
        &mut self,
        holder: &Holder,
        money_v2_contract_id: ContractId,
        recipient: PublicKey,
        amount: u64,
        auth_sig: pallas::Base,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, MoneyAuthTokenMintParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = MoneyAuthTokenMintParamsV1 {
            recipient,
            amount,
            auth_sig,
            token_id,
            fee: 0,
        };

        let mut data = vec![MoneyFunction::AuthTokenMintV2 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: money_v2_contract_id, data };

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

        Ok((tx, params, fee_params))
    }

    /// Execute a `MoneyV2::AuthTokenMintV2` transaction.
    pub async fn execute_money_v2_auth_token_mint_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &MoneyAuthTokenMintParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("money_v2::auth_token_mint", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `MoneyV2::AuthTokenFreezeV2` transaction.
    pub async fn money_v2_auth_token_freeze(
        &mut self,
        holder: &Holder,
        money_v2_contract_id: ContractId,
        auth_sig: pallas::Base,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, MoneyAuthTokenFreezeParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = MoneyAuthTokenFreezeParamsV1 { auth_sig, token_id, fee: 0 };

        let mut data = vec![MoneyFunction::AuthTokenFreezeV2 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: money_v2_contract_id, data };

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

        Ok((tx, params, fee_params))
    }

    /// Execute a `MoneyV2::AuthTokenFreezeV2` transaction.
    pub async fn execute_money_v2_auth_token_freeze_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &MoneyAuthTokenFreezeParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("money_v2::auth_token_freeze", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}