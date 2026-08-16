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

//! Lottery Test Harness
//!
//! Provides isolated testing for Lottery contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pasta_prelude::Group, poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};

use dwow_lottery_contract::client::{
    claim_prize::{ClaimPrizeCallData, create_claim_prize_proof},
    commit_ticket::{CommitTicketV1CallData, create_commit_ticket_v1_proof, CommitTicketV1PublicInputs},
    house_auth::{HouseAuthCallData, create_house_auth_proof},
    reveal_ticket::{RevealTicketV1CallData, create_reveal_ticket_v1_proof},
};
use dwow_lottery_contract::model::{
    BuyTicketParamsV1, ClaimPrizeParamsV1, DrawWinnersParamsV1, ExpireLotteryParamsV1,
    InitializeParamsV1, LotteryConfig, PrizeTierConfig, RevealTicketParamsV1,
};

/// Lottery Harness for isolated testing
pub struct LotteryHarness {
    /// CommitTicketV2 ZkBinary
    commit_ticket_zkbin: ZkBinary,
    /// CommitTicketV2 ProvingKey
    commit_ticket_pk: ProvingKey,
    /// RevealTicketV2 ZkBinary
    reveal_ticket_zkbin: ZkBinary,
    /// RevealTicketV2 ProvingKey
    reveal_ticket_pk: ProvingKey,
    /// ClaimPrizeV2 ZkBinary
    claim_prize_zkbin: ZkBinary,
    /// ClaimPrizeV2 ProvingKey
    claim_prize_pk: ProvingKey,
    /// DrawWinnersV2 ZkBinary
    draw_winners_zkbin: ZkBinary,
    /// DrawWinnersV2 ProvingKey
    draw_winners_pk: ProvingKey,
    /// ExpireLotteryV2 ZkBinary
    expire_lottery_zkbin: ZkBinary,
    /// ExpireLotteryV2 ProvingKey
    expire_lottery_pk: ProvingKey,
    /// InitializeV2 ZkBinary
    initialize_zkbin: ZkBinary,
    /// InitializeV2 ProvingKey
    initialize_pk: ProvingKey,
}

impl LotteryHarness {
    /// Spawn a new Lottery harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_ticket_bin = include_bytes!("../../../lottery/proof/commit_ticket.zk.bin");
        let reveal_ticket_bin = include_bytes!("../../../lottery/proof/reveal_ticket.zk.bin");
        let claim_prize_bin = include_bytes!("../../../lottery/proof/claim_prize.zk.bin");
        let draw_winners_bin = include_bytes!("../../../lottery/proof/draw_winners.zk.bin");
        let expire_lottery_bin = include_bytes!("../../../lottery/proof/expire_lottery.zk.bin");
        let initialize_bin = include_bytes!("../../../lottery/proof/initialize.zk.bin");

        let commit_ticket_zkbin = ZkBinary::decode(commit_ticket_bin, false).unwrap();
        let reveal_ticket_zkbin = ZkBinary::decode(reveal_ticket_bin, false).unwrap();
        let claim_prize_zkbin = ZkBinary::decode(claim_prize_bin, false).unwrap();
        let draw_winners_zkbin = ZkBinary::decode(draw_winners_bin, false).unwrap();
        let expire_lottery_zkbin = ZkBinary::decode(expire_lottery_bin, false).unwrap();
        let initialize_zkbin = ZkBinary::decode(initialize_bin, false).unwrap();

        let commit_ticket_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&commit_ticket_zkbin).unwrap(),
            &commit_ticket_zkbin,
        );
        let reveal_ticket_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&reveal_ticket_zkbin).unwrap(),
            &reveal_ticket_zkbin,
        );
        let claim_prize_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&claim_prize_zkbin).unwrap(),
            &claim_prize_zkbin,
        );
        let draw_winners_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&draw_winners_zkbin).unwrap(),
            &draw_winners_zkbin,
        );
        let expire_lottery_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&expire_lottery_zkbin).unwrap(),
            &expire_lottery_zkbin,
        );
        let initialize_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&initialize_zkbin).unwrap(),
            &initialize_zkbin,
        );

        let commit_ticket_pk = ProvingKey::build(commit_ticket_zkbin.k, &commit_ticket_circuit).expect("ProvingKey::build failed");
        let reveal_ticket_pk = ProvingKey::build(reveal_ticket_zkbin.k, &reveal_ticket_circuit).expect("ProvingKey::build failed");
        let claim_prize_pk = ProvingKey::build(claim_prize_zkbin.k, &claim_prize_circuit).expect("ProvingKey::build failed");
        let draw_winners_pk = ProvingKey::build(draw_winners_zkbin.k, &draw_winners_circuit).expect("ProvingKey::build failed");
        let expire_lottery_pk = ProvingKey::build(expire_lottery_zkbin.k, &expire_lottery_circuit).expect("ProvingKey::build failed");
        let initialize_pk = ProvingKey::build(initialize_zkbin.k, &initialize_circuit).expect("ProvingKey::build failed");

        Self {
            commit_ticket_zkbin,
            commit_ticket_pk,
            reveal_ticket_zkbin,
            reveal_ticket_pk,
            claim_prize_zkbin,
            claim_prize_pk,
            draw_winners_zkbin,
            draw_winners_pk,
            expire_lottery_zkbin,
            expire_lottery_pk,
            initialize_zkbin,
            initialize_pk,
        }
    }

    /// Initialize a lottery (non-ZK, function code 0x00).
    ///
    /// Uses a single prize tier paying 100% of the pool (`payout_percent: 10000`), with zero
    /// house edge, so the claim payout is deterministic (ticket_price for a single ticket).
    pub fn initialize(
        &self,
        house_pub: PublicKey,
        ticket_price: u64,
        num_picks: u8,
        number_range: u8,
        duration: u64,
        claim_duration: u64,
    ) -> Result<InitializeResult, Box<dyn std::error::Error>> {
        let config = LotteryConfig {
            num_picks,
            number_range,
            house_edge_bp: 0,
            ticket_price,
            prize_tiers: vec![PrizeTierConfig {
                matches_needed: 1,
                payout_percent: 10000,
                roll_to_next: false,
            }],
        };
        let params = InitializeParamsV1 {
            house_pub,
            config,
            duration,
            claim_duration,
            rolled_over: 0,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());

        Ok(InitializeResult { call_data })
    }

    /// Commit a ticket (for BuyTicketV1, 0x01).
    ///
    /// The `commitment` field in `BuyTicketParamsV1` is the contract-level commitment
    /// `Hash(...Hash(lottery_id, n1), n2..., nonce)` verified off-circuit by RevealTicketV1.
    /// The ZK proof's `ticket_id = poseidon_hash(4, lottery_id, px, py, amount, nonce)`.
    pub fn commit_ticket(
        &self,
        player_pub: PublicKey,
        lottery_id: pallas::Base,
        numbers: Vec<u8>,
        nonce: pallas::Base,
        ticket_price: u64,
        token_id: pallas::Base,
    ) -> Result<CommitTicketResult, Box<dyn std::error::Error>> {
        let call_data_input = CommitTicketV1CallData::new(lottery_id, player_pub, ticket_price, nonce);

        let (proof, public_inputs) = create_commit_ticket_v1_proof(
            &self.commit_ticket_zkbin,
            &self.commit_ticket_pk,
            &call_data_input,
        )?;

        // Contract-level commitment: iterative hash of lottery_id + sorted numbers + nonce.
        let mut sorted_numbers = numbers.clone();
        sorted_numbers.sort_unstable();
        let mut state = lottery_id;
        for &n in &sorted_numbers {
            state = poseidon_hash([state, pallas::Base::from(n as u64)]);
        }
        let commitment = poseidon_hash([state, nonce]);

        let params = BuyTicketParamsV1 {
            player_pub,
            commitment,
            token_id,
            value: ticket_price,
            value_commit: pallas::Point::identity(),
            signature: pallas::Base::zero(),
            instance_seed: [0u8; 32],
            lottery_id,
            nonce,
        };

        let mut call_data = vec![0x01]; // BuyTicketV1
        call_data.extend_from_slice(&params.encode());

        Ok(CommitTicketResult { call_data, proof, public_inputs })
    }

    /// Draw winners (house-auth, function code 0x02).
    pub fn draw_winners(
        &self,
        lottery_id: pallas::Base,
        house_secret: pallas::Base,
        nonce: pallas::Base,
    ) -> Result<DrawWinnersResult, Box<dyn std::error::Error>> {
        let data = HouseAuthCallData::new(lottery_id, house_secret);
        let (proof, _public_inputs) = create_house_auth_proof(
            &self.draw_winners_zkbin,
            &self.draw_winners_pk,
            &data,
        )?;

        let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
        let params = DrawWinnersParamsV1 {
            lottery_id,
            nonce,
            house_pub,
            house_nullifier: data.house_nullifier,
        };

        let mut call_data = vec![0x02]; // DrawWinnersV1
        call_data.extend_from_slice(&params.encode());

        Ok(DrawWinnersResult { call_data, proof })
    }

    /// Reveal a ticket (for RevealTicketV1, 0x03). The reveal proof is tx_binding-only.
    pub fn reveal_ticket(
        &self,
        ticket_id: pallas::Base,
        numbers: Vec<u8>,
        nonce: pallas::Base,
    ) -> Result<RevealTicketResult, Box<dyn std::error::Error>> {
        let call_data_input = RevealTicketV1CallData::new();
        let (proof, _public_inputs) = create_reveal_ticket_v1_proof(
            &self.reveal_ticket_zkbin,
            &self.reveal_ticket_pk,
            &call_data_input,
        )?;

        let params = RevealTicketParamsV1 {
            ticket_id,
            numbers,
            nonce,
            revealed_commitment: pallas::Base::zero(),
            matches: 0,
        };

        let mut call_data = vec![0x03]; // RevealTicketV1
        call_data.extend_from_slice(&params.encode());

        Ok(RevealTicketResult { call_data, proof })
    }

    /// Claim a prize (for ClaimPrizeV1, 0x04).
    pub fn claim_prize(
        &self,
        ticket_id: pallas::Base,
        ticket_secret: pallas::Base,
        tier: u8,
        matches: u8,
    ) -> Result<ClaimPrizeResult, Box<dyn std::error::Error>> {
        let call_data_input = ClaimPrizeCallData::new(ticket_id, ticket_secret);
        let (proof, public_inputs) = create_claim_prize_proof(
            &self.claim_prize_zkbin,
            &self.claim_prize_pk,
            &call_data_input,
        )?;

        let params = ClaimPrizeParamsV1 {
            ticket_id,
            proof: vec![],
            tier,
            matches,
            computed_commit: public_inputs.computed_commit,
        };

        let mut call_data = vec![0x04]; // ClaimPrizeV1
        call_data.extend_from_slice(&params.encode());

        Ok(ClaimPrizeResult { call_data, proof })
    }

    /// Expire a lottery (house-auth, function code 0x05).
    pub fn expire_lottery(
        &self,
        lottery_id: pallas::Base,
        house_secret: pallas::Base,
    ) -> Result<ExpireLotteryResult, Box<dyn std::error::Error>> {
        let data = HouseAuthCallData::new(lottery_id, house_secret);
        let (proof, _public_inputs) = create_house_auth_proof(
            &self.expire_lottery_zkbin,
            &self.expire_lottery_pk,
            &data,
        )?;

        let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
        let params = ExpireLotteryParamsV1 {
            lottery_id,
            house_pub,
            house_nullifier: data.house_nullifier,
        };

        let mut call_data = vec![0x05]; // ExpireLotteryV1
        call_data.extend_from_slice(&params.encode());

        Ok(ExpireLotteryResult { call_data, proof })
    }
}

impl super::ContractHarness for LotteryHarness {
    fn name(&self) -> &str {
        "lottery"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitTicketV2", "RevealTicketV2", "ClaimPrizeV2", "DrawWinnersV2", "ExpireLotteryV2", "InitializeV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitTicketV2" => Some(&self.commit_ticket_zkbin),
            "RevealTicketV2" => Some(&self.reveal_ticket_zkbin),
            "ClaimPrizeV2" => Some(&self.claim_prize_zkbin),
            "DrawWinnersV2" => Some(&self.draw_winners_zkbin),
            "ExpireLotteryV2" => Some(&self.expire_lottery_zkbin),
            "InitializeV2" => Some(&self.initialize_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitTicketV2" => Some(&self.commit_ticket_pk),
            "RevealTicketV2" => Some(&self.reveal_ticket_pk),
            "ClaimPrizeV2" => Some(&self.claim_prize_pk),
            "DrawWinnersV2" => Some(&self.draw_winners_pk),
            "ExpireLotteryV2" => Some(&self.expire_lottery_pk),
            "InitializeV2" => Some(&self.initialize_pk),
            _ => None,
        }
    }
}

/// Result of initialize (non-ZK)
pub struct InitializeResult {
    /// Encoded call data for InitializeV1 (0x00)
    pub call_data: Vec<u8>,
}

/// Result of commit_ticket
pub struct CommitTicketResult {
    /// Encoded call data for BuyTicketV1 (0x01)
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation (ticket_id available here)
    pub public_inputs: CommitTicketV1PublicInputs,
}

/// Result of reveal_ticket
pub struct RevealTicketResult {
    /// Encoded call data for RevealTicketV1 (0x03)
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
}

/// Result of draw_winners
pub struct DrawWinnersResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
/// Result of claim_prize
pub struct ClaimPrizeResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
/// Result of expire_lottery
pub struct ExpireLotteryResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
