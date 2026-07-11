use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, Nullifier, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::MultiSigError,
    model::{
        CreateGroupParamsV1, CreateGroupUpdateV1, FinalizeParamsV1, FinalizeUpdateV1,
        GroupId, MultiSigGroup, PartialSignature, SignParamsV1, SignUpdateV1,
    },
    MultiSigFunction,
    MULTISIG_CONTRACT_GROUPS_TREE, MULTISIG_CONTRACT_INFO_TREE,
    MULTISIG_CONTRACT_NULLIFIERS_TREE, MULTISIG_CONTRACT_SIGNATURES_TREE,
    MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V1, MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V1,
    MULTISIG_CONTRACT_ZKAS_SIGN_NS_V1,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[multisig::init_contract] Initializing MultiSig contract");
    wasm::db::zkas_db_set(include_bytes!("../../proof/create_group_v1.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/sign_v1.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/finalize_v1.zk.bin"))?;

    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
    }
    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
    }
    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_INFO_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_INFO_TREE)?;
    }
    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = MultiSigFunction::try_from(self_.data[0])?;
    let metadata: Vec<u8> = match func {
        MultiSigFunction::CreateGroupV1 => {
            let params: CreateGroupParamsV1 = deserialize(&self_.data[1..])?;
            let t = pallas::Base::from(params.threshold as u64);
            let n = pallas::Base::from(params.pubkeys.len() as u64);
            let first_pk = PublicKey::from_bytes(params.pubkeys[0])?;
            // from_bytes rejects identity, so xy() is always Some
            let (fx, fy) = first_pk.xy().expect("pk not identity");
            let group_id = poseidon_hash([fx, fy, t, n]);
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V1.to_string(), vec![group_id, t, n]));
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            meta
        }
        MultiSigFunction::SignV1 => {
            let params: SignParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_SIGN_NS_V1.to_string(), vec![params.group_id.inner(), params.message_hash]));
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            meta
        }
        MultiSigFunction::FinalizeV1 => {
            let params: FinalizeParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V1.to_string(), vec![params.group_id.inner(), params.message_hash]));
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            meta
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = MultiSigFunction::try_from(self_.data.data[0])?;

    match func {
        MultiSigFunction::CreateGroupV1 => {
            let params: CreateGroupParamsV1 = deserialize(&self_.data.data[1..])?;
            if params.pubkeys.is_empty() { return Err(MultiSigError::EmptyKeyList.into()); }
            if params.threshold == 0 || params.threshold as usize > params.pubkeys.len() {
                return Err(MultiSigError::InvalidThreshold.into());
            }
            let mut pubkeys = Vec::with_capacity(params.pubkeys.len());
            for b in &params.pubkeys { pubkeys.push(PublicKey::from_bytes(*b)?); }
            let group_id = MultiSigGroup::derive_group_id(&pubkeys[0], params.threshold, pubkeys.len() as u8);
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if wasm::db::db_contains_key(groups_db, &serialize(&group_id))? {
                return Err(MultiSigError::GroupAlreadyExists.into());
            }
            let _ = wasm::util::set_return_data(&serialize(&(MultiSigFunction::CreateGroupV1 as u8, CreateGroupUpdateV1 {
                group_id, pubkeys, threshold: params.threshold, total_keys: params.pubkeys.len() as u8,
            })));
        }
        MultiSigFunction::SignV1 => {
            let params: SignParamsV1 = deserialize(&self_.data.data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if !wasm::db::db_contains_key(groups_db, &serialize(&params.group_id))? {
                return Err(MultiSigError::GroupNotFound.into());
            }
            let nullifier = poseidon_hash([params.group_id.inner(), params.message_hash]);
            let nullifiers_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier))? {
                return Err(MultiSigError::DuplicateNullifier.into());
            }
            let _ = wasm::util::set_return_data(&serialize(&(MultiSigFunction::SignV1 as u8, SignUpdateV1 {
                group_id: params.group_id, message_hash: params.message_hash,
                nullifier: Nullifier::from_bytes(&nullifier.to_repr()).expect("non-zero poseidon output"),
            })));
        }
        MultiSigFunction::FinalizeV1 => {
            let params: FinalizeParamsV1 = deserialize(&self_.data.data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let data = wasm::db::db_get(groups_db, &serialize(&params.group_id))?
                .ok_or(MultiSigError::GroupNotFound)?;
            let group: MultiSigGroup = deserialize(&data)?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            let mut consumed: Vec<pallas::Base> = Vec::new();
            for pk in &group.pubkeys {
                // group.pubkeys are validated at creation, so xy() is always Some
                let (x, y) = pk.xy().expect("pk not identity");
                let nf = poseidon_hash([params.group_id.inner(), params.message_hash, x, y]);
                if wasm::db::db_contains_key(sigs_db, &serialize(&nf))? { consumed.push(nf); }
            }
            if consumed.len() < group.threshold as usize {
                return Err(MultiSigError::InsufficientSignatures.into());
            }
            let approval_commit = poseidon_hash([params.group_id.inner(), params.message_hash]);
            let _ = wasm::util::set_return_data(&serialize(&(MultiSigFunction::FinalizeV1 as u8, FinalizeUpdateV1 {
                group_id: params.group_id, message_hash: params.message_hash,
                approval_commit,
                consumed_nullifiers: consumed.iter().map(|n| Nullifier::from_bytes(&n.to_repr()).expect("non-zero")).collect(),
            })));
        }
        _ => return Err(ContractError::InvalidFunction),
    };
    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = MultiSigFunction::try_from(update_data[0])?;
    match func {
        MultiSigFunction::CreateGroupV1 => {
            let u: CreateGroupUpdateV1 = deserialize(&update_data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            wasm::db::db_set(groups_db, &serialize(&u.group_id), &serialize(&MultiSigGroup {
                version: 1, group_id: u.group_id, pubkeys: u.pubkeys,
                threshold: u.threshold, total_keys: u.total_keys,
            }))?;
            Ok(())
        }
        MultiSigFunction::SignV1 => {
            let u: SignUpdateV1 = deserialize(&update_data[1..])?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            wasm::db::db_set(sigs_db, &serialize(&u.nullifier), &serialize(&PartialSignature {
                group_id: u.group_id, message_hash: u.message_hash, nullifier: u.nullifier,
            }))?;
            let nf_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nf_db, &serialize(&u.nullifier), &[])?;
            Ok(())
        }
        MultiSigFunction::FinalizeV1 => {
            let u: FinalizeUpdateV1 = deserialize(&update_data[1..])?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            for nf in &u.consumed_nullifiers {
                if let Some(d) = wasm::db::db_get(sigs_db, &serialize(nf))? {
                    let mut sig: PartialSignature = deserialize(&d)?;
                    sig.nullifier = Nullifier::ZERO;
                    wasm::db::db_set(sigs_db, &serialize(nf), &serialize(&sig))?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
