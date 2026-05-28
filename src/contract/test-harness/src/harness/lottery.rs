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
    crypto::{pasta_prelude::Group, PublicKey, poseidon_hash},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_lottery_contract::client::{
    commit_ticket_v1::{CommitTicketV1CallData, create_commit_ticket_v1_proof, CommitTicketV1PublicInputs},
    reveal_ticket_v1::{RevealTicketV1CallData, create_reveal_ticket_v1_proof, RevealTicketV1PublicInputs},
};
use dwow_lottery_contract::model::{BuyTicketParamsV1, RevealTicketParamsV1};

/// Lottery Harness for isolated testing
pub struct LotteryHarness {
    /// CommitTicket_V1 ZkBinary
    commit_ticket_zkbin: ZkBinary,
    /// CommitTicket_V1 ProvingKey
    commit_ticket_pk: ProvingKey,
    /// RevealTicket_V1 ZkBinary
    reveal_ticket_zkbin: ZkBinary,
    /// RevealTicket_V1 ProvingKey
    reveal_ticket_pk: ProvingKey,
}

impl LotteryHarness {
    /// Spawn a new Lottery harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_ticket_bin = include_bytes!("../../../lottery/proof/commit_ticket_v1.zk.bin");
        let reveal_ticket_bin = include_bytes!("../../../lottery/proof/reveal_ticket_v1.zk.bin");

        let commit_ticket_zkbin = ZkBinary::decode(commit_ticket_bin, false).unwrap();
        let reveal_ticket_zkbin = ZkBinary::decode(reveal_ticket_bin, false).unwrap();

        let commit_ticket_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&commit_ticket_zkbin).unwrap(),
            &commit_ticket_zkbin,
        );
        let reveal_ticket_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&reveal_ticket_zkbin).unwrap(),
            &reveal_ticket_zkbin,
        );

        let commit_ticket_pk = ProvingKey::build(commit_ticket_zkbin.k, &commit_ticket_circuit);
        let reveal_ticket_pk = ProvingKey::build(reveal_ticket_zkbin.k, &reveal_ticket_circuit);

        Self { commit_ticket_zkbin, commit_ticket_pk, reveal_ticket_zkbin, reveal_ticket_pk }
    }

    /// Commit a ticket (for BuyTicketV1, 0x01)
    ///
    /// Returns call_data encoding `BuyTicketParamsV1` and the ZK proof.
    /// The entrypoint expects money_v3::transfer_v1 as a child call.
    ///
    /// The `commitment` field in `BuyTicketParamsV1` is computed as:
    ///   Hash(...Hash(lottery_id, n1), n2..., nonce)
    /// This is the contract-level commitment verified by RevealTicketV1.
    /// It is independent of the ZK proof's ticket_id.
    pub fn commit_ticket(
        &self,
        player_pub: PublicKey,
        lottery_id: pallas::Base,
        numbers: Vec<u8>,
        nonce: pallas::Base, // secret nonce for the commitment
        ticket_price: u64,
        blind: pallas::Base,
        token_id: pallas::Base,
        secret_key: pallas::Base,
    ) -> Result<CommitTicketResult, Box<dyn std::error::Error>> {
        // Generate ZK proof for commit_ticket circuit
        let call_data_input = CommitTicketV1CallData::new(
            player_pub,
            ticket_price,
            nonce, // secret_nonce
            blind,
            token_id,
        );

        let (proof, public_inputs) = create_commit_ticket_v1_proof(
            &self.commit_ticket_zkbin,
            &self.commit_ticket_pk,
            &call_data_input,
        )?;

        // Compute contract-level commitment: iterative hash of lottery_id + numbers + nonce
        let mut sorted_numbers = numbers.clone();
        sorted_numbers.sort_unstable();
        let mut state = lottery_id;
        for &n in &sorted_numbers {
            state = poseidon_hash([state, pallas::Base::from(n as u64)]);
        }
        let commitment = poseidon_hash([state, nonce]);
        // Signature: H(commitment, secret_key)
        let signature = poseidon_hash([commitment, secret_key]);

        let params = BuyTicketParamsV1 {
            player_pub,
            commitment,
            token_id,
            value: ticket_price,
            value_commit: pallas::Point::identity(),
            signature,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![0x01]; // BuyTicketV1
        params.encode(&mut call_data)?;

        Ok(CommitTicketResult { call_data, proof, public_inputs })
    }

    /// Reveal a ticket (for RevealTicketV1, 0x03)
    pub fn reveal_ticket(
        &self,
        player_pub: PublicKey,
        ticket_price: u64,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        nonce: pallas::Base,
        random: pallas::Base,
        ticket_id: pallas::Base,
        numbers: Vec<u8>,
    ) -> Result<RevealTicketResult, Box<dyn std::error::Error>> {
        let call_data_input = RevealTicketV1CallData::new(
            player_pub,
            ticket_price,
            secret_nonce,
            blind,
            nonce,
            random,
        );

        let (proof, public_inputs) = create_reveal_ticket_v1_proof(
            &self.reveal_ticket_zkbin,
            &self.reveal_ticket_pk,
            &call_data_input,
        )?;

        let params = RevealTicketParamsV1 {
            ticket_id,
            numbers,
            nonce: secret_nonce, // secret_nonce is the commitment nonce
            revealed_commitment: pallas::Base::zero(),
            matches: 0,
        };

        let mut call_data = vec![0x03]; // RevealTicketV1
        params.encode(&mut call_data)?;

        Ok(RevealTicketResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for LotteryHarness {
    fn name(&self) -> &str {
        "lottery"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitTicketV1", "RevealTicketV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitTicketV1" => Some(&self.commit_ticket_zkbin),
            "RevealTicketV1" => Some(&self.reveal_ticket_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitTicketV1" => Some(&self.commit_ticket_pk),
            "RevealTicketV1" => Some(&self.reveal_ticket_pk),
            _ => None,
        }
    }
}

/// Result of commit_ticket
pub struct CommitTicketResult {
    /// Encoded call data for BuyTicketV1 (0x01)
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: CommitTicketV1PublicInputs,
}

/// Result of reveal_ticket
pub struct RevealTicketResult {
    /// Encoded call data for RevealTicketV1 (0x03)
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: dwow_core::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: RevealTicketV1PublicInputs,
}