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

//! ZK Proof generation module for zkas CLI
//!
//! This module provides the `prove` subcommand for generating ZK proofs
//! from compiled .zk.bin circuit binaries.
//!
//! Usage:
//!   zkas prove <circuit.zk.bin> --witnesses <values> --public <values> --output <proof.bin>
//!
//! The tool loads a .zk.bin file, parses witness and public input definitions,
//! accepts values via CLI, and generates a proof.

use std::{
    fs::File,
    io::Write,
    path::Path,
    process::ExitCode,
};

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::{VarType, ZkBinary},
};
use dwow_sdk::pasta::pallas;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use rand::rngs::OsRng;

/// Parse a hex string into pallas::Base
fn parse_base(s: &str) -> Result<pallas::Base, String> {
    // Remove 0x prefix if present
    let s = s.trim_start_matches("0x");
    // Parse as hex
    let bytes = hex::decode(s).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.len() != 32 {
        return Err("Base must be exactly 32 bytes".to_string())
    }
    let mut repr = [0u8; 32];
    repr.copy_from_slice(&bytes);
    Ok(pallas::Base::from_repr(repr).unwrap())
}

/// Parse a hex string into pallas::Scalar
fn parse_scalar(s: &str) -> Result<pallas::Scalar, String> {
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.len() != 32 {
        return Err("Scalar must be exactly 32 bytes".to_string())
    }
    let mut repr = [0u8; 32];
    repr.copy_from_slice(&bytes);
    Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(repr))
        .ok_or_else(|| "Scalar value out of range".to_string())
}

/// Parse witness value based on type
fn parse_witness_value(vtype: &VarType, value: &str) -> Result<Witness, String> {
    match vtype {
        VarType::Base => Ok(Witness::Base(Value::known(parse_base(value)?))),
        VarType::Scalar => Ok(Witness::Scalar(Value::known(parse_scalar(value)?))),
        _ => Err(format!("Unsupported witness type: {:?}", vtype)),
    }
}

/// Print usage for the prove subcommand
fn prove_usage() {
    eprintln!(r#"
Usage: zkas prove <CIRCUIT.zk.bin> [OPTIONS]

Generate a ZK proof from a compiled circuit binary.

Arguments:
  <CIRCUIT.zk.bin>    Path to the compiled .zk.bin circuit file

Options:
  -w, --witnesses <VALUES>   Comma-separated witness values (hex)
  -p, --public <VALUES>      Comma-separated public input values (hex)
  -o, --output <FILE>        Output file for the proof (default: proof.bin)
  -h, --help                 Show this help

Examples:
  # Generate proof with witnesses and public inputs
  zkas prove contract/proof/init_v1.zk.bin \
    --witnesses "0x1234...,0xabcd...,0x5678..." \
    --public "0xdead...,0xbeef..."

  # Generate proof with shorter flags
  zkas prove contract.zk.bin -w "val1,val2" -p "pub1,pub2" -o my_proof.bin

Note: Witness and public input values must be provided in the same order
as defined in the circuit source .zk file.
"#);
}

/// Generate a proof from a circuit binary
fn prove(circuit_path: &Path, witnesses: &[String], public_inputs: &[String], output: &Path) -> ExitCode {
    // Read circuit binary
    let bincode = match std::fs::read(circuit_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed reading circuit file {}: {}", circuit_path.display(), e);
            return ExitCode::FAILURE
        }
    };

    // Decode circuit
    let zkbin = match ZkBinary::decode(&bincode, false) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to decode circuit: {}", e);
            return ExitCode::FAILURE
        }
    };

    println!("Circuit: {}", zkbin.namespace);
    println!("K: {}", zkbin.k);
    println!("Witnesses: {} types", zkbin.witnesses.len());
    println!("Public inputs: {} (derived from circuit)", zkbin.literals.len());

    // Validate witness count
    if witnesses.len() != zkbin.witnesses.len() {
        eprintln!(
            "Error: Expected {} witnesses, got {}",
            zkbin.witnesses.len(),
            witnesses.len()
        );
        return ExitCode::FAILURE
    }

    // Build witnesses
    let mut witness_vec = Vec::with_capacity(zkbin.witnesses.len());
    for (i, vtype) in zkbin.witnesses.iter().enumerate() {
        match parse_witness_value(vtype, &witnesses[i]) {
            Ok(w) => witness_vec.push(w),
            Err(e) => {
                eprintln!("Error parsing witness[{}]: {}", i, e);
                return ExitCode::FAILURE
            }
        }
    }

    // Build circuit
    let circuit = ZkCircuit::new(witness_vec, &zkbin);

    // Build proving key
    println!("Building proving key (this may take a while)...");
    let pk = ProvingKey::build(zkbin.k, &circuit)
        .unwrap_or_else(|e| {
            eprintln!("Error: Failed to build proving key: {e}");
            std::process::exit(1);
        });

    // Parse public inputs as base values
    let instances: Vec<pallas::Base> = match public_inputs
        .iter()
        .map(|s| parse_base(s))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing public inputs: {}", e);
            return ExitCode::FAILURE
        }
    };

    // Generate proof
    println!("Generating proof...");
    let proof = match Proof::create(&pk, &[circuit], &instances, &mut OsRng) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: Failed to create proof: {}", e);
            return ExitCode::FAILURE
        }
    };

    // Write proof to file
    let mut file = match File::create(output) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to create output file: {}", e);
            return ExitCode::FAILURE
        }
    };

    if let Err(e) = file.write_all(proof.as_ref()) {
        eprintln!("Error: Failed to write proof: {}", e);
        return ExitCode::FAILURE
    }

    println!("Proof written to: {}", output.display());
    println!("Proof size: {} bytes", proof.as_ref().len());

    ExitCode::SUCCESS
}

/// Parse prove subcommand arguments
pub fn run_prove(args: &[String]) -> ExitCode {
    if args.is_empty() {
        prove_usage();
        return ExitCode::FAILURE
    }

    let mut circuit_path = None;
    let mut witnesses: Vec<String> = Vec::new();
    let mut public_inputs: Vec<String> = Vec::new();
    let mut output = String::from("proof.bin");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                prove_usage();
                return ExitCode::SUCCESS
            }
            "-w" | "--witnesses" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --witnesses requires a value");
                    return ExitCode::FAILURE
                }
                witnesses = args[i].split(',').map(|s| s.trim().to_string()).collect();
            }
            "-p" | "--public" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --public requires a value");
                    return ExitCode::FAILURE
                }
                public_inputs = args[i].split(',').map(|s| s.trim().to_string()).collect();
            }
            "-o" | "--output" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --output requires a value");
                    return ExitCode::FAILURE
                }
                output = args[i].clone();
            }
            _ => {
                if circuit_path.is_none() {
                    circuit_path = Some(Path::new(&args[i]).to_path_buf());
                }
            }
        }
        i += 1;
    }

    let circuit_path = match circuit_path {
        Some(p) => p,
        None => {
            eprintln!("Error: No circuit file specified");
            prove_usage();
            return ExitCode::FAILURE
        }
    };

    if witnesses.is_empty() {
        eprintln!("Error: No witnesses provided (use --witnesses)");
        return ExitCode::FAILURE
    }

    prove(&circuit_path, &witnesses, &public_inputs, Path::new(&output))
}