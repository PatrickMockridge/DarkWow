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

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, PublicKey},
    pasta::pallas,
};
use darkfi_serial::Encodable;

use darkfi_slot_contract::model::{CommitSpinParamsV1, RevealSpinParamsV1};

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
}

impl SlotHarness {
    /// Spawn a new Slot harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_bet_bin = include_bytes!("../../../slot/proof/commit_bet_v1.zk.bin");
        let settle_bet_bin = include_bytes!("../../../slot/proof/settle_bet_v1.zk.bin");

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

impl super::ContractHarness for SlotHarness {
    fn name(&self) -> &str {
        "slot"
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

impl SlotHarness {
    /// Initialize the slot contract
    pub fn initialize(&self) -> Result<InitializeResult> {
        // InitializeV1 takes no params, just empty call_data
        let call_data = vec![];
        Ok(InitializeResult { call_data })
    }

    /// Commit a spin
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
        };
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CommitSpinResult { call_data })
    }

    /// Reveal a spin
    pub fn reveal_spin(
        &self,
        spin_id: pallas::Base,
        secret_nonce: pallas::Base,
    ) -> Result<RevealSpinResult> {
        let params = RevealSpinParamsV1 { spin_id, secret_nonce };
        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(RevealSpinResult { call_data })
    }
}

/// Result of initialize
pub struct InitializeResult {
    pub call_data: Vec<u8>,
}

/// Result of commit_spin
pub struct CommitSpinResult {
    pub call_data: Vec<u8>,
}

/// Result of reveal_spin
pub struct RevealSpinResult {
    pub call_data: Vec<u8>,
}