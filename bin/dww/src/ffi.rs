/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! C-compatible FFI exports for the DarkWow wallet.
//!
//! Every wallet implementation — GUI, CLI, mobile, embedded — links these
//! same functions. The type system guarantees all wallets see identical
//! scan results for identical chain data.
//!
//! # Design
//!
//! The C ABI pattern follows seatuya (<https://github.com/moebiusV/seatuya>):
//! opaque handles, enum error codes, caller-provided buffers, NULL-safe,
//! no Rust types crossing the boundary. Write the protocol binding once
//! in C, and every language gets it through FFI.
//!
//! # Opaque handle types
//!
//! All Rust types cross the FFI boundary as opaque pointers. Callers
//! must pair every `dwow_wallet_*` constructor with its corresponding
//! destructor (`dwow_wallet_free_*`).
//!
//! # Error convention
//!
//! - Functions returning pointers: NULL on error
//! - Functions returning integers: -1 on error, >= 0 on success
//! - Functions filling output buffers: -1 on error, bytes written on success

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dwow_sdk::crypto::{keypair::Network, ContractId};
use dwow_sdk::pasta::pallas;

use crate::walletdb::{WalletDb, WalletPtr};
use crate::Dww;

// ============================================================================
// Opaque handle types
// ============================================================================

pub struct AccountManagerHandle(dwow_accounts::AccountManager);
pub struct CapRecordHandle {
    cap_record: crate::walletdb::CapRecord,
    merkle_proof: crate::walletdb::MerkleProof,
}
pub struct WalletHandle {
    dww: Dww,
    _wallet: WalletPtr,
    last_error: std::cell::RefCell<Option<String>>,
}

// ============================================================================
// Version
// ============================================================================

/// Return the library version string (e.g. "0.5.0").
/// The returned pointer is static — do not free.
#[no_mangle]
pub extern "C" fn dwow_wallet_version() -> *const c_char {
    // Leak a CString to get a static lifetime. Called once, negligible leak.
    static VERSION: std::sync::OnceLock<CString> = std::sync::OnceLock::new();
    #[expect(clippy::unwrap_used, reason = "CARGO_PKG_VERSION is a valid C string (no NUL, compile-time)")]
    let s = VERSION.get_or_init(|| {
        CString::new(env!("CARGO_PKG_VERSION")).unwrap()
    });
    s.as_ptr()
}

// ============================================================================
// Error retrieval
// ============================================================================

/// Get the last error message for a wallet handle.
/// Writes a NUL-terminated string into `out_buf`.
/// Returns bytes written (excluding NUL), or -1 on error.
/// After retrieval, the stored error is cleared.
#[no_mangle]
pub extern "C" fn dwow_wallet_last_error(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let mut err = wallet.last_error.borrow_mut();
    if let Some(ref msg) = *err {
        let s = match CString::new(msg.as_str()) { Ok(s) => s, Err(_) => return -1 };
        let bytes = s.as_bytes_with_nul();
        if bytes.len() > buf_len as usize { return -1; }
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
        let len = (bytes.len() - 1) as i32;
        *err = None; // cleared after retrieval
        len
    } else {
        0 // no error stored
    }
}

// ============================================================================
// Lifecycle
// ============================================================================

/// Open a wallet identity from a keys.toml file.
///
/// Returns opaque handle, or NULL on error.
/// Free with `dwow_wallet_free_account`.
#[no_mangle]
pub extern "C" fn dwow_wallet_open_account(
    keys_path: *const c_char,
    section: *const c_char,
    network: *const c_char,
) -> *mut AccountManagerHandle {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // CStr::from_ptr on a NULL pointer is UB (SIGSEGV), which catch_unwind
        // cannot catch. Guard every path pointer before dereferencing.
        if keys_path.is_null() || section.is_null() || network.is_null() {
            return None;
        }
        let keys_path = unsafe { CStr::from_ptr(keys_path) }.to_str().ok()?;
        let section = unsafe { CStr::from_ptr(section) }.to_str().ok()?;
        let network_str = unsafe { CStr::from_ptr(network) }.to_str().ok()?;
        let net = match network_str {
            "mainnet" => Network::Mainnet,
            _ => Network::Testnet,
        };
        let mgr = dwow_accounts::AccountManager::open(
            Path::new(keys_path), net, section,
        )
        .ok()?;
        Some(Box::new(AccountManagerHandle(mgr)))
    }));
    match result {
        Ok(Some(b)) => Box::into_raw(b),
        _ => std::ptr::null_mut(),
    }
}

/// Free an AccountManager handle.
#[no_mangle]
pub extern "C" fn dwow_wallet_free_account(handle: *mut AccountManagerHandle) {
    if handle.is_null() { return; }
    let _ = unsafe { Box::from_raw(handle) };
}

/// Free a CapRecord handle.
#[no_mangle]
pub extern "C" fn dwow_wallet_free_cap(handle: *mut CapRecordHandle) {
    if handle.is_null() { return; }
    let _ = unsafe { Box::from_raw(handle) };
}

/// Free a full wallet handle.
#[no_mangle]
pub extern "C" fn dwow_wallet_free(handle: *mut WalletHandle) {
    if handle.is_null() { return; }
    let _ = unsafe { Box::from_raw(handle) };
}

// ============================================================================
// Key derivation — miner-wallet symmetry
// ============================================================================

/// Derive the per-block address for a given contract and height.
///
/// Uses `AccountManager::per_block_address` — the sanctioned delegation path
/// (wallet.md §0.1.3): derives `sk_H = derive_instance(default_owned_sk, cid,
/// height.to_le_bytes())`, computes the public key, formats a Testnet address,
/// and writes it to `out_address` as a C string (max 64 bytes including NUL).
///
/// Same derivation as the mining node — deterministic, zero shared state.
/// NEVER exports raw secret bytes (owned-secret discipline, wallet.md §4).
///
/// Returns the number of bytes written to out_address (including NUL), or 0
/// on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_derive_address(
    handle: *const AccountManagerHandle,
    contract_id: *const u8,
    height: u64,
    out_address: *mut c_char,
    out_len: i32,
) -> i32 {
    if handle.is_null() || contract_id.is_null() || out_address.is_null() || out_len < 64 {
        return 0;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mgr = unsafe { &(*handle).0 };
        let cid_bytes: [u8; 32] = unsafe { std::slice::from_raw_parts(contract_id, 32) }
            .try_into()
            .ok()?;
        let cid = ContractId::from_bytes(cid_bytes).ok()?;
        let addr = mgr.per_block_address(&cid, &height.to_le_bytes()).ok()?;
        let addr_str = addr.to_string();
        let bytes = addr_str.as_bytes();
        let len = bytes.len() + 1; // include NUL
        if len > out_len as usize { return None; }
        unsafe {
            let out = out_address as *mut u8;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
            std::ptr::write(out.add(bytes.len()), 0u8);
        }
        Some(len as i32)
    }));
    match result {
        Ok(Some(v)) => v,
        _ => 0,
    }
}

// ============================================================================
// Full wallet — AccountManager + WalletDb + scanner
// ============================================================================

/// Open a full wallet instance (AccountManager + in-memory WalletDb + Dww).
///
/// Initializes SQLite schema, loads lifecycle keys.
/// No P2P, no network — pure scan engine.
///
/// Returns opaque handle, or NULL on error.
/// Free with `dwow_wallet_free`.
#[no_mangle]
pub extern "C" fn dwow_wallet_open(
    keys_path: *const c_char,
    section: *const c_char,
    network: *const c_char,
) -> *mut WalletHandle {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // CStr::from_ptr on a NULL pointer is UB (SIGSEGV), which catch_unwind
        // cannot catch. Guard every path pointer before dereferencing.
        if keys_path.is_null() || section.is_null() || network.is_null() {
            return None;
        }
        let keys_path = unsafe { CStr::from_ptr(keys_path) }.to_str().ok()?;
        let section = unsafe { CStr::from_ptr(section) }.to_str().ok()?;
        let network_str = unsafe { CStr::from_ptr(network) }.to_str().ok()?;
        let net = match network_str {
            "mainnet" => Network::Mainnet,
            _ => Network::Testnet,
        };

        let wallet = WalletDb::new(None, None, false).ok()?;
        let account_mgr = dwow_accounts::AccountManager::open(
            Path::new(keys_path), net, section,
        )
        .ok()?;

        let dww = Dww {
            network: net,
            account_mgr,
            wallet: wallet.clone(),
            p2p: None,
            executor: None,
            p2p_settings: None,
            highest_peer_tip: Arc::new(
                crate::sync_task::HighestPeerTip::new(),
            ),
            local_genesis_hash: smol::lock::Mutex::new(None),
            verified_anchor_height: smol::lock::Mutex::new(dwow_sdk::blockchain::BlockHeight::new(0)),
            burn_pk_cache: smol::lock::Mutex::new(None),
            mint_pk_cache: smol::lock::Mutex::new(None),
        };
        dww.initialize_wallet().ok()?;

        Some(Box::new(WalletHandle { dww, _wallet: wallet, last_error: std::cell::RefCell::new(None) }))
    }));
    match result {
        Ok(Some(b)) => Box::into_raw(b),
        _ => std::ptr::null_mut(),
    }
}

/// Open a persistent wallet instance (AccountManager + on-disk WalletDb + Dww).
///
/// @param db_path   Path to wallet.db (SQLite file)
/// @param password  Password for encrypted DB (pass empty string for none)
/// @param production  Non-zero for production mode (HMAC checks enabled)
///
/// Returns opaque handle, or NULL on error.
/// Free with `dwow_wallet_free`.
#[no_mangle]
pub extern "C" fn dwow_wallet_open_persistent(
    keys_path: *const c_char,
    section: *const c_char,
    network: *const c_char,
    db_path: *const c_char,
    password: *const c_char,
    production: i32,
) -> *mut WalletHandle {
    if keys_path.is_null() || section.is_null() || network.is_null() || db_path.is_null() {
        return std::ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let keys_path_str = unsafe { CStr::from_ptr(keys_path) }.to_str().ok()?;
        let section_str = unsafe { CStr::from_ptr(section) }.to_str().ok()?;
        let network_str = unsafe { CStr::from_ptr(network) }.to_str().ok()?;
        let db_path_str = unsafe { CStr::from_ptr(db_path) }.to_str().ok()?;
        let pass_str = if password.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(password) }.to_str().ok()?.to_string()
        };
        let net = match network_str {
            "mainnet" => Network::Mainnet,
            _ => Network::Testnet,
        };
        let prod = production != 0;

        let wallet = WalletDb::new(
            Some(PathBuf::from(db_path_str)),
            Some(&pass_str),
            prod,
        ).ok()?;
        let account_mgr = dwow_accounts::AccountManager::open(
            Path::new(keys_path_str), net, section_str,
        ).ok()?;

        let dww = Dww {
            network: net,
            account_mgr,
            wallet: wallet.clone(),
            p2p: None,
            executor: None,
            p2p_settings: None,
            highest_peer_tip: Arc::new(crate::sync_task::HighestPeerTip::new()),
            local_genesis_hash: smol::lock::Mutex::new(None),
            verified_anchor_height: smol::lock::Mutex::new(dwow_sdk::blockchain::BlockHeight::new(0)),
            burn_pk_cache: smol::lock::Mutex::new(None),
            mint_pk_cache: smol::lock::Mutex::new(None),
        };
        dww.initialize_wallet().ok()?;
        Some(Box::new(WalletHandle { dww, _wallet: wallet, last_error: std::cell::RefCell::new(None) }))
    }));
    match result {
        Ok(Some(b)) => Box::into_raw(b),
        _ => std::ptr::null_mut(),
    }
}

// ============================================================================
// Scan — full pipeline (pure scan + SQLite persistence)
// ============================================================================

/// Scan a block through the full wallet pipeline.
///
/// Uses `Dww::scan_block_linear` — Merkle tree checkpoint, manifest
/// pre-loading, AEAD decryption, and capability persistence to the
/// wallet DB. Every wallet implementation gets identical results.
///
/// Returns number of native token outputs discovered (>= 0), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_scan_block_json(
    handle: *mut WalletHandle,
    block_json: *const c_char,
) -> i32 {
    if handle.is_null() || block_json.is_null() { return -1; }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let wallet = unsafe { &mut (*handle) };
        let json = unsafe { CStr::from_ptr(block_json) }.to_str().ok()?;
        let block: dwow_chain::Block = serde_json::from_str(json).ok()?;
        let mut tree = wallet.dww.get_capability_commitment_tree().ok()?;
        let result = wallet.dww.scan_block_linear(&mut tree, &block).ok()?;
        Some(result.native_outputs.len() as i32)
    }));
    match result {
        Ok(Some(v)) => v,
        _ => {
            // last_error may be set by scan_block_linear internally;
            // write a fallback if the outer catch_unwind caught a panic.
            if let Ok(Some(v)) = result { return v; }
            -1
        }
    }
}

// ============================================================================
// Capabilities — read CapRecord fields from the wallet DB
// ============================================================================

/// Get the total held capability count from the wallet database.
///
/// Returns count (>= 0), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_count(handle: *const WalletHandle) -> i32 {
    // NULL handle → 0, consistent with chain_height/balance (empty, not error).
    if handle.is_null() { return 0; }
    let wallet = unsafe { &(*handle) };
    match wallet._wallet.get_held_capabilities(Some(false)) {
        Ok(caps) => caps.len() as i32,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("cap_count: {:?}", e));
            -1
        }
    }
}

/// Get a capability by index from the wallet database.
///
/// Returns opaque handle, or NULL if index out of bounds or on error.
/// Free with `dwow_wallet_free_cap`.
#[no_mangle]
pub extern "C" fn dwow_wallet_get_cap(
    handle: *const WalletHandle,
    index: i32,
) -> *mut CapRecordHandle {
    if handle.is_null() { return std::ptr::null_mut(); }
    let wallet = unsafe { &(*handle) };
    let caps = match wallet._wallet.get_held_capabilities(Some(false)) {
        Ok(c) => c,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("get_cap: {:?}", e));
            return std::ptr::null_mut();
        }
    };
    if index < 0 || (index as usize) >= caps.len() {
        return std::ptr::null_mut();
    }
    let cap = &caps[index as usize];
    // Merkle proof lookup — optional, returns empty proof on failure
    let merkle_proof = wallet._wallet.get_merkle_proof(&cap.cap_id)
        .unwrap_or(crate::walletdb::MerkleProof {
            siblings: vec![],
            root: String::new(),
            leaf_position: 0,
        });
    let handle = Box::new(CapRecordHandle {
        cap_record: cap.clone(),
        merkle_proof,
    });
    Box::into_raw(handle)
}

// ============================================================================
// Capability field accessors
// ============================================================================

/// Get the value (in base units) of a capability.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_value(handle: *const CapRecordHandle) -> u64 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.value }
}

/// Get the block height at which this capability was created.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_height(handle: *const CapRecordHandle) -> u64 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.created_at_height.get() }
}

/// Get the capability ID as a bs58 string.
///
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_id(
    handle: *const CapRecordHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len <= 0 { return -1; }
    let s = match CString::new(unsafe { &(*handle).cap_record.cap_id }.as_str()) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let bytes = s.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Get the contract ID for a capability (always 32 bytes).
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_contract_id(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len < 32 { return -1; }
    let bytes = unsafe { (*handle).cap_record.contract_id }.to_bytes();
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, 32); }
    32
}

/// Get the Poseidon commitment for a capability (always 32 bytes).
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_commitment(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len < 32 { return -1; }
    let bytes = unsafe { (*handle).cap_record.commitment }.to_bytes();
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, 32); }
    32
}

/// Check if a capability has been revoked (spent).
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_revoked(handle: *const CapRecordHandle) -> i32 {
    if handle.is_null() { return -1; }
    if unsafe { (*handle).cap_record.revoked } { 1 } else { 0 }
}

// ============================================================================
// Balance
// ============================================================================

/// Get the sum of all unspent native token values (balance in base units).
#[no_mangle]
pub extern "C" fn dwow_wallet_balance(handle: *const WalletHandle) -> u64 {
    if handle.is_null() { return 0; }
    let wallet = unsafe { &(*handle) };
    match wallet.dww.capability_balance() {
        Ok(balances) => balances.values().sum(),
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("balance: {:?}", e));
            0
        }
    }
}

// ============================================================================
// Missing capability accessors
// ============================================================================

/// Get the asset ID for a capability (always 32 bytes).
/// Canonical name — T4 rename from `cap_token_id` per o-cap grammar.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_asset_id(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len < 32 { return -1; }
    let bytes = unsafe { (*handle).cap_record.asset_id }.to_bytes();
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, 32); }
    32
}

/// Deprecated alias for `dwow_wallet_cap_asset_id` — T4 rename.
/// Removal target: next major version. Use the canonical name.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_token_id(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    dwow_wallet_cap_asset_id(handle, out_buf, buf_len)
}

/// Get the Merkle tree leaf position for a capability.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_leaf_position(handle: *const CapRecordHandle) -> u64 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.leaf_position }
}

// ── Typed-capability accessors (wallet.md §2.2, ocap.md §6) ─────────

/// Return string from Option<String> into caller buffer. Returns bytes
/// written (excluding NUL), or -1 on error. Writes empty string for None.
fn write_str_opt(opt: &Option<String>, out_buf: *mut c_char, buf_len: i32) -> i32 {
    if out_buf.is_null() || buf_len <= 0 { return -1; }
    let s = opt.as_deref().unwrap_or("");
    let cstr = match CString::new(s) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    s.len() as i32
}

/// Join a slice into a comma-separated string, write to caller buffer.
fn write_csv(items: &[String], out_buf: *mut c_char, buf_len: i32) -> i32 {
    if out_buf.is_null() || buf_len <= 0 { return -1; }
    let csv = items.join(",");
    let cstr = match CString::new(csv) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

macro_rules! cap_str_accessor {
    ($name:ident, $field:ident, $desc:literal) => {
        #[doc = $desc]
        #[no_mangle]
        pub extern "C" fn $name(
            handle: *const CapRecordHandle,
            out_buf: *mut c_char,
            buf_len: i32,
        ) -> i32 {
            if handle.is_null() { return -1; }
            let cap = unsafe { &(*handle).cap_record };
            write_str_opt(&cap.$field, out_buf, buf_len)
        }
    };
}

cap_str_accessor!(dwow_wallet_cap_name, capability_name,
    "Get the manifest capability name (e.g. \"commitment\", \"credential\").");
cap_str_accessor!(dwow_wallet_cap_resource, resource,
    "Get the capability resource identity (ocap.md §3).");
cap_str_accessor!(dwow_wallet_cap_action, action,
    "Get the capability action identity (ocap.md §3).");

/// Get the manifest capability discriminant (u8). Returns 0 if unset.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_discriminant(handle: *const CapRecordHandle) -> u8 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.capability_discriminant.unwrap_or(0) }
}

/// Get the composed primitives as a comma-separated string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_primitives(
    handle: *const CapRecordHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() { return -1; }
    let cap = unsafe { &(*handle).cap_record };
    let names: Vec<String> = cap.primitives.iter().map(|p| p.name().to_string()).collect();
    write_csv(&names, out_buf, buf_len)
}

/// Get the covered barbs as a comma-separated string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_barbs(
    handle: *const CapRecordHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() { return -1; }
    let cap = unsafe { &(*handle).cap_record };
    let names: Vec<String> = cap.barbs.iter().map(|b| b.name().to_string()).collect();
    write_csv(&names, out_buf, buf_len)
}

/// Get the height at which this capability was revoked, or 0 if unspent.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_revoked_at_height(handle: *const CapRecordHandle) -> u64 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.revoked_at_height.map(|h| h.get()).unwrap_or(0) }
}

/// Get the spend hook FuncId as 32 bytes. Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_spend_hook(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len < 32 { return -1; }
    let cap = unsafe { &(*handle).cap_record };
    let hook_bytes: [u8; 32] = cap.spend_hook.map(|f| f.to_bytes()).unwrap_or([0u8; 32]);
    unsafe { std::ptr::copy_nonoverlapping(hook_bytes.as_ptr(), out_buf, 32); }
    0
}

/// Get the FuncId as 32 bytes. Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_func_id(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len < 32 { return -1; }
    let cap = unsafe { &(*handle).cap_record };
    let func_bytes: [u8; 32] = cap.func_id.map(|f| f.to_bytes()).unwrap_or([0u8; 32]);
    unsafe { std::ptr::copy_nonoverlapping(func_bytes.as_ptr(), out_buf, 32); }
    0
}

/// Get the stored Merkle proof for this capability as a JSON array of
/// bs58-encoded sibling strings. Returns bytes written (excluding NUL) or -1.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_merkle_proof(
    handle: *const CapRecordHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let cap_handle = unsafe { &(*handle) };
    let proof = &cap_handle.merkle_proof;
    let json = match serde_json::to_string(proof) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

// ============================================================================
// Additional lifecycle
// ============================================================================

/// Get the wallet's default address as a string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_default_address(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let addr = match wallet.dww.default_address() {
        Ok(a) => a,
        Err(_) => {
            wallet.last_error.borrow_mut().replace("default_address failed".into());
            return -1;
        }
    };
    let s = match CString::new(addr.to_string()) { Ok(s) => s, Err(_) => return -1 };
    let bytes = s.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Get the current chain height from the wallet's local block store.
#[no_mangle]
pub extern "C" fn dwow_wallet_chain_height(handle: *const WalletHandle) -> u64 {
    if handle.is_null() { return 0; }
    let wallet = unsafe { &(*handle) };
    match wallet.dww.chain_height() {
        // spec dispensation: type-system.md §6.4 (FFI boundaries) —
        // C ABI requires primitive types. BlockHeight::get() is the canonical
        // extraction point for the u64 domain value across the FFI boundary.
        Ok(h) => h.get(),
        Err(e) => {
            tracing::error!("FFI chain_height failed: {}", e);
            0
        }
    }
}

/// Run the AEAD encrypt/decrypt self-test.
/// Returns 0 on success, -1 on failure.
#[no_mangle]
pub extern "C" fn dwow_wallet_aead_self_test(handle: *const WalletHandle) -> i32 {
    if handle.is_null() { return -1; }
    let wallet = unsafe { &(*handle) };
    match wallet.dww.aead_self_test() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

// ============================================================================
// Phase 5 — Write Path / Transaction Construction
// ============================================================================
// Generic contract invocation, proof generation, capability selection, and
// transfer construction. All functions follow the same pattern: blocking
// wrappers over async Dww methods, caller-provided output buffer, return
// bytes written (excluding NUL) or -1 on error.

/// Invoke a contract function generically through the manifest path.
/// Handles both ZK and non-ZK functions — proofs are built by the generic
/// prover (wallet.md §6.4.1).
///
/// @param handle       Wallet handle
/// @param contract_id  bs58 contract ID string
/// @param function     Function name (e.g. "transfer")
/// @param params_json  JSON-encoded parameters (or NULL for none)
/// @param out_tx       Output buffer for serialized transaction JSON
/// @param buf_len      Output buffer size
/// @return bytes written (excluding NUL), or -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_invoke_contract(
    handle: *const WalletHandle,
    contract_id: *const c_char,
    function: *const c_char,
    params_json: *const c_char,
    out_tx: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || contract_id.is_null() || function.is_null() || out_tx.is_null() || buf_len <= 0 {
        return -1;
    }
    let wallet = unsafe { &(*handle) };
    let cid = match unsafe { CStr::from_ptr(contract_id) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let func = match unsafe { CStr::from_ptr(function) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let params = if params_json.is_null() {
        None
    } else {
        Some(match unsafe { CStr::from_ptr(params_json) }.to_str() { Ok(s) => s, Err(_) => return -1 })
    };
    let tx = match smol::block_on(wallet.dww.invoke_contract(cid, func, params, vec![], vec![])) {
        Ok(t) => t,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("invoke_contract: {e}"));
            return -1;
        }
    };
    // Serialize tx via dwow_serial (same as dispatch.rs:401-405)
    let tx_b64 = crate::wallet_util::base64_encode(&dwow_serial::serialize(&tx));
    let cstr = match CString::new(tx_b64) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_tx as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Encode contract call parameters from a manifest schema (static, no wallet needed).
/// This is the write-path dual of `decode_note_by_schema` (wallet.md §6.4.1 step 2).
///
/// @param schema_json   JSON array of ParameterField objects
/// @param function      Function name
/// @param params_json   JSON object of parameter values
/// @param out_buf       Output buffer for encoded bytes
/// @param buf_len       Output buffer size
/// @return bytes written (excluding NUL), or -1 on error
#[no_mangle]
pub extern "C" fn dwow_encode_params_by_schema(
    schema_json: *const c_char,
    function: *const c_char,
    params_json: *const c_char,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if schema_json.is_null() || function.is_null() || params_json.is_null() || out_buf.is_null() || buf_len <= 0 {
        return -1;
    }
    let schema_str = match unsafe { CStr::from_ptr(schema_json) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let params_str = match unsafe { CStr::from_ptr(params_json) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let schema: Vec<dwow_sdk::manifest::ParameterField> = match serde_json::from_str(schema_str) {
        Ok(s) => s, Err(_) => return -1,
    };
    let encoded = match dwow_sdk::manifest::encode_params_by_schema(&schema, params_str) {
        Ok(e) => e,
        Err(_e) => {
            // No wallet handle here — use a thread-local error or just return -1.
            // The caller can validate the schema beforehand.
            // Static function — no wallet handle to store error. Return -1;
            // caller can pre-validate schema via the schema JSON itself.
            return -1;
        }
    };
    if encoded.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(encoded.as_ptr(), out_buf as *mut u8, encoded.len()); }
    encoded.len() as i32
}

/// List held capabilities filtered by asset ID.
/// Returns a JSON array of CapInfo objects.
///
/// @param handle       Wallet handle
/// @param asset_id     bs58 asset ID (empty string = all assets)
/// @param revoked      Filter: 0 = unspent only, 1 = spent only, 2 = all
/// @param out_buf      Output buffer for JSON
/// @param buf_len      Output buffer size
/// @return bytes written (excluding NUL), or -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_caps_by_asset(
    handle: *const WalletHandle,
    asset_id: *const c_char,
    revoked: i32,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || asset_id.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let aid = match unsafe { CStr::from_ptr(asset_id) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let revoked_flag = match revoked { 1 => Some(true), 0 => Some(false), _ => None };
    let cap_records = match wallet.dww.wallet.get_held_capabilities(revoked_flag) {
        Ok(c) => c,
        Err(e) => { wallet.last_error.borrow_mut().replace(format!("{:?}", e)); return -1; }
    };
    let caps_json: Vec<serde_json::Value> = cap_records.iter()
        .filter(|c| aid.is_empty() || c.asset_id.to_bytes().to_vec() ==
            bs58::decode(aid).into_vec().unwrap_or_default())
        .map(|c| serde_json::json!({
            "cap_id": c.cap_id,
            "value": c.value,
            "asset_id": bs58::encode(c.asset_id.to_bytes()).into_string(),
            "leaf_position": c.leaf_position,
        }))
        .collect();
    let json = match serde_json::to_string(&caps_json) { Ok(j) => j, Err(_) => return -1 };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Resolve a held capability to its contract and transfer function.
///
/// @param handle       Wallet handle
/// @param cap_id       Capability ID (bs58 string)
/// @param action_name  Action name (e.g. "transfer")
/// @param out_cid      Output buffer for contract ID (bs58 string)
/// @param out_func     Output buffer for function name
/// @param func_len     Output buffer size for function name
/// @return bytes written to out_func (excluding NUL), or -1 on error.
///         out_cid receives the contract ID bs58 string (max 64 bytes needed).
#[no_mangle]
pub extern "C" fn dwow_wallet_resolve_transfer_contract(
    handle: *const WalletHandle,
    cap_id: *const c_char,
    action_name: *const c_char,
    out_cid: *mut c_char,
    out_func: *mut c_char,
    func_len: i32,
) -> i32 {
    if handle.is_null() || cap_id.is_null() || action_name.is_null() || out_cid.is_null() || out_func.is_null()
        || func_len <= 0
    {
        return -1;
    }
    let wallet = unsafe { &(*handle) };
    let cid = match unsafe { CStr::from_ptr(cap_id) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let act = match unsafe { CStr::from_ptr(action_name) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let caps = match wallet.dww.wallet.get_held_capabilities(Some(false)) {
        Ok(c) => c, Err(e) => { wallet.last_error.borrow_mut().replace(format!("{:?}", e)); return -1; }
    };
    let rec = match caps.iter().find(|c| c.cap_id == cid) {
        Some(r) => r, None => { wallet.last_error.borrow_mut().replace("cap not found".to_string()); return -1; }
    };
    let (contract_id, func_name) = match wallet.dww.resolve_transfer_contract(rec, act) {
        Ok(r) => r, Err(e) => { wallet.last_error.borrow_mut().replace(e); return -1; }
    };
    let cid_str = bs58::encode(contract_id.to_bytes()).into_string();
    let cid_cstr = match CString::new(cid_str) { Ok(c) => c, Err(_) => return -1 };
    let func_cstr = match CString::new(func_name) { Ok(c) => c, Err(_) => return -1 };
    // out_cid: bs58 string, max ~48 chars for 32-byte CID
    let cid_bytes = cid_cstr.as_bytes_with_nul();
    if cid_bytes.len() > 64 { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(cid_bytes.as_ptr(), out_cid as *mut u8, cid_bytes.len()); }
    // out_func: function name
    let fbytes = func_cstr.as_bytes_with_nul();
    if fbytes.len() > func_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(fbytes.as_ptr(), out_func as *mut u8, fbytes.len()); }
    (fbytes.len() - 1) as i32
}

/// Store a zkas circuit binary in the wallet's zkas_binaries store (wallet.md §3).
///
/// @param handle       Wallet handle
/// @param contract_id  bs58 contract ID string
/// @param namespace    Circuit namespace (e.g. "FeeCollect_V1")
/// @param circuit_name Circuit name (e.g. "FeeCollect_V1")
/// @param zkas_bytes   Raw zkas binary bytes
/// @param zkas_len     Length of zkas_bytes
/// @return 0 on success, -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_zkas_store(
    handle: *const WalletHandle,
    contract_id: *const c_char,
    namespace: *const c_char,
    circuit_name: *const c_char,
    zkas_bytes: *const u8,
    zkas_len: i32,
) -> i32 {
    if handle.is_null() || contract_id.is_null() || namespace.is_null() || circuit_name.is_null()
        || zkas_bytes.is_null() || zkas_len <= 0
    {
        return -1;
    }
    let wallet = unsafe { &(*handle) };
    let cid = match unsafe { CStr::from_ptr(contract_id) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let ns = match unsafe { CStr::from_ptr(namespace) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let name = match unsafe { CStr::from_ptr(circuit_name) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let bytes = unsafe { std::slice::from_raw_parts(zkas_bytes, zkas_len as usize) };
    match wallet.dww.wallet.store_zkas_binary(cid, ns, name, bytes) {
        Ok(_) => 0,
        Err(e) => { wallet.last_error.borrow_mut().replace(format!("{:?}", e)); -1 }
    }
}

/// Load a zkas circuit binary from the wallet's zkas_binaries store.
///
/// @param handle       Wallet handle
/// @param contract_id  bs58 contract ID string
/// @param namespace    Circuit namespace
/// @param circuit_name Circuit name
/// @param out_buf      Output buffer for zkas bytes
/// @param buf_len      Output buffer size
/// @return bytes written, 0 if not found, -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_zkas_load(
    handle: *const WalletHandle,
    contract_id: *const c_char,
    namespace: *const c_char,
    circuit_name: *const c_char,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || contract_id.is_null() || namespace.is_null() || circuit_name.is_null()
        || out_buf.is_null() || buf_len <= 0
    {
        return -1;
    }
    let wallet = unsafe { &(*handle) };
    let cid = match unsafe { CStr::from_ptr(contract_id) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let ns = match unsafe { CStr::from_ptr(namespace) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let name = match unsafe { CStr::from_ptr(circuit_name) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let result = match wallet.dww.wallet.load_zkas_binary(cid, ns, name) {
        Ok(r) => r,
        Err(e) => { wallet.last_error.borrow_mut().replace(format!("{:?}", e)); return -1; }
    };
    match result {
        Some(bytes) => {
            if bytes.len() > buf_len as usize { return -1; }
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, bytes.len()); }
            bytes.len() as i32
        }
        None => 0,
    }
}

/// List all stored zkas circuit entries as a JSON array.
///
/// @param handle   Wallet handle
/// @param out_buf  Output buffer for JSON
/// @param buf_len  Output buffer size
/// @return bytes written (excluding NUL), or -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_zkas_list(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    // zkas_binaries table has no enumeration API yet — return a diagnostic.
    // Implementation deferred to when the table gains a list query.
    let msg = "{\"note\":\"zkas_list not yet implemented — store/load only\"}";
    let cstr = match CString::new(msg) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Generate a ZK proof from a manifest-declared circuit (generic prover §6.4.1).
///
/// @param handle             Wallet handle
/// @param contract_id        bs58 contract ID string
/// @param witness_map_json   JSON array of witness source strings
/// @param zkas_bytes         Raw zkas binary bytes
/// @param zkas_len           Length of zkas_bytes
/// @param seed               Seed bytes (32 bytes, NULL = zero seed)
/// @param out_proof          Output buffer for encoded proof
/// @param proof_len          Output buffer size
/// @return bytes written, or -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_generate_proof(
    handle: *const WalletHandle,
    contract_id: *const c_char,
    witness_map_json: *const c_char,
    zkas_bytes: *const u8,
    zkas_len: i32,
    seed: *const u8,
    out_proof: *mut u8,
    proof_len: i32,
) -> i32 {
    if handle.is_null() || witness_map_json.is_null() || zkas_bytes.is_null() || zkas_len <= 0
        || out_proof.is_null() || proof_len <= 0
    {
        return -1;
    }
    let wallet = unsafe { &(*handle) };
    let cid = match unsafe { CStr::from_ptr(contract_id) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let wm_str = match unsafe { CStr::from_ptr(witness_map_json) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let entries: Vec<String> = match serde_json::from_str(wm_str) { Ok(e) => e, Err(_) => return -1 };
    let witness_map = match dwow_sdk::prover::CircuitWitnessMap::from_manifest(
        cid.to_string(), "ffi".to_string(), &entries,
    ) { Ok(m) => m, Err(_) => return -1 };
    let zkas = unsafe { std::slice::from_raw_parts(zkas_bytes, zkas_len as usize) };
    let seed_arr: [u8; 32] = if seed.is_null() {
        [0u8; 32]
    } else {
        let s = unsafe { std::slice::from_raw_parts(seed, 32) };
        let mut arr = [0u8; 32]; arr.copy_from_slice(&s[..32.min(s.len())]); arr
    };
    let proof = match crate::prover_impl::create_generic_proof(
        &dwow_sdk::prover::ProverContext::new(
            dwow_sdk::manifest::ContractManifest::empty(),
            cid.to_string(),
            witness_map,
            seed_arr,
        ),
        &crate::prover_impl::ResolvedCapProvider::new(
            vec![], // caller provides pre-resolved caps
            dwow_sdk::crypto::SecretKey::from_base(pallas::Base::zero()),
            vec![],
            0,
        ),
        zkas,
    ) {
        Ok(p) => p,
        Err(e) => { wallet.last_error.borrow_mut().replace(e); return -1; }
    };
    if proof.len() > proof_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(proof.as_ptr(), out_proof, proof.len()); }
    proof.len() as i32
}

// ============================================================================
// Phase 4 — Low-Level Type System Access
// ============================================================================
// Expose Primitive and Barb as first-class FFI types, not just CSV strings.
// Static functions — no wallet handle needed. Callable before wallet open.

// Static arrays for enumeration — Primitive/Barb don't derive EnumIter.
use dwow_sdk::capability::{Primitive, Barb};
static ALL_PRIMITIVES: &[Primitive] = &[
    Primitive::SecretKey, Primitive::PublicKey, Primitive::Nullifier,
    Primitive::Commitment, Primitive::ContractId, Primitive::FuncId,
    Primitive::AssetId, Primitive::MerkleNode, Primitive::OwnedSecretKey,
    Primitive::MiningRecipient,
];
static ALL_BARBS: &[Barb] = &[
    Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Prove,
    Barb::Verify, Barb::Dispatch, Barb::Gate, Barb::Denominate,
    Barb::ProveInclusion, Barb::Encrypt, Barb::Derive, Barb::Discover,
    Barb::Mine, Barb::View,
];

/// Count of known primitive types (type-system.md §8.1).
#[no_mangle]
pub extern "C" fn dwow_primitive_count() -> i32 {
    ALL_PRIMITIVES.len() as i32
}

/// Name of the primitive at `index`. Returns a static string (do not free).
/// Returns NULL if index is out of bounds.
///
/// Names are compiled into static tables (OnceLock) — no allocation, no leak.
/// Previous implementation allocated a fresh CString per call, contradicting
/// the "do not free" contract.
#[no_mangle]
pub extern "C" fn dwow_primitive_name(index: i32) -> *const c_char {
    use std::sync::OnceLock;
    static NAMES: OnceLock<Vec<CString>> = OnceLock::new();
    #[expect(clippy::unwrap_used, reason = "primitive names are static strs without NUL")]
    let names = NAMES.get_or_init(|| {
        ALL_PRIMITIVES.iter().map(|p| CString::new(p.name()).unwrap()).collect()
    });
    if index < 0 || (index as usize) >= names.len() {
        return std::ptr::null();
    }
    names[index as usize].as_ptr()
}

/// Barbs exhibited by the primitive at `index`, as a comma-separated string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_primitive_barbs(
    index: i32,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if out_buf.is_null() || buf_len <= 0 { return -1; }
    if index < 0 || (index as usize) >= ALL_PRIMITIVES.len() { return -1; }
    let barbs = ALL_PRIMITIVES[index as usize].barbs();
    let names: Vec<&str> = barbs.iter().map(|b| b.name()).collect();
    let csv = names.join(",");
    let cstr = match CString::new(csv) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Count of known barbs (type-system.md §1.1).
#[no_mangle]
pub extern "C" fn dwow_barb_count() -> i32 {
    ALL_BARBS.len() as i32
}

/// Name of the barb at `index`. Returns a static string (do not free).
/// Returns NULL if index is out of bounds.
///
/// Names are compiled into static tables (OnceLock) — no allocation, no leak.
/// Previous implementation allocated a fresh CString per call, contradicting
/// the "do not free" contract.
#[no_mangle]
pub extern "C" fn dwow_barb_name(index: i32) -> *const c_char {
    use std::sync::OnceLock;
    static NAMES: OnceLock<Vec<CString>> = OnceLock::new();
    #[expect(clippy::unwrap_used, reason = "barb names are static strs without NUL")]
    let names = NAMES.get_or_init(|| {
        ALL_BARBS.iter().map(|b| CString::new(b.name()).unwrap()).collect()
    });
    if index < 0 || (index as usize) >= names.len() {
        return std::ptr::null();
    }
    names[index as usize].as_ptr()
}

/// wallet_construct soundness gate (wallet.md §2.2, ocap.md §6).
/// Checks whether the given primitives cover all required barbs.
///
/// @param resource      Resource name (e.g. "value")
/// @param action        Action name (e.g. "transfer")
/// @param primitives_csv Comma-separated primitive names
/// @param barbs_csv     Comma-separated required barb names
/// @param out_buf       Output buffer for composed barb CSV (can be NULL to skip)
/// @param buf_len       Output buffer size (0 if out_buf is NULL)
/// @return 1 if covered, 0 if not covered, -1 on error
#[no_mangle]
pub extern "C" fn dwow_wallet_construct(
    resource: *const c_char,
    action: *const c_char,
    primitives_csv: *const c_char,
    barbs_csv: *const c_char,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if resource.is_null() || action.is_null() || primitives_csv.is_null() || barbs_csv.is_null() {
        return -1;
    }
    let resource = match unsafe { CStr::from_ptr(resource) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let action = match unsafe { CStr::from_ptr(action) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let p_csv = match unsafe { CStr::from_ptr(primitives_csv) }.to_str() { Ok(s) => s, Err(_) => return -1 };
    let b_csv = match unsafe { CStr::from_ptr(barbs_csv) }.to_str() { Ok(s) => s, Err(_) => return -1 };

    let primitives = match dwow_sdk::capability::primitives_from_csv(p_csv) {
        Some(p) => p,
        None => return -1,
    };
    let barbs = match dwow_sdk::capability::barbs_from_csv(b_csv) {
        Some(b) => b,
        None => return -1,
    };

    match dwow_sdk::capability::wallet_construct(resource, action, primitives, &barbs) {
        Some(tc) => {
            if !out_buf.is_null() && buf_len > 0 {
                let composed: Vec<&str> = tc.barbs.iter().map(|b| b.name()).collect();
                let csv = composed.join(",");
                if let Ok(cstr) = CString::new(csv) {
                    let bytes = cstr.as_bytes_with_nul();
                    if bytes.len() <= buf_len as usize {
                        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
                    }
                }
            }
            1 // covered
        }
        None => 0, // not covered
    }
}

// ============================================================================
// Phase 1 — Read-Path Gaps
// ============================================================================

/// Check whether the wallet has caught up with the network tip.
/// Returns 1 if synced, 0 if syncing, -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_is_synced(handle: *const WalletHandle) -> i32 {
    if handle.is_null() { return -1; }
    let wallet = unsafe { &(*handle) };
    if wallet.dww.is_synced() { 1 } else { 0 }
}

/// Get per-asset balances as a JSON map string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_balance_by_asset(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let balances = match wallet.dww.capability_balance() {
        Ok(b) => b,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("balance_by_asset: {}", e));
            return -1;
        }
    };
    let json = match serde_json::to_string(&balances) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Get a contract's manifest TOML by its bs58 contract ID string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_manifest(
    handle: *const WalletHandle,
    contract_id_str: *const c_char,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || contract_id_str.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let cid_str = match unsafe { CStr::from_ptr(contract_id_str) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let manifest = match wallet.dww.get_contract_manifest(cid_str) {
        Ok(Some(m)) => m.to_toml().unwrap_or_else(|e| format!("manifest toml: {e}")),
        Ok(None) => { return 0; }
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("manifest: {}", e));
            return -1;
        }
    };
    let cstr = match CString::new(manifest) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Get the native token commitment tree Merkle root (32 bytes).
/// Returns 32 on success, -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_merkle_root(
    handle: *const WalletHandle,
    out_buf: *mut u8,
) -> i32 {
    if handle.is_null() || out_buf.is_null() { return -1; }
    let wallet = unsafe { &(*handle) };
    let tree = match wallet.dww.get_capability_commitment_tree() {
        Ok(t) => t,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("merkle_root: {}", e));
            return -1;
        }
    };
    let root = tree.root(0).map(|n| n.to_bytes()).unwrap_or([0u8; 32]);
    unsafe { std::ptr::copy_nonoverlapping(root.as_ptr(), out_buf, 32); }
    32
}

/// Get all derived addresses as a JSON array of strings.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_addresses(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let addrs = match wallet.dww.addresses() {
        Ok(a) => a,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("addresses: {}", e));
            return -1;
        }
    };
    // addresses() returns Vec<(u64, PublicKey, SecretKey, u64)> — extract labels + pubkey strings
    let addr_strings: Vec<String> = addrs.iter()
        .map(|(label, pk, _, _)| format!("{}:{}", label, pk))
        .collect();
    let json = match serde_json::to_string(&addr_strings) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Get the asset alias map as a JSON object.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_aliases_by_asset(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let aliases = match wallet.dww.get_aliases_mapped_by_asset() {
        Ok(a) => a,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("aliases: {}", e));
            return -1;
        }
    };
    let json = match serde_json::to_string(&aliases) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Batch-retrieve capability IDs as a JSON array of cap_id strings.
/// Fixes O(n^2) per-index pattern. Callers use dwow_wallet_get_cap for details.
/// @param revoked_filter  0 = active only, 1 = revoked only, 2 = all
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_get_cap_batch(
    handle: *const WalletHandle,
    revoked_filter: i32,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let revoked = match revoked_filter {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    let caps = match wallet.dww.get_held_capabilities(revoked) {
        Ok(c) => c,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("cap_batch: {}", e));
            return -1;
        }
    };
    // Serialize just the cap IDs — callers use dwow_wallet_get_cap for full
    // details. cap_id is ALREADY a bs58 string (derive_cap_id) — do NOT
    // re-encode it (double-encoding produced unusable IDs).
    let cap_ids: Vec<String> = caps.iter()
        .map(|c| c.cap_id.clone())
        .collect();
    let json = match serde_json::to_string(&cap_ids) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

// ============================================================================
// Phase 2 — Write-Path Gaps
// ============================================================================

/// Insert a synced block into the wallet's local chain store.
/// Block is passed as JSON (matches scan_block_json format).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_insert_block(
    handle: *mut WalletHandle,
    block_json: *const c_char,
) -> i32 {
    if handle.is_null() || block_json.is_null() { return -1; }
    let wallet = unsafe { &(*handle) };
    let json_str = match unsafe { CStr::from_ptr(block_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let block: dwow_chain::Block = match serde_json::from_str(json_str) {
        Ok(b) => b,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("insert_block parse: {}", e));
            return -1;
        }
    };
    match wallet.dww.insert_synced_block(&block) {
        Ok(()) => 0,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("insert_block: {}", e));
            -1
        }
    }
}

/// Get a block by height as a JSON string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_get_block(
    handle: *const WalletHandle,
    height: u64,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let block = match wallet.dww.chain_block(dwow_sdk::blockchain::BlockHeight::new(height)) {
        Ok(b) => b,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("get_block: {}", e));
            return -1;
        }
    };
    let json = match serde_json::to_string(&block) {
        Ok(j) => j,
        Err(_) => return -1,
    };
    let cstr = match CString::new(json) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

/// Mark a capability as exercised. Accepts a JSON array of nullifier hex strings,
/// e.g. `["a1b2...", "c3d4..."]`. Each nullifier must be 64 hex chars (32 bytes).
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_mark_exercise(
    handle: *mut WalletHandle,
    nullifier_json: *const c_char,
) -> i32 {
    if handle.is_null() || nullifier_json.is_null() { return -1; }
    let wallet = unsafe { &(*handle) };
    let json_str = match unsafe { CStr::from_ptr(nullifier_json) }.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let nullifier_hexes: Vec<String> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("mark_exercise parse: {}", e));
            return -1;
        }
    };
    let nullifiers: Vec<dwow_chain::Nullifier> = nullifier_hexes.iter().filter_map(|hex| {
        let bytes = hex::decode(hex).ok()?;
        if bytes.len() != 32 { return None; }
        let arr: [u8; 32] = bytes.try_into().ok()?;
        dwow_chain::Nullifier::from_bytes(arr).ok()
    }).collect();
    if nullifiers.is_empty() {
        wallet.last_error.borrow_mut().replace("mark_exercise: no valid nullifiers".into());
        return -1;
    }
    let tx = dwow_core::tx::Transaction {
        calls: vec![],
        proofs: vec![],
        tx_commitment: [0u8; 32],
        nullifiers,
    };
    let mut output = Vec::new();
    match wallet.dww.mark_tx_exercise(&tx, &mut output) {
        Ok(0) => {
            wallet.last_error.borrow_mut().replace("mark_exercise: no held caps matched".into());
            -1
        }
        Ok(_) => 0,
        Err(e) => {
            wallet.last_error.borrow_mut().replace(format!("mark_exercise: {}", e));
            -1
        }
    }
}

/// Run a wallet diagnostic and return the report as a string.
/// Returns bytes written (excluding NUL), or -1 on error.
#[no_mangle]
pub extern "C" fn dwow_wallet_diagnostic(
    handle: *const WalletHandle,
    out_buf: *mut c_char,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || out_buf.is_null() || buf_len <= 0 { return -1; }
    let wallet = unsafe { &(*handle) };
    let mut output: Vec<String> = Vec::new();
    if let Err(e) = wallet.dww.diagnostic(&mut output) {
        wallet.last_error.borrow_mut().replace(format!("diagnostic: {}", e));
        return -1;
    }
    let report = output.join("\n");
    let cstr = match CString::new(report) { Ok(c) => c, Err(_) => return -1 };
    let bytes = cstr.as_bytes_with_nul();
    if bytes.len() > buf_len as usize { return -1; }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len()); }
    (bytes.len() - 1) as i32
}

// ============================================================================
// FFI self-tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Verify the FFI lifecycle: open account → derive address → free.
    /// Uses the same deterministic test key as the wallet integration test.
    #[test]
    fn test_ffi_lifecycle() {
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let path = std::env::temp_dir()
            .join(format!("dwow_ffi_test_{}.toml", std::process::id()));
        std::fs::write(&path, keys_toml).expect("write test keys");

        let keys_cstr = CString::new(path.to_str().unwrap()).unwrap();
        let section = CString::new("node0").unwrap();
        let network = CString::new("testnet").unwrap();

        let account = dwow_wallet_open_account(
            keys_cstr.as_ptr(), section.as_ptr(), network.as_ptr(),
        );
        assert!(!account.is_null());

        let cid = dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes();

        // Derive addresses at two different heights
        let mut addr1 = [0i8; 64];
        let ret1 = dwow_wallet_derive_address(
            account, cid.as_ptr(), 1, addr1.as_mut_ptr(), 64,
        );
        assert!(ret1 > 0, "derive_address height=1 failed");
        let s1 = CStr::from_bytes_until_nul(
            unsafe { std::slice::from_raw_parts(addr1.as_ptr() as *const u8, 64) }
        ).unwrap().to_str().unwrap().to_string();
        assert!(!s1.is_empty());

        let mut addr2 = [0i8; 64];
        let ret2 = dwow_wallet_derive_address(
            account, cid.as_ptr(), 2, addr2.as_mut_ptr(), 64,
        );
        assert!(ret2 > 0, "derive_address height=2 failed");
        let s2 = CStr::from_bytes_until_nul(
            unsafe { std::slice::from_raw_parts(addr2.as_ptr() as *const u8, 64) }
        ).unwrap().to_str().unwrap().to_string();
        assert!(!s2.is_empty());

        // Different heights produce different addresses
        assert_ne!(s1, s2);

        // Determinism: same height returns same address
        let mut addr1b = [0i8; 64];
        let ret1b = dwow_wallet_derive_address(
            account, cid.as_ptr(), 1, addr1b.as_mut_ptr(), 64,
        );
        assert!(ret1b > 0);
        let s1b = CStr::from_bytes_until_nul(
            unsafe { std::slice::from_raw_parts(addr1b.as_ptr() as *const u8, 64) }
        ).unwrap().to_str().unwrap().to_string();
        assert_eq!(s1, s1b);

        dwow_wallet_free_account(account);
        let _ = std::fs::remove_file(&path);
    }

    /// Verify the full wallet FFI lifecycle.
    #[test]
    fn test_ffi_full_wallet() {
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let path = std::env::temp_dir()
            .join(format!("dwow_ffi_wallet_{}.toml", std::process::id()));
        std::fs::write(&path, keys_toml).expect("write test keys");

        let keys_cstr = CString::new(path.to_str().unwrap()).unwrap();
        let section = CString::new("node0").unwrap();
        let network = CString::new("testnet").unwrap();

        let wallet = dwow_wallet_open(
            keys_cstr.as_ptr(), section.as_ptr(), network.as_ptr(),
        );
        assert!(!wallet.is_null());

        assert_eq!(dwow_wallet_cap_count(wallet), 0);
        assert_eq!(dwow_wallet_balance(wallet), 0);
        assert!(dwow_wallet_get_cap(wallet, 0).is_null());

        dwow_wallet_free(wallet);
        let _ = std::fs::remove_file(&path);
    }

    /// BW-10: Verify null-pointer rejection at FFI boundary.
    /// Per type-system.md §10.5: every C FFI entry point SHALL reject NULL
    /// handles with a documented sentinel, not a segfault.
    #[test]
    fn test_ffi_null_pointers_rejected() {
        // dwow_wallet_open_account(NULL, ...) → NULL
        let section = CString::new("node0").unwrap();
        let network = CString::new("testnet").unwrap();
        assert!(dwow_wallet_open_account(
            std::ptr::null(), section.as_ptr(), network.as_ptr(),
        ).is_null());

        // dwow_wallet_open(NULL, ...) → NULL
        assert!(dwow_wallet_open(
            std::ptr::null(), section.as_ptr(), network.as_ptr(),
        ).is_null());

        // dwow_wallet_free(NULL) → no-op (must not segfault)
        dwow_wallet_free(std::ptr::null_mut());

        // dwow_wallet_chain_height(NULL) → 0 (sentinel)
        assert_eq!(dwow_wallet_chain_height(std::ptr::null()), 0);

        // dwow_wallet_balance(NULL) → 0
        assert_eq!(dwow_wallet_balance(std::ptr::null()), 0);

        // dwow_wallet_cap_count(NULL) → 0
        assert_eq!(dwow_wallet_cap_count(std::ptr::null()), 0);

        // dwow_wallet_get_cap(NULL, 0) → NULL
        assert!(dwow_wallet_get_cap(std::ptr::null(), 0).is_null());
    }

    /// BW-11: Verify buffer-length cap enforcement at FFI boundary.
    /// Per type-system.md §10.5: undersized output buffers SHALL be rejected
    /// with a zero return, not a buffer overflow.
    #[test]
    fn test_ffi_buffer_caps_enforced() {
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let path = std::env::temp_dir()
            .join(format!("dwow_ffi_bufcap_{}.toml", std::process::id()));
        std::fs::write(&path, keys_toml).expect("write test keys");

        let keys_cstr = CString::new(path.to_str().unwrap()).unwrap();
        let section = CString::new("node0").unwrap();
        let network = CString::new("testnet").unwrap();

        let account = dwow_wallet_open_account(
            keys_cstr.as_ptr(), section.as_ptr(), network.as_ptr(),
        );
        assert!(!account.is_null());

        let cid = dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes();

        // out_len=0 must return 0 (no bytes written, no overflow)
        let mut buf = [0i8; 1];
        let ret = dwow_wallet_derive_address(
            account, cid.as_ptr(), 1, buf.as_mut_ptr(), 0,
        );
        assert_eq!(ret, 0, "out_len=0 must return 0");

        // out_len=1 must return 0 (buffer too small for any address)
        let ret = dwow_wallet_derive_address(
            account, cid.as_ptr(), 1, buf.as_mut_ptr(), 1,
        );
        assert_eq!(ret, 0, "out_len=1 must return 0");

        dwow_wallet_free_account(account);
        let _ = std::fs::remove_file(&path);
    }

    /// BW-12: catch_unwind isolation at FFI boundary.
    /// Per type-system.md §10.5: a panic inside an FFI-guarded function SHALL
    /// be caught at the boundary and returned as an error code, not abort
    /// the process.
    #[test]
    fn test_ffi_catch_unwind_isolation() {
        // dwow_wallet_last_error(NULL) — NULL handle triggers internal
        // catch_unwind that must return a sentinel, not abort.
        let mut buf = [0i8; 256];
        let ret = dwow_wallet_last_error(
            std::ptr::null(), buf.as_mut_ptr(), 256,
        );
        // NULL handle should return -1 (error sentinel), not panic/abort
        assert!(ret < 0, "NULL handle to last_error must return error sentinel, got {ret}");

        // dwow_wallet_scan_block_json(NULL, ...) — NULL handle with valid
        // JSON must be caught, not abort.
        let json = CString::new("{}").unwrap();
        let ret = dwow_wallet_scan_block_json(
            std::ptr::null_mut(), json.as_ptr(),
        );
        assert!(ret < 0, "NULL handle to scan_block_json must return error sentinel, got {ret}");

        // dwow_wallet_derive_address(NULL, ...) — NULL handle
        let cid = dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes();
        let mut addr = [0i8; 64];
        let ret = dwow_wallet_derive_address(
            std::ptr::null(), cid.as_ptr(), 1, addr.as_mut_ptr(), 64,
        );
        assert!(ret <= 0, "NULL handle to derive_address must return error sentinel, got {ret}");
    }
}
