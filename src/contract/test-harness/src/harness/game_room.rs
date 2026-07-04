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
    /// Call ZkBinary
    call_zkbin: ZkBinary,
    /// Call ProvingKey
    call_pk: ProvingKey,
    /// ClosePot ZkBinary
    close_pot_zkbin: ZkBinary,
    /// ClosePot ProvingKey
    close_pot_pk: ProvingKey,
    /// ContributeEntropy ZkBinary
    contribute_entropy_zkbin: ZkBinary,
    /// ContributeEntropy ProvingKey
    contribute_entropy_pk: ProvingKey,
    /// Fold ZkBinary
    fold_zkbin: ZkBinary,
    /// Fold ProvingKey
    fold_pk: ProvingKey,
    /// Raise ZkBinary
    raise_zkbin: ZkBinary,
    /// Raise ProvingKey
    raise_pk: ProvingKey,
    /// Withdraw ZkBinary
    withdraw_zkbin: ZkBinary,
    /// Withdraw ProvingKey
    withdraw_pk: ProvingKey,
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
        let call_bin =
            include_bytes!("../../../game_room/proof/call_v1.zk.bin");
        let close_pot_bin =
            include_bytes!("../../../game_room/proof/close_pot_v1.zk.bin");
        let contribute_entropy_bin =
            include_bytes!("../../../game_room/proof/contribute_entropy_v1.zk.bin");
        let fold_bin =
            include_bytes!("../../../game_room/proof/fold_v1.zk.bin");
        let raise_bin =
            include_bytes!("../../../game_room/proof/raise_v1.zk.bin");
        let withdraw_bin =
            include_bytes!("../../../game_room/proof/withdraw_v1.zk.bin");

        let create_room_zkbin = ZkBinary::decode(create_room_bin, false).unwrap();
        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let place_bet_zkbin = ZkBinary::decode(place_bet_bin, false).unwrap();
        let settle_pot_zkbin = ZkBinary::decode(settle_pot_bin, false).unwrap();
        let claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let call_zkbin = ZkBinary::decode(call_bin, false).unwrap();
        let close_pot_zkbin = ZkBinary::decode(close_pot_bin, false).unwrap();
        let contribute_entropy_zkbin = ZkBinary::decode(contribute_entropy_bin, false).unwrap();
        let fold_zkbin = ZkBinary::decode(fold_bin, false).unwrap();
        let raise_zkbin = ZkBinary::decode(raise_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();

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
        let call_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&call_zkbin).unwrap(),
            &call_zkbin,
        );
        let close_pot_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&close_pot_zkbin).unwrap(),
            &close_pot_zkbin,
        );
        let contribute_entropy_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&contribute_entropy_zkbin).unwrap(),
            &contribute_entropy_zkbin,
        );
        let fold_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&fold_zkbin).unwrap(),
            &fold_zkbin,
        );
        let raise_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&raise_zkbin).unwrap(),
            &raise_zkbin,
        );
        let withdraw_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&withdraw_zkbin).unwrap(),
            &withdraw_zkbin,
        );

        let create_room_pk = ProvingKey::build(create_room_zkbin.k, &create_room_circuit).expect("ProvingKey::build failed");
        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &deposit_circuit).expect("ProvingKey::build failed");
        let place_bet_pk = ProvingKey::build(place_bet_zkbin.k, &place_bet_circuit).expect("ProvingKey::build failed");
        let settle_pot_pk = ProvingKey::build(settle_pot_zkbin.k, &settle_pot_circuit).expect("ProvingKey::build failed");
        let claim_pk = ProvingKey::build(claim_zkbin.k, &claim_circuit).expect("ProvingKey::build failed");
        let call_pk = ProvingKey::build(call_zkbin.k, &call_circuit).expect("ProvingKey::build failed");
        let close_pot_pk = ProvingKey::build(close_pot_zkbin.k, &close_pot_circuit).expect("ProvingKey::build failed");
        let contribute_entropy_pk = ProvingKey::build(contribute_entropy_zkbin.k, &contribute_entropy_circuit).expect("ProvingKey::build failed");
        let fold_pk = ProvingKey::build(fold_zkbin.k, &fold_circuit).expect("ProvingKey::build failed");
        let raise_pk = ProvingKey::build(raise_zkbin.k, &raise_circuit).expect("ProvingKey::build failed");
        let withdraw_pk = ProvingKey::build(withdraw_zkbin.k, &withdraw_circuit).expect("ProvingKey::build failed");

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
            call_zkbin,
            call_pk,
            close_pot_zkbin,
            close_pot_pk,
            contribute_entropy_zkbin,
            contribute_entropy_pk,
            fold_zkbin,
            fold_pk,
            raise_zkbin,
            raise_pk,
            withdraw_zkbin,
            withdraw_pk,
        }
    }
}

impl super::ContractHarness for GameRoomHarness {
    fn name(&self) -> &str {
        "game_room"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateRoom", "Deposit", "PlaceBet", "SettlePot", "Claim",
             "CallV1", "ClosePotV1", "ContributeEntropyV1", "FoldV1", "RaiseV1", "WithdrawV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateRoom" => Some(&self.create_room_zkbin),
            "Deposit" => Some(&self.deposit_zkbin),
            "PlaceBet" => Some(&self.place_bet_zkbin),
            "SettlePot" => Some(&self.settle_pot_zkbin),
            "Claim" => Some(&self.claim_zkbin),
            "CallV1" => Some(&self.call_zkbin),
            "ClosePotV1" => Some(&self.close_pot_zkbin),
            "ContributeEntropyV1" => Some(&self.contribute_entropy_zkbin),
            "FoldV1" => Some(&self.fold_zkbin),
            "RaiseV1" => Some(&self.raise_zkbin),
            "WithdrawV1" => Some(&self.withdraw_zkbin),
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
            "CallV1" => Some(&self.call_pk),
            "ClosePotV1" => Some(&self.close_pot_pk),
            "ContributeEntropyV1" => Some(&self.contribute_entropy_pk),
            "FoldV1" => Some(&self.fold_pk),
            "RaiseV1" => Some(&self.raise_pk),
            "WithdrawV1" => Some(&self.withdraw_pk),
            _ => None,
        }
    }
}
