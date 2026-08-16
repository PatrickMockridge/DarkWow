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
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_game_room_contract::{
    client::{
        claim::{claim_v1_proof, ClaimCallData, ClaimPublicInputs},
        create_pot::{create_pot_v1_proof, CreatePotCallData, CreatePotPublicInputs},
        create_room::{create_room_v1_proof, CreateRoomCallData, CreateRoomPublicInputs},
        deposit::{deposit_v1_proof, DepositCallData, DepositPublicInputs},
        identity_proof::{create_identity_proof, IdentityCallData, IdentityPublicInputs},
        place_bet::{place_bet_v1_proof, PlaceBetCallData, PlaceBetPublicInputs},
        settle_pot::{settle_pot_v1_proof, SettlePotCallData, SettlePotPublicInputs},
    },
    model::{
        BetType, CallParamsV1, ClaimParamsV1, ClosePotParamsV1, ContributeEntropyParamsV1,
        CreatePotParamsV1, CreateRoomParamsV1, DepositParamsV1, EntropyMode, FoldParamsV1,
        PlaceBetParamsV1, RaiseParamsV1, SettlePotParamsV1, WithdrawParamsV1,
    },
};

/// GameRoom Harness for isolated testing
pub struct GameRoomHarness {
    create_room_zkbin: ZkBinary,
    create_room_pk: ProvingKey,
    deposit_zkbin: ZkBinary,
    deposit_pk: ProvingKey,
    place_bet_zkbin: ZkBinary,
    place_bet_pk: ProvingKey,
    settle_pot_zkbin: ZkBinary,
    settle_pot_pk: ProvingKey,
    claim_zkbin: ZkBinary,
    claim_pk: ProvingKey,
    call_zkbin: ZkBinary,
    call_pk: ProvingKey,
    close_pot_zkbin: ZkBinary,
    close_pot_pk: ProvingKey,
    contribute_entropy_zkbin: ZkBinary,
    contribute_entropy_pk: ProvingKey,
    fold_zkbin: ZkBinary,
    fold_pk: ProvingKey,
    raise_zkbin: ZkBinary,
    raise_pk: ProvingKey,
    withdraw_zkbin: ZkBinary,
    withdraw_pk: ProvingKey,
    create_pot_zkbin: ZkBinary,
    create_pot_pk: ProvingKey,
}

macro_rules! load_circuit {
    ($name:ident) => {{
        let bin = include_bytes!(concat!("../../../game_room/proof/", stringify!($name), ".zk.bin"));
        let zkbin = ZkBinary::decode(bin, false).unwrap();
        let circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(&zkbin).unwrap(), &zkbin);
        let pk = ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build failed");
        (zkbin, pk)
    }};
}

impl GameRoomHarness {
    pub fn spawn() -> Self {
        dwow_game_room_contract::enable_deterministic_zk();
        let (create_room_zkbin, create_room_pk) = load_circuit!(create_room);
        let (deposit_zkbin, deposit_pk) = load_circuit!(deposit);
        let (place_bet_zkbin, place_bet_pk) = load_circuit!(place_bet);
        let (settle_pot_zkbin, settle_pot_pk) = load_circuit!(settle_pot);
        let (claim_zkbin, claim_pk) = load_circuit!(claim);
        let (call_zkbin, call_pk) = load_circuit!(call);
        let (close_pot_zkbin, close_pot_pk) = load_circuit!(close_pot);
        let (contribute_entropy_zkbin, contribute_entropy_pk) = load_circuit!(contribute_entropy);
        let (fold_zkbin, fold_pk) = load_circuit!(fold);
        let (raise_zkbin, raise_pk) = load_circuit!(raise);
        let (withdraw_zkbin, withdraw_pk) = load_circuit!(withdraw);
        let (create_pot_zkbin, create_pot_pk) = load_circuit!(create_pot);

        Self {
            create_room_zkbin, create_room_pk,
            deposit_zkbin, deposit_pk,
            place_bet_zkbin, place_bet_pk,
            settle_pot_zkbin, settle_pot_pk,
            claim_zkbin, claim_pk,
            call_zkbin, call_pk,
            close_pot_zkbin, close_pot_pk,
            contribute_entropy_zkbin, contribute_entropy_pk,
            fold_zkbin, fold_pk,
            raise_zkbin, raise_pk,
            withdraw_zkbin, withdraw_pk,
            create_pot_zkbin, create_pot_pk,
        }
    }

    pub fn create_room(&self, owner_secret: pallas::Base, token_id: pallas::Base, block_height: u64, nonce: pallas::Base) -> dwow_core::Result<CreateRoomGRResult> {
        let owner = PublicKey::from_secret(SecretKey::from_base(owner_secret));
        let input = CreateRoomCallData::new(owner, token_id, block_height, nonce);
        let (proof, public_inputs) = create_room_v1_proof(&self.create_room_zkbin, &self.create_room_pk, &input)?;
        let params = CreateRoomParamsV1 {
            owner,
            token_id,
            min_stake: 1,
            max_stake: 1000,
            entropy_mode: EntropyMode::BlockHash,
            confirmation_depth: 0,
            required_entropy_contributions: 0,
            entropy_contribution_deadline: 0,
            max_players: 4,
            block_height,
            nonce,
            instance_seed: [0u8; 32],
        };
        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());
        Ok(CreateRoomGRResult { call_data, public_inputs, proof })
    }

    pub fn create_pot(&self, room_id: pallas::Base, player_secret: pallas::Base, nonce: pallas::Base) -> dwow_core::Result<CreatePotGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = CreatePotCallData::new(room_id, player, player_secret, nonce);
        let (proof, public_inputs) = create_pot_v1_proof(&self.create_pot_zkbin, &self.create_pot_pk, &input)?;
        let params = CreatePotParamsV1 {
            room_id,
            player,
            nonce,
            player_nullifier: public_inputs.player_nullifier,
        };
        let mut call_data = vec![0x0B];
        call_data.extend_from_slice(&params.encode());
        Ok(CreatePotGRResult { call_data, public_inputs, proof })
    }

    pub fn deposit(&self, room_id: pallas::Base, player_secret: pallas::Base, amount: u64, nonce: pallas::Base) -> dwow_core::Result<DepositGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = DepositCallData::new(room_id, player, amount, nonce);
        let (proof, public_inputs) = deposit_v1_proof(&self.deposit_zkbin, &self.deposit_pk, &input)?;
        let params = DepositParamsV1 { room_id, player, amount, instance_seed: [0u8; 32] };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());
        Ok(DepositGRResult { call_data, public_inputs, proof })
    }

    pub fn withdraw(&self, room_id: pallas::Base, player_secret: pallas::Base, amount: u64) -> dwow_core::Result<WithdrawGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = IdentityCallData::new(room_id, player, player_secret, 8u64);
        let (proof, public_inputs) = create_identity_proof(&self.withdraw_zkbin, &self.withdraw_pk, &input)?;
        let params = WithdrawParamsV1 { room_id, player, amount, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());
        Ok(WithdrawGRResult { call_data, public_inputs, proof })
    }

    pub fn place_bet(&self, room_id: pallas::Base, pot_id: pallas::Base, player_secret: pallas::Base, amount: u64, bet_type: BetType, block_height: u64, nonce: pallas::Base) -> dwow_core::Result<PlaceBetGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = PlaceBetCallData::new(room_id, pot_id, player, amount, block_height, nonce);
        let (proof, public_inputs) = place_bet_v1_proof(&self.place_bet_zkbin, &self.place_bet_pk, &input)?;
        let params = PlaceBetParamsV1 { room_id, pot_id, player, amount, bet_type, nonce, block_height: pallas::Base::from(block_height) };
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());
        Ok(PlaceBetGRResult { call_data, public_inputs, proof })
    }

    pub fn raise(&self, room_id: pallas::Base, player_secret: pallas::Base, amount: u64, nonce: pallas::Base) -> dwow_core::Result<RaiseGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = IdentityCallData::new(room_id, player, player_secret, 9u64);
        let (proof, public_inputs) = create_identity_proof(&self.raise_zkbin, &self.raise_pk, &input)?;
        let params = RaiseParamsV1 { room_id, player, amount, nonce, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());
        Ok(RaiseGRResult { call_data, public_inputs, proof })
    }

    pub fn call(&self, room_id: pallas::Base, player_secret: pallas::Base, nonce: pallas::Base) -> dwow_core::Result<CallGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = IdentityCallData::new(room_id, player, player_secret, 10u64);
        let (proof, public_inputs) = create_identity_proof(&self.call_zkbin, &self.call_pk, &input)?;
        let params = CallParamsV1 { room_id, player, nonce, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());
        Ok(CallGRResult { call_data, public_inputs, proof })
    }

    pub fn fold(&self, room_id: pallas::Base, player_secret: pallas::Base) -> dwow_core::Result<FoldGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = IdentityCallData::new(room_id, player, player_secret, 11u64);
        let (proof, public_inputs) = create_identity_proof(&self.fold_zkbin, &self.fold_pk, &input)?;
        let params = FoldParamsV1 { room_id, player, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode());
        Ok(FoldGRResult { call_data, public_inputs, proof })
    }

    pub fn close_pot(&self, room_id: pallas::Base, pot_id: pallas::Base, player_secret: pallas::Base) -> dwow_core::Result<ClosePotGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = IdentityCallData::new(room_id, player, player_secret, 12u64);
        let (proof, public_inputs) = create_identity_proof(&self.close_pot_zkbin, &self.close_pot_pk, &input)?;
        let params = ClosePotParamsV1 { room_id, pot_id, player, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());
        Ok(ClosePotGRResult { call_data, public_inputs, proof })
    }

    pub fn settle_pot(&self, caller_secret: pallas::Base, room_id: pallas::Base, pot_id: pallas::Base, winners: Vec<(PublicKey, u64)>, pot_total: u64, nonce: pallas::Base) -> dwow_core::Result<SettlePotGRResult> {
        let caller = PublicKey::from_secret(SecretKey::from_base(caller_secret));
        let input = SettlePotCallData::new(room_id, pot_id, caller, pot_total, winners.len() as u64, nonce);
        let (proof, public_inputs) = settle_pot_v1_proof(&self.settle_pot_zkbin, &self.settle_pot_pk, &input)?;
        let params = SettlePotParamsV1 {
            caller,
            room_id,
            pot_id,
            winners,
            signature: vec![],
            nonce,
            pot_total,
        };
        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&params.encode());
        Ok(SettlePotGRResult { call_data, public_inputs, proof })
    }

    pub fn contribute_entropy(&self, room_id: pallas::Base, player_secret: pallas::Base, commitment: pallas::Base, reveal: Option<pallas::Base>) -> dwow_core::Result<ContributeEntropyGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let input = IdentityCallData::new(room_id, player, player_secret, 13u64);
        let (proof, public_inputs) = create_identity_proof(&self.contribute_entropy_zkbin, &self.contribute_entropy_pk, &input)?;
        let params = ContributeEntropyParamsV1 { room_id, player, commitment, player_nullifier: public_inputs.nullifier, reveal };
        let mut call_data = vec![0x09];
        call_data.extend_from_slice(&params.encode());
        Ok(ContributeEntropyGRResult { call_data, public_inputs, proof })
    }

    pub fn claim(&self, room_id: pallas::Base, pot_id: pallas::Base, winner_secret: pallas::Base, payout_amount: u64, nonce: pallas::Base) -> dwow_core::Result<ClaimGRResult> {
        let winner = PublicKey::from_secret(SecretKey::from_base(winner_secret));
        let input = ClaimCallData::new(room_id, pot_id, winner, payout_amount, nonce);
        let (proof, public_inputs) = claim_v1_proof(&self.claim_zkbin, &self.claim_pk, &input)?;
        let params = ClaimParamsV1 { room_id, pot_id, winner, payout_amount, proof: vec![], nonce };
        let mut call_data = vec![0x0A];
        call_data.extend_from_slice(&params.encode());
        Ok(ClaimGRResult { call_data, public_inputs, proof })
    }
}

impl super::ContractHarness for GameRoomHarness {
    fn name(&self) -> &str { "game_room" }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateRoomV2", "DepositV2", "PlaceBetV2", "SettlePotV2", "ClaimV2",
            "CallV2", "ClosePotV2", "ContributeEntropyV2", "FoldV2", "RaiseV2",
            "WithdrawV2", "CreatePotV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateRoomV2" => Some(&self.create_room_zkbin),
            "DepositV2" => Some(&self.deposit_zkbin),
            "PlaceBetV2" => Some(&self.place_bet_zkbin),
            "SettlePotV2" => Some(&self.settle_pot_zkbin),
            "ClaimV2" => Some(&self.claim_zkbin),
            "CallV2" => Some(&self.call_zkbin),
            "ClosePotV2" => Some(&self.close_pot_zkbin),
            "ContributeEntropyV2" => Some(&self.contribute_entropy_zkbin),
            "FoldV2" => Some(&self.fold_zkbin),
            "RaiseV2" => Some(&self.raise_zkbin),
            "WithdrawV2" => Some(&self.withdraw_zkbin),
            "CreatePotV2" => Some(&self.create_pot_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateRoomV2" => Some(&self.create_room_pk),
            "DepositV2" => Some(&self.deposit_pk),
            "PlaceBetV2" => Some(&self.place_bet_pk),
            "SettlePotV2" => Some(&self.settle_pot_pk),
            "ClaimV2" => Some(&self.claim_pk),
            "CallV2" => Some(&self.call_pk),
            "ClosePotV2" => Some(&self.close_pot_pk),
            "ContributeEntropyV2" => Some(&self.contribute_entropy_pk),
            "FoldV2" => Some(&self.fold_pk),
            "RaiseV2" => Some(&self.raise_pk),
            "WithdrawV2" => Some(&self.withdraw_pk),
            "CreatePotV2" => Some(&self.create_pot_pk),
            _ => None,
        }
    }
}

pub struct CreateRoomGRResult { pub call_data: Vec<u8>, pub public_inputs: CreateRoomPublicInputs, pub proof: Proof }
pub struct CreatePotGRResult { pub call_data: Vec<u8>, pub public_inputs: CreatePotPublicInputs, pub proof: Proof }
pub struct DepositGRResult { pub call_data: Vec<u8>, pub public_inputs: DepositPublicInputs, pub proof: Proof }
pub struct WithdrawGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof }
pub struct PlaceBetGRResult { pub call_data: Vec<u8>, pub public_inputs: PlaceBetPublicInputs, pub proof: Proof }
pub struct RaiseGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof }
pub struct CallGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof }
pub struct FoldGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof }
pub struct ClosePotGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof }
pub struct SettlePotGRResult { pub call_data: Vec<u8>, pub public_inputs: SettlePotPublicInputs, pub proof: Proof }
pub struct ContributeEntropyGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof }
pub struct ClaimGRResult { pub call_data: Vec<u8>, pub public_inputs: ClaimPublicInputs, pub proof: Proof }
