use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, serialize};

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

fn get_metadata(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let func = PurseFunction::try_from(self_.data[0])?;
    match func {
        PurseFunction::DepositV1 => {
            let params: DepositParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            let vc_x = pallas::Base::from_bytes(&params.new_balance_commit.to_affine().coordinates().unwrap().x().to_repr()).unwrap_or(pallas::Base::zero());
            let vc_y = pallas::Base::from_bytes(&params.new_balance_commit.to_affine().coordinates().unwrap().y().to_repr()).unwrap_or(pallas::Base::zero());
            let ovc_x = pallas::Base::from_bytes(&params.old_balance_commit.to_affine().coordinates().unwrap().x().to_repr()).unwrap_or(pallas::Base::zero());
            let ovc_y = pallas::Base::from_bytes(&params.old_balance_commit.to_affine().coordinates().unwrap().y().to_repr()).unwrap_or(pallas::Base::zero());
            zk_inputs.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1.to_string(), vec![params.purse_id, ovc_x, ovc_y, vc_x, vc_y]));
            let mut metadata = vec![];
            dwow_serial::Encodable::encode(&zk_inputs, &mut metadata)?;
            let sigs: Vec<pallas::Base> = vec![];
            sigs.encode(&mut metadata)?;
            Ok(metadata)
        }
        PurseFunction::WithdrawV1 => {
            let params: WithdrawParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            let vc_x = pallas::Base::from_bytes(&params.new_balance_commit.to_affine().coordinates().unwrap().x().to_repr()).unwrap_or(pallas::Base::zero());
            let vc_y = pallas::Base::from_bytes(&params.new_balance_commit.to_affine().coordinates().unwrap().y().to_repr()).unwrap_or(pallas::Base::zero());
            let ovc_x = pallas::Base::from_bytes(&params.old_balance_commit.to_affine().coordinates().unwrap().x().to_repr()).unwrap_or(pallas::Base::zero());
            let ovc_y = pallas::Base::from_bytes(&params.old_balance_commit.to_affine().coordinates().unwrap().y().to_repr()).unwrap_or(pallas::Base::zero());
            zk_inputs.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1.to_string(), vec![params.purse_id, ovc_x, ovc_y, vc_x, vc_y, params.nullifier]));
            let mut metadata = vec![];
            dwow_serial::Encodable::encode(&zk_inputs, &mut metadata)?;
            let sigs: Vec<pallas::Base> = vec![];
            sigs.encode(&mut metadata)?;
            Ok(metadata)
        }
        PurseFunction::BalanceV1 => {
            let params: BalanceParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            let vc_x = pallas::Base::from_bytes(&params.balance_commit.to_affine().coordinates().unwrap().x().to_repr()).unwrap_or(pallas::Base::zero());
            let vc_y = pallas::Base::from_bytes(&params.balance_commit.to_affine().coordinates().unwrap().y().to_repr()).unwrap_or(pallas::Base::zero());
            zk_inputs.push((PURSE_CONTRACT_ZKAS_BALANCE_NS_V1.to_string(), vec![params.purse_id, vc_x, vc_y, params.token_commit]));
            let mut metadata = vec![];
            dwow_serial::Encodable::encode(&zk_inputs, &mut metadata)?;
            let sigs: Vec<pallas::Base> = vec![];
            sigs.encode(&mut metadata)?;
            Ok(metadata)
        }
        _ => Err(ContractError::InvalidFunction),
    }
}

fn process_instruction(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let func = PurseFunction::try_from(self_.data[0])?;
    match func {
        PurseFunction::DepositV1 => {
            let params: DepositParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[purse::deposit_v1] Deposit {} to purse {:?}", params.deposit_amount, params.purse_id);
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            let update = DepositUpdateV1 { purse_id: params.purse_id, new_balance_commit: params.new_balance_commit, deposit_amount: params.deposit_amount };
            Ok(serialize(&(PurseFunction::DepositV1 as u8, update)))
        }
        PurseFunction::WithdrawV1 => {
            let params: WithdrawParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[purse::withdraw_v1] Withdraw {} from purse {:?}", params.withdraw_amount, params.purse_id);
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.nullifier))? {
                return Err(PurseError::DuplicateNullifier.into());
            }
            let update = WithdrawUpdateV1 { purse_id: params.purse_id, nullifier: params.nullifier, new_balance_commit: params.new_balance_commit, withdraw_amount: params.withdraw_amount };
            Ok(serialize(&(PurseFunction::WithdrawV1 as u8, update)))
        }
        PurseFunction::BalanceV1 => {
            let _params: BalanceParamsV1 = deserialize(&self_.data[1..])?;
            msg!("[purse::balance_v1] Balance check");
            Ok(vec![])
        }
        _ => Err(ContractError::InvalidFunction),
    }
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
