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

//! Typed helpers for transition payload encoding/decoding.
//!
//! Payload format:
//! - byte 0: function code
//! - bytes 1..: `darkfi_serial` encoded params

use dwow_serial::{deserialize, Decodable, Encodable};

use super::{IntentConsumeCallV1, IntentConsumeTransitionV1, IntentPostTransitionV1};
use crate::ContractError;

fn payload_error(msg: &str) -> ContractError {
    ContractError::IoError(msg.to_string())
}

/// Encode a payload with a function code.
pub fn encode_payload<T: Encodable>(func_code: u8, params: &T) -> Result<Vec<u8>, ContractError> {
    let mut out = vec![func_code];
    params.encode(&mut out)?;
    Ok(out)
}

/// Decode a payload and verify the function code.
pub fn decode_payload<T: Decodable>(data: &[u8], expected_func_code: u8) -> Result<T, ContractError> {
    if data.is_empty() {
        return Err(payload_error("Payload is empty"))
    }
    if data[0] != expected_func_code {
        return Err(payload_error("Payload function code mismatch"))
    }
    Ok(deserialize(&data[1..])?)
}

/// Function IDs for a generic intent-set contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IntentSetFunctionV1 {
    /// Post a new intent
    PostV1 = 0x00,
    /// Cancel an existing intent
    CancelV1 = 0x01,
    /// Fill (consume) an intent
    FillV1 = 0x02,
}

impl IntentSetFunctionV1 {
    /// Try from byte value.
    pub fn from_u8(v: u8) -> Result<Self, ContractError> {
        match v {
            0x00 => Ok(Self::PostV1),
            0x01 => Ok(Self::CancelV1),
            0x02 => Ok(Self::FillV1),
            _ => Err(payload_error("Unknown intent-set function code")),
        }
    }
}

impl From<IntentSetFunctionV1> for u8 {
    fn from(v: IntentSetFunctionV1) -> Self {
        v as Self
    }
}

/// Encode an intent-set post payload.
pub fn encode_intent_set_post_v1(
    transition: &IntentPostTransitionV1,
) -> Result<Vec<u8>, ContractError> {
    encode_payload(IntentSetFunctionV1::PostV1 as u8, transition)
}

/// Decode an intent-set post payload.
pub fn decode_intent_set_post_v1(data: &[u8]) -> Result<IntentPostTransitionV1, ContractError> {
    decode_payload(data, IntentSetFunctionV1::PostV1 as u8)
}

/// Encode an intent-set cancel payload (uses consume transition).
pub fn encode_intent_set_cancel_v1(
    transition: &IntentConsumeTransitionV1,
) -> Result<Vec<u8>, ContractError> {
    encode_payload(IntentSetFunctionV1::CancelV1 as u8, transition)
}

/// Decode an intent-set cancel payload.
pub fn decode_intent_set_cancel_v1(data: &[u8]) -> Result<IntentConsumeTransitionV1, ContractError> {
    decode_payload(data, IntentSetFunctionV1::CancelV1 as u8)
}

/// Encode an intent-set fill payload (uses consume call).
pub fn encode_intent_set_fill_v1(consume: &IntentConsumeCallV1) -> Result<Vec<u8>, ContractError> {
    encode_payload(IntentSetFunctionV1::FillV1 as u8, consume)
}

/// Decode an intent-set fill payload.
pub fn decode_intent_set_fill_v1(data: &[u8]) -> Result<IntentConsumeCallV1, ContractError> {
    decode_payload(data, IntentSetFunctionV1::FillV1 as u8)
}
