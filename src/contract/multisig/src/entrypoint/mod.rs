use dwow_sdk::{
    blockchain::SerializedLen,
    // `PublicKey` is still needed: `get_metadata` encodes an empty `Vec<PublicKey>` as the
    // Schnorr-signature prohibition stub (contract-standards.md §3). No key is ever put in it.
    crypto::{pasta_prelude::PrimeField, ContractId, Nullifier, PublicKey},
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
            // Deduplicated the same way `process_instruction` deduplicates, so the `group_id`
            // pushed here is the one that will actually be stored. Before this the two sides could
            // disagree — this arm hashed the *raw* count while exec hashed the deduplicated one —
            // and nothing caught it, because `group_id` is a bare witness in `create_group.zk` that
            // the circuit relates to nothing.
            let mut commitments: Vec<pallas::Base> = Vec::with_capacity(params.member_commitments.len());
            for c in &params.member_commitments {
                if !commitments.contains(c) {
                    commitments.push(*c);
                }
            }
            let first = *commitments.first().ok_or_else(|| {
                ContractError::IoError("CreateGroupParamsV1: member_commitments is empty".to_string())
            })?;
            let total_keys = u8::try_from(commitments.len()).map_err(|_| {
                ContractError::IoError("CreateGroupParamsV1: too many members for total_keys".to_string())
            })?;
            let n = pallas::Base::from(total_keys as u64);
            let group_id = MultiSigGroup::derive_group_id(first, params.threshold, total_keys).inner();
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
                params.group_id.inner(), params.message_hash,
                params.member_commitment, params.nullifier,
                params.tx_binding, params.tx_nonce,
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
                params.group_id.inner(), params.message_hash, params.approval_commit,
                params.tx_binding, params.tx_nonce,
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

fn encode_create_group_update_v1(update: &CreateGroupUpdateV1) -> Result<Vec<u8>, ContractError> {
    let n = SerializedLen::try_from_len(update.member_commitments.len())?;
    let mut buf = Vec::with_capacity(39 + update.member_commitments.len() * 32);
    buf.push(MultiSigFunction::CreateGroupV1 as u8);
    buf.extend_from_slice(&update.group_id.to_bytes());
    buf.extend_from_slice(&n.to_le_bytes());
    for c in &update.member_commitments {
        buf.extend_from_slice(&c.to_repr());
    }
    buf.push(update.threshold);
    buf.push(update.total_keys);
    Ok(buf)
}

fn decode_create_group_update_v1(data: &[u8]) -> Result<CreateGroupUpdateV1, ContractError> {
    if data.len() < 38 {
        return Err(ContractError::IoError(format!(
            "CreateGroupUpdateV1: expected at least 38 bytes, got {}", data.len()
        )));
    }
    let group_id = GroupId::from_bytes(&read_field::<32>(data, 0)?)
        .ok_or_else(|| ContractError::IoError("CreateGroupUpdateV1: invalid GroupId".into()))?;
    let count = SerializedLen::from_le_bytes(read_field::<4>(data, 32)?).to_usize();
    let members_end = 36 + count * 32;
    if data.len() < members_end + 2 {
        return Err(ContractError::IoError(format!(
            "CreateGroupUpdateV1: expected {} bytes for {} member commitments, got {}",
            members_end + 2, count, data.len()
        )));
    }
    let mut member_commitments = Vec::with_capacity(count);
    for i in 0..count {
        let start = 36 + i * 32;
        let c = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, start)?))
            .ok_or_else(|| ContractError::IoError(format!(
                "CreateGroupUpdateV1: invalid member commitment[{}]", i
            )))?;
        member_commitments.push(c);
    }
    let threshold = read_byte(data, members_end)?;
    let total_keys = read_byte(data, members_end + 1)?;
    Ok(CreateGroupUpdateV1 { group_id, member_commitments, threshold, total_keys })
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

fn encode_finalize_update_v1(update: &FinalizeUpdateV1) -> Result<Vec<u8>, ContractError> {
    let n = SerializedLen::try_from_len(update.consumed_nullifiers.len())?;
    let mut buf = Vec::with_capacity(101 + update.consumed_nullifiers.len() * 32); // 1 + 96 + 4 + N*32
    buf.push(MultiSigFunction::FinalizeV1 as u8);
    buf.extend_from_slice(&update.group_id.to_bytes());
    buf.extend_from_slice(&update.message_hash.to_repr());
    buf.extend_from_slice(&update.approval_commit.to_repr());
    buf.extend_from_slice(&n.to_le_bytes());
    for nf in &update.consumed_nullifiers {
        buf.extend_from_slice(&nf.to_bytes());
    }
    Ok(buf)
}

fn decode_finalize_update_v1(data: &[u8]) -> Result<FinalizeUpdateV1, ContractError> {
    if data.len() < 100 {
        return Err(ContractError::IoError(format!(
            "FinalizeUpdateV1: expected at least 100 bytes, got {}", data.len()
        )));
    }
    let group_id = GroupId::from_bytes(&read_field::<32>(data, 0)?)
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid GroupId".into()))?;
    let message_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 32)?))
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid message_hash".into()))?;
    let approval_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(read_field::<32>(data, 64)?))
        .ok_or_else(|| ContractError::IoError("FinalizeUpdateV1: invalid approval_commit".into()))?;
    let nf_count = SerializedLen::from_le_bytes(read_field::<4>(data, 96)?).to_usize();
    let expected = 100 + nf_count * 32;
    if data.len() != expected {
        return Err(ContractError::IoError(format!(
            "FinalizeUpdateV1: expected {} bytes for {} nullifiers, got {}", expected, nf_count, data.len()
        )));
    }
    let mut consumed_nullifiers = Vec::with_capacity(nf_count);
    for i in 0..nf_count {
        let start = 100 + i * 32;
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
            if params.member_commitments.is_empty() { return Err(MultiSigError::EmptyMemberList.into()); }
            if params.threshold == 0 || params.threshold as usize > params.member_commitments.len() {
                return Err(MultiSigError::InvalidThreshold.into());
            }
            // Deduplicate member commitments — a duplicate would let one member count twice
            // toward the threshold. The nullifier dedup in SignV1 catches the same thing from the
            // other side (one secret produces one nullifier per message), but dedup at creation
            // time is defense-in-depth.
            let mut member_commitments = Vec::with_capacity(params.member_commitments.len());
            for c in &params.member_commitments {
                if !member_commitments.contains(c) {
                    member_commitments.push(*c);
                }
            }
            // Re-validate threshold against deduplicated count
            if params.threshold as usize > member_commitments.len() {
                return Err(MultiSigError::InvalidThreshold.into());
            }
            // `total_keys` is a `u8` field, so the count has to fit: `try_from`
            // with an error path rather than a truncating `as`
            // (contract-wasm-type-system.md §A.4.5). A member count the field cannot
            // represent also makes the threshold meaningless, so it is the same
            // invariant violation.
            let total_keys = u8::try_from(member_commitments.len())
                .map_err(|_| MultiSigError::InvalidThreshold)?;
            let group_id = MultiSigGroup::derive_group_id(
                *member_commitments.first().ok_or(MultiSigError::EmptyMemberList)?,
                params.threshold,
                total_keys,
            );
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            if wasm::db::db_contains_key(groups_db, &group_id.to_bytes())? {
                return Err(MultiSigError::GroupAlreadyExists.into());
            }
            wasm::util::set_return_data(&encode_create_group_update_v1(&CreateGroupUpdateV1 {
                group_id, member_commitments, threshold: params.threshold, total_keys,
            })?)?;
        }
        MultiSigFunction::SignV1 => {
            let params = SignParamsV1::decode(payload)?;
            let groups_db = wasm::db::db_lookup(cid, MULTISIG_CONTRACT_GROUPS_TREE)?;
            let data = wasm::db::db_get(groups_db, &params.group_id.to_bytes())?
                .ok_or(MultiSigError::GroupNotFound)?;
            let group = MultiSigGroup::decode(&data)?;
            // The signer is bound to the proof: `params.member_commitment` is an instance the
            // circuit derives from `signer_secret`, so a caller who names a commitment they cannot
            // open cannot produce a proof over it. This check is what OBL-Z11 was missing — it
            // used to compare a caller-typed *public key* against the group, which anyone could
            // copy out of the group record.
            if !group.member_commitments.contains(&params.member_commitment) {
                msg!("[multisig::SignV1] Error: signer is not a member of the group");
                return Err(MultiSigError::NotAMember.into());
            }
            // `params.nullifier` is proof-bound for the same reason, and bound in-circuit to
            // (signer_secret, group_id, message_hash) — so one member's signature cannot be lifted
            // onto another message, and two members signing the same message stay distinct.
            let nullifier = Nullifier::from_bytes(params.nullifier.to_repr()).map_err(|e| {
                ContractError::IoError(format!("SignV1: invalid nullifier: {e}"))
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
            // The approvals are named by the caller rather than recomputed from the group's keys,
            // because a member's nullifier is derived from their secret and the host cannot derive
            // it. That makes every entry a claim to check, not a fact: each must name a signature
            // record that exists, and that record must be for *this* group and *this* message.
            // A repeat of a nullifier already counted is one approval, not two.
            let mut consumed: Vec<Nullifier> = Vec::new();
            for nf in &params.approvals {
                if consumed.contains(nf) {
                    continue;
                }
                let record = match wasm::db::db_get(sigs_db, &nf.to_bytes())? {
                    Some(r) => r,
                    None => {
                        // Each of these three paths returns the same error code, so each states its
                        // own cause: without this a rejection says only "InsufficientSignatures"
                        // and a reader cannot tell a forged approval from a short one.
                        msg!("[multisig::FinalizeV1] Error: approval names no signature record");
                        return Err(MultiSigError::InsufficientSignatures.into());
                    }
                };
                let sig = PartialSignature::decode(&record)?;
                if sig.group_id != params.group_id || sig.message_hash != params.message_hash {
                    msg!("[multisig::FinalizeV1] Error: approval is a signature for a different group or message");
                    return Err(MultiSigError::InsufficientSignatures.into());
                }
                consumed.push(*nf);
            }
            if consumed.len() < group.threshold as usize {
                msg!(
                    "[multisig::FinalizeV1] Error: {} distinct approval(s), threshold is {}",
                    consumed.len(), group.threshold
                );
                return Err(MultiSigError::InsufficientSignatures.into());
            }
            wasm::util::set_return_data(&encode_finalize_update_v1(&FinalizeUpdateV1 {
                group_id: params.group_id, message_hash: params.message_hash,
                approval_commit: params.approval_commit,
                consumed_nullifiers: consumed,
            })?)?;
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
                version: 1, group_id: u.group_id, member_commitments: u.member_commitments,
                threshold: u.threshold, total_keys: u.total_keys,
            };
            wasm::db::db_set(groups_db, &u.group_id.to_bytes(), &group.encode()?)?;
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
