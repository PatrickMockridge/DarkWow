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

//! GameRoom Test Harness
//!
//! Provides isolated testing for GameRoom contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// GameRoom Harness for isolated testing
pub struct GameRoomHarness {
    /// CreateRoom ZkBinary
    create_room_zkbin: ZkBinary,
    /// CreateRoom ProvingKey
    create_room_pk: ProvingKey,
    /// Deposit ZkBinary
    deposit_zkbin: ZkBinary,
    /// Deposit ProvingKey
    deposit_pk: ProvingKey,
    /// PlaceBet ZkBinary
    place_bet_zkbin: ZkBinary,
    /// PlaceBet ProvingKey
    place_bet_pk: ProvingKey,
    /// SettlePot ZkBinary
    settle_pot_zkbin: ZkBinary,
    /// SettlePot ProvingKey
    settle_pot_pk: ProvingKey,
    /// Claim ZkBinary
    claim_zkbin: ZkBinary,
    /// Claim ProvingKey
    claim_pk: ProvingKey,
}

impl GameRoomHarness {
    /// Spawn a new GameRoom harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_room_bin =
            include_bytes!("../../../game_room/proof/create_room_v1.zk.bin");
        let deposit_bin =
            include_bytes!("../../../game_room/proof/deposit_v1.zk.bin");
        let place_bet_bin =
            include_bytes!("../../../game_room/proof/place_bet_v1.zk.bin");
        let settle_pot_bin =
            include_bytes!("../../../game_room/proof/settle_pot_v1.zk.bin");
        let claim_bin =
            include_bytes!("../../../game_room/proof/claim_v1.zk.bin");

        let create_room_zkbin = ZkBinary::decode(create_room_bin, false).unwrap();
        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let place_bet_zkbin = ZkBinary::decode(place_bet_bin, false).unwrap();
        let settle_pot_zkbin = ZkBinary::decode(settle_pot_bin, false).unwrap();
        let claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();

        let create_room_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_room_zkbin).unwrap(),
            &create_room_zkbin,
        );
        let deposit_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&deposit_zkbin).unwrap(),
            &deposit_zkbin,
        );
        let place_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&place_bet_zkbin).unwrap(),
            &place_bet_zkbin,
        );
        let settle_pot_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&settle_pot_zkbin).unwrap(),
            &settle_pot_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&claim_zkbin).unwrap(),
            &claim_zkbin,
        );

        let create_room_pk = ProvingKey::build(create_room_zkbin.k, &create_room_circuit);
        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &deposit_circuit);
        let place_bet_pk = ProvingKey::build(place_bet_zkbin.k, &place_bet_circuit);
        let settle_pot_pk = ProvingKey::build(settle_pot_zkbin.k, &settle_pot_circuit);
        let claim_pk = ProvingKey::build(claim_zkbin.k, &claim_circuit);

        Self {
            create_room_zkbin,
            create_room_pk,
            deposit_zkbin,
            deposit_pk,
            place_bet_zkbin,
            place_bet_pk,
            settle_pot_zkbin,
            settle_pot_pk,
            claim_zkbin,
            claim_pk,
        }
    }
}

impl super::ContractHarness for GameRoomHarness {
    fn name(&self) -> &str {
        "game_room"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateRoom", "Deposit", "PlaceBet", "SettlePot", "Claim"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateRoom" => Some(&self.create_room_zkbin),
            "Deposit" => Some(&self.deposit_zkbin),
            "PlaceBet" => Some(&self.place_bet_zkbin),
            "SettlePot" => Some(&self.settle_pot_zkbin),
            "Claim" => Some(&self.claim_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateRoom" => Some(&self.create_room_pk),
            "Deposit" => Some(&self.deposit_pk),
            "PlaceBet" => Some(&self.place_bet_pk),
            "SettlePot" => Some(&self.settle_pot_pk),
            "Claim" => Some(&self.claim_pk),
            _ => None,
        }
    }
}
