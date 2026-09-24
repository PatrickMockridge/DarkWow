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

//! OBL-C104's next wall — does `UpdateUsageV2`'s proof verify against its own circuit, with the
//! public inputs the client actually produces?
//!
//! The rebuilt subscription fixture gets past `SubscribeV1` and `VerifyAccessV1` and then loses the
//! block at the L2 proof verify with `invalid proof: call[0] namespace 'UpdateUsageV2'`. The host
//! verifies against the *metadata's* instance vector, so there are two homes: the proof and the
//! circuit disagree (a client, witness or zkbin problem), or the proof is sound and the instances the
//! contract publishes are not the ones it was made with. Reading cannot tell them apart; this test
//! can, verifying against the **same zkbin the contract embeds** with `to_vec()`'s instances and no
//! host involved.
//!
//! It also settles the one hypothesis the fixture suggested: `update_usage.zk` contains **no Merkle
//! proof** and no membership constraint, so the dummy `merkle_proof` and the `subscription_state_root`
//! the spec passes cannot be what fails this circuit — the fixture never told that circuit anything
//! about them.

#[cfg(feature = "client")]
mod client_side {
    use dwow_core::zk::{verify_zkp, Proof, ProvingKey, ZkCircuit, ZkVerifyResult};
    use dwow_core::zkas::ZkBinary;
    use dwow_sdk::pasta::pallas;
    use dwow_subscription_contract::client::update_usage::{
        create_update_usage_proof, UpdateUsageCallData,
    };

    const ZKBIN_BYTES: &[u8] = include_bytes!("../proof/update_usage.zk.bin");

    fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
        let circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(zkbin).expect("witnesses"), zkbin);
        ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
    }

    /// The counts, measured rather than assumed: eight declared witnesses, three instances.
    #[test]
    fn the_client_produces_one_witness_and_one_instance_per_declaration() {
        let zkbin = ZkBinary::decode(ZKBIN_BYTES, false).expect("update_usage.zk.bin decodes");
        let input = UpdateUsageCallData::new(
            pallas::Base::from(1u64),
            pallas::Base::from(2u64),
            pallas::Base::from(3u64),
            pallas::Base::from(100u64),
            pallas::Base::from(7u64),
        );
        let public_inputs = input.compute_public_inputs();
        assert_eq!(input.to_witnesses().len(), 8, "one per declared witness");
        assert_eq!(public_inputs.to_vec().len(), 3, "one per constrain_instance");
    }

    /// The client's own proof must verify against the client's own public inputs.
    #[test]
    fn update_usage_proof_verifies_against_its_own_circuit() {
        let zkbin = ZkBinary::decode(ZKBIN_BYTES, false).expect("update_usage.zk.bin decodes");
        let pk = proving_key(&zkbin);
        let input = UpdateUsageCallData::new(
            pallas::Base::from(1u64),
            pallas::Base::from(2u64),
            pallas::Base::from(3u64),
            pallas::Base::from(100u64),
            pallas::Base::from(7u64),
        );

        let (proof, public_inputs) =
            create_update_usage_proof(&zkbin, &pk, &input).expect("the client must build a proof");
        let inputs = public_inputs.to_vec();

        match verify_zkp(&proof, ZKBIN_BYTES, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!(
                "OBL-C104: the client's update_usage proof does not verify against its own circuit \
                 with its own public inputs ({other:?}) — the defect is below the metadata."
            ),
        }
    }
}
