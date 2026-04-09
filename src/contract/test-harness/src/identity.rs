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

//! Identity contract test harness
//!
//! This module provides a test harness for the Identity contract,
//! a WASM-based credential and claim management contract.

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_identity_contract::{
    model::{
        CreateClaimDAGParams, CreateClaimParams, CreateClaimParamsL1, InitializeParams,
        IssueCapabilityParams, IssueCredentialParams, RegisterCapabilityParams,
        RevokeCapabilityParams, RevokeCredentialParams, VerifyCapabilityParams,
        VerifyClaimParams,
    },
    IdentityFunction,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, ContractId, IntentCommitment, IntentNullifier},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::debug;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Identity WASM contract using the Deployooor.
    ///
    /// Returns the derived [`ContractId`] for the identity contract.
    pub async fn deploy_identity(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let identity_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed identity contract: {:?}",
            identity_contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(identity_contract_id)
    }

    /// Create an `Identity::InitializeV1` transaction.
    pub async fn identity_initialize(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        version: u32,
        block_height: u32,
    ) -> Result<(Transaction, InitializeParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = InitializeParams { version };

        let mut data = vec![IdentityFunction::InitializeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::InitializeV1` transaction.
    pub async fn execute_identity_initialize_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &InitializeParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::initialize", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::IssueCredentialV1` transaction.
    pub async fn identity_issue_credential(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        issuer_pub: [u8; 32],
        holder_pub: [u8; 32],
        schema_hash: [u8; 32],
        encrypted_attributes: Vec<u8>,
        commitment: IntentCommitment,
        nullifier: IntentNullifier,
        issued_at: u64,
        expires_at: u64,
        block_height: u32,
    ) -> Result<(Transaction, IssueCredentialParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = IssueCredentialParams {
            issuer_pub,
            holder_pub,
            schema_hash,
            encrypted_attributes,
            commitment,
            nullifier,
            issued_at,
            expires_at,
            proof: vec![],
            fee: 0,
        };

        let mut data = vec![IdentityFunction::IssueCredentialV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::IssueCredentialV1` transaction.
    pub async fn execute_identity_issue_credential_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &IssueCredentialParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::issue_credential", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::RevokeCredentialV1` transaction.
    pub async fn identity_revoke_credential(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        issuer_sig: Vec<u8>,
        nullifier: IntentNullifier,
        reason: Vec<u8>,
        block_height: u32,
    ) -> Result<(Transaction, RevokeCredentialParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = RevokeCredentialParams { issuer_sig, nullifier, reason, fee: 0 };

        let mut data = vec![IdentityFunction::RevokeCredentialV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::RevokeCredentialV1` transaction.
    pub async fn execute_identity_revoke_credential_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &RevokeCredentialParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::revoke_credential", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::CreateClaimV1` transaction.
    pub async fn identity_create_claim(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        nullifier: IntentNullifier,
        claim_type: Vec<u8>,
        predicate: Vec<u8>,
        revealed_attributes: Vec<Vec<u8>>,
        block_height: u32,
    ) -> Result<(Transaction, CreateClaimParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateClaimParams {
            nullifier,
            claim_type,
            predicate,
            revealed_attributes,
            proof: vec![],
            fee: 0,
        };

        let mut data = vec![IdentityFunction::CreateClaimV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::CreateClaimV1` transaction.
    pub async fn execute_identity_create_claim_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateClaimParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::create_claim", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::CreateClaimV1L1` transaction (Level 1 selective disclosure).
    pub async fn identity_create_claim_l1(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        nullifier: IntentNullifier,
        claim_type: Vec<u8>,
        predicate: Vec<u8>,
        revealed_attributes: Vec<Vec<u8>>,
        predicate_result: u8,
        block_height: u32,
    ) -> Result<(Transaction, CreateClaimParamsL1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateClaimParamsL1 {
            nullifier,
            claim_type,
            predicate,
            revealed_attributes,
            proof: vec![],
            predicate_result,
            fee: 0,
        };

        let mut data = vec![IdentityFunction::CreateClaimV1L1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::CreateClaimV1L1` transaction.
    pub async fn execute_identity_create_claim_l1_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateClaimParamsL1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::create_claim_l1", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::RegisterCapabilityV1` transaction.
    pub async fn identity_register_capability(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        name: Vec<u8>,
        credential_requirement: darkfi_identity_contract::model::CredentialRequirement,
        max_holders: Option<u64>,
        block_height: u32,
    ) -> Result<(Transaction, RegisterCapabilityParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = RegisterCapabilityParams { name, credential_requirement, max_holders, fee: 0 };

        let mut data = vec![IdentityFunction::RegisterCapabilityV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::RegisterCapabilityV1` transaction.
    pub async fn execute_identity_register_capability_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &RegisterCapabilityParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::register_capability", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::IssueCapabilityV1` transaction.
    pub async fn identity_issue_capability(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        capability_id: [u8; 32],
        holder_pub: [u8; 32],
        credential_nullifier: IntentNullifier,
        issuer_sig: Vec<u8>,
        block_height: u32,
    ) -> Result<(Transaction, IssueCapabilityParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = IssueCapabilityParams {
            capability_id,
            holder_pub,
            credential_nullifier,
            proof: vec![],
            issuer_sig,
            fee: 0,
        };

        let mut data = vec![IdentityFunction::IssueCapabilityV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::IssueCapabilityV1` transaction.
    pub async fn execute_identity_issue_capability_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &IssueCapabilityParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::issue_capability", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::VerifyCapabilityV1` transaction.
    pub async fn identity_verify_capability(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        capability_proof: darkfi_identity_contract::model::CapabilityProof,
        verifier_pub: [u8; 32],
        block_height: u32,
    ) -> Result<(Transaction, VerifyCapabilityParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = VerifyCapabilityParams { capability_proof, verifier_pub, fee: 0 };

        let mut data = vec![IdentityFunction::VerifyCapabilityV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::VerifyCapabilityV1` transaction.
    pub async fn execute_identity_verify_capability_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &VerifyCapabilityParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::verify_capability", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create an `Identity::CreateClaimDAGV1` transaction.
    pub async fn identity_create_claim_dag(
        &mut self,
        holder: &Holder,
        identity_contract_id: ContractId,
        dag_id: [u8; 32],
        path_index: u32,
        credentials: Vec<darkfi_identity_contract::model::DAGCredential>,
        predicate_result: u8,
        block_height: u32,
    ) -> Result<(Transaction, CreateClaimDAGParams, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateClaimDAGParams {
            dag_id,
            path_index,
            credentials,
            proof: vec![],
            predicate_result,
            fee: 0,
        };

        let mut data = vec![IdentityFunction::CreateClaimDAGV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: identity_contract_id, data };

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

    /// Execute an `Identity::CreateClaimDAGV1` transaction.
    pub async fn execute_identity_create_claim_dag_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateClaimDAGParams,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("identity::create_claim_dag", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}