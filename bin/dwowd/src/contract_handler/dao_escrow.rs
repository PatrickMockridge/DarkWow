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

//! DAO-Escrow contract handler for generalized invocation.
//!
//! This handler provides function selectors for DAO-Escrow.
//! The function selectors match those defined in dao_escrow/src/lib.rs:
//! - InitializeV1 = 0x00
//! - UpdateV1 = 0x01
//! - PayPremiumV1 = 0x02
//! - WithdrawV1 = 0x03
//! - EndowmentWithdrawV1 = 0x04
//! - TreasurySpendV1 = 0x05
//! - EnableDrainProtectionV1 = 0x06
//! - ProposeClaimV1 = 0x07
//! - VoteClaimV1 = 0x08
//! - ExecuteClaimV1 = 0x09
//! - RegisterCapabilityRequirementV1 = 0x0a
//! - VerifyMemberCapabilityV1 = 0x0b
//! - ResolveDisputeV1 = 0x0c
//! - CancelClaimV1 = 0x0d
//! - SetGovernanceConfigV1 = 0x0e
//!
//! Full calldata building and ZK proof generation requires wallet integration.

use serde_json::Value as JsonValue;

use crate::contract_registry::{ContractHandler, ContractHandlerError, HandlerResult};

/// DAO-Escrow function selectors (matching dao_escrow/src/lib.rs)
const SELECTOR_INITIALIZE_V1: u8 = 0x00;
const SELECTOR_UPDATE_V1: u8 = 0x01;
const SELECTOR_PAY_PREMIUM_V1: u8 = 0x02;
const SELECTOR_WITHDRAW_V1: u8 = 0x03;
const SELECTOR_ENDOWMENT_WITHDRAW_V1: u8 = 0x04;
const SELECTOR_TREASURY_SPEND_V1: u8 = 0x05;
const SELECTOR_ENABLE_DRAIN_PROTECTION_V1: u8 = 0x06;
const SELECTOR_PROPOSE_CLAIM_V1: u8 = 0x07;
const SELECTOR_VOTE_CLAIM_V1: u8 = 0x08;
const SELECTOR_EXECUTE_CLAIM_V1: u8 = 0x09;
const SELECTOR_REGISTER_CAPABILITY_REQUIREMENT_V1: u8 = 0x0a;
const SELECTOR_VERIFY_MEMBER_CAPABILITY_V1: u8 = 0x0b;
const SELECTOR_RESOLVE_DISPUTE_V1: u8 = 0x0c;
const SELECTOR_CANCEL_CLAIM_V1: u8 = 0x0d;
const SELECTOR_SET_GOVERNANCE_CONFIG_V1: u8 = 0x0e;

/// Handler for DAO-Escrow contract functions.
pub struct DaoEscrowContractHandler;

impl DaoEscrowContractHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DaoEscrowContractHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ContractHandler for DaoEscrowContractHandler {
    fn contract_id(&self) -> &'static str {
        "dao_escrow"
    }

    fn function_selector(&self, function: &str) -> Option<u8> {
        match function {
            "InitializeV1" => Some(SELECTOR_INITIALIZE_V1),
            "UpdateV1" => Some(SELECTOR_UPDATE_V1),
            "PayPremiumV1" => Some(SELECTOR_PAY_PREMIUM_V1),
            "WithdrawV1" => Some(SELECTOR_WITHDRAW_V1),
            "EndowmentWithdrawV1" => Some(SELECTOR_ENDOWMENT_WITHDRAW_V1),
            "TreasurySpendV1" => Some(SELECTOR_TREASURY_SPEND_V1),
            "EnableDrainProtectionV1" => Some(SELECTOR_ENABLE_DRAIN_PROTECTION_V1),
            "ProposeClaimV1" => Some(SELECTOR_PROPOSE_CLAIM_V1),
            "VoteClaimV1" => Some(SELECTOR_VOTE_CLAIM_V1),
            "ExecuteClaimV1" => Some(SELECTOR_EXECUTE_CLAIM_V1),
            "RegisterCapabilityRequirementV1" => Some(SELECTOR_REGISTER_CAPABILITY_REQUIREMENT_V1),
            "VerifyMemberCapabilityV1" => Some(SELECTOR_VERIFY_MEMBER_CAPABILITY_V1),
            "ResolveDisputeV1" => Some(SELECTOR_RESOLVE_DISPUTE_V1),
            "CancelClaimV1" => Some(SELECTOR_CANCEL_CLAIM_V1),
            "SetGovernanceConfigV1" => Some(SELECTOR_SET_GOVERNANCE_CONFIG_V1),
            _ => None,
        }
    }

    fn build_params(&self, function: &str, _params: JsonValue) -> HandlerResult<Vec<u8>> {
        let selector = self
            .function_selector(function)
            .ok_or_else(|| ContractHandlerError::FunctionNotFound(function.to_string()))?;

        // TODO: Full calldata building requires serializing params structs.
        // The dao_escrow contract has pre-existing bugs preventing this.
        // For now, return just the selector.
        let calldata: Vec<u8> = vec![];

        // Prepend function selector
        let mut result = vec![selector];
        result.extend(calldata);
        Ok(result)
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        vec![
            "InitializeV1",
            "UpdateV1",
            "PayPremiumV1",
            "WithdrawV1",
            "EndowmentWithdrawV1",
            "TreasurySpendV1",
            "EnableDrainProtectionV1",
            "ProposeClaimV1",
            "VoteClaimV1",
            "ExecuteClaimV1",
            "RegisterCapabilityRequirementV1",
            "VerifyMemberCapabilityV1",
            "ResolveDisputeV1",
            "CancelClaimV1",
            "SetGovernanceConfigV1",
        ]
    }
}
