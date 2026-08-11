use dwow_sdk::{
    crypto::{merkle_anchor, ContractId, MerkleNode, MerkleTree},
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
        BoxFunction::Put => { let p = PutParams::decode(&self_.data[1..]).map_err(|e| { msg!("[box::metadata] put decode: {:?}", e); e })?; put_metadata(p)? }
        BoxFunction::Take => { let p = TakeParams::decode(&self_.data[1..]).map_err(|e| { msg!("[box::metadata] take decode: {:?}", e); e })?; take_metadata(p)? }
        BoxFunction::Initialize => vec![],
    };
    wasm::util::set_return_data(&metadata)
}

fn put_metadata(p: PutParams) -> Result<Vec<u8>, ContractError> {
    // L1 metadata boundary (Boundary 4): type-annotated extraction.
    // Order MUST match circuit constrain_instance order.
    // Each .inner() call is isolated here — nowhere else in the contract.
    let zk_nullifier: pallas::Base = p.nullifier.inner();
    let zk_expected_root: pallas::Base = p.expected_root.inner();
    let zk_new_leaf: pallas::Base = p.new_leaf.inner();
    let zk_tx_binding: pallas::Base = p.tx_binding;
    let zk_tx_nonce: pallas::Base = p.tx_nonce;

    let mut z = vec![]; z.push((BOX_CONTRACT_ZKAS_PUT_NS.to_string(), vec![zk_nullifier, zk_expected_root, zk_new_leaf, zk_tx_binding, zk_tx_nonce]));
    let mut m = vec![]; z.encode(&mut m)?; let s: Vec<dwow_sdk::crypto::PublicKey> = vec![]; s.encode(&mut m)?; Ok(m)
}
fn take_metadata(p: TakeParams) -> Result<Vec<u8>, ContractError> {
    // L1 metadata boundary (Boundary 4): type-annotated extraction.
    let zk_nullifier: pallas::Base = p.nullifier.inner();
    let zk_expected_root: pallas::Base = p.expected_root.inner();
    let zk_tx_binding: pallas::Base = p.tx_binding;
    let zk_tx_nonce: pallas::Base = p.tx_nonce;

    let mut z = vec![]; z.push((BOX_CONTRACT_ZKAS_TAKE_NS.to_string(), vec![zk_nullifier, zk_expected_root, zk_tx_binding, zk_tx_nonce]));
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
            // Root check: skip when latest root is still the EMPTY genesis root
            // (first operation self-populates the tree)
            let idb_root = wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE)?;
            let skip_root_check = match wasm::db::db_get(idb_root, BOX_CONTRACT_LATEST_BOX_ROOT)? {
                Some(ref data) if data.len() == 32 => {
                    data == &EMPTY_BOX_TREE_ROOT
                }
                _ => true,
            };
            if !skip_root_check {
                let rdb = wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
                if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[box::put] Error: Invalid Merkle root"); return Err(BoxError::InvalidMerkleRoot.into()); }
            }
            let u = PutUpdate { nullifier: p.nullifier, new_leaf: p.new_leaf };
            wasm::util::set_return_data(&[&[func_tag(func)], &u.encode()?[..]].concat())?;
        }
        BoxFunction::Take => {
            let p = TakeParams::decode(&self_.data.data[1..])?; msg!("[box::take] Take");
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(ndb, &p.nullifier.to_bytes())? { msg!("[box::take] Error: Duplicate nullifier"); return Err(BoxError::DuplicateNullifier.into()); }
            let idb_root = wasm::db::db_lookup(cid, BOX_CONTRACT_INFO_TREE)?;
            let skip_root_check = match wasm::db::db_get(idb_root, BOX_CONTRACT_LATEST_BOX_ROOT)? {
                Some(ref data) if data.len() == 32 => {
                    data == &EMPTY_BOX_TREE_ROOT
                }
                _ => true,
            };
            if !skip_root_check {
                let rdb = wasm::db::db_lookup(cid, BOX_CONTRACT_BOX_ROOTS_TREE)?;
                if !wasm::db::db_contains_key(rdb, &p.expected_root.to_bytes())? { msg!("[box::take] Error: Invalid Merkle root"); return Err(BoxError::InvalidMerkleRoot.into()); }
            }
            // Read the current root in Exec (allowed) and pass through
            // the Exec→Apply bridge so Apply doesn't need db_get.
            let current_root = match wasm::db::db_get(idb_root, BOX_CONTRACT_LATEST_BOX_ROOT)? {
                Some(ref data) if data.len() == 32 => {
                    MerkleNode::from_bytes(data[..32].try_into().map_err(|_|
                        ContractError::IoError("Take root".into()))?)
                    .unwrap_or(MerkleNode::from_base(pallas::Base::zero()))
                }
                _ => MerkleNode::from_base(pallas::Base::zero()),
            };
            let u = TakeUpdate { nullifier: p.nullifier, current_root };
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
            // merkle_add now returns the new root directly — no db_get needed.
            // G-2: host→guest write, ACL preserved, Apply SHALL NOT read state.
            let contract_root = wasm::merkle::merkle_add(idb, rdb, BOX_CONTRACT_LATEST_BOX_ROOT, BOX_CONTRACT_BOX_MERKLE_TREE, &[u.new_leaf])?;
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
            // Block-level anchoring (§C.3.7) — after nullifier write (R7)
            let entry = merkle_anchor::AnchorEntry::new(u.nullifier, cid, contract_root);
            wasm::merkle::merkle_anchor_add(&entry.to_leaf_bytes())?;
        }
        BoxFunction::Take => {
            let u = TakeUpdate::decode(&update_data[1..])?;
            // Block-level anchoring (§C.3.7) — terminal consumption, anchor with
            // current contract tree root before nullifying (R3).
            // root was read in Exec and passed through TakeUpdate — no db_get in Update.
            let entry = merkle_anchor::AnchorEntry::new(u.nullifier, cid, u.current_root);
            wasm::merkle::merkle_anchor_add(&entry.to_leaf_bytes())?;
            let ndb = wasm::db::db_lookup(cid, BOX_CONTRACT_NULLIFIERS_TREE)?; wasm::db::db_set(ndb, &u.nullifier.to_bytes(), &[])?;
        }
        BoxFunction::Initialize => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}
