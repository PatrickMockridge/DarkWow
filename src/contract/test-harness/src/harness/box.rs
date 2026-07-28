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

//! Box Test Harness
//!
//! Provides isolated testing for the Box contract (put/take circuits).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, Nullifier, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_serial::{Encodable, SerialEncodable};
use rand::rngs::OsRng;

/// Box Harness for isolated testing
pub struct BoxHarness {
    /// PutV1 ZkBinary
    put_zkbin: ZkBinary,
    /// PutV1 ProvingKey
    put_pk: ProvingKey,
    /// TakeV1 ZkBinary
    take_zkbin: ZkBinary,
    /// TakeV1 ProvingKey
    take_pk: ProvingKey,
}

impl BoxHarness {
    /// Spawn a new Box harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let put_bin = include_bytes!("../../../box/proof/put_v1.zk.bin");
        let take_bin = include_bytes!("../../../box/proof/take_v1.zk.bin");

        let put_zkbin = ZkBinary::decode(put_bin, false).unwrap();
        let take_zkbin = ZkBinary::decode(take_bin, false).unwrap();

        // Build proving keys
        let put_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&put_zkbin).unwrap(), &put_zkbin);
        let take_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&take_zkbin).unwrap(), &take_zkbin);

        let put_pk = ProvingKey::build(put_zkbin.k, &put_circuit).expect("ProvingKey::build failed");
        let take_pk = ProvingKey::build(take_zkbin.k, &take_circuit).expect("ProvingKey::build failed");

        Self {
            put_zkbin,
            put_pk,
            take_zkbin,
            take_pk,
        }
    }

    pub fn put(&self) -> Result<BoxPutResult> {
        // Witness layout (PutV1 circuit):
        //   0: box_id
        //   1: old_contents_commit (must be 0)
        //   2: new_contents_commit
        //   3: capability_data
        //   4: owner_secret
        //   5: owner_pub_x
        //   6: owner_pub_y
        //   7: tx_commitment
        //   8: tx_nonce
        //   9: tx_binding
        //
        // Public inputs (matching get_metadata):
        //   box_id.inner(), old_contents_commit, new_contents_commit, tx_binding, tx_nonce

        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
        let owner_pub_x = owner_pub.x().expect("owner_pub.x()");
        let owner_pub_y = owner_pub.y().expect("owner_pub.y()");
        let box_id = pallas::Base::from(1u64);
        let old_contents_commit = pallas::Base::zero(); // Box must be empty
        let capability_data = pallas::Base::from(100u64);
        let new_contents_commit = poseidon_hash([capability_data]);
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(box_id)),
            Witness::Base(Value::known(old_contents_commit)),
            Witness::Base(Value::known(new_contents_commit)),
            Witness::Base(Value::known(capability_data)),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub_x)),
            Witness::Base(Value::known(owner_pub_y)),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        // Order MUST match circuit constrain_instance order:
        // box_id, old_contents_commit, tx_binding, tx_nonce, new_contents_commit
        let public_inputs = vec![
            box_id,
            old_contents_commit,
            tx_binding,
            tx_nonce,
            new_contents_commit,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.put_zkbin);
        let proof = Proof::create(&self.put_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        // Build call_data: [0x01] + serialize(PutParamsV1)
        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_box_contract::model::PutParamsV1 {
            box_id: dwow_box_contract::model::BoxId(box_id),
            old_contents_commit,
            new_contents_commit,
            owner: owner_pub,
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x01u8]; // PutV1 function selector
        call_data.extend_from_slice(&params.encode());

        Ok(BoxPutResult { call_data, proof })
    }

    pub fn take(&self) -> Result<BoxTakeResult> {
        // Witness layout (TakeV1 circuit):
        //   0: box_id
        //   1: contents_commit (must be > 0)
        //   2: owner_secret
        //   3: owner_pub_x
        //   4: owner_pub_y
        //   5: tx_commitment
        //   6: tx_nonce
        //   7: tx_binding
        //
        // Public inputs (matching get_metadata):
        //   nullifier.inner(), box_id.inner(), tx_binding, tx_nonce, contents_commit

        let owner_secret = pallas::Base::from(42u64);
        let owner_pub = PublicKey::from_secret(SecretKey::from_base(owner_secret));
        let owner_pub_x = owner_pub.x().expect("owner_pub.x()");
        let owner_pub_y = owner_pub.y().expect("owner_pub.y()");
        let box_id = pallas::Base::from(1u64);
        let contents_commit = pallas::Base::from(100u64); // Must be > 0
        let tx_commitment = pallas::Base::from(200u64);
        let tx_nonce = pallas::Base::from(300u64);
        let tx_binding = poseidon_hash([tx_commitment, tx_nonce]);
        let nullifier_val = poseidon_hash([owner_secret, box_id]);

        let witnesses = vec![
            Witness::Base(Value::known(box_id)),
            Witness::Base(Value::known(contents_commit)),
            Witness::Base(Value::known(owner_secret)),
            Witness::Base(Value::known(owner_pub_x)),
            Witness::Base(Value::known(owner_pub_y)),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];

        let public_inputs = vec![
            nullifier_val,
            box_id,
            tx_binding,
            tx_nonce,
            contents_commit,
        ];

        let circuit = ZkCircuit::new(witnesses, &self.take_zkbin);
        let proof = Proof::create(&self.take_pk, &[circuit], &public_inputs, OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!("Proof::create failed: {:?}", e)))?;

        // Build call_data: [0x02] + serialize(TakeParamsV1)
        let proof_bytes: Vec<u8> = dwow_serial::serialize(&proof);
        let params = dwow_box_contract::model::TakeParamsV1 {
            box_id: dwow_box_contract::model::BoxId(box_id),
            contents_commit,
            nullifier: Nullifier::from_bytes(nullifier_val.to_repr()).unwrap(),
            owner: owner_pub,
            proof: proof_bytes,
            tx_binding,
            tx_nonce,
        };
        let mut call_data = vec![0x02u8]; // TakeV1 function selector
        call_data.extend_from_slice(&params.encode());

        Ok(BoxTakeResult { call_data, proof })
    }
}

impl super::ContractHarness for BoxHarness {
    fn name(&self) -> &str {
        "box"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["PutV1", "TakeV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "PutV1" => Some(&self.put_zkbin),
            "TakeV1" => Some(&self.take_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "PutV1" => Some(&self.put_pk),
            "TakeV1" => Some(&self.take_pk),
            _ => None,
        }
    }
}

pub struct BoxPutResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct BoxTakeResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
