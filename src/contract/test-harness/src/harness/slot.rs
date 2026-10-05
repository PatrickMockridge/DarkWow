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
    crypto::{pedersen_commitment_u64, poseidon_hash, Blind, ContractId, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_slot_contract::client::{
    commit_bet::{CommitBetV1CallData, CommitBetV1PublicInputs, create_commit_bet_v1_proof},
    settle_bet::{SettleBetV1CallData, SettleBetV1PublicInputs, create_settle_bet_v1_proof},
    reveal_spin::{RevealSpinCallData, RevealSpinPublicInputs, create_reveal_spin_proof},
};
use dwow_slot_contract::model::{
    CancelSpinParamsV1, CommitSpinParamsV1, RevealSpinParamsV1, SettleSpinParamsV1,
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
    /// The contract's deployed id (`OBL-C198`): the transaction commitment is derived over the
    /// call set, and a call carries this id, so the harness has to know it. Taken as a parameter
    /// rather than defaulted — a wrong id is a wrong commitment, and the proof is refused.
    contract_id: ContractId,
}

/// The transaction commitment over an ordered call set — the same derivation the node recomputes
/// (`dwow_sdk::crypto::util::tx_commitment`), so a proof and the transaction that carries it agree.
///
/// The order is the one `DarkForest::build_vec` emits (`TransactionBuilder::build`): DFS
/// post-order, children before parents.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

impl SlotHarness {
    /// Spawn a new Slot harness with pre-loaded circuits
    pub fn spawn(contract_id: ContractId) -> Self {
        let commit_bet_bin = include_bytes!("../../../slot/proof/commit_bet.zk.bin");
        let settle_bet_bin = include_bytes!("../../../slot/proof/settle_bet.zk.bin");
        let reveal_spin_bin = include_bytes!("../../../slot/proof/reveal_spin.zk.bin");

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
            contract_id,
        }
    }

    /// Initialize the slot machine (non-ZK, function code 0x00)
    pub fn initialize(&self) -> Result<InitializeResult> {
        let call_data = vec![0x00];
        Ok(InitializeResult { call_data })
    }

    /// Prepare a commit-spin call: everything except the proof.
    ///
    /// **This is the one endpoint here that cannot be single-shot, and the reason is a genuine
    /// cycle.** The spec's child is a promissory_note transfer whose leaf blind is seeded from
    /// `spin_id`; so the child cannot be built until `spin_id` is known; `spin_id` is a function of
    /// *this* call's data; and the commitment — which the proof must bind to — is a function of the
    /// child's data. The order that satisfies all three is: build this call's data (`prepare`), let
    /// the caller build the child around the exposed `public_inputs.spin_id`, then make the proof
    /// over the whole set (`commit_spin_prove`).
    ///
    /// `reveal_spin` and `settle_spin` take their child set directly, because their children are
    /// seeded from a `spin_id` the caller already holds.
    #[expect(clippy::too_many_arguments, reason = "the params struct's fields, plus the split OBL-C198 forces")]
    pub fn commit_spin_prepare(
        &self,
        player_pub: PublicKey,
        bet_value: u64,
        paylines_played: u32,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        house_edge: u32,
        confirmation_depth: u8,
        asset_id: pallas::Base,
        value_blind: pallas::Scalar,
    ) -> Result<CommitSpinPlan> {
        let input = CommitBetV1CallData::new(
            player_pub, bet_value, paylines_played, secret_nonce,
            blind, asset_id, value_blind,
        );

        // The public inputs are a pure function of the call data — `spin_id` is derived, not
        // proven — so they are available before the proof, which is what lets the caller's child
        // be built and the commitment be taken over the final set.
        let public_inputs = input.compute_public_inputs()?;

        let value_commit = pedersen_commitment_u64(bet_value, Blind(value_blind));
        let params = CommitSpinParamsV1 {
            player_pub,
            bet_value,
            paylines_played,
            secret_nonce,
            blind,
            house_edge,
            confirmation_depth,
            asset_id,
            value_commit,
            instance_seed: [0u8; 32],
        };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(CommitSpinPlan { input, call_data, public_inputs })
    }

    /// Prove a prepared commit-spin against the whole ordered call set (`OBL-C198`).
    pub fn commit_spin_prove(
        &self,
        plan: CommitSpinPlan,
        children: &[dwow_sdk::tx::ContractCall],
    ) -> Result<CommitSpinResult> {
        let CommitSpinPlan { mut input, call_data, public_inputs } = plan;

        // ONE commitment over the whole ordered call set — children first, this call last (DFS
        // post-order) — so this proof and its children's bind to the same value.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_commit_bet_v1_proof(
            &self.commit_bet_zkbin, &self.commit_bet_pk, &input,
        )?;

        Ok(CommitSpinResult { call_data, proof, public_inputs, commitment })
    }

    /// Reveal a spin with ZK proof (function code 0x02)
    pub fn reveal_spin(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        spin_id: pallas::Base,
        secret_nonce: pallas::Base,
    ) -> Result<RevealSpinResult> {
        let secret_nonce_commit = poseidon_hash([pallas::Base::from(7u64), secret_nonce]);
        let mut input = RevealSpinCallData {
            spin_id,
            secret_nonce,
            secret_nonce_commit,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        let params = RevealSpinParamsV1 { spin_id, secret_nonce };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `commit_spin_prepare` — one commitment over the ordered set, this call
        // last. This endpoint carries no child in the spec, so the set is the call alone.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, public_inputs) = create_reveal_spin_proof(
            &self.reveal_spin_zkbin, &self.reveal_spin_pk, &input,
        )?;

        Ok(RevealSpinResult { call_data, proof, public_inputs, commitment })
    }

    /// Settle a bet with ZK proof (function code 0x03)
    #[expect(clippy::too_many_arguments, reason = "the circuit's witness list, plus the child set OBL-C198 needs")]
    pub fn settle_bet(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        player_pub: PublicKey,
        bet_value: u64,
        paylines: u32,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        asset_id: pallas::Base,
        positions: [u64; 3],
        match_count: u64,
        payout: u64,
    ) -> Result<SettleBetResult> {
        let mut input = SettleBetV1CallData::new(
            player_pub, bet_value, paylines, secret_nonce, blind, asset_id,
            positions, match_count, payout,
        );

        // `spin_id` is a pure function of the call data, so the params — and therefore the call
        // data, and therefore the commitment — can all be built before the proof.
        let public_inputs = input.compute_public_inputs();

        let params = SettleSpinParamsV1 { spin_id: public_inputs.spin_id, payout };
        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `commit_spin_prepare`.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_settle_bet_v1_proof(
            &self.settle_bet_zkbin, &self.settle_bet_pk, &input,
        )?;

        Ok(SettleBetResult { call_data, proof, public_inputs, commitment })
    }

    /// Cancel a spin (non-ZK, function code 0x04)
    pub fn cancel_spin(&self, spin_id: pallas::Base) -> Result<CancelSpinResult> {
        let params = CancelSpinParamsV1 { spin_id };
        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());
        Ok(CancelSpinResult { call_data })
    }
}

impl super::ContractHarness for SlotHarness {
    fn name(&self) -> &str {
        "slot"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitBet_V2", "SettleBet_V2", "RevealSpin_V2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitBet_V2" => Some(&self.commit_bet_zkbin),
            "SettleBet_V2" => Some(&self.settle_bet_zkbin),
            "RevealSpin_V2" => Some(&self.reveal_spin_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitBet_V2" => Some(&self.commit_bet_pk),
            "SettleBet_V2" => Some(&self.settle_bet_pk),
            "RevealSpin_V2" => Some(&self.reveal_spin_pk),
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
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// A commit-spin call built but not yet proven — see `commit_spin_prepare` for why this endpoint
/// needs the split. `public_inputs.spin_id` is what the caller's child is seeded from.
pub struct CommitSpinPlan {
    input: CommitBetV1CallData,
    call_data: Vec<u8>,
    pub public_inputs: CommitBetV1PublicInputs,
}

/// Result of reveal_spin
pub struct RevealSpinResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: RevealSpinPublicInputs,
    pub commitment: pallas::Base,
}

/// Result of settle_bet
pub struct SettleBetResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SettleBetV1PublicInputs,
    pub commitment: pallas::Base,
}

/// Result of cancel_spin
pub struct CancelSpinResult {
    pub call_data: Vec<u8>,
}
