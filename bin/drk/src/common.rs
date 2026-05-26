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

use dwow_core::{tx::Transaction, util::parse::encode_base10, zk::halo2::Field};
use dwow_sdk::{
    crypto::{
        keypair::{Address, Network, PublicKey, SecretKey, StandardAddress},
        ContractId, DEPLOYOOOR_CONTRACT_ID,
    },
    pasta::pallas,
};

use crate::contract_imports::{MONEY_V3_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
use dwow_serial::{deserialize, serialize};
use prettytable::{format, row, Table};

use crate::contract_imports::money::{MoneyV3Note, TokenId, BALANCE_BASE10_DECIMALS};
use dwow_sdk::crypto::util::FieldElemAsStr;

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

pub fn prettytable_balance(
    balmap: &HashMap<String, u64>,
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Token ID", "Aliases", "Balance"]);

    for (token_id, balance) in balmap.iter() {
        let alias = match alimap.get(token_id) {
            Some(v) => v,
            None => "-",
        };

        table.add_row(row![token_id, alias, encode_base10(*balance, BALANCE_BASE10_DECIMALS)]);
    }

    table
}

pub fn prettytable_coins(
    coins: &[MoneyV3Note],
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![
        "Token ID",
        "Aliases",
        "Value",
        "Spend Hook",
        "User Data",
    ]);

    for coin in coins {
        let alias = match alimap.get(&coin.token_id.to_string()) {
            Some(v) => v,
            None => "-",
        };

        let spend_hook = if coin.spend_hook != pallas::Base::zero() {
            format!("{:?}", coin.spend_hook)
        } else {
            String::from("-")
        };

        let user_data = if coin.user_data != pallas::Base::ZERO {
            bs58::encode(serialize(&coin.user_data)).into_string().to_string()
        } else {
            String::from("-")
        };

        table.add_row(row![
            coin.token_id,
            alias,
            format!(
                "{} ({})",
                coin.value,
                encode_base10(coin.value, BALANCE_BASE10_DECIMALS)
            ),
            spend_hook,
            user_data,
        ]);
    }

    table
}

pub fn prettytable_tokenlist(
    tokens: &[(TokenId, SecretKey, pallas::Base, bool, Option<u32>)],
    alimap: &HashMap<String, String>,
) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row![
        "Token ID",
        "Aliases",
        "Mint Authority",
        "Token Blind",
        "Frozen",
        "Freeze Height",
    ]);

    for (token_id, authority, _blind, frozen, freeze_height) in tokens {
        let alias = match alimap.get(&token_id.to_string()) {
            Some(v) => v,
            None => "-",
        };

        let freeze_height = match freeze_height {
            Some(freeze_height) => freeze_height.to_string(),
            None => String::from("-"),
        };

        table.add_row(row![token_id, alias, authority, "-", frozen, freeze_height]);
    }

    table
}

pub fn prettytable_contract_history(deploy_history: &[(String, String, u32)]) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Transaction Hash", "Type", "Block Height"]);

    for (tx_hash, tx_type, block_height) in deploy_history {
        table.add_row(row![tx_hash, tx_type, block_height]);
    }

    table
}

pub fn prettytable_contract_auth(auths: &[(ContractId, SecretKey, bool, Option<u32>)]) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Contract ID", "Secret Key", "Locked", "Lock Height"]);

    for (contract_id, secret_key, is_locked, lock_height) in auths {
        let lock_height = match lock_height {
            Some(lock_height) => lock_height.to_string(),
            None => String::from("-"),
        };

        table.add_row(row![contract_id, secret_key, is_locked, lock_height]);
    }

    table
}

pub fn prettytable_aliases(alimap: &HashMap<String, TokenId>) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Alias", "Token ID"]);

    for (alias, token_id) in alimap.iter() {
        table.add_row(row![alias, token_id]);
    }

    table
}

pub fn prettytable_scanned_blocks(scanned_blocks: &[(u32, String, String)]) -> Table {
    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["Height", "Hash", "Signing Key"]);
    for (height, hash, signing_key) in scanned_blocks {
        table.add_row(row![height, hash, signing_key]);
    }

    table
}

pub fn pretty_tx(tx: &Transaction) -> String {
    let hash = tx.hash().to_string();

    let mut fees: Vec<String> = vec![];
    let mut fees_total: u64 = 0;
    let mut fees_overflow = false;

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.add_row(row!["", "Contract", "Function"]);

    for (i, call) in tx.calls.iter().enumerate() {
        // NativeToken fee check: contract ID matches and function byte is 0x00 (FeeV1)
        let is_native_fee = call.data.contract_id == *NATIVE_TOKEN_CONTRACT_ID
            && !call.data.data.is_empty()
            && call.data.data[0] == 0x00;

        if is_native_fee {
            if let Ok(fee) = deserialize(&call.data.data[1..9]) {
                fees.push(format!("{} DRK", encode_base10(fee, BALANCE_BASE10_DECIMALS)));
                fees_total = fees_total.checked_add(fee).unwrap_or_else(|| {
                    fees_overflow = true;
                    u64::MAX
                });
            } else {
                fees.push("invalid".to_string());
            }
        }

        let contract_name = match call.data.contract_id {
            id if id == *MONEY_V3_CONTRACT_ID.get().unwrap() => "Money",
            // DAO disabled on this fork
            id if id == *DEPLOYOOOR_CONTRACT_ID => "Deployooor",
            _ => "Custom",
        };

        let calldata = &call.data.data;
        table.add_row(row![
            i.to_string(),
            format!("{} [{}]", call.data.contract_id.to_string(), contract_name),
            // Function code
            if !calldata.is_empty() { calldata[0].to_string() } else { "-".to_string() },
        ]);
    }

    let fee = match fees.len() {
        0 => "-".to_string(),
        1 => fees[0].clone(),
        _ => format!(
            "{} [TOTAL: {}]",
            fees.join(", "),
            if fees_overflow {
                "OVERFLOW".to_string()
            } else {
                format!("{} DRK", encode_base10(fees_total, BALANCE_BASE10_DECIMALS))
            }
        ),
    };

    format!("Hash: {hash}\nFee:  {fee}\n\n{table}")
}
