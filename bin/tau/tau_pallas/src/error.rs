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

//! Error types for tau_pallas

use dwow::rpc::jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult};
use tinyjson::JsonValue;

#[derive(Debug, thiserror::Error)]
pub enum TauPallasError {
    #[error("Due timestamp invalid")]
    InvalidDueTime,
    #[error("Invalid Id")]
    InvalidId,
    #[error("Invalid Data/Params: `{0}` ")]
    InvalidData(String),
    #[error("InternalError")]
    Darkfi(#[from] dwow::error::Error),
    #[error("Json serialization error: `{0}`")]
    JsonError(String),
    #[error("Encryption error: `{0}`")]
    EncryptionError(String),
    #[error("Decryption error: `{0}`")]
    DecryptionError(String),
    #[error("IO Error: `{0}`")]
    IoError(String),
    #[error("Capability verification failed: `{0}`")]
    CapabilityVerificationFailed(String),
    #[error("Missing required capability for task: `{0}`")]
    MissingRequiredCapability(String),
    #[error("Parse failed: `{0}`")]
    ParseFailed(String),
    #[error("Unauthorized: `{0}`")]
    Unauthorized(String),
    #[error("RPC error: `{0}`")]
    RpcError(String),
    #[error("Transaction error: `{0}`")]
    TransactionError(String),
    #[error("Not implemented: `{0}`")]
    NotImplemented(String),
}

pub type TauPallasResult<T> = std::result::Result<T, TauPallasError>;

impl From<std::io::Error> for TauPallasError {
    fn from(err: std::io::Error) -> TauPallasError {
        TauPallasError::IoError(err.to_string())
    }
}

pub fn to_json_result(res: TauPallasResult<JsonValue>, id: u16) -> JsonResult {
    match res {
        Ok(v) => JsonResponse::new(v, id).into(),
        Err(err) => match err {
            TauPallasError::InvalidId => {
                JsonError::new(ErrorCode::InvalidParams, Some("invalid task id".into()), id).into()
            }
            TauPallasError::InvalidData(e) | TauPallasError::JsonError(e) => {
                JsonError::new(ErrorCode::InvalidParams, Some(e), id).into()
            }
            TauPallasError::InvalidDueTime => {
                JsonError::new(ErrorCode::InvalidParams, Some("invalid due time".into()), id).into()
            }
            TauPallasError::EncryptionError(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TauPallasError::DecryptionError(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TauPallasError::Darkfi(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e.to_string()), id).into()
            }
            TauPallasError::IoError(e) => JsonError::new(ErrorCode::InternalError, Some(e), id).into(),
            TauPallasError::CapabilityVerificationFailed(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TauPallasError::MissingRequiredCapability(e) => {
                JsonError::new(ErrorCode::InvalidParams, Some(e), id).into()
            }
            TauPallasError::ParseFailed(e) => {
                JsonError::new(ErrorCode::InvalidParams, Some(e), id).into()
            }
            TauPallasError::Unauthorized(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TauPallasError::RpcError(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TauPallasError::TransactionError(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
            TauPallasError::NotImplemented(e) => {
                JsonError::new(ErrorCode::InternalError, Some(e), id).into()
            }
        },
    }
}
