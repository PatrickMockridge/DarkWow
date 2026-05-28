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

//! Cross-contract validation helpers shared by all contracts.
//!
//! These functions prevent common classes of cross-contract bugs:
//! routing attacks (wrong contract for a given opcode) and amount
//! verification failures.

use crate::crypto::ContractId;
use crate::error::{ContractError, ContractResult};
use crate::msg;

/// Verify a child call is routed to the expected contract.
///
/// Prevents cross-contract routing attacks where a transaction builder
/// routes a child call to the wrong contract that happens to share the
/// same function opcode (e.g., PromissoryNote::TransferV1 and
/// Attestation::VerifyClaimV1 both use `0x04`).
///
/// Call from parent contracts **after** validating `child_call.data[0]`.
pub fn validate_child_contract_id(
    child_contract_id: &ContractId,
    expected_contract_id: &ContractId,
) -> ContractResult {
    if child_contract_id != expected_contract_id {
        msg!(
            "[validate_child_contract_id] Error: Expected contract_id {:?}, got {:?}",
            expected_contract_id.inner(),
            child_contract_id.inner()
        );
        return Err(ContractError::Custom(255))
    }
    Ok(())
}
