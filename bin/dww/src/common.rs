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

use std::collections::HashMap;

use crate::wallet_util::encode_base10;
use dwow_sdk::crypto::keypair::{Address, Network, PublicKey, SecretKey, StandardAddress};
use dwow_sdk::crypto::FuncId;

use prettytable::{format, row, Table};

use crate::walletdb::CapRecord;
const CAP_VALUE_DECIMALS: usize = 8;

pub fn prettytable_addrs(
    network: Network,
    addresses: &[(u64, PublicKey, SecretKey, u64)],
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Key ID", "Address", "Public Key", "Secret Key", "Is Default"]);
    for (key_id, public_key, secret_key, is_default) in addresses {
        let is_default = match is_default {
            1 => "*",
            _ => "",
        };

        let address: Address = StandardAddress::from_public(network, *public_key).into();
        table.add_row(row![key_id, address, public_key, secret_key, is_default]);
    }

    table
}

// prettytable_balance REMOVED — dead code, never called (HAZOP round 2)

pub fn prettytable_held_capabilities(
    caps: &[CapRecord],
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![
        "Asset ID",
        "Aliases",
        "Value",
        "Spend Hook",
        "User Data",
    ]);

    for cap in caps {
        let asset_str = bs58::encode(&cap.asset_id.to_bytes()).into_string();
        let alias = match alimap.get(&asset_str) {
            Some(v) => v,
            None => "-",
        };

        let spend_hook = match cap.spend_hook {
            Some(hook) if hook != FuncId::none() =>
                bs58::encode(&hook.to_bytes()).into_string(),
            _ => String::from("-"),
        };

        let user_data = match cap.user_data {
            Some(data) if data != [0u8; 32] => bs58::encode(data).into_string(),
            _ => String::from("-"),
        };

        table.add_row(row![
            asset_str,
            alias,
            format!(
                "{} ({})",
                cap.value,
                encode_base10(cap.value, CAP_VALUE_DECIMALS)
            ),
            spend_hook,
            user_data,
        ]);
    }

    table
}

// prettytable_tokenlist REMOVED — dead code (zero callers).
// prettytable_contract_history / prettytable_contract_auth / prettytable_scanned_blocks
// REMOVED — dead code (zero callers).
// prettytable_aliases REMOVED — dead code (zero callers).


// pretty_tx REMOVED — dead code (zero callers).
// PROMISSORY_NOTE_CONTRACT_ID match arm was the last per-contract
// display label in common.rs. Contract name display is now handled
// generically via manifests.
