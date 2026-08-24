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

//! Cross-contract validation helpers for parent contracts calling promissory_note.
//!
//! These functions are always compiled (not behind `no-entrypoint`) so that
//! caller contracts can import and use them regardless of feature flags.

use dwow_sdk::{
    crypto::{pedersen_commitment_u64, util::fp_mod_fv, Blind, ContractId},
    msg,
    pasta::pallas,
};

use crate::{error::PromissoryNoteError, model::{RedeemParamsV1, TransferParamsV1}};

/// Validate a promissory_note::transfer_v1 child call's value_commit against the expected amount.
///
/// Privacy-preserving: uses Pedersen commitment comparison instead of plaintext values.
/// The parent contract derives `blind_seed` deterministically from its own unique state
/// (e.g., `poseidon_hash([value, nullifier])`) as a `pallas::Base`, and this function
/// converts it to a `pallas::Scalar` via `fp_mod_fv` for the Pedersen commitment.
/// The child transfer must use the same derivation when generating its BlindOutput_V1 ZK proof.
///
/// Call from parent contracts after verifying `child_call.data[0] == 0x04`.
pub fn validate_child_value_commit(
    child_call_data: &[u8],
    expected_value: u64,
    blind_seed: pallas::Base,
) -> Result<(), crate::ContractError> {
    if child_call_data.is_empty() {
        return Err(crate::ContractError::InvalidFunction)
    }

    let params = TransferParamsV1::decode(&child_call_data[1..])
        .map_err(|_| crate::ContractError::InvalidFunction)?;

    // Convert the base-field blind seed to a scalar for Pedersen.
    // spec dispensation: type-system.md §2.3 — base field < scalar field, conversion guaranteed valid.
    #[expect(clippy::expect_used, reason = "type-system.md §2.3 — base field < scalar field, conversion guaranteed valid")]
    let value_blind = Blind(fp_mod_fv(blind_seed)
        .expect("base field to scalar: mathematically guaranteed valid"));
    let expected_commit = pedersen_commitment_u64(expected_value, value_blind);

    for output in &params.outputs {
        if output.value_commit == expected_commit {
            return Ok(());
        }
    }

    Err(PromissoryNoteError::ValueMismatch.into())
}

/// Validate that a child call targets the expected contract.
///
/// Prevents cross-contract routing attacks where a transaction builder
/// could route a child call to the wrong contract with the same opcode
/// (e.g., PromissoryNote::TransferV1 and Attestation::VerifyClaimV1 both use 0x04).
///
/// Call from parent contracts after validating `child_call.data[0]`.
pub fn validate_child_contract_id(
    child_contract_id: &ContractId,
    expected_contract_id: &ContractId,
) -> Result<(), crate::ContractError> {
    if child_contract_id != expected_contract_id {
        msg!(
            "[promissory_note::validate_child_contract_id] Error: Expected contract_id {:?}, got {:?}",
            expected_contract_id.inner(),
            child_contract_id.inner()
        );
        return Err(PromissoryNoteError::InvalidChildContractId.into())
    }
    Ok(())
}

/// Validate a promissory_note::redeem_v1 child call and return the receipt commitment's
/// value_commit and token_commit for parent inspection.
///
/// Call from parent contracts after verifying `child_call.data[0] == 0x01`.
/// The ZK circuit constrains `value = 0` as a public input, so the parent
/// does not need to independently verify the zero-value property — it can trust
/// the ZK proof verification performed by the host.
pub fn validate_child_redeem_v1(
    child_call_data: &[u8],
) -> Result<(pallas::Point, pallas::Base), crate::ContractError> {
    if child_call_data.is_empty() {
        return Err(crate::ContractError::InvalidFunction)
    }

    let params = RedeemParamsV1::decode(&child_call_data[1..])
        .map_err(|_| crate::ContractError::InvalidFunction)?;

    Ok((params.output.value_commit, params.output.token_commit))
}
