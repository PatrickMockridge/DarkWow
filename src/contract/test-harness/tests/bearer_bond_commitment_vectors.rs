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

//! The bearer_bond note-commitment vectors, end to end (OBL-Z15).
//!
//! `bearer_bond`'s `BlindOutput_V2` and `Redeem_V2` could not be proven at all before 2026-09-20:
//! the circuit's first public input is the note commitment `coin`, and
//!
//!   * `CommitmentAttributes::to_commitment` hashed in a `maturity_block` the circuit does not —
//!     so the client's commitment was never the circuit's `coin`;
//!   * neither the note commitment nor the *receipt* commitment was carried in the params, and the
//!     host cannot recompute either (their preimages hold blinds the holder drew);
//!   * and the tx pair was a literal zero on one side and `poseidon_hash([3, 0, 0])` on the other.
//!
//! These tests pin the *values*, which is what the metadata gate cannot see — it compares counts,
//! and every one of these faults left the counts equal or nearly so. Each case proves **and
//! verifies**: an unsatisfied circuit still produces proof bytes, so an assertion on
//! `Proof::create` alone would report success for exactly the faults it is meant to catch.

use dwow_bearer_bond_contract::model::CommitmentAttributes;
use dwow_core::zk::{
    empty_witnesses, halo2::Value, verify_zkp, Proof, ProvingKey, Witness, ZkCircuit, ZkVerifyResult,
};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::crypto::{
    pasta_prelude::{Curve, CurveAffine, PrimeField},
    pedersen_commitment_u64, poseidon_hash, BaseBlind, ScalarBlind,
};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;

/// `DOMAIN_TX_BINDING` in the circuits, and the all-zero tx pair they are proven against.
fn tx_binding() -> pallas::Base {
    poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()])
}

fn zkbin(path: &[u8]) -> ZkBinary {
    ZkBinary::decode(path, false).expect("circuit decodes")
}

fn proving_key(z: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(z).expect("witnesses"), z);
    ProvingKey::build(z.k, &circuit).expect("ProvingKey::build")
}

fn attempt(z: &ZkBinary, zbytes: &[u8], pk: &ProvingKey, w: Vec<Witness>, inputs: &[pallas::Base]) -> Result<(), String> {
    let circuit = ZkCircuit::new(w, z);
    let proof =
        Proof::create(pk, &[circuit], inputs, OsRng).map_err(|e| format!("synthesis: {e:?}"))?;
    match verify_zkp(&proof, zbytes, inputs) {
        ZkVerifyResult::Ok => Ok(()),
        other => Err(format!("verification: {other:?}")),
    }
}

/// A note's commitment, computed exactly as the *client* computes it: `CommitmentAttributes` with
/// the seven fields the circuits hash, and no maturity.
fn note_commitment(staker: pallas::Base, principal: u64, asset_id: pallas::Base, spend_hook: pallas::Base, user_data: pallas::Base, blind: pallas::Base, maturity_block: u64) -> pallas::Base {
    CommitmentAttributes {
        public_key: staker,
        value: principal,
        asset_id,
        spend_hook,
        user_data,
        blind,
        maturity_block,
    }
    .to_commitment()
}

const BLIND_OUTPUT_BYTES: &[u8] = include_bytes!("../../bearer_bond/proof/blind_output.zk.bin");
const REDEEM_BYTES: &[u8] = include_bytes!("../../bearer_bond/proof/redeem.zk.bin");

#[test]
fn blind_output_verifies_against_the_metadata_vector() {
    // The vector an honest `issue_stake_metadata` / `transfer_stake_metadata` push, in the
    // circuit's order: [coin, vc_x, vc_y, token_commit, spend_hook, tx_binding, tx_nonce].
    let z = zkbin(BLIND_OUTPUT_BYTES);
    let pk = proving_key(&z);

    let (staker, principal, asset_id) = (
        poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(11u64)]),
        1_000u64,
        pallas::Base::from(42u64),
    );
    let (spend_hook, user_data, blind) = (
        pallas::Base::from(9u64),
        pallas::Base::from(13u64),
        pallas::Base::from(17u64),
    );
    let asset_id_blind = BaseBlind::from_u64(23).inner();

    let commitment = note_commitment(staker, principal, asset_id, spend_hook, user_data, blind, 500);
    let token_commit = poseidon_hash([pallas::Base::from(2u64), asset_id, asset_id_blind]);

    // The value commitment, as the client builds it (`pedersen_commitment_u64`).
    let value_commit = pedersen_commitment_u64(principal, ScalarBlind::from_u64(21));
    let affine = value_commit.to_affine();
    let coords = affine.coordinates().expect("non-identity");
    let (vc_x, vc_y) = (*coords.x(), *coords.y());

    let inputs = [
        commitment,
        vc_x,
        vc_y,
        token_commit,
        spend_hook,
        tx_binding(),
        pallas::Base::zero(),
    ];
    let witnesses = vec![
        Witness::Base(Value::known(staker)),
        Witness::Base(Value::known(pallas::Base::from(principal))),
        Witness::Base(Value::known(asset_id)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(blind)),
        Witness::Scalar(Value::known(ScalarBlind::from_u64(21).inner())),
        Witness::Base(Value::known(asset_id_blind)),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(tx_binding())),
    ];
    attempt(&z, BLIND_OUTPUT_BYTES, &pk, witnesses, &inputs)
        .expect("the honest blind_output vector must prove and verify");
}

#[test]
fn blind_output_rejects_token_commit_in_place_of_the_note_commitment() {
    // The pre-fix metadata pushed `token_commit` where the circuit exposes the note commitment —
    // a different hash, over a different domain. Same length, different value: exactly the fault a
    // count-based check cannot see.
    let z = zkbin(BLIND_OUTPUT_BYTES);
    let pk = proving_key(&z);

    let (asset_id_blind, asset_id) = (pallas::Base::from(23u64), pallas::Base::from(42u64));
    let token_commit = poseidon_hash([pallas::Base::from(2u64), asset_id, asset_id_blind]);
    let commitment = note_commitment(
        pallas::Base::from(5u64), 1_000u64, asset_id,
        pallas::Base::from(9u64), pallas::Base::from(13u64), pallas::Base::from(17u64), 0,
    );
    assert_ne!(token_commit, commitment);

    let value_commit = pedersen_commitment_u64(1_000u64, ScalarBlind::from_u64(21));
    let affine = value_commit.to_affine();
    let coords = affine.coordinates().expect("non-identity");
    let (vc_x, vc_y) = (*coords.x(), *coords.y());

    let inputs = [
        token_commit, // where the commitment belongs
        vc_x,
        vc_y,
        token_commit,
        pallas::Base::from(9u64),
        tx_binding(),
        pallas::Base::zero(),
    ];
    let witnesses = vec![
        Witness::Base(Value::known(pallas::Base::from(5u64))),
        Witness::Base(Value::known(pallas::Base::from(1_000u64))),
        Witness::Base(Value::known(asset_id)),
        Witness::Base(Value::known(pallas::Base::from(9u64))),
        Witness::Base(Value::known(pallas::Base::from(13u64))),
        Witness::Base(Value::known(pallas::Base::from(17u64))),
        Witness::Scalar(Value::known(ScalarBlind::from_u64(21).inner())),
        Witness::Base(Value::known(asset_id_blind)),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(tx_binding())),
    ];
    assert!(
        attempt(&z, BLIND_OUTPUT_BYTES, &pk, witnesses, &inputs).is_err(),
        "`token_commit` in the commitment's position must not verify",
    );
}

#[test]
fn redeem_verifies_against_the_receipt_vector() {
    // Redeem_V2's order is [coin, vc_x, vc_y, token_commit, value, tx_binding, tx_nonce,
    // spend_hook] — the tx pair *before* the hook. The client's vector used to put the hook first
    // and the metadata disagreed with both.
    let z = zkbin(REDEEM_BYTES);
    let pk = proving_key(&z);

    let (recipient, value, asset_id, spend_hook, user_data, blind) = (
        poseidon_hash([pallas::Base::from(7u64), pallas::Base::from(31u64)]),
        0u64, // a receipt is a zero-value note
        pallas::Base::from(42u64),
        pallas::Base::from(9u64),
        pallas::Base::from(13u64),
        pallas::Base::from(17u64),
    );
    let asset_id_blind = BaseBlind::from_u64(23).inner();

    let commitment = note_commitment(recipient, value, asset_id, spend_hook, user_data, blind, 0);
    let token_commit = poseidon_hash([pallas::Base::from(2u64), asset_id, asset_id_blind]);
    let value_commit = pedersen_commitment_u64(value, ScalarBlind::from_u64(21));
    let affine = value_commit.to_affine();
    let coords = affine.coordinates().expect("non-identity");
    let (vc_x, vc_y) = (*coords.x(), *coords.y());

    let inputs = [
        commitment,
        vc_x,
        vc_y,
        token_commit,
        pallas::Base::from(value),
        tx_binding(),
        pallas::Base::zero(),
        spend_hook,
    ];
    let witnesses = vec![
        Witness::Base(Value::known(recipient)),
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Base(Value::known(asset_id)),
        Witness::Base(Value::known(spend_hook)),
        Witness::Base(Value::known(user_data)),
        Witness::Base(Value::known(blind)),
        Witness::Scalar(Value::known(ScalarBlind::from_u64(21).inner())),
        Witness::Base(Value::known(asset_id_blind)),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(pallas::Base::zero())),
        Witness::Base(Value::known(tx_binding())),
    ];
    attempt(&z, REDEEM_BYTES, &pk, witnesses, &inputs)
        .expect("the honest redeem vector must prove and verify");
}
