use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
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
    error::BoxError,
    model::{PutParams, PutUpdate, TakeParams, TakeUpdate},
    BoxFunction,
    BOX_CONTRACT_BOXES_TREE, BOX_CONTRACT_BOX_MERKLE_TREE,
    BOX_CONTRACT_BOX_ROOTS_TREE, BOX_CONTRACT_INFO_TREE,
    BOX_CONTRACT_LATEST_BOX_ROOT, BOX_CONTRACT_LATEST_NULLIFIER_ROOT,
    BOX_CONTRACT_NULLIFIER_ROOTS_TREE, BOX_CONTRACT_NULLIFIERS_TREE,
    BOX_CONTRACT_ZKAS_PUT_NS, BOX_CONTRACT_ZKAS_TAKE_NS,
    EMPTY_BOX_TREE_ROOT,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[box::init_contract] Initializing Box contract (L1)");

    wasm::db::zkas_db_set(include_bytes!("../../proof/put.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/take.zk.bin"))?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(33);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;

    if wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_BOXES_TREE)?;
    }
    if wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE).is_err() {
        let db_box_roots = wasm::db::db_init(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
        wasm::db::db_set(db_box_roots, &EMPTY_BOX_TREE_ROOT, &roots_value_data)?;
    }
    if wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, BOX_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(db_null_roots, &pallas::Base::zero().to_repr(), &roots_value_data)?;
    }

    let info_db = match wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, BOX_CONTRACT_INFO_TREE)?,
    };

    if !wasm::db::db_contains_key(info_db, BOX_CONTRACT_BOX_MERKLE_TREE)? {
        let mut box_tree = MerkleTree::new(1);
        box_tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let mut box_tree_data = vec![];
        box_tree_data.write_u32(0)?;
        box_tree.encode(&mut box_tree_data)?;
        wasm::db::db_set(info_db, BOX_CONTRACT_BOX_MERKLE_TREE, &box_tree_data)?;
        wasm::db::db_set(info_db, BOX_CONTRACT_LATEST_BOX_ROOT, &EMPTY_BOX_TREE_ROOT)?;
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
    let func = BoxFunction::try_from(self_.data[0])?;

    let metadata = match func {
        BoxFunction::Put => {
            let params = match PutParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Failed to decode PutParams: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            put_get_metadata(params)?
        }
        BoxFunction::Take => {
            let params = match TakeParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Failed to decode TakeParams: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            take_get_metadata(params)?
        }
        BoxFunction::Initialize => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn put_get_metadata(params: PutParams) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((BOX_CONTRACT_ZKAS_PUT_NS.to_string(), vec![
        params.nullifier,
        params.merkle_root,
        params.new_contents_commit,
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn take_get_metadata(params: TakeParams) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((BOX_CONTRACT_ZKAS_TAKE_NS.to_string(), vec![
        params.nullifier,
        params.merkle_root,
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
    let func = BoxFunction::try_from(self_.data.data[0])?;

    match func {
        BoxFunction::Put => {
            let params = PutParams::decode(&self_.data.data[1..])?;
            msg!("[box::put] Put into box");
            if params.old_contents_commit != pallas::Base::zero() {
                let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
                let old_nullifier = dwow_sdk::crypto::poseidon_hash([
                    params.box_id.inner(), params.old_state_nonce,
                ]);
                if wasm::db::db_contains_key(nullifiers_db, &old_nullifier.to_repr())? {
                    return Err(BoxError::BoxNotEmpty.into());
                }
            }
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.box_id.inner(), params.old_state_nonce,
            ]);
            let update = PutUpdate { nullifier: nullifier_val, new_contents_commit: params.new_contents_commit };
            wasm::util::set_return_data(&[&[BoxFunction::Put as u8], &update.encode()[..]].concat())?;
        }
        BoxFunction::Take => {
            let params = TakeParams::decode(&self_.data.data[1..])?;
            msg!("[box::take] Take from box");
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.box_id.inner(), params.state_nonce,
            ]);
            if wasm::db::db_contains_key(nullifiers_db, &nullifier_val.to_repr())? {
                return Err(BoxError::DuplicateNullifier.into());
            }
            let update = TakeUpdate { nullifier: nullifier_val };
            wasm::util::set_return_data(&[&[BoxFunction::Take as u8], &update.encode()[..]].concat())?;
        }
        BoxFunction::Initialize => {
            msg!("[box::process_instruction] Initialize must be called via init");
            return Err(ContractError::InvalidFunction);
        }
    };

    Ok(())
}

// ============================================================================
// APPLY
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = BoxFunction::try_from(update_data[0])?;
    match func {
        BoxFunction::Put => {
            let update = PutUpdate::decode(&update_data[1..])?;
            let info_db = wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE)?;
            let new_leaf = MerkleNode::from_base(dwow_sdk::crypto::poseidon_hash([
                update.new_contents_commit, update.nullifier,
            ]));
            wasm::merkle::merkle_add(
                info_db,
                wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?,
                BOX_CONTRACT_LATEST_BOX_ROOT,
                BOX_CONTRACT_BOX_MERKLE_TREE,
                &[new_leaf],
            )?;
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &update.nullifier.to_repr(), &[])?;
            Ok(())
        }
        BoxFunction::Take => {
            let update = TakeUpdate::decode(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &update.nullifier.to_repr(), &[])?;
            Ok(())
        }
        BoxFunction::Initialize => {
            msg!("[box::process_update] Initialize must be called via init");
            Err(ContractError::InvalidFunction)
        }
    }
}
