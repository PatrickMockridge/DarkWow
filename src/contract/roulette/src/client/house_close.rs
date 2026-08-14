/* This file is part of DarkWow
 * Copyright (C) 2020-2026 Dyne.org foundation
 * License: GNU AGPL v3 or later
 */

//! Roulette HouseCloseV1 Client API — ZK proof for house close authorization.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;
use rand::SeedableRng;

pub struct HouseClosePublicInputs {
    pub table_id: pallas::Base,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub close_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl HouseClosePublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.table_id, self.house_pub_x, self.house_pub_y, self.close_nullifier, self.tx_binding, self.tx_nonce]
    }
}

pub struct HouseCloseCallData {
    pub table_id: pallas::Base,
    pub house_secret: pallas::Base,
    pub house_pub_x: pallas::Base,
    pub house_pub_y: pallas::Base,
    pub close_nullifier: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl HouseCloseCallData {
    pub fn new() -> Self {
        Self {
            table_id: pallas::Base::zero(), house_secret: pallas::Base::zero(),
            house_pub_x: pallas::Base::zero(), house_pub_y: pallas::Base::zero(),
            close_nullifier: pallas::Base::zero(), tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
        }
    }
    pub fn compute_public_inputs(&self) -> HouseClosePublicInputs {
        HouseClosePublicInputs {
            table_id: self.table_id, house_pub_x: self.house_pub_x,
            house_pub_y: self.house_pub_y, close_nullifier: self.close_nullifier,
            tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce,
        }
    }
}

pub fn create_house_close_proof(zkbin: &ZkBinary, pk: &ProvingKey, data: &HouseCloseCallData) -> Result<(Proof, HouseClosePublicInputs)> {
    let pi = data.compute_public_inputs();
    let w = vec![Witness::Base(Value::known(data.table_id)), Witness::Base(Value::known(data.house_secret)), Witness::Base(Value::known(data.house_pub_x)), Witness::Base(Value::known(data.house_pub_y)), Witness::Base(Value::known(data.close_nullifier)), Witness::Base(Value::known(data.tx_commitment)), Witness::Base(Value::known(data.tx_nonce)), Witness::Base(Value::known(pallas::Base::zero()))];
    let c = ZkCircuit::new(w, zkbin);
    #[cfg(not(target_arch = "wasm32"))]
    let p = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[c], &pi.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[c], &pi.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let p = Proof::create(pk, &[c], &pi.to_vec(), &mut OsRng)?;
    Ok((p, pi))
}
