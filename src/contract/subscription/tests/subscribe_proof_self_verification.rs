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

//! OBL-C107 — does `SubscribeV2`'s proof verify against its own circuit, with the public inputs the
//! client actually produces?
//!
//! This exists to localise a failure, and the distinction it draws is the point. The rebuilt
//! subscription fixture (`OBL-C104`) gets the subscribe's **exec** to succeed — the plan lookup, the
//! payment child and the id derivation all pass — and then loses the block at the L2 proof verify
//! with `invalid proof: call[1] namespace 'SubscribeV2'`. The host verifies against the *metadata's*
//! instance vector, so the failure has two possible homes: the proof and the circuit disagree (a
//! client, witness or zkbin problem), or the proof is sound and the instances the contract publishes
//! are not the ones it was made with (a metadata, arm or harness problem). Reading cannot tell them
//! apart; this test can, because it verifies the client's proof against the **same zkbin the contract
//! embeds**, with the inputs `to_vec()` produces, and never involves the host.
//!
//! Proving *and* verifying, following the sibling file in `test-harness`: an unsatisfied circuit
//! still produces proof bytes, so an assertion on `Proof::create` alone would report success for
//! exactly the case this is meant to catch.
//!
//! The inputs are the fixture's, not arbitrary ones: the same values `subscription_spec.rs` passes,
//! so a pass here and a failure there is a statement about the host's side of the wire.

#[cfg(feature = "client")]
mod client_side {
    use dwow_core::zk::{verify_zkp, Proof, ProvingKey, ZkCircuit, ZkVerifyResult};
    use dwow_core::zkas::ZkBinary;
    use dwow_sdk::crypto::{
        pasta_prelude::{Curve, CurveAffine, Group, PrimeField},
        pedersen_commitment_u64, util::fp_mod_fv, Blind, MerkleNode, PublicKey, SecretKey,
    };
    use dwow_sdk::pasta::pallas;
    use dwow_subscription_contract::client::subscribe::{
        create_subscribe_proof, SubscribeCallData,
    };
    use dwow_subscription_contract::model::{Subscription, SubscriptionId};

    const ZKBIN_BYTES: &[u8] = include_bytes!("../proof/subscribe.zk.bin");

    /// The fixture's plan and subscriber, matching `subscription_spec.rs`.
    const PLAN_ID: u32 = 1;
    const PLAN_PRICE: u64 = 1000;
    const PLAN_DURATION: u64 = 200;

    fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
        let circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(zkbin).expect("witnesses"),
            zkbin,
        );
        ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
    }

    fn call_data(zkbin: &ZkBinary) -> SubscribeCallData {
        let sub_secret = pallas::Base::from(10u64);
        let sub_pub = PublicKey::from_secret(SecretKey::from_base(sub_secret));
        let asset_id = pallas::Base::from(5u64);
        let nonce = pallas::Base::from(1u64);
        let id = Subscription::derive_id(
            &sub_pub, PLAN_ID, PLAN_PRICE, asset_id, PLAN_DURATION, sub_secret, nonce,
        );
        let blind_seed = pallas::Base::from(7u64);
        let value_blind = Blind(fp_mod_fv(blind_seed).expect("base fits scalar"));
        let vc = pedersen_commitment_u64(PLAN_PRICE, value_blind.clone());
        #[expect(clippy::unwrap_used, reason = "a Pedersen commitment is never the identity point")]
        let (vc_x, vc_y) = {
            let a = vc.to_affine();
            let c = a.coordinates().unwrap();
            (*c.x(), *c.y())
        };
        let dummy = MerkleNode::new(pallas::Base::from(0u64));
        let mut input = SubscribeCallData::new(
            sub_secret,
            nonce,
            vec![dummy.clone()],
            value_blind.inner(),
            pallas::Base::from(2u64),
            pallas::Base::from(3u64),
            1000,
            pallas::Base::from(4u64),
            0,
            vec![dummy.clone()],
            0,
            vec![dummy.clone()],
            id.inner(),
            sub_pub,
            PLAN_ID,
            PLAN_PRICE,
            asset_id,
            PLAN_DURATION,
            pallas::Base::from(6u64),
            100,
            vc_x,
            vc_y,
            pallas::Base::from(9u64),
            pallas::Base::from(10u64),
            pallas::Base::from(11u64),
        );
        assert_eq!(id, SubscriptionId(input.subscription_id), "the id is the derivation");
        // The zkbin is the one the *contract* embeds, so a witness/instance mismatch is not hidden.
        assert_eq!(zkbin.k, ZkBinary::decode(ZKBIN_BYTES, false).expect("decodes").k);
        input.subscription_id = id.inner();
        input
    }

    /// The counts, measured rather than assumed.
    ///
    /// Everything else in this file rests on the client producing one witness per declared witness
    /// and one instance per `constrain_instance`; if either count is off, the proof cannot verify and
    /// the reason is not a value at all.
    #[test]
    fn the_client_produces_one_witness_and_one_instance_per_declaration() {
        use dwow_core::zkas::ZkBinary;
        let zkbin = ZkBinary::decode(ZKBIN_BYTES, false).expect("subscribe.zk.bin decodes");
        let input = call_data(&zkbin);
        let public_inputs = input.compute_public_inputs();

        // `witness "SubscribeV2"` declares fifteen; `constrain_instance` appears three times.
        assert_eq!(
            input.to_witnesses().len(),
            15,
            "the client's witness vector must have one entry per declared witness"
        );
        assert_eq!(
            public_inputs.to_vec().len(),
            3,
            "the client's instance vector must have one entry per constrain_instance"
        );
    }

    /// The fork: the same zkbin, the same instance vector, but the witness vector assembled **here**
    /// in the declared order rather than by the client.
    ///
    /// A pass says the client's vector is the defect — compare it entry for entry with this one. A
    /// failure says the compiled artifact is, and then the question is what `zkas` emits for this
    /// file rather than what any Rust in the tree does.
    #[test]
    fn subscribe_proof_verifies_with_a_hand_built_witness_vector() {
        use dwow_core::zk::{halo2::Value, Witness};

        let zkbin = ZkBinary::decode(ZKBIN_BYTES, false).expect("subscribe.zk.bin decodes");
        let pk = proving_key(&zkbin);
        let input = call_data(&zkbin);
        let public_inputs = input.compute_public_inputs();

        /// `SubscribeV2`'s declaration order, read off the .zk.
        #[expect(clippy::unwrap_used, reason = "PublicKey rejects identity, so x()/y() is always Some")]
        let witnesses = vec![
            Witness::Base(Value::known(input.subscription_id)),
            Witness::Base(Value::known(input.subscriber_public.x().unwrap())),
            Witness::Base(Value::known(input.subscriber_public.y().unwrap())),
            Witness::Base(Value::known(pallas::Base::from(input.plan_id as u64))),
            Witness::Base(Value::known(pallas::Base::from(input.deposit))),
            Witness::Base(Value::known(input.asset_id)),
            Witness::Base(Value::known(pallas::Base::from(input.lock_until_block))),
            Witness::Base(Value::known(input.value_commit_x)),
            Witness::Base(Value::known(input.value_commit_y)),
            Witness::Base(Value::known(input.subscriber_secret)),
            Witness::Base(Value::known(input.nonce)),
            Witness::Scalar(Value::known(input.value_blind)),
            Witness::Base(Value::known(input.tx_commitment)),
            Witness::Base(Value::known(input.tx_nonce)),
            Witness::Base(Value::known(public_inputs.tx_binding)),
        ];
        assert_eq!(witnesses.len(), 15, "one per declared witness");

        let instances = vec![
            public_inputs.tx_binding,
            public_inputs.tx_nonce,
            public_inputs.subscription_id,
        ];

        let proof = Proof::create(
            &pk,
            &[ZkCircuit::new(witnesses, &zkbin)],
            &instances,
            &mut rand::rngs::OsRng,
        )
        .expect("the hand-built vector must produce a proof");

        match verify_zkp(&proof, ZKBIN_BYTES, &instances) {
            ZkVerifyResult::Ok => {}
            other => panic!(
                "OBL-C107: a witness vector assembled in the declared order, with the same zkbin and \
                 the same instances, does not verify either ({other:?}) — so the defect is in what \
                 `zkas` emits for subscribe.zk, not in the client's vector."
            ),
        }
    }

    /// The client's own proof must verify against the client's own public inputs.
    ///
    /// A failure here says the defect is *below* the metadata: the witnesses, the instance vector or
    /// the circuit. A pass says the proof is sound and the host's side of the wire is what disagrees.
    #[test]
    fn subscribe_proof_verifies_against_its_own_circuit() {
        let zkbin = ZkBinary::decode(ZKBIN_BYTES, false).expect("subscribe.zk.bin decodes");
        let pk = proving_key(&zkbin);
        let input = call_data(&zkbin);

        let (proof, public_inputs) = create_subscribe_proof(&zkbin, &pk, &input)
            .expect("the client must build a proof");
        let inputs = public_inputs.to_vec();

        assert_eq!(
            inputs.len(),
            3,
            "subscribe.zk instances tx_binding, tx_nonce and derived_id (OBL-C75)"
        );

        // `verify_zkp` takes the raw transcript bytes; `Proof::create` returns those for the client.
        let proof_bytes = proof.as_ref();
        let _ = proof_bytes;
        match verify_zkp(&proof, ZKBIN_BYTES, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!(
                "OBL-C107: the client's subscribe proof does not verify against its own circuit with \
                 its own public inputs ({other:?}). The defect is in the client, the witnesses or the \
                 zkbin — not in the contract's metadata, which this test never touches."
            ),
        }
    }
}
