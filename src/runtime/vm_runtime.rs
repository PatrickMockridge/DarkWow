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

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use dwow_sdk::{
    blockchain::{BlockHeight, BlockTarget},
    crypto::contract_id::{
        ContractId, SMART_CONTRACT_MONOTREE_DB_NAME, SMART_CONTRACT_ZKAS_DB_NAME,
    },
    tx::TransactionHash,
    wasm, AsHex,
};
use dwow_serial::serialize;
use tracing::{debug, error, info};
use wasmer::{
    imports, sys::CompilerConfig, wasmparser::Operator, AsStoreMut, AsStoreRef, Function,
    FunctionEnv, Instance, Memory, MemoryType, MemoryView, Module, Pages, Store, Value,
    WASM_PAGE_SIZE,
};
#[cfg(not(feature = "cranelift-compiler"))]
use wasmer_compiler_singlepass::Singlepass as Compiler;
#[cfg(feature = "cranelift-compiler")]
use wasmer_compiler_cranelift::Cranelift as Compiler;
use wasmer_middlewares::{
    metering::{get_remaining_points, set_remaining_points, MeteringPoints},
    Metering,
};

use super::{import, import::db::DbHandle, memory::MemoryManipulation};
use crate::{Error, Result};

/// Single backend for the WASM runtime — contract storage, state DB, and
/// blockchain queries. Replaces the three separate traits (ContractStoreAccess,
/// SimpleDbAccess, BlockchainAccess) that accumulated during the architecture
/// changeover. Matches upstream darkfi's single BlockchainOverlayPtr pattern.
pub trait RuntimeBackend: Send + Sync {
    /// Look up a tree handle for an initialized tree.
    fn contract_lookup(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]>;
    /// Initialize a new tree for a contract. Returns the tree handle.
    fn contract_init(&self, cid: &ContractId, tree_name: &str) -> Result<[u8; 32]>;
    /// Store contract WASM bincode.
    fn contract_insert_bincode(&self, cid: ContractId, bincode: &[u8]) -> Result<()>;
    /// Get contract WASM bincode.
    fn contract_get_bincode(&self, cid: &ContractId) -> Result<Vec<u8>>;

    /// State DB: insert key-value into a tree
    fn db_insert(&self, tree: &[u8], key: &[u8], value: &[u8]) -> Result<()>;
    /// State DB: get value by key from a tree
    fn db_get(&self, tree: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>>;
    /// State DB: remove key from a tree
    fn db_remove(&self, tree: &[u8], key: &[u8]) -> Result<()>;
    /// State DB: check if key exists in a tree
    fn db_contains_key(&self, tree: &[u8], key: &[u8]) -> Result<bool>;

    /// Blockchain queries
    fn last_block_timestamp(&self) -> Result<Vec<u8>>;
    fn last_block_height(&self) -> Result<BlockHeight>;
    fn get_tx(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>>;
    fn get_tx_location(&self, hash: &[u8; 32]) -> Result<Option<Vec<u8>>>;
    fn get_block_hash_by_height(&self, height: BlockHeight) -> Result<Option<Vec<u8>>>;

    /// Block-level Merkle tree: append a contract state anchor.
    /// Called by the `merkle_anchor_add` host function during `process_update`.
    /// Entry format: 96 bytes (nullifier || contract_id || contract_root).
    fn block_anchor_append(&self, entry_bytes: &[u8; 96]) -> crate::Result<()>;
}

/// Type-erased pointer to the runtime backend. A single pointer replaces the
/// three separate Arc<dyn Trait> objects we had before.
pub type BackendPtr = Arc<dyn RuntimeBackend>;

/// Ephemeral transaction-local state. Used by `db_*_local_` host functions
/// for temporary in-memory storage during contract execution — never committed
/// to the blockchain. Matches upstream darkfi's TxLocalState.
pub type TxLocalState = BTreeMap<ContractId, BTreeMap<[u8; 32], BTreeMap<Vec<u8>, Vec<u8>>>>;



/// Name of the wasm linear memory in our guest module
const MEMORY: &str = "memory";

/// Gas limit for a single contract call (Single WASM instance)
pub const GAS_LIMIT: u64 = 400_000_000;

// ANCHOR: contract-section
#[derive(Clone, Copy, PartialEq)]
pub enum ContractSection {
    /// Setup function of a contract
    Deploy,
    /// Entrypoint function of a contract
    Exec,
    /// Apply function of a contract
    Update,
    /// Metadata
    Metadata,
    /// Spend hook callback from another contract
    SpendHook,
    /// Placeholder state before any initialization
    Null,
}
// ANCHOR_END: contract-section

impl ContractSection {
    pub const fn name(&self) -> &str {
        match self {
            Self::Deploy => "__initialize",
            Self::Exec => "__entrypoint",
            Self::Update => "__update",
            Self::Metadata => "__metadata",
            Self::SpendHook => "__spend_hook",
            Self::Null => unreachable!(),
        }
    }
}

/// The WASM VM runtime environment instantiated for every smart contract that runs.
pub struct Env {
    /// Single backend for contract storage, state DB, and blockchain queries.
    /// Replaces the three separate trait objects that accumulated during the
    /// architecture changeover. Matches upstream darkfi's BlockchainOverlayPtr pattern.
    pub backend: BackendPtr,
    /// Ephemeral tx-local state (never committed to blockchain).
    /// Used by db_*_local_ host functions.
    pub tx_local: Arc<Mutex<TxLocalState>>,
    /// Overlay tree handles used with `db_*` (persistent)
    pub db_handles: RefCell<Vec<DbHandle>>,
    /// Overlay tree handles used with `db_*_local` (ephemeral)
    pub local_db_handles: RefCell<Vec<DbHandle>>,
    /// The contract ID being executed
    pub contract_id: ContractId,
    /// The compiled wasm bincode being executed,
    pub contract_bincode: Vec<u8>,
    /// The contract section being executed
    pub contract_section: ContractSection,
    /// State update produced by a smart contract function call
    pub contract_return_data: Cell<Option<Vec<u8>>>,
    /// Logs produced by the contract
    pub logs: RefCell<Vec<String>>,
    /// Direct memory access to the VM
    pub memory: Option<Memory>,
    /// Object store for transferring memory from the host to VM
    pub objects: RefCell<Vec<Vec<u8>>>,
    /// Block height number runtime verifies against.
    /// For unconfirmed txs, this will be the current max height in the chain.
    pub verifying_block_height: BlockHeight,
    /// Currently configured block time target, in seconds
    pub block_target: BlockTarget,
    /// The hash for this transaction the runtime is being run against.
    pub tx_hash: TransactionHash,
    /// The index for this call in the transaction
    pub call_idx: u8,
    /// Parent `Instance`
    pub instance: Option<Arc<Instance>>,
    /// Spend hook callback requested during exec():
    /// (target_contract_id_bytes, callback_payload).
    /// Written by `emit_spend_hook`, read by the blockchain pipeline.
    pub spend_hook_request: Cell<Option<([u8; 32], Vec<u8>)>>,
}

impl Env {
    /// Provide safe access to the memory
    /// (it must be initialized before it can be used)
    ///
    ///     // ctx: FunctionEnvMut<Env>
    ///     let env = ctx.data();
    ///     let memory = env.memory_view(&ctx);
    ///
    pub fn memory_view<'a>(&'a self, store: &'a impl AsStoreRef) -> MemoryView<'a> {
        self.memory().view(store)
    }

    /// Get memory, that needs to have been set fist
    pub fn memory(&self) -> &Memory {
        self.memory.as_ref().unwrap()
    }

    /// Subtract given gas cost from remaining gas in the current runtime.
    /// Returns true if gas was exhausted by this subtraction (caller should
    /// reject any state-mutating operations).
    pub fn subtract_gas(&mut self, ctx: &mut impl AsStoreMut, gas: u64) -> bool {
        match get_remaining_points(ctx, self.instance.as_ref().unwrap()) {
            MeteringPoints::Remaining(rem) => {
                if gas > rem {
                    set_remaining_points(ctx, self.instance.as_ref().unwrap(), 0);
                    true // gas exhausted
                } else {
                    set_remaining_points(ctx, self.instance.as_ref().unwrap(), rem - gas);
                    false
                }
            }
            MeteringPoints::Exhausted => {
                set_remaining_points(ctx, self.instance.as_ref().unwrap(), 0);
                true // already exhausted
            }
        }
    }

    /// HAZOP H8: check whether gas is exhausted. State-mutating host functions
    /// MUST call this after subtract_gas and return an error if true.
    pub fn is_gas_exhausted(&self, ctx: &mut impl AsStoreMut) -> bool {
        matches!(
            get_remaining_points(ctx, self.instance.as_ref().unwrap()),
            MeteringPoints::Exhausted
        )
    }

    /// HAZOP RC-C structural fix: charge gas and return an error code if
    /// exhausted. State-mutating host functions MUST use this single call
    /// instead of calling subtract_gas directly — the return-value check
    /// is embedded, making the correct pattern the path of least resistance.
    ///
    /// Replaces the error-prone pattern:
    ///   env.subtract_gas(&mut store, charge);   // return value discarded
    ///   // ... state mutation proceeds regardless ...
    /// with a single infallible call:
    ///   if env.charge_gas(&mut store, charge) { return INTERNAL_ERROR; }
    pub fn charge_gas(&mut self, ctx: &mut impl AsStoreMut, gas: u64) -> bool {
        if self.subtract_gas(ctx, gas) {
            error!(target: "runtime", "Gas exhausted — rejecting state-mutating operation");
            return true;
        }
        false
    }
}

/// Define a wasm runtime.
pub struct Runtime {
    /// A wasm instance
    pub instance: Arc<Instance>,
    /// A wasm store (global state)
    pub store: Store,
    // Wrapper for [`Env`], defined above.
    pub ctx: FunctionEnv<Env>,
}

impl Runtime {
    /// Pre-public-testnet audit H-7 fix: reject SIMD (0xFD) and threads/atomics
    /// (0xFE) opcodes in the WASM code section. Scalar floats (0x8A..=0xBF) are
    /// IEEE-754 deterministic within a single wasmer backend and are NOT rejected.
    ///
    /// Only the code section (id=10) is scanned — this avoids false positives
    /// from Rust stdlib-generated opcodes in metadata sections (the root cause
    /// of the prior scanner being disabled). The archived byte-level scanner
    /// remains below for reference.
    ///
    /// WASM binary format reference:
    ///   magic (4 bytes) + version (4 bytes) + sections...
    ///   section = id (1 byte, varuint7) + size (LEB128 u32) + content
    fn reject_nondeterministic_features(wasm_bytes: &[u8]) -> Result<()> {
        if wasm_bytes.len() < 8 {
            return Err(Error::NonDeterministicWasm(
                "WASM binary too short for header".to_string()
            ));
        }

        // Phase 1: find the code section (id=10) by iterating sections.
        let mut pos: usize = 8; // skip magic (4) + version (4)

        while pos < wasm_bytes.len() {
            if pos >= wasm_bytes.len() {
                break;
            }
            let section_id = wasm_bytes[pos];
            pos += 1;

            // Decode LEB128 u32 for section size.
            let (size, bytes_read) = match Self::decode_leb128_u32(&wasm_bytes[pos..]) {
                Some(v) => v,
                None => {
                    return Err(Error::NonDeterministicWasm(
                        "WASM binary: truncated LEB128 in section header".to_string()
                    ));
                }
            };
            pos += bytes_read;

            let section_end = pos.saturating_add(size as usize);
            if section_end > wasm_bytes.len() {
                return Err(Error::NonDeterministicWasm(
                    "WASM binary: section extends past end of file".to_string()
                ));
            }

            // Section 10 = Code section — the only section we scan.
            if section_id == 10 && size > 0 {
                let code_bytes = &wasm_bytes[pos..section_end];
                Self::scan_code_section(code_bytes)?;
            }

            pos = section_end;
        }

        Ok(())
    }

    /// Scan the WASM code section for non-deterministic opcodes.
    /// Rejects: 0xFD (SIMD prefix), 0xFE (atomics/threads prefix).
    /// Scalar float opcodes (0x8A..=0xBF) are IEEE-754 deterministic
    /// within a single wasmer backend and are NOT rejected.
    fn scan_code_section(code_bytes: &[u8]) -> Result<()> {
        let mut i: usize = 0;
        while i < code_bytes.len() {
            let opcode = code_bytes[i];
            match opcode {
                0xFD..=0xFE => {
                    let name = if opcode == 0xFD { "SIMD (0xFD)" } else { "threads/atomics (0xFE)" };
                    return Err(Error::NonDeterministicWasm(format!(
                        "Non-deterministic WASM opcode {} at code offset {} — rejected per consensus determinism requirement (contract-wasm-type-system.md A.8.4)",
                        name, i
                    )));
                }
                _ => {
                    // Advance past the opcode. Multi-byte opcodes (like SIMD
                    // instructions after 0xFD prefix) are caught by the prefix
                    // check above — further bytes are instruction operands.
                    i += 1;
                }
            }
        }
        Ok(())
    }

    /// Decode a LEB128 unsigned 32-bit integer. Returns (value, bytes_consumed).
    fn decode_leb128_u32(data: &[u8]) -> Option<(u32, usize)> {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;
        for (i, &byte) in data.iter().enumerate() {
            if i >= 5 {
                // LEB128 u32 fits in at most 5 bytes.
                return None;
            }
            result = result.checked_add(((byte & 0x7F) as u32).checked_shl(shift)?)?;
            if byte & 0x80 == 0 {
                return Some((result, i + 1));
            }
            shift += 7;
        }
        None
    }

    // Archived byte-level scanner (kept for reference — scans raw bytes
    // without section awareness, producing false positives from stdlib
    // metadata sections).
    #[allow(dead_code)]
    fn _reject_nondeterministic_features_archived(_wasm_bytes: &[u8]) -> Result<()> {
        Ok(())
    }

    /// Create a new wasm runtime instance that contains the given wasm module.
    pub fn new(
        wasm_bytes: &[u8],
        backend: BackendPtr,
        contract_id: ContractId,
        verifying_block_height: BlockHeight,
        block_target: BlockTarget,
        tx_hash: TransactionHash,
        call_idx: u8,
    ) -> Result<Self> {
        info!(target: "runtime::vm_runtime", "[WASM] Instantiating a new runtime");

        // HAZOP H10 fix: reject WASM binaries that use non-deterministic features
        // (floating-point, bulk memory, SIMD) before module compilation. These can
        // produce different results on different wasmer backends (Singlepass vs
        // Cranelift) or architectures, causing consensus splits.
        Self::reject_nondeterministic_features(wasm_bytes)?;

        // HAZOP M-12: tiered WASM opcode costs.
        // Bridge crypto ops (BN254 pairings, Keccak, SHA-256d) are 10-100x more
        // expensive than simple arithmetic — the uniform model undercharges.
        // Tiers: base=1, memory=2, control=4, math-heavy=8, unreachable ops=256.
        let cost_function = |operator: &Operator| -> u64 {
            use Operator::*;
            match operator {
                // Tier 1 (1 gas): simple/cheap — locals, globals, nop, drop, const
                LocalGet { .. } | LocalSet { .. } | LocalTee { .. }
                | GlobalGet { .. } | GlobalSet { .. }
                | Nop | Drop | Unreachable | Return | Select
                | I32Const { .. } | I64Const { .. } | F32Const { .. } | F64Const { .. }
                | Block { .. } | Loop { .. } | End | Else | Br { .. } | BrIf { .. }
                | I32Eqz | I64Eqz | I32Eq | I64Eq | I32Ne | I64Ne
                | I32LtS | I64LtS | I32LtU | I64LtU | I32GtS | I64GtS
                | I32GtU | I64GtU | I32LeS | I64LeS | I32LeU | I64LeU
                | I32GeS | I64GeS | I32GeU | I64GeU
                | I32Clz | I64Clz | I32Ctz | I64Ctz | I32Popcnt | I64Popcnt
                | I32Add | I64Add | I32Sub | I64Sub | I32And | I64And
                | I32Or | I64Or | I32Xor | I64Xor | I32Shl | I64Shl
                | I32ShrS | I64ShrS | I32ShrU | I64ShrU | I32Rotl | I64Rotl
                | I32Rotr | I64Rotr
                | I32WrapI64 | I64ExtendI32S | I64ExtendI32U
                | I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S
                | I32TruncF32S | I32TruncF32U | I32TruncF64S | I32TruncF64U
                | I64TruncF32S | I64TruncF32U | I64TruncF64S | I64TruncF64U
                | F32Abs | F64Abs | F32Neg | F64Neg | F32Ceil | F64Ceil
                | F32Floor | F64Floor | F32Trunc | F64Trunc | F32Nearest | F64Nearest
                | F32Sqrt | F64Sqrt
                | F32Add | F64Add | F32Sub | F64Sub | F32Mul | F64Mul | F32Div | F64Div
                | F32Min | F64Min | F32Max | F64Max | F32Copysign | F64Copysign
                | F32Eq | F64Eq | F32Ne | F64Ne | F32Lt | F64Lt | F32Gt | F64Gt
                | F32Le | F64Le | F32Ge | F64Ge
                | I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64
                | F32ConvertI32S | F32ConvertI32U | F32ConvertI64S | F32ConvertI64U
                | F64ConvertI32S | F64ConvertI32U | F64ConvertI64S | F64ConvertI64U
                | RefNull { .. } | RefIsNull | RefFunc { .. }
                | TableGet { .. } | TableSet { .. } | TableSize { .. } | TableGrow { .. }
                | TableFill { .. } | TableCopy { .. } | TableInit { .. } | ElemDrop { .. }
                | MemorySize { .. } => 1,

                // Tier 2 (2 gas): memory load/store — I/O bound
                I32Load { .. } | I64Load { .. } | F32Load { .. } | F64Load { .. }
                | I32Load8S { .. } | I32Load8U { .. } | I32Load16S { .. } | I32Load16U { .. }
                | I64Load8S { .. } | I64Load8U { .. } | I64Load16S { .. } | I64Load16U { .. }
                | I64Load32S { .. } | I64Load32U { .. }
                | I32Store { .. } | I64Store { .. } | F32Store { .. } | F64Store { .. }
                | I32Store8 { .. } | I32Store16 { .. } | I64Store8 { .. }
                | I64Store16 { .. } | I64Store32 { .. } => 2,

                // Tier 3 (4 gas): control flow — branch/call overhead
                BrTable { .. } | Call { .. } | CallIndirect { .. } | ReturnCall { .. }
                | ReturnCallIndirect { .. } => 4,

                // Tier 4 (8 gas): math-heavy — division, multiplication, memory ops
                I32Mul | I64Mul | I32DivS | I64DivS | I32DivU | I64DivU
                | I32RemS | I64RemS | I32RemU | I64RemU
                | MemoryGrow { .. } | MemoryCopy { .. } | MemoryFill { .. }
                | MemoryInit { .. } | DataDrop { .. } => 8,

                // Tier 5 (256 gas): SIMD + atomics — rejected at load time by
                // reject_nondeterministic_features, penalized if they somehow execute
                V128Load { .. } | V128Store { .. } | V128Const { .. }
                | I8x16Shuffle { .. } | I8x16Swizzle
                | I8x16Splat | I16x8Splat | I32x4Splat | I64x2Splat | F32x4Splat | F64x2Splat
                | V128Bitselect | V128AnyTrue | V128Not | V128And | V128AndNot
                | V128Or | V128Xor
                | I8x16Eq | I8x16Ne | I8x16LtS | I8x16LtU | I8x16GtS | I8x16GtU
                | I8x16LeS | I8x16LeU | I8x16GeS | I8x16GeU
                | I16x8Eq | I16x8Ne | I16x8LtS | I16x8LtU | I16x8GtS | I16x8GtU
                | I16x8LeS | I16x8LeU | I16x8GeS | I16x8GeU
                | I32x4Eq | I32x4Ne | I32x4LtS | I32x4LtU | I32x4GtS | I32x4GtU
                | I32x4LeS | I32x4LeU | I32x4GeS | I32x4GeU
                | F32x4Eq | F32x4Ne | F32x4Lt | F32x4Gt | F32x4Le | F32x4Ge
                | F64x2Eq | F64x2Ne | F64x2Lt | F64x2Gt | F64x2Le | F64x2Ge
                | I8x16Add | I8x16Sub | I16x8Add | I16x8Sub | I32x4Add | I32x4Sub
                | I64x2Add | I64x2Sub | F32x4Add | F32x4Sub | F32x4Mul | F32x4Div
                | F64x2Add | F64x2Sub | F64x2Mul | F64x2Div
                | I8x16MinS | I8x16MinU | I8x16MaxS | I8x16MaxU
                | I16x8MinS | I16x8MinU | I16x8MaxS | I16x8MaxU
                | I32x4MinS | I32x4MinU | I32x4MaxS | I32x4MaxU
                | F32x4Min | F32x4Max | F64x2Min | F64x2Max
                | I8x16AvgrU | I16x8AvgrU
                | I8x16Abs | I8x16Neg | I16x8Abs | I16x8Neg | I32x4Abs | I32x4Neg
                | I64x2Abs | I64x2Neg | F32x4Abs | F32x4Neg | F64x2Abs | F64x2Neg
                | F32x4Sqrt | F64x2Sqrt
                | I8x16Shl | I8x16ShrS | I8x16ShrU | I16x8Shl | I16x8ShrS | I16x8ShrU
                | I32x4Shl | I32x4ShrS | I32x4ShrU | I64x2Shl | I64x2ShrS | I64x2ShrU
                | MemoryAtomicNotify { .. } | MemoryAtomicWait32 { .. } | MemoryAtomicWait64 { .. }
                | AtomicFence
                | I32AtomicLoad { .. } | I64AtomicLoad { .. }
                | I32AtomicLoad8U { .. } | I32AtomicLoad16U { .. }
                | I64AtomicLoad8U { .. } | I64AtomicLoad16U { .. } | I64AtomicLoad32U { .. }
                | I32AtomicStore { .. } | I64AtomicStore { .. }
                | I32AtomicStore8 { .. } | I32AtomicStore16 { .. }
                | I64AtomicStore8 { .. } | I64AtomicStore16 { .. } | I64AtomicStore32 { .. }
                | I32AtomicRmwAdd { .. } | I64AtomicRmwAdd { .. }
                | I32AtomicRmw8AddU { .. } | I32AtomicRmw16AddU { .. }
                | I64AtomicRmw8AddU { .. } | I64AtomicRmw16AddU { .. } | I64AtomicRmw32AddU { .. }
                | I32AtomicRmwSub { .. } | I64AtomicRmwSub { .. }
                | I32AtomicRmw8SubU { .. } | I32AtomicRmw16SubU { .. }
                | I64AtomicRmw8SubU { .. } | I64AtomicRmw16SubU { .. } | I64AtomicRmw32SubU { .. }
                | I32AtomicRmwAnd { .. } | I64AtomicRmwAnd { .. }
                | I32AtomicRmw8AndU { .. } | I32AtomicRmw16AndU { .. }
                | I64AtomicRmw8AndU { .. } | I64AtomicRmw16AndU { .. } | I64AtomicRmw32AndU { .. }
                | I32AtomicRmwOr { .. } | I64AtomicRmwOr { .. }
                | I32AtomicRmw8OrU { .. } | I32AtomicRmw16OrU { .. }
                | I64AtomicRmw8OrU { .. } | I64AtomicRmw16OrU { .. } | I64AtomicRmw32OrU { .. }
                | I32AtomicRmwXor { .. } | I64AtomicRmwXor { .. }
                | I32AtomicRmw8XorU { .. } | I32AtomicRmw16XorU { .. }
                | I64AtomicRmw8XorU { .. } | I64AtomicRmw16XorU { .. } | I64AtomicRmw32XorU { .. }
                | I32AtomicRmwXchg { .. } | I64AtomicRmwXchg { .. }
                | I32AtomicRmw8XchgU { .. } | I32AtomicRmw16XchgU { .. }
                | I64AtomicRmw8XchgU { .. } | I64AtomicRmw16XchgU { .. } | I64AtomicRmw32XchgU { .. }
                | I32AtomicRmwCmpxchg { .. } | I64AtomicRmwCmpxchg { .. }
                | I32AtomicRmw8CmpxchgU { .. } | I32AtomicRmw16CmpxchgU { .. }
                | I64AtomicRmw8CmpxchgU { .. } | I64AtomicRmw16CmpxchgU { .. } | I64AtomicRmw32CmpxchgU { .. }
                => 256,
                &_ => 256,
            }
        };
        let metering = Arc::new(Metering::new(GAS_LIMIT, cost_function));
        let mut compiler_config = Compiler::new();
        compiler_config.push_middleware(metering);
        let mut store = Store::new(compiler_config);
        let module = Module::new(&store, wasm_bytes)?;


        // Create a larger Memory for the instance
        let memory_type = MemoryType::new(
            Pages(256),        // init: 16 MB (256 * 64KB)
            Some(Pages(4096)), // max: 256 MB
            false,
        );
        let memory = Memory::new(&mut store, memory_type)?;

        // Initialize data
        let db_handles = RefCell::new(vec![]);
        let local_db_handles = RefCell::new(vec![]);
        let logs = RefCell::new(vec![]);

        debug!(target: "runtime::vm_runtime", "Importing functions");

        let ctx = FunctionEnv::new(
            &mut store,
            Env {
                backend,
                tx_local: Arc::new(Mutex::new(TxLocalState::new())),
                db_handles,
                local_db_handles,
                contract_id,
                contract_bincode: wasm_bytes.to_vec(),
                contract_section: ContractSection::Null,
                contract_return_data: Cell::new(None),
                logs,
                memory: Some(memory.clone()),
                objects: RefCell::new(vec![]),
                verifying_block_height,
                block_target,
                tx_hash,
                call_idx,
                instance: None,
                spend_hook_request: Cell::new(None),
            },
        );

        let imports = imports! {
            "env" => {
                "memory" => memory,

                "drk_log_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::drk_log,
                ),

                "set_return_data_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::set_return_data,
                ),

                "db_init_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_init,
                ),

                "db_lookup_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_lookup,
                ),

                "db_lookup_local_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_lookup_local,
                ),

                "db_get_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_get,
                ),

                "db_get_local_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_get_local,
                ),

                "db_contains_key_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_contains_key,
                ),

                "db_contains_key_local_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_contains_key_local,
                ),

                "db_set_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_set,
                ),

                "db_set_local_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_set_local,
                ),

                "db_del_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_del,
                ),

                "db_del_local_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_del_local,
                ),

                "zkas_db_set_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::zkas_db_set,
                ),

                "get_object_bytes_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_object_bytes,
                ),

                "get_object_size_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_object_size,
                ),

                "merkle_add_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::merkle::merkle_add,
                ),

                "sparse_merkle_insert_batch_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::smt::sparse_merkle_insert_batch,
                ),

                "merkle_anchor_add_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::merkle_anchor::merkle_anchor_add,
                ),

                "get_verifying_block_height_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_verifying_block_height,
                ),

                "get_block_target_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_block_target,
                ),

                "get_tx_hash_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_tx_hash,
                ),

                "get_call_index_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_call_index,
                ),

                "get_blockchain_time_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_blockchain_time,
                ),

                "get_last_block_height_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_last_block_height,
                ),

                "get_tx_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_tx,
                ),

                "get_tx_location_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_tx_location,
                ),

                "get_block_hash_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_block_hash_,
                ),

                "emit_spend_hook_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::emit_spend_hook,
                ),
            }
        };

        debug!(target: "runtime::vm_runtime", "Instantiating module");
        let instance = Arc::new(Instance::new(&mut store, &module, &imports)?);

        let env_mut = ctx.as_mut(&mut store);
        env_mut.memory = Some(instance.exports.get_with_generics(MEMORY)?);
        env_mut.instance = Some(Arc::clone(&instance));

        Ok(Self { instance, store, ctx })
    }

    /// Call a contract method defined by a [`ContractSection`] using a supplied
    /// payload. Returns a `Vec<u8>` corresponding to the result data of the call.
    /// For calls that do not return any data, an empty `Vec<u8>` is returned.
    fn call(&mut self, section: ContractSection, payload: &[u8]) -> Result<Vec<u8>> {
        debug!(target: "runtime::vm_runtime", "Calling {} method", section.name());

        let env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = section;
        // Verify contract's return data is empty, or quit.
        assert!(env_mut.contract_return_data.take().is_none());

        // Clear the logs and objects between sections.
        // objects accumulates host function return data (db_get, etc.) and
        // must be cleared between metadata/exec/spend_hook/apply to prevent
        // unbounded memory growth during WASM execution.
        let _ = env_mut.logs.take();
        env_mut.objects.borrow_mut().clear();

        // Serialize the payload for the format the wasm runtime is expecting.
        let payload = Self::serialize_payload(&env_mut.contract_id, payload);

        // Allocate enough memory for the payload and copy it into the memory.
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        // HAZOP M-14: charge gas proportional to memory growth.
        // Previously memory.grow cost 1 gas per opcode (uniform cost model).
        // Now charges per page to make memory exhaustion attacks expensive.
        let current_pages = self.memory_pages();
        if pages_required as u64 > current_pages {
            let new_pages = pages_required as u64 - current_pages;
            let gas_cost = new_pages * WASM_PAGE_SIZE as u64;
            match get_remaining_points(&mut self.store, &self.instance) {
                MeteringPoints::Remaining(rem) => {
                    if gas_cost > rem {
                        set_remaining_points(&mut self.store, &self.instance, 0);
                        return Err(Error::WasmerRuntimeError(
                            "Gas exhausted during memory allocation".into(),
                        ));
                    }
                    set_remaining_points(&mut self.store, &self.instance, rem - gas_cost);
                }
                MeteringPoints::Exhausted => {
                    set_remaining_points(&mut self.store, &self.instance, 0);
                    return Err(Error::WasmerRuntimeError(
                        "Gas exhausted during memory allocation".into(),
                    ));
                }
            }
        }
        self.set_memory_page_size(pages_required as u32)?;
        self.copy_to_memory(&payload)?;

        debug!(target: "runtime::vm_runtime", "Getting {} function", section.name());
        let entrypoint = self.instance.exports.get_function(section.name())?;

        // Call the entrypoint. On success, `call` returns a WASM [`Value`]. (The
        // value may be empty.) This value functions similarly to a UNIX exit code.
        // The following section is intended to unwrap the exit code and handle fatal
        // errors in the Wasmer runtime. The value itself and the return data of the
        // contract are processed later.
        debug!(target: "runtime::vm_runtime", "Executing wasm");
        #[cfg(debug_assertions)]
        eprintln!("[VM-DIAG] About to call WASM section: {}", section.name());
        // HAZOP M-13: wall-clock timeout defense-in-depth.
        // Gas metering bounds instruction count but not wall-clock time.
        // A contract with 400M expensive opcodes can consume unbounded CPU.
        let call_start = std::time::Instant::now();
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0_i32)]) {
            Ok(retvals) => {
                let elapsed = call_start.elapsed();
                // MAX_WASM_CALL_TIME is a soft limit — exceeded calls log a
                // warning but don't fail. Hard enforcement would require
                // cooperative yield in the WASM metering middleware.
                const MAX_WASM_CALL_TIME: std::time::Duration = std::time::Duration::from_secs(30);
                if elapsed > MAX_WASM_CALL_TIME {
                    tracing::warn!(target: "runtime::vm_runtime",
                        "[WASM] {} took {:.1}s (max recommended: {}s) — possible DoS",
                        section.name(), elapsed.as_secs_f64(), MAX_WASM_CALL_TIME.as_secs());
                }
                #[cfg(debug_assertions)]
                eprintln!("[VM-DIAG] WASM section {} returned OK", section.name());
                self.print_logs();
                info!(target: "runtime::vm_runtime", "[WASM] {}", self.gas_info());
                retvals
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("[VM-DIAG] WASM section {} FAILED: {:?}", section.name(), e);
                self.print_logs();
                info!(target: "runtime::vm_runtime", "[WASM] {}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                error!(target: "runtime::vm_runtime", "[WASM] Wasmer Runtime Error: {e:#?}");
                return Err(e.into())
            }
        };

        debug!(target: "runtime::vm_runtime", "wasm executed successfully");

        // Move the contract's return data into `retdata`.
        let env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Null;
        let retdata = env_mut.contract_return_data.take().unwrap_or_default();

        // Determine the return value of the contract call. If `ret` is empty,
        // assumed that the contract call was successful.
        let retval: i64 = match ret.len() {
            0 => {
                // Return a success value if there is no return value from
                // the contract.
                debug!(target: "runtime::vm_runtime", "Contract has no return value (expected)");
                wasm::entrypoint::SUCCESS
            }
            _ => {
                match ret[0] {
                    Value::I64(v) => {
                        debug!(target: "runtime::vm_runtime", "Contract returned: {:?}", ret[0]);
                        v
                    }
                    // The only supported return type is i64, so panic if another
                    // value is returned.
                    _ => unreachable!("Got unexpected result return value: {ret:?}"),
                }
            }
        };

        // Check the integer return value of the call. A value of `entrypoint::SUCCESS` (i.e. zero)
        // corresponds to a successful contract call; in this case, we return the contract's
        // result data. Otherwise, map the integer return value to a [`ContractError`].
        match retval {
            wasm::entrypoint::SUCCESS => Ok(retdata),
            _ => {
                // Surface WASM-side msg!() context before the logs are cleared.
                // Per contract-wasm-type-system.md §5.5: every error SHALL be
                // accompanied by a msg!() identifying the contract, function,
                // and specific failure. The i64 ABI cannot carry the string,
                // so we recover it from the log buffer.
                self.print_logs();
                let mut err = dwow_sdk::error::ContractError::from(retval);
                // If the contract left a msg!() before returning the error,
                // propagate it into the IoError so callers see the real cause
                // instead of "Unknown".
                if let dwow_sdk::error::ContractError::IoError(_) = &err {
                    let logs = self.ctx.as_ref(&self.store).logs.borrow();
                    if let Some(last_msg) = logs.last() {
                        err = dwow_sdk::error::ContractError::IoError(last_msg.clone());
                    }
                }
                error!(target: "runtime::vm_runtime", "[WASM] Contract returned: {err:?}");
                Err(Error::ContractError(err))
            }
        }
    }

    /// This function runs when a smart contract is initially deployed, or re-deployed.
    ///
    /// The runtime will look for an `__initialize` symbol in the wasm code, and execute
    /// it if found. Optionally, it is possible to pass in a payload for any kind of special
    /// instructions the developer wants to manage in the initialize function.
    ///
    /// This process is supposed to set up the overlay trees for storing the smart contract
    /// state, and it can create, delete, modify, read, and write to databases it's allowed to.
    /// The permissions for this are handled by the `ContractId` in the overlay db API so we
    /// assume that the contract is only able to do write operations on its own overlay trees.
    pub fn deploy(&mut self, payload: &[u8]) -> Result<()> {
        let cid = self.ctx.as_ref(&self.store).contract_id;
        info!(target: "runtime::vm_runtime", "[WASM] Running deploy() for ContractID: {cid}");

        // Scoped for borrows
        {
            let env_mut = self.ctx.as_mut(&mut self.store);

            // Open or create the zkas db tree for this contract
            let zkas_tree_handle = match env_mut.backend.contract_lookup(&env_mut.contract_id, SMART_CONTRACT_ZKAS_DB_NAME) {
                Ok(v) => v,
                Err(_) => env_mut.backend.contract_init(&env_mut.contract_id, SMART_CONTRACT_ZKAS_DB_NAME)?,
            };

            // Create the monotree db tree for this contract,
            // if it doesn't exists.
            if env_mut.backend.contract_lookup(&env_mut.contract_id, SMART_CONTRACT_MONOTREE_DB_NAME).is_err() {
                env_mut.backend.contract_init(&env_mut.contract_id, SMART_CONTRACT_MONOTREE_DB_NAME)?;
            }

            let mut db_handles = env_mut.db_handles.borrow_mut();
            db_handles.push(DbHandle::new(env_mut.contract_id, zkas_tree_handle));
        }

        //debug!(target: "runtime::vm_runtime", "[WASM] payload: {payload:?}");
        let _ = self.call(ContractSection::Deploy, payload)?;

        // Update the wasm bincode in the ContractStore wasm tree if the deploy exec passed successfully.
        let env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.backend.contract_insert_bincode(env_mut.contract_id, &env_mut.contract_bincode)?;

        info!(target: "runtime::vm_runtime", "[WASM] Successfully deployed ContractID: {cid}");
        Ok(())
    }

    /// This function runs first in the entire scheme of executing a smart contract.
    ///
    /// The runtime will look for a `__metadata` symbol in the wasm code and execute it.
    /// It is supposed to correctly extract public inputs for any ZK proofs included
    /// in the contract calls, and also extract the public keys used to verify the
    /// call/transaction signatures.
    pub fn metadata(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let cid = self.ctx.as_ref(&self.store).contract_id;
        info!(target: "runtime::vm_runtime", "[WASM] Running metadata() for ContractID: {cid}");

        debug!(target: "runtime::vm_runtime", "metadata payload: {}", payload.hex());
        let ret = self.call(ContractSection::Metadata, payload)?;
        debug!(target: "runtime::vm_runtime", "metadata returned: {:?}", ret.hex());

        info!(target: "runtime::vm_runtime", "[WASM] Successfully got metadata ContractID: {cid}");
        Ok(ret)
    }

    /// This function runs when someone wants to execute a smart contract.
    ///
    /// The runtime will look for an `__entrypoint` symbol in the wasm code, and
    /// execute it if found. A payload is also passed as an instruction that can
    /// be used inside the vm by the runtime.
    pub fn exec(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let cid = self.ctx.as_ref(&self.store).contract_id;
        info!(target: "runtime::vm_runtime", "[WASM] Running exec() for ContractID: {cid}");

        debug!(target: "runtime::vm_runtime", "exec payload: {}", payload.hex());
        let ret = self.call(ContractSection::Exec, payload)?;
        debug!(target: "runtime::vm_runtime", "exec returned: {:?}", ret.hex());

        info!(target: "runtime::vm_runtime", "[WASM] Successfully executed ContractID: {cid}");
        Ok(ret)
    }

    /// This function runs after successful execution of `exec` and applies the
    /// state change to the overlay databases.
    ///
    /// The runtime looks for an `__update` symbol in the wasm code and executes
    /// it if found. The caller passes the state update returned by `exec` as
    /// `update`; it is copied into wasm memory as the entrypoint payload (the
    /// wasm side reads it there — there is no `env` side-channel).
    pub fn apply(&mut self, update: &[u8]) -> Result<()> {
        let cid = self.ctx.as_ref(&self.store).contract_id;
        info!(target: "runtime::vm_runtime", "[WASM] Running apply() for ContractID: {cid}");

        debug!(target: "runtime::vm_runtime", "apply payload: {:?}", update.hex());
        let ret = self.call(ContractSection::Update, update)?;
        debug!(target: "runtime::vm_runtime", "apply returned: {:?}", ret.hex());

        info!(target: "runtime::vm_runtime", "[WASM] Successfully applied ContractID: {cid}");
        Ok(())
    }

    /// This function runs a spend_hook callback on a target contract.
    ///
    /// The runtime will look for a `__spend_hook` symbol in the wasm code and execute
    /// it. A payload is passed containing the burn details (nullifiers, value_commits,
    /// token_commits, user_data_encs) so the target can verify the burn and act on it.
    pub fn spend_hook(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let cid = self.ctx.as_ref(&self.store).contract_id;
        info!(target: "runtime::vm_runtime", "[WASM] Running spend_hook() for ContractID: {cid}");

        debug!(target: "runtime::vm_runtime", "spend_hook payload: {}", payload.hex());
        let ret = self.call(ContractSection::SpendHook, payload)?;
        debug!(target: "runtime::vm_runtime", "spend_hook returned: {:?}", ret.hex());

        info!(target: "runtime::vm_runtime", "[WASM] Successfully executed spend_hook ContractID: {cid}");
        Ok(ret)
    }

    /// Prints the wasm contract logs.
    fn print_logs(&self) {
        let logs = self.ctx.as_ref(&self.store).logs.borrow();
        for msg in logs.iter() {
            info!(target: "runtime::vm_runtime", "[WASM] Contract log: {msg}");
        }
    }

    /// Calculate the remaining gas using wasm's concept
    /// of metering points.
    pub fn gas_used(&mut self) -> u64 {
        let remaining_points = get_remaining_points(&mut self.store, &self.instance);

        match remaining_points {
            MeteringPoints::Remaining(rem) => {
                if rem > GAS_LIMIT {
                    // This should never occur, but catch it explicitly to avoid
                    // potential underflow issues when calculating `remaining_points`.
                    unreachable!("Remaining wasm points exceed GAS_LIMIT");
                }
                GAS_LIMIT - rem
            }
            MeteringPoints::Exhausted => GAS_LIMIT + 1,
        }
    }

    // Return a message informing the user whether there is any
    // gas remaining. Values equal to GAS_LIMIT are not considered
    // to be exhausted. e.g. Using 100/100 gas should not give a
    // 'gas exhausted' message.
    fn gas_info(&mut self) -> String {
        let gas_used = self.gas_used();

        if gas_used > GAS_LIMIT {
            format!("Gas fully exhausted: {gas_used}/{GAS_LIMIT}")
        } else {
            format!("Gas used: {gas_used}/{GAS_LIMIT}")
        }
    }

    /// Get the current number of memory pages.
    fn memory_pages(&self) -> u64 {
        let env = self.ctx.as_ref(&self.store);
        env.memory.as_ref().map_or(0, |m| m.size(&self.store).0 as u64)
    }

    /// Set the memory page size. Returns the previous memory size.
    fn set_memory_page_size(&mut self, pages: u32) -> Result<Pages> {
        // Grab memory by value
        let memory = self.take_memory();
        // Modify the memory
        let ret = memory.grow(&mut self.store, Pages(pages))?;
        // Replace the memory back again
        self.ctx.as_mut(&mut self.store).memory = Some(memory);
        Ok(ret)
    }

    /// Take Memory by value. Needed to modify the Memory object
    /// Will panic if memory isn't set.
    fn take_memory(&mut self) -> Memory {
        let env_memory = &mut self.ctx.as_mut(&mut self.store).memory;
        let memory = env_memory.take();
        memory.expect("memory should be set")
    }

    /// Copy payload to the start of the memory
    fn copy_to_memory(&self, payload: &[u8]) -> Result<()> {
        // Payload is copied to index 0.
        // Get the memory view
        let env = self.ctx.as_ref(&self.store);
        let memory_view = env.memory_view(&self.store);
        memory_view.write_slice(payload, 0)
    }

    /// Serialize contract payload to the format accepted by the runtime functions.
    /// We keep the same payload as a slice of bytes, and prepend it with a [`ContractId`],
    /// and then a little-endian u64 to tell the payload's length.
    fn serialize_payload(cid: &ContractId, payload: &[u8]) -> Vec<u8> {
        let ser_cid = serialize(cid);
        let payload_len = payload.len();
        let mut out = Vec::with_capacity(ser_cid.len() + 8 + payload_len);
        out.extend_from_slice(&ser_cid);
        out.extend_from_slice(&(payload_len as u64).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }
}
