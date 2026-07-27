use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, Nullifier, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::{group::GroupEncoding, pallas},
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
            let params = match CreateGroupParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[multisig::get_metadata] Error: Failed to deserialize CreateGroupParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            let t = pallas::Base::from(params.threshold as u64);
            let n = pallas::Base::from(params.pubkeys.len() as u64);
            let first_pk = params.pubkeys[0];
            let (fx, fy) = first_pk.xy().expect("pk not identity");
            let group_id = poseidon_hash([fx, fy, t, n]);
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V1.to_string(), vec![
                params.tx_binding, params.tx_nonce, group_id, t, n,
            ]));
            // Schnorr signatures prohibited (contract-standards.md §3). Member keys are in ZK public inputs.
            let sigs: Vec<PublicKey> = vec![];
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            sigs.encode(&mut meta)?;
            meta
        }
        MultiSigFunction::SignV1 => {
            let params = match SignParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[multisig::get_metadata] Error: Failed to deserialize SignParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_SIGN_NS_V1.to_string(), vec![
                params.tx_binding, params.tx_nonce, params.group_id.inner(), params.message_hash,
            ]));
            let sigs: Vec<PublicKey> = vec![];
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            sigs.encode(&mut meta)?;
            meta
        }
        MultiSigFunction::FinalizeV1 => {
            let params = match FinalizeParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[multisig::get_metadata] Error: Failed to deserialize FinalizeParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V1.to_string(), vec![
                params.tx_binding, params.tx_nonce, params.group_id.inner(), params.message_hash,
            ]));
            let sigs: Vec<PublicKey> = vec![];
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            sigs.encode(&mut meta)?;
            meta
        }
        MultiSigFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

// --- Rho-calculus explicit encode/decode for update structs ---
// Per type-system.md §2.2: bytes round-trip across module boundaries is forbidden.
// Per §10.5: re-lift validation SHALL use named constructors (from_bytes).

fn encode_create_group_update_v1(update: &CreateGroupUpdateV1) -> Vec<u8> {
    let mut buf = Vec::with_capacity(37 + update.pubkeys.len() * 32);
    buf.push(MultiSigFunction::CreateGroupV1 as u8);
    buf.extend_from_slice(&update.group_id.to_bytes());
    buf.push(update.pubkeys.len() as u8); // u8 prefix — max 255 members
    for pk in &update.pubkeys {
        buf.extend_from_slice(&pk.to_bytes());
    }
    buf.push(update.threshold);
    buf.push(update.total_keys);
    buf
}

fn decode_create_group_update_v1(data: &[u8]) -> Result<CreateGroupUpdateV1, ContractError> {
    if data.len() < 35 {
        return Err(ContractError::IoError(format!(
            "CreateGroupUpdateV1: expected at least 35 bytes, got {}", data.len()
        )));
    }
    let group_id = GroupId::from_bytes(&data[0..32].try_into().unwrap())
        .ok_or_else(|| ContractError::IoError("CreateGroupUpdateV1: invalid GroupId".into()))?;
    let pk_count = data[32] as usize;
    let pk_end = 33 + pk_count * 32;
    if data.len() < pk_end + 2 {
        return Err(ContractError::IoError(format!(
            "CreateGroupUpdateV1: expected {} bytes for {} pubkeys, got {}", pk_end + 2, pk_count, data.len()
        )));
    }
    let mut pubkeys = Vec::with_capacity(pk_count);
    for i in 0..pk_count {
        let start = 33 + i * 32;
        let pk = PublicKey::from_bytes(data[start..start+32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CreateGroupUpdateV1: invalid PublicKey[{}]: {e}", i)))?;
        pubkeys.push(pk);
    }
    let threshold = data[pk_end];
    let total_keys = data[pk_end + 1];
    Ok(CreateGroupUpdateV1 { group_id, pubkeys, threshold, total_keys })
}

fn encode_sign_update_v1(update: &SignUpdateV1) -> Vec<u8> {
    let mut buf = Vec::with_capacity(97); // 1 + 96
    buf.push(MultiSigFunction::SignV1 as u8);
    buf.extend_from_slice(&update.group_id.to_bytes());
    buf.extend_from_slice(&update.message_hash.to_repr());
    buf.extend_from_slice(&update.nullifier.to_bytes());
    buf
}

fn decode_sign_update_v1(data: &[u8]) -> Result<SignUpdateV1, ContractError> {
    const EXPECTED: usize = 96;
    if data.len() != EXPECTED {
        return Err(ContractError::IoError(format!(
            "SignUpdateV1: expected {} bytes, got {}", EXPECTED, data.len()
        )));
    }
    let group_id = GroupId::from_bytes(&data[0..32].try_into().unwrap())
        .ok_or_else(|| ContractError::IoError("SignUpdateV1: invalid GroupId".into()))?;
    let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
        .ok_or_else(|| ContractError::IoError("SignUpdateV1: invalid message_hash".into()))?;
    let nullifier = Nullifier::from_bytes(data[64..96].try_into().unwrap())
        .map_err(|e| ContractError::IoError(format!("SignUpdateV1: invalid Nullifier: {e}")))?;
    Ok(SignUpdateV1 { group_id, message_hash, nullifier })
}

fn encode_finalize_update_v1(update: &FinalizeUpdateV1) -> Vec<u8> {
    let nf_count = update.consumed_nullifiers.len();
    let mut buf = Vec::with_capacity(98 + nf_count * 32); // 1 + 96 + 1 + N*32
    buf.push(MultiSigFunction::FinalizeV1 as u8);
    buf.extend_from_slice(&update.group_id.to_bytes());
    buf.extend_from_slice(&update.message_hash.to_repr());
    buf.extend_from_slice(&update.approval_commit.to_repr());
    buf.push(nf_count as u8);
    for nf in &update.consumed_nullifiers {
        buf.extend_from_slice(&nf.to_bytes());
    }
    buf
}

fn decode_finalize_update_v1(data: &[u8]) -> Result<FinalizeUpdateV1, ContractError> {
    if data.len() < 97 {
        return Err(ContractError::IoError(format!(
            "FinalizeUpdateV1: expected at least 97 bytes, got {}", data.len()
        )));
    }
    let group_id = GroupId::from_bytes(&data[0..32].try_into().unwrap())
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid GroupId".into()))?;
    let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap()))
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid message_hash".into()))?;
    let approval_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap()))
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid approval_commit".into()))?;
    let nf_count = data[96] as usize;
    let expected = 97 + nf_count * 32;
    if data.len() != expected {
        return Err(ContractError::IoError(format!(
            "FinalizeUpdateV1: expected {} bytes for {} nullifiers, got {}", expected, nf_count, data.len()
        )));
    }
    let mut consumed_nullifiers = Vec::with_capacity(nf_count);
    for i in 0..nf_count {
        let start = 97 + i * 32;
        let nf = Nullifier::from_bytes(data[start..start+32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("FinalizeUpdateV1: invalid Nullifier[{}]: {e}", i)))?;
        consumed_nullifiers.push(nf);
    }
    Ok(FinalizeUpdateV1 { group_id, message_hash, approval_commit, consumed_nullifiers })
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
            for b in &params.pubkeys { pubkeys.push(*b); }
            let group_id = MultiSigGroup::derive_group_id(&pubkeys[0], params.threshold, pubkeys.len() as u8);
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if wasm::db::db_contains_key(groups_db, &group_id.to_bytes())? {
                return Err(MultiSigError::GroupAlreadyExists.into());
            }
            wasm::util::set_return_data(&encode_create_group_update_v1(&CreateGroupUpdateV1 {
                group_id, pubkeys, threshold: params.threshold, total_keys: params.pubkeys.len() as u8,
            }))?;
        }
        MultiSigFunction::SignV1 => {
            let params: SignParamsV1 = deserialize(&self_.data.data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if !wasm::db::db_contains_key(groups_db, &params.group_id.to_bytes())? {
                return Err(MultiSigError::GroupNotFound.into());
            }
            // Nullifier binds signer pubkey to prevent collision across signers
            // Must match FinalizeV1 lookup: poseidon_hash([group_id, msg_hash, pk_x, pk_y])
            let (pk_x, pk_y) = params.signer_pub.xy().expect("pk not identity");
            let nf_base = poseidon_hash([params.group_id.inner(), params.message_hash, pk_x, pk_y]);
            let nullifier = Nullifier::from_bytes(nf_base.to_repr()).expect("non-zero poseidon output");
            let nullifiers_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &nullifier.to_bytes())? {
                return Err(MultiSigError::DuplicateNullifier.into());
            }
            wasm::util::set_return_data(&encode_sign_update_v1(&SignUpdateV1 {
                group_id: params.group_id, message_hash: params.message_hash,
                nullifier,
            }))?;
        }
        MultiSigFunction::FinalizeV1 => {
            let params: FinalizeParamsV1 = deserialize(&self_.data.data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let data = wasm::db::db_get(groups_db, &params.group_id.to_bytes())?
                .ok_or(MultiSigError::GroupNotFound)?;
            let group = MultiSigGroup::decode(&data)?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            let mut consumed: Vec<Nullifier> = Vec::new();
            for pk in &group.pubkeys {
                // group.pubkeys are validated at creation, so xy() is always Some
                let (x, y) = pk.xy().expect("pk not identity");
                let nf = poseidon_hash([params.group_id.inner(), params.message_hash, x, y]);
                if wasm::db::db_contains_key(sigs_db, &nf.to_repr())? { consumed.push(Nullifier::from_bytes(nf.to_repr()).expect("non-zero")); }
            }
            if consumed.len() < group.threshold as usize {
                return Err(MultiSigError::InsufficientSignatures.into());
            }
            let approval_commit = poseidon_hash([params.group_id.inner(), params.message_hash]);
            wasm::util::set_return_data(&encode_finalize_update_v1(&FinalizeUpdateV1 {
                group_id: params.group_id, message_hash: params.message_hash,
                approval_commit,
                consumed_nullifiers: consumed,
            }))?;
        }
        MultiSigFunction::InitializeV1 => {
            msg!("[multisig::process_instruction] Error: InitializeV1 must be called via init");
            return Err(ContractError::InvalidFunction);
        }
    };
    Ok(())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = MultiSigFunction::try_from(update_data[0])?;
    match func {
        MultiSigFunction::CreateGroupV1 => {
            let u = decode_create_group_update_v1(&update_data[1..])?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let group = MultiSigGroup {
                version: 1, group_id: u.group_id, pubkeys: u.pubkeys,
                threshold: u.threshold, total_keys: u.total_keys,
            };
            wasm::db::db_set(groups_db, &u.group_id.to_bytes(), &group.encode())?;
            Ok(())
        }
        MultiSigFunction::SignV1 => {
            let u = decode_sign_update_v1(&update_data[1..])?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            let sig = PartialSignature {
                group_id: u.group_id, message_hash: u.message_hash, nullifier: u.nullifier,
            };
            wasm::db::db_set(sigs_db, &u.nullifier.to_bytes(), &sig.encode())?;
            let nf_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(nf_db, &u.nullifier.to_bytes(), &[])?;
            Ok(())
        }
        MultiSigFunction::FinalizeV1 => {
            let u = decode_finalize_update_v1(&update_data[1..])?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            for nf in &u.consumed_nullifiers {
                if let Some(d) = wasm::db::db_get(sigs_db, &nf.to_bytes())? {
                    let mut sig = PartialSignature::decode(&d)?;
                    sig.nullifier = Nullifier::ZERO;
                    wasm::db::db_set(sigs_db, &nf.to_bytes(), &sig.encode())?;
                }
            }
            Ok(())
        }
        MultiSigFunction::InitializeV1 => {
            msg!("[multisig::process_update] Error: InitializeV1 must be called via init");
            Err(ContractError::InvalidFunction)
        },
    }
}
