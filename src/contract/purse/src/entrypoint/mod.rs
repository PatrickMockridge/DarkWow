use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
        poseidon_hash, ContractId,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::PurseError,
    model::{BalanceParamsV1, DepositParamsV1, DepositUpdateV1, Purse, WithdrawParamsV1, WithdrawUpdateV1},
    PurseFunction,
    PURSE_CONTRACT_DB_VERSION, PURSE_CONTRACT_INFO_TREE, PURSE_CONTRACT_NULLIFIERS_TREE,
    PURSE_CONTRACT_PURSES_TREE,
    PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1, PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1,
    PURSE_CONTRACT_ZKAS_BALANCE_NS_V1,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[purse::init_contract] Initializing Purse contract");
    let deposit_bin = include_bytes!("../../proof/deposit_v1.zk.bin");
    let withdraw_bin = include_bytes!("../../proof/withdraw_v1.zk.bin");
    let balance_bin = include_bytes!("../../proof/balance_v1.zk.bin");
    wasm::db::zkas_db_set(&deposit_bin[..])?;
    wasm::db::zkas_db_set(&withdraw_bin[..])?;
    wasm::db::zkas_db_set(&balance_bin[..])?;

    if wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_PURSES_TREE)?;
    }
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_INFO_TREE)?;
    }
    Ok(())
}

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PurseFunction::try_from(self_.data[0])?;

    let metadata = match func {
        PurseFunction::DepositV1 => {
            let params: DepositParamsV1 = deserialize(&self_.data[1..])?;
            purse_deposit_get_metadata_v1(params)?
        }
        PurseFunction::WithdrawV1 => {
            let params: WithdrawParamsV1 = deserialize(&self_.data[1..])?;
            purse_withdraw_get_metadata_v1(params)?
        }
        PurseFunction::BalanceV1 => {
            let params: BalanceParamsV1 = deserialize(&self_.data[1..])?;
            purse_balance_get_metadata_v1(params)?
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn purse_deposit_get_metadata_v1(params: DepositParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let old_coords = params.old_balance_commit.to_affine().coordinates();
    let new_coords = params.new_balance_commit.to_affine().coordinates();
    if old_coords.is_none().into() || new_coords.is_none().into() {
        return Err(ContractError::InvalidFunction);
    }
    let old_coords = old_coords.unwrap();
    let new_coords = new_coords.unwrap();
    zk_inputs.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1.to_string(), vec![params.purse_id, *old_coords.x(), *old_coords.y(), *new_coords.x(), *new_coords.y()]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<pallas::Base> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn purse_withdraw_get_metadata_v1(params: WithdrawParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let old_coords = params.old_balance_commit.to_affine().coordinates();
    let new_coords = params.new_balance_commit.to_affine().coordinates();
    if old_coords.is_none().into() || new_coords.is_none().into() {
        return Err(ContractError::InvalidFunction);
    }
    let old_coords = old_coords.unwrap();
    let new_coords = new_coords.unwrap();
    zk_inputs.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1.to_string(), vec![params.purse_id, *old_coords.x(), *old_coords.y(), *new_coords.x(), *new_coords.y(), params.nullifier]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<pallas::Base> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn purse_balance_get_metadata_v1(params: BalanceParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let coords = params.balance_commit.to_affine().coordinates();
    if coords.is_none().into() {
        return Err(ContractError::InvalidFunction);
    }
    let coords = coords.unwrap();
    zk_inputs.push((PURSE_CONTRACT_ZKAS_BALANCE_NS_V1.to_string(), vec![params.purse_id, *coords.x(), *coords.y(), params.token_commit]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<pallas::Base> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = PurseFunction::try_from(self_.data.data[0])?;

    match func {
        PurseFunction::DepositV1 => {
            let params: DepositParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[purse::deposit_v1] Deposit {} to purse {:?}", params.deposit_amount, params.purse_id);
            let update = DepositUpdateV1 { purse_id: params.purse_id, new_balance_commit: params.new_balance_commit, deposit_amount: params.deposit_amount };
            let _ = wasm::util::set_return_data(&serialize(&(PurseFunction::DepositV1 as u8, update)));
        }
        PurseFunction::WithdrawV1 => {
            let params: WithdrawParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[purse::withdraw_v1] Withdraw {} from purse {:?}", params.withdraw_amount, params.purse_id);
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
                return Err(PurseError::DuplicateNullifier.into());
            }
            let update = WithdrawUpdateV1 { purse_id: params.purse_id, nullifier: params.nullifier, new_balance_commit: params.new_balance_commit, withdraw_amount: params.withdraw_amount };
            let _ = wasm::util::set_return_data(&serialize(&(PurseFunction::WithdrawV1 as u8, update)));
        }
        PurseFunction::BalanceV1 => {
            let _params: BalanceParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[purse::balance_v1] Balance check");
        }
        _ => return Err(ContractError::InvalidFunction),
    };

    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = PurseFunction::try_from(update_data[0])?;
    match func {
        PurseFunction::DepositV1 => {
            let update: DepositUpdateV1 = deserialize(&update_data[1..])?;
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            let mut purse: Purse = match wasm::db::db_get(purses_db, &serialize(&update.purse_id))? {
                Some(data) => deserialize(&data)?,
                None => Purse { version: 1, purse_id: update.purse_id, token_commit: pallas::Base::zero(), balance_commit: update.new_balance_commit, owner_commit: pallas::Base::zero() },
            };
            purse.balance_commit = update.new_balance_commit;
            wasm::db::db_set(purses_db, &serialize(&update.purse_id), &serialize(&purse))?;
            Ok(())
        }
        PurseFunction::WithdrawV1 => {
            let update: WithdrawUpdateV1 = deserialize(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            if let Some(data) = wasm::db::db_get(purses_db, &serialize(&update.purse_id))? {
                let mut purse: Purse = deserialize(&data)?;
                purse.balance_commit = update.new_balance_commit;
                wasm::db::db_set(purses_db, &serialize(&update.purse_id), &serialize(&purse))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
