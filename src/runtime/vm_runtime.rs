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
    /// HAZOP H10: reject WASM binaries containing non-deterministic features
    /// (floating-point operations, bulk memory, SIMD). These can produce
    /// different results on different wasmer backends or architectures.
    /// Uses byte-level opcode scanning — no external parser dependency.
    /// NOTE: scans ALL bytes, not just code sections. Float byte patterns
    /// in non-code sections (data, name, custom) will also be rejected.
    /// This is fail-closed: false positives are safe, and production
    /// contracts should not contain these opcodes in any section.
    fn reject_nondeterministic_features(wasm_bytes: &[u8]) -> Result<()> {
        let mut i = 0;
        while i < wasm_bytes.len() {
            match wasm_bytes[i] {
                // WASM float opcodes (f32/f64):
                // 0x7A-0x7D: f32/f64 abs, neg, ceil, floor, trunc, nearest, sqrt
                // 0x8A-0x9F: f32/f64 add, sub, mul, div, min, max, copysign
                // 0xB0-0xBF: f32/f64 eq, ne, lt, gt, le, ge, convert, reinterpret
                0x8A..=0x9F | 0x7A..=0x7D | 0xB0..=0xBF => {
                    return Err(Error::WasmerCompileError(
                        "WASM module uses floating-point (f32/f64) — non-deterministic across architectures".into()
                    ));
                }
                // 0xFC prefix: bulk-memory (0x08-0x0B = memory.copy/fill/init, data.drop)
                0xFC => {
                    if i + 1 < wasm_bytes.len() && matches!(wasm_bytes[i + 1], 0x08..=0x0B) {
                        return Err(Error::WasmerCompileError(
                            "WASM module uses bulk-memory (memory.copy/fill/init) — non-deterministic across backends".into()
                        ));
                    }
                }
                // 0xFD prefix: SIMD
                0xFD => {
                    return Err(Error::WasmerCompileError(
                        "WASM module uses SIMD (0xFD) — non-deterministic across architectures".into()
                    ));
                }
                // HAZOP H-7 fix: 0xFE prefix — WASM threads/atomics
                // (atomic.notify, atomic.wait32/64, atomic.fence, atomic RMW ops).
                // Atomic operations produce non-deterministic results across CPU
                // architectures and memory models.
                0xFE => {
                    return Err(Error::WasmerCompileError(
                        "WASM module uses threads/atomics (0xFE) — non-deterministic across architectures".into()
                    ));
                }
                _ => {}
            }
            i += 1;
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

        // HAZOP H10 fix: reject WASM binaries that use non-deterministic features
        // (floating-point, bulk memory, SIMD) before module compilation. These can
        // produce different results on different wasmer backends (Singlepass vs
        // Cranelift) or architectures, causing consensus splits.
        Self::reject_nondeterministic_features(wasm_bytes)?;

        let cost_function = |_operator: &Operator| -> u64 { 1 };
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
            let env_mut = self.ctx.as_mut(&mut self.store);
            if env_mut.charge_gas(&mut self.store, new_pages * WASM_PAGE_SIZE as u64) {
                return Err(Error::WasmerRuntimeError("Gas exhausted during memory allocation".into()));
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
