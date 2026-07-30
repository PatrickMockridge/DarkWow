use dwow_sdk::{
    crypto::{pasta_prelude::*, merkle_anchor, ContractId, MerkleNode, MerkleTree},
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
        PurseFunction::Deposit => { let p = DepositParams::decode(&self_.data[1..]).map_err(|e| { msg!("[purse::metadata] deposit: {:?}", e); e })?; deposit_metadata(p)? }
        PurseFunction::Withdraw => { let p = WithdrawParams::decode(&self_.data[1..]).map_err(|e| { msg!("[purse::metadata] withdraw: {:?}", e); e })?; withdraw_metadata(p)? }
        PurseFunction::Balance => { let p = BalanceParams::decode(&self_.data[1..]).map_err(|e| { msg!("[purse::metadata] balance: {:?}", e); e })?; balance_metadata(p)? }
        PurseFunction::Initialize => vec![],
    };
    wasm::util::set_return_data(&metadata)
}

fn deposit_metadata(p: DepositParams) -> Result<Vec<u8>, ContractError> {
    // L1 metadata boundary (Boundary 4): type-annotated extraction.
    // Order MUST match circuit constrain_instance order.
    let zk_nullifier: pallas::Base = p.nullifier.inner();
    let zk_expected_root: pallas::Base = p.expected_root.inner();
    let zk_new_leaf: pallas::Base = p.new_leaf.inner();
    let zk_old_commit_x: pallas::Base = p.old_commit_x;
    let zk_old_commit_y: pallas::Base = p.old_commit_y;
    let zk_new_commit_x: pallas::Base = p.new_commit_x;
    let zk_new_commit_y: pallas::Base = p.new_commit_y;
    let zk_tx_binding: pallas::Base = p.tx_binding;
    let zk_tx_nonce: pallas::Base = p.tx_nonce;

    let mut z = vec![]; z.push((PURSE_CONTRACT_ZKAS_DEPOSIT_NS.to_string(), vec![zk_nullifier, zk_expected_root, zk_old_commit_x, zk_old_commit_y, zk_new_commit_x, zk_new_commit_y, zk_new_leaf, zk_tx_binding, zk_tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}
fn withdraw_metadata(p: WithdrawParams) -> Result<Vec<u8>, ContractError> {
    // L1 metadata boundary (Boundary 4): type-annotated extraction.
    let zk_nullifier: pallas::Base = p.nullifier.inner();
    let zk_expected_root: pallas::Base = p.expected_root.inner();
    let zk_new_leaf: pallas::Base = p.new_leaf.inner();
    let zk_old_commit_x: pallas::Base = p.old_commit_x;
    let zk_old_commit_y: pallas::Base = p.old_commit_y;
    let zk_new_commit_x: pallas::Base = p.new_commit_x;
    let zk_new_commit_y: pallas::Base = p.new_commit_y;
    let zk_tx_binding: pallas::Base = p.tx_binding;
    let zk_tx_nonce: pallas::Base = p.tx_nonce;

    let mut z = vec![]; z.push((PURSE_CONTRACT_ZKAS_WITHDRAW_NS.to_string(), vec![zk_nullifier, zk_expected_root, zk_old_commit_x, zk_old_commit_y, zk_new_commit_x, zk_new_commit_y, zk_new_leaf, zk_tx_binding, zk_tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}
fn balance_metadata(p: BalanceParams) -> Result<Vec<u8>, ContractError> {
    // L1 metadata boundary (Boundary 4): type-annotated extraction.
    let zk_derived_purse_id: pallas::Base = p.derived_purse_id;
    let zk_expected_root: pallas::Base = p.expected_root.inner();
    let zk_balance_commit_x: pallas::Base = p.balance_commit_x;
    let zk_balance_commit_y: pallas::Base = p.balance_commit_y;
    let zk_token_commit: pallas::Base = p.token_commit;
    let zk_tx_binding: pallas::Base = p.tx_binding;
    let zk_tx_nonce: pallas::Base = p.tx_nonce;

    let mut z = vec![]; z.push((PURSE_CONTRACT_ZKAS_BALANCE_NS.to_string(), vec![zk_derived_purse_id, zk_expected_root, zk_balance_commit_x, zk_balance_commit_y, zk_token_commit, zk_tx_binding, zk_tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}

// ============================================================================
// EXEC — nullifier check only
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    if ix.is_empty() { msg!("[purse::process_instruction] Error: Empty call data"); return Err(ContractError::IoError("Empty call data".to_string())); }
    let call_idx = usize::try_from(wasm::util::get_call_index()?).map_err(|e| ContractError::IoError(format!("call_index: {e}")))?;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?; let self_ = &calls[call_idx];
    let func = PurseFunction::try_from(self_.data.data[0])?;
    match func {
        PurseFunction::Deposit => {
            let p = DepositParams::decode(&self_.data.data[1..])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { msg!("[purse::deposit] Error: Duplicate nullifier"); return Err(PurseError::DuplicateNullifier.into()); }
            let idb_root = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let skip_root_check = match wasm::db::db_get(idb_root, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
                Some(ref data) if data.len() > 4 => {
                    let tree: MerkleTree = dwow_serial::deserialize(&data[4..])
                        .map_err(|_| ContractError::IoError("purse tree deser".into()))?;
                    tree.root(0).map_or(true, |r| r.to_bytes() == EMPTY_PURSE_TREE_ROOT)
                }
                _ => true,
            };
            if !skip_root_check {
                let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
                if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[purse::deposit] Error: Invalid Merkle root"); return Err(PurseError::InvalidMerkleRoot.into()); }
            }
            let u = DepositUpdate { nullifier: p.nullifier, new_leaf: p.new_leaf };
            wasm::util::set_return_data(&[&[PurseFunction::Deposit as u8], &u.encode()?[..]].concat())?;
        }
        PurseFunction::Withdraw => {
            let p = WithdrawParams::decode(&self_.data.data[1..])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { msg!("[purse::withdraw] Error: Duplicate nullifier"); return Err(PurseError::DuplicateNullifier.into()); }
            let idb_root = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let skip_root_check = match wasm::db::db_get(idb_root, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
                Some(ref data) if data.len() > 4 => {
                    let tree: MerkleTree = dwow_serial::deserialize(&data[4..])
                        .map_err(|_| ContractError::IoError("purse tree deser".into()))?;
                    tree.root(0).map_or(true, |r| r.to_bytes() == EMPTY_PURSE_TREE_ROOT)
                }
                _ => true,
            };
            if !skip_root_check {
                let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
                if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[purse::withdraw] Error: Invalid Merkle root"); return Err(PurseError::InvalidMerkleRoot.into()); }
            }
            let u = WithdrawUpdate { nullifier: p.nullifier, new_leaf: p.new_leaf };
            wasm::util::set_return_data(&[&[PurseFunction::Withdraw as u8], &u.encode()?[..]].concat())?;
        }
        PurseFunction::Balance => {
            let p = BalanceParams::decode(&self_.data.data[1..])?;
            let idb_root = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let skip_root_check = match wasm::db::db_get(idb_root, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
                Some(ref data) if data.len() > 4 => {
                    let tree: MerkleTree = dwow_serial::deserialize(&data[4..])
                        .map_err(|_| ContractError::IoError("purse tree deser".into()))?;
                    tree.root(0).map_or(true, |r| r.to_bytes() == EMPTY_PURSE_TREE_ROOT)
                }
                _ => true,
            };
            if !skip_root_check {
                let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
                if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[purse::balance] Error: Invalid Merkle root"); return Err(PurseError::InvalidMerkleRoot.into()); }
            }
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
    if update_data.is_empty() { msg!("[purse::process_update] Error: Empty update data"); return Err(ContractError::IoError("Empty update data".to_string())); }
    let func = PurseFunction::try_from(update_data[0])?;
    match func {
        PurseFunction::Deposit => {
            let u = DepositUpdate::decode(&update_data[1..])?; let idb = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            wasm::merkle::merkle_add(idb, rdb, PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_PURSE_MERKLE_TREE, &[u.new_leaf])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
            // Block-level anchoring (§C.3.7) — after nullifier write (R7)
            // Read the updated tree root for anchoring (QC Fix 3)
            let contract_root = if let Some(tree_data) = wasm::db::db_get(idb, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
                let tree: MerkleTree = dwow_serial::deserialize(&tree_data[4..])
                    .map_err(|_| ContractError::IoError("anchor: tree deser".into()))?;
                tree.root(0).unwrap_or(MerkleNode::from_base(pallas::Base::zero()))
            } else {
                MerkleNode::from_base(pallas::Base::zero())
            };
            let entry = merkle_anchor::AnchorEntry::new(u.nullifier, cid, contract_root);
            wasm::merkle::merkle_anchor_add(&entry.to_leaf_bytes())?;
        }
        PurseFunction::Withdraw => {
            let u = WithdrawUpdate::decode(&update_data[1..])?; let idb = wasm::db::db_lookup(cid, PURSE_CONTRACT_INFO_TREE)?;
            let rdb = wasm::db::db_lookup(cid, PURSE_CONTRACT_PURSE_ROOTS_TREE)?;
            wasm::merkle::merkle_add(idb, rdb, PURSE_CONTRACT_LATEST_PURSE_ROOT, PURSE_CONTRACT_PURSE_MERKLE_TREE, &[u.new_leaf])?;
            let ndb = wasm::db::db_lookup(cid, PURSE_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
            // Block-level anchoring (§C.3.7) — after nullifier write (R7)
            // Read the updated tree root for anchoring (QC Fix 3)
            let contract_root = if let Some(tree_data) = wasm::db::db_get(idb, PURSE_CONTRACT_PURSE_MERKLE_TREE)? {
                let tree: MerkleTree = dwow_serial::deserialize(&tree_data[4..])
                    .map_err(|_| ContractError::IoError("anchor: tree deser".into()))?;
                tree.root(0).unwrap_or(MerkleNode::from_base(pallas::Base::zero()))
            } else {
                MerkleNode::from_base(pallas::Base::zero())
            };
            let entry = merkle_anchor::AnchorEntry::new(u.nullifier, cid, contract_root);
            wasm::merkle::merkle_anchor_add(&entry.to_leaf_bytes())?;
        }
        PurseFunction::Balance => {}
        PurseFunction::Initialize => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}
