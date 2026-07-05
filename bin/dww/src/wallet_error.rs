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

/// Wallet-owned error type — replaces dwow_core::Error.
/// Named `Error` so function signatures don't change during extraction.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("Config invalid")]
    ConfigInvalid,

    #[error("Parse failed: {0}")]
    ParseFailed(&'static str),

    #[error("Decode error: {0}")]
    DecodeError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Serialization error: {0}")]
    SerialError(String),

    #[error("Contract error: {0}")]
    ContractError(String),
}

// Transition bridges — convert external error types to wallet Error.
// These impls are REMOVED after dwow_core is fully extracted.

impl From<dwow_core::Error> for Error {
    fn from(e: dwow_core::Error) -> Self {
        Error::Custom(e.to_string())
    }
}

impl From<crate::error::WalletDbError> for Error {
    fn from(e: crate::error::WalletDbError) -> Self {
        Error::DatabaseError(e.to_string())
    }
}

impl From<sled::Error> for Error {
    fn from(e: sled::Error) -> Self {
        Error::DatabaseError(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::SerialError(e.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::DatabaseError(e.to_string())
    }
}

impl From<dwow_sdk::error::DarkTreeError> for Error {
    fn from(e: dwow_sdk::error::DarkTreeError) -> Self {
        Error::Custom(e.to_string())
    }
}

/// Wallet-owned Result type — replaces dwow_core::Result. Same signature.
pub type Result<T> = std::result::Result<T, Error>;

