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

//! DarkBet Exchange AddLiquidity ZK proof generation (AddLiquidityV2 circuit).
//!
//! witness (9): market_id, provider_pub_x, provider_pub_y, provider_secret, amount, block_height,
//! tx_commitment, tx_nonce, tx_binding.
//! `derived_lp_share_id = poseidon_hash(4, market_id, px, py, amount, block_height)`;
//! `computed_nullifier = poseidon_hash(1, lp_share_id, provider_secret)`.
//! instances (4): derived_lp_share_id, computed_nullifier, tx_binding, tx_nonce.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

#[derive(Debug, Clone)]
pub struct AddLiquidityV1PublicInputs {
    pub derived_lp_share_id: pallas::Base,
    pub computed_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AddLiquidityV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_lp_share_id, self.computed_nullifier, self.tx_binding, self.tx_nonce]
    }
}

#[derive(Debug, Clone)]
pub struct AddLiquidityV1CallData {
    pub market_id: pallas::Base,
    pub provider_secret: pallas::Base,
    pub provider_pub_x: pallas::Base,
    pub provider_pub_y: pallas::Base,
    pub amount: u64,
    pub block_height: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl AddLiquidityV1CallData {
    pub fn new(
        market_id: pallas::Base,
        provider_public: PublicKey,
        provider_secret: pallas::Base,
        amount: u64,
        block_height: u64,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (px, py) = provider_public.xy().expect("pk not identity");
        Self {
            market_id,
            provider_secret,
            provider_pub_x: px,
            provider_pub_y: py,
            amount,
            block_height,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> AddLiquidityV1PublicInputs {
        let derived_lp_share_id = poseidon_hash([
            pallas::Base::from(4u64),
            self.market_id,
            self.provider_pub_x,
            self.provider_pub_y,
            pallas::Base::from(self.amount),
            pallas::Base::from(self.block_height),
        ]);
        let computed_nullifier = poseidon_hash([pallas::Base::from(1u64), derived_lp_share_id, self.provider_secret]);
        AddLiquidityV1PublicInputs {
            derived_lp_share_id,
            computed_nullifier,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.market_id)),
            Witness::Base(Value::known(self.provider_pub_x)),
            Witness::Base(Value::known(self.provider_pub_y)),
            Witness::Base(Value::known(self.provider_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create an AddLiquidity ZK proof
pub fn add_liquidity_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AddLiquidityV1CallData,
) -> Result<(Proof, AddLiquidityV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
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
