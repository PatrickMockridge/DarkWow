//! FeeThreshold_V1 proving WASM widget — wallet-side circuit carrier.
//!
//! This is NOT a contract. It is a minimal cdylib WASM module with two raw
//! `extern "C"` exports following Mudra's verifier pattern. The wallet loads
//! this module via Wasmer, reads the circuit binary from WASM memory, decodes
//! `ZkBinary` to learn witness order, then creates proofs natively (Halo2
//! needs rayon — proof creation cannot run in WASM).
//!
//! Exports:
//! - `get_zkbin_ptr() -> *const u8`
//! - `get_zkbin_len() -> usize`

static ZKBIN: &[u8] = include_bytes!("../../../src/contract/native_token/proof/fee_threshold_v1.zk.bin");

/// Return a pointer to the embedded circuit binary in WASM linear memory.
#[no_mangle]
pub extern "C" fn get_zkbin_ptr() -> *const u8 {
    ZKBIN.as_ptr()
}

/// Return the length of the embedded circuit binary in bytes.
#[no_mangle]
pub extern "C" fn get_zkbin_len() -> usize {
    ZKBIN.len()
}
