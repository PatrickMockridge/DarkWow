//! Shared test modules — imported by tests that need them (RG-MODULAR).
//! Each module has a single responsibility traceable to heavyweight-spec.md.

pub mod chain_setup;
pub mod coinbase_coordination;
pub mod deploy_router;
pub mod determinism;
pub mod endpoint_exercise;
pub mod integrity_checks;
pub mod nullifier_replay;
pub mod block_submission;
pub mod uncle_helpers;
pub mod witness_helpers;
