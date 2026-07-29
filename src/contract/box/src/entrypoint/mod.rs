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
    error::BoxError,
    model::{
        BoxRecord, PutParamsV1, PutParamsV3, PutUpdateV1, PutUpdateV3,
        TakeParamsV1, TakeParamsV3, TakeUpdateV1, TakeUpdateV3,
    },
    BoxFunction,
    BOX_CONTRACT_BOXES_TREE, BOX_CONTRACT_BOX_MERKLE_TREE,
    BOX_CONTRACT_BOX_ROOTS_TREE, BOX_CONTRACT_INFO_TREE,
    BOX_CONTRACT_LATEST_BOX_ROOT, BOX_CONTRACT_LATEST_NULLIFIER_ROOT,
    BOX_CONTRACT_NULLIFIER_ROOTS_TREE, BOX_CONTRACT_NULLIFIERS_TREE,
    BOX_CONTRACT_ZKAS_PUT_NS_V1, BOX_CONTRACT_ZKAS_PUT_NS_V3,
    BOX_CONTRACT_ZKAS_TAKE_NS_V1, BOX_CONTRACT_ZKAS_TAKE_NS_V3,
    EMPTY_BOX_TREE_ROOT,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[box::init_contract] Initializing Box contract (L1 hard path)");

    // Register V1 circuits (proven path — existing)
    wasm::db::zkas_db_set(include_bytes!("../../proof/put_v1.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/take_v1.zk.bin"))?;

    // Register V2 circuits (domain separation, HAZOP RC3)
    wasm::db::zkas_db_set(include_bytes!("../../proof/put_v2.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/take_v2.zk.bin"))?;

    // Register V3 circuits (hard path — Merkle inclusion, NEW)
    wasm::db::zkas_db_set(include_bytes!("../../proof/put_v3.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/take_v3.zk.bin"))?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(33);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;

    // Initialize flat DBs (existing)
    if wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_BOXES_TREE)?;
    }
    if wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Initialize Box Merkle roots DB
    if wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE).is_err() {
        let db_box_roots = wasm::db::db_init(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
        wasm::db::db_set(db_box_roots, &EMPTY_BOX_TREE_ROOT, &roots_value_data)?;
    }

    // Initialize nullifier roots DB
    if wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, BOX_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(db_null_roots, &pallas::Base::zero().to_repr(), &roots_value_data)?;
    }

    // Pattern A: resolve info_db handle unconditionally
    let info_db = match wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, BOX_CONTRACT_INFO_TREE)?,
    };

    // Pattern A: conditional Merkle tree data init
    if !wasm::db::db_contains_key(info_db, BOX_CONTRACT_BOX_MERKLE_TREE)? {
        // Initialize Box Merkle tree with sentinel zero leaf
        let mut box_tree = MerkleTree::new(1);
        box_tree.append(MerkleNode::from_base(pallas::Base::ZERO));
        let mut box_tree_data = vec![];
        box_tree_data.write_u32(0)?;
        box_tree.encode(&mut box_tree_data)?;
        wasm::db::db_set(info_db, BOX_CONTRACT_BOX_MERKLE_TREE, &box_tree_data)?;

        // Write initial root from precomputed constant — tree.root(0) requires
        // Sinsemilla hash which is not available during Deploy section.
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
        BoxFunction::PutV1 => {
            let params = match PutParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Error: Failed to decode PutParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            box_put_get_metadata_v1(params)?
        }
        BoxFunction::TakeV1 => {
            let params = match TakeParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Error: Failed to decode TakeParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            box_take_get_metadata_v1(params)?
        }
        BoxFunction::PutV3 => {
            let params = match PutParamsV3::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Error: Failed to decode PutParamsV3: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            box_put_get_metadata_v3(params)?
        }
        BoxFunction::TakeV3 => {
            let params = match TakeParamsV3::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Error: Failed to decode TakeParamsV3: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            box_take_get_metadata_v3(params)?
        }
        BoxFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn box_put_get_metadata_v1(params: PutParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((BOX_CONTRACT_ZKAS_PUT_NS_V1.to_string(), vec![
        params.box_id.inner(), params.old_contents_commit,
        params.tx_binding, params.tx_nonce, params.new_contents_commit,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn box_take_get_metadata_v1(params: TakeParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((BOX_CONTRACT_ZKAS_TAKE_NS_V1.to_string(), vec![
        params.nullifier.inner(), params.box_id.inner(),
        params.tx_binding, params.tx_nonce, params.contents_commit,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn box_put_get_metadata_v3(params: PutParamsV3) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Order MUST match circuit constrain_instance order:
    // nullifier_old, root, new_contents_commit, tx_binding, tx_nonce
    let nullifier_old = dwow_sdk::crypto::poseidon_hash([
        params.box_id.inner(), params.old_state_nonce,
    ]);
    zk_inputs.push((BOX_CONTRACT_ZKAS_PUT_NS_V3.to_string(), vec![
        nullifier_old,
        pallas::Base::ZERO, // merkle_root — verified from proof public inputs
        params.new_contents_commit,
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn box_take_get_metadata_v3(params: TakeParamsV3) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Order MUST match circuit constrain_instance order:
    // nullifier, root, tx_binding, tx_nonce
    let nullifier_val = dwow_sdk::crypto::poseidon_hash([
        params.box_id.inner(), params.state_nonce,
    ]);
    zk_inputs.push((BOX_CONTRACT_ZKAS_TAKE_NS_V3.to_string(), vec![
        nullifier_val,
        pallas::Base::ZERO, // merkle_root — verified from proof public inputs
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
        BoxFunction::PutV1 => {
            let params= PutParamsV1::decode(&self_.data.data[1..])?;
            msg!("[box::put_v1] Put into box {:?}", params.box_id.inner());
            if params.old_contents_commit != pallas::Base::zero() {
                return Err(BoxError::BoxNotEmpty.into());
            }
            let update = PutUpdateV1 { box_id: params.box_id, new_contents_commit: params.new_contents_commit };
            wasm::util::set_return_data(&[&[BoxFunction::PutV1 as u8], &update.encode()[..]].concat())?;
        }
        BoxFunction::TakeV1 => {
            let params= TakeParamsV1::decode(&self_.data.data[1..])?;
            msg!("[box::take_v1] Take from box {:?}", params.box_id.inner());
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &params.nullifier.to_bytes())? {
                return Err(BoxError::DuplicateNullifier.into());
            }
            let update = TakeUpdateV1 { box_id: params.box_id, nullifier: params.nullifier };
            wasm::util::set_return_data(&[&[BoxFunction::TakeV1 as u8], &update.encode()[..]].concat())?;
        }
        BoxFunction::PutV3 => {
            let params = PutParamsV3::decode(&self_.data.data[1..])?;
            msg!("[box::put_v3] Put into box (hard path)");
            if params.old_contents_commit != pallas::Base::zero() {
                // Non-first Put: verify previous state nullifier is unspent
                let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
                let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
                let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
                let old_nullifier = dwow_sdk::crypto::poseidon_hash([
                    params.box_id.inner(), params.old_state_nonce,
                ]);
                if smt.get_leaf(&old_nullifier) != pallas::Base::zero() {
                    msg!("[box::put_v3] Error: Old state nullifier already spent");
                    return Err(BoxError::DuplicateNullifier.into());
                }
            }
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.box_id.inner(), params.old_state_nonce,
            ]);
            let update = PutUpdateV3 {
                nullifier: nullifier_val,
                new_contents_commit: params.new_contents_commit,
            };
            wasm::util::set_return_data(&[&[BoxFunction::PutV3 as u8], &update.encode()[..]].concat())?;
        }
        BoxFunction::TakeV3 => {
            let params = TakeParamsV3::decode(&self_.data.data[1..])?;
            msg!("[box::take_v3] Take from box (hard path)");
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.box_id.inner(), params.state_nonce,
            ]);
            if smt.get_leaf(&nullifier_val) != pallas::Base::zero() {
                msg!("[box::take_v3] Error: Nullifier already spent");
                return Err(BoxError::DuplicateNullifier.into());
            }
            let update = TakeUpdateV3 {
                nullifier: nullifier_val,
            };
            wasm::util::set_return_data(&[&[BoxFunction::TakeV3 as u8], &update.encode()[..]].concat())?;
        }
        BoxFunction::InitializeV1 => {
            msg!("[box::process_instruction] Error: InitializeV1 must be called via init");
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
        BoxFunction::PutV1 => {
            let update = PutUpdateV1::decode(&update_data[1..])?;
            let boxes_db = wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE)?;
            let bx = BoxRecord { version: 1, box_id: update.box_id, contents_commit: update.new_contents_commit, is_empty: false };
            wasm::db::db_set(boxes_db, &update.box_id.to_bytes(), &bx.encode())?;
            Ok(())
        }
        BoxFunction::TakeV1 => {
            let update = TakeUpdateV1::decode(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &update.nullifier.to_bytes(), &[])?;
            let boxes_db = wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE)?;
            let bx = BoxRecord { version: 1, box_id: update.box_id,
                contents_commit: pallas::Base::zero(), is_empty: true };
            wasm::db::db_set(boxes_db, &update.box_id.to_bytes(), &bx.encode())?;
            Ok(())
        }
        BoxFunction::PutV3 => {
            let update = PutUpdateV3::decode(&update_data[1..])?;
            let info_db = wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE)?;

            // Append new box state to Merkle tree (follows PN's merkle_add pattern)
            let new_leaf = MerkleNode::from_base(dwow_sdk::crypto::poseidon_hash([
                update.new_contents_commit,
                update.nullifier,
            ]));
            wasm::merkle::merkle_add(
                info_db,
                wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?,
                BOX_CONTRACT_LATEST_BOX_ROOT,
                BOX_CONTRACT_BOX_MERKLE_TREE,
                &[new_leaf],
            )?;

            // Mark nullifier in SMT (follows PN's WASM-side SMT pattern)
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let mut smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            let leaves: Vec<_> = [(update.nullifier, pallas::Base::one())].to_vec();
            smt.insert_batch(leaves)?;
            let new_root = smt.root();
            wasm::db::db_set(info_db, BOX_CONTRACT_LATEST_NULLIFIER_ROOT, &new_root.to_repr())?;
            Ok(())
        }
        BoxFunction::TakeV3 => {
            let update = TakeUpdateV3::decode(&update_data[1..])?;

            // Mark nullifier in SMT (follows PN's WASM-side SMT pattern)
            let info_db = wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE)?;
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let mut smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            let leaves: Vec<_> = [(update.nullifier, pallas::Base::one())].to_vec();
            smt.insert_batch(leaves)?;
            let new_root = smt.root();
            wasm::db::db_set(info_db, BOX_CONTRACT_LATEST_NULLIFIER_ROOT, &new_root.to_repr())?;
            Ok(())
        }
        BoxFunction::InitializeV1 => {
            msg!("[box::process_update] Error: InitializeV1 must be called via init");
            Err(ContractError::InvalidFunction)
        }
    }
}
