//! Entropy contract entrypoint — provably-fair randomness for betting contracts.
//!
//! ## Design (not yet implemented)
//!
//! 1. Any party calls `commit_entropy` with a start block height
//! 2. Contract records the commitment
//! 3. After 3 blocks are mined, any party calls `reveal_entropy`
//! 4. Contract hashes the 3 block headers → deterministic entropy output
//! 5. No party can predict or manipulate the result
//!
//! See doc/src/contract/entropy.md for the full design specification.

use dwow_sdk::{
    crypto::ContractId,
    error::ContractResult,
    msg, wasm,
};

use crate::{
    error::EntropyError,
    ENTROPY_CONTRACT_INFO_TREE, ENTROPY_CONTRACT_NULLIFIERS_TREE,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[entropy::init_contract] Entropy contract (designed, not yet implemented)");
    wasm::db::db_init(cid, ENTROPY_CONTRACT_INFO_TREE)?;
    wasm::db::db_init(cid, ENTROPY_CONTRACT_NULLIFIERS_TREE)?;
    Ok(())
}

fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    wasm::util::set_return_data(&vec![])
}

fn process_instruction(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Err(EntropyError::NotImplemented.into())
}

fn process_update(_cid: ContractId, _update_data: &[u8]) -> ContractResult {
    Err(EntropyError::NotImplemented.into())
}
