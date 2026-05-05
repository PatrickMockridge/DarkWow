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

//! Atomic Swap Contract Errors

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum AtomicSwapError {
    #[error("Invalid children indexes for child call")]
    InvalidChildrenIndexes,

    #[error("Invalid child call")]
    InvalidChildCall,
}

impl From<AtomicSwapError> for ContractError {
    fn from(e: AtomicSwapError) -> Self {
        match e {
            AtomicSwapError::InvalidChildrenIndexes => Self::Custom(1),
            AtomicSwapError::InvalidChildCall => Self::Custom(2),
        }
    }
}
