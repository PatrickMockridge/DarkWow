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

//! Length-proportional metering for the bulk-memory operators (`OBL-C277`).
//!
//! ## The defect
//!
//! `MemoryFill`, `MemoryCopy` and `MemoryInit` are charged a flat tier-4 cost of 8 gas by the
//! runtime's cost function (`vm_runtime.rs`, `cost_function`), whatever their length. A guest
//! loops them inside its 400M-gas budget and each iteration writes up to the memory's declared
//! maximum (256 MiB here): one *valid* block can execute for days. Gas prices instructions, not
//! bytes, so no flat price fixes it — a price high enough to bound the loop refuses honest copies.
//!
//! Every one of the 32 artifacts contains these operators (55–398 each), so refusing them at load
//! is not available without rebuilding them, and the toolchain used here forces the feature on:
//! `-C target-feature=-bulk-memory` is ignored by rustc 1.95 for `wasm32-unknown-unknown` (a byte
//! loop still lowers to `memory.fill`), and `-C target-cpu=mvp` removes only the guest's own
//! operators, leaving five inside precompiled `std` (`__rdl_realloc`, `__rdl_alloc_zeroed`,
//! `write_str`). Rebuilding `std` was measured and does not link in this workspace today.
//!
//! ## The control
//!
//! This is a compiler middleware, so every node transforms every module identically at load; it
//! is not a wall-clock check and does not make acceptance host-dependent. Before each bulk-memory
//! operator it:
//!
//! 1. stashes the length operand in a private scratch global (the operand is on top of the stack
//!    for all three operators: `[dst, src?, len]`);
//! 2. charges `len >> 2` gas — the tier-2 rate of one gas per four bytes, so a copy of N bytes
//!    costs what N/4 single-byte stores would;
//! 3. traps with **metering's own exhausted flag set** when the charge exceeds what is left, so
//!    the host reports gas exhaustion rather than a malformed artifact;
//! 4. restores the length and lets the original operator run.
//!
//! The charge comes out of the same remaining-points global the `Metering` middleware maintains,
//! which is why this middleware **must be pushed after it**: it looks the two globals up by the
//! names `Metering` exports and refuses to compile the module if either is missing, rather than
//! silently not charging.
//!
//! `FunctionBulkMetering` never calls out, so the scratch global cannot be clobbered between the
//! stash and the restore; a global (rather than a local) is used because a module-level middleware
//! cannot add locals to function bodies in this Wasmer version.
use std::fmt;
use std::sync::Mutex;

use wasmer::sys::{FunctionMiddleware, MiddlewareError, MiddlewareReaderState, ModuleMiddleware};
use wasmer::wasmparser::{BlockType as WpTypeOrFuncType, Operator};
use wasmer::{ExportIndex, GlobalInit, GlobalType, LocalFunctionIndex, Mutability, Type};
use wasmer_types::{GlobalIndex, ModuleInfo};

/// The remaining-points global `Metering` adds and exports.
const METERING_POINTS_EXPORT: &str = "wasmer_metering_remaining_points";
/// The exhausted flag `Metering` adds and exports.
const METERING_EXHAUSTED_EXPORT: &str = "wasmer_metering_points_exhausted";
/// `len >> BYTES_PER_GAS_SHIFT` gas per bulk operator: one gas per four bytes.
const BYTES_PER_GAS_SHIFT: i64 = 2;

/// Charges bulk-memory operators by the number of bytes they touch.
#[derive(Debug)]
pub struct BulkMemoryMetering {
    /// The metering globals and this middleware's scratch global, resolved once per module in
    /// `transform_module_info` and read by every function middleware.
    indexes: Mutex<Option<(GlobalIndex, GlobalIndex, GlobalIndex)>>,
}

impl BulkMemoryMetering {
    pub fn new() -> Self {
        Self { indexes: Mutex::new(None) }
    }
}

impl Default for BulkMemoryMetering {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleMiddleware for BulkMemoryMetering {
    fn generate_function_middleware(&self, _: LocalFunctionIndex) -> Box<dyn FunctionMiddleware> {
        let (remaining, exhausted, scratch) = match *self.indexes.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(indexes) => indexes,
            None => unreachable!(
                "BulkMemoryMetering::transform_module_info runs before any function middleware"
            ),
        };
        Box::new(FunctionBulkMetering { remaining, exhausted, scratch })
    }

    fn transform_module_info(&self, module_info: &mut ModuleInfo) -> Result<(), MiddlewareError> {
        let exported_global = |name: &str| match module_info.exports.get(name) {
            Some(ExportIndex::Global(index)) => Ok(*index),
            _ => Err(MiddlewareError::new(
                "bulk-memory metering",
                format!(
                    "the metering global `{name}` is not exported — push `Metering` before \
                     `BulkMemoryMetering`"
                ),
            )),
        };
        let remaining = exported_global(METERING_POINTS_EXPORT)?;
        let exhausted = exported_global(METERING_EXHAUSTED_EXPORT)?;

        // The length operand's temporary home. Appended after metering's globals, so their
        // indices (already captured by that middleware) are unchanged.
        let scratch = module_info.globals.push(GlobalType::new(Type::I32, Mutability::Var));
        module_info.global_initializers.push(GlobalInit::I32Const(0));

        *self.indexes.lock().unwrap_or_else(|e| e.into_inner()) = Some((remaining, exhausted, scratch));
        Ok(())
    }
}

struct FunctionBulkMetering {
    remaining: GlobalIndex,
    exhausted: GlobalIndex,
    scratch: GlobalIndex,
}

impl fmt::Debug for FunctionBulkMetering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FunctionBulkMetering")
            .field("remaining", &self.remaining)
            .field("exhausted", &self.exhausted)
            .field("scratch", &self.scratch)
            .finish()
    }
}

impl FunctionMiddleware for FunctionBulkMetering {
    fn feed<'a>(
        &mut self,
        operator: Operator<'a>,
        state: &mut MiddlewareReaderState<'a>,
    ) -> Result<(), MiddlewareError> {
        let bulk = matches!(
            operator,
            Operator::MemoryFill { .. } | Operator::MemoryCopy { .. } | Operator::MemoryInit { .. }
        );
        if !bulk {
            state.push_operator(operator);
            return Ok(());
        }

        let remaining = self.remaining.as_u32();
        let exhausted = self.exhausted.as_u32();
        let scratch = self.scratch.as_u32();

        // Stack on entry: [dst, src?, len]. Stash the length, then compare the charge with what
        // is left: `remaining < len >> 2` traps with the exhausted flag set (so the host names gas
        // exhaustion), otherwise the charge is subtracted and the length restored.
        state.push_operator(Operator::GlobalSet { global_index: scratch });
        state.extend([
            Operator::GlobalGet { global_index: remaining },
            Operator::GlobalGet { global_index: scratch },
            Operator::I64ExtendI32U,
            Operator::I64Const { value: BYTES_PER_GAS_SHIFT },
            Operator::I64ShrU,
            Operator::I64LtU,
            Operator::If { blockty: WpTypeOrFuncType::Empty },
            Operator::I32Const { value: 1 },
            Operator::GlobalSet { global_index: exhausted },
            Operator::Unreachable,
            Operator::Else,
            Operator::GlobalGet { global_index: remaining },
            Operator::GlobalGet { global_index: scratch },
            Operator::I64ExtendI32U,
            Operator::I64Const { value: BYTES_PER_GAS_SHIFT },
            Operator::I64ShrU,
            Operator::I64Sub,
            Operator::GlobalSet { global_index: remaining },
            Operator::End,
            Operator::GlobalGet { global_index: scratch },
        ]);
        state.push_operator(operator);
        Ok(())
    }
}

/// The failing-before/after control, without any contract or RandomX involved: a module whose
/// one exported function fills 64 KiB in a permanent loop.
///
/// * Unmetered, a fill costs 8 gas whatever its length, so a small budget buys thousands of
///   fills and the memory is written.
/// * Metered, `64 KiB >> 2 = 16 384` gas is charged before the first fill and a budget below that
///   traps with the memory untouched.
///
/// The assertions are on *state* (the first byte written or not), not on elapsed time, so the
/// control cannot flake on a loaded host.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wasmer::sys::CompilerConfig;
    use wasmer::{Instance, Module, Store};
    use wasmer_compiler_singlepass::Singlepass;
    use wasmer_middlewares::metering::{get_remaining_points, MeteringPoints};
    use wasmer_middlewares::Metering;

    const FILL_VALUE: u8 = 0xAB;
    const FILL_LEN: u32 = 65_536; // 64 KiB, 16 memory pages
    const FILL_GAS: u64 = (FILL_LEN >> BYTES_PER_GAS_SHIFT) as u64;

    fn uleb(out: &mut Vec<u8>, mut value: u32) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return
            }
            out.push(byte | 0x80);
        }
    }

    fn section(out: &mut Vec<u8>, id: u8, body: &[u8]) {
        out.push(id);
        uleb(out, body.len() as u32);
        out.extend_from_slice(body);
    }

    /// One exported `run() -> ()` that loops `memory.fill(0, 0xAB, 65 536)` forever, over a
    /// 16-page memory with a declared maximum (the shape the host requires of every artifact).
    fn looped_fill_module() -> Vec<u8> {
        let mut module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

        // Type 0: () -> ().
        section(&mut module, 1, &[0x01, 0x60, 0x00, 0x00]);
        // Function 0 has type 0.
        section(&mut module, 3, &[0x01, 0x00]);
        // Memory 0: min 16 pages, max 16.
        section(&mut module, 5, &[0x01, 0x01, 0x10, 0x10]);
        // Export "run" as function 0, "memory" as memory 0.
        let mut exports = vec![0x02];
        exports.push(3);
        exports.extend_from_slice(b"run");
        exports.push(0x00);
        uleb(&mut exports, 0);
        exports.push(6);
        exports.extend_from_slice(b"memory");
        exports.push(0x02);
        uleb(&mut exports, 0);
        section(&mut module, 7, &exports);

        // Code: loop { i32.const 0; i32.const 0xAB; i32.const 65 536; memory.fill 0; br 0 }.
        let mut body = vec![0x00]; // no locals
        body.extend_from_slice(&[0x03, 0x40]); // loop, empty block type
        body.extend_from_slice(&[0x41, 0x00]); // i32.const 0  (dst)
        body.extend_from_slice(&[0x41, 0xAB, 0x01]); // i32.const 0xAB  (value)
        body.extend_from_slice(&[0x41, 0x80, 0x80, 0x04]); // i32.const 65 536 (len)
        body.extend_from_slice(&[0xFC, 0x0B, 0x00]); // memory.fill 0
        body.extend_from_slice(&[0x0C, 0x00]); // br 0
        body.push(0x0B); // end loop
        body.push(0x0B); // end function

        let mut code = vec![0x01];
        uleb(&mut code, body.len() as u32);
        code.extend_from_slice(&body);
        section(&mut module, 10, &code);
        module
    }

    fn run(limit: u64, metered: bool) -> (bool, u8, bool) {
        // The metered arm goes through the **production** compiler stack, so removing the
        // middleware from `contract_compiler` fails this control. The unmetered arm is the
        // failing-before twin: metering alone, which is what the runtime did before `OBL-C277`.
        let compiler = if metered {
            crate::runtime::vm_runtime::contract_compiler(limit, |_| 1u64)
        } else {
            let mut compiler = Singlepass::new();
            compiler.push_middleware(Arc::new(Metering::new(limit, |_| 1u64)));
            compiler
        };
        let mut store = Store::new(compiler);
        let module = Module::new(&store, looped_fill_module()).expect("the test module compiles");
        let imports = wasmer::imports!();
        let instance = Instance::new(&mut store, &module, &imports)
            .expect("the test module instantiates");
        let run = instance.exports.get_function("run").expect("run is exported");
        let trapped = run.call(&mut store, &[]).is_err();
        let memory = instance.exports.get_memory("memory").expect("memory is exported");
        let first = memory.view(&store).read_u8(0).expect("first byte");
        let exhausted = matches!(get_remaining_points(&mut store, &instance), MeteringPoints::Exhausted);
        (trapped, first, exhausted)
    }

    #[test]
    fn bulk_memory_is_charged_by_length() {
        // A budget below one metered fill, so the two runs differ in *whether* they write.
        let limit = FILL_GAS / 2;

        let (plain_trapped, plain_first, _) = run(limit, false);
        assert!(plain_trapped, "the unmetered loop must exhaust its budget");
        assert_eq!(
            plain_first, FILL_VALUE,
            "unmetered, a 64 KiB fill costs 8 gas, so the budget buys thousands of them and the \
             memory is written — this is the defect the middleware charges for"
        );

        let (metered_trapped, metered_first, metered_exhausted) = run(limit, true);
        assert!(metered_trapped, "the metered loop must exhaust its budget");
        assert_eq!(
            metered_first, 0,
            "metered, one fill costs {FILL_GAS} gas and the budget is {limit}, so it must trap \
             before writing a byte"
        );
        assert!(metered_exhausted, "the trap must set metering's exhausted flag, so the host \
                                    reports gas exhaustion rather than a malformed artifact");

        // The acceptance twin: a budget above one fill lets the same module write, so the charge
        // is proportional rather than a refusal.
        let (trapped, first, _) = run(FILL_GAS * 4, true);
        assert!(trapped, "the loop still runs forever and must exhaust the larger budget");
        assert_eq!(first, FILL_VALUE, "with room for at least one fill the write happens");
    }
}
