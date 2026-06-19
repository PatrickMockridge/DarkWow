use dwow_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, serialize};

use crate::{
    error::BoxError,
    model::{BoxRecord, PutParamsV1, PutUpdateV1, TakeParamsV1, TakeUpdateV1},
    BoxFunction,
    BOX_CONTRACT_BOXES_TREE, BOX_CONTRACT_INFO_TREE, BOX_CONTRACT_NULLIFIERS_TREE,
    BOX_CONTRACT_ZKAS_PUT_NS_V1, BOX_CONTRACT_ZKAS_TAKE_NS_V1,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[box::init_contract] Initializing Box contract");
    let put_bin = include_bytes!("../../proof/put_v1.zk.bin");
    let take_bin = include_bytes!("../../proof/take_v1.zk.bin");
    wasm::db::zkas_db_set(&put_bin[..])?;
    wasm::db::zkas_db_set(&take_bin[..])?;

    if wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_BOXES_TREE)?;
    }
    if wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE).is_err() {
        wasm::db::db_init(cid, BOX_CONTRACT_INFO_TREE)?;
    }
    Ok(())
}

fn get_metadata(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let func = BoxFunction::try_from(self_.data[0])?;
    match func {
        BoxFunction::PutV1 => {
            let params: PutParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((BOX_CONTRACT_ZKAS_PUT_NS_V1.to_string(), vec![params.box_id, params.old_contents_commit, params.new_contents_commit]));
            let mut metadata = vec![];
            dwow_serial::Encodable::encode(&zk_inputs, &mut metadata)?;
            let sigs: Vec<pallas::Base> = vec![params.owner_pub_x, params.owner_pub_y];
            sigs.encode(&mut metadata)?;
            Ok(metadata)
        }
        BoxFunction::TakeV1 => {
            let params: TakeParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((BOX_CONTRACT_ZKAS_TAKE_NS_V1.to_string(), vec![params.box_id, params.contents_commit, params.nullifier]));
            let mut metadata = vec![];
            dwow_serial::Encodable::encode(&zk_inputs, &mut metadata)?;
            let sigs: Vec<pallas::Base> = vec![params.owner_pub_x, params.owner_pub_y];
            sigs.encode(&mut metadata)?;
            Ok(metadata)
        }
        _ => Err(ContractError::InvalidFunction),
    }
}

fn process_instruction(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let func = BoxFunction::try_from(self_.data[0])?;
    match func {
        BoxFunction::PutV1 => {
            let params: PutParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[box::put_v1] Put into box {:?}", params.box_id);
            if params.old_contents_commit != pallas::Base::zero() {
                return Err(BoxError::BoxNotEmpty.into());
            }
            let update = PutUpdateV1 { box_id: params.box_id, new_contents_commit: params.new_contents_commit };
            Ok(serialize(&(BoxFunction::PutV1 as u8, update)))
        }
        BoxFunction::TakeV1 => {
            let params: TakeParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[box::take_v1] Take from box {:?}", params.box_id);
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
                return Err(BoxError::DuplicateNullifier.into());
            }
            let update = TakeUpdateV1 { box_id: params.box_id, nullifier: params.nullifier };
            Ok(serialize(&(BoxFunction::TakeV1 as u8, update)))
        }
        _ => Err(ContractError::InvalidFunction),
    }
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = BoxFunction::try_from(update_data[0])?;
    match func {
        BoxFunction::PutV1 => {
            let update: PutUpdateV1 = deserialize(&update_data[1..])?;
            let boxes_db = wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE)?;
            let bx = BoxRecord { version: 1, box_id: update.box_id, contents_commit: update.new_contents_commit, is_empty: false };
            wasm::db::db_set(boxes_db, &serialize(&update.box_id), &serialize(&bx))?;
            Ok(())
        }
        BoxFunction::TakeV1 => {
            let update: TakeUpdateV1 = deserialize(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;
            let boxes_db = wasm::db::db_lookup(cid, BOX_CONTRACT_BOXES_TREE)?;
            if let Some(data) = wasm::db::db_get(boxes_db, &serialize(&update.box_id))? {
                let mut bx: BoxRecord = deserialize(&data)?;
                bx.is_empty = true;
                bx.contents_commit = pallas::Base::zero();
                wasm::db::db_set(boxes_db, &serialize(&update.box_id), &serialize(&bx))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
