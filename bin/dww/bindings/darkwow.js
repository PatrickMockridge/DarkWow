/**
 * DarkWow Wallet — Node.js FFI binding.
 *
 *   npm install darkwow  // (future)
 *   // or just copy this file — only depends on ffi-napi
 *
 *   const { DarkWow } = require('./darkwow');
 *   const w = new DarkWow('keys.toml', 'wallet-1', 'testnet');
 *   const n = w.scanBlock(blockJson);
 *   console.log(`scanned ${n} outputs, balance=${w.balance()}, caps=${w.capCount()}`);
 *   w.close();
 *
 * C ABI design follows seatuya (https://github.com/moebiusV/seatuya):
 * write the protocol binding once in C, and every language gets it through FFI.
 */

// Using ffi-napi (npm install ffi-napi) or bun:ffi for Bun runtime.
// Falls back to bun:ffi if running under Bun.
const isBun = typeof Bun !== 'undefined';

let lib;
if (isBun) {
    const { dlopen, FFIType, suffix } = require('bun:ffi');
    const libPath = process.env.DARKWOW_LIB || `libdwow_wallet.${suffix}`;
    lib = dlopen(libPath, {
        dwow_wallet_version: { args: [], returns: FFIType.cstring },
        dwow_wallet_open: { args: [FFIType.cstring, FFIType.cstring, FFIType.cstring], returns: FFIType.pointer },
        dwow_wallet_free: { args: [FFIType.pointer], returns: FFIType.void },
        dwow_wallet_scan_block_json: { args: [FFIType.pointer, FFIType.cstring], returns: FFIType.i32 },
        dwow_wallet_cap_count: { args: [FFIType.pointer], returns: FFIType.i32 },
        dwow_wallet_balance: { args: [FFIType.pointer], returns: FFIType.u64 },
        dwow_wallet_chain_height: { args: [FFIType.pointer], returns: FFIType.u64 },
        dwow_wallet_default_address: { args: [FFIType.pointer, FFIType.pointer, FFIType.i32], returns: FFIType.i32 },
        dwow_wallet_aead_self_test: { args: [FFIType.pointer], returns: FFIType.i32 },
    });
    lib.symbols = lib.symbols;
} else {
    const ffi = require('ffi-napi');
    const ref = require('ref-napi');
    const path = require('path');

    const libPath = process.env.DARKWOW_LIB || (
        process.platform === 'linux' ? 'libdwow_wallet.so' :
        process.platform === 'darwin' ? 'libdwow_wallet.dylib' :
        'dwow_wallet.dll'
    );

    lib = ffi.Library(libPath, {
        'dwow_wallet_version': ['string', []],
        'dwow_wallet_open': ['pointer', ['string', 'string', 'string']],
        'dwow_wallet_free': ['void', ['pointer']],
        'dwow_wallet_scan_block_json': ['int', ['pointer', 'string']],
        'dwow_wallet_cap_count': ['int', ['pointer']],
        'dwow_wallet_balance': ['uint64', ['pointer']],
        'dwow_wallet_chain_height': ['uint64', ['pointer']],
        'dwow_wallet_default_address': ['int', ['pointer', 'pointer', 'int']],
        'dwow_wallet_aead_self_test': ['int', ['pointer']],
    });
}

class DarkWow {
    /**
     * Open a DarkWow wallet from a keys.toml file.
     * @param {string} keysPath - path to keys.toml
     * @param {string} section - TOML section name (e.g. "wallet-1")
     * @param {string} network - "testnet" or "mainnet"
     */
    constructor(keysPath, section, network = 'testnet') {
        this._handle = lib.dwow_wallet_open(keysPath, section, network);
        if (!this._handle) {
            throw new Error('dwow_wallet_open failed — check keys_path, section, and network');
        }
    }

    close() {
        if (this._handle) {
            lib.dwow_wallet_free(this._handle);
            this._handle = null;
        }
    }

    /** Scan a block (JSON format). Returns number of outputs discovered. */
    scanBlock(blockJson) {
        return lib.dwow_wallet_scan_block_json(this._handle, blockJson);
    }

    /** Total active held capabilities. */
    capCount() {
        return lib.dwow_wallet_cap_count(this._handle);
    }

    /** Sum of all unspent native token values (base units). */
    balance() {
        return lib.dwow_wallet_balance(this._handle);
    }

    /** Current local chain tip height. */
    chainHeight() {
        return lib.dwow_wallet_chain_height(this._handle);
    }

    /** Wallet's default address as a string. */
    defaultAddress() {
        const buf = Buffer.alloc(128);
        const n = lib.dwow_wallet_default_address(this._handle, buf, 128);
        if (n < 0) throw new Error('dwow_wallet_default_address failed');
        return buf.toString('utf8', 0, n);
    }

    /** AEAD encrypt/decrypt roundtrip. Returns true on success. */
    aeadSelfTest() {
        return lib.dwow_wallet_aead_self_test(this._handle) === 0;
    }

    /** Library version string. */
    static version() {
        return lib.dwow_wallet_version();
    }
}

module.exports = { DarkWow };
