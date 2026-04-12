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

//! NativeToken contract test harness
//!
//! This module provides a test harness for the NativeToken contract,
//! a CONSENSUS-FIRST native token design.
//!
//! Functions:
//! 0x00 FeeV1, 0x01 GenesisMintV1, 0x02 PoWRewardV1, 0x03 TransferV1
//! 0x04 SpendV1, 0x05 MeltV1

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, BaseBlind, ContractId, PublicKey, ScalarBlind},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;
use darkfi_native_token_contract::{
    model::{
        ClearInput, Coin, FeeParamsV1, FeeUpdateV1, GenesisMintParamsV1, GenesisMintUpdateV1,
        Input, MeltParamsV1, MeltUpdateV1, Output, PoWRewardParamsV1, PoWRewardUpdateV1,
        SpendParamsV1, SpendUpdateV1, TransferParamsV1, TransferUpdateV1,
    },
    NativeTokenFunction,
};

use super::{Holder, TestHarness};

/// OwnCoin for NativeToken - represents a coin owned by the user
#[derive(Debug, Clone)]
pub struct OwnCoinNativeToken {
    pub coin: Coin,
    pub value: u64,
    pub token_id: pallas::Base,
    pub secret: pallas::Base,
    pub public: PublicKey,
}

impl TestHarness {
    /// Deploy the NativeToken WASM contract using the Deployooor.
    pub async fn deploy_native_token(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let native_token_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed native_token contract: {:?}",
            native_token_contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(native_token_contract_id)
    }

    /// Create a `NativeToken::TransferV1` transaction.
    pub async fn native_token_transfer(
        &mut self,
        holder: &Holder,
        native_token_contract_id: ContractId,
        inputs: Vec<Input>,
        outputs: Vec<Output>,
        block_height: u32,
    ) -> Result<(Transaction, TransferParamsV1, Option<FeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = TransferParamsV1 { inputs: inputs.clone(), outputs: outputs.clone() };

        let mut data = vec![NativeTokenFunction::TransferV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: native_token_contract_id, data };

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

    /// Execute a `NativeToken::TransferV1` transaction.
    pub async fn execute_native_token_transfer_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &TransferParamsV1,
        fee_params: &Option<FeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoinNativeToken>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("native_token::transfer", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_native_token_outputs())
    }

    /// Create a `NativeToken::GenesisMintV1` transaction.
    pub async fn native_token_genesis_mint(
        &mut self,
        holder: &Holder,
        native_token_contract_id: ContractId,
        input: ClearInput,
        outputs: Vec<Output>,
        block_height: u32,
    ) -> Result<(Transaction, GenesisMintParamsV1, Option<FeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = GenesisMintParamsV1 { input: input.clone(), outputs: outputs.clone() };

        let mut data = vec![NativeTokenFunction::GenesisMintV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: native_token_contract_id, data };

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

    /// Execute a `NativeToken::GenesisMintV1` transaction.
    pub async fn execute_native_token_genesis_mint_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &GenesisMintParamsV1,
        fee_params: &Option<FeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoinNativeToken>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("native_token::genesis_mint", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_native_token_outputs())
    }

    /// Create a `NativeToken::PoWRewardV1` transaction.
    pub async fn native_token_pow_reward(
        &mut self,
        holder: &Holder,
        native_token_contract_id: ContractId,
        input: ClearInput,
        output: Output,
        block_height: u32,
    ) -> Result<(Transaction, PoWRewardParamsV1, Option<FeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = PoWRewardParamsV1 { input: input.clone(), output: output.clone() };

        let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: native_token_contract_id, data };

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

    /// Execute a `NativeToken::PoWRewardV1` transaction.
    pub async fn execute_native_token_pow_reward_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &PoWRewardParamsV1,
        fee_params: &Option<FeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoinNativeToken>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("native_token::pow_reward", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_native_token_outputs())
    }

    /// Create a `NativeToken::FeeV1` transaction.
    pub async fn native_token_fee(
        &mut self,
        holder: &Holder,
        native_token_contract_id: ContractId,
        input: Input,
        output: Output,
        fee_value_blind: pallas::Scalar,
        fee_token_blind: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, FeeParamsV1)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = FeeParamsV1 {
            input: input.clone(),
            output: output.clone(),
            fee_value_blind,
            fee_token_blind,
        };

        let mut data = vec![NativeTokenFunction::FeeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: native_token_contract_id, data };

        let tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: vec![] }, vec![])?;

        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[holder_secret])?;
        tx.signatures = vec![sigs];

        Ok((tx, params))
    }

    /// Execute a `NativeToken::FeeV1` transaction.
    pub async fn execute_native_token_fee_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &FeeParamsV1,
        block_height: u32,
    ) -> Result<()> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("native_token::fee", tx, block_height).await?;

        Ok(())
    }
}