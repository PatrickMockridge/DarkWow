use dwow_sdk::{
    crypto::{
        pasta_prelude::*, ContractId, Nullifier,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::{group::GroupEncoding, pallas},
    ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::PurseError,
    model::{BalanceParamsV1, DepositParamsV1, DepositUpdateV1, Purse, PurseId, WithdrawParamsV1, WithdrawUpdateV1},
    PurseFunction, PURSE_CONTRACT_INFO_TREE, PURSE_CONTRACT_NULLIFIERS_TREE,
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

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PurseFunction::try_from(self_.data[0])?;

    let metadata = match func {
        PurseFunction::DepositV1 => {
            let params = match DepositParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to deserialize DepositParamsV1: {:?}", e); return Ok(()); }
            };
            purse_deposit_get_metadata_v1(params)?
        }
        PurseFunction::WithdrawV1 => {
            let params = match WithdrawParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to deserialize WithdrawParamsV1: {:?}", e); return Ok(()); }
            };
            purse_withdraw_get_metadata_v1(params)?
        }
        PurseFunction::BalanceV1 => {
            let params = match BalanceParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to deserialize BalanceParamsV1: {:?}", e); return Ok(()); }
            };
            purse_balance_get_metadata_v1(params)?
        }
        PurseFunction::InitializeV1 => vec![],
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
    // Order MUST match circuit constrain_instance:
    // purse_id, old_x, old_y, new_x, tx_binding, tx_nonce, new_y
    zk_inputs.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1.to_string(), vec![
        params.purse_id.inner(), *old_coords.x(), *old_coords.y(),
        *new_coords.x(),
        params.tx_binding, params.tx_nonce,
        *new_coords.y(),
    ]));
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
    zk_inputs.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1.to_string(), vec![
        params.nullifier.inner(), params.purse_id.inner(),
        *old_coords.x(), *old_coords.y(), *new_coords.x(), *new_coords.y(),
        params.tx_binding, params.tx_nonce,
    ]));
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
    zk_inputs.push((PURSE_CONTRACT_ZKAS_BALANCE_NS_V1.to_string(), vec![
        params.purse_id.inner(), *coords.x(), *coords.y(), params.token_commit,
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<pallas::Base> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

// --- Rho-calculus explicit encode/decode for update structs ---
// Per type-system.md §2.2: bytes round-trip across module boundaries is forbidden.
// Per §10.5: re-lift validation SHALL use named constructors (from_bytes).
// quote(x) = to_bytes(), eval(x) = from_bytes() with validation.

fn encode_deposit_update_v1(update: &DepositUpdateV1) -> Vec<u8> {
    let mut buf = Vec::with_capacity(73); // 1 byte func_code + 72 bytes fields
    buf.push(PurseFunction::DepositV1 as u8);
    buf.extend_from_slice(&update.purse_id.to_bytes());
    buf.extend_from_slice(&update.new_balance_commit.to_bytes());
    buf.extend_from_slice(&update.deposit_amount.to_le_bytes());
    buf
}

fn decode_deposit_update_v1(data: &[u8]) -> Result<DepositUpdateV1, ContractError> {
    const EXPECTED: usize = 72;
    if data.len() != EXPECTED {
        return Err(ContractError::IoError(format!(
            "DepositUpdateV1: expected {} bytes, got {}", EXPECTED, data.len()
        )));
    }
    let purse_id = PurseId::from_bytes(&data[0..32].try_into().unwrap())
        .ok_or_else(|| ContractError::IoError("DepositUpdateV1: invalid PurseId bytes".into()))?;
    let new_balance_commit = Option::<pallas::Point>::from(
        pallas::Point::from_bytes(data[32..64].try_into().unwrap())
    ).ok_or_else(|| ContractError::IoError("DepositUpdateV1: invalid Point bytes".into()))?;
    let deposit_amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
    Ok(DepositUpdateV1 { purse_id, new_balance_commit, deposit_amount })
}

fn encode_withdraw_update_v1(update: &WithdrawUpdateV1) -> Vec<u8> {
    let mut buf = Vec::with_capacity(105); // 1 + 104
    buf.push(PurseFunction::WithdrawV1 as u8);
    buf.extend_from_slice(&update.purse_id.to_bytes());
    buf.extend_from_slice(&update.nullifier.to_bytes());
    buf.extend_from_slice(&update.new_balance_commit.to_bytes());
    buf.extend_from_slice(&update.withdraw_amount.to_le_bytes());
    buf
}

fn decode_withdraw_update_v1(data: &[u8]) -> Result<WithdrawUpdateV1, ContractError> {
    if data.len() != 104 {
        return Err(ContractError::Custom(0xD100));
    }
    let purse_id = PurseId::from_bytes(data[0..32].try_into().unwrap())
        .ok_or(ContractError::Custom(0xD101))?;
    let nullifier = Nullifier::from_bytes(data[32..64].try_into().unwrap())
        .map_err(|_| ContractError::Custom(0xD102))?;
    let new_balance_commit = Option::<pallas::Point>::from(
        pallas::Point::from_bytes(data[64..96].try_into().unwrap())
    ).ok_or(ContractError::Custom(0xD103))?;
    let withdraw_amount = u64::from_le_bytes(data[96..104].try_into().unwrap());
    Ok(WithdrawUpdateV1 { purse_id, nullifier, new_balance_commit, withdraw_amount })
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
            wasm::util::set_return_data(&encode_deposit_update_v1(&update))?;
        }
        PurseFunction::WithdrawV1 => {
            let params: WithdrawParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[purse::withdraw_v1] Withdraw {} from purse {:?}", params.withdraw_amount, params.purse_id);
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &params.nullifier.to_bytes())? {
                return Err(PurseError::DuplicateNullifier.into());
            }
            let update = WithdrawUpdateV1 { purse_id: params.purse_id, nullifier: params.nullifier, new_balance_commit: params.new_balance_commit, withdraw_amount: params.withdraw_amount };
            wasm::util::set_return_data(&encode_withdraw_update_v1(&update))?;
        }
        PurseFunction::BalanceV1 => {
            let _params: BalanceParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[purse::balance_v1] Balance check");
        }
        PurseFunction::InitializeV1 => {
            msg!("[purse::process_instruction] Error: InitializeV1 must be called via init");
            return Err(ContractError::InvalidFunction);
        }
    };

    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = PurseFunction::try_from(update_data[0])?;
    match func {
        PurseFunction::DepositV1 => {
            let update = decode_deposit_update_v1(&update_data[1..])?;
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            let mut purse: Purse = match wasm::db::db_get(purses_db, &update.purse_id.to_bytes())? {
                Some(data) => Purse::decode(&data)?,
                None => Purse { version: 1, purse_id: update.purse_id, token_commit: pallas::Base::zero(), balance_commit: update.new_balance_commit, owner_commit: pallas::Base::zero() },
            };
            purse.balance_commit = update.new_balance_commit;
            wasm::db::db_set(purses_db, &update.purse_id.to_bytes(), &purse.encode())?;
            Ok(())
        }
        PurseFunction::WithdrawV1 => {
            let update = decode_withdraw_update_v1(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &update.nullifier.to_bytes(), &[])?;
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            if let Some(data) = wasm::db::db_get(purses_db, &update.purse_id.to_bytes())? {
                let mut purse: Purse = Purse::decode(&data)?;
                purse.balance_commit = update.new_balance_commit;
                wasm::db::db_set(purses_db, &update.purse_id.to_bytes(), &purse.encode())?;
            }
            msg!("[purse::process_update::WithdrawV1] complete");
            Ok(())
        }
        PurseFunction::BalanceV1 => {
            // BalanceV1 is read-only; no state updates needed.
            Ok(())
        }
        PurseFunction::InitializeV1 => {
            msg!("[purse::process_update] Error: InitializeV1 must be called via init");
            Err(ContractError::InvalidFunction)
        }
    }
}
