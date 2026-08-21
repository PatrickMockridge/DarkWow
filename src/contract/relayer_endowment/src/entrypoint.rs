/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Relayer Endowment Contract Entrypoint

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};
use dwow_serial::{deserialize, Encodable};
use pasta_curves::{arithmetic::CurveAffine, group::Curve};

use crate::error::RelayerEndowmentError;
use crate::model::*;
use crate::RelayerEndowmentFunction;
use crate::{
    RELAYER_ENDOWMENT_DEPLOYMENTS_TREE, RELAYER_ENDOWMENT_REGISTRY_TREE,
    RELAYER_ENDOWMENT_MIN_DEPLOY, RELAYER_ENDOWMENT_INFO_TREE,
    RELAYER_ENDOWMENT_PROMISSORY_NOTE_CONTRACT_ID,
    RELAYER_ENDOWMENT_ZKAS_INIT_NS_V2, RELAYER_ENDOWMENT_ZKAS_DEPLOY_CAPITAL_NS_V2,
    RELAYER_ENDOWMENT_ZKAS_CLAIM_FEES_NS_V2,
    RELAYER_ENDOWMENT_FORCE_SETTLEMENT_TIMEOUT,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Compute a Poseidon-hashed DB key from a relayer pubkey.
///
/// Raw pubkeys as DB keys make identity trivially enumerable on-chain.
/// Hashing through Poseidon preserves look-up capability (anyone who
/// knows the pubkey can recompute the key) but prevents casual
/// enumeration — the key reveals nothing without knowing the pubkey first.
pub(crate) fn compute_relayer_key(relayer_pub: &PublicKey) -> Vec<u8> {
    let pubkey_bytes = relayer_pub.to_bytes();
    let mut chunks = [0u64; 4];
    for i in 0..4 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&pubkey_bytes[i * 8..(i + 1) * 8]);
        chunks[i] = u64::from_le_bytes(bytes);
    }
    let hash = poseidon_hash([
        pallas::Base::from(chunks[0]),
        pallas::Base::from(chunks[1]),
        pallas::Base::from(chunks[2]),
        pallas::Base::from(chunks[3]),
    ]);
    hash.to_repr().to_vec()
}

// ============================================================================
// INITIALIZATION
// ============================================================================

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize INFO_TREE with redeployment guard
    let info_db = match wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, RELAYER_ENDOWMENT_INFO_TREE)?,
    };
    wasm::db::db_set(info_db, RELAYER_ENDOWMENT_PROMISSORY_NOTE_CONTRACT_ID, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;

    // Initialize database trees with redeployment guards
    if wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE).is_err() {
        wasm::db::db_init(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    }
    if wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE).is_err() {
        wasm::db::db_init(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    }
    #[cfg(feature = "relayer")]
    if wasm::db::db_lookup(cid, crate::relayer::RELAYER_ENDOWMENT_RELAYERS_TREE).is_err() {
        wasm::db::db_init(cid, crate::relayer::RELAYER_ENDOWMENT_RELAYERS_TREE)?;
    }

    let claim_fees_v1_bincode = include_bytes!("../proof/claim_fees.zk.bin");
    wasm::db::zkas_db_set(&claim_fees_v1_bincode[..])?;
    let deploy_capital_v1_bincode = include_bytes!("../proof/deploy_capital.zk.bin");
    wasm::db::zkas_db_set(&deploy_capital_v1_bincode[..])?;
    let initialize_v1_bincode = include_bytes!("../proof/initialize.zk.bin");
    wasm::db::zkas_db_set(&initialize_v1_bincode[..])?;

    Ok(())
}

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = RelayerEndowmentFunction::try_from(self_.data[0])?;

    let metadata = match func {
        RelayerEndowmentFunction::InitializeV1 => {
            let params = InitializeParamsV1::decode(&self_.data[1..])?;
            relayer_endowment_initialize_get_metadata_v1(cid, params)?
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            let params = DeployCapitalParamsV1::decode(&self_.data[1..])?;
            relayer_endowment_deploy_capital_get_metadata_v1(cid, params)?
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            let params = ClaimFeesParamsV1::decode(&self_.data[1..])?;
            relayer_endowment_claim_fees_get_metadata_v1(cid, params)?
        }
        // No ZK circuits for WithdrawDeployment, SettleFees, UpdateConfig
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn relayer_endowment_initialize_get_metadata_v1(
    _cid: ContractId,
    params: InitializeParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (rx, ry) = params.signature_public.xy().expect("pk not identity");
    let config_hash = poseidon_hash([pallas::Base::from(4), pallas::Base::from(params.default_backer_cut_bp as u64)]);
    let nonce = pallas::Base::from(wasm::util::get_verifying_block_height()?.get());
    let endowment_id = poseidon_hash([pallas::Base::from(4), rx, ry, config_hash, nonce]);
    // Circuit order: tx_binding(0), tx_nonce(1), derived_endowment_id(2)
    zk_public_inputs.push((
        RELAYER_ENDOWMENT_ZKAS_INIT_NS_V2.to_string(),
        vec![poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]), pallas::Base::zero(), endowment_id],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn relayer_endowment_deploy_capital_get_metadata_v1(
    _cid: ContractId,
    params: DeployCapitalParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (rx, ry) = params.relayer_pub.xy().expect("pk not identity");
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let (bx, by) = params.signature_public.xy().expect("pk not identity");
    let nonce = pallas::Base::from(wasm::util::get_verifying_block_height()?.get());
    let config_hash = poseidon_hash([pallas::Base::from(4), pallas::Base::from(params.backer_cut_bp as u64)]);
    let endowment_id = poseidon_hash([pallas::Base::from(4), rx, ry, config_hash, nonce]);
    let deployment_id = poseidon_hash([
        pallas::Base::from(4),
        endowment_id,
        bx,
        by,
        pallas::Base::from(params.amount),
        nonce,
    ]);
    let vc_affine = params.value_commit.to_affine();
    let coords = vc_affine.coordinates();
    if coords.is_none().into() {
        return Ok(vec![]);
    } else {
    let vc_coords = coords.unwrap();
    // Circuit order: deployment_id(0), vc_x(1), tx_binding(2), tx_nonce(3), vc_y(4)
    zk_public_inputs.push((
        RELAYER_ENDOWMENT_ZKAS_DEPLOY_CAPITAL_NS_V2.to_string(),
        vec![deployment_id, *vc_coords.x(), poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]), pallas::Base::zero(), *vc_coords.y()],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
    }
}

fn relayer_endowment_claim_fees_get_metadata_v1(
    _cid: ContractId,
    params: ClaimFeesParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let nonce = pallas::Base::from(wasm::util::get_verifying_block_height()?.get());
    let claim_id = poseidon_hash([
        pallas::Base::from(4),
        params.deployment_id,
        pallas::Base::from_repr(params.backer_pub_x).unwrap(),
        pallas::Base::from_repr(params.backer_pub_y).unwrap(),
        pallas::Base::from(params.fee_share),
        nonce,
    ]);
    // Circuit order: tx_binding(0), tx_nonce(1), derived_claim_id(2)
    zk_public_inputs.push((
        RELAYER_ENDOWMENT_ZKAS_CLAIM_FEES_NS_V2.to_string(),
        vec![poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]), pallas::Base::zero(), claim_id],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = RelayerEndowmentFunction::try_from(func_byte)?;

    let update_bytes = match func {
        RelayerEndowmentFunction::InitializeV1 => {
            process_initialize_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            process_deploy_capital_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::WithdrawDeploymentV1 => {
            process_withdraw_deployment_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            process_claim_fees_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::SettleFeesV1 => {
            process_settle_fees_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::UpdateConfigV1 => {
            process_update_config_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::ForceSettleV1 => {
            process_force_settle_instruction(cid, call_idx, calls)?
        }
        RelayerEndowmentFunction::DeactivateEndowmentV1 => {
            process_deactivate_endowment_instruction(cid, call_idx, calls)?
        }
        #[cfg(feature = "relayer")]
        RelayerEndowmentFunction::RegisterRelayerV1 => {
            crate::relayer::process_register_relayer(cid, call_idx, calls)?
        }
        #[cfg(feature = "relayer")]
        RelayerEndowmentFunction::VerifyRelayerReputationV1 => {
            crate::relayer::process_verify_reputation(cid, call_idx, calls)?
        }
        #[cfg(feature = "relayer")]
        RelayerEndowmentFunction::RegisterFeeScheduleV1 => {
            crate::relayer::process_register_fee_schedule(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = RelayerEndowmentFunction::try_from(update_data[0])?;

    match func {
        RelayerEndowmentFunction::InitializeV1 => {
            let update = InitializeUpdateV1::decode(&update_data[1..])?;
            apply_initialize_update(cid, update)
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            let update = DeployCapitalUpdateV1::decode(&update_data[1..])?;
            apply_deploy_capital_update(cid, update)
        }
        RelayerEndowmentFunction::WithdrawDeploymentV1 => {
            let update = WithdrawDeploymentUpdateV1::decode(&update_data[1..])?;
            apply_withdraw_deployment_update(cid, update)
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            let update = ClaimFeesUpdateV1::decode(&update_data[1..])?;
            apply_claim_fees_update(cid, update)
        }
        RelayerEndowmentFunction::SettleFeesV1 => {
            let update = SettleFeesUpdateV1::decode(&update_data[1..])?;
            apply_settle_fees_update(cid, update)
        }
        RelayerEndowmentFunction::UpdateConfigV1 => {
            let update = UpdateConfigUpdateV1::decode(&update_data[1..])?;
            apply_update_config_update(cid, update)
        }
        RelayerEndowmentFunction::ForceSettleV1 => {
            let update = ForceSettleUpdateV1::decode(&update_data[1..])?;
            apply_force_settle_update(cid, update)
        }
        RelayerEndowmentFunction::DeactivateEndowmentV1 => {
            let update = DeactivateEndowmentUpdateV1::decode(&update_data[1..])?;
            apply_deactivate_endowment_update(cid, update)
        }
        #[cfg(feature = "relayer")]
        RelayerEndowmentFunction::RegisterRelayerV1 => {
            let update = crate::relayer::RegisterRelayerUpdateV1::decode(&update_data[1..])?;
            crate::relayer::apply_register_relayer(cid, update)
        }
        #[cfg(feature = "relayer")]
        RelayerEndowmentFunction::VerifyRelayerReputationV1 => {
            // read-only, no state change
            Ok(())
        }
        #[cfg(feature = "relayer")]
        RelayerEndowmentFunction::RegisterFeeScheduleV1 => {
            let update = crate::relayer::RegisterFeeScheduleUpdateV1::decode(&update_data[1..])?;
            crate::relayer::apply_register_fee_schedule(cid, update)
        }
    }
}

// ============================================================================
// INITIALIZE
// ============================================================================

fn process_initialize_instruction(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = InitializeParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::initialize] Initializing endowment account");

    // Validate backer cut
    if params.default_backer_cut_bp > 10000 {
        return Err(RelayerEndowmentError::InvalidParams("backer_cut_bp > 10000".into()).into());
    }

    // Use signature_public from params as the relayer's public key
    let relayer_pub = params.signature_public;

    let update = InitializeUpdateV1 {
        instance_seed: params.instance_seed,
        relayer_pub,
        default_backer_cut_bp: params.default_backer_cut_bp,
        created_at: wasm::util::get_verifying_block_height()?.get(),
    };

    msg!("[relayer_endowment::initialize] Endowment account created");
    Ok(update.encode())
}

fn apply_initialize_update(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;

    let account = RelayerEndowmentAccount {
        version: 1,
        instance_seed: update.instance_seed,
        relayer_pub: update.relayer_pub,
        total_deployed: 0,
        active_deployments: 0,
        accumulated_fees: 0,
        default_backer_cut_bp: update.default_backer_cut_bp,
        created_at: update.created_at,
        last_settlement_height: update.created_at,
        total_collected_fees_log: 0,
        is_active: true,
        total_slashed: 0,
        total_successful: 0,
    };

    wasm::db::db_set(
        registry_db,
        &compute_relayer_key(&update.relayer_pub),
        &account.encode(),
    )?;
    msg!("[relayer_endowment::initialize::update] Account stored");

    Ok(())
}

// ============================================================================
// DEPLOY CAPITAL
// ============================================================================

fn process_deploy_capital_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[relayer_endowment::DeployCapitalV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(RelayerEndowmentError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[relayer_endowment::DeployCapitalV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(RelayerEndowmentError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, RELAYER_ENDOWMENT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(RelayerEndowmentError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params = DeployCapitalParamsV1::decode(&self_.data[1..])?;

    // Validate child transfer amount using value_commit comparison
    let relayer_key = compute_relayer_key(&params.relayer_pub);
    #[expect(clippy::expect_used, reason = "slice length checked above")]
    let key_arr: [u8; 32] = relayer_key[..32].try_into()
        .expect("poseidon_hash output is always 32 bytes");
    let relayer_base = pallas::Base::from_repr(key_arr)
        .expect("poseidon hash output is always a canonical field element");
    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        relayer_base,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    msg!("[relayer_endowment::deploy] Deploying {} to relayer", params.amount);

    // Validate amount
    if params.amount < RELAYER_ENDOWMENT_MIN_DEPLOY {
        return Err(RelayerEndowmentError::InsufficientDeploy(RELAYER_ENDOWMENT_MIN_DEPLOY).into());
    }

    // Get endowment account
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &compute_relayer_key(&params.relayer_pub))? {
            Some(data) => RelayerEndowmentAccount::decode(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    // Reputation check (Phase 2d hardening): backers can set minimum thresholds
    if let Some(max_slash) = params.max_slash_count {
        if account.total_slashed > max_slash {
            msg!(
                "[relayer_endowment::deploy] Reputation check failed: total_slashed {} > max_slash_count {}",
                account.total_slashed, max_slash
            );
            return Err(RelayerEndowmentError::ReputationCheckFailed(
                format!("total_slashed {} exceeds max {}", account.total_slashed, max_slash)
            ).into());
        }
    }

    if let Some(min_rate) = params.min_success_rate_bp {
        let total_events = account.total_successful + account.total_slashed;
        let success_rate = if total_events > 0 {
            ((account.total_successful as u128 * 10000) / total_events as u128) as u64
        } else {
            10000 // No history yet, assume perfect
        };
        if success_rate < min_rate {
            msg!(
                "[relayer_endowment::deploy] Reputation check failed: success_rate {} < min_success_rate_bp {}",
                success_rate, min_rate
            );
            return Err(RelayerEndowmentError::ReputationCheckFailed(
                format!("success_rate {} below min {}", success_rate, min_rate)
            ).into());
        }
    }

    // Generate deployment ID
    // Use signature_public from params as the backer's public key
    let backer_pub = params.signature_public;
    let block_height = wasm::util::get_verifying_block_height()?.get();
    let deployment_id = derive_deployment_id(params.relayer_pub, &backer_pub, block_height);

    // Update account
    account.total_deployed += params.amount;
    account.active_deployments += 1;

    let deployment = EndowmentDeployment {
        version: 1,
        deployment_id,
        relayer_pub: params.relayer_pub,
        backer_pub,
        amount: params.amount,
        backer_cut_bp: params.backer_cut_bp,
        accumulated_fees: 0,
        deployed_at: block_height,
        withdraw_requested_at: None,
        withdrawn: false,
    };

    let update = DeployCapitalUpdateV1 { account, deployment };

    msg!("[relayer_endowment::deploy] Deployment {:?} created", deployment_id);
    Ok(update.encode())
}

fn apply_deploy_capital_update(cid: ContractId, update: DeployCapitalUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    wasm::db::db_set(
        registry_db,
        &compute_relayer_key(&update.account.relayer_pub),
        &update.account.encode(),
    )?;

    wasm::db::db_set(
        deployments_db,
        &update.deployment.deployment_id.to_repr(),
        &update.deployment.encode(),
    )?;
    msg!("[relayer_endowment::deploy::update] Deployment stored");

    Ok(())
}

// ============================================================================
// WITHDRAW DEPLOYMENT
// ============================================================================

fn process_withdraw_deployment_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token withdrawal
    if this_call.children_indexes.len() != 1 {
        msg!("[relayer_endowment::WithdrawDeploymentV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(RelayerEndowmentError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[relayer_endowment::WithdrawDeploymentV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(RelayerEndowmentError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, RELAYER_ENDOWMENT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(RelayerEndowmentError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params = WithdrawDeploymentParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::withdraw] Withdrawal for deployment {:?}", params.deployment_id);

    // Get deployment
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    let mut deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &params.deployment_id.to_repr())? {
            Some(data) => EndowmentDeployment::decode(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    if deployment.withdrawn {
        return Err(RelayerEndowmentError::DeploymentAlreadyWithdrawn.into());
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(deployment.amount),
        params.deployment_id,
    ]);
    validate_child_value_commit(&child_call.data, deployment.amount, value_blind)?;

    let payout_amount = deployment.amount;

    deployment.withdrawn = true;
    deployment.accumulated_fees = 0;

    let update = WithdrawDeploymentUpdateV1 { deployment };

    msg!("[relayer_endowment::withdraw] Payout: {}", payout_amount);
    Ok(update.encode())
}

fn apply_withdraw_deployment_update(cid: ContractId, update: WithdrawDeploymentUpdateV1) -> ContractResult {
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    wasm::db::db_set(
        deployments_db,
        &update.deployment.deployment_id.to_repr(),
        &update.deployment.encode(),
    )?;
    msg!("[relayer_endowment::withdraw::update] Deployment withdrawn");

    Ok(())
}

// ============================================================================
// CLAIM FEES
// ============================================================================

fn process_claim_fees_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = ClaimFeesParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::claim_fees] Claiming fees for deployment {:?}", params.deployment_id);

    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    let mut deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &params.deployment_id.to_repr())? {
            Some(data) => EndowmentDeployment::decode(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    if deployment.accumulated_fees == 0 {
        return Err(RelayerEndowmentError::NoFees.into());
    }

    let claimed_amount = deployment.accumulated_fees;
    deployment.accumulated_fees = 0;

    let update = ClaimFeesUpdateV1 { deployment };

    msg!("[relayer_endowment::claim_fees] Claimed: {}", claimed_amount);
    Ok(update.encode())
}

fn apply_claim_fees_update(cid: ContractId, update: ClaimFeesUpdateV1) -> ContractResult {
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    wasm::db::db_set(
        deployments_db,
        &update.deployment.deployment_id.to_repr(),
        &update.deployment.encode(),
    )?;
    msg!("[relayer_endowment::claim_fees::update] Fees claimed");

    Ok(())
}

// ============================================================================
// SETTLE FEES
// ============================================================================

fn process_settle_fees_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = SettleFeesParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::settle_fees] Settling {} fees to relayer {:?}", params.total_fees, params.relayer_pub);

    // Auth: caller must be the relayer
    if params.signature_public != params.relayer_pub {
        return Err(RelayerEndowmentError::Unauthorized.into());
    }

    // Get endowment account
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &compute_relayer_key(&params.relayer_pub))? {
            Some(data) => RelayerEndowmentAccount::decode(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    if !account.is_active {
        return Err(RelayerEndowmentError::EndpointInactive.into());
    }

    // Validate allocation sum matches total_fees
    let alloc_sum: u64 = params.allocations.iter().map(|a| a.fee_amount).sum();
    if alloc_sum != params.total_fees {
        msg!("[relayer_endowment::settle_fees] Allocation sum {} != total_fees {}", alloc_sum, params.total_fees);
        return Err(RelayerEndowmentError::InvalidParams("allocation sum != total_fees".into()).into());
    }

    // Verify each deployment exists, belongs to this relayer, and is not withdrawn
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    if params.allocations.len() > crate::RELAYER_ENDOWMENT_MAX_ALLOCATIONS {
        msg!("[relayer_endowment::settle_fees] Too many allocations: {} (max {})",
            params.allocations.len(), crate::RELAYER_ENDOWMENT_MAX_ALLOCATIONS);
        return Err(RelayerEndowmentError::InvalidParams("too many allocations".into()).into());
    }
    let mut deployments = Vec::with_capacity(params.allocations.len());
    for alloc in &params.allocations {
        let mut deployment: EndowmentDeployment =
            match wasm::db::db_get(deployments_db, &alloc.deployment_id.to_repr())? {
                Some(data) => EndowmentDeployment::decode(&data)?,
                None => {
                    msg!("[relayer_endowment::settle_fees] Deployment {:?} not found", alloc.deployment_id);
                    return Err(RelayerEndowmentError::DeploymentNotFound.into());
                }
            };
        if deployment.relayer_pub != params.relayer_pub {
            msg!("[relayer_endowment::settle_fees] Deployment {:?} belongs to different relayer", alloc.deployment_id);
            return Err(RelayerEndowmentError::Unauthorized.into());
        }
        if deployment.withdrawn {
            msg!("[relayer_endowment::settle_fees] Deployment {:?} already withdrawn", alloc.deployment_id);
            return Err(RelayerEndowmentError::DeploymentAlreadyWithdrawn.into());
        }
        deployment.accumulated_fees += alloc.fee_amount;
        deployments.push(deployment);
    }

    account.accumulated_fees += params.total_fees;
    account.last_settlement_height = wasm::util::get_verifying_block_height()?.get();

    let update = SettleFeesUpdateV1 { account, deployments };

    msg!("[relayer_endowment::settle_fees] Settled fees to {} deployments", update.deployments.len());
    Ok(update.encode())
}

fn apply_settle_fees_update(cid: ContractId, update: SettleFeesUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    for deployment in &update.deployments {
        wasm::db::db_set(
            deployments_db,
            &deployment.deployment_id.to_repr(),
            &deployment.encode(),
        )?;
    }

    wasm::db::db_set(
        registry_db,
        &compute_relayer_key(&update.account.relayer_pub),
        &update.account.encode(),
    )?;
    msg!("[relayer_endowment::settle_fees::update] Fees distributed to {} deployments", update.deployments.len());

    Ok(())
}

// ============================================================================
// UPDATE CONFIG
// ============================================================================

fn process_update_config_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = UpdateConfigParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::update_config] Updating config for relayer");

    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &compute_relayer_key(&params.relayer_pub))? {
            Some(data) => RelayerEndowmentAccount::decode(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    account.default_backer_cut_bp = params.default_backer_cut_bp;

    let update = UpdateConfigUpdateV1 { account };

    Ok(update.encode())
}

fn apply_update_config_update(cid: ContractId, update: UpdateConfigUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;

    wasm::db::db_set(
        registry_db,
        &compute_relayer_key(&update.account.relayer_pub),
        &update.account.encode(),
    )?;
    msg!("[relayer_endowment::update_config::update] Config updated");

    Ok(())
}

// ============================================================================
// FORCE SETTLE
// ============================================================================

fn process_force_settle_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = ForceSettleParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::force_settle] Force settling fees for deployment {:?}", params.deployment_id);

    // Get endowment account
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &compute_relayer_key(&params.relayer_pub))? {
            Some(data) => RelayerEndowmentAccount::decode(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    if !account.is_active {
        return Err(RelayerEndowmentError::EndpointInactive.into());
    }

    // Verify settlement timeout has elapsed (use on-chain block height, not caller-provided)
    let current_block = wasm::util::get_verifying_block_height()?;
    let blocks_since_settlement = current_block.get().saturating_sub(account.last_settlement_height);
    if blocks_since_settlement < RELAYER_ENDOWMENT_FORCE_SETTLEMENT_TIMEOUT {
        msg!("[relayer_endowment::force_settle] Settlement not due: {} blocks since last settlement (need {})",
            blocks_since_settlement, RELAYER_ENDOWMENT_FORCE_SETTLEMENT_TIMEOUT);
        return Err(RelayerEndowmentError::SettlementNotDue.into());
    }

    // Get the deployment and verify backer ownership
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    let mut deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &params.deployment_id.to_repr())? {
            Some(data) => EndowmentDeployment::decode(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    if deployment.withdrawn {
        return Err(RelayerEndowmentError::DeploymentAlreadyWithdrawn.into());
    }
    if deployment.relayer_pub != params.relayer_pub {
        return Err(RelayerEndowmentError::Unauthorized.into());
    }
    if deployment.backer_pub != params.signature_public {
        return Err(RelayerEndowmentError::Unauthorized.into());
    }

    // Compute pro-rata share of logged fees
    let force_settled_amount = if account.total_collected_fees_log > 0 && account.total_deployed > 0 {
        (account.total_collected_fees_log as u128)
            .saturating_mul(deployment.amount as u128)
            .saturating_div(account.total_deployed as u128) as u64
    } else {
        0
    };

    if force_settled_amount > 0 {
        deployment.accumulated_fees += force_settled_amount;
    }
    account.total_collected_fees_log = account.total_collected_fees_log.saturating_sub(force_settled_amount);
    account.last_settlement_height = wasm::util::get_verifying_block_height()?.get();

    let update = ForceSettleUpdateV1 {
        account,
        force_settled_amount,
        deployment,
    };

    msg!("[relayer_endowment::force_settle] Force settled {} fees", force_settled_amount);
    Ok(update.encode())
}

fn apply_force_settle_update(cid: ContractId, update: ForceSettleUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    if update.force_settled_amount > 0 {
        wasm::db::db_set(
            deployments_db,
            &update.deployment.deployment_id.to_repr(),
            &update.deployment.encode(),
        )?;
    }

    wasm::db::db_set(
        registry_db,
        &compute_relayer_key(&update.account.relayer_pub),
        &update.account.encode(),
    )?;
    msg!("[relayer_endowment::force_settle::update] Force settlement complete");

    Ok(())
}

// ============================================================================
// DEACTIVATE ENDOWMENT
// ============================================================================

fn process_deactivate_endowment_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = DeactivateEndowmentParamsV1::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::deactivate_endowment] Deactivating endowment");

    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let account_data = wasm::db::db_get(registry_db, &compute_relayer_key(&params.relayer_pub))?
        .ok_or(ContractError::DbGetEmpty)?;
    let mut account: RelayerEndowmentAccount = RelayerEndowmentAccount::decode(&account_data)?;

    if !account.is_active {
        return Err(RelayerEndowmentError::EndpointInactive.into())
    }

    account.is_active = false;

    let update = DeactivateEndowmentUpdateV1 { account };

    msg!("[relayer_endowment::deactivate_endowment] Endowment deactivated");
    Ok(update.encode())
}

fn apply_deactivate_endowment_update(
    cid: ContractId,
    update: DeactivateEndowmentUpdateV1,
) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    wasm::db::db_set(
        registry_db,
        &compute_relayer_key(&update.account.relayer_pub),
        &update.account.encode(),
    )?;

    msg!("[relayer_endowment::deactivate_endowment::update] Endowment deactivated");
    Ok(())
}

// ============================================================================
// HELPERS
// ============================================================================

#[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
fn derive_deployment_id(relayer_pub: PublicKey, backer_pub: &PublicKey, nonce: u64) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    poseidon_hash([relayer_pub.x().expect("pk not identity"), relayer_pub.y().expect("pk not identity"), backer_pub.x().expect("pk not identity"), backer_pub.y().expect("pk not identity"), pallas::Base::from(nonce)])
}
