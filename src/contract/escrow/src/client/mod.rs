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

//! Escrow contract client module

pub mod create_escrow_v1;
pub mod fund_v1;
pub mod claim_v1;
pub mod refund_v1;
pub mod cancel_v1;

use dwow_sdk::contract_client::ContractClient;

/// Escrow contract client — implements ContractClient for the wallet's
/// generic dispatch. Lives in the contract crate, NOT the wallet.
pub struct EscrowClient;

impl ContractClient for EscrowClient {
    fn build(&self, function: &str, params: &str) -> Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        match function {
            "cancel" => Ok((vec![], vec![])),  // Non-ZK
            _ => Err(format!("Escrow: unsupported function '{}'", function)),
        }
    }
}

