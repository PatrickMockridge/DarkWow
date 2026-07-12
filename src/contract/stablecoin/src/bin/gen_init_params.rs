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

//! Generate serialized InitializeParams for stablecoin deploy_ix.
//!
//! Usage:
//!   cargo run -p dwow_stablecoin_contract --bin gen_init_params --features client -- \
//!     <pn_contract_id_hex> <token_authority_pub_hex> [output_file]
//!
//! If output_file is omitted, writes to stdout.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::process;

use dwow_serial::Encodable;
use dwow_sdk::crypto::{ContractId, PublicKey};
use dwow_stablecoin_contract::model::{
    CollateralParams, CollateralType, DeadManAction, DeadManSwitchConfig, InitializeParams,
    StablecoinModel,
};

fn parse_hex_32(s: &str) -> Result<[u8; 32], String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 64 {
        return Err(format!("expected 64 hex chars, got {}", s.len()));
    }
    let mut arr = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        arr[i] = (hi << 4) | lo;
    }
    Ok(arr)
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("invalid hex char: {}", b as char)),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <pn_contract_id_hex> <token_authority_pub_hex> [output_file]",
            args[0]
        );
        process::exit(1);
    }

    let pn_contract_id_bytes = parse_hex_32(&args[1]).unwrap_or_else(|e| {
        eprintln!("Error parsing pn_contract_id: {e}");
        process::exit(1);
    });
    let pn_contract_id = ContractId::from_bytes(pn_contract_id_bytes).unwrap_or_else(|e| {
        eprintln!("Error decoding pn_contract_id: {e:?}");
        process::exit(1);
    });

    let token_authority_pub_bytes = parse_hex_32(&args[2]).unwrap_or_else(|e| {
        eprintln!("Error parsing token_authority_pub: {e}");
        process::exit(1);
    });
    let token_authority_pub = PublicKey::from_bytes(token_authority_pub_bytes).unwrap_or_else(|e| {
        eprintln!("Error decoding token_authority_pub: {e:?}");
        process::exit(1);
    });

    let params = InitializeParams {
        model: StablecoinModel::PooledDebt,
        min_collateralization_ratio: 15000, // 150%
        liquidation_threshold: 13000,        // 130%
        liquidation_penalty: 1000,           // 10%
        base_rate: 500,                      // 5% annual
        pi_kp: 1000,
        pi_ki: 100,
        twap_window: 3600,                   // 1 hour
        price_deviation_threshold: 500,      // 5%
        collateral_params: vec![CollateralParams {
            collateral_type: CollateralType::Drk,
            haircut: 10000,                  // 100% (no discount)
            liquidation_threshold: 13000,    // 130%
            max_debt_share: 10000,           // 100% (single-collateral pool)
        }],
        dead_man_switch: DeadManSwitchConfig {
            enabled: false,
            timeout_blocks: 43200,           // ~1 day at 2s blocks
            action: DeadManAction::LiquidateAll,
            last_action_block: 0,
        },
        token_authority_pub,
        create_token: false,                 // USDx created manually via PN
        token_symbol: *b"USDx\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        deployer_auth: dwow_sdk::pasta::pallas::Base::zero(),
        promissory_note_contract_id: pn_contract_id,
    };

    let mut buf = vec![];
    params.encode(&mut buf).unwrap_or_else(|e| {
        eprintln!("Error encoding params: {e}");
        process::exit(1);
    });

    let output = &args.get(3).cloned();
    match output {
        Some(path) => {
            fs::write(path, &buf).unwrap_or_else(|e| {
                eprintln!("Error writing to {path}: {e}");
                process::exit(1);
            });
            println!("Wrote {} bytes to {path}", buf.len());
        }
        None => {
            io::stdout().write_all(&buf).unwrap_or_else(|e| {
                eprintln!("Error writing to stdout: {e}");
                process::exit(1);
            });
        }
    }
}
