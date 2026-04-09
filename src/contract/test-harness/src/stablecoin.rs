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

//! Stablecoin (CDP) contract test harness
//!
//! This module provides a test harness for the Stablecoin contract,
//! a WASM-based collateralized debt position (CDP) contract.
//!
//! Note: Since stablecoin is a WASM contract (not native), it must be
//! deployed before use. Use `deploy_stablecoin()` to deploy and
//! `stablecoin_initialize()` to initialize the CDP engine.

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, pasta_prelude::*, ContractId, ScalarBlind},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;
use darkfi_stablecoin_contract::{
    model::{CollateralType, DepositCollateralParams, UpdateConfigParams},
    StablecoinFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Stablecoin WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the stablecoin contract.
    /// After deployment, use `stablecoin_initialize()` to initialize the CDP engine.
    pub async fn deploy_stablecoin(
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

        // Derive the stablecoin contract ID from the deploy public key
        let stablecoin_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed stablecoin contract: {:?}",
            stablecoin_contract_id
        );

        // Execute the deploy transaction
        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(stablecoin_contract_id)
    }

    /// Create a `Stablecoin::InitializeV1` transaction.
    ///
    /// This initializes the CDP engine with configuration parameters.
    /// Must be called after `deploy_stablecoin()`.
    pub async fn stablecoin_initialize(
        &mut self,
        holder: &Holder,
        stablecoin_contract_id: ContractId,
        min_collateralization_ratio: u64,
        liquidation_threshold: u64,
        liquidation_penalty: u64,
        base_rate: u64,
        pi_kp: i64,
        pi_ki: i64,
        twap_window: u64,
        price_deviation_threshold: u64,
        block_height: u32,
    ) -> Result<(Transaction, UpdateConfigParams, Option<MoneyFeeParamsV1>)> {
        // Get secret key before mutable borrow
        let holder_secret = self.wallet(holder).keypair.secret;

        let params = UpdateConfigParams {
            min_collateralization_ratio,
            liquidation_threshold,
            liquidation_penalty,
            base_rate,
            pi_kp,
            pi_ki,
            twap_window,
            price_deviation_threshold,
        };

        // Build contract call data
        let mut data = vec![StablecoinFunction::InitializeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: stablecoin_contract_id, data };

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

    /// Execute a `Stablecoin::InitializeV1` transaction.
    pub async fn execute_stablecoin_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &UpdateConfigParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("stablecoin::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Stablecoin::OpenPositionV1` transaction.
    ///
    /// Opens a new collateralized debt position by depositing collateral
    /// and minting stablecoin against it.
    pub async fn stablecoin_open_position(
        &mut self,
        holder: &Holder,
        stablecoin_contract_id: ContractId,
        collateral_amount: u64,
        collateral_type: CollateralType,
        block_height: u32,
    ) -> Result<(Transaction, DepositCollateralParams, Option<MoneyFeeParamsV1>)> {
        // Get necessary data before mutable borrow
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;
        let holder_public = wallet.keypair.public;
        let collateral_blind = darkfi_sdk::crypto::ScalarBlind::random(&mut OsRng);

        // Compute Pedersen commitment for collateral
        let collateral_commit = pedersen_commitment_u64(collateral_amount, collateral_blind);
        let collateral_coords = collateral_commit.to_affine().coordinates().unwrap();

        // Compute position commitment: poseidon_hash(collateral_x, pub_x, pub_y)
        // Note: In the pooled model, debt is tracked separately
        let position_commitment =
            poseidon_hash([*collateral_coords.x(), holder_public.x(), holder_public.y()]);

        let params = DepositCollateralParams {
            deposit_commitment: position_commitment.into(),
            collateral_amount,
            collateral_type,
            proof: vec![], // ZK proof would be generated by the client
            fee: 0,        // Fee handled separately
        };

        // Build contract call data
        let mut data = vec![StablecoinFunction::OpenPositionV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: stablecoin_contract_id, data };

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

    /// Execute a `Stablecoin::OpenPositionV1` transaction.
    pub async fn execute_stablecoin_open_position_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &DepositCollateralParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("stablecoin::open_position", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}