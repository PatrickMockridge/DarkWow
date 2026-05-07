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

//! Tender Test Harness
//!
//! Provides isolated testing for Tender contract.

use dwow::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use dwow_serial::Encodable;

use darkfi_tender_contract::client::{
    create_tender_v1::{CreateTenderV1CallData, create_tender_v1_proof, CreateTenderV1PublicInputs},
    reveal_bid_v1::{RevealBidV1CallData, reveal_bid_v1_proof, RevealBidV1PublicInputs},
    select_winner_v1::{SelectWinnerV1CallData, select_winner_v1_proof, SelectWinnerV1PublicInputs},
    submit_bid_v1::{SubmitBidV1CallData, submit_bid_v1_proof, SubmitBidV1PublicInputs},
};
use darkfi_tender_contract::model::{
    CreateTenderParamsV1, SubmitBidParamsV1, RevealBidParamsV1, SelectWinnerParamsV1,
};

/// Tender Harness for isolated testing
pub struct TenderHarness {
    /// CreateTender_V1 ZkBinary
    create_tender_zkbin: ZkBinary,
    /// CreateTender_V1 ProvingKey
    create_tender_pk: ProvingKey,
    /// SubmitBid_V1 ZkBinary
    submit_bid_zkbin: ZkBinary,
    /// SubmitBid_V1 ProvingKey
    submit_bid_pk: ProvingKey,
    /// RevealBid_V1 ZkBinary
    reveal_bid_zkbin: ZkBinary,
    /// RevealBid_V1 ProvingKey
    reveal_bid_pk: ProvingKey,
    /// SelectWinner_V1 ZkBinary
    select_winner_zkbin: ZkBinary,
    /// SelectWinner_V1 ProvingKey
    select_winner_pk: ProvingKey,
}

impl TenderHarness {
    /// Spawn a new Tender harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../tender/proof/create_tender_v1.zk.bin");
        let submit_bin = include_bytes!("../../../tender/proof/submit_bid_v1.zk.bin");
        let reveal_bin = include_bytes!("../../../tender/proof/reveal_bid_v1.zk.bin");
        let select_bin = include_bytes!("../../../tender/proof/select_winner_v1.zk.bin");

        let create_tender_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let submit_bid_zkbin = ZkBinary::decode(submit_bin, false).unwrap();
        let reveal_bid_zkbin = ZkBinary::decode(reveal_bin, false).unwrap();
        let select_winner_zkbin = ZkBinary::decode(select_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&create_tender_zkbin).unwrap(),
            &create_tender_zkbin,
        );
        let submit_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&submit_bid_zkbin).unwrap(),
            &submit_bid_zkbin,
        );
        let reveal_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&reveal_bid_zkbin).unwrap(),
            &reveal_bid_zkbin,
        );
        let select_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&select_winner_zkbin).unwrap(),
            &select_winner_zkbin,
        );

        let create_tender_pk = ProvingKey::build(create_tender_zkbin.k, &create_circuit);
        let submit_bid_pk = ProvingKey::build(submit_bid_zkbin.k, &submit_circuit);
        let reveal_bid_pk = ProvingKey::build(reveal_bid_zkbin.k, &reveal_circuit);
        let select_winner_pk = ProvingKey::build(select_winner_zkbin.k, &select_circuit);

        Self {
            create_tender_zkbin,
            create_tender_pk,
            submit_bid_zkbin,
            submit_bid_pk,
            reveal_bid_zkbin,
            reveal_bid_pk,
            select_winner_zkbin,
            select_winner_pk,
        }
    }

    /// Create a tender (function code 0x00)
    pub fn create_tender(
        &self,
        requester_public: PublicKey,
        requester_secret: pallas::Base,
        title: String,
        specification: pallas::Base,
        attestation_id: pallas::Base,
        min_bid: u64,
        max_bid: u64,
        bid_deadline: u64,
        reveal_deadline: u64,
        delivery_deadline: u64,
    ) -> Result<CreateTenderResult, Box<dyn std::error::Error>> {
        let call_data_input =
            CreateTenderV1CallData::new(requester_secret, requester_public);
        let (proof, public_inputs) = create_tender_v1_proof(
            &self.create_tender_zkbin,
            &self.create_tender_pk,
            &call_data_input,
        )?;
        let (ix, iy) = requester_public.xy();
        let tender_id = darkfi_tender_contract::model::Tender::derive_id(
            ix, iy, &title, specification, attestation_id,
            min_bid, max_bid, bid_deadline, reveal_deadline,
            delivery_deadline, requester_secret,
        );
        let params = CreateTenderParamsV1 {
            proof: proof.as_ref().to_vec(),
            tender_id,
            requester_pub_x: ix,
            requester_pub_y: iy,
            title,
            specification,
            attestation_id,
            min_bid,
            max_bid,
            bid_deadline,
            reveal_deadline,
            delivery_deadline,
        };
        let mut call_data = vec![0x00];
        params.encode(&mut call_data)?;
        Ok(CreateTenderResult { call_data, proof, public_inputs, tender_id })
    }

    /// Submit a bid (function code 0x01)
    pub fn submit_bid(
        &self,
        tender_id: pallas::Base,
        bidder_public: PublicKey,
        bidder_secret: pallas::Base,
        amount: u64,
        bid_nonce: pallas::Base,
        claim_id: pallas::Base,
        encrypted_payload: Vec<u8>,
    ) -> Result<SubmitBidResult, Box<dyn std::error::Error>> {
        let call_data_input = SubmitBidV1CallData::new(
            tender_id,
            bidder_secret,
            pallas::Base::from(amount),
            bid_nonce,
            bidder_public,
        );
        let (proof, public_inputs) = submit_bid_v1_proof(
            &self.submit_bid_zkbin,
            &self.submit_bid_pk,
            &call_data_input,
        )?;
        let (ix, iy) = bidder_public.xy();
        let params = SubmitBidParamsV1 {
            proof: proof.as_ref().to_vec(),
            tender_id,
            bid_id: public_inputs.bid_id,
            bidder_pub_x: ix,
            bidder_pub_y: iy,
            amount,
            claim_id,
            encrypted_payload,
        };
        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;
        Ok(SubmitBidResult { call_data, proof, public_inputs })
    }

    /// Reveal a bid (function code 0x02)
    pub fn reveal_bid(
        &self,
        tender_id: pallas::Base,
        bid_id: pallas::Base,
        bidder_public: PublicKey,
        bidder_secret: pallas::Base,
        revealed_amount: u64,
    ) -> Result<RevealBidResult, Box<dyn std::error::Error>> {
        let call_data_input = RevealBidV1CallData::new(
            tender_id,
            bid_id,
            bidder_secret,
            pallas::Base::from(revealed_amount),
            bidder_public,
        );
        let (proof, public_inputs) = reveal_bid_v1_proof(
            &self.reveal_bid_zkbin,
            &self.reveal_bid_pk,
            &call_data_input,
        )?;
        let params = RevealBidParamsV1 {
            proof: proof.as_ref().to_vec(),
            tender_id,
            bid_id,
            revealed_amount,
        };
        let mut call_data = vec![0x02];
        params.encode(&mut call_data)?;
        Ok(RevealBidResult { call_data, proof, public_inputs })
    }

    /// Select a winner (function code 0x04)
    pub fn select_winner(
        &self,
        tender_id: pallas::Base,
        winner_bid_id: pallas::Base,
        requester_public: PublicKey,
        requester_secret: pallas::Base,
        winner_public: PublicKey,
        winning_amount: u64,
    ) -> Result<SelectWinnerResult, Box<dyn std::error::Error>> {
        let call_data_input = SelectWinnerV1CallData::new(
            tender_id,
            winner_bid_id,
            requester_secret,
            requester_public,
        );
        let (proof, public_inputs) = select_winner_v1_proof(
            &self.select_winner_zkbin,
            &self.select_winner_pk,
            &call_data_input,
        )?;
        let (wx, wy) = winner_public.xy();
        let params = SelectWinnerParamsV1 {
            proof: proof.as_ref().to_vec(),
            tender_id,
            winner_bid_id,
            winner_pub_x: wx,
            winner_pub_y: wy,
            winning_amount,
        };
        let mut call_data = vec![0x04];
        params.encode(&mut call_data)?;
        Ok(SelectWinnerResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for TenderHarness {
    fn name(&self) -> &str {
        "tender"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateTenderV1",
            "SubmitBidV1",
            "RevealBidV1",
            "SelectWinnerV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateTenderV1" => Some(&self.create_tender_zkbin),
            "SubmitBidV1" => Some(&self.submit_bid_zkbin),
            "RevealBidV1" => Some(&self.reveal_bid_zkbin),
            "SelectWinnerV1" => Some(&self.select_winner_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateTenderV1" => Some(&self.create_tender_pk),
            "SubmitBidV1" => Some(&self.submit_bid_pk),
            "RevealBidV1" => Some(&self.reveal_bid_pk),
            "SelectWinnerV1" => Some(&self.select_winner_pk),
            _ => None,
        }
    }
}

/// Result of create_tender
pub struct CreateTenderResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: CreateTenderV1PublicInputs,
    pub tender_id: pallas::Base,
}

/// Result of submit_bid
pub struct SubmitBidResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: SubmitBidV1PublicInputs,
}

/// Result of reveal_bid
pub struct RevealBidResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: RevealBidV1PublicInputs,
}

/// Result of select_winner
pub struct SelectWinnerResult {
    pub call_data: Vec<u8>,
    pub proof: dwow::zk::Proof,
    pub public_inputs: SelectWinnerV1PublicInputs,
}