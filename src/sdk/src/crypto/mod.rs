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

/// Blinding factors
pub mod blind;
pub use blind::{BaseBlind, Blind, ScalarBlind};

/// Cryptographic constants
pub mod constants;

/// Diffie-Hellman techniques
pub mod diffie_hellman;

/// Miscellaneous utilities
pub mod util;
pub use util::poseidon_hash;

/// Entropy and randomness for provably fair betting
pub mod entropy;
pub use entropy::{
    draw_single, draw_unique_range, combine_block_hashes, draw_with_depth, mix_entropy,
    tx_hash_to_base,
};

/// Keypairs, secret keys, and public keys
pub mod keypair;
pub use keypair::{Keypair, PublicKey, SecretKey};

/// Contract ID definitions and methods
pub mod contract_id;
pub use contract_id::{ContractId, DEPLOYOOOR_CONTRACT_ID, MONEY_V2_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};

/// Function ID definitions and methods
pub mod func_ref;
pub use func_ref::{FuncId, FuncRef};

/// Generic private intent primitives (commitment/nullifier lifecycle)
pub mod intent;
pub use intent::{IntentCommitment, IntentNullifier, PrivateIntent};

/// Generic intent-set state machine and transition types
pub mod intent_set;
pub use intent_set::{
    IntentConsumeCallV1, IntentConsumeTransitionV1, IntentPostTransitionV1, IntentSetIndexV1,
    IntentSetState,
};

/// Transition payload encoding/decoding helpers
pub mod transition_payload;
pub use transition_payload::{
    decode_intent_set_cancel_v1, decode_intent_set_fill_v1, decode_intent_set_post_v1,
    encode_intent_set_cancel_v1, encode_intent_set_fill_v1, encode_intent_set_post_v1,
    IntentSetFunctionV1,
};

/// Merkle node definitions
pub mod merkle_node;
pub use merkle_node::{MerkleNode, MerkleTree};

/// Note encryption
pub mod note;

/// Pedersen commitment utilities
pub mod pedersen;
pub use pedersen::{pedersen_commitment_base, pedersen_commitment_u64};

/// Schnorr signature traits
pub mod schnorr;

/// MiMC VDF
pub mod mimc_vdf;

/// Elliptic curve VRF (Verifiable Random Function)
pub mod ecvrf;

/// Sparse Merkle Tree implementation
pub mod smt;

/// Convenience module to import all the pasta traits.
/// You still have to import the curves.
pub mod pasta_prelude {
    pub use pasta_curves::{
        arithmetic::{CurveAffine, CurveExt},
        group::{
            ff::{Field, FromUniformBytes, PrimeField},
            prime::PrimeCurveAffine,
            Curve, Group,
        },
    };
}

#[macro_export]
macro_rules! fp_from_bs58 {
    ($ty:ident) => {
        impl FromStr for $ty {
            type Err = ContractError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let bytes = match bs58::decode(s).into_vec() {
                    Ok(v) => v,
                    Err(e) => return Err(ContractError::IoError(e.to_string())),
                };

                if bytes.len() != 32 {
                    return Err(ContractError::IoError(
                        "Length of decoded bytes is not 32".to_string(),
                    ))
                }

                Self::from_bytes(bytes.try_into().unwrap())
            }
        }
    };
}

#[macro_export]
macro_rules! fp_to_bs58 {
    ($ty:ident) => {
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", bs58::encode(self.to_bytes()).into_string())
            }
        }
    };
}

#[macro_export]
macro_rules! ty_from_fp {
    ($ty:ident) => {
        impl From<pallas::Base> for $ty {
            fn from(x: pallas::Base) -> Self {
                Self(x)
            }
        }
    };
}
