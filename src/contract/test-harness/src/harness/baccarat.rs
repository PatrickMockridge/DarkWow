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

//! Baccarat Test Harness
//!
//! Provides isolated testing for Baccarat contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::{Field, Group}, poseidon_hash, PublicKey, SecretKey},
    crypto::schnorr::SchnorrSecret,
    pasta::pallas,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;

use darkfi_baccarat_contract::client::{
    commit_bet_v1::{CommitBetV1CallData, CommitBetV1PublicInputs, create_commit_bet_v1_proof},
    settle_bet_v1::{SettleBetV1CallData, SettleBetV1PublicInputs, create_settle_bet_v1_proof},
};
use darkfi_baccarat_contract::model::{
    derive_bet_id, BetId, BetType, CommitBetParamsV1, DrawCardsParamsV1, HouseCloseParamsV1,
    SettleBetParamsV1,
};

/// Baccarat Harness for isolated testing
pub struct BaccaratHarness {
    /// CommitBet_V1 ZkBinary
    commit_bet_zkbin: ZkBinary,
    /// CommitBet_V1 ProvingKey
    commit_bet_pk: ProvingKey,
    /// SettleBet_V1 ZkBinary
    settle_bet_zkbin: ZkBinary,
    /// SettleBet_V1 ProvingKey
    settle_bet_pk: ProvingKey,
}

impl BaccaratHarness {
    /// Spawn a new Baccarat harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_bet_bin = include_bytes!("../../../baccarat/proof/commit_bet_v1.zk.bin");
        let settle_bet_bin = include_bytes!("../../../baccarat/proof/settle_bet_v1.zk.bin");

        let commit_bet_zkbin = ZkBinary::decode(commit_bet_bin, false).unwrap();
        let settle_bet_zkbin = ZkBinary::decode(settle_bet_bin, false).unwrap();

        let commit_bet_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&commit_bet_zkbin).unwrap(),
            &commit_bet_zkbin,
        );
        let settle_bet_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&settle_bet_zkbin).unwrap(),
            &settle_bet_zkbin,
        );

        let commit_bet_pk = ProvingKey::build(commit_bet_zkbin.k, &commit_bet_circuit);
        let settle_bet_pk = ProvingKey::build(settle_bet_zkbin.k, &settle_bet_circuit);

        Self { commit_bet_zkbin, commit_bet_pk, settle_bet_zkbin, settle_bet_pk }
    }
}

impl BaccaratHarness {
    /// Create a bet commitment with ZK proof and return encoded call data
    ///
    /// # Arguments
    /// * `player_pub` - Player's public key
    /// * `bet_value` - Amount to bet
    /// * `bet_type` - Type of bet (0=Player, 1=Banker, 2=Tie)
    /// * `secret_nonce` - Secret nonce for randomness
    /// * `blind` - Blinding factor
    /// * `token_id` - Token ID being wagered
    /// * `house_edge` - House edge in basis points
    /// * `confirmation_depth` - Confirmation depth for randomness
    pub fn commit_bet(
        &self,
        player_pub: PublicKey,
        bet_value: u64,
        bet_type: BetType,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        token_id: pallas::Base,
        house_edge: u32,
        confirmation_depth: u8,
    ) -> Result<CommitBetResult, Box<dyn std::error::Error>> {
        // Generate random value blind for Pedersen commitment
        let value_blind = pallas::Scalar::random(&mut rand::rngs::OsRng);

        let input = CommitBetV1CallData::new(
            player_pub,
            bet_value,
            bet_type as u8,
            secret_nonce,
            blind,
            token_id,
            value_blind,
        );

        let (proof, public_inputs) = create_commit_bet_v1_proof(
            &self.commit_bet_zkbin,
            &self.commit_bet_pk,
            &input,
        )?;

        // Create value commitment using Pedersen
        let value_commit = darkfi_sdk::crypto::pedersen_commitment_u64(
            bet_value,
            darkfi_sdk::crypto::Blind(value_blind),
        );

        // Derive bet_id
        let bet_id = derive_bet_id(
            &player_pub,
            bet_type as u8,
            bet_value,
            secret_nonce,
            blind,
            token_id,
        );

        // Build CommitBetParamsV1
        let params = CommitBetParamsV1 {
            player_pub,
            bet_type: bet_type as u8,
            bet_value,
            secret_nonce,
            blind,
            token_id,
            house_edge,
            confirmation_depth,
            value_commit,
        };

        // Encode call data (function_id will be added by pipeline.exec())
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CommitBetResult { call_data, proof, public_inputs, bet_id })
    }

    /// Create draw cards call data (no ZK proof needed)
    pub fn draw_cards(
        &self,
        bet_id: BetId,
        secret_nonce: pallas::Base,
    ) -> Result<DrawCardsResult, Box<dyn std::error::Error>> {
        let params = DrawCardsParamsV1 { bet_id, secret_nonce };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(DrawCardsResult { call_data, bet_id })
    }

    /// Create a settle bet proof and call data
    pub fn settle_bet(
        &self,
        bet_id: BetId,
        secret_nonce: pallas::Base,
        player_pub: PublicKey,
        bet_value: u64,
        bet_type: BetType,
        token_id: pallas::Base,
        blind: pallas::Base,
    ) -> Result<SettleBetResult, Box<dyn std::error::Error>> {
        let input = SettleBetV1CallData::new(
            bet_id,
            secret_nonce,
            player_pub,
            bet_value,
            bet_type as u8,
            token_id,
            blind,
        );

        let (proof, public_inputs) = create_settle_bet_v1_proof(
            &self.settle_bet_zkbin,
            &self.settle_bet_pk,
            &input,
        )?;

        // Build SettleBetParamsV1
        let params = SettleBetParamsV1 { bet_id };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(SettleBetResult { call_data, proof, public_inputs })
    }

    /// Create house close call data with signature authorization
    pub fn house_close(
        &self,
        bet_id: BetId,
        house_secret: SecretKey,
        house_pub: PublicKey,
    ) -> Result<HouseCloseResult, Box<dyn std::error::Error>> {
        let current_block = 1000u64; // Placeholder - would be set from blockchain state

        // Create signature over (bet_id, current_block)
        let signature_msg = {
            let mut msg = vec![];
            bet_id.encode(&mut msg)?;
            current_block.encode(&mut msg)?;
            msg
        };

        let signature = house_secret.sign(&signature_msg);

        let params = HouseCloseParamsV1 { bet_id, house_pub, signature };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(HouseCloseResult { call_data, bet_id })
    }
}

impl super::ContractHarness for BaccaratHarness {
    fn name(&self) -> &str {
        "baccarat"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitBetV1", "SettleBetV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitBetV1" => Some(&self.commit_bet_zkbin),
            "SettleBetV1" => Some(&self.settle_bet_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitBetV1" => Some(&self.commit_bet_pk),
            "SettleBetV1" => Some(&self.settle_bet_pk),
            _ => None,
        }
    }
}

// ============================================================================
// Result Structs
// ============================================================================

/// Result of commit_bet
pub struct CommitBetResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: darkfi::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: CommitBetV1PublicInputs,
    /// Derived bet ID
    pub bet_id: BetId,
}

/// Result of draw_cards
pub struct DrawCardsResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// Bet ID
    pub bet_id: BetId,
}

/// Result of settle_bet
pub struct SettleBetResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// ZK proof
    pub proof: darkfi::zk::Proof,
    /// Public inputs from proof generation
    pub public_inputs: SettleBetV1PublicInputs,
}

/// Result of house_close
pub struct HouseCloseResult {
    /// Encoded call data for contract execution
    pub call_data: Vec<u8>,
    /// Bet ID
    pub bet_id: BetId,
}