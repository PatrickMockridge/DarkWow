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

//! Cross-contract validation helpers for parent contracts calling money_v3.
//!
//! These functions are always compiled (not behind `no-entrypoint`) so that
//! caller contracts can import and use them regardless of feature flags.

use dwow_sdk::{crypto::ContractId, msg, pasta::pallas};
use dwow_serial::deserialize;

use crate::{error::MoneyV3Error, model::TransferParamsV1};

/// Validate a money_v3::transfer_v1 child call's value_commit against the expected amount.
///
/// Privacy-preserving: uses Poseidon commitment comparison instead of plaintext values.
/// The parent contract derives `value_blind` deterministically from its own unique state
/// (e.g., `poseidon_hash([value, nullifier])`) and recomputes the expected `value_commit`.
/// The child transfer must use the same blind when generating its BlindOutput_V1 ZK proof.
///
/// Call from parent contracts after verifying `child_call.data[0] == 0x04`.
pub fn validate_child_value_commit(
    child_call_data: &[u8],
    expected_value: u64,
    value_blind: pallas::Base,
) -> Result<(), crate::ContractError> {
    if child_call_data.is_empty() {
        return Err(crate::ContractError::InvalidFunction)
    }

    let params: TransferParamsV1 = deserialize(&child_call_data[1..])
        .map_err(|_| crate::ContractError::InvalidFunction)?;

    let expected_commit = dwow_sdk::crypto::poseidon_hash([
        pallas::Base::from(expected_value),
        value_blind,
    ]);

    for output in &params.outputs {
        if output.value_commit == expected_commit {
            return Ok(());
        }
    }

    Err(MoneyV3Error::ValueMismatch.into())
}

/// Validate that a child call targets the expected contract.
///
/// Prevents cross-contract routing attacks where a transaction builder
/// could route a child call to the wrong contract with the same opcode
/// (e.g., MoneyV3::TransferV1 and Attestation::VerifyClaimV1 both use 0x04).
///
/// Call from parent contracts after validating `child_call.data[0]`.
pub fn validate_child_contract_id(
    child_contract_id: &ContractId,
    expected_contract_id: &ContractId,
) -> Result<(), crate::ContractError> {
    if child_contract_id != expected_contract_id {
        msg!(
            "[money_v3::validate_child_contract_id] Error: Expected contract_id {:?}, got {:?}",
            expected_contract_id.inner(),
            child_contract_id.inner()
        );
        return Err(MoneyV3Error::InvalidChildContractId.into())
    }
    Ok(())
}
