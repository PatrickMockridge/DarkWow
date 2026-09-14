use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, Nullifier, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, wasm,
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::MultiSigError,
    model::{
        read_byte, read_field,
        CreateGroupParamsV1, CreateGroupUpdateV1, FinalizeParamsV1, FinalizeUpdateV1,
        GroupId, MultiSigGroup, PartialSignature, SignParamsV1, SignUpdateV1,
    },
    MultiSigFunction,
    MULTISIG_CONTRACT_GROUPS_TREE,
    MULTISIG_CONTRACT_NULLIFIERS_TREE, MULTISIG_CONTRACT_SIGNATURES_TREE,
    MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V2, MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V2,
    MULTISIG_CONTRACT_ZKAS_SIGN_NS_V2,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[multisig::init_contract] Initializing MultiSig contract");

    // Register V2 circuits (domain separation, HAZOP RC3)
    wasm::db::zkas_db_set(include_bytes!("../../proof/create_group.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/sign.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../../proof/finalize.zk.bin"))?;

    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
    }
    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
    }
    if wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
    }
    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    // `call_idx` is host-supplied and the call data attacker-supplied, so the lookup and the
    // selector byte are read rather than indexed-into.
    let self_ = &calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?.data;
    let func = MultiSigFunction::try_from(*self_.data.first().ok_or_else(|| {
        ContractError::IoError("empty call data: no selector byte".to_string())
    })?)?;
    let payload = self_.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;
    let metadata: Vec<u8> = match func {
        MultiSigFunction::CreateGroupV1 => {
            let params = match CreateGroupParamsV1::decode(payload) {
                Ok(p) => p, Err(e) => { msg!("[multisig::get_metadata] Error: Failed to deserialize CreateGroupParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            let t = pallas::Base::from(params.threshold as u64);
            let n = pallas::Base::from(params.pubkeys.len() as u64);
            // Both of these were reachable panics on attacker-supplied params: `pubkeys` may be
            // empty (`pk_count` comes off the wire and may be 0), and a decoded `PublicKey` may be
            // the identity point, because the derived `Decodable` builds the point directly rather
            // than going through `from_bytes` (sdk/src/crypto/keypair.rs
            // `decoded_public_key_can_be_the_identity`).
            let first_pk = params.pubkeys.first().ok_or_else(|| {
                ContractError::IoError("CreateGroupParamsV1: pubkeys is empty".to_string())
            })?;
            let Some((fx, fy)) = first_pk.xy() else {
                return Err(ContractError::IoError(
                    "CreateGroupParamsV1: first pubkey is the identity point".to_string(),
                ))
            };
            let group_id = poseidon_hash([fx, fy, t, n]);
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V2.to_string(), vec![
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
            let params = SignParamsV1::decode(payload)?;
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_SIGN_NS_V2.to_string(), vec![
                params.tx_binding, params.tx_nonce, params.group_id.inner(), params.message_hash,
            ]));
            let sigs: Vec<PublicKey> = vec![];
            let mut meta = vec![];
            zk_inputs.encode(&mut meta)?;
            sigs.encode(&mut meta)?;
            meta
        }
        MultiSigFunction::FinalizeV1 => {
            let params = match FinalizeParamsV1::decode(payload) {
                Ok(p) => p, Err(e) => { msg!("[multisig::get_metadata] Error: Failed to deserialize FinalizeParamsV1: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            let mut zk_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_inputs.push((MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V2.to_string(), vec![
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
    let group_id = GroupId::from_bytes(&read_field::<32>(data, 0)?)
        .ok_or_else(|| ContractError::IoError("CreateGroupUpdateV1: invalid GroupId".into()))?;
    let pk_count = read_byte(data, 32)? as usize;
    let pk_end = 33 + pk_count * 32;
    if data.len() < pk_end + 2 {
        return Err(ContractError::IoError(format!(
            "CreateGroupUpdateV1: expected {} bytes for {} pubkeys, got {}", pk_end + 2, pk_count, data.len()
        )));
    }
    let mut pubkeys = Vec::with_capacity(pk_count);
    for i in 0..pk_count {
        let start = 33 + i * 32;
        let pk = PublicKey::from_bytes(read_field::<32>(data, start)?)
            .map_err(|e| ContractError::IoError(format!("CreateGroupUpdateV1: invalid PublicKey[{}]: {e}", i)))?;
        pubkeys.push(pk);
    }
    let threshold = read_byte(data, pk_end)?;
    let total_keys = read_byte(data, pk_end + 1)?;
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
    let group_id = GroupId::from_bytes(&read_field::<32>(data, 0)?)
        .ok_or_else(|| ContractError::IoError("SignUpdateV1: invalid GroupId".into()))?;
    let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?))
        .ok_or_else(|| ContractError::IoError("SignUpdateV1: invalid message_hash".into()))?;
    let nullifier = Nullifier::from_bytes(read_field::<32>(data, 64)?)
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
    let group_id = GroupId::from_bytes(&read_field::<32>(data, 0)?)
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid GroupId".into()))?;
    let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?))
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid message_hash".into()))?;
    let approval_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 64)?))
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid approval_commit".into()))?;
    let nf_count = read_byte(data, 96)? as usize;
    let expected = 97 + nf_count * 32;
    if data.len() != expected {
        return Err(ContractError::IoError(format!(
            "FinalizeUpdateV1: expected {} bytes for {} nullifiers, got {}", expected, nf_count, data.len()
        )));
    }
    let mut consumed_nullifiers = Vec::with_capacity(nf_count);
    for i in 0..nf_count {
        let start = 97 + i * 32;
        let nf = Nullifier::from_bytes(read_field::<32>(data, start)?)
            .map_err(|e| ContractError::IoError(format!("FinalizeUpdateV1: invalid Nullifier[{}]: {e}", i)))?;
        consumed_nullifiers.push(nf);
    }
    Ok(FinalizeUpdateV1 { group_id, message_hash, approval_commit, consumed_nullifiers })
}

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    // `call_idx` is host-supplied and the call data attacker-supplied: read, never index.
    let self_ = calls.get(call_idx).ok_or_else(|| {
        ContractError::IoError(format!("call_index {call_idx} out of range ({} calls)", calls.len()))
    })?;
    let func = MultiSigFunction::try_from(*self_.data.data.first().ok_or_else(|| {
        ContractError::IoError("empty call data: no selector byte".to_string())
    })?)?;
    let payload = self_.data.data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty call data: no payload after selector".to_string())
    })?;

    match func {
        MultiSigFunction::CreateGroupV1 => {
            let params = CreateGroupParamsV1::decode(payload)?;
            if params.pubkeys.is_empty() { return Err(MultiSigError::EmptyKeyList.into()); }
            if params.threshold == 0 || params.threshold as usize > params.pubkeys.len() {
                return Err(MultiSigError::InvalidThreshold.into());
            }
            // Deduplicate public keys — duplicate keys would let a single
            // signer count multiple times toward the threshold, bypassing the
            // multisig security model. Nullifier-based signature tracking in
            // FinalizeV1 provides partial protection (duplicate keys produce
            // identical nullifiers which SignV1 rejects), but dedup at creation
            // time is defense-in-depth.
            let mut pubkeys = Vec::with_capacity(params.pubkeys.len());
            for b in &params.pubkeys {
                if !pubkeys.contains(b) {
                    pubkeys.push(*b);
                }
            }
            // Re-validate threshold against deduplicated count
            if params.threshold as usize > pubkeys.len() {
                return Err(MultiSigError::InvalidThreshold.into());
            }
            let group_id = MultiSigGroup::derive_group_id(
                pubkeys.first().ok_or(MultiSigError::EmptyKeyList)?,
                params.threshold,
                pubkeys.len() as u8,
            )?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if wasm::db::db_contains_key(groups_db, &group_id.to_bytes())? {
                return Err(MultiSigError::GroupAlreadyExists.into());
            }
            wasm::util::set_return_data(&encode_create_group_update_v1(&CreateGroupUpdateV1 {
                group_id, pubkeys, threshold: params.threshold, total_keys: params.pubkeys.len() as u8,
            }))?;
        }
        MultiSigFunction::SignV1 => {
            let params = SignParamsV1::decode(payload)?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if !wasm::db::db_contains_key(groups_db, &params.group_id.to_bytes())? {
                return Err(MultiSigError::GroupNotFound.into());
            }
            // HAZOP H-4 fix: verify signer is a group member
            let data = wasm::db::db_get(groups_db, &params.group_id.to_bytes())?
                .ok_or(MultiSigError::GroupNotFound)?;
            let group = MultiSigGroup::decode(&data)?;
            if !group.pubkeys.iter().any(|pk| pk == &params.signer_pub) {
                msg!("[multisig::SignV1] Error: signer is not a member of the group");
                return Err(MultiSigError::KeyNotInGroup.into());
            }
            // Nullifier binds signer pubkey to prevent collision across signers
            // Must match FinalizeV1 lookup: poseidon_hash([group_id, msg_hash, pk_x, pk_y])
            //
            // The `#[expect]` here read "PublicKey constructor rejects identity"; what actually
            // holds is the membership check above — `signer_pub` equals one of `group.pubkeys`,
            // and those were built by `CreateGroupParamsV1::decode`, which uses
            // `PublicKey::from_bytes`. The constructor is not the reason, and this is a typed
            // error now so that neither reason has to be relied on.
            let Some((pk_x, pk_y)) = params.signer_pub.xy() else {
                return Err(ContractError::IoError(
                    "SignV1: signer public key is the identity point".to_string(),
                ))
            };
            let nf_base = poseidon_hash([params.group_id.inner(), params.message_hash, pk_x, pk_y]);
            let nullifier = Nullifier::from_bytes(nf_base.to_repr()).map_err(|e| {
                ContractError::IoError(format!("SignV1: nullifier from poseidon output: {e}"))
            })?;
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
            let params = FinalizeParamsV1::decode(payload)?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let data = wasm::db::db_get(groups_db, &params.group_id.to_bytes())?
                .ok_or(MultiSigError::GroupNotFound)?;
            let group = MultiSigGroup::decode(&data)?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            let mut consumed: Vec<Nullifier> = Vec::new();
            for pk in &group.pubkeys {
                // As in SignV1, the invariant is `CreateGroupParamsV1::decode`'s use of
                // `PublicKey::from_bytes`, not the constructor — and neither is relied on now.
                let Some((x, y)) = pk.xy() else {
                    return Err(ContractError::IoError(
                        "FinalizeV1: group contains the identity point".to_string(),
                    ))
                };
                let nf = poseidon_hash([params.group_id.inner(), params.message_hash, x, y]);
                if wasm::db::db_contains_key(sigs_db, &nf.to_repr())? {
                    let consumed_nf = Nullifier::from_bytes(nf.to_repr()).map_err(|e| {
                        ContractError::IoError(format!("FinalizeV1: nullifier from poseidon output: {e}"))
                    })?;
                    consumed.push(consumed_nf);
                }
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
    if update_data.is_empty() {
        msg!("[multisig::process_update] EMPTY");
        return Err(ContractError::Custom(254))
    }
    let update_func = *update_data.first().ok_or_else(|| {
        ContractError::IoError("empty update data: no selector byte".to_string())
    })?;
    let update_payload = update_data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty update data: no payload after selector".to_string())
    })?;
    let func = match MultiSigFunction::try_from(update_func) {
        Ok(f) => f,
        Err(_) => { msg!("[multisig::process_update] BAD 0x{:02x} len={}", update_func, update_data.len()); return Err(ContractError::InvalidFunction.into()) }
    };
    match func {
        MultiSigFunction::CreateGroupV1 => {
            let u = decode_create_group_update_v1(update_payload)?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let group = MultiSigGroup {
                version: 1, group_id: u.group_id, pubkeys: u.pubkeys,
                threshold: u.threshold, total_keys: u.total_keys,
            };
            wasm::db::db_set(groups_db, &u.group_id.to_bytes(), &group.encode())?;
            Ok(())
        }
        MultiSigFunction::SignV1 => {
            let u = decode_sign_update_v1(update_payload)?;
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            let sig = PartialSignature {
                group_id: u.group_id, message_hash: u.message_hash, nullifier: u.nullifier,
            };
            wasm::db::db_set(sigs_db, &u.nullifier.to_bytes(), &sig.encode())?;
            let nf_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_mark_spent(nf_db, &u.nullifier.to_bytes())?;
            Ok(())
        }
        MultiSigFunction::FinalizeV1 => {
            let u = decode_finalize_update_v1(update_payload)?;
            // HAZOP H-5 fix: delete consumed signatures (previously zeroed value
            // but kept key — db_contains_key still returned true, enabling replay).
            let sigs_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_SIGNATURES_TREE)?;
            for nf in &u.consumed_nullifiers {
                wasm::db::db_del(sigs_db, &nf.to_bytes())?;
            }
            Ok(())
        }
        MultiSigFunction::InitializeV1 => {
            msg!("[multisig::process_update] Error: InitializeV1 must be called via init");
            Err(ContractError::InvalidFunction)
        },
    }
}
