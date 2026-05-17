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
use dwow_serial::{deserialize, serialize, Encodable};
use pasta_curves::{arithmetic::CurveAffine, group::Curve};

use crate::error::RelayerEndowmentError;
use crate::model::*;
use crate::RelayerEndowmentFunction;
use crate::{
    RELAYER_ENDOWMENT_DEPLOYMENTS_TREE, RELAYER_ENDOWMENT_REGISTRY_TREE,
    RELAYER_ENDOWMENT_MIN_DEPLOY, RELAYER_ENDOWMENT_INFO_TREE,
    RELAYER_ENDOWMENT_ZKAS_INIT_NS_V1, RELAYER_ENDOWMENT_ZKAS_DEPLOY_CAPITAL_NS_V1,
    RELAYER_ENDOWMENT_ZKAS_CLAIM_FEES_NS_V1,
    RELAYER_ENDOWMENT_FORCE_SETTLEMENT_TIMEOUT,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize INFO_TREE with redeployment guard
    let _info_db = match wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, RELAYER_ENDOWMENT_INFO_TREE)?,
    };

    // Initialize database trees with redeployment guards
    if wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE).is_err() {
        wasm::db::db_init(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    }
    if wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE).is_err() {
        wasm::db::db_init(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    }

    let claim_fees_v1_bincode = include_bytes!("../proof/claim_fees_v1.zk.bin");
    wasm::db::zkas_db_set(&claim_fees_v1_bincode[..])?;
    let deploy_capital_v1_bincode = include_bytes!("../proof/deploy_capital_v1.zk.bin");
    wasm::db::zkas_db_set(&deploy_capital_v1_bincode[..])?;
    let initialize_v1_bincode = include_bytes!("../proof/initialize_v1.zk.bin");
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
            let params: InitializeParamsV1 = deserialize(&self_.data[1..])?;
            relayer_endowment_initialize_get_metadata_v1(cid, params)?
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            let params: DeployCapitalParamsV1 = deserialize(&self_.data[1..])?;
            relayer_endowment_deploy_capital_get_metadata_v1(cid, params)?
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            let params: ClaimFeesParamsV1 = deserialize(&self_.data[1..])?;
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
    let (rx, ry) = params.signature_public.xy();
    let config_hash = poseidon_hash([pallas::Base::from(params.default_backer_cut_bp as u64)]);
    let nonce = pallas::Base::from(wasm::util::get_verifying_block_height()? as u64);
    let endowment_id = poseidon_hash([rx, ry, config_hash, nonce]);
    zk_public_inputs.push((
        RELAYER_ENDOWMENT_ZKAS_INIT_NS_V1.to_string(),
        vec![endowment_id],
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
    let (rx, ry) = params.relayer_pub.xy();
    let (bx, by) = params.signature_public.xy();
    let nonce = pallas::Base::from(wasm::util::get_verifying_block_height()? as u64);
    // Compute endowment_id the same way as initialize
    let config_hash = poseidon_hash([pallas::Base::from(params.backer_cut_bp as u64)]);
    let endowment_id = poseidon_hash([rx, ry, config_hash, nonce]);
    let deployment_id = poseidon_hash([
        endowment_id,
        bx,
        by,
        pallas::Base::from(params.amount),
        nonce,
    ]);
    let vc_affine = params.value_commit.to_affine();
    let vc_coords = vc_affine.coordinates().unwrap();
    zk_public_inputs.push((
        RELAYER_ENDOWMENT_ZKAS_DEPLOY_CAPITAL_NS_V1.to_string(),
        vec![deployment_id, *vc_coords.x(), *vc_coords.y()],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn relayer_endowment_claim_fees_get_metadata_v1(
    _cid: ContractId,
    params: ClaimFeesParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let nonce = pallas::Base::from(wasm::util::get_verifying_block_height()? as u64);
    let backer_x = Option::from(pallas::Base::from_repr(params.backer_pub_x))
        .ok_or(RelayerEndowmentError::InvalidParams("Invalid backer_pub_x".to_string()))?;
    let backer_y = Option::from(pallas::Base::from_repr(params.backer_pub_y))
        .ok_or(RelayerEndowmentError::InvalidParams("Invalid backer_pub_y".to_string()))?;
    let claim_id = poseidon_hash([
        params.deployment_id,
        backer_x,
        backer_y,
        pallas::Base::from(params.fee_share),
        nonce,
    ]);
    zk_public_inputs.push((
        RELAYER_ENDOWMENT_ZKAS_CLAIM_FEES_NS_V1.to_string(),
        vec![claim_id],
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
    let func = RelayerEndowmentFunction::try_from(self_.data[0])?;

    match func {
        RelayerEndowmentFunction::InitializeV1 => {
            process_initialize_instruction(cid, call_idx, calls)
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            process_deploy_capital_instruction(cid, call_idx, calls)
        }
        RelayerEndowmentFunction::WithdrawDeploymentV1 => {
            process_withdraw_deployment_instruction(cid, call_idx, calls)
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            process_claim_fees_instruction(cid, call_idx, calls)
        }
        RelayerEndowmentFunction::SettleFeesV1 => {
            process_settle_fees_instruction(cid, call_idx, calls)
        }
        RelayerEndowmentFunction::UpdateConfigV1 => {
            process_update_config_instruction(cid, call_idx, calls)
        }
        RelayerEndowmentFunction::ForceSettleV1 => {
            process_force_settle_instruction(cid, call_idx, calls)
        }
    }
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = RelayerEndowmentFunction::try_from(update_data[0])?;

    match func {
        RelayerEndowmentFunction::InitializeV1 => {
            let update: InitializeUpdateV1 = deserialize(&update_data[1..])?;
            apply_initialize_update(cid, update)
        }
        RelayerEndowmentFunction::DeployCapitalV1 => {
            let update: DeployCapitalUpdateV1 = deserialize(&update_data[1..])?;
            apply_deploy_capital_update(cid, update)
        }
        RelayerEndowmentFunction::WithdrawDeploymentV1 => {
            let update: WithdrawDeploymentUpdateV1 = deserialize(&update_data[1..])?;
            apply_withdraw_deployment_update(cid, update)
        }
        RelayerEndowmentFunction::ClaimRelayerFeesV1 => {
            let update: ClaimFeesUpdateV1 = deserialize(&update_data[1..])?;
            apply_claim_fees_update(cid, update)
        }
        RelayerEndowmentFunction::SettleFeesV1 => {
            let update: SettleFeesUpdateV1 = deserialize(&update_data[1..])?;
            apply_settle_fees_update(cid, update)
        }
        RelayerEndowmentFunction::UpdateConfigV1 => {
            let update: UpdateConfigUpdateV1 = deserialize(&update_data[1..])?;
            apply_update_config_update(cid, update)
        }
        RelayerEndowmentFunction::ForceSettleV1 => {
            let update: ForceSettleUpdateV1 = deserialize(&update_data[1..])?;
            apply_force_settle_update(cid, update)
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: InitializeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::initialize] Initializing endowment account");

    // Validate backer cut
    if params.default_backer_cut_bp > 10000 {
        return Err(RelayerEndowmentError::InvalidParams("backer_cut_bp > 10000".into()).into());
    }

    // Use signature_public from params as the relayer's public key
    let relayer_pub = params.signature_public;

    let update = InitializeUpdateV1 {
        relayer_pub,
        default_backer_cut_bp: params.default_backer_cut_bp,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[relayer_endowment::initialize] Endowment account created");
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_initialize_update(cid: ContractId, update: InitializeUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;

    let account = RelayerEndowmentAccount {
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
        &serialize(&update.relayer_pub),
        &serialize(&account),
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
) -> ContractResult {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[relayer_endowment::DeployCapitalV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(RelayerEndowmentError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[relayer_endowment::DeployCapitalV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(RelayerEndowmentError::InvalidChildCall.into())
    }

    let self_ = &calls[call_idx].data;
    let params: DeployCapitalParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::deploy] Deploying {} to relayer", params.amount);

    // Validate amount
    if params.amount < RELAYER_ENDOWMENT_MIN_DEPLOY {
        return Err(RelayerEndowmentError::InsufficientDeploy(RELAYER_ENDOWMENT_MIN_DEPLOY).into());
    }

    // Get endowment account
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&params.relayer_pub))? {
            Some(data) => deserialize(&data)?,
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
    let deployment_id = derive_deployment_id(
        params.relayer_pub,
        &backer_pub,
        wasm::util::get_verifying_block_height()? as u64,
    );

    // Update account
    account.total_deployed += params.amount;
    account.active_deployments += 1;

    let update = DeployCapitalUpdateV1 {
        deployment_id,
        relayer_pub: params.relayer_pub,
        backer_pub,
        amount: params.amount,
        backer_cut_bp: params.backer_cut_bp,
        total_deployed: account.total_deployed,
        active_deployments: account.active_deployments,
    };

    msg!("[relayer_endowment::deploy] Deployment {:?} created", deployment_id);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_deploy_capital_update(cid: ContractId, update: DeployCapitalUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    // Update account
    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&update.relayer_pub))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    account.total_deployed = update.total_deployed;
    account.active_deployments = update.active_deployments;

    wasm::db::db_set(
        registry_db,
        &serialize(&update.relayer_pub),
        &serialize(&account),
    )?;

    // Create deployment
    let deployment = EndowmentDeployment {
        deployment_id: update.deployment_id,
        relayer_pub: update.relayer_pub,
        backer_pub: update.backer_pub,
        amount: update.amount,
        backer_cut_bp: update.backer_cut_bp,
        accumulated_fees: 0,
        deployed_at: wasm::util::get_verifying_block_height()? as u64,
        withdraw_requested_at: None,
        withdrawn: false,
    };

    wasm::db::db_set(
        deployments_db,
        &serialize(&update.deployment_id),
        &serialize(&deployment),
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
) -> ContractResult {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token withdrawal
    if this_call.children_indexes.len() != 1 {
        msg!("[relayer_endowment::WithdrawDeploymentV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", this_call.children_indexes.len());
        return Err(RelayerEndowmentError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[relayer_endowment::WithdrawDeploymentV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(RelayerEndowmentError::InvalidChildCall.into())
    }

    let self_ = &calls[call_idx].data;
    let params: WithdrawDeploymentParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::withdraw] Withdrawal for deployment {:?}", params.deployment_id);

    // Get deployment
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    let deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &serialize(&params.deployment_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    if deployment.withdrawn {
        return Err(RelayerEndowmentError::DeploymentAlreadyWithdrawn.into());
    }

    // Calculate payout (principal + accumulated fees)
    let payout_amount = deployment.amount;
    let fees_claimed = deployment.accumulated_fees;

    let update = WithdrawDeploymentUpdateV1 {
        deployment_id: params.deployment_id,
        payout_amount,
        fees_claimed,
    };

    msg!("[relayer_endowment::withdraw] Payout: {}", payout_amount);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_withdraw_deployment_update(cid: ContractId, update: WithdrawDeploymentUpdateV1) -> ContractResult {
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    // Get and update deployment
    let mut deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &serialize(&update.deployment_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    deployment.withdrawn = true;
    deployment.accumulated_fees = 0;

    wasm::db::db_set(
        deployments_db,
        &serialize(&update.deployment_id),
        &serialize(&deployment),
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ClaimFeesParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::claim_fees] Claiming fees for deployment {:?}", params.deployment_id);

    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    let deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &serialize(&params.deployment_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    if deployment.accumulated_fees == 0 {
        return Err(RelayerEndowmentError::NoFees.into());
    }

    let update = ClaimFeesUpdateV1 {
        deployment_id: params.deployment_id,
        claimed_amount: deployment.accumulated_fees,
        remaining_fees: 0,
    };

    msg!("[relayer_endowment::claim_fees] Claimed: {}", deployment.accumulated_fees);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_claim_fees_update(cid: ContractId, update: ClaimFeesUpdateV1) -> ContractResult {
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    let mut deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &serialize(&update.deployment_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
        };

    deployment.accumulated_fees = update.remaining_fees;

    wasm::db::db_set(
        deployments_db,
        &serialize(&update.deployment_id),
        &serialize(&deployment),
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: SettleFeesParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::settle_fees] Settling {} fees to relayer {:?}", params.total_fees, params.relayer_pub);

    // Auth: caller must be the relayer
    if params.signature_public != params.relayer_pub {
        return Err(RelayerEndowmentError::Unauthorized.into());
    }

    // Get endowment account
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&params.relayer_pub))? {
            Some(data) => deserialize(&data)?,
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
    for alloc in &params.allocations {
        let deployment: EndowmentDeployment =
            match wasm::db::db_get(deployments_db, &serialize(&alloc.deployment_id))? {
                Some(data) => deserialize(&data)?,
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
    }

    let update = SettleFeesUpdateV1 {
        relayer_pub: params.relayer_pub,
        total_fees_settled: params.total_fees,
        deployments_updated: params.allocations.len() as u64,
        allocations: params.allocations,
    };

    msg!("[relayer_endowment::settle_fees] Settled fees to {} deployments", update.deployments_updated);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_settle_fees_update(cid: ContractId, update: SettleFeesUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&update.relayer_pub))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    // Distribute fees to each deployment
    for alloc in &update.allocations {
        let mut deployment: EndowmentDeployment =
            match wasm::db::db_get(deployments_db, &serialize(&alloc.deployment_id))? {
                Some(data) => deserialize(&data)?,
                None => continue,
            };
        deployment.accumulated_fees += alloc.fee_amount;
        wasm::db::db_set(
            deployments_db,
            &serialize(&alloc.deployment_id),
            &serialize(&deployment),
        )?;
    }

    // Track total fees ever settled and update settlement height
    account.accumulated_fees += update.total_fees_settled;
    account.last_settlement_height = wasm::util::get_verifying_block_height()? as u64;

    wasm::db::db_set(
        registry_db,
        &serialize(&update.relayer_pub),
        &serialize(&account),
    )?;
    msg!("[relayer_endowment::settle_fees::update] Fees distributed to {} deployments", update.deployments_updated);

    Ok(())
}

// ============================================================================
// UPDATE CONFIG
// ============================================================================

fn process_update_config_instruction(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: UpdateConfigParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::update_config] Updating config for relayer");

    let update = UpdateConfigUpdateV1 {
        relayer_pub: params.relayer_pub,
        default_backer_cut_bp: params.default_backer_cut_bp,
    };

    wasm::util::set_return_data(&serialize(&update))
}

fn apply_update_config_update(cid: ContractId, update: UpdateConfigUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;

    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&update.relayer_pub))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    account.default_backer_cut_bp = update.default_backer_cut_bp;

    wasm::db::db_set(
        registry_db,
        &serialize(&update.relayer_pub),
        &serialize(&account),
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
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ForceSettleParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[relayer_endowment::force_settle] Force settling fees for deployment {:?}", params.deployment_id);

    // Get endowment account
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&params.relayer_pub))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    if !account.is_active {
        return Err(RelayerEndowmentError::EndpointInactive.into());
    }

    // Verify settlement timeout has elapsed
    let blocks_since_settlement = params.current_block.saturating_sub(account.last_settlement_height);
    if blocks_since_settlement < RELAYER_ENDOWMENT_FORCE_SETTLEMENT_TIMEOUT {
        msg!("[relayer_endowment::force_settle] Settlement not due: {} blocks since last settlement (need {})",
            blocks_since_settlement, RELAYER_ENDOWMENT_FORCE_SETTLEMENT_TIMEOUT);
        return Err(RelayerEndowmentError::SettlementNotDue.into());
    }

    // Get the deployment and verify backer ownership
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;
    let deployment: EndowmentDeployment =
        match wasm::db::db_get(deployments_db, &serialize(&params.deployment_id))? {
            Some(data) => deserialize(&data)?,
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

    let update = ForceSettleUpdateV1 {
        deployment_id: params.deployment_id,
        relayer_pub: params.relayer_pub,
        force_settled_amount,
    };

    msg!("[relayer_endowment::force_settle] Force settled {} fees", force_settled_amount);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_force_settle_update(cid: ContractId, update: ForceSettleUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_REGISTRY_TREE)?;
    let deployments_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_DEPLOYMENTS_TREE)?;

    let mut account: RelayerEndowmentAccount =
        match wasm::db::db_get(registry_db, &serialize(&update.relayer_pub))? {
            Some(data) => deserialize(&data)?,
            None => return Err(RelayerEndowmentError::EndowmentNotFound.into()),
        };

    // Credit the force-settled fees to the deployment
    if update.force_settled_amount > 0 {
        let mut deployment: EndowmentDeployment =
            match wasm::db::db_get(deployments_db, &serialize(&update.deployment_id))? {
                Some(data) => deserialize(&data)?,
                None => return Err(RelayerEndowmentError::DeploymentNotFound.into()),
            };
        deployment.accumulated_fees += update.force_settled_amount;
        wasm::db::db_set(
            deployments_db,
            &serialize(&update.deployment_id),
            &serialize(&deployment),
        )?;
    }

    // Subtract force-settled amount from fee log and update settlement height
    account.total_collected_fees_log = account.total_collected_fees_log.saturating_sub(update.force_settled_amount);
    account.last_settlement_height = wasm::util::get_verifying_block_height()? as u64;

    wasm::db::db_set(
        registry_db,
        &serialize(&update.relayer_pub),
        &serialize(&account),
    )?;
    msg!("[relayer_endowment::force_settle::update] Force settlement complete");

    Ok(())
}

// ============================================================================
// HELPERS
// ============================================================================

fn derive_deployment_id(relayer_pub: PublicKey, backer_pub: &PublicKey, nonce: u64) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    poseidon_hash([relayer_pub.x(), relayer_pub.y(), backer_pub.x(), backer_pub.y(), pallas::Base::from(nonce)])
}