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

use std::{io::Result, str::Chars};

use darkfi_serial::{serialize, VarInt};

use super::{
    ast::{Arg, Constant, Literal, Statement, StatementType, Witness},
    constants::{
        SECTION_TYPE_CIRCUIT, SECTION_TYPE_CONSTANT, SECTION_TYPE_DEBUG, SECTION_TYPE_LITERAL,
        SECTION_TYPE_SOURCE_HASH, SECTION_TYPE_WITNESS,
    },
    error::ErrorEmitter,
    types::HeapType,
};

/// Version of the binary
pub const BINARY_VERSION: u8 = 3;
/// Magic bytes prepended to the binary
pub const MAGIC_BYTES: [u8; 4] = [0x0b, 0x01, 0xb1, 0x35];

pub struct Compiler {
    namespace: String,
    k: u32,
    constants: Vec<Constant>,
    witnesses: Vec<Witness>,
    statements: Vec<Statement>,
    literals: Vec<Literal>,
    debug_info: bool,
    source_hash: Option<String>,
    error: ErrorEmitter,
}

impl Compiler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        filename: &str,
        source: Chars,
        namespace: String,
        k: u32,
        constants: Vec<Constant>,
        witnesses: Vec<Witness>,
        statements: Vec<Statement>,
        literals: Vec<Literal>,
        debug_info: bool,
        source_hash: Option<String>,
    ) -> Self {
        // For nice error reporting, we'll load everything into a string
        // vector so we have references to lines.
        let lines: Vec<String> = source.as_str().lines().map(|x| x.to_string()).collect();
        let error = ErrorEmitter::new("Compiler", filename, lines);

        Self { namespace, k, constants, witnesses, statements, literals, debug_info, source_hash, error }
    }

    pub fn compile(&self) -> Result<Vec<u8>> {
        let mut bincode = vec![];

        // Write the magic bytes and version
        bincode.extend_from_slice(&MAGIC_BYTES);
        bincode.push(BINARY_VERSION);

        // Write the circuit's k param
        bincode.extend_from_slice(&serialize(&self.k));

        // Write the circuit's namespace
        bincode.extend_from_slice(&serialize(&self.namespace));

        // Write source hash section (version 3+)
        if let Some(ref hash) = self.source_hash {
            let mut section_data = vec![];
            section_data.extend_from_slice(&serialize(hash));
            bincode.push(SECTION_TYPE_SOURCE_HASH);
            bincode.extend_from_slice(&u32::to_le_bytes(section_data.len() as u32));
            bincode.extend_from_slice(&section_data);
        }

        // Temporary heap vector for lookups
        let mut tmp_heap = vec![];

        // Write .constant section: [type=2][length:4][data]
        // Data format: for each constant: [1 byte type][varint string length][string bytes]
        let mut constant_data = vec![];
        for i in &self.constants {
            tmp_heap.push(i.name.as_str());
            constant_data.push(i.typ as u8);
            constant_data.extend_from_slice(&serialize(&i.name));
        }
        bincode.push(SECTION_TYPE_CONSTANT);
        bincode.extend_from_slice(&u32::to_le_bytes(constant_data.len() as u32));
        bincode.extend_from_slice(&constant_data);

        // Write .literal section: [type=3][length:4][data]
        // Data format: for each literal: [1 byte type][varint string length][string bytes]
        let mut literal_data = vec![];
        for i in &self.literals {
            literal_data.push(i.typ as u8);
            literal_data.extend_from_slice(&serialize(&i.name));
        }
        bincode.push(SECTION_TYPE_LITERAL);
        bincode.extend_from_slice(&u32::to_le_bytes(literal_data.len() as u32));
        bincode.extend_from_slice(&literal_data);

        // Write .witness section: [type=4][length:4][data]
        // Data format: for each witness: [1 byte type]
        let mut witness_data = vec![];
        for i in &self.witnesses {
            tmp_heap.push(i.name.as_str());
            witness_data.push(i.typ as u8);
        }
        bincode.push(SECTION_TYPE_WITNESS);
        bincode.extend_from_slice(&u32::to_le_bytes(witness_data.len() as u32));
        bincode.extend_from_slice(&witness_data);

        // Write .circuit section: [type=5][length:4][data]
        let mut circuit_data = vec![];
        for i in &self.statements {
            match i.typ {
                StatementType::Assign => tmp_heap.push(&i.lhs.as_ref().unwrap().name),
                StatementType::Call => {}
                _ => unreachable!("Invalid statement type in circuit: {:?}", i.typ),
            }

            circuit_data.push(i.opcode as u8);
            circuit_data.extend_from_slice(&serialize(&VarInt(i.rhs.len() as u64)));

            for arg in &i.rhs {
                match arg {
                    Arg::Var(arg) => {
                        let heap_idx =
                            Compiler::lookup_heap(&tmp_heap, &arg.name).ok_or_else(|| {
                                self.error.abort(
                                    &format!("Failed finding a heap reference for `{}`", arg.name),
                                    arg.line,
                                    arg.column,
                                )
                            })?;

                        circuit_data.push(HeapType::Var as u8);
                        circuit_data.extend_from_slice(&serialize(&VarInt(heap_idx as u64)));
                    }
                    Arg::Lit(lit) => {
                        let lit_idx = Compiler::lookup_literal(&self.literals, &lit.name)
                            .ok_or_else(|| {
                                self.error.abort(
                                    &format!("Failed finding literal `{}`", lit.name),
                                    lit.line,
                                    lit.column,
                                )
                            })?;

                        circuit_data.push(HeapType::Lit as u8);
                        circuit_data.extend_from_slice(&serialize(&VarInt(lit_idx as u64)));
                    }
                    _ => unreachable!(),
                };
            }
        }
        bincode.push(SECTION_TYPE_CIRCUIT);
        bincode.extend_from_slice(&u32::to_le_bytes(circuit_data.len() as u32));
        bincode.extend_from_slice(&circuit_data);

        // If we're not doing debug info, we're done here.
        if !self.debug_info {
            return Ok(bincode)
        }

        // Write .debug section: [type=6][length:4][data]
        let mut debug_data = vec![];

        // Write source locations for each opcode.
        debug_data.extend_from_slice(&serialize(&VarInt(self.statements.len() as u64)));
        for stmt in &self.statements {
            debug_data.extend_from_slice(&serialize(&VarInt(stmt.line as u64)));
            let column = stmt.lhs.as_ref().map(|v| v.column).unwrap_or(0);
            debug_data.extend_from_slice(&serialize(&VarInt(column as u64)));
        }

        // Write heap variable names.
        let heap_size = self.constants.len() +
            self.witnesses.len() +
            self.statements.iter().filter(|s| s.typ == StatementType::Assign).count();
        debug_data.extend_from_slice(&serialize(&VarInt(heap_size as u64)));

        for constant in &self.constants {
            debug_data.extend_from_slice(&serialize(&constant.name));
        }

        for witness in &self.witnesses {
            debug_data.extend_from_slice(&serialize(&witness.name));
        }

        for stmt in &self.statements {
            if stmt.typ == StatementType::Assign {
                debug_data.extend_from_slice(&serialize(&stmt.lhs.as_ref().unwrap().name));
            }
        }

        // Write literal names
        debug_data.extend_from_slice(&serialize(&VarInt(self.literals.len() as u64)));
        for literal in &self.literals {
            debug_data.extend_from_slice(&serialize(&literal.name));
        }

        bincode.push(SECTION_TYPE_DEBUG);
        bincode.extend_from_slice(&u32::to_le_bytes(debug_data.len() as u32));
        bincode.extend_from_slice(&debug_data);

        Ok(bincode)
    }

    fn lookup_heap(heap: &[&str], name: &str) -> Option<usize> {
        heap.iter().position(|&n| n == name)
    }

    fn lookup_literal(literals: &[Literal], name: &str) -> Option<usize> {
        literals.iter().position(|n| n.name == name)
    }
}
