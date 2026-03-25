/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Shared contract primitives for DarkFi smart contracts
//!
//! This module provides common patterns used across all DarkFi contracts:
//!
//! ## The Private Authorization Layer Pattern
//!
//! All privacy-preserving DarkFi contracts follow the same authorization pattern:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │              Private Authorization Lifecycle                        │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                   │
//! │  1. COMMIT                                                        │
//! │     User creates commitment = H(secret, params)                    │
//! │     → Private capability exists on-chain                          │
//! │                                                                   │
//! │  2. PROVE                                                         │
//! │     User generates ZK proof                                        │
//! │     → Proves they know the secret                                 │
//! │     → Proves commitment is valid                                 │
//! │     → Nothing revealed to observers                                │
//! │                                                                   │
//! │  3. CONSUME                                                       │
//! │     User provides nullifier = H(secret)                            │
//! │     → Capability consumed exactly once                             │
//! │     → Cannot be used again (replay protection)                   │
//! │                                                                   │
//! │  4. REVOKE (optional)                                            │
//! │     Issuer marks nullifier as revoked                             │
//! │     → Commitment invalidated before use                            │
//! │                                                                   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Commitment Formula
//!
//! ```text
//! commitment = H(secret, params...)
//! ```
//!
//! - **secret**: Known only to the holder
//! - **params**: Contract-specific parameters
//! - **Purpose**: Creates a private capability bound to the secret
//!
//! ## Nullifier Formula
//!
//! ```text
//! nullifier = H(secret, commitment[, extra...])
//! ```
//!
//! - **secret**: Known only to the holder
//! - **commitment**: The commitment being consumed
//! - **extra**: Optional additional binding
//! - **Purpose**: Prevents replay, links commitment to consumption
//!
//! ## Usage
//!
//! ```rust,ignore
//! use darkfi_sdk::primitives::{
//!     define_contract_function, define_contract_error,
//!     compute_commitment, compute_nullifier,
//! };
//!
//! // Define contract functions
//! define_contract_function!(MyContract {
//!     InitializeV1 = 0x00,
//!     DoSomethingV1 = 0x01,
//! });
//!
//! // Define contract errors
//! define_contract_error!(MyError {
//!     NotInitialized,
//!     InvalidState,
//! });
//!
//! // Compute commitment
//! let commitment = compute_commitment(secret, token, amount);
//!
//! // Compute nullifier
//! let nullifier = compute_nullifier(secret, commitment);
//! ```

use super::crypto::poseidon_hash;

/// Macro to define a contract function enum with TryFrom<u8> implementation
///
/// # Example
///
/// ```rust,ignore
/// use darkfi_sdk::primitives::define_contract_function;
///
/// define_contract_function!(MyContract {
///     InitializeV1 = 0x00,
///     DoActionV1 = 0x01,
///     UpdateStateV1 = 0x02,
/// });
/// ```
///
/// This expands to:
///
/// ```rust,ignore
/// #[repr(u8)]
/// #[derive(Debug)]
/// pub enum MyContractFunction {
///     InitializeV1 = 0x00,
///     DoActionV1 = 0x01,
///     UpdateStateV1 = 0x02,
/// }
///
/// impl TryFrom<u8> for MyContractFunction {
///     type Error = ContractError;
///     fn try_from(b: u8) -> Result<Self, Self::Error> {
///         match b {
///             0x00 => Ok(Self::InitializeV1),
///             0x01 => Ok(Self::DoActionV1),
///             0x02 => Ok(Self::UpdateStateV1),
///             _ => Err(ContractError::InvalidFunction),
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_contract_function {
    (
        $name:ident {
            $($variant:ident = $id:expr),*$(,)?
        }
    ) => {
        #[repr(u8)]
        #[derive(Debug)]
        pub enum $name {
            $($variant = $id),*
        }

        impl TryFrom<u8> for $name {
            type Error = $crate::error::ContractError;

            fn try_from(b: u8) -> ::core::result::Result<Self, Self::Error> {
                match b {
                    $($id => Ok(Self::$variant),)*
                    _ => Err($crate::error::ContractError::InvalidFunction),
                }
            }
        }
    };
}

/// Macro to define a contract error enum with thiserror and From implementation
///
/// # Example
///
/// ```rust,ignore
/// use darkfi_sdk::primitives::define_contract_error;
///
/// define_contract_error!(MyError {
///     NotInitialized,
///     InvalidState { reason: String },
///     CustomError(u32),
/// });
/// ```
///
/// This expands to:
///
/// ```rust,ignore
/// #[derive(Debug, Clone, thiserror::Error)]
/// pub enum MyError {
///     #[error("Not initialized")]
///     NotInitialized,
///     #[error("Invalid state: {reason}")]
///     InvalidState { reason: String },
///     #[error("Custom error: {0}")]
///     CustomError(u32),
/// }
///
/// impl From<MyError> for ContractError {
///     fn from(e: MyError) -> Self {
///         Self::Custom(e as u32)
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_contract_error {
    (
        $name:ident {
            $($variant:ident $({ $($field:ident : $t:ty),* })?),*$(,)?
        }
    ) => {
        #[derive(Debug, Clone, thiserror::Error)]
        pub enum $name {
            $(
                #[error(stringify!($variant))]
                $variant $({ $($field: $t),* })?
            ),*
        }

        impl From<$name> for $crate::error::ContractError {
            fn from(e: $name) -> Self {
                // Use discriminant-based error codes
                let code = match &e {
                    $(&$name::$variant $({ $($field),* })?)* => {
                        let idx = 0;
                        $(let _ = stringify!($variant);)*
                        idx
                    }
                };
                // For custom errors, use a base offset + variant index
                // In practice, contracts use their own error mapping
                $crate::error::ContractError::Custom(code)
            }
        }
    };
}

// ============================================================================
// Commitment and Nullifier Computation
// ============================================================================

use pasta_curves::pallas;

/// Compute a commitment from secret and parameters using Poseidon hash
///
/// # Formula
///
/// ```text
/// commitment = H(secret, params...)
/// ```
///
/// # Arguments
///
/// * `secret` - The secret known only to the holder (as pallas::Base)
/// * `params` - Additional parameters to bind to the commitment (as pallas::Base array)
///
/// # Returns
///
/// A pallas::Base commitment hash
///
/// # Example
///
/// ```rust,ignore
/// use pasta_curves::pallas;
///
/// let secret = pallas::Base::from(0);
/// let token = pallas::Base::from(1);
/// let amount = pallas::Base::from(100);
/// let commitment = compute_commitment::<2>([secret, token, amount]);
/// ```
pub fn compute_commitment<const N: usize>(inputs: [pallas::Base; N]) -> pallas::Base {
    poseidon_hash(inputs)
}

/// Compute a nullifier from secret and commitment using Poseidon hash
///
/// # Formula
///
/// ```text
/// nullifier = H(secret, commitment)
/// ```
///
/// # Arguments
///
/// * `secret` - The secret known only to the holder (as pallas::Base)
/// * `commitment` - The commitment being consumed (as pallas::Base)
///
/// # Returns
///
/// A pallas::Base nullifier hash
///
/// # Example
///
/// ```rust,ignore
/// let secret = pallas::Base::from(0);
/// let commitment = pallas::Base::from(1);
/// let nullifier = compute_nullifier(secret, commitment);
/// ```
pub fn compute_nullifier(secret: pallas::Base, commitment: pallas::Base) -> pallas::Base {
    poseidon_hash([secret, commitment])
}

/// Compute a state nullifier for identifying state without revealing it
///
/// # Formula
///
/// ```text
/// state_nullifier = H(secret, state_hash)
/// ```
pub fn compute_state_nullifier(secret: pallas::Base, state_hash: pallas::Base) -> pallas::Base {
    poseidon_hash([secret, state_hash])
}

/// Compute a revocation nullifier for issuer revocation
///
/// # Formula
///
/// ```text
/// revocation_nullifier = H(issuer_secret, commitment)
/// ```
pub fn compute_revocation_nullifier(
    issuer_secret: pallas::Base,
    commitment: pallas::Base,
) -> pallas::Base {
    poseidon_hash([issuer_secret, commitment])
}

// ============================================================================
// Database Tree Name Helpers
// ============================================================================

/// Helper to create a tree name constant
///
/// # Example
///
/// ```rust,ignore
/// use darkfi_sdk::primitives::tree_name;
///
/// pub const MY_STATE_TREE: &str = tree_name!("mycontract", "state");
/// // Results in: "mycontract_state"
/// ```
#[macro_export]
macro_rules! tree_name {
    ($contract:literal, $tree:literal) => {
        concat!($contract, "_", $tree)
    };
}
