use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
        smt::{wasmdb::SmtWasmFp, PoseidonFp, EMPTY_NODES_FP},
        ContractId, MerkleNode, MerkleTree,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, Encodable, WriteExt};

use crate::{
    error::PurseError,
    model::{
        BalanceParams, DepositParams, DepositUpdate,
        Purse, WithdrawParams, WithdrawUpdate,
    },
    PurseFunction,
    PURSE_CONTRACT_INFO_TREE, PURSE_CONTRACT_LATEST_NULLIFIER_ROOT,
    PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_NULLIFIER_ROOTS_TREE,
    PURSE_CONTRACT_NULLIFIERS_TREE, PURSE_CONTRACT_PURSE_MERKLE_TREE,
    PURSE_CONTRACT_PURSE_ROOTS_TREE, PURSE_CONTRACT_PURSES_TREE,
    PURSE_CONTRACT_ZKAS_BALANCE_NS, PURSE_CONTRACT_ZKAS_DEPOSIT_NS,
    PURSE_CONTRACT_ZKAS_WITHDRAW_NS,
    EMPTY_PURSE_TREE_ROOT,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[purse::init_contract] Initializing Purse contract (L1)");

    wasm::db::zkas_db_set(include_bytes!("../../proof/deposit.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/withdraw.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/balance.zk.bin"))?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(33);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;

    if wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_PURSES_TREE)?;
    }
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE).is_err() {
        let db_purse_roots = wasm::db::db_init(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
        wasm::db::db_set(db_purse_roots, &EMPTY_PURSE_TREE_ROOT, &roots_value_data)?;
    }
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, PURSE_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(db_null_roots, &pallas::Base::zero().to_repr(), &roots_value_data)?;
    }

    let info_db = match wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, PURSE_CONTRACT_INFO_TREE)?,
    };

    if !wasm::db::db_contains_key(info_db, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
        let mut purse_tree = MerkleTree::new(1);
        purse_tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let mut purse_tree_data = vec![];
        purse_tree_data.write_u32(0)?;
        purse_tree.encode(&mut purse_tree_data)?;
        wasm::db::db_set(info_db, PURSE_CONTRACT_PURSE_MERKLE_TREE, &purse_tree_data)?;
        wasm::db::db_set(info_db, PURSE_CONTRACT_LATEST_PURSE_ROOT, &EMPTY_PURSE_TREE_ROOT)?;
    }

    Ok(())
}

// ============================================================================
// METADATA
// ============================================================================

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PurseFunction::try_from(self_.data[0])?;

    let metadata = match func {
        PurseFunction::Deposit => {
            let params = match DepositParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Failed to decode DepositParams: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            deposit_get_metadata(params)?
        }
        PurseFunction::Withdraw => {
            let params = match WithdrawParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Failed to decode WithdrawParams: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            withdraw_get_metadata(params)?
        }
        PurseFunction::Balance => {
            let params = match BalanceParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Failed to decode BalanceParams: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            balance_get_metadata(params)?
        }
        PurseFunction::Initialize => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn deposit_get_metadata(params: DepositParams) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let nullifier_old = dwow_sdk::crypto::poseidon_hash([
        params.purse_id.inner(), params.state_nonce,
    ]);
    zk_inputs.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS.to_string(), vec![
        nullifier_old,
        pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
        params.tx_binding, params.tx_nonce,
        pallas::Base::zero(),
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn withdraw_get_metadata(params: WithdrawParams) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let nullifier_val = dwow_sdk::crypto::poseidon_hash([
        params.purse_id.inner(), params.state_nonce,
    ]);
    zk_inputs.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS.to_string(), vec![
        nullifier_val,
        pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
        pallas::Base::zero(), pallas::Base::zero(),
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn balance_get_metadata(params: BalanceParams) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let derived_purse_id = dwow_sdk::crypto::poseidon_hash([
        params.purse_id.inner(), params.token_id,
    ]);
    zk_inputs.push((PURSE_CONTRACT_ZKAS_BALANCE_NS.to_string(), vec![
        derived_purse_id,
        pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
        params.token_id,
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// EXEC
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = PurseFunction::try_from(self_.data.data[0])?;

    match func {
        PurseFunction::Deposit => {
            let params = DepositParams::decode(&self_.data.data[1..])?;
            msg!("[purse::deposit] Deposit");
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.purse_id.inner(), params.state_nonce,
            ]);
            let update = DepositUpdate {
                nullifier: nullifier_val,
                new_balance_commit: pallas::Point::identity(),
            };
            wasm::util::set_return_data(&[&[PurseFunction::Deposit as u8], &update.encode()[..]].concat())?;
        }
        PurseFunction::Withdraw => {
            let params = WithdrawParams::decode(&self_.data.data[1..])?;
            msg!("[purse::withdraw] Withdraw");
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.purse_id.inner(), params.state_nonce,
            ]);
            if smt.get_leaf(&nullifier_val) != pallas::Base::zero() {
                return Err(PurseError::DuplicateNullifier.into());
            }
            let update = WithdrawUpdate { nullifier: nullifier_val };
            wasm::util::set_return_data(&[&[PurseFunction::Withdraw as u8], &update.encode()[..]].concat())?;
        }
        PurseFunction::Balance => {
            let _params = BalanceParams::decode(&self_.data.data[1..])?;
            msg!("[purse::balance] Balance check");
            wasm::util::set_return_data(&[PurseFunction::Balance as u8])?;
        }
        PurseFunction::Initialize => {
            msg!("[purse::process_instruction] Initialize must be called via init");
            return Err(ContractError::InvalidFunction);
        }
    };

    Ok(())
}

// ============================================================================
// APPLY
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = PurseFunction::try_from(update_data[0])?;
    match func {
        PurseFunction::Deposit => {
            let update = DepositUpdate::decode(&update_data[1..])?;
            let info_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let new_leaf = MerkleNode::from_base(update.nullifier);
            wasm::merkle::merkle_add(
                info_db,
                wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?,
                PURSE_CONTRACT_LATEST_PURSE_ROOT,
                PURSE_CONTRACT_PURSE_MERKLE_TREE,
                &[new_leaf],
            )?;
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let mut smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            smt.insert_batch(vec![(update.nullifier, pallas::Base::one())])?;
            let new_root = smt.root();
            wasm::db::db_set(info_db, PURSE_CONTRACT_LATEST_NULLIFIER_ROOT, &new_root.to_repr())?;
            Ok(())
        }
        PurseFunction::Withdraw => {
            let update = WithdrawUpdate::decode(&update_data[1..])?;
            let info_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let mut smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            smt.insert_batch(vec![(update.nullifier, pallas::Base::one())])?;
            let new_root = smt.root();
            wasm::db::db_set(info_db, PURSE_CONTRACT_LATEST_NULLIFIER_ROOT, &new_root.to_repr())?;
            Ok(())
        }
        PurseFunction::Balance => Ok(()),
        PurseFunction::Initialize => {
            msg!("[purse::process_update] Initialize must be called via init");
            Err(ContractError::InvalidFunction)
        }
    }
}
