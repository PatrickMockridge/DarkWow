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

//! OTC Swap FundSwapV1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{pedersen_commitment_u64, pasta_prelude::Curve, pasta_prelude::CurveAffine, poseidon_hash, Blind, MerkleNode},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

use crate::error::OtcSwapError;

/// FundSwapV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct FundSwapPublicInputs {
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
    pub swap_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub merkle_root: pallas::Base,
}

impl FundSwapPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.value_commit_x,
            self.value_commit_y,
            self.swap_id,
            // `OBL-C198`: the tx pair is the last two instances (see fund_swap.zk).
            self.merkle_root,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for fund_swap proof generation
#[derive(Debug, Clone)]
pub struct FundSwapCallData {
    pub value: u64,
    pub value_blind: pallas::Scalar,
    pub swap_id: pallas::Base,
    pub merkle_leaf_pos: u32,
    pub merkle_path: Vec<MerkleNode>,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl FundSwapCallData {
    pub fn new(
        value: u64,
        value_blind: pallas::Scalar,
        swap_id: pallas::Base,
        merkle_leaf_pos: u32,
        merkle_path: Vec<MerkleNode>,
    ) -> Self {
        Self { value, value_blind, swap_id, merkle_leaf_pos, merkle_path, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// Compute merkle root from leaf position and path
    pub fn compute_merkle_root(&self) -> pallas::Base {
        let mut current = MerkleNode::new(self.swap_id);
        let position: u64 = self.merkle_leaf_pos.into();
        for (level, sibling) in self.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current.inner()
    }

    pub fn compute_public_inputs(&self) -> std::result::Result<FundSwapPublicInputs, OtcSwapError> {
        let value_commit = pedersen_commitment_u64(self.value, Blind(self.value_blind));
        // `Affine::coordinates()` is `None` exactly for the identity point, and
        // `pedersen_commitment_u64` returns the identity exactly when value and blind are both zero
        // — so this is the only `None`. The former `.expect(...)` therefore panicked on that
        // (reachable) input; reject it as an error instead, stating the real condition.
        let value_coords = value_commit
            .to_affine()
            .coordinates()
            .into_option()
            .ok_or(OtcSwapError::InvalidCommitment)?;

        Ok(FundSwapPublicInputs {
            value_commit_x: *value_coords.x(),
            value_commit_y: *value_coords.y(),
            swap_id: self.swap_id,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
            merkle_root: self.compute_merkle_root(),
        })
    }

    #[expect(clippy::unwrap_used, reason = "merkle path length equals fixed tree depth")]
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.swap_id)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Scalar(Value::known(self.value_blind)),
            Witness::Uint32(Value::known(self.merkle_leaf_pos)),
            Witness::MerklePath(Value::known(self.merkle_path.clone().try_into().unwrap())),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create a FundSwap ZK proof
pub fn fund_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &FundSwapCallData,
) -> Result<(Proof, FundSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs()?;
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R8 control: a `FundSwapCallData` whose `value` and `value_blind` are both zero commits to the
    /// identity, whose affine coordinates are absent. `compute_public_inputs` must return `Err`,
    /// never panic.
    ///
    /// `pedersen_commitment_u64(0, 0)` is `V*0 + R*0`, the identity — so `Affine::coordinates()` is
    /// `None`, the input the former `.expect(...)` panicked on. This is the reachable case the guard
    /// now rejects.
    #[test]
    fn identity_value_commit_is_rejected() {
        let data = FundSwapCallData::new(
            0,                                            // value — zero, with a zero blind, commits to the identity
            pallas::Scalar::from(0u64),                   // value_blind
            pallas::Base::from(7u64),                     // swap_id
            0,                                            // merkle_leaf_pos
            vec![],                                       // merkle_path
        );
        assert!(
            data.compute_public_inputs().is_err(),
            "an identity value commitment must be rejected with an error, never panic"
        );
    }
}
