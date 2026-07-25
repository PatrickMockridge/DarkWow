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

//! Slot Test Harness
//!
//! Provides isolated testing for Slot contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_slot_contract::client::{
    commit_bet_v1::{CommitBetV1CallData, CommitBetV1PublicInputs, create_commit_bet_v1_proof},
    settle_bet_v1::{SettleBetV1CallData, SettleBetV1PublicInputs, create_settle_bet_v1_proof},
    reveal_spin_v1::{RevealSpinCallData, RevealSpinPublicInputs, create_reveal_spin_proof},
};
use dwow_slot_contract::model::{
    CommitSpinParamsV1, RevealSpinParamsV1,
};

/// Slot Harness for isolated testing
pub struct SlotHarness {
    /// CommitBet_V1 ZkBinary
    commit_bet_zkbin: ZkBinary,
    /// CommitBet_V1 ProvingKey
    commit_bet_pk: ProvingKey,
    /// SettleBet_V1 ZkBinary
    settle_bet_zkbin: ZkBinary,
    /// SettleBet_V1 ProvingKey
    settle_bet_pk: ProvingKey,
    /// RevealSpin_V1 ZkBinary
    reveal_spin_zkbin: ZkBinary,
    /// RevealSpin_V1 ProvingKey
    reveal_spin_pk: ProvingKey,
}

impl SlotHarness {
    /// Spawn a new Slot harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_bet_bin = include_bytes!("../../../slot/proof/commit_bet_v1.zk.bin");
        let settle_bet_bin = include_bytes!("../../../slot/proof/settle_bet_v1.zk.bin");
        let reveal_spin_bin = include_bytes!("../../../slot/proof/reveal_spin_v1.zk.bin");

        let commit_bet_zkbin = ZkBinary::decode(commit_bet_bin, false).unwrap();
        let settle_bet_zkbin = ZkBinary::decode(settle_bet_bin, false).unwrap();
        let reveal_spin_zkbin = ZkBinary::decode(reveal_spin_bin, false).unwrap();

        let commit_bet_pk = ProvingKey::build(
            commit_bet_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&commit_bet_zkbin).unwrap(), &commit_bet_zkbin),
        ).expect("ProvingKey::build failed");
        let settle_bet_pk = ProvingKey::build(
            settle_bet_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&settle_bet_zkbin).unwrap(), &settle_bet_zkbin),
        ).expect("ProvingKey::build failed");
        let reveal_spin_pk = ProvingKey::build(
            reveal_spin_zkbin.k,
            &ZkCircuit::new(dwow_core::zk::empty_witnesses(&reveal_spin_zkbin).unwrap(), &reveal_spin_zkbin),
        ).expect("ProvingKey::build failed");

        Self {
            commit_bet_zkbin, commit_bet_pk,
            settle_bet_zkbin, settle_bet_pk,
            reveal_spin_zkbin, reveal_spin_pk,
        }
    }

    /// Initialize the slot machine (non-ZK, function code 0x00)
    pub fn initialize(&self) -> Result<InitializeResult> {
        let call_data = vec![];
        Ok(InitializeResult { call_data })
    }

    /// Commit a spin with ZK proof (function code 0x01)
    pub fn commit_spin(
        &self,
        player_pub: PublicKey,
        bet_value: u64,
        paylines_played: u32,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        house_edge: u32,
        confirmation_depth: u8,
        token_id: pallas::Base,
        value_commit: pallas::Point,
    ) -> Result<CommitSpinResult> {
        let input = CommitBetV1CallData::new(
            player_pub, bet_value, paylines_played, secret_nonce,
            blind, token_id, house_edge,
        );
        let (proof, public_inputs) = create_commit_bet_v1_proof(
            &self.commit_bet_zkbin, &self.commit_bet_pk, &input,
        )?;

        let params = CommitSpinParamsV1 {
            player_pub,
            bet_value,
            paylines_played,
            secret_nonce,
            blind,
            house_edge,
            confirmation_depth,
            token_id,
            value_commit,
            instance_seed: [0u8; 32],
        };
        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;

        Ok(CommitSpinResult { call_data, proof, public_inputs })
    }

    /// Reveal a spin with ZK proof (function code 0x02)
    pub fn reveal_spin(
        &self,
        spin_id: pallas::Base,
        secret_nonce: pallas::Base,
    ) -> Result<RevealSpinResult> {
        let input = RevealSpinCallData::new();
        let (proof, public_inputs) = create_reveal_spin_proof(
            &self.reveal_spin_zkbin, &self.reveal_spin_pk, &input,
        )?;

        let params = RevealSpinParamsV1 { spin_id, secret_nonce };
        let mut call_data = vec![0x02];
        params.encode(&mut call_data)?;

        Ok(RevealSpinResult { call_data, proof, public_inputs })
    }

    /// Settle a bet with ZK proof (function code 0x03)
    pub fn settle_bet(
        &self,
        player_pub: PublicKey,
        bet_value: u64,
        paylines: u32,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        token_id: pallas::Base,
    ) -> Result<SettleBetResult> {
        let input = SettleBetV1CallData::new(
            player_pub, bet_value, paylines, secret_nonce, blind, token_id,
        );
        let (proof, public_inputs) = create_settle_bet_v1_proof(
            &self.settle_bet_zkbin, &self.settle_bet_pk, &input,
        )?;

        // Build minimal call_data for settle_bet
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(bet_value.to_le_bytes().as_ref());

        Ok(SettleBetResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for SlotHarness {
    fn name(&self) -> &str {
        "slot"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitBetV1", "SettleBetV1", "RevealSpinV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitBetV1" => Some(&self.commit_bet_zkbin),
            "SettleBetV1" => Some(&self.settle_bet_zkbin),
            "RevealSpinV1" => Some(&self.reveal_spin_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitBetV1" => Some(&self.commit_bet_pk),
            "SettleBetV1" => Some(&self.settle_bet_pk),
            "RevealSpinV1" => Some(&self.reveal_spin_pk),
            _ => None,
        }
    }
}

/// Result of initialize
pub struct InitializeResult {
    pub call_data: Vec<u8>,
}

/// Result of commit_spin
pub struct CommitSpinResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CommitBetV1PublicInputs,
}

/// Result of reveal_spin
pub struct RevealSpinResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: RevealSpinPublicInputs,
}

/// Result of settle_bet
pub struct SettleBetResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SettleBetV1PublicInputs,
}
