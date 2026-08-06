//! FeeV2 client builder — privacy-preserving fee payment.
//!
//! Stub: full implementation requires FeeThreshold_V1 circuit compilation
//! and Fee_V3 circuit modifications. This file defines the type boundaries
//! so the WASM dispatch and FeeExtractor can reference them.
//!
//! Spec: fee-spec.md §5.

use dwow_sdk::crypto::keypair::Keypair;
use dwow_sdk::crypto::{MerkleNode, PublicKey};
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::pallas;

use crate::model::fee_v2::FeeParamsV2;
use crate::model::{BaseBlind, Input, Output, ScalarBlind};

/// Input parameters for building a FeeV2 call.
pub struct FeeV2CallInput {
    pub value: u64,
    pub token_id: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
    pub coin_blind: pallas::Base,
    pub leaf_position: u64,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: Keypair,
    pub ephemeral_signature_secret: Keypair,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output specification for a FeeV2 call.
pub struct FeeV2CallOutput {
    pub recipient: PublicKey,
    pub value: u64,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
    pub coin_blind: pallas::Base,
}

/// Result of building a FeeV2 call.
pub struct FeeV2Result {
    pub call_data: Vec<u8>,
    pub params: FeeParamsV2,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Builder for FeeV2 calls — stub pending FeeThreshold_V1 circuit compilation.
pub struct FeeV2CallBuilder {
    pub input: FeeV2CallInput,
    pub output: FeeV2CallOutput,
    pub fee_amount: u64,
    pub threshold: u64,
}

impl FeeV2CallBuilder {
    /// Build a FeeV2 call (stub — returns NotImplemented).
    pub fn build(&self) -> Result<FeeV2Result, ContractError> {
        // Stub: full implementation requires:
        // 1. Fee_V3 circuit (value conservation with hidden fee)
        // 2. FeeThreshold_V1 circuit (fee >= threshold)
        // 3. Pedersen commitment of fee amount
        // 4. Deterministic AEAD encryption of fee in witness
        Err(ContractError::IoError(
            "FeeV2 not yet implemented — circuit compilation pending".into()
        ))
    }
}
