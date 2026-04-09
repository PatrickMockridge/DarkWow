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

//! DEX contract test harness
//!
//! This module provides a test harness for the DEX (decentralized exchange)
//! contract, a WASM-based atomic swap contract.

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_dex_contract::{
    model::{
        AcceptSwapParams, CancelSwapParams, CreateSwapParams, ExecuteSwapFeeParams,
        ExecuteSwapParams, ExecuteSwapSlippageParams, InitializeParams,
        SetTransparencyLevelParams, TransparencyConfig, TransparencyLevel, UpdateConfigParams,
    },
    DexFunction,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{ContractId, IntentCommitment, IntentNullifier, PublicKey},
    ContractCall,
};
use darkfi_serial::Encodable;
use tracing::debug;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the DEX WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the DEX contract.
    pub async fn deploy_dex(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let dex_contract_id = ContractId::derive_public(deploy_public);

        debug!(target: "test-harness", "Deployed DEX contract: {:?}", dex_contract_id);

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(dex_contract_id)
    }

    /// Create a `Dex::InitializeV1` transaction.
    pub async fn dex_initialize(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        timeout: u32,
        fee: u64,
        trusted_money_merkle_root: [u8; 32],
        transparency_config: TransparencyConfig,
        block_height: u32,
    ) -> Result<(Transaction, InitializeParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = InitializeParams { timeout, fee, trusted_money_merkle_root, transparency_config };

        let mut data = vec![DexFunction::InitializeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::InitializeV1` transaction.
    pub async fn execute_dex_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &InitializeParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::CreateSwapV1` transaction.
    ///
    /// Note: The ZK proof for CreateSwap is not yet compiled. This creates
    /// the transaction with an empty proof that must be generated externally.
    pub async fn dex_create_swap(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        swap_id: [u8; 32],
        offer_token: [u8; 32],
        offer_amount: u64,
        request_token: [u8; 32],
        request_amount: u64,
        lock_commitment: IntentCommitment,
        nullifier: IntentNullifier,
        lock_proof: Vec<[u8; 32]>,
        signature_public: PublicKey,
        fee: u64,
        open_execution: bool,
        block_height: u32,
    ) -> Result<(Transaction, CreateSwapParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateSwapParams {
            swap_id,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            lock_commitment,
            nullifier,
            lock_proof,
            signature_public,
            fee,
            open_execution,
        };

        let mut data = vec![DexFunction::CreateSwapV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::CreateSwapV1` transaction.
    pub async fn execute_dex_create_swap_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateSwapParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::create_swap", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::AcceptSwapV1` transaction.
    ///
    /// Note: The ZK proof for AcceptSwap is not yet compiled. This creates
    /// the transaction with an empty proof that must be generated externally.
    pub async fn dex_accept_swap(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        swap_id: [u8; 32],
        lock_commitment: IntentCommitment,
        nullifier: IntentNullifier,
        lock_proof: Vec<[u8; 32]>,
        signature_public: PublicKey,
        fee: u64,
        immediate_execute: bool,
        block_height: u32,
    ) -> Result<(Transaction, AcceptSwapParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = AcceptSwapParams {
            swap_id,
            lock_commitment,
            nullifier,
            lock_proof,
            signature_public,
            fee,
            immediate_execute,
        };

        let mut data = vec![DexFunction::AcceptSwapV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::AcceptSwapV1` transaction.
    pub async fn execute_dex_accept_swap_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &AcceptSwapParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::accept_swap", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::ExecuteSwapV1` transaction.
    ///
    /// This is the only DEX function with a compiled ZK proof.
    pub async fn dex_execute_swap(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        swap_id: [u8; 32],
        alice_secret: [u8; 32],
        bob_secret: [u8; 32],
        alice_lock: IntentCommitment,
        bob_lock: IntentCommitment,
        alice_nullifier: IntentNullifier,
        bob_nullifier: IntentNullifier,
        proof: Vec<u8>,
        fee: u64,
        block_height: u32,
    ) -> Result<(Transaction, ExecuteSwapParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ExecuteSwapParams {
            swap_id,
            alice_secret,
            bob_secret,
            alice_lock,
            bob_lock,
            alice_nullifier,
            bob_nullifier,
            proof,
            fee,
        };

        let mut data = vec![DexFunction::ExecuteSwapV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::ExecuteSwapV1` transaction.
    pub async fn execute_dex_execute_swap_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ExecuteSwapParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::execute_swap", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::CancelSwapV1` transaction.
    ///
    /// Note: The ZK proof for CancelSwap is not yet compiled. This creates
    /// the transaction with an empty proof that must be generated externally.
    pub async fn dex_cancel_swap(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        swap_id: [u8; 32],
        secret: [u8; 32],
        nullifier: IntentNullifier,
        proof: Vec<u8>,
        fee: u64,
        block_height: u32,
    ) -> Result<(Transaction, CancelSwapParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CancelSwapParams { swap_id, secret, nullifier, proof, fee };

        let mut data = vec![DexFunction::CancelSwapV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::CancelSwapV1` transaction.
    pub async fn execute_dex_cancel_swap_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CancelSwapParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::cancel_swap", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::UpdateConfigV1` transaction.
    pub async fn dex_update_config(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        timeout: u32,
        fee: u64,
        block_height: u32,
    ) -> Result<(Transaction, UpdateConfigParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = UpdateConfigParams { timeout, fee };

        let mut data = vec![DexFunction::UpdateConfigV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::UpdateConfigV1` transaction.
    pub async fn execute_dex_update_config_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &UpdateConfigParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::update_config", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::SetTransparencyLevelV1` transaction.
    pub async fn dex_set_transparency_level(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        level: TransparencyLevel,
        block_height: u32,
    ) -> Result<(Transaction, SetTransparencyLevelParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SetTransparencyLevelParams { level };

        let mut data = vec![DexFunction::SetTransparencyLevelV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::SetTransparencyLevelV1` transaction.
    pub async fn execute_dex_set_transparency_level_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SetTransparencyLevelParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::set_transparency_level", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::ExecuteSwapFeeV1` transaction.
    ///
    /// Executes a swap with fee deduction.
    /// Fee calculation: fee = fill_amount * fee_bps / 10000
    pub async fn dex_execute_swap_fee(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        swap_id: [u8; 32],
        alice_secret: [u8; 32],
        bob_secret: [u8; 32],
        alice_lock: IntentCommitment,
        bob_lock: IntentCommitment,
        alice_nullifier: IntentNullifier,
        bob_nullifier: IntentNullifier,
        fee_bps: u64,
        proof: Vec<u8>,
        fee: u64,
        block_height: u32,
    ) -> Result<(Transaction, ExecuteSwapFeeParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ExecuteSwapFeeParams {
            swap_id,
            alice_secret,
            bob_secret,
            alice_lock,
            bob_lock,
            alice_nullifier,
            bob_nullifier,
            fee_bps,
            proof,
            fee,
        };

        let mut data = vec![DexFunction::ExecuteSwapFeeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::ExecuteSwapFeeV1` transaction.
    pub async fn execute_dex_execute_swap_fee_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ExecuteSwapFeeParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::execute_swap_fee", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Dex::ExecuteSwapSlippageV1` transaction.
    ///
    /// Executes a swap with slippage tolerance.
    /// Slippage tolerance: received >= min_expected * (1 - slippage_bps / 10000)
    pub async fn dex_execute_swap_slippage(
        &mut self,
        holder: &Holder,
        dex_contract_id: ContractId,
        swap_id: [u8; 32],
        alice_secret: [u8; 32],
        bob_secret: [u8; 32],
        alice_lock: IntentCommitment,
        bob_lock: IntentCommitment,
        alice_nullifier: IntentNullifier,
        bob_nullifier: IntentNullifier,
        slippage_bps: u64,
        proof: Vec<u8>,
        fee: u64,
        block_height: u32,
    ) -> Result<(Transaction, ExecuteSwapSlippageParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ExecuteSwapSlippageParams {
            swap_id,
            alice_secret,
            bob_secret,
            alice_lock,
            bob_lock,
            alice_nullifier,
            bob_nullifier,
            slippage_bps,
            proof,
            fee,
        };

        let mut data = vec![DexFunction::ExecuteSwapSlippageV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: dex_contract_id, data };

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

    /// Execute a `Dex::ExecuteSwapSlippageV1` transaction.
    pub async fn execute_dex_execute_swap_slippage_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ExecuteSwapSlippageParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("dex::execute_swap_slippage", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}