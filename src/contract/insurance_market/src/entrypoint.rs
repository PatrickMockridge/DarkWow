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

//! Insurance Market Contract Entrypoint

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};

use crate::error::InsuranceMarketError;
use crate::model::*;
use crate::InsuranceMarketFunction;

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// Include entrypoint modules
mod register_risk_type_v1;
mod create_market_v1;
mod underwrite_v1;
mod purchase_coverage_v1;
mod file_claim_v1;
mod resolve_claim_v1;
mod withdraw_premium_v1;
mod update_premium_v1;
mod underwrite_with_capability_v1;
mod purchase_coverage_with_capability_v1;
mod purchase_coverage_with_dag_v1;
mod resolve_claim_with_capability_v1;
mod deactivate_underwriter_v1;
mod close_market_v1;
mod retire_risk_type_v1;

use register_risk_type_v1::*;
use create_market_v1::*;
use underwrite_v1::*;
use purchase_coverage_v1::*;
use file_claim_v1::*;
use resolve_claim_v1::*;
use withdraw_premium_v1::*;
use update_premium_v1::*;
use underwrite_with_capability_v1::*;
use purchase_coverage_with_capability_v1::*;
use purchase_coverage_with_dag_v1::*;
use resolve_claim_with_capability_v1::*;
use deactivate_underwriter_v1::*;
use close_market_v1::*;
use retire_risk_type_v1::*;

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize database trees
    let info_db = wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, crate::INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_MARKETS_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_COVERAGES_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_CLAIMS_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_ENDOWMENT_TREE)?;


    let underwrite_with_capability_v2_bincode =
        include_bytes!("../proof/underwrite_with_capability_v2.zk.bin");
    wasm::db::zkas_db_set(&underwrite_with_capability_v2_bincode[..])?;
    let purchase_coverage_with_capability_v2_bincode =
        include_bytes!("../proof/purchase_coverage_with_capability_v2.zk.bin");
    wasm::db::zkas_db_set(&purchase_coverage_with_capability_v2_bincode[..])?;

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = InsuranceMarketFunction::try_from(self_.data[0])?;

    let metadata = match func {
        InsuranceMarketFunction::UnderwriteWithCapabilityV1 => {
            let params = match UnderwriteWithCapabilityParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[insurance_market::get_metadata] Error: Failed to decode UnderwriteWithCapabilityParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            underwrite_with_capability_get_metadata_v1(params)?
        }
        InsuranceMarketFunction::PurchaseCoverageV1 => {
            let params = match PurchaseCoverageParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[insurance_market::get_metadata] Error: Failed to decode PurchaseCoverageParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purchase_coverage_get_metadata_v1(params)?
        }
        InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1 => {
            let params = match PurchaseCoverageWithCapabilityParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[insurance_market::get_metadata] Error: Failed to decode PurchaseCoverageWithCapabilityParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purchase_coverage_with_capability_get_metadata_v1(params)?
        }
        InsuranceMarketFunction::PurchaseCoverageWithDAGV1 => {
            let params = match PurchaseCoverageWithDAGParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[insurance_market::get_metadata] Error: Failed to decode PurchaseCoverageWithDAGParamsV1: {:?}", e); wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            purchase_coverage_with_dag_get_metadata_v1(params)?
        }
        InsuranceMarketFunction::InitializeV1 => vec![],
        InsuranceMarketFunction::RegisterRiskTypeV1 => vec![],
        InsuranceMarketFunction::CreateMarketV1 => vec![],
        InsuranceMarketFunction::UnderwriteV1 => vec![],
        InsuranceMarketFunction::FileClaimV1 => vec![],
        InsuranceMarketFunction::ResolveClaimV1 => vec![],
        InsuranceMarketFunction::WithdrawPremiumV1 => vec![],
        InsuranceMarketFunction::UpdatePremiumV1 => vec![],
        InsuranceMarketFunction::ResolveClaimWithCapabilityV1 => vec![],
        InsuranceMarketFunction::DeactivateUnderwriterV1 => vec![],
        InsuranceMarketFunction::CloseMarketV1 => vec![],
        InsuranceMarketFunction::RetireRiskTypeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for UnderwriteWithCapabilityV1
///
/// The ZK circuit `underwrite_with_capability_v1.zk` constrains:
///   constrain_instance(underwriter_pub_x);
///   constrain_instance(underwriter_pub_y);
///   constrain_instance(required_capability_id);
fn underwrite_with_capability_get_metadata_v1(
    params: UnderwriteWithCapabilityParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let (ux, uy) = params.underwriter.xy().expect("pk not identity");
    let cap = Option::from(pallas::Base::from_repr(params.capability_secret))
        .ok_or(InsuranceMarketError::InvalidCapability)?;
    zk_public_inputs.push((
        crate::INSURANCE_MARKET_ZKAS_UNDERWRITE_WITH_CAPABILITY_NS_V2.to_string(),
        vec![ux, uy, cap],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for PurchaseCoverageWithCapabilityV1
///
/// The ZK circuit `purchase_coverage_with_capability_v1.zk` constrains:
///   constrain_instance(buyer_pub_x);
///   constrain_instance(buyer_pub_y);
///   constrain_instance(required_capability_id);
fn purchase_coverage_with_capability_get_metadata_v1(
    params: PurchaseCoverageWithCapabilityParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let (bx, by) = params.buyer.xy().expect("pk not identity");
    let cap = Option::from(pallas::Base::from_repr(params.capability_secret))
        .ok_or(InsuranceMarketError::InvalidCapability)?;
    zk_public_inputs.push((
        crate::INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_WITH_CAPABILITY_NS_V2.to_string(),
        vec![bx, by, cap],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = InsuranceMarketFunction::try_from(self_.data[0])?;

    let update_data = match func {
        InsuranceMarketFunction::InitializeV1 => {
            return Err(InsuranceMarketError::InvalidParameter("Use init".to_string()).into())
        }
        InsuranceMarketFunction::RegisterRiskTypeV1 => {
            insurance_market_register_risk_type_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::CreateMarketV1 => {
            insurance_market_create_market_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::UnderwriteV1 => {
            insurance_market_underwrite_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::PurchaseCoverageV1 => {
            insurance_market_purchase_coverage_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::FileClaimV1 => {
            insurance_market_file_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::ResolveClaimV1 => {
            insurance_market_resolve_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::WithdrawPremiumV1 => {
            insurance_market_withdraw_premium_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::UpdatePremiumV1 => {
            insurance_market_update_premium_process_instruction_v1(cid, call_idx, calls)?
        }
        // O-Cap enabled functions
        InsuranceMarketFunction::UnderwriteWithCapabilityV1 => {
            insurance_market_underwrite_with_capability_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1 => {
            insurance_market_purchase_coverage_with_capability_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::PurchaseCoverageWithDAGV1 => {
            insurance_market_purchase_coverage_with_dag_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::ResolveClaimWithCapabilityV1 => {
            insurance_market_resolve_claim_with_capability_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::DeactivateUnderwriterV1 => {
            insurance_market_deactivate_underwriter_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::CloseMarketV1 => {
            insurance_market_close_market_process_instruction_v1(cid, call_idx, calls)?
        }
        InsuranceMarketFunction::RetireRiskTypeV1 => {
            insurance_market_retire_risk_type_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match InsuranceMarketFunction::try_from(update_data[0])? {
        InsuranceMarketFunction::InitializeV1 => {
            Err(InsuranceMarketError::InvalidParameter("Use init".to_string()).into())
        }
        InsuranceMarketFunction::RegisterRiskTypeV1 => {
            let update: RegisterRiskTypeUpdateV1 = RegisterRiskTypeUpdateV1::decode(&update_data[1..])?;
            insurance_market_register_risk_type_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::CreateMarketV1 => {
            let update: CreateMarketUpdateV1 = CreateMarketUpdateV1::decode(&update_data[1..])?;
            insurance_market_create_market_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::UnderwriteV1 => {
            let update: UnderwriteUpdateV1 = UnderwriteUpdateV1::decode(&update_data[1..])?;
            insurance_market_underwrite_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::PurchaseCoverageV1 => {
            let update: PurchaseCoverageUpdateV1 = PurchaseCoverageUpdateV1::decode(&update_data[1..])?;
            insurance_market_purchase_coverage_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::FileClaimV1 => {
            let update: FileClaimUpdateV1 = FileClaimUpdateV1::decode(&update_data[1..])?;
            insurance_market_file_claim_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::ResolveClaimV1 => {
            let update: ResolveClaimUpdateV1 = ResolveClaimUpdateV1::decode(&update_data[1..])?;
            insurance_market_resolve_claim_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::WithdrawPremiumV1 => {
            let update: WithdrawPremiumUpdateV1 = WithdrawPremiumUpdateV1::decode(&update_data[1..])?;
            insurance_market_withdraw_premium_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::UpdatePremiumV1 => {
            let update: update_premium_v1::UpdatePremiumUpdateV1 = update_premium_v1::UpdatePremiumUpdateV1::decode(&update_data[1..])?;
            insurance_market_update_premium_process_update_v1(cid, update)
        }
        // O-Cap enabled functions
        InsuranceMarketFunction::UnderwriteWithCapabilityV1 => {
            let update: UnderwriteWithCapabilityUpdateV1 = UnderwriteWithCapabilityUpdateV1::decode(&update_data[1..])?;
            insurance_market_underwrite_with_capability_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1 => {
            let update: PurchaseCoverageWithCapabilityUpdateV1 = PurchaseCoverageWithCapabilityUpdateV1::decode(&update_data[1..])?;
            insurance_market_purchase_coverage_with_capability_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::PurchaseCoverageWithDAGV1 => {
            let update: PurchaseCoverageWithDAGUpdateV1 = PurchaseCoverageWithDAGUpdateV1::decode(&update_data[1..])?;
            insurance_market_purchase_coverage_with_dag_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::ResolveClaimWithCapabilityV1 => {
            let update: ResolveClaimWithCapabilityUpdateV1 = ResolveClaimWithCapabilityUpdateV1::decode(&update_data[1..])?;
            insurance_market_resolve_claim_with_capability_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::DeactivateUnderwriterV1 => {
            let update: DeactivateUnderwriterUpdateV1 = DeactivateUnderwriterUpdateV1::decode(&update_data[1..])?;
            insurance_market_deactivate_underwriter_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::CloseMarketV1 => {
            let update: CloseMarketUpdateV1 = CloseMarketUpdateV1::decode(&update_data[1..])?;
            insurance_market_close_market_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::RetireRiskTypeV1 => {
            let update: RetireRiskTypeUpdateV1 = RetireRiskTypeUpdateV1::decode(&update_data[1..])?;
            insurance_market_retire_risk_type_process_update_v1(cid, update)
        }
    }
}