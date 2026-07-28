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
//! Transfer V1 uses the same mint proof as PoWReward since both create new coins.
//! Burn proof is used for destroying coins.
//!
//! # Example (construction pattern — `no_run` because proving keys need ZK setup)
//!
//! ```rust,no_run
//! use dwow_native_token_contract::client::transfer_v1::{
//!     TransferCallBuilder, TransferCallInput, TransferCallOutput,
//! };
//! use dwow_native_token_contract::model::{CoinAttributes, InputWitness};
//! use dwow_core::zk::{ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses};
//! use dwow_core::zkas::ZkBinary;
//! use dwow_sdk::crypto::{Blind, FuncId, MerkleNode, PublicKey, SecretKey};
//! use dwow_sdk::pasta::pallas;
//! use rand::rngs::OsRng;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let burn_bin = dwow_native_token_contract::client::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V1_BIN;
//! let mint_bin = dwow_native_token_contract::client::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN;
//! let burn_zk = ZkBinary::decode(burn_bin, false)?;
//! let mint_zk = ZkBinary::decode(mint_bin, false)?;
//! let burn_pk = { let c = ZkCircuit::new(empty_witnesses(&burn_zk)?, &burn_zk);
//!     ProvingKey::build(burn_zk.k, &c)? };
//! let mint_pk = { let c = ZkCircuit::new(empty_witnesses(&mint_zk)?, &mint_zk);
//!     ProvingKey::build(mint_zk.k, &c)? };
//!
//! let builder = TransferCallBuilder {
//!     inputs: vec![/* (InputWitness, SecretKey, spend_hook) */],
//!     outputs: vec![/* CoinAttributes */],
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

// Re-export TransferCallOutput as CoinAttributes for compatibility
pub use crate::model::CoinAttributes as TransferCallOutput;
pub use crate::model::Input as TransferCallInput;

use crate::model::{CoinAttributes, InputWitness, Nullifier, TransferParamsV1};
use crate::client::NativeToken;
use dwow_core::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::crypto::{
    note::AeadEncryptedNote,
    pedersen_commitment_u64, poseidon_hash,
    BaseBlind, Blind, FuncId, MerkleNode, PublicKey, ScalarBlind, SecretKey,
};
use dwow_sdk::pasta::pallas;
use pasta_curves::group::ff::PrimeField;
use rand::{CryptoRng, RngCore};

// ---------------------------------------------------------------------------
// TransferCallBuilder — type composition ported from:
//   FeeCallBuilder  (fee_v1.rs:280-439)
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
    /// Outputs being created: CoinAttributes (carries recipient public_key)
    pub outputs: Vec<CoinAttributes>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
    /// Transaction commitment (binds proofs to the same call set)
    pub tx_commitment: pallas::Base,
    /// Transaction nonce (unique per transaction)
    pub tx_nonce: pallas::Base,
}

impl TransferCallBuilder {
    /// Build the TransferV1 call debris (ported from FeeCallBuilder::build,
    /// fee_v1.rs:303-439).
    ///
    /// `rng` is the caller's randomness name (wallet.md §6.1) — the wallet
    /// seeds it from the transaction `Seed`, so identical (inputs, Seed) yield
    /// identical params/notes. All blinds and the AEAD ephemerals derive from it.
    ///
    /// Blind discipline (entrypoint cross-proof value conservation,
    /// entrypoint/mod.rs `transfer_v1`):
    /// - `token_blind` is ZERO for every input and output. The native token's
    ///   `token_commit` is pinned to `poseidon([0, 0])` by the fee/spend
    ///   entrypoints (`TokenId::DRKW` = zero), and TransferV1 groups its
    ///   conservation sums by token_commit equality — all entries must share it.
    /// - Output value blinds are balanced: the LAST output's blind is
    ///   `sum(input blinds) − sum(other output blinds)`, so
    ///   `sum(input value_commits) == sum(output value_commits)` holds under
    ///   Pedersen's additive homomorphism.
    /// - The output coin blind is `output.blind` (caller-provided) — the SAME
    ///   blind feeds the mint proof, the params coin, and the encrypted note,
    ///   so the proof's constrained coin is the coin the chain stores and the
    ///   coin the recipient's scan reconstructs (scan.rs `build_native_token_cap_record`).
    pub fn build(self, rng: &mut (impl CryptoRng + RngCore)) -> Result<TransferCallDebris> {
        let mut proofs: Vec<Proof> = vec![];
        let mut input_entries: Vec<crate::model::Input> = vec![];
        let mut output_entries: Vec<crate::model::Output> = vec![];
        let mut signature_secrets: Vec<SecretKey> = vec![];

        // Native token convention: token_commit = poseidon(token_id, 0) with
        // token_id = 0 (TokenId::DRKW). Shared by ALL inputs and outputs so the
        // entrypoint's per-token conservation sums group correctly.
        let token_blind = BaseBlind::ZERO;

        // --- Per-input: burn proof ---
        // Track the input blind sum for output-side balancing.
        let mut input_blind_sum = pallas::Scalar::zero();
        for (witness, secret, spend_hook) in &self.inputs {
            let value_blind = ScalarBlind::random(rng);
            input_blind_sum += value_blind.clone().inner();
            let user_data_blind = BaseBlind::random(rng);

            // Pre-compute the Pedersen commitments that the proof function
            // requires via the TransferCallInput (model::Input) parameter.
            let value_commit = pedersen_commitment_u64(witness.value, value_blind.clone());
            let token_commit = poseidon_hash([witness.token_id, token_blind.clone().inner()]);
            let user_data_enc = poseidon_hash([witness.user_data, user_data_blind.clone().inner()]);
            let call_input = crate::model::Input {
                value_commit,
                token_commit,
                nullifier: Nullifier::new(secret.clone(), pallas::Base::zero()), // proof fills in
                merkle_root: MerkleNode::from_base(pallas::Base::zero()),      // proof fills in
                user_data_enc,
                spend_hook: FuncId::from_base(*spend_hook),
                signature_public: PublicKey::from_secret(secret.clone()),
            };

            let (burn_proof, revealed, sig_secret) = proof::create_transfer_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &call_input,
                witness,
                value_blind,
                token_blind,
                user_data_blind,
                secret.clone(),
                self.tx_commitment,
                self.tx_nonce,
            )?;

            proofs.push(burn_proof);
            signature_secrets.push(sig_secret);

            input_entries.push(crate::model::Input {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                nullifier: revealed.nullifier,  // TransferBurnRevealed: Nullifier type
                merkle_root: revealed.merkle_root,
                user_data_enc: revealed.user_data_enc,
                spend_hook: FuncId::from_base(*spend_hook),
                signature_public: revealed.signature_public,
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

            // Use the first input's secret for output nullifier derivation
            // (mirrors FeeCallBuilder at fee_v1.rs:416).
            let coin_secret = self.inputs.first()
                .map(|(_, s, _)| s.clone())
                .unwrap_or(SecretKey::random(rng));

            let (mint_proof, revealed) = proof::create_transfer_mint_proof(
                &self.mint_zkbin,
                &self.mint_pk,
                output,
                coin_secret,
                value_blind,
                token_blind,
                output.spend_hook.inner(),
                output.user_data,
                Blind(output.blind.clone().inner()),
                0,                       // old_cumulative_value (identity for non-coinbase)
                pallas::Scalar::zero(),   // old_cumulative_blind (identity for non-coinbase)
                self.tx_commitment,
                self.tx_nonce,
            )?;

            proofs.push(mint_proof);

            // Compose the note — blinds MUST match proof witnesses. The coin
            // blind is `output.blind`: the recipient's scan reconstructs the
            // coin from the note, and the chain stores the proof's coin — one
            // blind, one coin.
            let note = NativeToken {
                value: output.value,
                token_id: output.token_id.inner(),
                spend_hook: output.spend_hook.inner(),
                user_data: output.user_data,
                coin_blind: output.blind.clone().inner(),
                value_blind: value_blind.clone().inner(),
                token_blind: token_blind.clone().inner(),
                memo: vec![],
            };
            let encrypted_note =
                AeadEncryptedNote::encrypt(&note, &output.public_key, rng)?;

            output_entries.push(crate::model::Output {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                // The proof's constrained coin IS the params coin — computed
                // from `output.blind` inside create_transfer_mint_proof.
                coin: revealed.coin,
                nullifier: Nullifier::from_bytes(revealed.nullifier.to_repr()).expect("nf zero"),
                note: encrypted_note,
            });
        }

        let tx_binding = poseidon_hash([self.tx_commitment, self.tx_nonce]);

        Ok(TransferCallDebris {
            params: TransferParamsV1 {
                inputs: input_entries,
                outputs: output_entries,
                tx_binding,
                tx_nonce: self.tx_nonce,
            },
            proofs,
            signature_secrets,
        })
    }
}