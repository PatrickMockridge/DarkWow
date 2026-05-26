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

//! DarkToshi Dice Test Harness
//!
//! Provides isolated testing for DarkToshi Dice contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Blind, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;

use dwow_darktoshi_dice_contract::client::{
    commit_bet_v1::{create_commit_bet_v1_proof, CommitBetV1CallData, CommitBetV1PublicInputs},
    settle_bet_v1::{create_settle_bet_v1_proof, SettleBetV1CallData, SettleBetV1PublicInputs},
};
use dwow_darktoshi_dice_contract::model::{
    CommitBetParamsV1, RevealRollParamsV1, SettleBetParamsV1,
};

/// DarkToshiDice Harness for isolated testing
pub struct DarkToshiDiceHarness {
    /// CommitBet_V1 ZkBinary
    commit_bet_zkbin: ZkBinary,
    /// CommitBet_V1 ProvingKey
    commit_bet_pk: ProvingKey,
    /// SettleBet_V1 ZkBinary
    settle_bet_zkbin: ZkBinary,
    /// SettleBet_V1 ProvingKey
    settle_bet_pk: ProvingKey,
}

impl DarkToshiDiceHarness {
    /// Spawn a new DarkToshiDice harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_bet_bin = include_bytes!("../../../darktoshi_dice/proof/commit_bet_v1.zk.bin");
        let settle_bet_bin = include_bytes!("../../../darktoshi_dice/proof/settle_bet_v1.zk.bin");

        let commit_bet_zkbin = ZkBinary::decode(commit_bet_bin, false).unwrap();
        let settle_bet_zkbin = ZkBinary::decode(settle_bet_bin, false).unwrap();

        let commit_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&commit_bet_zkbin).unwrap(),
            &commit_bet_zkbin,
        );
        let settle_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&settle_bet_zkbin).unwrap(),
            &settle_bet_zkbin,
        );

        let commit_bet_pk = ProvingKey::build(commit_bet_zkbin.k, &commit_bet_circuit);
        let settle_bet_pk = ProvingKey::build(settle_bet_zkbin.k, &settle_bet_circuit);

        Self { commit_bet_zkbin, commit_bet_pk, settle_bet_zkbin, settle_bet_pk }
    }
}

impl super::ContractHarness for DarkToshiDiceHarness {
    fn name(&self) -> &str {
        "darktoshi_dice"
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

impl DarkToshiDiceHarness {
    /// Commit to a bet
    pub fn commit_bet(
        &self,
        player_pub: PublicKey,
        bet_value: u64,
        target: u8,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        token_id: pallas::Base,
        house_edge: u32,
    ) -> Result<CommitBetResult> {
        // Generate random value blind for Pedersen commitment
        let value_blind = pallas::Scalar::random(&mut OsRng);

        let input = CommitBetV1CallData::new(
            player_pub,
            bet_value,
            target,
            secret_nonce,
            blind,
            token_id,
            house_edge,
            value_blind,
        );

        let (proof, public_inputs) =
            create_commit_bet_v1_proof(&self.commit_bet_zkbin, &self.commit_bet_pk, &input)?;

        // Create proper value commitment using Pedersen commitment
        let value_commit = pedersen_commitment_u64(bet_value, Blind(value_blind));

        // Create signature as poseidon hash of bet parameters
        let signature = poseidon_hash([
            pallas::Base::from(bet_value),
            secret_nonce,
            blind,
        ]);

        let params = CommitBetParamsV1 {
            player_pub,
            bet_value,
            target,
            secret_nonce,
            blind,
            token_id,
            value_commit,
            signature,
            house_edge,
            confirmation_depth: 3,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CommitBetResult { call_data, public_inputs, proof })
    }

    /// Reveal the roll for a committed bet (no ZK proof needed)
    pub fn reveal_roll(
        &self,
        bet_id: pallas::Base,
        secret_nonce: pallas::Base,
    ) -> Result<RevealRollResult> {
        let params = RevealRollParamsV1 { bet_id, secret_nonce };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(RevealRollResult { call_data })
    }

    /// Settle a bet (proves knowledge of secret without revealing it)
    pub fn settle_bet(
        &self,
        bet_id: pallas::Base,
        secret_nonce: pallas::Base,
        player_pub_x: pallas::Base,
        player_pub_y: pallas::Base,
        bet_value: pallas::Base,
        target: pallas::Base,
        token_id: pallas::Base,
        blind: pallas::Base,
    ) -> Result<SettleBetResult> {
        let input = SettleBetV1CallData::new(
            bet_id,
            secret_nonce,
            player_pub_x,
            player_pub_y,
            bet_value,
            target,
            token_id,
            blind,
        );

        let (proof, public_inputs) =
            create_settle_bet_v1_proof(&self.settle_bet_zkbin, &self.settle_bet_pk, &input)?;

        // Build SettleBetParamsV1 for call_data
        let params = SettleBetParamsV1 { bet_id, proof: vec![], roll_hash: pallas::Base::zero() };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(SettleBetResult { call_data, public_inputs, proof })
    }
}

/// Result of commit_bet
pub struct CommitBetResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CommitBetV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of reveal_roll
pub struct RevealRollResult {
    pub call_data: Vec<u8>,
}

/// Result of settle_bet
pub struct SettleBetResult {
    pub call_data: Vec<u8>,
    pub public_inputs: SettleBetV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}