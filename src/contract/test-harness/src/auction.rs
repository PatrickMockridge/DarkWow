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

//! Auction contract test harness
//!
//! This module provides a test harness for the Auction contract,
//! a WASM-based privacy-preserving auction with escrow composition.
//!
//! Flow:
//! 1. Seller creates auction with CreateAuctionV1
//! 2. Bidders place bids with PlaceBidV1
//! 3. Seller closes auction with CloseAuctionV1
//! 4. Winner claims item with ClaimWinningsV1
//! 5. Seller settles to receive funds with SettleAuctionV1
//! 6. Outbid bidders get refunds with RefundBidV1

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    Result,
};
use darkfi_auction_contract::{
    model::{
        AuctionId, BidId, ClaimWinningsParamsV1, CloseAuctionParamsV1,
        CreateAuctionParamsV1, PlaceBidParamsV1, RefundBidParamsV1, SettleAuctionParamsV1,
    },
    AuctionFunction,
};
use darkfi_money_contract::{client::OwnCoin, model::MoneyFeeParamsV1};
use darkfi_sdk::{
    crypto::{poseidon_hash, ContractId, PublicKey},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use tracing::debug;

use super::{Holder, TestHarness};

impl TestHarness {
    /// Deploy the Auction WASM contract using the Deployooor.
    pub async fn deploy_auction(
        &mut self,
        holder: &Holder,
        wasm_bincode: Vec<u8>,
        block_height: u32,
    ) -> Result<ContractId> {
        let deploy_public = self.wallet(holder).contract_deploy_authority.public;

        let (tx, deploy_params, fee_params) =
            self.deploy_contract(holder, wasm_bincode, block_height).await?;

        let auction_contract_id = ContractId::derive_public(deploy_public);

        debug!(
            target: "test-harness",
            "Deployed auction contract: {:?}",
            auction_contract_id
        );

        self.execute_deploy_tx(holder, tx, &deploy_params, &fee_params, block_height, true)
            .await?;

        Ok(auction_contract_id)
    }

    /// Create a `Auction::CreateAuctionV1` transaction.
    pub async fn auction_create(
        &mut self,
        holder: &Holder,
        auction_contract_id: ContractId,
        seller_pubkey: PublicKey,
        item_commitment: pallas::Base,
        reserve_price: u64,
        token_id: pallas::Base,
        deadline_block: u64,
        seller_secret: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, CreateAuctionParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        // Derive auction ID
        let auction_id: AuctionId = poseidon_hash([
            seller_pubkey.x(),
            seller_pubkey.y(),
            item_commitment,
            pallas::Base::from(reserve_price),
            token_id,
            pallas::Base::from(deadline_block),
            seller_secret,
        ]);

        let seller_commitment = poseidon_hash([seller_pubkey.x(), seller_pubkey.y()]);

        let params = CreateAuctionParamsV1 {
            seller_pubkey,
            item_commitment,
            reserve_price,
            token_id,
            deadline_block,
            auction_id,
            seller_commitment,
            merkle_proof: vec![],
            merkle_root: pallas::Base::zero(),
        };

        let mut data = vec![AuctionFunction::CreateAuctionV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: auction_contract_id, data };

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

    /// Execute a `Auction::CreateAuctionV1` transaction.
    pub async fn execute_auction_create_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CreateAuctionParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("auction::create", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Auction::PlaceBidV1` transaction.
    pub async fn auction_place_bid(
        &mut self,
        holder: &Holder,
        auction_contract_id: ContractId,
        auction_id: AuctionId,
        bidder_pubkey: PublicKey,
        amount: u64,
        bid_nonce: pallas::Base,
        escrow_id: pallas::Base,
        current_high_bid: u64,
        block_height: u32,
    ) -> Result<(Transaction, PlaceBidParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let bid_id: BidId = poseidon_hash([
            auction_id,
            bidder_pubkey.x(),
            bidder_pubkey.y(),
            pallas::Base::from(amount),
            bid_nonce,
        ]);

        let params = PlaceBidParamsV1 {
            auction_id,
            bidder_pubkey,
            amount,
            bid_nonce,
            bid_id,
            escrow_id,
            current_high_bid,
        };

        let mut data = vec![AuctionFunction::PlaceBidV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: auction_contract_id, data };

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

    /// Execute a `Auction::PlaceBidV1` transaction.
    pub async fn execute_auction_place_bid_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &PlaceBidParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("auction::place_bid", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Auction::CloseAuctionV1` transaction.
    pub async fn auction_close(
        &mut self,
        holder: &Holder,
        auction_contract_id: ContractId,
        auction_id: AuctionId,
        winner_bid_id: BidId,
        seller_pubkey: PublicKey,
        block_height: u32,
    ) -> Result<(Transaction, CloseAuctionParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = CloseAuctionParamsV1 {
            auction_id,
            winner_bid_id,
            seller_pubkey,
            current_block: block_height as u64,
        };

        let mut data = vec![AuctionFunction::CloseAuctionV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: auction_contract_id, data };

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

    /// Execute a `Auction::CloseAuctionV1` transaction.
    pub async fn execute_auction_close_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &CloseAuctionParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("auction::close", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Auction::ClaimWinningsV1` transaction.
    pub async fn auction_claim_winnings(
        &mut self,
        holder: &Holder,
        auction_contract_id: ContractId,
        auction_id: AuctionId,
        winner_bid_id: BidId,
        winner_pubkey: PublicKey,
        winner_secret: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, ClaimWinningsParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = ClaimWinningsParamsV1 {
            auction_id,
            winner_bid_id,
            winner_pubkey,
            winner_secret,
        };

        let mut data = vec![AuctionFunction::ClaimWinningsV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: auction_contract_id, data };

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

    /// Execute a `Auction::ClaimWinningsV1` transaction.
    pub async fn execute_auction_claim_winnings_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &ClaimWinningsParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("auction::claim_winnings", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Auction::SettleAuctionV1` transaction.
    pub async fn auction_settle(
        &mut self,
        holder: &Holder,
        auction_contract_id: ContractId,
        auction_id: AuctionId,
        seller_pubkey: PublicKey,
        highest_bid_amount: u64,
        settlement_nullifier: pallas::Base,
        seller_secret: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, SettleAuctionParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = SettleAuctionParamsV1 {
            auction_id,
            seller_pubkey,
            highest_bid_amount,
            settlement_nullifier,
            seller_secret,
        };

        let mut data = vec![AuctionFunction::SettleAuctionV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: auction_contract_id, data };

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

    /// Execute a `Auction::SettleAuctionV1` transaction.
    pub async fn execute_auction_settle_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &SettleAuctionParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("auction::settle", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }

    /// Create a `Auction::RefundBidV1` transaction.
    pub async fn auction_refund_bid(
        &mut self,
        holder: &Holder,
        auction_contract_id: ContractId,
        bid_id: BidId,
        bidder_pubkey: PublicKey,
        refund_nullifier: pallas::Base,
        bidder_secret: pallas::Base,
        block_height: u32,
    ) -> Result<(Transaction, RefundBidParamsV1, Option<MoneyFeeParamsV1>)> {
        let wallet = self.wallet(holder);
        let holder_secret = wallet.keypair.secret;

        let params = RefundBidParamsV1 {
            bid_id,
            bidder_pubkey,
            refund_nullifier,
            bidder_secret,
        };

        let mut data = vec![AuctionFunction::RefundBidV1 as u8];
        params.encode(&mut data)?;
        let call = ContractCall { contract_id: auction_contract_id, data };

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

    /// Execute a `Auction::RefundBidV1` transaction.
    pub async fn execute_auction_refund_bid_tx(
        &mut self,
        holder: &Holder,
        tx: Transaction,
        _params: &RefundBidParamsV1,
        fee_params: &Option<MoneyFeeParamsV1>,
        block_height: u32,
        append: bool,
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.wallet_mut(holder);

        wallet.add_transaction("auction::refund_bid", tx, block_height).await?;

        if !append {
            return Ok(vec![]);
        }

        Ok(wallet.process_fee(fee_params, holder))
    }
}