use dwow_sdk::{
    crypto::{pasta_prelude::*, ContractId, MerkleNode, MerkleTree},
    dark_tree::DarkLeaf, error::{ContractError, ContractResult}, msg, wasm,
    pasta::pallas, ContractCall,
};
use dwow_serial::{deserialize, Encodable, WriteExt};

use crate::{
    error::PurseError, model::{BalanceParams, DepositParams, DepositUpdate, WithdrawParams, WithdrawUpdate},
    PurseFunction, PURSE_CONTRACT_INFO_TREE, PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_NULLIFIERS_TREE,
    PURSE_CONTRACT_PURSE_MERKLE_TREE, PURSE_CONTRACT_PURSE_ROOTS_TREE,
    PURSE_CONTRACT_ZKAS_BALANCE_NS, PURSE_CONTRACT_ZKAS_DEPOSIT_NS, PURSE_CONTRACT_ZKAS_WITHDRAW_NS, EMPTY_PURSE_TREE_ROOT,
};

dwow_sdk::define_contract!(init: init_contract, exec: process_instruction, apply: process_update, metadata: get_metadata);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[purse::init_contract] Initializing Purse contract (L1)");
    wasm::db::zkas_db_set(include_bytes!("../../proof/deposit.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/withdraw.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/balance.zk.bin"))?;
    let tx_hash = wasm::util::get_tx_hash()?; let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(33); tx_hash.encode(&mut roots_value_data)?; call_idx.encode(&mut roots_value_data)?;
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE).is_err() { wasm::db::db_init(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?; }
    let roots_db = match wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE) { Ok(v) => v, Err(_) => wasm::db::db_init(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)? };
if !wasm::db::db_contains_key(roots_db, &EMPTY_PURSE_TREE_ROOT)? { wasm::db::db_set(roots_db, &EMPTY_PURSE_TREE_ROOT, &roots_value_data)?; }
    let info_db = match wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE) { Ok(v) => v, Err(_) => wasm::db::db_init(cid, PURSE_CONTRACT_INFO_TREE)? };
    if !wasm::db::db_contains_key(info_db, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
        let mut tree = MerkleTree::new(1); tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let mut data = vec![]; data.write_u32(0)?; tree.encode(&mut data)?;
        wasm::db::db_set(info_db, PURSE_CONTRACT_PURSE_MERKLE_TREE, &data)?;
        wasm::db::db_set(info_db, PURSE_CONTRACT_LATEST_PURSE_ROOT, &EMPTY_PURSE_TREE_ROOT)?;
    }
    Ok(())
}

// ============================================================================
// METADATA — pure echo
// ============================================================================

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = usize::try_from(wasm::util::get_call_index()?).map_err(|e| ContractError::IoError(format!("call_index: {e}")))?;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?; let self_ = &calls[call_idx].data;
    let func = PurseFunction::try_from(self_.data[0])?;
    let metadata = match func {
        PurseFunction::Deposit => { let p = DepositParams::decode(&self_.data[1..]).map_err(|e| { msg!("[purse::metadata] deposit: {:?}", e); ContractError::IoError("decode".into()) })?; deposit_metadata(p)? }
        PurseFunction::Withdraw => { let p = WithdrawParams::decode(&self_.data[1..]).map_err(|e| { msg!("[purse::metadata] withdraw: {:?}", e); ContractError::IoError("decode".into()) })?; withdraw_metadata(p)? }
        PurseFunction::Balance => { let p = BalanceParams::decode(&self_.data[1..]).map_err(|e| { msg!("[purse::metadata] balance: {:?}", e); ContractError::IoError("decode".into()) })?; balance_metadata(p)? }
        PurseFunction::Initialize => vec![],
    };
    wasm::util::set_return_data(&metadata)
}

fn deposit_metadata(p: DepositParams) -> Result<Vec<u8>, ContractError> {
    let mut z = vec![]; z.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS.to_string(), vec![p.nullifier.inner(), p.expected_root, p.old_commit_x, p.old_commit_y, p.new_commit_x, p.new_commit_y, p.new_leaf, p.tx_binding, p.tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}
fn withdraw_metadata(p: WithdrawParams) -> Result<Vec<u8>, ContractError> {
    let mut z = vec![]; z.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS.to_string(), vec![p.nullifier.inner(), p.expected_root, p.old_commit_x, p.old_commit_y, p.new_commit_x, p.new_commit_y, p.new_leaf, p.tx_binding, p.tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}
fn balance_metadata(p: BalanceParams) -> Result<Vec<u8>, ContractError> {
    let mut z = vec![]; z.push((PURSE_CONTRACT_ZKAS_BALANCE_NS.to_string(), vec![p.derived_purse_id, p.expected_root, p.balance_commit_x, p.balance_commit_y, p.token_commit, p.tx_binding, p.tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}

// ============================================================================
// EXEC — nullifier check only
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = usize::try_from(wasm::util::get_call_index()?).map_err(|e| ContractError::IoError(format!("call_index: {e}")))?;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?; let self_ = &calls[call_idx];
    let func = PurseFunction::try_from(self_.data.data[0])?;
    match func {
        PurseFunction::Deposit => {
            let p = DepositParams::decode(&self_.data.data[1..])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { return Err(PurseError::DuplicateNullifier.into()); }
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            if !wasm::db::db_contains_key(rdb, &p.expected_root.to_repr())? { return Err(ContractError::IoError("Merkle root not found in roots DB".into())); }
            let u = DepositUpdate { nullifier: p.nullifier, new_leaf: p.new_leaf };
            wasm::util::set_return_data(&[&[PurseFunction::Deposit as u8], &u.encode()[..]].concat())?;
        }
        PurseFunction::Withdraw => {
            let p = WithdrawParams::decode(&self_.data.data[1..])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { return Err(PurseError::DuplicateNullifier.into()); }
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            if !wasm::db::db_contains_key(rdb, &p.expected_root.to_repr())? { return Err(ContractError::IoError("Merkle root not found in roots DB".into())); }
            let u = WithdrawUpdate { nullifier: p.nullifier, new_leaf: p.new_leaf };
            wasm::util::set_return_data(&[&[PurseFunction::Withdraw as u8], &u.encode()[..]].concat())?;
        }
        PurseFunction::Balance => {
            let p = BalanceParams::decode(&self_.data.data[1..])?;
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            if !wasm::db::db_contains_key(rdb, &p.expected_root.to_repr())? { return Err(ContractError::IoError("Merkle root not found in roots DB".into())); }
            wasm::util::set_return_data(&[PurseFunction::Balance as u8])?;
        }
        PurseFunction::Initialize => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}

// ============================================================================
// APPLY — write state only
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = PurseFunction::try_from(update_data[0])?;
    match func {
        PurseFunction::Deposit => {
            let u = DepositUpdate::decode(&update_data[1..])?; let idb = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            wasm::merkle::merkle_add(idb, rdb, PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_PURSE_MERKLE_TREE, &[MerkleNode::from_base(u.new_leaf)])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?; wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
        }
        PurseFunction::Withdraw => {
            let u = WithdrawUpdate::decode(&update_data[1..])?; let idb = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            wasm::merkle::merkle_add(idb, rdb, PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_PURSE_MERKLE_TREE, &[MerkleNode::from_base(u.new_leaf)])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?; wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
        }
        PurseFunction::Balance => {}
        PurseFunction::Initialize => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}
