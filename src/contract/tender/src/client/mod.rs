/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! Tender contract client API
//!
//! This module provides builder structs for constructing tender contract calls.
//! Also includes ZK proof generation modules for circuit verification.

//! ZK proof client modules
pub mod zkbins;

pub mod create_tender;
pub mod reveal_bid;
pub mod select_winner;
pub mod submit_bid;
pub mod submit_bid_with_capability;

use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};

use crate::model::{
    CancelTenderParamsV1, CloseTenderParamsV1, CreateTenderParamsV1, RejectBidParamsV1,
    RevealBidParamsV1, SelectWinnerParamsV1, SubmitBidParamsV1,
};

/// The transaction binding every tender circuit instances: `poseidon_hash([3, tx_commitment, tx_nonce])`
/// (`OBL-C78`).
///
/// **One function, so the three sides cannot disagree.** The circuit derives this value from two
/// witnesses and constrains its public input to it (`create_tender.zk:74-75` and the same pair in the
/// other four); the client's proof builder must instance the *same* value; and the contract's metadata
/// must publish the same value again, from the pair carried in the call. The five clients each used to
/// instance a literal `Base::zero()` here, which no circuit ever derives for any input — so every tender
/// proof failed verification against its own circuit, which is what `invalid proof: call[0] namespace
/// 'CreateTenderV2'` was. A caller that wants a real binding sets `tx_commitment`/`tx_nonce` on the call
/// data and passes the same pair to the params builder, and both sides derive the binding through this.
pub fn tx_binding_of(tx_commitment: &pallas::Base, tx_nonce: &pallas::Base) -> pallas::Base {
    dwow_sdk::crypto::poseidon_hash([
        dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING,
        *tx_commitment,
        *tx_nonce,
    ])
}

/// Builder for CreateTenderV1 params
#[derive(Default)]
pub struct CreateTenderBuilder {
    tender_id: Option<pallas::Base>,
    requester_pub_x: Option<pallas::Base>,
    requester_pub_y: Option<pallas::Base>,
    title: Option<String>,
    specification: Option<pallas::Base>,
    requirement_commitment: Option<pallas::Base>,
    min_bid: Option<u64>,
    max_bid: Option<u64>,
    bid_deadline: Option<u64>,
    reveal_deadline: Option<u64>,
    delivery_deadline: Option<u64>,
    /// The transaction the proof is bound to (`OBL-C78`); `None` means the zero pair, which is what
    /// `to_witnesses` defaults to as well. Set both through `tx_pair` so the params and the proof
    /// cannot disagree.
    tx_commitment: Option<pallas::Base>,
    tx_nonce: Option<pallas::Base>,
}

impl CreateTenderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn requester_pubkey(mut self, pubkey: PublicKey) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (x, y) = pubkey.xy().expect("pk not identity");
        self.requester_pub_x = Some(x);
        self.requester_pub_y = Some(y);
        self
    }

    pub fn title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn specification(mut self, spec: pallas::Base) -> Self {
        self.specification = Some(spec);
        self
    }

    pub fn requirement_commitment(mut self, commitment: pallas::Base) -> Self {
        self.requirement_commitment = Some(commitment);
        self
    }

    pub fn min_bid(mut self, amount: u64) -> Self {
        self.min_bid = Some(amount);
        self
    }

    pub fn max_bid(mut self, amount: u64) -> Self {
        self.max_bid = Some(amount);
        self
    }

    pub fn bid_deadline(mut self, block: u64) -> Self {
        self.bid_deadline = Some(block);
        self
    }

    pub fn reveal_deadline(mut self, block: u64) -> Self {
        self.reveal_deadline = Some(block);
        self
    }

    pub fn delivery_deadline(mut self, block: u64) -> Self {
        self.delivery_deadline = Some(block);
        self
    }

    /// Bind the proof to a transaction: the pair `to_witnesses` must use, so params and proof agree
    /// (`OBL-C78`). Without it both default to zero, which is what the heavyweight fixtures use.
    pub fn tx_pair(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        self.tx_commitment = Some(tx_commitment);
        self.tx_nonce = Some(tx_nonce);
        self
    }

    pub fn build(self) -> Result<CreateTenderParamsV1, &'static str> {
        Ok(CreateTenderParamsV1 {
            proof: vec![],
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            requester_pub_x: self.requester_pub_x.ok_or("requester_pub_x not set")?,
            requester_pub_y: self.requester_pub_y.ok_or("requester_pub_y not set")?,
            title: self.title.ok_or("title not set")?,
            specification: self.specification.ok_or("specification not set")?,
            attestation_id: self.requirement_commitment.ok_or("attestation_id not set")?,
            min_bid: self.min_bid.ok_or("min_bid not set")?,
            max_bid: self.max_bid.ok_or("max_bid not set")?,
            bid_deadline: self.bid_deadline.ok_or("bid_deadline not set")?,
            reveal_deadline: self.reveal_deadline.ok_or("reveal_deadline not set")?,
            delivery_deadline: self.delivery_deadline.ok_or("delivery_deadline not set")?,
            tx_binding: tx_binding_of(
                &self.tx_commitment.unwrap_or_else(pallas::Base::zero),
                &self.tx_nonce.unwrap_or_else(pallas::Base::zero),
            ),
            tx_nonce: self.tx_nonce.unwrap_or_else(pallas::Base::zero),
        })
    }
}

/// Builder for SubmitBidV1 params
#[derive(Default)]
pub struct SubmitBidBuilder {
    tender_id: Option<pallas::Base>,
    bid_id: Option<pallas::Base>,
    bidder_pub_x: Option<pallas::Base>,
    bidder_pub_y: Option<pallas::Base>,
    amount: Option<u64>,
    competency_commitment: Option<pallas::Base>,
    encrypted_payload: Option<Vec<u8>>,
    /// The transaction the proof is bound to (`OBL-C78`); `None` means the zero pair, which is what
    /// `to_witnesses` defaults to as well. Set both through `tx_pair` so the params and the proof
    /// cannot disagree.
    tx_commitment: Option<pallas::Base>,
    tx_nonce: Option<pallas::Base>,
}

impl SubmitBidBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn bid_id(mut self, id: pallas::Base) -> Self {
        self.bid_id = Some(id);
        self
    }

    pub fn bidder_pubkey(mut self, pubkey: PublicKey) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (x, y) = pubkey.xy().expect("pk not identity");
        self.bidder_pub_x = Some(x);
        self.bidder_pub_y = Some(y);
        self
    }

    pub fn amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    pub fn competency_commitment(mut self, commitment: pallas::Base) -> Self {
        self.competency_commitment = Some(commitment);
        self
    }

    pub fn encrypted_payload(mut self, payload: Vec<u8>) -> Self {
        self.encrypted_payload = Some(payload);
        self
    }

    /// Bind the proof to a transaction: the pair `to_witnesses` must use, so params and proof agree
    /// (`OBL-C78`). Without it both default to zero, which is what the heavyweight fixtures use.
    pub fn tx_pair(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        self.tx_commitment = Some(tx_commitment);
        self.tx_nonce = Some(tx_nonce);
        self
    }

    pub fn build(self) -> Result<SubmitBidParamsV1, &'static str> {
        Ok(SubmitBidParamsV1 {
            proof: vec![],
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            bid_id: self.bid_id.ok_or("bid_id not set")?,
            bidder_pub_x: self.bidder_pub_x.ok_or("bidder_pub_x not set")?,
            bidder_pub_y: self.bidder_pub_y.ok_or("bidder_pub_y not set")?,
            amount: self.amount.ok_or("amount not set")?,
            claim_id: self.competency_commitment.ok_or("claim_id not set")?,
            encrypted_payload: self.encrypted_payload.unwrap_or_default(),
            tx_binding: tx_binding_of(
                &self.tx_commitment.unwrap_or_else(pallas::Base::zero),
                &self.tx_nonce.unwrap_or_else(pallas::Base::zero),
            ),
            tx_nonce: self.tx_nonce.unwrap_or_else(pallas::Base::zero),
        })
    }
}

/// Builder for RevealBidV1 params
#[derive(Default)]
pub struct RevealBidBuilder {
    tender_id: Option<pallas::Base>,
    bid_id: Option<pallas::Base>,
    revealed_amount: Option<u64>,
    /// The transaction the proof is bound to (`OBL-C78`); `None` means the zero pair, which is what
    /// `to_witnesses` defaults to as well. Set both through `tx_pair` so the params and the proof
    /// cannot disagree.
    tx_commitment: Option<pallas::Base>,
    tx_nonce: Option<pallas::Base>,
}

impl RevealBidBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn bid_id(mut self, id: pallas::Base) -> Self {
        self.bid_id = Some(id);
        self
    }

    pub fn revealed_amount(mut self, amount: u64) -> Self {
        self.revealed_amount = Some(amount);
        self
    }

    /// Bind the proof to a transaction: the pair `to_witnesses` must use, so params and proof agree
    /// (`OBL-C78`). Without it both default to zero, which is what the heavyweight fixtures use.
    pub fn tx_pair(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        self.tx_commitment = Some(tx_commitment);
        self.tx_nonce = Some(tx_nonce);
        self
    }

    pub fn build(self) -> Result<RevealBidParamsV1, &'static str> {
        Ok(RevealBidParamsV1 {
            proof: vec![],
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            bid_id: self.bid_id.ok_or("bid_id not set")?,
            revealed_amount: self.revealed_amount.ok_or("revealed_amount not set")?,
            tx_binding: tx_binding_of(
                &self.tx_commitment.unwrap_or_else(pallas::Base::zero),
                &self.tx_nonce.unwrap_or_else(pallas::Base::zero),
            ),
            tx_nonce: self.tx_nonce.unwrap_or_else(pallas::Base::zero),
        })
    }
}

/// Builder for CloseTenderV1 params
#[derive(Default)]
pub struct CloseTenderBuilder {
    tender_id: Option<pallas::Base>,
    requester_pubkey: Option<PublicKey>,
}

impl CloseTenderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn requester_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.requester_pubkey = Some(pubkey);
        self
    }

    pub fn build(self) -> Result<CloseTenderParamsV1, &'static str> {
        let requester_pubkey = self.requester_pubkey.ok_or("requester_pubkey not set")?;
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (x, y) = requester_pubkey.xy().expect("pk not identity");
        Ok(CloseTenderParamsV1 {
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            requester_pub_x: x,
            requester_pub_y: y,
        })
    }
}

/// Builder for SelectWinnerV1 params
#[derive(Default)]
pub struct SelectWinnerBuilder {
    tender_id: Option<pallas::Base>,
    winner_bid_id: Option<pallas::Base>,
    winner_pubkey: Option<PublicKey>,
    winning_amount: Option<u64>,
    requester_pubkey: Option<PublicKey>,
    /// The transaction the proof is bound to (`OBL-C78`); `None` means the zero pair, which is what
    /// `to_witnesses` defaults to as well. Set both through `tx_pair` so the params and the proof
    /// cannot disagree.
    tx_commitment: Option<pallas::Base>,
    tx_nonce: Option<pallas::Base>,
}

impl SelectWinnerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn winner_bid_id(mut self, id: pallas::Base) -> Self {
        self.winner_bid_id = Some(id);
        self
    }

    pub fn winner_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.winner_pubkey = Some(pubkey);
        self
    }

    pub fn winning_amount(mut self, amount: u64) -> Self {
        self.winning_amount = Some(amount);
        self
    }

    pub fn requester_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.requester_pubkey = Some(pubkey);
        self
    }

    /// Bind the proof to a transaction: the pair `to_witnesses` must use, so params and proof agree
    /// (`OBL-C78`). Without it both default to zero, which is what the heavyweight fixtures use.
    pub fn tx_pair(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        self.tx_commitment = Some(tx_commitment);
        self.tx_nonce = Some(tx_nonce);
        self
    }

    pub fn build(self) -> Result<SelectWinnerParamsV1, &'static str> {
        let winner_pubkey = self.winner_pubkey.ok_or("winner_pubkey not set")?;
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (pub_x, pub_y) = winner_pubkey.xy().expect("pk not identity");
        let requester = self.requester_pubkey.ok_or("requester_pubkey not set")?;
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (req_x, req_y) = requester.xy().expect("pk not identity");
        Ok(SelectWinnerParamsV1 {
            proof: vec![],
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            winner_bid_id: self.winner_bid_id.ok_or("winner_bid_id not set")?,
            winner_pub_x: pub_x,
            winner_pub_y: pub_y,
            winning_amount: self.winning_amount.ok_or("winning_amount not set")?,
            requester_pub_x: req_x,
            requester_pub_y: req_y,
            tx_binding: tx_binding_of(
                &self.tx_commitment.unwrap_or_else(pallas::Base::zero),
                &self.tx_nonce.unwrap_or_else(pallas::Base::zero),
            ),
            tx_nonce: self.tx_nonce.unwrap_or_else(pallas::Base::zero),
        })
    }
}

/// Builder for CancelTenderV1 params
#[derive(Default)]
pub struct CancelTenderBuilder {
    tender_id: Option<pallas::Base>,
    requester_pubkey: Option<PublicKey>,
}

impl CancelTenderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn requester_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.requester_pubkey = Some(pubkey);
        self
    }

    pub fn build(self) -> Result<CancelTenderParamsV1, &'static str> {
        let requester_pubkey = self.requester_pubkey.ok_or("requester_pubkey not set")?;
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (x, y) = requester_pubkey.xy().expect("pk not identity");
        Ok(CancelTenderParamsV1 {
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            requester_pub_x: x,
            requester_pub_y: y,
        })
    }
}

/// Builder for RejectBidV1 params
#[derive(Default)]
pub struct RejectBidBuilder {
    tender_id: Option<pallas::Base>,
    bid_id: Option<pallas::Base>,
    requester_pubkey: Option<PublicKey>,
}

impl RejectBidBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tender_id(mut self, id: pallas::Base) -> Self {
        self.tender_id = Some(id);
        self
    }

    pub fn bid_id(mut self, id: pallas::Base) -> Self {
        self.bid_id = Some(id);
        self
    }

    pub fn requester_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.requester_pubkey = Some(pubkey);
        self
    }

    pub fn build(self) -> Result<RejectBidParamsV1, &'static str> {
        let requester_pubkey = self.requester_pubkey.ok_or("requester_pubkey not set")?;
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (x, y) = requester_pubkey.xy().expect("pk not identity");
        Ok(RejectBidParamsV1 {
            tender_id: self.tender_id.ok_or("tender_id not set")?,
            bid_id: self.bid_id.ok_or("bid_id not set")?,
            requester_pub_x: x,
            requester_pub_y: y,
        })
    }
}