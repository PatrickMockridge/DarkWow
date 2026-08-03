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

//! DAO-Escrow Proof Generation Binary
//!
//! Generates ZK proofs for DAO-Escrow contract functions.
//!
//! Usage:
//!   cargo run -p darkfi-contract-test-harness --bin prove_dao_escrow -- \
//!     init <dao_bulla> <owner_secret> <token_id> <bulla_blind>
//!   cargo run -p darkfi-contract-test-harness --bin prove_dao_escrow -- \
//!     pay_premium <escrow_bulla> <member_secret> <value> <token_id> <expiry>

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::pasta_prelude::PrimeField,
    pasta::pallas,
};
use dwow_dao_escrow_contract::client::{
    init::{init_v1_proof, InitV1CallData},
    pay_premium::{pay_premium_v1_proof, PayPremiumV1CallData},
};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  prove_dao_escrow init <dao_bulla_hex> <owner_secret_hex> <token_id_hex> <bulla_blind_hex>");
        eprintln!("  prove_dao_escrow pay_premium <escrow_bulla_hex> <member_secret_hex> <value_u64> <token_id_hex> <expiry_u64>");
        std::process::exit(1);
    }

    let subcmd = &args[1];

    match subcmd.as_str() {
        "init" => run_init(&args[2..]),
        "pay_premium" => run_pay_premium(&args[2..]),
        _ => {
            eprintln!("Unknown subcommand: {}", subcmd);
            std::process::exit(1);
        }
    }
}

fn run_init(args: &[String]) {
    if args.len() != 4 {
        eprintln!("Usage: init <dao_bulla_hex> <owner_secret_hex> <token_id_hex> <bulla_blind_hex>");
        std::process::exit(1);
    }

    let dao_bulla = parse_hex_to_base(&args[0]).expect("Invalid dao_bulla hex");
    let owner_secret = parse_hex_to_base(&args[1]).expect("Invalid owner_secret hex");
    let token_id = parse_hex_to_base(&args[2]).expect("Invalid token_id hex");
    let bulla_blind = parse_hex_to_base(&args[3]).expect("Invalid bulla_blind hex");

    // Load circuit binary
    let init_bin = include_bytes!("../../dao_escrow/proof/init.zk.bin");
    let zkbin = ZkBinary::decode(init_bin, false).unwrap();

    let circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(&zkbin).unwrap(), &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build failed");

    // Generate proof
    let input = InitV1CallData::new(
        pallas::Scalar::zero(), // nullifier_k - not used in init
        dao_bulla,
        owner_secret,
        token_id,
        bulla_blind,
    );

    let (proof, public_inputs) =
        init_v1_proof(&zkbin, &pk, &input).expect("Failed to generate proof");

    // Output proof and public inputs
    println!("Proof: {}", hex::encode(proof.as_ref()));
    println!("Public inputs:");
    println!("  dao_bulla: {:?}", public_inputs.dao_bulla);
    println!("  endowment_bulla: {:?}", public_inputs.endowment_bulla);
}

fn run_pay_premium(args: &[String]) {
    if args.len() != 5 {
        eprintln!("Usage: pay_premium <escrow_bulla_hex> <member_secret_hex> <value_u64> <token_id_hex> <expiry_u64>");
        std::process::exit(1);
    }

    let escrow_bulla = parse_hex_to_base(&args[0]).expect("Invalid escrow_bulla hex");
    let member_secret = parse_hex_to_base(&args[1]).expect("Invalid member_secret hex");
    let value: u64 = args[2].parse().expect("Invalid value");
    let token_id = parse_hex_to_base(&args[3]).expect("Invalid token_id hex");
    let expiry: u64 = args[4].parse().expect("Invalid expiry");

    // Load circuit binary
    let pay_premium_bin = include_bytes!("../../dao_escrow/proof/pay_premium.zk.bin");
    let zkbin = ZkBinary::decode(pay_premium_bin, false).unwrap();

    let circuit = ZkCircuit::new(dwow_core::zk::empty_witnesses(&zkbin).unwrap(), &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build failed");

    // Generate proof
    let input = PayPremiumV1CallData::new(
        pallas::Scalar::zero(), // nullifier_k
        escrow_bulla,
        0, // current_block
        member_secret,
        value,
        token_id,
        expiry,
        pallas::Base::zero(), // membership_blind
        pallas::Scalar::zero(), // value_blind
        pallas::Base::zero(), // mpc_secret_1
        pallas::Base::zero(), // mpc_secret_2
        pallas::Base::zero(), // mpc_secret_3
    );

    let (proof, public_inputs) =
        pay_premium_v1_proof(&zkbin, &pk, &input).expect("Failed to generate proof");

    // Output proof and public inputs
    println!("Proof: {}", hex::encode(proof.as_ref()));
    println!("Public inputs:");
    println!("  dao_escrow_bulla: {:?}", public_inputs.dao_escrow_bulla);
    println!("  membership_note: {:?}", public_inputs.membership_note);
}

fn parse_hex_to_base(s: &str) -> Result<pallas::Base, String> {
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.len() != 32 {
        return Err("Must be 32 bytes".to_string())
    }
    let mut repr = [0u8; 32];
    repr.copy_from_slice(&bytes);
    Option::from(pallas::Base::from_repr(repr)).ok_or_else(|| "Invalid base value".to_string())
}