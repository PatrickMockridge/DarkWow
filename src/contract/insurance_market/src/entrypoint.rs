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

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    wasm, ContractCall,
};
use darkfi_serial::deserialize;

use crate::error::InsuranceMarketError;
use crate::model::*;
use crate::InsuranceMarketFunction;

darkfi_sdk::define_contract!(
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

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize database trees
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_MARKETS_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_COVERAGES_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_CLAIMS_TREE)?;
    wasm::db::db_init(cid, crate::INSURANCE_CONTRACT_ENDOWMENT_TREE)?;

    Ok(())
}

/// Get metadata for verification
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
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
            let update: RegisterRiskTypeUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_register_risk_type_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::CreateMarketV1 => {
            let update: CreateMarketUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_create_market_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::UnderwriteV1 => {
            let update: UnderwriteUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_underwrite_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::PurchaseCoverageV1 => {
            let update: PurchaseCoverageUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_purchase_coverage_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::FileClaimV1 => {
            let update: FileClaimUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_file_claim_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::ResolveClaimV1 => {
            let update: ResolveClaimUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_resolve_claim_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::WithdrawPremiumV1 => {
            let update: WithdrawPremiumUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_withdraw_premium_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::UpdatePremiumV1 => {
            let update: UpdatePremiumUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_update_premium_process_update_v1(cid, update)
        }
        // O-Cap enabled functions
        InsuranceMarketFunction::UnderwriteWithCapabilityV1 => {
            let update: UnderwriteWithCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_underwrite_with_capability_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::PurchaseCoverageWithCapabilityV1 => {
            let update: PurchaseCoverageWithCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_purchase_coverage_with_capability_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::PurchaseCoverageWithDAGV1 => {
            let update: PurchaseCoverageWithDAGUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_purchase_coverage_with_dag_process_update_v1(cid, update)
        }
        InsuranceMarketFunction::ResolveClaimWithCapabilityV1 => {
            let update: ResolveClaimWithCapabilityUpdateV1 = deserialize(&update_data[1..])?;
            insurance_market_resolve_claim_with_capability_process_update_v1(cid, update)
        }
    }
}