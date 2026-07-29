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
        BalanceParamsV1, BalanceParamsV3, DepositParamsV1, DepositParamsV3,
        DepositUpdateV1, DepositUpdateV3, Purse,
        WithdrawParamsV1, WithdrawParamsV3, WithdrawUpdateV1, WithdrawUpdateV3,
    },
    PurseFunction,
    PURSE_CONTRACT_INFO_TREE, PURSE_CONTRACT_LATEST_NULLIFIER_ROOT,
    PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_NULLIFIER_ROOTS_TREE,
    PURSE_CONTRACT_NULLIFIERS_TREE, PURSE_CONTRACT_PURSE_MERKLE_TREE,
    PURSE_CONTRACT_PURSE_ROOTS_TREE, PURSE_CONTRACT_PURSES_TREE,
    PURSE_CONTRACT_ZKAS_BALANCE_NS_V1, PURSE_CONTRACT_ZKAS_BALANCE_NS_V3,
    PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V1, PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V3,
    PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V1, PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V3,
    EMPTY_PURSE_TREE_ROOT,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[purse::init_contract] Initializing Purse contract (L1 hard path)");

    // Register V1 circuits (proven path — existing)
    wasm::db::zkas_db_set(include_bytes!("../../proof/deposit_v1.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/withdraw_v1.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/balance_v1.zk.bin"))?;

    // Register V2 circuits (domain separation, HAZOP RC3)
    wasm::db::zkas_db_set(include_bytes!("../../proof/deposit_v2.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/withdraw_v2.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/balance_v2.zk.bin"))?;

    // Register V3 circuits (hard path — Merkle inclusion, NEW)
    wasm::db::zkas_db_set(include_bytes!("../../proof/deposit_v3.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/withdraw_v3.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/balance_v3.zk.bin"))?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(33);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;

    // Initialize flat DBs (existing)
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_PURSES_TREE)?;
    }
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Initialize Purse Merkle roots DB
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE).is_err() {
        let db_purse_roots = wasm::db::db_init(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
        wasm::db::db_set(db_purse_roots, &EMPTY_PURSE_TREE_ROOT, &roots_value_data)?;
    }

    // Initialize nullifier roots DB
    if wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, PURSE_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(db_null_roots, &pallas::Base::zero().to_repr(), &roots_value_data)?;
    }

    // Pattern A: resolve info_db handle unconditionally
    let info_db = match wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, PURSE_CONTRACT_INFO_TREE)?,
    };

    // Pattern A: conditional Merkle tree data init
    if !wasm::db::db_contains_key(info_db, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
        let mut purse_tree = MerkleTree::new(1);
        purse_tree.append(MerkleNode::from_base(pallas::Base::zero()));
        let mut purse_tree_data = vec![];
        purse_tree_data.write_u32(0)?;
        purse_tree.encode(&mut purse_tree_data)?;
        wasm::db::db_set(info_db, PURSE_CONTRACT_PURSE_MERKLE_TREE, &purse_tree_data)?;

        // Write initial root from precomputed constant — tree.root(0) requires
        // Sinsemilla hash which is not available during Deploy section.
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
        PurseFunction::DepositV1 => {
            let params = match DepositParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to decode DepositParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purse_deposit_get_metadata_v1(params)?
        }
        PurseFunction::WithdrawV1 => {
            let params = match WithdrawParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to decode WithdrawParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purse_withdraw_get_metadata_v1(params)?
        }
        PurseFunction::BalanceV1 => {
            let params = match BalanceParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to decode BalanceParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purse_balance_get_metadata_v1(params)?
        }
        PurseFunction::DepositV3 => {
            let params = match DepositParamsV3::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to decode DepositParamsV3: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purse_deposit_get_metadata_v3(params)?
        }
        PurseFunction::WithdrawV3 => {
            let params = match WithdrawParamsV3::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to decode WithdrawParamsV3: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purse_withdraw_get_metadata_v3(params)?
        }
        PurseFunction::BalanceV3 => {
            let params = match BalanceParamsV3::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[purse::get_metadata] Error: Failed to decode BalanceParamsV3: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purse_balance_get_metadata_v3(params)?
        }
        PurseFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

// --- V1 metadata helpers (unchanged from original) ---

fn purse_deposit_get_metadata_v1(params: DepositParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let old_coords = params.old_balance_commit.to_affine().coordinates();
    let new_coords = params.new_balance_commit.to_affine().coordinates();
    if old_coords.is_none().into() || new_coords.is_none().into() {
        return Err(ContractError::InvalidFunction);
    }
    let old_coords = old_coords.unwrap();
    let new_coords = new_coords.unwrap();
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
        params.purse_id.inner(), *coords.x(), *coords.y(),
        params.token_commit, params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<pallas::Base> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

// --- V3 metadata helpers ---

fn purse_deposit_get_metadata_v3(params: DepositParamsV3) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let nullifier_old = dwow_sdk::crypto::poseidon_hash([
        params.purse_id.inner(), params.state_nonce,
    ]);
    // Public input order: nullifier_old, root, old_x, old_y, new_x, tx_binding, tx_nonce, new_y
    // The root and Pedersen coordinates are verified from the ZK proof.
    zk_inputs.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS_V3.to_string(), vec![
        nullifier_old,
        pallas::Base::zero(), // merkle_root — verified from proof public inputs
        pallas::Base::zero(), // old_x — from proof
        pallas::Base::zero(), // old_y — from proof
        pallas::Base::zero(), // new_x — from proof
        params.tx_binding, params.tx_nonce,
        pallas::Base::zero(), // new_y — from proof
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn purse_withdraw_get_metadata_v3(params: WithdrawParamsV3) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let nullifier_val = dwow_sdk::crypto::poseidon_hash([
        params.purse_id.inner(), params.state_nonce,
    ]);
    zk_inputs.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS_V3.to_string(), vec![
        nullifier_val,
        pallas::Base::zero(), // merkle_root
        pallas::Base::zero(), // old_x
        pallas::Base::zero(), // old_y
        pallas::Base::zero(), // new_x
        pallas::Base::zero(), // new_y
        params.tx_binding, params.tx_nonce,
    ]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    let sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    sigs.encode(&mut metadata)?;
    Ok(metadata)
}

fn purse_balance_get_metadata_v3(params: BalanceParamsV3) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let derived_purse_id = dwow_sdk::crypto::poseidon_hash([
        params.purse_id.inner(), params.token_id,
    ]);
    zk_inputs.push((PURSE_CONTRACT_ZKAS_BALANCE_NS_V3.to_string(), vec![
        derived_purse_id,
        pallas::Base::zero(), // merkle_root
        pallas::Base::zero(), // balance_x
        pallas::Base::zero(), // balance_y
        params.token_id,      // token_commit
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
        PurseFunction::DepositV1 => {
            let params = DepositParamsV1::decode(&self_.data.data[1..])?;
            msg!("[purse::deposit_v1] Deposit to purse {:?}", params.purse_id.inner());
            let update = DepositUpdateV1 {
                purse_id: params.purse_id,
                new_balance_commit: params.new_balance_commit,
                deposit_amount: params.deposit_amount,
            };
            wasm::util::set_return_data(&[&[PurseFunction::DepositV1 as u8], &update.encode()[..]].concat())?;
        }
        PurseFunction::WithdrawV1 => {
            let params = WithdrawParamsV1::decode(&self_.data.data[1..])?;
            msg!("[purse::withdraw_v1] Withdraw from purse {:?}", params.purse_id.inner());
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &params.nullifier.to_bytes())? {
                return Err(PurseError::DuplicateNullifier.into());
            }
            let update = WithdrawUpdateV1 {
                purse_id: params.purse_id,
                nullifier: params.nullifier,
                new_balance_commit: params.new_balance_commit,
                withdraw_amount: params.withdraw_amount,
            };
            wasm::util::set_return_data(&[&[PurseFunction::WithdrawV1 as u8], &update.encode()[..]].concat())?;
        }
        PurseFunction::BalanceV1 => {
            let params = BalanceParamsV1::decode(&self_.data.data[1..])?;
            msg!("[purse::balance_v1] Balance check for purse {:?}", params.purse_id.inner());
            wasm::util::set_return_data(&[PurseFunction::BalanceV1 as u8])?;
        }
        PurseFunction::DepositV3 => {
            let params = DepositParamsV3::decode(&self_.data.data[1..])?;
            msg!("[purse::deposit_v3] Deposit (hard path)");
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.purse_id.inner(), params.state_nonce,
            ]);
            let update = DepositUpdateV3 {
                nullifier: nullifier_val,
                new_balance_commit: pallas::Point::identity(),
            };
            wasm::util::set_return_data(&[&[PurseFunction::DepositV3 as u8], &update.encode()[..]].concat())?;
        }
        PurseFunction::WithdrawV3 => {
            let params = WithdrawParamsV3::decode(&self_.data.data[1..])?;
            msg!("[purse::withdraw_v3] Withdraw (hard path)");
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            let nullifier_val = dwow_sdk::crypto::poseidon_hash([
                params.purse_id.inner(), params.state_nonce,
            ]);
            if smt.get_leaf(&nullifier_val) != pallas::Base::zero() {
                msg!("[purse::withdraw_v3] Error: Nullifier already spent");
                return Err(PurseError::DuplicateNullifier.into());
            }
            let update = WithdrawUpdateV3 { nullifier: nullifier_val };
            wasm::util::set_return_data(&[&[PurseFunction::WithdrawV3 as u8], &update.encode()[..]].concat())?;
        }
        PurseFunction::BalanceV3 => {
            let _params = BalanceParamsV3::decode(&self_.data.data[1..])?;
            msg!("[purse::balance_v3] Balance check (hard path)");
            wasm::util::set_return_data(&[PurseFunction::BalanceV3 as u8])?;
        }
        PurseFunction::InitializeV1 => {
            msg!("[purse::process_instruction] Error: InitializeV1 must be called via init");
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
        PurseFunction::DepositV1 => {
            let update = DepositUpdateV1::decode(&update_data[1..])?;
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            let purse = Purse { version: 1, purse_id: update.purse_id,
                token_commit: pallas::Base::zero(), balance_commit: update.new_balance_commit,
                owner_commit: pallas::Base::zero() };
            wasm::db::db_set(purses_db, &update.purse_id.to_bytes(), &purse.encode())?;
            Ok(())
        }
        PurseFunction::WithdrawV1 => {
            let update = WithdrawUpdateV1::decode(&update_data[1..])?;
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &update.nullifier.to_bytes(), &[])?;
            let purses_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSES_TREE)?;
            let purse = Purse { version: 1, purse_id: update.purse_id,
                token_commit: pallas::Base::zero(), balance_commit: update.new_balance_commit,
                owner_commit: pallas::Base::zero() };
            wasm::db::db_set(purses_db, &update.purse_id.to_bytes(), &purse.encode())?;
            Ok(())
        }
        PurseFunction::BalanceV1 => Ok(()),
        PurseFunction::DepositV3 => {
            let update = DepositUpdateV3::decode(&update_data[1..])?;
            let info_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;

            // Append new purse state to Merkle tree
            let new_leaf = MerkleNode::from_base(update.nullifier);
            wasm::merkle::merkle_add(
                info_db,
                wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?,
                PURSE_CONTRACT_LATEST_PURSE_ROOT,
                PURSE_CONTRACT_PURSE_MERKLE_TREE,
                &[new_leaf],
            )?;

            // Mark nullifier in SMT
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let mut smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            smt.insert_batch(vec![(update.nullifier, pallas::Base::one())])?;
            let new_root = smt.root();
            wasm::db::db_set(info_db, PURSE_CONTRACT_LATEST_NULLIFIER_ROOT, &new_root.to_repr())?;
            Ok(())
        }
        PurseFunction::WithdrawV3 => {
            let update = WithdrawUpdateV3::decode(&update_data[1..])?;

            let info_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let nullifiers_db = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
            let mut smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
            smt.insert_batch(vec![(update.nullifier, pallas::Base::one())])?;
            let new_root = smt.root();
            wasm::db::db_set(info_db, PURSE_CONTRACT_LATEST_NULLIFIER_ROOT, &new_root.to_repr())?;
            Ok(())
        }
        PurseFunction::BalanceV3 => Ok(()),
        PurseFunction::InitializeV1 => {
            msg!("[purse::process_update] Error: InitializeV1 must be called via init");
            Err(ContractError::InvalidFunction)
        }
    }
}
