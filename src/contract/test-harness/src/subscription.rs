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

//! Subscription contract test harness

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
use darkfi_subscription_contract::{
    model::{SubscribeParamsV1, CancelParamsV1, RenewParamsV1},
    SubscriptionFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Subscription WASM contract.
    pub async fn deploy_subscription(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed subscription contract: {:?}",
            contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(contract_id)
    }

    /// Create a `Subscription::SubscribeV1` transaction.
    pub async fn subscription_subscribe(
        &mut self,
        holder: &Holder,
        contract_id: ContractId,
        plan_id: u32,
        subscriber_pubkey: darkfi_sdk::crypto::PublicKey,
        commitment: pallas::Base,
        value_commit: pallas::Point,
        merkle_proof: Vec<pallas::Base>,
        merkle_root: pallas::Base,
        dao_escrow_bulla: Option<pallas::Base>,
        dao_membership_note: Option<pallas::Base>,
        dao_escrow_merkle_root: Option<pallas::Base>,
        dao_merkle_proof: Option<Vec<pallas::Base>>,
        dao_leaf_pos: Option<u32>,
        block_height: u32,
    ) -> Result<(Transaction, SubscribeParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SubscribeParamsV1 {
            plan_id,
            subscriber_pubkey,
            commitment,
            value_commit,
            merkle_proof,
            merkle_root,
            dao_escrow_bulla,
            dao_membership_note,
            dao_escrow_merkle_root,
            dao_merkle_proof,
            dao_leaf_pos,
        };

        let mut data = vec![SubscriptionFunction::SubscribeV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id, data };

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

    /// Execute a `Subscription::SubscribeV1` transaction.
    pub async fn execute_subscription_subscribe_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SubscribeParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("subscription::subscribe", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Subscription::CancelV1` transaction.
    pub async fn subscription_cancel(
        &mut self,
        holder: &Holder,
        contract_id: ContractId,
        subscription_id: pallas::Base,
        subscriber_secret: pallas::Base,
        spent_nullifier: pallas::Base,
        current_block: u64,
        recipient_pubkey: darkfi_sdk::crypto::PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, CancelParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CancelParamsV1 {
            subscription_id,
            subscriber_secret,
            spent_nullifier,
            current_block,
            recipient_pubkey,
        };

        let mut data = vec![SubscriptionFunction::CancelV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id, data };

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

    /// Execute a `Subscription::CancelV1` transaction.
    pub async fn execute_subscription_cancel_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CancelParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("subscription::cancel", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}