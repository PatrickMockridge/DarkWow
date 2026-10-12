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

//! DAO-Escrow PayPremium ZK proof generation

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

/// PayPremiumV2 circuit public inputs (4 — matching PayPremiumV2 circuit constrain_instance order)
/// Circuit order: [dao_escrow_bulla, membership_note, tx_binding, tx_nonce]
#[derive(Debug, Clone)]
pub struct PayPremiumV1PublicInputs {
    /// The endowment the premium is paid into — published, not derived (`OBL-C78`).
    pub dao_escrow_bulla: pallas::Base,
    /// `poseidon_hash(4, member_pub_x, member_pub_y, value, asset_id, expiry, membership_blind)`
    pub membership_note: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PayPremiumV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.dao_escrow_bulla, self.membership_note, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for PayPremium proof generation
#[derive(Debug, Clone)]
pub struct PayPremiumV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub dao_escrow_bulla: pallas::Base,
    pub current_block: u64,
    pub member_secret: pallas::Base,
    pub value: u64,
    pub asset_id: pallas::Base,
    pub expiry: u64,
    pub membership_blind: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub member_pub_x: pallas::Base,
    pub member_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PayPremiumV1CallData {
    /// The `mpc_secret_*` arguments this constructor used to take are gone with the witness block
    /// that named them: they fed a bulla the circuit constrained `dao_escrow_bulla` to equal, and
    /// the two preimages could not coincide, so no proof for a real endowment could be made at all
    /// (`OBL-C78`).
    pub fn new(
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        current_block: u64,
        member_secret: pallas::Base,
        value: u64,
        asset_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Base,
        value_blind: pallas::Scalar,
    ) -> Self {
        // Derive member public key from secret
        let member_pub = PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_base(member_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (mx, my) = member_pub.xy().expect("pk not identity");
        Self {
            nullifier_k,
            dao_escrow_bulla,
            current_block,
            member_secret,
            value,
            asset_id,
            expiry,
            membership_blind,
            value_blind,
            member_pub_x: mx,
            member_pub_y: my,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// `membership_note = poseidon_hash(DOMAIN_COIN_COMMIT, member_pub_x, member_pub_y, value,
    /// asset_id, expiry, membership_blind)` with `DOMAIN_COIN_COMMIT = 4`, which is the formula
    /// `pay_premium.zk` constrains. The caller must supply this exact value in
    /// `PayPremiumParamsV1::membership_note`, because the arm republishes the params' value and the
    /// circuit constrains the two to agree.
    pub fn compute_membership_note(&self) -> pallas::Base {
        poseidon_hash([
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            self.member_pub_x,
            self.member_pub_y,
            pallas::Base::from(self.value),
            self.asset_id,
            pallas::Base::from(self.expiry),
            self.membership_blind,
        ])
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3. This was
    /// a literal `Base::zero()` on both sides while the circuit constrains the witness to equal
    /// this hash: unsatisfiable, not merely unbound (register OBL-C78).
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn compute_public_inputs(&self) -> PayPremiumV1PublicInputs {
        // Circuit constrain_instance order: [dao_escrow_bulla, membership_note, tx_binding, tx_nonce]
        PayPremiumV1PublicInputs {
            dao_escrow_bulla: self.dao_escrow_bulla,
            membership_note: self.compute_membership_note(),
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Must match `pay_premium.zk`'s `witness` block exactly:
        //   dao_escrow_bulla, member_secret, member_pub_x, member_pub_y, value, asset_id, expiry,
        //   current_block, membership_blind, tx_commitment, tx_nonce, tx_binding
        // `member_pub_x/y` were missing (14 entries against 15) and `value_blind` was supplied as a
        // `Scalar` although the circuit declares no such witness; the mpcs were also
        // mis-ordered relative to `value`/`asset_id`/`expiry`/`current_block`, and they are gone
        // from the circuit entirely now (register OBL-C78).
        vec![
            // nullifier_k is a CONSTANT in circuit - do NOT pass as witness
            // max_membership_blocks/max_expiry are INTERNAL to circuit (witness_base, base_add)
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.member_secret)),
            Witness::Base(Value::known(self.member_pub_x)),
            Witness::Base(Value::known(self.member_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.asset_id)),
            Witness::Base(Value::known(pallas::Base::from(self.expiry))),
            Witness::Base(Value::known(pallas::Base::from(self.current_block))),
            Witness::Base(Value::known(self.membership_blind)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
        ]
    }
}

/// Create a PayPremium ZK proof
pub fn pay_premium_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &PayPremiumV1CallData,
) -> Result<(Proof, PayPremiumV1PublicInputs)> {
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