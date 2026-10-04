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
    crypto::{
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, Blind, ContractId, PublicKey,
        SecretKey,
    },
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;

use dwow_darktoshi_dice_contract::client::{
    commit_bet::{create_commit_bet_v1_proof, CommitBetV1CallData, CommitBetV1PublicInputs},
    house_close::{create_house_close_proof, HouseCloseCallData, HouseClosePublicInputs},
    reveal_roll::{create_reveal_roll_proof, RevealRollCallData},
    settle_bet::{create_settle_bet_v1_proof, SettleBetV1CallData, SettleBetV1PublicInputs},
};
use dwow_darktoshi_dice_contract::model::{
    CommitBetParamsV1, HouseCloseParamsV1, RevealRollParamsV1, SettleBetParamsV1,
};

/// DarkToshiDice Harness for isolated testing
pub struct DarkToshiDiceHarness {
    /// CommitBet_V1 ZkBinary
    commit_bet_zkbin: ZkBinary,
    /// CommitBet_V1 ProvingKey
    commit_bet_pk: ProvingKey,
    /// HouseClose_V1 ZkBinary
    house_close_zkbin: ZkBinary,
    /// HouseClose_V1 ProvingKey
    house_close_pk: ProvingKey,
    /// RevealRoll_V1 ZkBinary
    reveal_roll_zkbin: ZkBinary,
    /// RevealRoll_V1 ProvingKey
    reveal_roll_pk: ProvingKey,
    /// SettleBet_V1 ZkBinary
    settle_bet_zkbin: ZkBinary,
    /// SettleBet_V1 ProvingKey
    settle_bet_pk: ProvingKey,
    /// The contract's deployed id (`OBL-C198`): the transaction commitment is derived over the
    /// call set, and a call carries this id, so the harness has to know it. Taken as a parameter
    /// rather than defaulted — a wrong id is a wrong commitment, and the proof is refused.
    contract_id: ContractId,
}

/// The transaction commitment over an ordered call set — the same derivation the node recomputes
/// (`dwow_sdk::crypto::util::tx_commitment`), so a proof and the transaction that carries it agree.
///
/// The order is the one `DarkForest::build_vec` emits (`TransactionBuilder::build`): DFS
/// post-order, children before parents. A harness that pushed itself first would compute a real
/// commitment for a transaction that does not exist.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

impl DarkToshiDiceHarness {
    /// Spawn a new DarkToshiDice harness with pre-loaded circuits
    pub fn spawn(contract_id: ContractId) -> Self {
        let commit_bet_bin = include_bytes!("../../../darktoshi_dice/proof/commit_bet.zk.bin");
        let house_close_bin = include_bytes!("../../../darktoshi_dice/proof/house_close.zk.bin");
        let reveal_roll_bin = include_bytes!("../../../darktoshi_dice/proof/reveal_roll.zk.bin");
        let settle_bet_bin = include_bytes!("../../../darktoshi_dice/proof/settle_bet.zk.bin");

        let commit_bet_zkbin = ZkBinary::decode(commit_bet_bin, false).unwrap();
        let house_close_zkbin = ZkBinary::decode(house_close_bin, false).unwrap();
        let reveal_roll_zkbin = ZkBinary::decode(reveal_roll_bin, false).unwrap();
        let settle_bet_zkbin = ZkBinary::decode(settle_bet_bin, false).unwrap();

        let commit_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&commit_bet_zkbin).unwrap(),
            &commit_bet_zkbin,
        );
        let house_close_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&house_close_zkbin).unwrap(),
            &house_close_zkbin,
        );
        let reveal_roll_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&reveal_roll_zkbin).unwrap(),
            &reveal_roll_zkbin,
        );
        let settle_bet_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&settle_bet_zkbin).unwrap(),
            &settle_bet_zkbin,
        );

        let commit_bet_pk = ProvingKey::build(commit_bet_zkbin.k, &commit_bet_circuit).expect("ProvingKey::build failed");
        let house_close_pk = ProvingKey::build(house_close_zkbin.k, &house_close_circuit).expect("ProvingKey::build failed");
        let reveal_roll_pk = ProvingKey::build(reveal_roll_zkbin.k, &reveal_roll_circuit).expect("ProvingKey::build failed");
        let settle_bet_pk = ProvingKey::build(settle_bet_zkbin.k, &settle_bet_circuit).expect("ProvingKey::build failed");

        Self {
            commit_bet_zkbin,
            commit_bet_pk,
            house_close_zkbin,
            house_close_pk,
            reveal_roll_zkbin,
            reveal_roll_pk,
            settle_bet_zkbin,
            settle_bet_pk,
            contract_id,
        }
    }
}

impl super::ContractHarness for DarkToshiDiceHarness {
    fn name(&self) -> &str {
        "darktoshi_dice"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitBet_V2", "HouseClose_V2", "RevealRoll_V2", "SettleBet_V2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitBet_V2" => Some(&self.commit_bet_zkbin),
            "HouseClose_V2" => Some(&self.house_close_zkbin),
            "RevealRoll_V2" => Some(&self.reveal_roll_zkbin),
            "SettleBet_V2" => Some(&self.settle_bet_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitBet_V2" => Some(&self.commit_bet_pk),
            "HouseClose_V2" => Some(&self.house_close_pk),
            "RevealRoll_V2" => Some(&self.reveal_roll_pk),
            "SettleBet_V2" => Some(&self.settle_bet_pk),
            _ => None,
        }
    }
}

impl DarkToshiDiceHarness {
    /// Prepare a `commit_bet` call: everything except the proof.
    ///
    /// **This is the one endpoint here that cannot be single-shot, and the reason is a genuine
    /// cycle.** The spec's child is a promissory_note transfer whose leaf blind is seeded from
    /// `bet_id`; so the child cannot be built until `bet_id` is known; `bet_id` is a function of
    /// *this* call's data; and the commitment — which the proof must bind to — is a function of
    /// the child's data. The order that satisfies all three is: build this call's data
    /// (`prepare`), let the caller build the child around the exposed `public_inputs.bet_id`,
    /// then make the proof over the whole set (`commit_bet_prove`).
    ///
    /// The other three endpoints' children are seeded from a `bet_id` the caller already holds
    /// from this step, so they take their child set directly.
    pub fn commit_bet_prepare(
        &self,
        player_pub: PublicKey,
        bet_value: u64,
        target: u8,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        asset_id: pallas::Base,
        house_edge: u32,
    ) -> Result<CommitBetPlan> {
        // Deterministic value blind for Pedersen commitment (PI-7 replay).
        let value_blind = pallas::Scalar::from(7u64);

        let input = CommitBetV1CallData::new(
            player_pub,
            bet_value,
            target,
            secret_nonce,
            blind,
            asset_id,
            house_edge,
            value_blind,
        );

        // The public inputs are a pure function of the call data — `bet_id` is derived, not
        // proven — so they are available before the proof, which is what lets the caller's child
        // be built and the commitment be taken over the final set.
        let public_inputs = input.compute_public_inputs()?;

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
            asset_id,
            value_commit,
            signature,
            house_edge,
            confirmation_depth: 3,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(CommitBetPlan { input, call_data, public_inputs })
    }

    /// Prove a prepared `commit_bet` against the whole ordered call set (`OBL-C198`).
    pub fn commit_bet_prove(
        &self,
        plan: CommitBetPlan,
        children: &[dwow_sdk::tx::ContractCall],
    ) -> Result<CommitBetResult> {
        let CommitBetPlan { mut input, call_data, public_inputs } = plan;

        // ONE commitment over the whole ordered call set — children first, this call last (DFS
        // post-order) — so this proof and its children's bind to the same value.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_commit_bet_v1_proof(&self.commit_bet_zkbin, &self.commit_bet_pk, &input)?;

        Ok(CommitBetResult { call_data, public_inputs, proof, commitment })
    }

    /// Reveal the roll for a committed bet (ZK proof of secret-nonce knowledge)
    pub fn reveal_roll(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        bet_id: pallas::Base,
        secret_nonce: pallas::Base,
    ) -> Result<RevealRollResult> {
        let secret_nonce_commit = poseidon_hash([pallas::Base::from(7u64), secret_nonce]);
        let mut input = RevealRollCallData {
            bet_id,
            secret_nonce,
            secret_nonce_commit,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        let params = RevealRollParamsV1 { bet_id, secret_nonce };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `commit_bet`. This endpoint carries no child in the spec, so the set is
        // the call alone — which is exactly `build_witness`'s single-call transaction.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_reveal_roll_proof(&self.reveal_roll_zkbin, &self.reveal_roll_pk, &input)?;

        Ok(RevealRollResult { call_data, proof, commitment })
    }

    /// Settle a bet (proves knowledge of secret without revealing it)
    #[expect(clippy::too_many_arguments, reason = "the circuit's witness list, plus the child set OBL-C198 needs")]
    pub fn settle_bet(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        bet_id: pallas::Base,
        player_pub_x: pallas::Base,
        player_pub_y: pallas::Base,
        bet_value: pallas::Base,
        target: pallas::Base,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        asset_id: pallas::Base,
        block_hash: pallas::Base,
    ) -> Result<SettleBetResult> {
        let mut input = SettleBetV1CallData::new(
            player_pub_x,
            player_pub_y,
            bet_value,
            target,
            secret_nonce,
            blind,
            asset_id,
            block_hash,
        );

        // `roll_hash` is a pure function of the call data, so the params — and therefore the call
        // data, and therefore the commitment — can all be built before the proof.
        let public_inputs = input.compute_public_inputs();

        // Build SettleBetParamsV1 for call_data
        let params = SettleBetParamsV1 { bet_id, proof: vec![], roll_hash: public_inputs.roll_hash };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        // `OBL-C198`: see `commit_bet` — one commitment over the ordered set, this call last.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_settle_bet_v1_proof(&self.settle_bet_zkbin, &self.settle_bet_pk, &input)?;

        Ok(SettleBetResult { call_data, public_inputs, proof, commitment })
    }

    /// Close a bet (house close, function code 0x04). Derives house_pub + close_nullifier from
    /// the house secret (matches the house_close.zk circuit + the init_contract house_pubkey).
    pub fn house_close(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        bet_id: pallas::Base,
        house_secret: pallas::Base,
    ) -> Result<HouseCloseResult> {
        let house_pub = PublicKey::from_secret(SecretKey::from_base(house_secret));
        let (house_pub_x, house_pub_y) = house_pub.xy().expect("pk not identity");
        let close_nullifier = poseidon_hash([pallas::Base::from(1u64), bet_id, house_secret]);

        let mut input = HouseCloseCallData {
            bet_id,
            house_secret,
            house_pub_x,
            house_pub_y,
            close_nullifier,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        let params = HouseCloseParamsV1 {
            bet_id,
            house_pub_x,
            house_pub_y,
            close_nullifier,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `commit_bet`.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, public_inputs) =
            create_house_close_proof(&self.house_close_zkbin, &self.house_close_pk, &input)?;

        Ok(HouseCloseResult { call_data, public_inputs, proof, commitment })
    }
}

/// Result of house_close
pub struct HouseCloseResult {
    pub call_data: Vec<u8>,
    pub public_inputs: HouseClosePublicInputs,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of commit_bet
pub struct CommitBetResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CommitBetV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// A `commit_bet` call built but not yet proven — see `commit_bet_prepare` for why this endpoint
/// needs the split. `public_inputs.bet_id` is what the caller's child is seeded from.
pub struct CommitBetPlan {
    input: CommitBetV1CallData,
    call_data: Vec<u8>,
    pub public_inputs: CommitBetV1PublicInputs,
}

/// Result of reveal_roll
pub struct RevealRollResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub commitment: pallas::Base,
}

/// Result of settle_bet
pub struct SettleBetResult {
    pub call_data: Vec<u8>,
    pub public_inputs: SettleBetV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
    pub commitment: pallas::Base,
}