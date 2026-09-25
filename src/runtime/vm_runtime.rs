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
// Singlepass, unconditionally. A `cranelift-compiler` feature used to swap this
// alias, so `make test` — which runs `--all-features` — exercised a different WASM
// backend from the one the node ships. The two are not guaranteed to agree, and the
// scanner below allows scalar floats *because* they are deterministic within one
// backend. Removed 2026-09-25; see the note in `Cargo.toml` where the feature stood.
use wasmer_compiler_singlepass::Singlepass as Compiler;
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
        #[expect(clippy::unwrap_used, reason = "memory is initialized before use")]
        let m = self.memory.as_ref().unwrap();
        m
    }

    /// Subtract given gas cost from remaining gas in the current runtime.
    /// Returns true if gas was exhausted by this subtraction (caller should
    /// reject any state-mutating operations).
    pub fn subtract_gas(&mut self, ctx: &mut impl AsStoreMut, gas: u64) -> bool {
        #[expect(clippy::unwrap_used, reason = "instance is set during Runtime::new")]
        let instance = self.instance.as_ref().unwrap();
        match get_remaining_points(ctx, instance) {
            MeteringPoints::Remaining(rem) => {
                if gas > rem {
                    set_remaining_points(ctx, instance, 0);
                    true // gas exhausted
                } else {
                    set_remaining_points(ctx, instance, rem - gas);
                    false
                }
            }
            MeteringPoints::Exhausted => {
                set_remaining_points(ctx, instance, 0);
                true // already exhausted
            }
        }
    }

    /// HAZOP H8: check whether gas is exhausted. State-mutating host functions
    /// MUST call this after subtract_gas and return an error if true.
    pub fn is_gas_exhausted(&self, ctx: &mut impl AsStoreMut) -> bool {
        #[expect(clippy::unwrap_used, reason = "instance is set during Runtime::new")]
        let instance = self.instance.as_ref().unwrap();
        matches!(get_remaining_points(ctx, instance), MeteringPoints::Exhausted)
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
    /// Pre-public-testnet audit H-7 fix: reject SIMD (0xFD) and threads/atomics
    /// (0xFE) opcodes in WASM code section bodies. Uses wasmparser's
    /// `OperatorsReader` to correctly distinguish opcode prefix bytes from
    /// operand data — the prior byte-by-byte scan flagged 0xFE/0xFD bytes
    /// within LEB128 immediates, locals declarations, and function body sizes
    /// as false positives (e.g., deployooor offset 2146).
    ///
    /// Scalar float opcodes (0x8A..=0xBF) are IEEE-754 deterministic within
    /// a single wasmer backend and are NOT rejected.
    fn reject_nondeterministic_features(wasm_bytes: &[u8]) -> Result<()> {
        use wasmer::wasmparser::{Parser, Payload};

        let parser = Parser::new(0);
        for payload in parser.parse_all(wasm_bytes) {
            let payload = payload.map_err(|e| {
                Error::NonDeterministicWasm(format!(
                    "WASM binary parse error at offset {}: {}",
                    e.offset(),
                    e.message()
                ))
            })?;

            if let Payload::CodeSectionEntry(body) = payload {
                let mut ops = body.get_operators_reader().map_err(|e| {
                    Error::NonDeterministicWasm(format!(
                        "WASM function body parse error: {}",
                        e.message()
                    ))
                })?;

                while !ops.eof() {
                    let pos = ops.original_position();
                    let byte = wasm_bytes[pos];
                    if byte == 0xFD || byte == 0xFE {
                        let name = if byte == 0xFD {
                            "SIMD (0xFD)"
                        } else {
                            "threads/atomics (0xFE)"
                        };
                        return Err(Error::NonDeterministicWasm(format!(
                            "Non-deterministic WASM opcode {} at offset {} — \
                             rejected per consensus determinism requirement \
                             (contract-wasm-type-system.md A.8.4)",
                            name, pos
                        )));
                    }
                    // read() consumes the ENTIRE instruction:
                    // multi-byte opcode + all operands (LEB128, br_table, etc.)
                    ops.read().map_err(|e| {
                        Error::NonDeterministicWasm(format!(
                            "WASM opcode parse error at offset {}: {}",
                            e.offset(),
                            e.message()
                        ))
                    })?;
                }
            }
        }
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

        // HAZOP H10 fix: reject WASM binaries that use the non-deterministic feature
        // set — SIMD (0xFD) and threads/atomics (0xFE) — before module compilation.
        //
        // Scalar floats (0x8A..=0xBF) are deliberately ALLOWED, and this comment said
        // the opposite until 2026-09-25 (it claimed floats and bulk memory are
        // rejected; this function's own doc has always been the accurate description).
        // Corrected here because this is the line a reader consults to decide what the
        // guard protects. The allowance is sound only because there is now exactly one
        // backend: wasm floats are IEEE-754, but only "deterministic within a single
        // wasmer backend" — which was this repository's own stated reason for the
        // scanner, back when `cranelift-compiler` could silently swap the backend.
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

        // Every section export the host calls must have the signature the host calls it
        // with — `(i32) -> i64`. That is checked **here**, host-side, because the
        // deploy-time validator inside the Deployooor contract checks only that these
        // exports *exist as functions*, never their signatures.
        //
        // The consequence of the missing check was a node halt: an artifact exporting
        // `__entrypoint() -> i32` was deployable, reached `Runtime::call`, and returned a
        // `Value::I32` where an `i64` was expected — hitting
        // `unreachable!("Got unexpected result return value")` in the host. There is no
        // `catch_unwind` around a contract call, so that is a panic in the consensus
        // path, triggered from a deployed artifact.
        //
        // Host-side rather than in the contract for two reasons: it covers *every*
        // module rather than only those deployed through Deployooor, and Deployooor is a
        // genesis contract, so an edit there would rebuild an artifact and move the
        // genesis pin for a check the host can make for free.
        {
            const SECTION_EXPORTS: [&str; 4] =
                ["__initialize", "__metadata", "__entrypoint", "__update"];
            for export in module.exports() {
                if !SECTION_EXPORTS.contains(&export.name()) {
                    continue;
                }
                let wasmer::ExternType::Function(ty) = export.ty() else {
                    return Err(Error::WasmerRuntimeError(format!(
                        "contract export `{}` is not a function",
                        export.name()
                    )));
                };
                let params = ty.params();
                let results = ty.results();
                let signature_ok = params.len() == 1
                    && params[0] == wasmer::Type::I32
                    && results.len() == 1
                    && results[0] == wasmer::Type::I64;
                if !signature_ok {
                    return Err(Error::WasmerRuntimeError(format!(
                        "contract export `{}` has signature ({:?}) -> ({:?}); the host calls \
                         every section as (i32) -> i64",
                        export.name(),
                        params,
                        results
                    )));
                }
            }
        }


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
        // (The payload's address and the page count are computed together, below,
        // because the payload is placed *above* the guest's live data rather than
        // at offset 0.)
        // REMOVED 2026-09-25 — a `HAZOP M-14` block stood here that charged gas
        // proportional to this growth (`new_pages * WASM_PAGE_SIZE`) and could
        // return "Gas exhausted during memory allocation".
        //
        // It is deleted because it charged for the wrong thing: this growth is
        // HOST-driven — the line below copies the payload in — so it is not an
        // attack surface, and its only lever was to return an error on an honest
        // call. An adversarial audit measured its magnitude and confirms it was
        // never a bound: for deployooor's ~1.49 MB payload it cost 327,680 gas,
        // 0.082% of `GAS_LIMIT`, and 8.1% in the worst case the frame caps permit.
        // The allocation it nominally policed was bounded all along by the frame
        // codec upstream.
        //
        // A justification that stood here is CORRECTED, because it was false. It
        // said metering "already charges the contract's own `memory.grow` per
        // opcode". The metering middleware charges **8 gas flat per instruction
        // whatever the page count** (see the cost tiers above) and does not account
        // `MemoryGrow` at all. So nothing prices attacker-chosen memory growth —
        // before or after this change. That gap is recorded rather than papered
        // over: a contract can request up to 65536 pages in a single `memory.grow`
        // for 8 gas. Related and UNVERIFIED: `MemoryFill` is likewise 8 gas flat for
        // any length with bulk-memory enabled, which would make a multi-GiB fill
        // nearly free. Flagged, not fixed, and not introduced here.
        // The payload is stashed **below `__heap_base`** — the shadow stack's unused
        // space, which is the one region the guest's allocator never hands out. It fits
        // only while `payload.len()` is under `__stack_pointer`, which is **1,048,576**
        // in all 32 artifacts, with `.rodata` beginning exactly at that address and the
        // heap at `__heap_base` (1,151,648) above it.
        //
        // Past that the write covers `.rodata`, the bss tail and the allocator's own
        // state — and the guest's first `malloc` returns payload bytes. That is the
        // `out of bounds memory access` in `dlmalloc::malloc` → `finish_grow` →
        // `dwow_serial::deserialize` that deployooor's own 1,494,589-byte artifact
        // produced: it is the only artifact over the window, which is why it was the
        // only one of 32 that failed.
        //
        // So an oversized payload is **refused**, loudly, rather than written. This is
        // a *policy* limit, not a validity verdict — the transaction is not malformed,
        // it asks for something this ABI cannot carry. Making the ABI carry it is a
        // guest-side change (the payload must be placed where the allocator cannot
        // reach, which the guest has to be told about), and it is its own unit.
        //
        // An earlier attempt moved the payload to the memory's pre-growth end and
        // handed the guest that address. That was WRONG and the tree said so: the
        // pre-growth end is only ~28 KB above `__heap_base`, so the payload sat inside
        // the heap and the guest's own allocations overwrote it — which surfaced
        // further along the deploy as `unreachable` inside `wasmparser`, validating
        // bytes that were no longer the artifact.
        const GUEST_STACK_WINDOW: usize = 1_048_576;
        if payload.len() >= GUEST_STACK_WINDOW {
            return Err(Error::WasmerRuntimeError(format!(
                "payload of {} bytes does not fit the guest's stack window ({} bytes): the \
                 host stashes the payload below `__heap_base`, the only region the guest's \
                 allocator never returns, and a larger payload would be written over \
                 `.rodata` and the allocator's state",
                payload.len(),
                GUEST_STACK_WINDOW
            )))
        }
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
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
        // The guest reads the payload from offset 0 — the shadow-stack region the
        // allocator never returns. See `copy_to_memory` for why that offset, and for the
        // refusal that keeps the payload inside it.
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
                    // A non-`i64` return kind. The export-signature check in
                    // `Runtime::new` makes this unreachable — but it must not *panic*:
                    // this is the consensus path, there is no `catch_unwind` around a
                    // contract call, and `unreachable!` here means a deployed artifact
                    // can halt the node. A bad callback fails the *call*, not the process.
                    _ => {
                        return Err(Error::WasmerRuntimeError(format!(
                            "contract returned an unexpected value kind: {:?} — the host \
                             calls every section as (i32) -> i64",
                            ret[0]
                        )))
                    }
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
                self.print_logs();
                eprintln!("[DIAG] raw WASM exit code: {} (0x{:x})", retval, retval);
                let mut err = dwow_sdk::error::ContractError::from(retval);
                eprintln!("[DIAG] reconstructed error: {:?}", err);
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

    /// Grow the memory so it is at least `pages` pages. Returns nothing; the
    /// previous size was discarded by the single caller.
    ///
    /// (`memory_pages()` stood above this until 2026-09-25. Its only caller was
    /// the removed `HAZOP M-14` block, so it went with it.)
    fn set_memory_page_size(&mut self, pages: u32) -> Result<()> {
        // Grab memory by value
        let memory = self.take_memory();
        // `grow_at_least`, NOT `grow`. wasmer's `Memory::grow(store, delta)` takes a
        // DELTA, and this caller computes an ABSOLUTE requirement
        // (`payload.len() / WASM_PAGE_SIZE + 1`) — so the old `grow(Pages(pages))`
        // grew the memory BY the absolute figure on every contract call instead of
        // TO it. A contract's memory therefore over-allocated by that figure on each
        // of its calls.
        //
        // An adversarial audit corrected the severity that stood here: this said the
        // memory "would eventually pass its maximum and fail `CouldNotGrow`, on a
        // chain that had done nothing wrong". It would not. A fresh `Runtime` is
        // built per call job and at most three sections run on one, so the old
        // ceiling was `18 + 3 × pages_required` — 1,557 pages for the largest
        // permitted payload, against a 65,536-page maximum. The real defect was a
        // ~3x over-allocation, not a cap blowout, and `grow_at_least` fixes it.
        //
        // `grow_at_least` is the existing wasmer API whose meaning matches this
        // caller, and it is idempotent: if the memory is already big enough it does
        // nothing. Verified against the backend (wasmer-vm 6.1.0
        // `src/memory.rs:128-137`): `min_size` is in BYTES — it compares against
        // `self.size.bytes()` and converts the byte growth to pages itself — so the
        // page count is converted here.
        let min_size = (pages as u64) * WASM_PAGE_SIZE as u64;
        memory.grow_at_least(&mut self.store, min_size)?;
        // Replace the memory back again
        self.ctx.as_mut(&mut self.store).memory = Some(memory);
        Ok(())
    }

    /// Take Memory by value. Needed to modify the Memory object
    /// Will panic if memory isn't set.
    fn take_memory(&mut self) -> Memory {
        let env_memory = &mut self.ctx.as_mut(&mut self.store).memory;
        let memory = env_memory.take();
        #[expect(clippy::expect_used, reason = "memory is always set before take_memory is called")]
        let m = memory.expect("memory should be set");
        m
    }

    /// Copy payload to the start of the memory
    /// Copy the payload into guest memory at offset 0.
    ///
    /// Offset 0 is not arbitrary: it is the guest's **shadow stack** region, which lies
    /// below `__heap_base` and is therefore the one region the guest's allocator never
    /// hands out. The payload fits only below `__stack_pointer` — 1,048,576, uniform
    /// across all 32 artifacts — and the caller **refuses** anything larger rather than
    /// writing it over `.rodata` and the allocator's state. See the call site.
    ///
    /// (An attempt on 2026-09-25 moved this write to the memory's pre-growth end and
    /// handed the guest that address. It was wrong: the pre-growth end sits ~28 KB above
    /// `__heap_base`, i.e. *inside* the heap, so the guest's own allocations overwrote
    /// the payload.)
    fn copy_to_memory(&self, payload: &[u8]) -> Result<()> {
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

/// `OBL-C17` — the non-determinism scanner, which was enforced on every instantiation and had no
/// test at all.
///
/// The rule the row states is that the scanner rejects threads and atomics (`0xFE`); the function
/// also rejects SIMD (`0xFD`) and **deliberately allows scalar floats** (`0x8A..=0xBF`), because they
/// are IEEE-754 deterministic within a backend. That last part is why the acceptance control below is
/// not optional: without it, a scanner that rejected every module would pass the rejection tests and
/// look correct. (The call site's comment claims floats and bulk memory are rejected; the function's
/// own doc says otherwise, and the doc is the accurate one — recorded in the register rather than
/// fixed, since a comment-only edit to a runtime file is not worth the diff.)
#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but well-formed module whose single function body is `body`, after the locals
    /// declaration. The type and function sections are present so the code section has something to
    /// refer to — `parse_all` is a streaming parser and the scanner only looks at code-section
    /// entries, but a module it cannot walk to the code section would make these tests vacuous.
    fn module_with_body(body: &[u8]) -> Vec<u8> {
        let mut full = vec![0x00]; // no locals
        full.extend_from_slice(body);
        full.push(0x0B); // end

        let mut out = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]; // \0asm, version 1
        out.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]); // type section: () -> ()
        out.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]); // function section: one func, type 0
        out.push(0x0A); // code section
        out.push((1 + 1 + full.len()) as u8); // section size (all sizes here are < 128)
        out.push(0x01); // one function body
        out.push(full.len() as u8);
        out.extend_from_slice(&full);
        out
    }

    #[test]
    fn scanner_rejects_threads_and_atomics() {
        // `0xFE` is the threads/atomics prefix; the operands are give the reader something to advance
        // past, though the scanner refuses at the prefix byte before reading the operator.
        let module = module_with_body(&[0xFE, 0x00, 0x00, 0x00, 0x00]);
        let err = Runtime::reject_nondeterministic_features(&module).unwrap_err();
        let text = format!("{err:?}");
        assert!(text.contains("threads/atomics"), "got {text}");
    }

    #[test]
    fn scanner_rejects_simd() {
        let module = module_with_body(&[0xFD, 0x00, 0x00, 0x00, 0x00]);
        let err = Runtime::reject_nondeterministic_features(&module).unwrap_err();
        let text = format!("{err:?}");
        assert!(text.contains("SIMD"), "got {text}");
    }

    #[test]
    fn scanner_accepts_scalar_floats() {
        // The control for the two rejections above: `0x92` is `f32.add`, a scalar float opcode the
        // function documents as deliberately permitted.
        let module = module_with_body(&[0x92]);
        assert!(
            Runtime::reject_nondeterministic_features(&module).is_ok(),
            "scalar floats are deterministic within a backend and must not be rejected"
        );
    }

    #[test]
    fn scanner_accepts_an_empty_module() {
        let module = vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert!(Runtime::reject_nondeterministic_features(&module).is_ok());
    }
}
