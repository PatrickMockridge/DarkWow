//! Bridge cross-chain cryptographic verification module.
//!
//! Each external chain requires specific cryptographic verification of its
//! deposit proofs. These are NOT zkVM features — they are pure Rust crypto
//! verification functions compiled to WASM and executed in the bridge contract.
//!
//! Gate: `#[cfg(feature = "bridge-verify")]` — other contracts see zero changes.
//! `src/zk/vm.rs` is NOT modified — all verification lives here.
//!
//! ## Implemented
//! - Ethereum: MPT proof verification (RLP + Keccak256 + trie traversal)
//!
//! ## Stubbed (pending implementation)
//! - Monero: DLEq proof verification (Montgomery + Pallas EC ops)
//! - Zcash: Groth16 spend/output proof verification (BN254 pairings)
//! - Aztec: PLONK note proof verification (same pairing infra)
//! - Litecoin: Merkle proof + Bulletproof range proof verification

use dwow_sdk::error::ContractResult;

use crate::error::BridgeError;
use crate::model::{
    ExternalChainProof, AztecDepositProof, LitecoinDepositProof,
    XmrDepositProof, ZcashDepositProof,
};

#[cfg(feature = "bridge-verify")]
mod ethereum;
#[cfg(feature = "bridge-verify")]
mod litecoin;
#[cfg(feature = "bridge-verify")]
mod monero;
#[cfg(feature = "bridge-verify")]
mod zcash;
#[cfg(feature = "bridge-verify")]
mod aztec;

/// Dispatch to the appropriate chain-specific verifier.
/// Called from `process_deposit_instruction` in entrypoint.rs.
pub fn verify_chain_proof(proof: &ExternalChainProof, eth_merkle_proof: &[u8]) -> ContractResult {
    match proof {
        ExternalChainProof::Ethereum => {
            #[cfg(feature = "bridge-verify")]
            { ethereum::verify_mpt_proof(eth_merkle_proof) }
            #[cfg(not(feature = "bridge-verify"))]
            { Err(BridgeError::InvalidDeposit(
                "Ethereum verification not compiled (enable bridge-verify feature)".into()
            ).into()) }
        }
        ExternalChainProof::Monero(proof) => {
            #[cfg(feature = "bridge-verify")]
            { monero::verify_dleq_proof(proof) }
            #[cfg(not(feature = "bridge-verify"))]
            { Err(BridgeError::InvalidDeposit(
                "Monero DLEq verification not compiled (enable bridge-verify feature)".into()
            ).into()) }
        }
        ExternalChainProof::Zcash(proof) => {
            #[cfg(feature = "bridge-verify")]
            { zcash::verify_groth16_proof(proof) }
            #[cfg(not(feature = "bridge-verify"))]
            { Err(BridgeError::InvalidDeposit(
                "Zcash Groth16 verification not compiled (enable bridge-verify feature)".into()
            ).into()) }
        }
        ExternalChainProof::Aztec(proof) => {
            #[cfg(feature = "bridge-verify")]
            { aztec::verify_plonk_proof(proof) }
            #[cfg(not(feature = "bridge-verify"))]
            { Err(BridgeError::InvalidDeposit(
                "Aztec PLONK verification not compiled (enable bridge-verify feature)".into()
            ).into()) }
        }
        ExternalChainProof::Litecoin(proof) => {
            #[cfg(feature = "bridge-verify")]
            { litecoin::verify_merkle_proof(proof) }
            #[cfg(not(feature = "bridge-verify"))]
            { Err(BridgeError::InvalidDeposit(
                "Litecoin verification not compiled (enable bridge-verify feature)".into()
            ).into()) }
        }
    }
}
