//! Error type bridge — converts harness errors to dwow_core::Error.
//!
//! Used by: Attestation (9 methods), native_token (3 methods), oracle (5 methods).
//! Spec: RG-MODULAR (2+ contracts).
//!
//! Some harnesses return `Result<T, Box<dyn std::error::Error>>` while
//! the uniform runner expects `dwow_core::Result<T>`.
//! Use: `.map_err(error_bridge::bridge)?` in generate closures.

/// Bridge any Display+Debug error into dwow_core::Error::Custom.
pub fn bridge(e: impl std::fmt::Display + std::fmt::Debug) -> dwow_core::Error {
    dwow_core::Error::Custom(format!("{e}"))
}
