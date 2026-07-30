use dwow_sdk::{
    crypto::{pasta_prelude::*, ContractId, MerkleNode, MerkleTree},
    dark_tree::DarkLeaf, error::{ContractError, ContractResult}, msg, wasm,
    pasta::pallas, ContractCall,
};
use dwow_serial::{deserialize, Encodable, WriteExt};

use crate::{
    error::BoxError, model::{PutParams, PutUpdate, TakeParams, TakeUpdate}, BoxFunction,
    BOX_CONTRACT_BOX_MERKLE_TREE, BOX_CONTRACT_BOX_ROOTS_TREE, BOX_CONTRACT_INFO_TREE,
    BOX_CONTRACT_LATEST_BOX_ROOT, BOX_CONTRACT_NULLIFIERS_TREE,
    BOX_CONTRACT_ZKAS_PUT_NS, BOX_CONTRACT_ZKAS_TAKE_NS, EMPTY_BOX_TREE_ROOT,
};

dwow_sdk::define_contract!(init: init_contract, exec: process_instruction, apply: process_update, metadata: get_metadata);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[box::init_contract] Initializing Box contract (L1)");
    wasm::db::zkas_db_set(include_bytes!("../../proof/put.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/take.zk.bin"))?;
    let tx_hash = wasm::util::get_tx_hash()?; let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(33); tx_hash.encode(&mut roots_value_data)?; call_idx.encode(&mut roots_value_data)?;
    if wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE).is_err() { wasm::db::db_init(cid, BOX_CONTRACT_NULLIFIERS_TREE)?; }
    let roots_db = match wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE) { Ok(v) => v, Err(_) => wasm::db::db_init(cid, BOX_CONTRACT_BOX_ROOTS_TREE)? };
if !wasm::db::db_contains_key(roots_db, &EMPTY_BOX_TREE_ROOT)? { wasm::db::db_set(roots_db, &EMPTY_BOX_TREE_ROOT, &roots_value_data)?; }
    let info_db = match wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE) { Ok(v) => v, Err(_) => wasm::db::db_init(cid, BOX_CONTRACT_INFO_TREE)? };
    if !wasm::db::db_contains_key(info_db, BOX_CONTRACT_BOX_MERKLE_TREE)? {
        let mut box_tree = MerkleTree::new(1); box_tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let mut box_tree_data = vec![]; box_tree_data.write_u32(0)?; box_tree.encode(&mut box_tree_data)?;
        wasm::db::db_set(info_db, BOX_CONTRACT_BOX_MERKLE_TREE, &box_tree_data)?;
        wasm::db::db_set(info_db, BOX_CONTRACT_LATEST_BOX_ROOT, &EMPTY_BOX_TREE_ROOT)?;
    }
    Ok(())
}

// ============================================================================
// METADATA — pure echo
// ============================================================================

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = usize::try_from(wasm::util::get_call_index()?).map_err(|e| ContractError::IoError(format!("call_index: {e}")))?;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?; let self_ = &calls[call_idx].data;
    let func = BoxFunction::try_from(self_.data[0])?;
    let metadata = match func {
        BoxFunction::Put => { let p = PutParams::decode(&self_.data[1..]).map_err(|e| { msg!("[box::metadata] put decode: {:?}", e); ContractError::IoError("decode".into()) })?; put_metadata(p)? }
        BoxFunction::Take => { let p = TakeParams::decode(&self_.data[1..]).map_err(|e| { msg!("[box::metadata] take decode: {:?}", e); ContractError::IoError("decode".into()) })?; take_metadata(p)? }
        BoxFunction::Initialize => vec![],
    };
    wasm::util::set_return_data(&metadata)
}

fn put_metadata(p: PutParams) -> Result<Vec<u8>, ContractError> {
    let mut z = vec![]; z.push((BOX_CONTRACT_ZKAS_PUT_NS.to_string(), vec![p.nullifier.inner(), p.expected_root.inner(), p.new_leaf.inner(), p.tx_binding, p.tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}
fn take_metadata(p: TakeParams) -> Result<Vec<u8>, ContractError> {
    let mut z = vec![]; z.push((BOX_CONTRACT_ZKAS_TAKE_NS.to_string(), vec![p.nullifier.inner(), p.expected_root.inner(), p.tx_binding, p.tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}

// ============================================================================
// EXEC — nullifier check only
// ============================================================================

fn func_tag(f: BoxFunction) -> u8 { match f { BoxFunction::Initialize => 0x00, BoxFunction::Put => 0x01, BoxFunction::Take => 0x02 } }

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    if ix.is_empty() { msg!("[box::process_instruction] Error: Empty call data"); return Err(ContractError::IoError("Empty call data".to_string())); }
    let call_idx = usize::try_from(wasm::util::get_call_index()?).map_err(|e| ContractError::IoError(format!("call_index: {e}")))?;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?; let self_ = &calls[call_idx];
    let func = BoxFunction::try_from(self_.data.data[0])?;
    match func {
        BoxFunction::Put => {
            let p = PutParams::decode(&self_.data.data[1..])?; msg!("[box::put] Put");
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { msg!("[box::put] Error: Duplicate nullifier"); return Err(BoxError::DuplicateNullifier.into()); }
            let rdb = wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
            if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[box::put] Error: Merkle root not found in roots DB"); return Err(ContractError::IoError("Merkle root not found in roots DB".into())); }
            let u = PutUpdate { nullifier: p.nullifier, new_leaf: p.new_leaf };
            wasm::util::set_return_data(&[&[func_tag(func)], &u.encode()?[..]].concat())?;
        }
        BoxFunction::Take => {
            let p = TakeParams::decode(&self_.data.data[1..])?; msg!("[box::take] Take");
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { msg!("[box::take] Error: Duplicate nullifier"); return Err(BoxError::DuplicateNullifier.into()); }
            let rdb = wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
            if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[box::take] Error: Merkle root not found in roots DB"); return Err(ContractError::IoError("Merkle root not found in roots DB".into())); }
            let u = TakeUpdate { nullifier: p.nullifier };
            wasm::util::set_return_data(&[&[func_tag(func)], &u.encode()?[..]].concat())?;
        }
        BoxFunction::Initialize => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}

// ============================================================================
// APPLY — write state only
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    if update_data.is_empty() { msg!("[box::process_update] Error: Empty update data"); return Err(ContractError::IoError("Empty update data".to_string())); }
    let func = BoxFunction::try_from(update_data[0])?;
    match func {
        BoxFunction::Put => {
            let u = PutUpdate::decode(&update_data[1..])?; let idb = wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE)?;
            let rdb = wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
            wasm::merkle::merkle_add(idb, rdb, BOX_CONTRACT_LATEST_BOX_ROOT, BOX_CONTRACT_BOX_MERKLE_TREE, &[u.new_leaf])?;
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?; wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
        }
        BoxFunction::Take => {
            let u = TakeUpdate::decode(&update_data[1..])?;
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?; wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
        }
        BoxFunction::Initialize => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}
