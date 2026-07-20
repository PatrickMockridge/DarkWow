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

//! Native-Token contract handler for generalized invocation.
//!
//! This handler provides function selectors for Native-Token.
//! The function selectors match those defined in native_token/src/lib.rs:
//! - FeeV1 = 0x00
//! - BurnV1 = 0x02
//! - TransferV1 = 0x03
//! - SpendV1 = 0x04
//! - PoWRewardV1 = 0x05
//!
//! Full calldata building and ZK proof generation requires wallet integration.

use serde_json::Value as JsonValue;

use crate::contract_registry::{ContractHandler, ContractHandlerError, HandlerResult};

/// Native-Token function selectors (matching native_token/src/lib.rs)
const SELECTOR_FEE_V1: u8 = 0x00;
const SELECTOR_BURN_V1: u8 = 0x02;
const SELECTOR_TRANSFER_V1: u8 = 0x03;
const SELECTOR_SPEND_V1: u8 = 0x04;
const SELECTOR_POW_REWARD_V1: u8 = 0x05;

/// Handler for Native-Token contract functions.
pub struct NativeTokenContractHandler;

impl NativeTokenContractHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeTokenContractHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ContractHandler for NativeTokenContractHandler {
    fn contract_id(&self) -> &'static str {
        "native_token"
    }

    fn function_selector(&self, function: &str) -> Option<u8> {
        // MintV1 is intentionally disabled — coin creation is walled off behind
        // PoWRewardV1 (the consensus-locked coinbase). Exposing MintV1 through
        // the handler would tell RPC callers it is available when WASM will
        // hard-reject it as InvalidFunction.
        match function {
            "FeeV1" => Some(SELECTOR_FEE_V1),
            "BurnV1" => Some(SELECTOR_BURN_V1),
            "TransferV1" => Some(SELECTOR_TRANSFER_V1),
            "SpendV1" => Some(SELECTOR_SPEND_V1),
            "PoWRewardV1" => Some(SELECTOR_POW_REWARD_V1),
            _ => None,
        }
    }

    fn build_params(&self, function: &str, _params: JsonValue) -> HandlerResult<Vec<u8>> {
        let selector = self
            .function_selector(function)
            .ok_or_else(|| ContractHandlerError::FunctionNotFound(function.to_string()))?;

        // Full calldata building requires serializing params structs per function.
        // The wallet handles ZK proof generation, so we return just the selector
        // with empty params for now.
        let calldata: Vec<u8> = vec![];

        // Prepend function selector
        let mut result = vec![selector];
        result.extend(calldata);
        Ok(result)
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        // MintV1 is intentionally excluded — coin creation is walled off
        // behind PoWRewardV1 (consensus-locked coinbase).
        vec![
            "FeeV1",
            "BurnV1",
            "TransferV1",
            "SpendV1",
            "PoWRewardV1",
        ]
    }
}