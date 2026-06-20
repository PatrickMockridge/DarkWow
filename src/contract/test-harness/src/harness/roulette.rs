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

//! Roulette Test Harness
//!
//! Provides isolated testing for Roulette contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_roulette_contract::client::{
    place_bet_v1::{PlaceBetV1CallData, create_place_bet_v1_proof},
    settle_bet_v1::{SettleBetV1CallData, create_settle_bet_v1_proof},
};
use dwow_roulette_contract::model::{
    InitializeParamsV1, PlaceBetParamsV1, SpinWheelParamsV1,
    SettleBetsParamsV1, HouseCloseParamsV1,
};

/// Roulette Harness for isolated testing
pub struct RouletteHarness {
    /// PlaceBet_V1 ZkBinary
    place_bet_zkbin: ZkBinary,
    /// PlaceBet_V1 ProvingKey
    place_bet_pk: ProvingKey,
    /// SettleBet_V1 ZkBinary
    settle_bet_zkbin: ZkBinary,
    /// SettleBet_V1 ProvingKey
    settle_bet_pk: ProvingKey,
}

impl RouletteHarness {
    /// Spawn a new Roulette harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let place_bet_bin = include_bytes!("../../../roulette/proof/place_bet_v1.zk.bin");
        let settle_bet_bin = include_bytes!("../../../roulette/proof/settle_bet_v1.zk.bin");

        let place_bet_zkbin = ZkBinary::decode(place_bet_bin, false).unwrap();
        let settle_bet_zkbin = ZkBinary::decode(settle_bet_bin, false).unwrap();

        let place_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&place_bet_zkbin).unwrap(),
            &place_bet_zkbin,
        );
        let settle_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&settle_bet_zkbin).unwrap(),
            &settle_bet_zkbin,
        );

        let place_bet_pk = ProvingKey::build(place_bet_zkbin.k, &place_bet_circuit);
        let settle_bet_pk = ProvingKey::build(settle_bet_zkbin.k, &settle_bet_circuit);

        Self { place_bet_zkbin, place_bet_pk, settle_bet_zkbin, settle_bet_pk }
    }

    /// Initialize a roulette table
    pub fn initialize(
        &self,
        house_pub: PublicKey,
        american_wheel: bool,
        house_capital: u64,
        max_straight_bet: u64,
        duration_blocks: u64,
    ) -> Result<InitializeResult, Box<dyn std::error::Error>> {
        let params = InitializeParamsV1 {
            house_pub,
            american_wheel,
            house_capital,
            max_straight_bet,
            duration_blocks,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(InitializeResult { call_data })
    }

    /// Place a bet
    /// bet_type: 0=Straight, 1=Split, 2=Street, 3=Corner, 4=SixLine, 5=Dozen, 6=Column, 7=EvenMoney
    pub fn place_bet(
        &self,
        table_id: pallas::Base,
        player_pub: PublicKey,
        bet_type: u8,
        numbers: Vec<u8>,
        amount: u64,
        nonce: pallas::Base,
    ) -> Result<PlaceBetResult, Box<dyn std::error::Error>> {
        let input = PlaceBetV1CallData::new(
            table_id,
            player_pub,
            bet_type as u8,
            amount,
            nonce,
        );

        let (proof, public_inputs) = create_place_bet_v1_proof(
            &self.place_bet_zkbin,
            &self.place_bet_pk,
            &input,
        )?;

        // Signature for PlaceBetParamsV1 (simplified - uses poseidon_hash as signature)
        let signature = poseidon_hash([
            table_id,
            player_pub.x(),
            player_pub.y(),
            pallas::Base::from(amount),
        ]);

        let params = PlaceBetParamsV1 {
            table_id,
            player_pub,
            bet_type: unsafe { std::mem::transmute(bet_type) },
            numbers,
            amount,
            signature,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(PlaceBetResult { call_data, bet_id: public_inputs.bet_id, nullifier: public_inputs.nullifier, proof })
    }

    /// Spin the wheel
    pub fn spin_wheel(
        &self,
        table_id: pallas::Base,
        house_pub: PublicKey,
        nonce: pallas::Base,
    ) -> Result<SpinWheelResult, Box<dyn std::error::Error>> {
        let (hx, hy) = house_pub.xy();
        let params = SpinWheelParamsV1 {
            table_id,
            nonce,
            house_pub_x: hx,
            house_pub_y: hy,
            spin_nullifier: pallas::Base::zero(),
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(SpinWheelResult { call_data })
    }

    /// Settle bets
    pub fn settle_bets(
        &self,
        table_id: pallas::Base,
        bet_ids: Vec<pallas::Base>,
    ) -> Result<SettleBetsResult, Box<dyn std::error::Error>> {
        let params = SettleBetsParamsV1 {
            table_id,
            bet_ids: bet_ids.clone(),
            payout: 0,
        };

        // For settle bet, we need a proof for each bet
        // For simplicity, just create one settle proof
        let bet_id = bet_ids.first().copied().unwrap_or(pallas::Base::zero());
        let input = SettleBetV1CallData::new(table_id, bet_id, false, 0);

        let (proof, _public_inputs) = create_settle_bet_v1_proof(
            &self.settle_bet_zkbin,
            &self.settle_bet_pk,
            &input,
        )?;

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(SettleBetsResult { call_data, proof })
    }

    /// House close
    pub fn house_close(
        &self,
        table_id: pallas::Base,
        house_pub: PublicKey,
    ) -> Result<HouseCloseResult, Box<dyn std::error::Error>> {
        let (hx, hy) = house_pub.xy();
        let params = HouseCloseParamsV1 {
            table_id,
            house_pub_x: hx,
            house_pub_y: hy,
            close_nullifier: pallas::Base::zero(),
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(HouseCloseResult { call_data })
    }
}

impl super::ContractHarness for RouletteHarness {
    fn name(&self) -> &str {
        "roulette"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["PlaceBetV1", "SettleBetV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "PlaceBetV1" => Some(&self.place_bet_zkbin),
            "SettleBetV1" => Some(&self.settle_bet_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "PlaceBetV1" => Some(&self.place_bet_pk),
            "SettleBetV1" => Some(&self.settle_bet_pk),
            _ => None,
        }
    }
}

/// Result of initialize
pub struct InitializeResult {
    pub call_data: Vec<u8>,
}

/// Result of place_bet
pub struct PlaceBetResult {
    pub call_data: Vec<u8>,
    pub bet_id: pallas::Base,
    pub nullifier: pallas::Base,
    pub proof: dwow_core::zk::Proof,
}

/// Result of spin_wheel
pub struct SpinWheelResult {
    pub call_data: Vec<u8>,
}

/// Result of settle_bets
pub struct SettleBetsResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

/// Result of house_close
pub struct HouseCloseResult {
    pub call_data: Vec<u8>,
}