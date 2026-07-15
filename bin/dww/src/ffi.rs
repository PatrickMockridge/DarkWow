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
use std::path::Path;
use std::sync::Arc;

use dwow_sdk::crypto::{keypair::Network, ContractId};

use crate::walletdb::{WalletDb, WalletPtr};
use crate::Dww;

// ============================================================================
// Opaque handle types
// ============================================================================

pub struct AccountManagerHandle(dwow_accounts::AccountManager);
pub struct WalletDbHandle(WalletPtr);
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
    height: u32,
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
        let addr = mgr.per_block_address(&cid, height).ok()?;
        let addr_str = addr.to_string();
        let bytes = addr_str.as_bytes();
        let len = bytes.len() + 1; // include NUL
        if len > out_len as usize { return None; }
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_address as *mut u8, bytes.len());
            std::ptr::write(out_address.add(bytes.len()), 0u8);
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
                crate::sync_task::HighestPeerTip(
                    std::sync::atomic::AtomicU64::new(0),
                ),
            ),
            last_synced_tip_hash: smol::lock::Mutex::new(None),
            verified_anchor_height: smol::lock::Mutex::new(0),
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
            highest_peer_tip: Arc::new(HighestPeerTip(AtomicU64::new(0))),
            last_synced_tip_hash: smol::lock::Mutex::new(None),
            verified_anchor_height: smol::lock::Mutex::new(0),
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
    if handle.is_null() { return -1; }
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
pub extern "C" fn dwow_wallet_cap_height(handle: *const CapRecordHandle) -> u32 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.created_at_height }
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
/// Symbol name keeps `token_id` until T4 (C-ABI surface rename, coordinated).
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_token_id(
    handle: *const CapRecordHandle,
    out_buf: *mut u8,
    buf_len: i32,
) -> i32 {
    if handle.is_null() || buf_len < 32 { return -1; }
    let bytes = unsafe { (*handle).cap_record.asset_id }.to_bytes();
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf, 32); }
    32
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
    "Get the manifest capability name (e.g. \"coin\", \"credential\").");
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

/// Get the amount at which this capability was revoked, or 0 if unspent.
#[no_mangle]
pub extern "C" fn dwow_wallet_cap_revoked_at_height(handle: *const CapRecordHandle) -> u32 {
    if handle.is_null() { return 0; }
    unsafe { (*handle).cap_record.revoked_at_height.unwrap_or(0) }
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
    wallet.dww.chain_height().unwrap_or(0)
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
// FFI self-tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Verify the FFI lifecycle: open account → derive key → free.
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

        let mut secret = [0u8; 32];
        let ret = dwow_wallet_derive_key(
            account,
            dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes().as_ptr(),
            1,
            secret.as_mut_ptr(),
        );
        assert_eq!(ret, 0);
        assert_ne!(secret, [0u8; 32]);

        let mut secret2 = [0u8; 32];
        dwow_wallet_derive_key(
            account,
            dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes().as_ptr(),
            2,
            secret2.as_mut_ptr(),
        );
        assert_ne!(secret, secret2);

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
}
