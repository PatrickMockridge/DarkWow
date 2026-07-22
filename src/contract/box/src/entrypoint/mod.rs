use dwow_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

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

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BoxFunction::try_from(self_.data[0])?;

    let metadata = match func {
        BoxFunction::PutV1 => {
            let params: PutParamsV1 = match deserialize(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Error: Failed to deserialize PutParamsV1: {:?}", e); return Ok(()); }
            };
            box_put_get_metadata_v1(params)?
        }
        BoxFunction::TakeV1 => {
            let params: TakeParamsV1 = match deserialize(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[box::get_metadata] Error: Failed to deserialize TakeParamsV1: {:?}", e); return Ok(()); }
            };
            box_take_get_metadata_v1(params)?
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn box_put_get_metadata_v1(params: PutParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((BOX_CONTRACT_ZKAS_PUT_NS_V1.to_string(), vec![
        params.box_id.inner(), params.old_contents_commit, params.new_contents_commit,
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![params.owner];
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
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![params.owner];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = BoxFunction::try_from(self_.data.data[0])?;

    match func {
        BoxFunction::PutV1 => {
            let params: PutParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[box::put_v1] Put into box {:?}", params.box_id.inner());
            if params.old_contents_commit != pallas::Base::zero() {
                return Err(BoxError::BoxNotEmpty.into());
            }
            let update = PutUpdateV1 { box_id: params.box_id, new_contents_commit: params.new_contents_commit };
            wasm::util::set_return_data(&serialize(&(BoxFunction::PutV1 as u8, update)))?;
        }
        BoxFunction::TakeV1 => {
            let params: TakeParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[box::take_v1] Take from box {:?}", params.box_id.inner());
            let nullifiers_db = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
                return Err(BoxError::DuplicateNullifier.into());
            }
            let update = TakeUpdateV1 { box_id: params.box_id, nullifier: params.nullifier };
            wasm::util::set_return_data(&serialize(&(BoxFunction::TakeV1 as u8, update)))?;
        }
        _ => return Err(ContractError::InvalidFunction),
    };

    Ok(())
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
        _ => {
            msg!("[box::process_update] Error: Unknown function selector");
            Err(ContractError::InvalidFunction)
        }
    }
}
