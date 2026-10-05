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
    /// The contract's deployed id (`OBL-C198`): the transaction commitment is derived over the
    /// call set, and a call carries this id, so the harness has to know it.
    contract_id: dwow_sdk::crypto::ContractId,
}

/// The transaction commitment over an ordered call set — the same derivation the node recomputes
/// (`dwow_sdk::crypto::util::tx_commitment`). The order is the one `DarkForest::build_vec` emits:
/// DFS post-order, children before parents.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
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
    pub fn spawn(contract_id: dwow_sdk::crypto::ContractId) -> Self {
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
            contract_id,
        }
    }

    /// The commitment over `children` followed by this call — the ordered set the node hashes.
    ///
    /// This is the *root* form: correct for a call that ends its transaction, which every endpoint
    /// here is. A call used as a **child** of another cannot use it, because its parent's bytes are
    /// part of its commitment and come after it in post-order; such a builder needs a plan the
    /// caller proves against its own commitment.
    fn commitment_over(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        call_data: &[u8],
    ) -> pallas::Base {
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.to_vec() });
        commitment_of(&calls)
    }

    pub fn create_room(&self, children: &[dwow_sdk::tx::ContractCall], owner_secret: pallas::Base, asset_id: pallas::Base, block_height: u64, nonce: pallas::Base) -> dwow_core::Result<CreateRoomGRResult> {
        let owner = PublicKey::from_secret(SecretKey::from_base(owner_secret));
        let mut input = CreateRoomCallData::new(owner, asset_id, block_height, nonce);
        let params = CreateRoomParamsV1 {
            owner,
            asset_id,
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

        // `OBL-C198`: the call data comes first because the commitment is a derivation over it, and
        // the set is `children` followed by this call — DFS post-order, children before parents.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) = create_room_v1_proof(&self.create_room_zkbin, &self.create_room_pk, &input)?;
        Ok(CreateRoomGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn create_pot(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base, nonce: pallas::Base) -> dwow_core::Result<CreatePotGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = CreatePotCallData::new(room_id, player, player_secret, nonce);

        // `OBL-C198`: the params carry a value the proof derives, so the public inputs are taken
        // first — they are a pure function of the call data — and the call data before the
        // commitment.
        let public_inputs = input.compute_public_inputs();
        let params = CreatePotParamsV1 {
            room_id,
            player,
            nonce,
            player_nullifier: public_inputs.player_nullifier,
        };
        let mut call_data = vec![0x0B];
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_pot_v1_proof(&self.create_pot_zkbin, &self.create_pot_pk, &input)?;
        Ok(CreatePotGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn deposit(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base, amount: u64, nonce: pallas::Base) -> dwow_core::Result<DepositGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = DepositCallData::new(room_id, player, amount, nonce);
        let params = DepositParamsV1 { room_id, player, amount, instance_seed: [0u8; 32] };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `create_room`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) = deposit_v1_proof(&self.deposit_zkbin, &self.deposit_pk, &input)?;
        Ok(DepositGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn withdraw(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base, amount: u64) -> dwow_core::Result<WithdrawGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = IdentityCallData::new(room_id, player, player_secret, 8u64);

        // `OBL-C198`: `params` carries a value the proof derives, so the public inputs come first.
        let public_inputs = input.compute_public_inputs();
        let params = WithdrawParamsV1 { room_id, player, amount, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_identity_proof(&self.withdraw_zkbin, &self.withdraw_pk, &input)?;
        Ok(WithdrawGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn place_bet(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, pot_id: pallas::Base, player_secret: pallas::Base, amount: u64, bet_type: BetType, block_height: u64, nonce: pallas::Base) -> dwow_core::Result<PlaceBetGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = PlaceBetCallData::new(room_id, pot_id, player, amount, block_height, nonce);
        let params = PlaceBetParamsV1 { room_id, pot_id, player, amount, bet_type, nonce, block_height: pallas::Base::from(block_height) };
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `create_room`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) = place_bet_v1_proof(&self.place_bet_zkbin, &self.place_bet_pk, &input)?;
        Ok(PlaceBetGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn raise(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base, amount: u64, nonce: pallas::Base) -> dwow_core::Result<RaiseGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = IdentityCallData::new(room_id, player, player_secret, 9u64);
        let public_inputs = input.compute_public_inputs();
        let params = RaiseParamsV1 { room_id, player, amount, nonce, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `create_room`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_identity_proof(&self.raise_zkbin, &self.raise_pk, &input)?;
        Ok(RaiseGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn call(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base, nonce: pallas::Base) -> dwow_core::Result<CallGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = IdentityCallData::new(room_id, player, player_secret, 10u64);
        let public_inputs = input.compute_public_inputs();
        let params = CallParamsV1 { room_id, player, nonce, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_identity_proof(&self.call_zkbin, &self.call_pk, &input)?;
        Ok(CallGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn fold(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base) -> dwow_core::Result<FoldGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = IdentityCallData::new(room_id, player, player_secret, 11u64);
        let public_inputs = input.compute_public_inputs();
        let params = FoldParamsV1 { room_id, player, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_identity_proof(&self.fold_zkbin, &self.fold_pk, &input)?;
        Ok(FoldGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn close_pot(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, pot_id: pallas::Base, player_secret: pallas::Base) -> dwow_core::Result<ClosePotGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = IdentityCallData::new(room_id, player, player_secret, 12u64);
        let public_inputs = input.compute_public_inputs();
        let params = ClosePotParamsV1 { room_id, pot_id, player, player_nullifier: public_inputs.nullifier };
        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_identity_proof(&self.close_pot_zkbin, &self.close_pot_pk, &input)?;
        Ok(ClosePotGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn settle_pot(&self, children: &[dwow_sdk::tx::ContractCall], caller_secret: pallas::Base, room_id: pallas::Base, pot_id: pallas::Base, winners: Vec<(PublicKey, u64)>, pot_total: u64, nonce: pallas::Base) -> dwow_core::Result<SettlePotGRResult> {
        let caller = PublicKey::from_secret(SecretKey::from_base(caller_secret));
        let mut input = SettlePotCallData::new(room_id, pot_id, caller, pot_total, winners.len() as u64, nonce);
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
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        // `OBL-C198`: see `create_room`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) = settle_pot_v1_proof(&self.settle_pot_zkbin, &self.settle_pot_pk, &input)?;
        Ok(SettlePotGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn contribute_entropy(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, player_secret: pallas::Base, commitment: pallas::Base, reveal: Option<pallas::Base>) -> dwow_core::Result<ContributeEntropyGRResult> {
        let player = PublicKey::from_secret(SecretKey::from_base(player_secret));
        let mut input = IdentityCallData::new(room_id, player, player_secret, 13u64);
        let public_inputs = input.compute_public_inputs();
        let params = ContributeEntropyParamsV1 { room_id, player, commitment, player_nullifier: public_inputs.nullifier, reveal };
        let mut call_data = vec![0x09];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `create_room`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = create_identity_proof(&self.contribute_entropy_zkbin, &self.contribute_entropy_pk, &input)?;
        Ok(ContributeEntropyGRResult { call_data, public_inputs, proof, commitment })
    }

    pub fn claim(&self, children: &[dwow_sdk::tx::ContractCall], room_id: pallas::Base, pot_id: pallas::Base, winner_secret: pallas::Base, payout_amount: u64, nonce: pallas::Base) -> dwow_core::Result<ClaimGRResult> {
        let winner = PublicKey::from_secret(SecretKey::from_base(winner_secret));
        let mut input = ClaimCallData::new(room_id, pot_id, winner, payout_amount, nonce);
        let params = ClaimParamsV1 { room_id, pot_id, winner, payout_amount, proof: vec![], nonce };
        let mut call_data = vec![0x0A];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        // `OBL-C198`: see `create_room`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) = claim_v1_proof(&self.claim_zkbin, &self.claim_pk, &input)?;
        Ok(ClaimGRResult { call_data, public_inputs, proof, commitment })
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

pub struct CreateRoomGRResult { pub call_data: Vec<u8>, pub public_inputs: CreateRoomPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct CreatePotGRResult { pub call_data: Vec<u8>, pub public_inputs: CreatePotPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct DepositGRResult { pub call_data: Vec<u8>, pub public_inputs: DepositPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct WithdrawGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct PlaceBetGRResult { pub call_data: Vec<u8>, pub public_inputs: PlaceBetPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct RaiseGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct CallGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct FoldGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct ClosePotGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct SettlePotGRResult { pub call_data: Vec<u8>, pub public_inputs: SettlePotPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct ContributeEntropyGRResult { pub call_data: Vec<u8>, pub public_inputs: IdentityPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
pub struct ClaimGRResult { pub call_data: Vec<u8>, pub public_inputs: ClaimPublicInputs, pub proof: Proof, pub commitment: pallas::Base }
