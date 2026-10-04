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

//! NativeToken Transfer API (wallet.md §6.4 — the one bespoke write-path citizen)
//!
//! Transfer V1 proves its outputs with the `Mint_V2` circuit and destroys its inputs with `Burn_V2`.
//! The coinbase is **not** on this path: `PoWRewardV1` (0x05) is a plaintext call — no ZK proof and no
//! circuit — and shares only the commitment layout with the transfer mint, not a proof.
//!
//! # Example (construction pattern — `no_run` because proving keys need ZK setup)
//!
//! ```rust,no_run
//! use dwow_native_token_contract::client::transfer::{
//!     TransferCallBuilder, TransferCallInput, TransferCallOutput,
//! };
//! use dwow_native_token_contract::model::{CommitmentAttributes, InputWitness};
//! use dwow_core::zk::{ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses};
//! use dwow_core::zkas::ZkBinary;
//! use dwow_sdk::crypto::{Blind, FuncId, MerkleNode, PublicKey, SecretKey};
//! use dwow_sdk::pasta::pallas;
//! use rand::rngs::OsRng;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let burn_bin = dwow_native_token_contract::client::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V2_BIN;
//! let mint_bin = dwow_native_token_contract::client::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V2_BIN;
//! let burn_zk = ZkBinary::decode(burn_bin, false)?;
//! let mint_zk = ZkBinary::decode(mint_bin, false)?;
//! let burn_pk = { let c = ZkCircuit::new(empty_witnesses(&burn_zk)?, &burn_zk);
//!     ProvingKey::build(burn_zk.k, &c)? };
//! let mint_pk = { let c = ZkCircuit::new(empty_witnesses(&mint_zk)?, &mint_zk);
//!     ProvingKey::build(mint_zk.k, &c)? };
//!
//! let builder = TransferCallBuilder {
//!     inputs: vec![/* (InputWitness, SecretKey, spend_hook) */],
//!     outputs: vec![/* CommitmentAttributes */],
//!     burn_zkbin: burn_zk, burn_pk,
//!     mint_zkbin: mint_zk, mint_pk,
//!     tx_commitment: pallas::Base::zero(),
//!     tx_nonce: pallas::Base::zero(),
//! };
//! // let debris = builder.build(&mut OsRng)?;  // rng = the tx Seed (wallet.md §6.1)
//! // vec![0x03] + dwow_serial::serialize(&debris.params) → ContractCallLeaf
//! Ok(())
//! # }
//! ```

pub mod proof;

// Re-export TransferCallOutput as CommitmentAttributes for compatibility
pub use crate::model::CommitmentAttributes as TransferCallOutput;
pub use crate::model::Input as TransferCallInput;

use crate::model::{CommitmentAttributes, InputWitness, Nullifier, TransferParamsV1};
use crate::client::NativeToken;
use dwow_core::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::crypto::{
    constants::{DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_USER_DATA_ENC},
    note::AeadEncryptedNote,
    pedersen_commitment_u64, poseidon_hash,
    BaseBlind, Blind, FuncId, MerkleNode, PublicKey, ScalarBlind, SecretKey,
};
use dwow_sdk::pasta::pallas;
use pasta_curves::group::ff::PrimeField;
use rand::{CryptoRng, RngCore};

// ---------------------------------------------------------------------------
// TransferCallBuilder — type composition ported from:
//   FeeV3CallBuilder (fee.rs)
//   PN TransferCallBuilder (promissory_note/src/client/transfer_v1.rs:174-295)
// wallet.md §6.4: native_token is the one bespoke write-path citizen.
// ---------------------------------------------------------------------------

/// Debris produced by building a TransferV1 call.
pub struct TransferCallDebris {
    /// The contract call parameters
    pub params: TransferParamsV1,
    /// The ZK proofs (burn proofs first, then mint proofs)
    pub proofs: Vec<Proof>,
    /// The per-input secret keys for per-call signing
    pub signature_secrets: Vec<SecretKey>,
}

/// Struct holding necessary information to build a NativeToken TransferV1 call.
pub struct TransferCallBuilder {
    /// Inputs being spent: per input (witness data, owning secret, spend_hook)
    pub inputs: Vec<(InputWitness, SecretKey, pallas::Base)>,
    /// Outputs being created: CommitmentAttributes (carries recipient public_key)
    pub outputs: Vec<CommitmentAttributes>,
    /// `Burn_V2` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V2` zk circuit
    pub burn_pk: ProvingKey,
    /// `Mint_V2` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V2` zk circuit
    pub mint_pk: ProvingKey,
    /// Transaction commitment (binds proofs to the same call set)
    pub tx_commitment: pallas::Base,
    /// Transaction nonce (unique per transaction)
    pub tx_nonce: pallas::Base,
}

impl TransferCallBuilder {
    /// Build the TransferV1 call debris (ported from FeeV3CallBuilder::build,
    /// fee.rs).
    ///
    /// `rng` is the caller's randomness name (wallet.md §6.1) — the wallet
    /// seeds it from the transaction `Seed`, so identical (inputs, Seed) yield
    /// identical params/notes. All blinds and the AEAD ephemerals derive from it.
    ///
    /// Blind discipline (entrypoint cross-proof value conservation,
    /// entrypoint/mod.rs `transfer_v1`):
    /// - `token_blind` is ZERO for every input and output. The native token's
    ///   `token_commit` is pinned to `poseidon([0, 0])` by the fee/spend
    ///   entrypoints (`AssetId::DRKW` = zero), and TransferV1 groups its
    ///   conservation sums by token_commit equality — all entries must share it.
    /// - Output value blinds are balanced: the LAST output's blind is
    ///   `sum(input blinds) − sum(other output blinds)`, so
    ///   `sum(input value_commits) == sum(output value_commits)` holds under
    ///   Pedersen's additive homomorphism.
    /// - The output commitment blind is `output.blind` (caller-provided) — the SAME
    ///   blind feeds the mint proof, the params commitment, and the encrypted note,
    ///   so the proof's constrained commitment is the commitment the chain stores and the
    ///   commitment the recipient's scan reconstructs (scan.rs `build_native_token_cap_record`).
    /// Build the call and prove it in one step, with the commitment the builder was given.
    ///
    /// Kept for callers that already know theirs; the wallet no longer does, because the
    /// commitment is a derivation over the finished call set, so it uses [`Self::prepare`] and
    /// then [`TransferCallPlan::prove`] once the whole transaction is assembled (`OBL-C198`).
    #[expect(clippy::expect_used, reason = "type-system.md §2.3 — base field < scalar field, conversion guaranteed valid")]
    pub fn build(self, rng: &mut (impl CryptoRng + RngCore)) -> Result<TransferCallDebris> {
        let commitment = self.tx_commitment;
        let nonce = self.tx_nonce;
        self.prepare(rng)?.prove(commitment, nonce)
    }

    /// Assemble the call's data and stop — **before** any proof exists.
    ///
    /// `OBL-C198`: the transaction commitment is a derivation over the call data, so a prover
    /// cannot bind to it until the call is built. This is the first half; the second is
    /// [`TransferCallPlan::prove`], which takes the commitment once the caller has derived it
    /// over the transaction's whole call set — not over this one call.
    ///
    /// Nothing here depends on the commitment. The revealed values a burn and a mint produce
    /// are derived by `derive_transfer_burn`/`derive_transfer_mint`, which are the same
    /// functions the proof builders call (`safety.md` RC5).
    pub fn prepare(self, rng: &mut (impl CryptoRng + RngCore)) -> Result<TransferCallPlan> {
        let mut input_entries: Vec<crate::model::Input> = vec![];
        let mut output_entries: Vec<crate::model::Output> = vec![];
        let mut planned_inputs: Vec<PlannedInput> = vec![];
        let mut planned_outputs: Vec<PlannedOutput> = vec![];

        // Native token convention: token_commit = poseidon(asset_id, 0) with
        // asset_id = 0 (AssetId::DRKW). Shared by ALL inputs and outputs so the
        // entrypoint's per-token conservation sums group correctly.
        let token_blind = BaseBlind::ZERO;

        // --- Per-input: derive, and record what the burn proof will need ---
        // Track the input blind sum for output-side balancing.
        let mut input_blind_sum = pallas::Scalar::zero();
        for (witness, secret, spend_hook) in &self.inputs {
            let value_blind = ScalarBlind::random(rng);
            input_blind_sum += value_blind.clone().inner();
            let user_data_blind = BaseBlind::random(rng);

            // Pre-compute the Pedersen commitments that the proof function
            // requires via the TransferCallInput (model::Input) parameter.
            let value_commit = pedersen_commitment_u64(witness.value, value_blind.clone());
            let token_commit = poseidon_hash([DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, witness.asset_id, token_blind.clone().inner()]);
            let user_data_enc = poseidon_hash([DRK_POSEIDON_DOMAIN_USER_DATA_ENC, witness.user_data, user_data_blind.clone().inner()]);
            let call_input = crate::model::Input {
                value_commit,
                token_commit,
                nullifier: Nullifier::new(secret.clone(), pallas::Base::zero()), // proof fills in
                merkle_root: MerkleNode::from_base(pallas::Base::zero()),      // proof fills in
                user_data_enc,
                spend_hook: FuncId::from_base(*spend_hook),
                signature_public: PublicKey::from_secret(secret.clone()),
            };

            // The derivation the proof will constrain, made once and shared with it.
            let derived = proof::derive_transfer_burn(&call_input, witness, secret);

            input_entries.push(crate::model::Input {
                value_commit: call_input.value_commit,
                token_commit: call_input.token_commit,
                nullifier: derived.nullifier,
                merkle_root: derived.merkle_root,
                user_data_enc: call_input.user_data_enc,
                spend_hook: FuncId::from_base(*spend_hook),
                signature_public: derived.signature_public,
            });

            planned_inputs.push(PlannedInput {
                call_input,
                witness: witness.clone(),
                value_blind,
                token_blind: token_blind.clone(),
                user_data_blind,
                secret: secret.clone(),
            });
        }

        // --- Per-output: mint proof + AEAD note ---
        let mut output_blind_sum = pallas::Scalar::zero();
        let n_outputs = self.outputs.len();
        for (i, output) in self.outputs.iter().enumerate() {
            // Balance the last output's value blind so input and output
            // commitment sums are equal (cross-proof conservation).
            let value_blind = if i + 1 == n_outputs {
                Blind(input_blind_sum - output_blind_sum)
            } else {
                let b = ScalarBlind::random(rng);
                output_blind_sum += b.inner();
                b
            };

            // Full recipient support: a FRESH per-output spend_secret, not the
            // spender's secret. The output commitment's public key is derived from
            // this secret (Mint_V2 C2 `commitment_public == from_secret(spend_secret)`),
            // and the secret is handed to the recipient inside the AEAD note so
            // they can compute the nullifier and spend the commitment later.
            let spend_secret = SecretKey::random(rng);

            // The derivation the mint proof will constrain, made once and shared with it —
            // `OBL-C198`, and it needs no commitment, which is what lets the call be built
            // before the proof.
            let derived = proof::derive_transfer_mint(
                output,
                output.value,             // effective_value == value (no uncle split on transfers)
                0,                        // total_pin — transfers are not split
                spend_secret.clone(),
                value_blind.clone(),
                token_blind.clone(),
                output.spend_hook.inner(),
                output.user_data,
                Blind(output.blind.clone().inner()),
                0,                       // old_cumulative_value (identity for non-coinbase)
                pallas::Scalar::zero(),   // old_cumulative_blind (identity for non-coinbase)
            );

            planned_outputs.push(PlannedOutput {
                output: output.clone(),
                effective_value: output.value,
                spend_secret: spend_secret.clone(),
                value_blind: value_blind.clone(),
                token_blind: token_blind.clone(),
                commitment_blind: Blind(output.blind.clone().inner()),
            });

            // Compose the note — blinds MUST match proof witnesses. The commitment
            // blind is `output.blind`: the recipient's scan reconstructs the
            // commitment from the note, and the chain stores the proof's commitment — one
            // blind, one commitment.
            let note = NativeToken {
                value: output.value,
                asset_id: output.asset_id.inner(),
                spend_hook: output.spend_hook.inner(),
                user_data: output.user_data,
                commitment_blind: output.blind.clone().inner(),
                spend_secret: *spend_secret.inner(),
                value_blind: value_blind.clone().inner(),
                token_blind: token_blind.clone().inner(),
                memo: vec![],
            };
            let encrypted_note =
                AeadEncryptedNote::encrypt(&note, &output.public_key, rng)?;

            output_entries.push(crate::model::Output {
                value_commit: derived.value_commit,
                token_commit: derived.token_commit,
                // The proof's constrained commitment IS the params commitment — computed
                // from `output.blind` by `derive_transfer_mint`.
                commitment: derived.commitment,
                nullifier: Some(
                    Nullifier::from_bytes(derived.nullifier.to_repr()).expect("nf zero"),
                ),
                note: encrypted_note,
            });
        }

        Ok(TransferCallPlan {
            input_entries,
            output_entries,
            planned_inputs,
            planned_outputs,
            tx_nonce: self.tx_nonce,
            burn_zkbin: self.burn_zkbin,
            burn_pk: self.burn_pk,
            mint_zkbin: self.mint_zkbin,
            mint_pk: self.mint_pk,
        })
    }
}

/// A transfer call whose data is assembled and whose proofs are not yet made.
///
/// The second half of the split `OBL-C198` forced. The transaction commitment is a derivation
/// over the call data, so the call has to exist before its proof — and it covers the **whole
/// transaction's** call set, not this call alone. A caller therefore builds every call it will
/// submit (parent and children), derives the commitment once over that ordered set, and only
/// then calls [`TransferCallPlan::prove`] with it.
pub struct TransferCallPlan {
    input_entries: Vec<crate::model::Input>,
    output_entries: Vec<crate::model::Output>,
    planned_inputs: Vec<PlannedInput>,
    planned_outputs: Vec<PlannedOutput>,
    tx_nonce: pallas::Base,
    burn_zkbin: ZkBinary,
    burn_pk: ProvingKey,
    mint_zkbin: ZkBinary,
    mint_pk: ProvingKey,
}

/// What a burn proof needs that the derivation does not produce.
struct PlannedInput {
    call_input: crate::model::Input,
    witness: InputWitness,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    user_data_blind: BaseBlind,
    secret: SecretKey,
}

/// What a mint proof needs that the derivation does not produce.
struct PlannedOutput {
    output: TransferCallOutput,
    effective_value: u64,
    spend_secret: SecretKey,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    commitment_blind: BaseBlind,
}

impl TransferCallPlan {
    /// The contract's call parameters.
    ///
    /// **No `tx_binding`**: the field left this struct in `OBL-C198`. That binding is derived
    /// by `get_metadata` from the commitment the host exposes, because the commitment covers
    /// the call data and a binding inside it would be computed from a value that covers it —
    /// a cycle with no fixed point. `tx_nonce` stays: the prover chooses it and it does not
    /// depend on the commitment.
    pub fn params(&self) -> TransferParamsV1 {
        TransferParamsV1 {
            inputs: self.input_entries.clone(),
            outputs: self.output_entries.clone(),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Prove every call in this plan, binding to `tx_commitment`.
    pub fn prove(
        self,
        tx_commitment: pallas::Base,
        tx_nonce: pallas::Base,
    ) -> Result<TransferCallDebris> {
        let mut proofs: Vec<Proof> = vec![];
        let mut signature_secrets: Vec<SecretKey> = vec![];

        for p in &self.planned_inputs {
            let (burn_proof, _revealed, sig_secret) = proof::create_transfer_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &p.call_input,
                &p.witness,
                p.value_blind.clone(),
                p.token_blind.clone(),
                p.user_data_blind.clone(),
                p.secret.clone(),
                tx_commitment,
                tx_nonce,
            )?;
            proofs.push(burn_proof);
            signature_secrets.push(sig_secret);
        }

        for p in &self.planned_outputs {
            let (mint_proof, _revealed) = proof::create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                &p.output,
                p.effective_value,   // effective_value == value (no uncle split on transfers)
                0,                   // total_pin — transfers are not split
                p.spend_secret.clone(),
                p.value_blind.clone(),
                p.token_blind.clone(),
                p.output.spend_hook.inner(),
                p.output.user_data,
                p.commitment_blind.clone(),
                0,                   // old_cumulative_value (identity for non-coinbase)
                pallas::Scalar::zero(), // old_cumulative_blind (identity for non-coinbase)
                tx_commitment,
                tx_nonce,
            )?;
            proofs.push(mint_proof);
        }

        Ok(TransferCallDebris { params: self.params(), proofs, signature_secrets })
    }
}