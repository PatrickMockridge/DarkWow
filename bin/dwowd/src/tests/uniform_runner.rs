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

//! Uniform Heavyweight Test Runner
//!
//! The single standardized test runner for all Level 2 heavyweight tests.
//! Every contract's test provides a `ContractTestSpec` to `run_heavyweight_test()`.
//! The runner composes shared modules (RG-MODULAR) to structurally enforce
//! heavyweight-spec.md requirements.
//!
//! Spec: heavyweight-spec.md §9 (Per-Contract Test Template).

use std::sync::Mutex;

use dwow_core::zk::Proof;
use dwow_core::Result;
use dwow_sdk::crypto::ContractId;
use dwow_contract_test_harness::harness::ContractHarness;

use crate::tests::blockchain::HeavyweightPipeline;
use crate::tests::modules;

// ── Spec Types ──────────────────────────────────────────────────────────────

/// Result of generating proofs + call_data for one contract endpoint.
pub struct EndpointResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
    /// Child contract calls bundled with this call (DFS post-order; the parent
    /// call is last). Empty for contracts that make no cross-contract child calls.
    pub children: Vec<ChildCall>,
}

/// A child contract call bundled under a parent call in a single transaction.
pub struct ChildCall {
    pub contract_id: ContractId,
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
}

/// Whether an endpoint expects accept_block to succeed or reject.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EndpointExpectation {
    /// Normal accept_block acceptance.
    Success,
    /// Expect accept_block to return an error (e.g., MintV1 FunctionDisabled).
    Rejection,
}

/// Specification for a single contract endpoint.
pub struct EndpointSpec<'a> {
    /// Function name (matches function enum variant).
    pub name: &'static str,
    /// Whether this function requires a ZK proof.
    pub is_zk: bool,
    /// Produces call_data + proofs for this endpoint.
    pub generate: Box<dyn Fn() -> Result<EndpointResult> + 'a>,
    /// For FeeV1/BurnV1: uses prefetched coinbase params instead of `generate`.
    pub generate_with_coinbase: Option<Box<dyn Fn(&modules::coinbase_coordination::PrefetchedCoinbase) -> Result<EndpointResult> + 'a>>,
    /// Cross-block state verification (HAZOP finding — compound correctness).
    /// Called after accept_block succeeds. Receives the pipeline for state queries.
    pub verify_state: Option<Box<dyn Fn(&HeavyweightPipeline) -> Result<()> + 'a>>,
    /// Whether this endpoint expects acceptance or rejection.
    pub expectation: EndpointExpectation,
}

/// Full specification for a contract's heavyweight test.
pub struct ContractTestSpec<'a> {
    /// Contract name (matches directory name).
    pub name: &'static str,
    /// Whether this is a genesis contract (uses static ContractId).
    pub is_genesis: bool,
    /// ContractId — static for genesis, derived for WASM.
    pub contract_id: ContractId,
    /// The contract harness.
    pub harness: &'a dyn ContractHarness,
    /// WASM bytes — None for genesis, Some for WASM.
    pub wasm_bytes: Option<&'a [u8]>,
    /// Whether this contract has an InitializeV1 function.
    pub has_initialize: bool,
    /// Generate InitializeV1 call_data (if has_initialize).
    pub initialize: Option<Box<dyn Fn() -> Result<EndpointResult> + 'a>>,
    /// All endpoints in function enum order.
    pub endpoints: Vec<EndpointSpec<'a>>,
    /// Whether any endpoint needs coinbase parameter coordination (native_token only).
    pub needs_coinbase_coordination: bool,
    /// Optional cross-contract setup: issues capabilities (e.g. PN notes via
    /// `PromissoryNoteHarness`) on-chain before the endpoint loop so child calls
    /// can spend them. Runs on both chain A and chain B (determinism replay).
    pub setup: Option<Box<dyn Fn(&HeavyweightPipeline) -> Result<()> + 'a>>,
}

impl<'a> ContractTestSpec<'a> {
    /// Verify the spec is internally consistent before running the test.
    pub fn validate(&self) -> Result<()> {
        if self.is_genesis && self.wasm_bytes.is_some() {
            return Err(dwow_core::Error::Custom(
                "Genesis contract must not have wasm_bytes".into()
            ));
        }
        if !self.is_genesis && self.wasm_bytes.is_none() {
            return Err(dwow_core::Error::Custom(
                "WASM contract must have wasm_bytes".into()
            ));
        }
        Ok(())
    }


    /// Index of the first ZK endpoint (for nullifier replay testing).
    pub fn first_zk_index(&self) -> Option<usize> {
        // Skip endpoints that need coinbase params — they can't be generated
        // standalone for nullifier replay (HAZOP H-UR-013).
        self.endpoints.iter().position(|e| e.is_zk && e.generate_with_coinbase.is_none())
    }
}

// ── Runner ─────────────────────────────────────────────────────────────────

/// The uniform test runner for spec-based heavyweight tests. Spec-based
/// tests (32 contracts) call this. Standalone tests (block execution,
/// relayer lifecycle, fee_v2, metadata) use direct HeavyweightPipeline.
/// Composes shared modules to structurally enforce heavyweight-spec.md §9.
pub async fn run_heavyweight_test(spec: &ContractTestSpec<'_>) -> Result<()> {
    spec.validate()?;

    // ── Pipeline A (primary) ────────────────────────────────────────
    let mut chain_a = modules::chain_setup::init_test_chain().await?;
    chain_a.log_file = Some(Mutex::new(crate::tests::test_output::create_log_file(spec.name)));

    // ── Pre-test integrity checks (spec §5.2) ───────────────────────
    modules::integrity_checks::pre_test_integrity(
        &chain_a, spec.is_genesis, spec.contract_id, spec.harness,
    )?;

    // ── Deploy if WASM ─────────────────────────────────────────────
    let cid = modules::deploy_router::resolve_contract_id(
        &chain_a,
        spec.is_genesis,
        spec.contract_id,
        spec.harness,
        spec.name,
        spec.wasm_bytes,
    ).await?;

    // ── Cross-contract setup (issue capabilities for child calls) ──
    if let Some(ref setup_fn) = spec.setup {
        setup_fn(&chain_a).map_err(|e| dwow_core::Error::Custom(
            format!("TEST-FAIL [{}::setup]: cross-contract setup failed — {}", spec.name, e)
        ))?;
    }

    // ── Initialize (if contract has InitializeV1) ───────────────────
    let mut height_before = chain_a.height();
    if let Some(ref init_fn) = spec.initialize {
        let result = init_fn().map_err(|e| dwow_core::Error::Custom(
            format!("TEST-FAIL [{}::initialize]: InitializeV1 harness failed — {}", spec.name, e)
        ))?;
        assert!(!result.call_data.is_empty(),
            "TEST-FAIL [{}::initialize]: call_data must not be empty", spec.name);
        height_before = modules::block_submission::submit_single_call_block(
            &chain_a, cid, spec.harness,
            &result.call_data, result.proofs, false, // InitializeV1 is non-ZK
        ).await?;
        assert!(height_before > chain_a.height().pred().unwrap(),
            "TEST-FAIL [{}::initialize]: height must advance after InitializeV1", spec.name);
    }

    // ── Exercise every endpoint (one per block) ────────────────────
    // Coinbase coordination for native_token (RG-MODULAR §9)
    let coinbase = if spec.needs_coinbase_coordination {
        Some(modules::coinbase_coordination::prefetch_coinbase_params(&chain_a).await?)
    } else {
        None
    };

    for endpoint in &spec.endpoints {
        // Use generate_with_coinbase if this endpoint needs coinbase params (FeeV1/BurnV1)
        let result = if let Some(ref gen) = endpoint.generate_with_coinbase {
            gen(coinbase.as_ref().expect("needs_coinbase_coordination must be true when generate_with_coinbase is set"))?
        } else {
            (endpoint.generate)().map_err(|e| dwow_core::Error::Custom(
                format!("TEST-FAIL [{}::{}]: harness generate failed — {}", spec.name, endpoint.name, e)
            ))?
        };
        assert!(!result.call_data.is_empty(),
            "TEST-FAIL [{}::{}]: call_data must not be empty", spec.name, endpoint.name);

        if endpoint.expectation == EndpointExpectation::Rejection {
            // Expect accept_block to REJECT this call (e.g., MintV1 FunctionDisabled)
            let submit_result = modules::block_submission::submit_single_call_block(
                &chain_a, cid, spec.harness,
                &result.call_data, result.proofs, endpoint.is_zk,
            ).await;
            assert!(submit_result.is_err(),
                "TEST-FAIL [{}::{}]: expected rejection but accept_block succeeded",
                spec.name, endpoint.name);
        } else if endpoint.generate_with_coinbase.is_some() {
            // Coinbase-dependent endpoints use submit_with_coinbase
            let cb = coinbase.as_ref().expect("needs_coinbase_coordination must be true");
            let new_height = modules::coinbase_coordination::submit_with_coinbase(
                &chain_a, cid, spec.harness,
                &result.call_data, result.proofs, endpoint.is_zk,
                cb.coinbase_tx.clone(),
            ).await?;
            assert!(new_height > height_before,
                "TEST-FAIL [{}::{}]: height must advance after accept_block", spec.name, endpoint.name);
            height_before = new_height;
        } else {
            // Normal acceptance path
            let new_height = modules::endpoint_exercise::exercise_endpoint(
                &chain_a, cid, spec.harness, endpoint, height_before,
            ).await?;
            // Cross-block state verification (HAZOP finding — compound correctness).
            // Red Team FP-1/FP-2: verify_state errors were previously downgraded to
            // warnings, making state checks non-enforcing. Now hard-fail.
            if let Some(ref verify) = endpoint.verify_state {
                verify(&chain_a).map_err(|e| dwow_core::Error::Custom(format!(
                    "TEST-FAIL [{}::{}]: verify_state failed — {}",
                    spec.name, endpoint.name, e
                )))?;
            }
            height_before = new_height;
        }
    }
    drop(coinbase);

    // ── Nullifier replay rejection (spec §3.6) ─────────────────────
    if let Some(idx) = spec.first_zk_index() {
        let endpoint = &spec.endpoints[idx];
        let result = (endpoint.generate)()?;
        modules::nullifier_replay::verify_nullifier_replay(
            &chain_a, cid, spec.harness,
            &result.call_data, result.proofs, endpoint.is_zk,
        ).await?;
    }

    // ── Post-test integrity checks (spec §5.3) ─────────────────────
    modules::integrity_checks::post_test_integrity(&chain_a)?;

    // ── Determinism (spec §3.7) ────────────────────────────────────
    let chain_b = HeavyweightPipeline::new().await?;
    chain_b.init_genesis().await?;
    if let Err(e) = spec.harness.verify_zk_coverage() {
        eprintln!("WARN [integrity_checks]: PI-4 ZK coverage check failed (determinism pipeline) — {}", e);
    }

    let cid_b = modules::deploy_router::resolve_contract_id(
        &chain_b, spec.is_genesis, spec.contract_id,
        spec.harness, spec.name, spec.wasm_bytes,
    ).await?;

    // Replay cross-contract setup on chain B (determinism)
    if let Some(ref setup_fn) = spec.setup {
        setup_fn(&chain_b)?;
    }

    // Replay init on chain B
    if let Some(ref init_fn) = spec.initialize {
        let result = init_fn()?;
        let _ = modules::block_submission::submit_single_call_block(
            &chain_b, cid_b, spec.harness,
            &result.call_data, result.proofs, false,
        ).await?;
    }

    // Replay all endpoints on chain B
    let mut h_b = chain_b.height();
    for endpoint in &spec.endpoints {
        let result = (endpoint.generate)()?;
        if endpoint.expectation == EndpointExpectation::Rejection {
            let _ = modules::block_submission::submit_single_call_block(
                &chain_b, cid_b, spec.harness,
                &result.call_data, result.proofs, endpoint.is_zk,
            ).await;
        } else {
            h_b = modules::endpoint_exercise::exercise_endpoint(
                &chain_b, cid_b, spec.harness, endpoint, h_b,
            ).await?;
        }
    }

    // Compare final block hashes (PI-7)
    let hash_a = chain_a.block_hash_at(chain_a.height())?;
    let hash_b = chain_b.block_hash_at(chain_b.height())?;
    assert_eq!(hash_a, hash_b,
        "INFRA-FAIL [determinism]: PI-7 block hashes must match for {}", spec.name);

    Ok(())
}
