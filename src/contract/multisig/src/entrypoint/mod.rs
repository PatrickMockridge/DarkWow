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
    error::MultiSigError,
    model::{
        CreateGroupParamsV1, CreateGroupUpdateV1, FinalizeParamsV1, FinalizeUpdateV1,
        MultiSigGroup, PartialSignature, SignParamsV1, SignUpdateV1,
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
    let create_group_bin = include_bytes!("../../proof/create_group_v1.zk.bin");
    let sign_bin = include_bytes!("../../proof/sign_v1.zk.bin");
    let finalize_bin = include_bytes!("../../proof/finalize_v1.zk.bin");
    wasm::db::zkas_db_set(&create_group_bin[..])?;
    wasm::db::zkas_db_set(&sign_bin[..])?;
    wasm::db::zkas_db_set(&finalize_bin[..])?;

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

    let metadata = match func {
        MultiSigFunction::CreateGroupV1 => {
            let params: CreateGroupParamsV1 = deserialize(&self_.data[1..])?;
            create_group_get_metadata_v1(params)?
        }
        MultiSigFunction::SignV1 => {
            let params: SignParamsV1 = deserialize(&self_.data[1..])?;
            sign_get_metadata_v1(params)?
        }
        MultiSigFunction::FinalizeV1 => {
            let params: FinalizeParamsV1 = deserialize(&self_.data[1..])?;
            finalize_get_metadata_v1(params)?
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn create_group_get_metadata_v1(params: CreateGroupParamsV1) -> Result<Vec<u8>, ContractError> {
    let group_id = MultiSigGroup::compute_group_id(
        &params.pubkeys.iter().map(|b| pallas::Point::from_bytes(b).unwrap_or_default()).collect::<Vec<_>>(),
        params.threshold,
    );
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V1.to_string(), vec![group_id, pallas::Base::from(params.threshold as u64), pallas::Base::from(params.pubkeys.len() as u64)]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn sign_get_metadata_v1(params: SignParamsV1) -> Result<Vec<u8>, ContractError> {
    let message_hash = pallas::Base::from_raw(
        <[u8; 32]>::try_from(&blake3::hash(&params.message).as_bytes()[..32]).unwrap_or_default(),
    );
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((MULTISIG_CONTRACT_ZKAS_SIGN_NS_V1.to_string(), vec![params.group_id, message_hash]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn finalize_get_metadata_v1(params: FinalizeParamsV1) -> Result<Vec<u8>, ContractError> {
    let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_inputs.push((MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V1.to_string(), vec![params.group_id, params.message_hash]));
    let mut metadata = vec![];
    zk_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = MultiSigFunction::try_from(self_.data.data[0])?;

    match func {
        MultiSigFunction::CreateGroupV1 => {
            let params: CreateGroupParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[multisig::create_group_v1] Creating group with {} keys, threshold {}", params.pubkeys.len(), params.threshold);

            if params.pubkeys.is_empty() {
                return Err(MultiSigError::EmptyKeyList.into());
            }
            if params.threshold == 0 || params.threshold as usize > params.pubkeys.len() {
                return Err(MultiSigError::InvalidThreshold.into());
            }

            let pubkeys: Vec<pallas::Point> = params.pubkeys.iter()
                .map(|b| pallas::Point::from_bytes(b).unwrap_or_default())
                .collect();
            let group_id = MultiSigGroup::compute_group_id(&pubkeys, params.threshold);

            // Check group doesn't already exist
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if wasm::db::db_contains_key(groups_db, &serialize(&group_id))? {
                return Err(MultiSigError::GroupAlreadyExists.into());
            }

            let update = CreateGroupUpdateV1 {
                group_id,
                pubkeys,
                threshold: params.threshold,
                total_keys: params.pubkeys.len() as u8,
            };
            let _ = wasm::util::set_return_data(&serialize(&(MultiSigFunction::CreateGroupV1 as u8, update)));
        }
        MultiSigFunction::SignV1 => {
            let params: SignParamsV1 = deserialize(&self_.data.data[1..])?;
            let message_hash = pallas::Base::from_raw(
                <[u8; 32]>::try_from(&blake3::hash(&params.message).as_bytes()[..32]).unwrap_or_default(),
            );
            msg!("[multisig::sign_v1] Signing message {:?} for group {:?}", message_hash, params.group_id);

            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if !wasm::db::db_contains_key(groups_db, &serialize(&params.group_id))? {
                return Err(MultiSigError::GroupNotFound.into());
            }

            let signer_pubkey = pallas::Point::from_bytes(&params.proof[..32]).unwrap_or_default();
            let nullifier = pallas::Base::from_raw(
                <[u8; 32]>::try_from(
                    &pallas::crypto::hash::poseidon_hash(&[params.group_id, message_hash, signer_pubkey.get_x(), signer_pubkey.get_y()])
                    .as_bytes()[..32]
                ).unwrap_or_default(),
            );

            // Check no duplicate partial signature
            let nullifiers_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier))? {
                return Err(MultiSigError::DuplicateNullifier.into());
            }

            let update = SignUpdateV1 {
                group_id: params.group_id,
                message_hash,
                signer_pubkey,
                nullifier,
            };
            let _ = wasm::util::set_return_data(&serialize(&(MultiSigFunction::SignV1 as u8, update)));
        }
        MultiSigFunction::FinalizeV1 => {
            let params: FinalizeParamsV1 = deserialize(&self_.data.data[1..])?;
            msg!("[multisig::finalize_v1] Finalizing message {:?} for group {:?}", params.message_hash, params.group_id);

            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let group_data = wasm::db::db_get(groups_db, &serialize(&params.group_id))?
                .ok_or(MultiSigError::GroupNotFound)?;
            let group: MultiSigGroup = deserialize(&group_data)?;

            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;

            // Count partial signatures for this group + message
            let mut consumed_nullifiers: Vec<pallas::Base> = Vec::new();
            for pk in &group.pubkeys {
                let nullifier = pallas::Base::from_raw(
                    <[u8; 32]>::try_from(
                        &pallas::crypto::hash::poseidon_hash(&[params.group_id, params.message_hash, pk.get_x(), pk.get_y()])
                        .as_bytes()[..32]
                    ).unwrap_or_default(),
                );
                if wasm::db::db_contains_key(sigs_db, &serialize(&nullifier))? {
                    consumed_nullifiers.push(nullifier);
                }
            }

            if consumed_nullifiers.len() < group.threshold as usize {
                return Err(MultiSigError::InsufficientSignatures.into());
            }

            let approval_commit = pallas::crypto::hash::poseidon_hash(&[
                params.group_id, params.message_hash,
            ]);

            let update = FinalizeUpdateV1 {
                group_id: params.group_id,
                message_hash: params.message_hash,
                approval_commit,
                consumed_nullifiers,
            };
            let _ = wasm::util::set_return_data(&serialize(&(MultiSigFunction::FinalizeV1 as u8, update)));
        }
        _ => return Err(ContractError::InvalidFunction),
    };

    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = MultiSigFunction::try_from(update_data[0])?;
    match func {
        MultiSigFunction::CreateGroupV1 => {
            let update: CreateGroupUpdateV1 = deserialize(&update_data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let group = MultiSigGroup {
                group_id: update.group_id,
                pubkeys: update.pubkeys,
                threshold: update.threshold,
                total_keys: update.total_keys,
            };
            wasm::db::db_set(groups_db, &serialize(&update.group_id), &serialize(&group))?;
            Ok(())
        }
        MultiSigFunction::SignV1 => {
            let update: SignUpdateV1 = deserialize(&update_data[1..])?;
            let sig = PartialSignature {
                group_id: update.group_id,
                message_hash: update.message_hash,
                signer_pubkey: update.signer_pubkey,
                nullifier: update.nullifier,
            };
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            wasm::db::db_set(sigs_db, &serialize(&update.nullifier), &serialize(&sig))?;
            let nullifiers_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;
            Ok(())
        }
        MultiSigFunction::FinalizeV1 => {
            let update: FinalizeUpdateV1 = deserialize(&update_data[1..])?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            for nullifier in &update.consumed_nullifiers {
                // Mark each partial signature as consumed
                if let Some(data) = wasm::db::db_get(sigs_db, &serialize(nullifier))? {
                    let mut sig: PartialSignature = deserialize(&data)?;
                    // Overwrite with consumed marker
                    sig.nullifier = pallas::Base::zero();
                    wasm::db::db_set(sigs_db, &serialize(nullifier), &serialize(&sig))?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
