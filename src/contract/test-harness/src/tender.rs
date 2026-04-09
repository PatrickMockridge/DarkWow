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

//! Tender contract test harness

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
use darkfi_tender_contract::{
    model::{CreateTenderParamsV1, SubmitBidParamsV1, RevealBidParamsV1, SelectWinnerParamsV1},
    TenderFunction,
};

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Tender WASM contract.
    pub async fn deploy_tender(
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
            "Deployed tender contract: {:?}",
            contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(contract_id)
    }

    /// Create a `Tender::CreateTenderV1` transaction.
    pub async fn tender_create(
        &mut self,
        holder: &Holder,
        contract_id: ContractId,
        requester_pub: pallas::Base,
        tender_hash: pallas::Base,
        deposit_amount: u64,
        token_id: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, CreateTenderParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CreateTenderParamsV1 {
            requester_pub,
            tender_hash,
            deposit_amount,
            token_id,
        };

        let mut data = vec![TenderFunction::CreateTenderV1 as u8];
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

    /// Execute a `Tender::CreateTenderV1` transaction.
    pub async fn execute_tender_create_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateTenderParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("tender::create", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Tender::SubmitBidV1` transaction.
    pub async fn tender_submit_bid(
        &mut self,
        holder: &Holder,
        contract_id: ContractId,
        tender_id: pallas::Base,
        bidder_pub: pallas::Base,
        bid_hash: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, SubmitBidParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SubmitBidParamsV1 {
            tender_id,
            bidder_pub,
            bid_hash,
        };

        let mut data = vec![TenderFunction::SubmitBidV1 as u8];
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

    /// Execute a `Tender::SubmitBidV1` transaction.
    pub async fn execute_tender_submit_bid_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SubmitBidParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("tender::submit_bid", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}