/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 0-2026 Dyne.org foundation
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

use darkfi_serial::{deserialize, serialize};
use tinyjson::JsonValue;

use crate::{
    contract_registry::{ContractHandler, ContractHandlerError, HandlerResult},
    RpcError,
};

/// DAO-Escrow function selectors
mod functions {
    /// InitializeV1 function selector
    pub const INITIALIZE_V1: u8 = 0x00;
    /// UpdateV1 function selector
    pub const UPDATE_V1: u8 = 0x01;
    /// PayPremiumV1 function selector
    pub const PAY_PREMIUM_V1: u8 = 0x02;
    /// WithdrawV1 function selector
    pub const WITHDRAW_V1: u8 = 0x03;
    /// EndowmentWithdrawV1 function selector
    pub const ENDOWMENT_WITHDRAW_V1: u8 = 0x04;
    /// TreasurySpendV1 function selector
    pub const TREASURY_SPEND_V1: u8 = 0x05;
    /// EnableDrainProtectionV1 function selector
    pub const ENABLE_DRAIN_PROTECTION_V1: u8 = 0x06;
}

/// DAO-Escrow contract handler.
///
/// This handler supports the following functions:
/// - InitializeV1 (0x00): Create a new DAO-Escrow instance
/// - UpdateV1 (0x01): Update DAO-Escrow parameters
/// - PayPremiumV1 (0x02): Pay premium as a member
/// - WithdrawV1 (0x03): Owner withdrawal
/// - EndowmentWithdrawV1 (0x04): Endowment withdrawal (insurance)
/// - TreasurySpendV1 (0x05): Treasury spending
/// - EnableDrainProtectionV1 (0x06): Enable DrainProtection
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
            "InitializeV1" => Some(functions::INITIALIZE_V1),
            "UpdateV1" => Some(functions::UPDATE_V1),
            "PayPremiumV1" => Some(functions::PAY_PREMIUM_V1),
            "WithdrawV1" => Some(functions::WITHDRAW_V1),
            "EndowmentWithdrawV1" => Some(functions::ENDOWMENT_WITHDRAW_V1),
            "TreasurySpendV1" => Some(functions::TREASURY_SPEND_V1),
            "EnableDrainProtectionV1" => Some(functions::ENABLE_DRAIN_PROTECTION_V1),
            _ => None,
        }
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
        ]
    }

    fn build_params(&self, function: &str, params: JsonValue) -> HandlerResult<Vec<u8>> {
        let selector = self.function_selector(function)
            .ok_or_else(|| ContractHandlerError::FunctionNotFound(function.to_string()))?;

        // The calldata is: [selector_byte, serialized_params]
        let params_bytes = match function {
            "InitializeV1" => self.build_initialize_params(&params)?,
            "UpdateV1" => self.build_update_params(&params)?,
            "PayPremiumV1" => self.build_pay_premium_params(&params)?,
            "WithdrawV1" => self.build_withdraw_params(&params)?,
            "EnableDrainProtectionV1" => self.build_enable_drain_protection_params(&params)?,
            _ => {
                return Err(ContractHandlerError::FunctionNotFound(format!(
                    "Function {} not yet implemented in handler",
                    function
                )))
            }
        };

        let mut calldata = vec![selector];
        calldata.extend(params_bytes);
        Ok(calldata)
    }
}

impl DaoEscrowContractHandler {
    /// Build InitializeV1 params from JSON.
    ///
    /// Expected JSON format:
    /// ```json
    /// {
    ///     "owner_pubkey": "base58publickey",
    ///     "endowment_token_id": "base58tokenid",
    ///     "enable_drain_protection": true
    /// }
    /// ```
    fn build_initialize_params(&self, params: &JsonValue) -> HandlerResult<Vec<u8>> {
        // TODO: Proper parsing with validation
        // For now, this is a placeholder that returns empty params
        // The actual implementation needs proper PublicKey and token ID parsing
        Ok(vec![])
    }

    /// Build UpdateV1 params from JSON.
    fn build_update_params(&self, params: &JsonValue) -> HandlerResult<Vec<u8>> {
        Ok(vec![])
    }

    /// Build PayPremiumV1 params from JSON.
    ///
    /// Expected JSON format:
    /// ```json
    /// {
    ///     "dao_escrow_bulla": "base58bulla",
    ///     "value": 1000,
    ///     "token_id": "base58tokenid"
    /// }
    /// ```
    fn build_pay_premium_params(&self, params: &JsonValue) -> HandlerResult<Vec<u8>> {
        Ok(vec![])
    }

    /// Build WithdrawV1 params from JSON.
    ///
    /// Expected JSON format:
    /// ```json
    /// {
    ///     "dao_escrow_bulla": "base58bulla",
    ///     "value": 500,
    ///     "recipient_pubkey": "base58publickey"
    /// }
    /// ```
    fn build_withdraw_params(&self, params: &JsonValue) -> HandlerResult<Vec<u8>> {
        Ok(vec![])
    }

    /// Build EnableDrainProtectionV1 params from JSON.
    ///
    /// Expected JSON format:
    /// ```json
    /// {
    ///     "dao_escrow_bulla": "base58bulla",
    ///     "drain_protection_bulla": "base58bulla"
    /// }
    /// ```
    fn build_enable_drain_protection_params(&self, params: &JsonValue) -> HandlerResult<Vec<u8>> {
        Ok(vec![])
    }
}
