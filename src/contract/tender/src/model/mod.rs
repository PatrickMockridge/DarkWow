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

//! Tender contract data structures
//!
//! ## Tender State Machine
//!
//! ```text
//! Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded
//!                                                │
//!                                                └──[Cancel]──> Cancelled
//! ```
//!
//! ## Bid State Machine
//!
//! ```text
//! Sealed ──[Reveal]──> Revealed ──[Accept]──> Accepted
//!   │                        │
//!   └──[Timeout]──> Expired  └──[Reject]──> Rejected
//! ```

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash},
    error::ContractError,
    pasta::pallas,
};

/// Tender unique identifier (hash of tender data)
pub type TenderId = pallas::Base;

/// Bid unique identifier
pub type BidId = pallas::Base;

/// Represents the current state of a tender
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenderState {
    /// Tender created, accepting bids
    Created = 0,
    /// Tender is accepting bids (transitioned from Created on first bid)
    Bidding = 1,
    /// Bidding period ended, revealing bids
    Revealed = 2,
    /// Winner selected, job created in labor market
    Awarded = 3,
    /// Tender cancelled
    Cancelled = 4,
}

impl TryFrom<u8> for TenderState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Created),
            1 => Ok(Self::Bidding),
            2 => Ok(Self::Revealed),
            3 => Ok(Self::Awarded),
            4 => Ok(Self::Cancelled),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Represents the current state of a bid
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidState {
    /// Bid submitted but not yet revealed
    Sealed = 0,
    /// Bid revealed (amount public)
    Revealed = 1,
    /// Bid accepted as winning bid
    Accepted = 2,
    /// Bid rejected after reveal
    Rejected = 3,
    /// Bid expired (not revealed in time)
    Expired = 4,
}

impl TryFrom<u8> for BidState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Sealed),
            1 => Ok(Self::Revealed),
            2 => Ok(Self::Accepted),
            3 => Ok(Self::Rejected),
            4 => Ok(Self::Expired),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Core tender data stored on-chain
#[derive(Debug, Clone)]
pub struct Tender {
    pub version: u8,
    /// Tender identifier (commitment)
    pub id: TenderId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
    /// Title of the tender
    pub title: String,
    /// Hash of the specification document
    pub specification: pallas::Base,
    /// Attestation ID for competency requirements (references attestation contract)
    pub attestation_id: pallas::Base,
    /// Minimum bid amount
    pub min_bid: u64,
    /// Maximum bid amount
    pub max_bid: u64,
    /// Block height when bidding closes
    pub bid_deadline: u64,
    /// Block height when reveal period ends
    pub reveal_deadline: u64,
    /// Block height when delivery is due
    pub delivery_deadline: u64,
    /// Current state
    pub state: TenderState,
    /// ID of the winning bid
    pub selected_bid_id: Option<BidId>,
    /// Total number of bids received
    pub bid_count: u64,
    /// Block height when tender was created
    pub created_at: u64,
    /// Required capability ID for bidders (None = any bidder via attestation)
    pub required_capability: Option<[u8; 32]>,
    /// Required DAG ID for multi-path qualification (None = no DAG requirement)
    pub required_dag_id: Option<[u8; 32]>,
}

impl Tender {
    /// Encode Tender to bytes.
    /// Variable-length due to String title.
    pub fn encode(&self) -> Vec<u8> {
        let title_bytes = self.title.as_bytes();
        let cap = 154 + 1 + title_bytes.len() + 1 + 1 + 1;
        //             ^ prefix: version+id+req_x+req_y+spec+att+min+max+bid_d+rev_d+del_d+state+bid_count+created
        //               1+32+32+32+32+32+8+8+8+8+8+1+8+8 = 218, but we build progressively
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.requester_pub_x.to_repr());
        b.extend_from_slice(&self.requester_pub_y.to_repr());
        // String: u8 length prefix + bytes
        b.push(title_bytes.len() as u8);
        b.extend_from_slice(title_bytes);
        b.extend_from_slice(&self.specification.to_repr());
        b.extend_from_slice(&self.attestation_id.to_repr());
        b.extend_from_slice(&self.min_bid.to_le_bytes());
        b.extend_from_slice(&self.max_bid.to_le_bytes());
        b.extend_from_slice(&self.bid_deadline.to_le_bytes());
        b.extend_from_slice(&self.reveal_deadline.to_le_bytes());
        b.extend_from_slice(&self.delivery_deadline.to_le_bytes());
        b.push(self.state as u8);
        // selected_bid_id: Option<pallas::Base>
        match &self.selected_bid_id {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(&v.to_repr());
            }
        }
        b.extend_from_slice(&self.bid_count.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        // required_capability: Option<[u8;32]>
        match &self.required_capability {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(v);
            }
        }
        // required_dag_id: Option<[u8;32]>
        match &self.required_dag_id {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(v);
            }
        }
        b
    }

    /// Decode Tender from bytes.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // Minimum size: 1+32+32+32+1(min_title)+32+32+8+8+8+8+8+1+1+8+8+1+1 = 222
        if data.len() < 222 {
            return Err(ContractError::IoError(format!(
                "Tender: expected at least 222 bytes, got {}",
                data.len()
            )));
        }
        let version = data[0];
        let id = pallas::Base::from_repr(data[1..33].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Tender: invalid id".into()))?;
        let requester_pub_x = pallas::Base::from_repr(data[33..65].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Tender: invalid requester_pub_x".into()))?;
        let requester_pub_y = pallas::Base::from_repr(data[65..97].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Tender: invalid requester_pub_y".into()))?;
        let title_len = data[97] as usize;
        if data.len() < 98 + title_len {
            return Err(ContractError::IoError(format!(
                "Tender: truncated title at pos 97, len {}",
                title_len
            )));
        }
        let title = String::from_utf8(data[98..98 + title_len].to_vec())
            .map_err(|_| ContractError::IoError("Tender: invalid UTF-8 in title".into()))?;
        let mut pos = 98 + title_len;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("Tender: truncated specification".into()));
        }
        let specification = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Tender: invalid specification".into()))?;
        pos += 32;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("Tender: truncated attestation_id".into()));
        }
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Tender: invalid attestation_id".into()))?;
        pos += 32;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated min_bid".into()));
        }
        let min_bid = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated max_bid".into()));
        }
        let max_bid = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated bid_deadline".into()));
        }
        let bid_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated reveal_deadline".into()));
        }
        let reveal_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated delivery_deadline".into()));
        }
        let delivery_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Tender: truncated state".into()));
        }
        let state = TenderState::try_from(data[pos])?;
        pos += 1;
        // selected_bid_id: Option<pallas::Base>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Tender: truncated selected_bid_id tag".into()));
        }
        let (selected_bid_id, advance): (Option<pallas::Base>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("Tender: truncated selected_bid_id value".into()));
                }
                let v = Some(pallas::Base::from_repr(data[pos+1..pos+33].try_into().unwrap())
                    .into_option()
                    .ok_or_else(|| ContractError::IoError("Tender: invalid selected_bid_id".into()))?);
                (v, 33)
            }
            tag => return Err(ContractError::IoError(format!("Tender: invalid selected_bid_id tag {}", tag))),
        };
        pos += advance;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated bid_count".into()));
        }
        let bid_count = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Tender: truncated created_at".into()));
        }
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        // required_capability: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Tender: truncated required_capability tag".into()));
        }
        let (required_capability, advance2): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("Tender: truncated required_capability value".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("Tender: invalid required_capability tag {}", tag))),
        };
        pos += advance2;
        // required_dag_id: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Tender: truncated required_dag_id tag".into()));
        }
        let (required_dag_id, advance3): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("Tender: truncated required_dag_id value".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("Tender: invalid required_dag_id tag {}", tag))),
        };
        pos += advance3;

        // Verify we consumed exactly all bytes
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "Tender: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(Tender {
            version,
            id,
            requester_pub_x,
            requester_pub_y,
            title,
            specification,
            attestation_id,
            min_bid,
            max_bid,
            bid_deadline,
            reveal_deadline,
            delivery_deadline,
            state,
            selected_bid_id,
            bid_count,
            created_at,
            required_capability,
            required_dag_id,
        })
    }

    /// Derive the tender ID from tender parameters
    #[allow(dead_code)]
    pub fn derive_id(
        requester_pub_x: pallas::Base,
        requester_pub_y: pallas::Base,
        _title: &str,
        specification: pallas::Base,
        attestation_id: pallas::Base,
        min_bid: u64,
        max_bid: u64,
        bid_deadline: u64,
        reveal_deadline: u64,
        delivery_deadline: u64,
        requester_secret: pallas::Base,
    ) -> TenderId {
        poseidon_hash([
            requester_pub_x,
            requester_pub_y,
            // Note: title conversion would need proper implementation
            pallas::Base::zero(),
            specification,
            attestation_id,
            pallas::Base::from(min_bid),
            pallas::Base::from(max_bid),
            pallas::Base::from(bid_deadline),
            pallas::Base::from(reveal_deadline),
            pallas::Base::from(delivery_deadline),
            requester_secret,
        ])
    }
}

/// Core bid data stored on-chain
#[derive(Debug, Clone)]
pub struct Bid {
    pub version: u8,
    /// Bid identifier (commitment)
    pub id: BidId,
    /// Tender this bid is for
    pub tender_id: TenderId,
    /// Bidder's public key x coordinate
    pub bidder_pub_x: pallas::Base,
    /// Bidder's public key y coordinate
    pub bidder_pub_y: pallas::Base,
    /// Bid amount (hidden until reveal)
    pub amount: u64,
    /// Attestation claim ID (proving competency via attestation contract)
    pub claim_id: pallas::Base,
    /// Encrypted bid details (decrypted by requester after reveal)
    pub encrypted_payload: Vec<u8>,
    /// Current state
    pub state: BidState,
    /// Amount revealed (if revealed)
    pub revealed_amount: Option<u64>,
    /// Block height when bid was submitted
    pub created_at: u64,
}

impl Bid {
    /// Encode Bid to bytes.
    /// Variable-length due to Vec<u8> encrypted_payload.
    pub fn encode(&self) -> Vec<u8> {
        let cap = 1+32+32+32+32+8+32+1+self.encrypted_payload.len()+1+1+8;
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.tender_id.to_repr());
        b.extend_from_slice(&self.bidder_pub_x.to_repr());
        b.extend_from_slice(&self.bidder_pub_y.to_repr());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.claim_id.to_repr());
        // encrypted_payload: u8 len + bytes
        b.push(self.encrypted_payload.len() as u8);
        b.extend_from_slice(&self.encrypted_payload);
        b.push(self.state as u8);
        // revealed_amount: Option<u64>
        match &self.revealed_amount {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }

    /// Decode Bid from bytes.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // Minimum: 1+32+32+32+32+8+32+1(min_payload)+1+1+8 = 180
        if data.len() < 180 {
            return Err(ContractError::IoError(format!(
                "Bid: expected at least 180 bytes, got {}",
                data.len()
            )));
        }
        let version = data[0];
        let id = pallas::Base::from_repr(data[1..33].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Bid: invalid id".into()))?;
        let tender_id = pallas::Base::from_repr(data[33..65].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Bid: invalid tender_id".into()))?;
        let bidder_pub_x = pallas::Base::from_repr(data[65..97].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Bid: invalid bidder_pub_x".into()))?;
        let bidder_pub_y = pallas::Base::from_repr(data[97..129].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Bid: invalid bidder_pub_y".into()))?;
        let amount = u64::from_le_bytes(data[129..137].try_into().unwrap());
        let claim_id = pallas::Base::from_repr(data[137..169].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Bid: invalid claim_id".into()))?;
        let payload_len = data[169] as usize;
        let pos_after_payload = 170 + payload_len;
        if data.len() < pos_after_payload {
            return Err(ContractError::IoError(format!(
                "Bid: truncated encrypted_payload, needed {} more bytes",
                pos_after_payload - data.len()
            )));
        }
        let encrypted_payload = data[170..170+payload_len].to_vec();
        let pos = pos_after_payload;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Bid: truncated state".into()));
        }
        let state = BidState::try_from(data[pos])?;
        let pos = pos + 1;
        // revealed_amount: Option<u64>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Bid: truncated revealed_amount tag".into()));
        }
        let (revealed_amount, advance): (Option<u64>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 8 > data.len() {
                    return Err(ContractError::IoError("Bid: truncated revealed_amount value".into()));
                }
                let v = Some(u64::from_le_bytes(data[pos+1..pos+9].try_into().unwrap()));
                (v, 9)
            }
            tag => return Err(ContractError::IoError(format!("Bid: invalid revealed_amount tag {}", tag))),
        };
        let pos = pos + advance;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Bid: truncated created_at".into()));
        }
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        let pos = pos + 8;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "Bid: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(Bid {
            version,
            id,
            tender_id,
            bidder_pub_x,
            bidder_pub_y,
            amount,
            claim_id,
            encrypted_payload,
            state,
            revealed_amount,
            created_at,
        })
    }

    /// Derive the bid ID from bid parameters
    #[allow(dead_code)]
    pub fn derive_id(
        tender_id: TenderId,
        bidder_pub_x: pallas::Base,
        bidder_pub_y: pallas::Base,
        amount: u64,
        bid_nonce: pallas::Base,
    ) -> BidId {
        poseidon_hash([
            tender_id,
            bidder_pub_x,
            bidder_pub_y,
            pallas::Base::from(amount),
            bid_nonce,
        ])
    }
}

/// Parameters for creating a new tender
#[derive(Debug, Clone)]
pub struct CreateTenderParamsV1 {
    /// ZK proof for tender creation
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
    /// Title of the tender
    pub title: String,
    /// Hash of the specification document
    pub specification: pallas::Base,
    /// Attestation ID for competency requirements
    pub attestation_id: pallas::Base,
    /// Minimum bid amount
    pub min_bid: u64,
    /// Maximum bid amount
    pub max_bid: u64,
    /// Bidding deadline block
    pub bid_deadline: u64,
    /// Reveal deadline block
    pub reveal_deadline: u64,
    /// Delivery deadline block
    pub delivery_deadline: u64,
}

impl CreateTenderParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // Minimum: 1(proof_len)+32(tender_id)+32+32+1(title_len)+32+32+8+8+8+8+8 = 201
        if data.len() < 201 {
            return Err(ContractError::IoError(format!(
                "CreateTenderParamsV1: expected at least 201 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated tender_id".into()));
        }
        let tender_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderParamsV1: invalid tender_id".into()))?;
        pos += 32;
        if pos + 64 > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated requester pubkeys".into()));
        }
        let requester_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderParamsV1: invalid requester_pub_x".into()))?;
        pos += 32;
        let requester_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderParamsV1: invalid requester_pub_y".into()))?;
        pos += 32;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated title_len".into()));
        }
        let title_len = data[pos] as usize;
        pos += 1;
        if pos + title_len > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated title".into()));
        }
        let title = String::from_utf8(data[pos..pos+title_len].to_vec())
            .map_err(|_| ContractError::IoError("CreateTenderParamsV1: invalid UTF-8 in title".into()))?;
        pos += title_len;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated specification".into()));
        }
        let specification = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderParamsV1: invalid specification".into()))?;
        pos += 32;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated attestation_id".into()));
        }
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderParamsV1: invalid attestation_id".into()))?;
        pos += 32;
        if pos + 40 > data.len() {
            return Err(ContractError::IoError("CreateTenderParamsV1: truncated numeric fields".into()));
        }
        let min_bid = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let max_bid = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let bid_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let reveal_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let delivery_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CreateTenderParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(CreateTenderParamsV1 {
            proof,
            tender_id,
            requester_pub_x,
            requester_pub_y,
            title,
            specification,
            attestation_id,
            min_bid,
            max_bid,
            bid_deadline,
            reveal_deadline,
            delivery_deadline,
        })
    }
}

/// State update for CreateTenderV1
#[derive(Debug, Clone)]
pub struct CreateTenderUpdateV1 {
    /// The created tender ID
    pub tender_id: TenderId,
}

impl CreateTenderUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;

    pub fn encode(&self) -> Vec<u8> {
        self.tender_id.to_repr().to_vec()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "CreateTenderUpdateV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(CreateTenderUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CreateTenderUpdateV1: invalid tender_id".into()))?,
        })
    }
}

/// Parameters for submitting a bid
#[derive(Debug, Clone)]
pub struct SubmitBidParamsV1 {
    /// ZK proof for bid submission
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID
    pub bid_id: BidId,
    /// Bidder's public key x coordinate
    pub bidder_pub_x: pallas::Base,
    /// Bidder's public key y coordinate
    pub bidder_pub_y: pallas::Base,
    /// Bid amount (hidden)
    pub amount: u64,
    /// Attestation claim ID (from attestation.create_claim)
    pub claim_id: pallas::Base,
    /// Encrypted bid details
    pub encrypted_payload: Vec<u8>,
}

impl SubmitBidParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 171 {
            return Err(ContractError::IoError(format!(
                "SubmitBidParamsV1: expected at least 171 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("SubmitBidParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // tender_id (32), bid_id (32), bidder_pub_x (32), bidder_pub_y (32), amount (8), claim_id (32)
        if pos + 168 > data.len() {
            return Err(ContractError::IoError("SubmitBidParamsV1: truncated fixed fields".into()));
        }
        let tender_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidParamsV1: invalid tender_id".into()))?;
        pos += 32;
        let bid_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidParamsV1: invalid bid_id".into()))?;
        pos += 32;
        let bidder_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidParamsV1: invalid bidder_pub_x".into()))?;
        pos += 32;
        let bidder_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidParamsV1: invalid bidder_pub_y".into()))?;
        pos += 32;
        let amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let claim_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidParamsV1: invalid claim_id".into()))?;
        pos += 32;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("SubmitBidParamsV1: truncated payload_len".into()));
        }
        let payload_len = data[pos] as usize;
        pos += 1;
        if pos + payload_len > data.len() {
            return Err(ContractError::IoError("SubmitBidParamsV1: truncated encrypted_payload".into()));
        }
        let encrypted_payload = data[pos..pos+payload_len].to_vec();
        pos += payload_len;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SubmitBidParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(SubmitBidParamsV1 {
            proof,
            tender_id,
            bid_id,
            bidder_pub_x,
            bidder_pub_y,
            amount,
            claim_id,
            encrypted_payload,
        })
    }
}

/// State update for SubmitBidV1
#[derive(Debug, Clone)]
pub struct SubmitBidUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The submitted bid ID
    pub bid_id: BidId,
}

impl SubmitBidUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&self.tender_id.to_repr());
        b.extend_from_slice(&self.bid_id.to_repr());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 {
            return Err(ContractError::IoError(format!(
                "SubmitBidUpdateV1: expected 64 bytes, got {}",
                data.len()
            )));
        }
        Ok(SubmitBidUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("SubmitBidUpdateV1: invalid tender_id".into()))?,
            bid_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("SubmitBidUpdateV1: invalid bid_id".into()))?,
        })
    }
}

/// Parameters for revealing a bid
#[derive(Debug, Clone)]
pub struct RevealBidParamsV1 {
    /// ZK proof for bid reveal
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID
    pub bid_id: BidId,
    /// Revealed bid amount
    pub revealed_amount: u64,
}

impl RevealBidParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 73 {
            return Err(ContractError::IoError(format!(
                "RevealBidParamsV1: expected at least 73 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("RevealBidParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 72 > data.len() {
            return Err(ContractError::IoError("RevealBidParamsV1: truncated fixed fields".into()));
        }
        let tender_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RevealBidParamsV1: invalid tender_id".into()))?;
        pos += 32;
        let bid_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RevealBidParamsV1: invalid bid_id".into()))?;
        pos += 32;
        let revealed_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "RevealBidParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(RevealBidParamsV1 { proof, tender_id, bid_id, revealed_amount })
    }
}

/// State update for RevealBidV1
#[derive(Debug, Clone)]
pub struct RevealBidUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The revealed bid ID
    pub bid_id: BidId,
}

impl RevealBidUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&self.tender_id.to_repr());
        b.extend_from_slice(&self.bid_id.to_repr());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 {
            return Err(ContractError::IoError(format!(
                "RevealBidUpdateV1: expected 64 bytes, got {}",
                data.len()
            )));
        }
        Ok(RevealBidUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RevealBidUpdateV1: invalid tender_id".into()))?,
            bid_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RevealBidUpdateV1: invalid bid_id".into()))?,
        })
    }
}

/// Parameters for closing bidding and starting reveal
#[derive(Debug, Clone)]
pub struct CloseTenderParamsV1 {
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
}

impl CloseTenderParamsV1 {
    pub const ENCODED_SIZE: usize = 96;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 96 {
            return Err(ContractError::IoError(format!(
                "CloseTenderParamsV1: expected 96 bytes, got {}",
                data.len()
            )));
        }
        Ok(CloseTenderParamsV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CloseTenderParamsV1: invalid tender_id".into()))?,
            requester_pub_x: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CloseTenderParamsV1: invalid requester_pub_x".into()))?,
            requester_pub_y: pallas::Base::from_repr(data[64..96].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CloseTenderParamsV1: invalid requester_pub_y".into()))?,
        })
    }
}

/// State update for CloseTenderV1
#[derive(Debug, Clone)]
pub struct CloseTenderUpdateV1 {
    /// The closed tender ID
    pub tender_id: TenderId,
}

impl CloseTenderUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;

    pub fn encode(&self) -> Vec<u8> {
        self.tender_id.to_repr().to_vec()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "CloseTenderUpdateV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(CloseTenderUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CloseTenderUpdateV1: invalid tender_id".into()))?,
        })
    }
}

/// Parameters for selecting winner
#[derive(Debug, Clone)]
pub struct SelectWinnerParamsV1 {
    /// ZK proof for winner selection
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Winner's bid ID
    pub winner_bid_id: BidId,
    /// Requester's public key x coordinate (must match tender creator)
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate (must match tender creator)
    pub requester_pub_y: pallas::Base,
    /// Winner's public key x coordinate
    pub winner_pub_x: pallas::Base,
    /// Winner's public key y coordinate
    pub winner_pub_y: pallas::Base,
    /// Winning bid amount
    pub winning_amount: u64,
}

impl SelectWinnerParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 201 {
            return Err(ContractError::IoError(format!(
                "SelectWinnerParamsV1: expected at least 201 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("SelectWinnerParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // tender_id(32) + winner_bid_id(32) + requester_pub_x(32) + requester_pub_y(32)
        // + winner_pub_x(32) + winner_pub_y(32) + winning_amount(8) = 200
        if pos + 200 > data.len() {
            return Err(ContractError::IoError("SelectWinnerParamsV1: truncated fixed fields".into()));
        }
        let tender_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerParamsV1: invalid tender_id".into()))?;
        pos += 32;
        let winner_bid_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerParamsV1: invalid winner_bid_id".into()))?;
        pos += 32;
        let requester_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerParamsV1: invalid requester_pub_x".into()))?;
        pos += 32;
        let requester_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerParamsV1: invalid requester_pub_y".into()))?;
        pos += 32;
        let winner_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerParamsV1: invalid winner_pub_x".into()))?;
        pos += 32;
        let winner_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerParamsV1: invalid winner_pub_y".into()))?;
        pos += 32;
        let winning_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SelectWinnerParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(SelectWinnerParamsV1 {
            proof,
            tender_id,
            winner_bid_id,
            requester_pub_x,
            requester_pub_y,
            winner_pub_x,
            winner_pub_y,
            winning_amount,
        })
    }
}

/// State update for SelectWinnerV1
#[derive(Debug, Clone)]
pub struct SelectWinnerUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The winning bid ID
    pub winner_bid_id: BidId,
    /// The job ID in labor market (for tracking)
    pub labor_job_id: Option<pallas::Base>,
}

impl SelectWinnerUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 65 + match &self.labor_job_id { Some(_) => 32, None => 0 };
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.tender_id.to_repr());
        b.extend_from_slice(&self.winner_bid_id.to_repr());
        match &self.labor_job_id {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(&v.to_repr());
            }
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 65 {
            return Err(ContractError::IoError(format!(
                "SelectWinnerUpdateV1: expected at least 65 bytes, got {}",
                data.len()
            )));
        }
        let tender_id = pallas::Base::from_repr(data[0..32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerUpdateV1: invalid tender_id".into()))?;
        let winner_bid_id = pallas::Base::from_repr(data[32..64].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SelectWinnerUpdateV1: invalid winner_bid_id".into()))?;
        let (labor_job_id, advance): (Option<pallas::Base>, usize) = match data[64] {
            0 => (None, 1),
            1 => {
                if data.len() < 97 {
                    return Err(ContractError::IoError("SelectWinnerUpdateV1: truncated labor_job_id".into()));
                }
                let v = Some(pallas::Base::from_repr(data[65..97].try_into().unwrap())
                    .into_option()
                    .ok_or_else(|| ContractError::IoError("SelectWinnerUpdateV1: invalid labor_job_id".into()))?);
                (v, 33)
            }
            tag => return Err(ContractError::IoError(format!("SelectWinnerUpdateV1: invalid labor_job_id tag {}", tag))),
        };
        let pos = 64 + advance;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SelectWinnerUpdateV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(SelectWinnerUpdateV1 { tender_id, winner_bid_id, labor_job_id })
    }
}

/// Parameters for cancelling a tender
#[derive(Debug, Clone)]
pub struct CancelTenderParamsV1 {
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
}

impl CancelTenderParamsV1 {
    pub const ENCODED_SIZE: usize = 96;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 96 {
            return Err(ContractError::IoError(format!(
                "CancelTenderParamsV1: expected 96 bytes, got {}",
                data.len()
            )));
        }
        Ok(CancelTenderParamsV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CancelTenderParamsV1: invalid tender_id".into()))?,
            requester_pub_x: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CancelTenderParamsV1: invalid requester_pub_x".into()))?,
            requester_pub_y: pallas::Base::from_repr(data[64..96].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CancelTenderParamsV1: invalid requester_pub_y".into()))?,
        })
    }
}

/// State update for CancelTenderV1
#[derive(Debug, Clone)]
pub struct CancelTenderUpdateV1 {
    /// The cancelled tender ID
    pub tender_id: TenderId,
}

impl CancelTenderUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;

    pub fn encode(&self) -> Vec<u8> {
        self.tender_id.to_repr().to_vec()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "CancelTenderUpdateV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(CancelTenderUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CancelTenderUpdateV1: invalid tender_id".into()))?,
        })
    }
}

/// Parameters for rejecting a bid
#[derive(Debug, Clone)]
pub struct RejectBidParamsV1 {
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID being rejected
    pub bid_id: BidId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
}

impl RejectBidParamsV1 {
    pub const ENCODED_SIZE: usize = 128;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 128 {
            return Err(ContractError::IoError(format!(
                "RejectBidParamsV1: expected 128 bytes, got {}",
                data.len()
            )));
        }
        Ok(RejectBidParamsV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RejectBidParamsV1: invalid tender_id".into()))?,
            bid_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RejectBidParamsV1: invalid bid_id".into()))?,
            requester_pub_x: pallas::Base::from_repr(data[64..96].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RejectBidParamsV1: invalid requester_pub_x".into()))?,
            requester_pub_y: pallas::Base::from_repr(data[96..128].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RejectBidParamsV1: invalid requester_pub_y".into()))?,
        })
    }
}

/// State update for RejectBidV1
#[derive(Debug, Clone)]
pub struct RejectBidUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The rejected bid ID
    pub bid_id: BidId,
}

impl RejectBidUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&self.tender_id.to_repr());
        b.extend_from_slice(&self.bid_id.to_repr());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 {
            return Err(ContractError::IoError(format!(
                "RejectBidUpdateV1: expected 64 bytes, got {}",
                data.len()
            )));
        }
        Ok(RejectBidUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RejectBidUpdateV1: invalid tender_id".into()))?,
            bid_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("RejectBidUpdateV1: invalid bid_id".into()))?,
        })
    }
}

// ============================================================================
// O-Cap Enabled Functions (0x07-0x08)
// ============================================================================

/// Parameters for creating a tender with capability requirements
#[derive(Debug, Clone)]
pub struct CreateTenderWithCapabilityParamsV1 {
    /// ZK proof for tender creation
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Requester's public key x coordinate
    pub requester_pub_x: pallas::Base,
    /// Requester's public key y coordinate
    pub requester_pub_y: pallas::Base,
    /// Title of the tender
    pub title: String,
    /// Hash of the specification document
    pub specification: pallas::Base,
    /// Attestation ID for competency requirements
    pub attestation_id: pallas::Base,
    /// Minimum bid amount
    pub min_bid: u64,
    /// Maximum bid amount
    pub max_bid: u64,
    /// Bidding deadline block
    pub bid_deadline: u64,
    /// Reveal deadline block
    pub reveal_deadline: u64,
    /// Delivery deadline block
    pub delivery_deadline: u64,
    /// Required capability ID for bidders
    pub required_capability: Option<[u8; 32]>,
    /// Required DAG ID for multi-path qualification
    pub required_dag_id: Option<[u8; 32]>,
}

impl CreateTenderWithCapabilityParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // Minimum: like CreateTenderParamsV1 but with 2 option tags = 201 + 2 = 203
        if data.len() < 203 {
            return Err(ContractError::IoError(format!(
                "CreateTenderWithCapabilityParamsV1: expected at least 203 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated tender_id".into()));
        }
        let tender_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderWithCapabilityParamsV1: invalid tender_id".into()))?;
        pos += 32;
        if pos + 64 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated requester pubkeys".into()));
        }
        let requester_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderWithCapabilityParamsV1: invalid requester_pub_x".into()))?;
        pos += 32;
        let requester_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderWithCapabilityParamsV1: invalid requester_pub_y".into()))?;
        pos += 32;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated title_len".into()));
        }
        let title_len = data[pos] as usize;
        pos += 1;
        if pos + title_len > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated title".into()));
        }
        let title = String::from_utf8(data[pos..pos+title_len].to_vec())
            .map_err(|_| ContractError::IoError("CreateTenderWithCapabilityParamsV1: invalid UTF-8 in title".into()))?;
        pos += title_len;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated specification".into()));
        }
        let specification = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderWithCapabilityParamsV1: invalid specification".into()))?;
        pos += 32;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated attestation_id".into()));
        }
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateTenderWithCapabilityParamsV1: invalid attestation_id".into()))?;
        pos += 32;
        if pos + 40 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated numeric fields".into()));
        }
        let min_bid = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let max_bid = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let bid_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let reveal_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let delivery_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        // required_capability: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated required_capability tag".into()));
        }
        let (required_capability, advance): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated required_capability value".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("CreateTenderWithCapabilityParamsV1: invalid required_capability tag {}", tag))),
        };
        pos += advance;
        // required_dag_id: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated required_dag_id tag".into()));
        }
        let (required_dag_id, advance2): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("CreateTenderWithCapabilityParamsV1: truncated required_dag_id value".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("CreateTenderWithCapabilityParamsV1: invalid required_dag_id tag {}", tag))),
        };
        pos += advance2;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CreateTenderWithCapabilityParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(CreateTenderWithCapabilityParamsV1 {
            proof,
            tender_id,
            requester_pub_x,
            requester_pub_y,
            title,
            specification,
            attestation_id,
            min_bid,
            max_bid,
            bid_deadline,
            reveal_deadline,
            delivery_deadline,
            required_capability,
            required_dag_id,
        })
    }
}

/// State update for CreateTenderWithCapabilityV1
#[derive(Debug, Clone)]
pub struct CreateTenderWithCapabilityUpdateV1 {
    /// The created tender ID
    pub tender_id: TenderId,
}

impl CreateTenderWithCapabilityUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;

    pub fn encode(&self) -> Vec<u8> {
        self.tender_id.to_repr().to_vec()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "CreateTenderWithCapabilityUpdateV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(CreateTenderWithCapabilityUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("CreateTenderWithCapabilityUpdateV1: invalid tender_id".into()))?,
        })
    }
}

/// Parameters for submitting a bid with capability proof
#[derive(Debug, Clone)]
pub struct SubmitBidWithCapabilityParamsV1 {
    /// ZK proof for bid submission
    pub proof: Vec<u8>,
    /// Tender ID
    pub tender_id: TenderId,
    /// Bid ID
    pub bid_id: BidId,
    /// Bidder's public key x coordinate
    pub bidder_pub_x: pallas::Base,
    /// Bidder's public key y coordinate
    pub bidder_pub_y: pallas::Base,
    /// Bid amount (hidden)
    pub amount: u64,
    /// Attestation claim ID (from attestation.create_claim)
    pub claim_id: pallas::Base,
    /// Encrypted bid details
    pub encrypted_payload: Vec<u8>,
    /// Required capability ID (must match tender's requirement)
    pub required_capability_id: [u8; 32],
    /// Capability predicate result (from Identity contract)
    pub capability_predicate_result: pallas::Base,
}

impl SubmitBidWithCapabilityParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 236 {
            return Err(ContractError::IoError(format!(
                "SubmitBidWithCapabilityParamsV1: expected at least 236 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("SubmitBidWithCapabilityParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // tender_id(32)+bid_id(32)+bidder_pub_x(32)+bidder_pub_y(32)+amount(8)+claim_id(32) = 168
        if pos + 168 > data.len() {
            return Err(ContractError::IoError("SubmitBidWithCapabilityParamsV1: truncated fixed fields 1".into()));
        }
        let tender_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityParamsV1: invalid tender_id".into()))?;
        pos += 32;
        let bid_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityParamsV1: invalid bid_id".into()))?;
        pos += 32;
        let bidder_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityParamsV1: invalid bidder_pub_x".into()))?;
        pos += 32;
        let bidder_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityParamsV1: invalid bidder_pub_y".into()))?;
        pos += 32;
        let amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let claim_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityParamsV1: invalid claim_id".into()))?;
        pos += 32;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("SubmitBidWithCapabilityParamsV1: truncated payload_len".into()));
        }
        let payload_len = data[pos] as usize;
        pos += 1;
        if pos + payload_len > data.len() {
            return Err(ContractError::IoError("SubmitBidWithCapabilityParamsV1: truncated encrypted_payload".into()));
        }
        let encrypted_payload = data[pos..pos+payload_len].to_vec();
        pos += payload_len;
        // required_capability_id(32)+capability_predicate_result(32) = 64
        if pos + 64 > data.len() {
            return Err(ContractError::IoError("SubmitBidWithCapabilityParamsV1: truncated cap fields".into()));
        }
        let mut required_capability_id = [0u8; 32];
        required_capability_id.copy_from_slice(&data[pos..pos+32]);
        pos += 32;
        let capability_predicate_result = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityParamsV1: invalid capability_predicate_result".into()))?;
        pos += 32;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SubmitBidWithCapabilityParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(SubmitBidWithCapabilityParamsV1 {
            proof,
            tender_id,
            bid_id,
            bidder_pub_x,
            bidder_pub_y,
            amount,
            claim_id,
            encrypted_payload,
            required_capability_id,
            capability_predicate_result,
        })
    }
}

/// State update for SubmitBidWithCapabilityV1
#[derive(Debug, Clone)]
pub struct SubmitBidWithCapabilityUpdateV1 {
    /// The tender ID
    pub tender_id: TenderId,
    /// The submitted bid ID
    pub bid_id: BidId,
}

impl SubmitBidWithCapabilityUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&self.tender_id.to_repr());
        b.extend_from_slice(&self.bid_id.to_repr());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 {
            return Err(ContractError::IoError(format!(
                "SubmitBidWithCapabilityUpdateV1: expected 64 bytes, got {}",
                data.len()
            )));
        }
        Ok(SubmitBidWithCapabilityUpdateV1 {
            tender_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityUpdateV1: invalid tender_id".into()))?,
            bid_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("SubmitBidWithCapabilityUpdateV1: invalid bid_id".into()))?,
        })
    }
}
