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

//! Attestation contract test harness
//!
//! This module provides a test harness for the Attestation contract,
//! a WASM-based generalized attestation and claims system.

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::ContractId,
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use tracing::debug;
use darkfi_attestation_contract::{
    model::{
        CreateAttestationParamsV1, CreateClaimParamsV1, RevokeAttestationParamsV1,
        ExpireAttestationParamsV1, Predicate,
    },
    AttestationFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Attestation WASM contract using the Deployooor.
    pub async fn deploy_attestation(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let attestation_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed attestation contract: {:?}",
            attestation_contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(attestation_contract_id)
    }

    /// Create a `Attestation::CreateAttestationV1` transaction.
    pub async fn attestation_create(
        &mut self,
        holder: &Holder,
        attestation_contract_id: ContractId,
        attestation_id: pallas::Base,
        attestor_pub_x: pallas::Base,
        attestor_pub_y: pallas::Base,
        claim_type: Predicate,
        claim_data: Vec<pallas::Base>,
        metadata: Vec<u8>,
        expires_at: Option<u64>,
        block_height: u32,
    ) -> Result<(Transaction, CreateAttestationParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateAttestationParamsV1 {
            proof: vec![],
            attestation_id,
            attestor_pub_x,
            attestor_pub_y,
            claim_type,
            claim_data,
            metadata,
            expires_at,
        };

        let mut data = vec![AttestationFunction::CreateAttestationV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: attestation_contract_id, data };

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

    /// Execute a `Attestation::CreateAttestationV1` transaction.
    pub async fn execute_attestation_create_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateAttestationParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("attestation::create", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Attestation::CreateClaimV1` transaction.
    pub async fn attestation_create_claim(
        &mut self,
        holder: &Holder,
        attestation_contract_id: ContractId,
        claim_id: pallas::Base,
        attestation_id: pallas::Base,
        claimant_pub_x: pallas::Base,
        claimant_pub_y: pallas::Base,
        predicate: Predicate,
        evidence_commitment: Vec<u8>,
        revealed_result: Vec<u8>,
        block_height: u32,
    ) -> Result<(Transaction, CreateClaimParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateClaimParamsV1 {
            proof: vec![],
            claim_id,
            attestation_id,
            claimant_pub_x,
            claimant_pub_y,
            predicate,
            evidence_commitment,
            revealed_result,
        };

        let mut data = vec![AttestationFunction::CreateClaimV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: attestation_contract_id, data };

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

    /// Execute a `Attestation::CreateClaimV1` transaction.
    pub async fn execute_attestation_create_claim_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateClaimParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("attestation::create_claim", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}