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
use dwow_sdk::blockchain::BlockHeight;
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
///
/// **`children` is what makes a call tree more than two levels deep**, and its absence was a bound
/// nobody had recorded: a cross-contract child that itself validates a child — `dao_escrow`'s
/// `propose_claim_v1` does, `require_governance_child` reads `calls[call_idx].children_indexes[0]`
/// (`dao_escrow/src/entrypoint.rs:1094-1098`) — could not be expressed at all, so an endpoint that
/// needed one could not be driven from any fixture. `labor_market`'s `DisputeV1` is that endpoint
/// (`OBL-C170`'s bound, on the frame side rather than the call-builder side).
///
/// The tree is emitted **DFS post-order** — a node's children precede it, and the parent is last
/// (`build_witness_tree`, `heavyweight_pipeline.rs:102`), which is the order `with_call_tree` already
/// used for one level and the order the verifier reconciles against the transaction's call list.
pub struct ChildCall {
    pub contract_id: ContractId,
    pub call_data: Vec<u8>,
    pub proofs: Vec<Proof>,
    /// This child's own children, in the same shape and the same post-order. Empty for a leaf, which
    /// is every child that existed before 2026-09-29.
    pub children: Vec<ChildCall>,
}

/// Whether an endpoint expects accept_block to succeed or reject.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EndpointExpectation {
    /// Normal accept_block acceptance.
    Success,
    /// Expect a rejection whose text names the **reason** under test, without asserting *which
    /// contract* refused.
    ///
    /// This is the weakest of the three rejection variants and the honest home for a row whose
    /// subject is a *stage* or *reason* that is not attributable to one contract id — a row refused
    /// before any contract runs, or a placeholder that names the stage it stops at. A needle is
    /// satisfied by *any* call in the frame, so a `RejectionNaming` row that carries a child is
    /// weaker than it looks; prefer the blamed variants below wherever the refuser is determinable.
    ///
    /// **A needle alone cannot say *which contract* refused.** Every per-call failure carries the
    /// same shape — `… (contract <id>): ContractError(Custom(N))` — and `N` is a per-contract enum
    /// index, so `Custom(14)` is `DuplicateCommitment` in `promissory_note`, `GovernanceNotActive`
    /// in `dao_escrow` and `UnauthorizedCaller` in `darktoshi_dice`. Use the blamed variants below
    /// when it matters who refused.
    RejectionNaming(&'static [&'static str]),
    /// Expect a rejection **the endpoint itself** produced.
    ///
    /// The refusal's text must name the endpoint's own resolved contract id, as well as containing
    /// each needle. This is what separates "rejected, for the reason under test" from "rejected a
    /// step earlier by a child": the calls in a tx are ordered DFS post-order, so children occupy
    /// the low indices and the endpoint is last — a child that refuses means the endpoint never
    /// executed at all, and its own checks were never reached. `OBL-C192`'s `HouseCloseV1` row was
    /// green that way for a whole run: the block was refused at `call_idx=0` by the child's
    /// `Duplicate commitment in output 0` and the endpoint's `exec` never appeared.
    RejectionByEndpoint(&'static [&'static str]),
    /// Expect a rejection **a child call in the frame** produced, with the endpoint never executed.
    ///
    /// Calls run DFS post-order, so children occupy the low indices and the endpoint is last. The
    /// runner derives the blame from the frame itself — the first generated child's resolved
    /// `ContractId` — and asserts the refusal text names it. A row with this expectation whose
    /// `generate` returns no child asserts nothing, so the branch refuses it loudly and `validate()`
    /// refuses an empty needle slice.
    RejectionByChild(&'static [&'static str]),
}

impl EndpointExpectation {
    /// All non-`Success` arms mean "expect a rejection"; the payload only narrows *which* one.
    fn is_rejection(&self) -> bool {
        !matches!(self, Self::Success)
    }

    /// The needles this row requires the run's own error text to contain, if any.
    fn needles(&self) -> &'static [&'static str] {
        match self {
            Self::Success => &[],
            Self::RejectionNaming(n) | Self::RejectionByEndpoint(n) | Self::RejectionByChild(n) => n,
        }
    }
}

/// Specification for a single contract endpoint.
pub struct EndpointSpec<'a> {
    /// Function name (matches function enum variant).
    pub name: &'static str,
    /// Whether this function requires a ZK proof.
    pub is_zk: bool,
    /// Produces call_data + proofs for this endpoint.
    pub generate: Box<dyn Fn() -> Result<EndpointResult> + 'a>,
    /// For FeeV3/BurnV1: uses prefetched coinbase params instead of `generate`.
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
    /// Optional deployment init ix (for WASM contracts whose init_contract reads
    /// cross-contract cids, e.g. stablecoin's promissory_note_contract_id). When
    /// set, the contract is deployed via deploy_with_ix instead of the empty ix.
    pub deploy_ix: Option<Vec<u8>>,
}

impl EndpointSpec<'_> {
    /// A rejection row must name the reason it is about. An empty needle slice is the bare "any
    /// failure will do" shape this class retired: it is satisfied by a missing child, a wrong
    /// selector or a failed child, so it constrains nothing. A `Success` row names nothing and is
    /// fine.
    fn validate_expectation(&self) -> Result<()> {
        if self.expectation.is_rejection() && self.expectation.needles().is_empty() {
            return Err(dwow_core::Error::Custom(format!(
                "rejection endpoint '{}' names no needle — a rejection row must name the reason it \
                 is about (a bare rejection is a control that cannot fail)",
                self.name
            )));
        }
        Ok(())
    }
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
        // A rejection row must name the check it is about — see `EndpointSpec::validate_expectation`.
        // This runs before the test body, so a row that names nothing fails in milliseconds rather
        // than after a proving run.
        for ep in &self.endpoints {
            ep.validate_expectation()?;
        }
        Ok(())
    }


    /// Index of the first ZK endpoint (for nullifier replay testing).
    /// Index of the first ZK endpoint the nullifier-replay control can be built on.
    ///
    /// **The `expectation` filter is not incidental.** The control asserts that a *second* submission
    /// is refused, and `nullifier_replay`'s own contract says "First submission must have already
    /// succeeded (caller's responsibility)" — a responsibility nothing checked. A row that is
    /// expected to be rejected anyway was never accepted, so its "replay" is refused for whatever
    /// refused the original, and the control proves nothing about replay at all. Four specs were in
    /// exactly that state and were selected by this function: `bridge`, `drain_protection`,
    /// `insurance_market`, `dao_escrow` (`OBL-C193`).
    pub fn first_zk_index(&self) -> Option<usize> {
        // Skip endpoints that need coinbase params — they can't be generated
        // standalone for nullifier replay (HAZOP H-UR-013).
        self.endpoints.iter().position(|e| {
            e.is_zk && e.generate_with_coinbase.is_none() && !e.expectation.is_rejection()
        })
    }
}

/// Install a tracing subscriber so each CONTRACT's own `msg!` output is visible.
///
/// Set `DWOW_TEST_LOGS=1` to enable. Off by default because INFO-level tracing
/// from the whole node is very loud and would swamp a normal run.
///
/// Why it matters: a contract's `msg!` reaches the host as
/// `info!(target: "runtime::vm_runtime", "[WASM] Contract log: {msg}")`
/// (`src/runtime/vm_runtime.rs`), and with no subscriber installed it goes
/// nowhere. Several heavyweight failures are a contract *rejecting* a call —
/// which, by `contract-standards.md` §3, it signals by returning empty metadata
/// — so the reason exists only in that log, and without this the failure reports
/// a decode error and no cause.
///
/// Idempotent: `set_global_default` fails after the first install, which is the
/// expected outcome for every test after the first in a process.
///
/// `pub(crate)` because the standalone pipeline tests need it too. Until
/// 2026-09-25 it was private to this runner, so `DWOW_TEST_LOGS=1` did nothing for
/// `tests::pipeline::*` — which is where `test_all_contracts_deploy` lives, the one
/// test that deploys all 32 contracts and reports a count. A batch run that names
/// the contracts it failed to deploy could not say *why* any of them failed, since
/// the contract's own `msg!` — the only channel that carries the reason — was
/// discarded.
pub(crate) fn init_contract_logging() {
    if std::env::var("DWOW_TEST_LOGS").is_err() {
        return;
    }
    let subscriber = tracing_subscriber::fmt::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

// ── Runner ─────────────────────────────────────────────────────────────────

/// The uniform test runner for spec-based heavyweight tests. Spec-based
/// tests (32 contracts) call this. Standalone tests (block execution,
/// relayer lifecycle, fee_v3, metadata) use direct HeavyweightPipeline.
/// Composes shared modules to structurally enforce heavyweight-spec.md §9.
pub async fn run_heavyweight_test(spec: &ContractTestSpec<'_>) -> Result<()> {
    spec.validate()?;
    init_contract_logging();

    // ── Pipeline A (primary) ────────────────────────────────────────
    let mut chain_a = modules::chain_setup::init_test_chain().await?;
    // `run_heavyweight_test` still returns `dwow_core::Result`, so the typed cause crosses via
    // the shared bridge; see the note in `fee_integration_spec`. This becomes `TestResult` when
    // the runner is converted.
    chain_a.log_file = Some(Mutex::new(
        crate::tests::test_output::create_log_file(spec.name)
            .map_err(modules::error_bridge::bridge)?,
    ));

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
        spec.deploy_ix.as_deref(),
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
        // A contract's circuits can bind the verifying block height into a public input, and a
        // generator runs before the block exists — so the runner publishes the height the call
        // will land at (the tip plus one) immediately before generating. See
        // `ContractHarness::set_next_block_height`.
        spec.harness.set_next_block_height(height_before.succ());
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
    for endpoint in &spec.endpoints {
        // Coinbase-dependent endpoints re-prefetch the coinbase at the CURRENT
        // height — each such endpoint mints a fresh commitment at its own block, so a
        // single pre-fetched coinbase (height N) would be reused at height N+1
        // and duplicate the commitment.
        let coinbase = if endpoint.generate_with_coinbase.is_some() {
            Some(modules::coinbase_coordination::prefetch_coinbase_params(&chain_a).await?)
        } else {
            None
        };

        // Use generate_with_coinbase if this endpoint needs coinbase params (FeeV3/BurnV1)
        spec.harness.set_next_block_height(height_before.succ());
        let result = if let Some(ref gen) = endpoint.generate_with_coinbase {
            gen(coinbase.as_ref().expect("needs_coinbase_coordination must be true when generate_with_coinbase is set"))?
        } else {
            (endpoint.generate)().map_err(|e| dwow_core::Error::Custom(
                format!("TEST-FAIL [{}::{}]: harness generate failed — {}", spec.name, endpoint.name, e)
            ))?
        };
        assert!(!result.call_data.is_empty(),
            "TEST-FAIL [{}::{}]: call_data must not be empty", spec.name, endpoint.name);

        if endpoint.expectation.is_rejection() {
            // Expect accept_block to REJECT this call (e.g., MintV1 FunctionDisabled).
            //
            // The endpoint's child calls ride with it. Without them, a call whose rejection comes
            // from a *later* check is turned away a step earlier for the missing child instead, and
            // the assertion then holds for a reason the endpoint does not name — a control that
            // cannot fail. For an endpoint with no children this is byte-identical to the
            // single-call submitter: `build_witness_tree` with an empty child list emits exactly
            // `build_witness`'s transaction, and `build_contract_tx_tree` likewise.
            //
            // The first child's id is bound *before* the submitter, because that call moves
            // `result.children` — the same idiom, and the same reason, as the replay control below.
            let first_child_cid = result.children.first().map(|c| c.contract_id);
            let submit_result = modules::block_submission::submit_multi_call_block(
                &chain_a, cid, spec.harness,
                &result.call_data, result.proofs, endpoint.is_zk, result.children,
            ).await;
            let err = submit_result.expect_err(&format!(
                "TEST-FAIL [{}::{}]: expected rejection but accept_block succeeded",
                spec.name, endpoint.name
            ));
            // When the row names the check it tests, require the run's own error text to say so.
            // This is what separates "rejected, for the reason under test" from "rejected, earlier".
            let text = format!("{err}");
            // Every per-call failure is formatted `… (contract <id>): …`, so the contract id is the
            // only attribution the text carries. Note `fn_code` in that same line is the call-tree
            // *length prefix*, not a function selector — never assert on it.
            let blamed = match endpoint.expectation {
                EndpointExpectation::RejectionByEndpoint(_) => Some(cid),
                EndpointExpectation::RejectionByChild(_) => match first_child_cid {
                    // A child-bearing frame is required for this attribution to mean anything:
                    // with no child there is nothing for "the child refused" to be, and the
                    // assertion below would pass vacuously.
                    Some(child) => Some(child),
                    None => panic!(
                        "TEST-FAIL [{}::{}]: RejectionByChild row generated no child — there is no \
                         child whose refusal could be asserted, so this row constrains nothing",
                        spec.name, endpoint.name
                    ),
                },
                _ => None,
            };
            if let Some(id) = blamed {
                let who = format!("(contract {id})");
                assert!(text.contains(&who),
                    "TEST-FAIL [{}::{}]: expected the rejection to come from {} — it was refused by \
                     some other call in the frame, so the endpoint's own checks were never reached. \
                     Full error: {}",
                    spec.name, endpoint.name, who, text);
            }
            for needle in endpoint.expectation.needles() {
                assert!(text.contains(needle),
                    "TEST-FAIL [{}::{}]: rejection did not name {:?} — it was rejected for \
                     some other reason. Full error: {}",
                    spec.name, endpoint.name, needle, text);
            }
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

    // ── Nullifier replay rejection (spec §3.6) ─────────────────────
    if let Some(idx) = spec.first_zk_index() {
        let endpoint = &spec.endpoints[idx];
        // Regenerate for the block the replay will actually land in, not the one the original
        // call landed in: a height-bound proof made for the old height would be rejected as an
        // invalid proof and the replay assertion would then hold for a reason it does not name.
        spec.harness.set_next_block_height(height_before.succ());
        let result = (endpoint.generate)()?;
        // The children execute before the endpoint (DFS post-order), so a replay is refused by the
        // first child when there is one and by the endpoint otherwise. State which, rather than
        // accepting any refusal at all: accepting any refusal is what made this control vacuous for
        // the twelve specs whose first ZK endpoint requires a child (`OBL-C193`).
        let blamed = result.children.first().map(|c| c.contract_id).unwrap_or(cid);
        modules::nullifier_replay::verify_nullifier_replay(
            &chain_a, cid, spec.harness,
            &result.call_data, result.proofs, endpoint.is_zk, result.children, blamed,
        ).await?;
    } else if spec.endpoints.iter().any(|e| e.is_zk) {
        // Loud, not silent: §3.6 requires this control for every contract with a ZK-gated function,
        // and a spec that cannot host it has an *unverified* replay rejection rather than a passing
        // one. The alternative — picking a row that is expected to be rejected anyway — is the
        // vacuity `OBL-C193` records.
        eprintln!(
            "WARN [{}]: §3.6 nullifier-replay control SKIPPED — no ZK endpoint is both \
             standalone-generatable and expected to succeed, so there is nothing whose replay could \
             be refused. This contract's replay rejection is UNVERIFIED (OBL-C193).",
            spec.name
        );
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
        spec.deploy_ix.as_deref(),
    ).await?;

    // Replay cross-contract setup on chain B (determinism)
    if let Some(ref setup_fn) = spec.setup {
        setup_fn(&chain_b)?;
    }

    // Replay init on chain B
    if let Some(ref init_fn) = spec.initialize {
        spec.harness.set_next_block_height(chain_b.height().succ());
        let result = init_fn()?;
        let _ = modules::block_submission::submit_single_call_block(
            &chain_b, cid_b, spec.harness,
            &result.call_data, result.proofs, false,
        ).await?;
    }

    // Replay all endpoints on chain B
    let mut h_b = chain_b.height();
    for endpoint in &spec.endpoints {
        if endpoint.generate_with_coinbase.is_some() {
            // Coinbase-coordinated endpoint: re-prefetch at chain B's CURRENT
            // height (mirrors chain A) so the coinbase coin + on-chain tree
            // state match chain A exactly. Without this, the deterministic
            // replay would skip FeeV3/BurnV1/TransferV1/SpendV1 and PI-7 would
            // compare hashes of blocks with different transaction sets.
            let cb = modules::coinbase_coordination::prefetch_coinbase_params(&chain_b).await?;
            let result = endpoint.generate_with_coinbase.as_ref()
                .expect("generate_with_coinbase must be Some")
                (&cb)?;
            assert!(!result.call_data.is_empty(),
                "TEST-FAIL [{}::{}]: call_data must not be empty (chain B)",
                spec.name, endpoint.name);
            let new_h = modules::coinbase_coordination::submit_with_coinbase(
                &chain_b, cid_b, spec.harness,
                &result.call_data, result.proofs, endpoint.is_zk,
                cb.coinbase_tx.clone(),
            ).await?;
            assert!(new_h > h_b,
                "TEST-FAIL [{}::{}]: height must advance after accept_block (chain B)",
                spec.name, endpoint.name);
            h_b = new_h;
        } else {
            spec.harness.set_next_block_height(h_b.succ());
            let result = (endpoint.generate)()?;
            if endpoint.expectation.is_rejection() {
                let _ = modules::block_submission::submit_multi_call_block(
                    &chain_b, cid_b, spec.harness,
                    &result.call_data, result.proofs, endpoint.is_zk, result.children,
                ).await;
            } else {
                h_b = modules::endpoint_exercise::exercise_endpoint(
                    &chain_b, cid_b, spec.harness, endpoint, h_b,
                ).await?;
            }
        }
    }

    // Compare final block hashes (PI-7)
    //
    // A mismatch is localised before it is reported. Two opaque final hashes are
    // why diagnosing `OBL-C206` cost three ~10-minute runs: the sentence named
    // neither a height nor a value. Heights are compared first, because a height
    // mismatch and a value mismatch otherwise print identically.
    let (height_a, height_b) = (chain_a.height(), chain_b.height());
    let hash_a = chain_a.block_hash_at(height_a)?;
    let hash_b = chain_b.block_hash_at(height_b)?;
    if hash_a != hash_b || height_a != height_b {
        // Only here, and only once: the bisect re-hashes blocks and the RandomX
        // VM cache holds six entries keyed per height, so it is not a per-run cost.
        let report = pi7_divergence_report(&chain_a, &chain_b, spec.name);
        assert_eq!(height_a, height_b,
            "INFRA-FAIL [determinism]: PI-7 chain heights must match for {}\n{}",
            spec.name, report);
        assert_eq!(hash_a, hash_b,
            "INFRA-FAIL [determinism]: PI-7 block hashes must match for {}\n{}",
            spec.name, report);
    }

    Ok(())
}

/// The offset of the first element at which two runs' per-height block hashes
/// disagree, or `None` when they agree over the whole common prefix.
///
/// Pure, and free of chain I/O on purpose: it makes the diagnostic's own
/// negative control a millisecond `#[test]` (`pi7_diagnostic::first_divergence_*`)
/// instead of a ten-minute run, which is the reason `OBL-C206`'s instrument is
/// shaped this way rather than inline in the assertion.
fn first_divergence(a: &[Option<blake3::Hash>], b: &[Option<blake3::Hash>]) -> Option<usize> {
    let common = a.len().min(b.len());
    for i in 0..common {
        if a[i] != b[i] {
            return Some(i);
        }
    }
    if a.len() == b.len() {
        None
    } else {
        Some(common)
    }
}

/// Localise a `PI-7` mismatch to a height, a transaction and a byte, and write
/// both sides of any differing contract call to `/tmp`.
///
/// The payload is deliberately narrow: `Transaction::hash()` never commits to
/// `witness` (`src/linear/src/transaction.rs`), so ZK proof bytes cannot move a
/// block hash. What can is a value in a contract call's `data` — which is what
/// this prints, to the byte.
fn pi7_divergence_report(
    a: &HeavyweightPipeline,
    b: &HeavyweightPipeline,
    spec_name: &str,
) -> String {
    let (top_a, top_b) = (a.height(), b.height());
    let mut out = format!(
        "  PI-7 divergence for {spec_name}: chain_a height {top_a}, chain_b height {top_b}"
    );
    if top_a != top_b {
        // The two runs built different numbers of blocks. That is its own
        // finding — not a value divergence — and there is no common tip to diff.
        out.push_str("\n  (heights differ: the runs built different block counts)");
        return out;
    }

    // Block hashes are a prefix property — each header commits to `previous` —
    // so the first differing height is binary-searchable.
    let tip_differs = a.block_hash_at(top_a).ok().flatten() != b.block_hash_at(top_b).ok().flatten();
    if !tip_differs {
        out.push_str(
            "\n  (final hashes differ but the tip's block hashes agree — the mismatch is not a stored block)",
        );
        return out;
    }
    let mut lo = BlockHeight::new(2);
    let mut hi = top_a;
    while lo < hi {
        let mid = BlockHeight::new((lo.get() + hi.get()) / 2);
        let agree = a.block_hash_at(mid).ok().flatten() == b.block_hash_at(mid).ok().flatten();
        if agree {
            lo = BlockHeight::new(mid.get() + 1);
        } else {
            hi = mid;
        }
    }
    let h = hi;
    out.push_str(&format!("\n  first differing height: {h}"));

    let (blk_a, blk_b) = match (a.chain_state.store.get_block(h), b.chain_state.store.get_block(h)) {
        (Ok(x), Ok(y)) => (x, y),
        _ => {
            out.push_str("\n  (could not read both blocks at that height)");
            return out;
        }
    };
    out.push_str(&format!(
        "\n  transactions: chain_a {}, chain_b {}",
        blk_a.transactions.len(),
        blk_b.transactions.len()
    ));
    let common = blk_a.transactions.len().min(blk_b.transactions.len());
    for i in 0..common {
        let (ta, tb) = (&blk_a.transactions[i], &blk_b.transactions[i]);
        if ta.hash() == tb.hash() {
            continue;
        }
        out.push_str(&format!("\n  first differing transaction: index {i}"));
        let (ca, cb) = (&ta.contract_calls, &tb.contract_calls);
        out.push_str(&format!("\n    calls: chain_a {}, chain_b {}", ca.len(), cb.len()));
        for j in 0..ca.len().min(cb.len()) {
            if ca[j].data == cb[j].data {
                continue;
            }
            let off = ca[j]
                .data
                .iter()
                .zip(cb[j].data.iter())
                .position(|(x, y)| x != y)
                .unwrap_or(ca[j].data.len().min(cb[j].data.len()));
            out.push_str(&format!(
                "\n    call[{j}] contract {} differs at byte {off} (len {} vs {})",
                ca[j].contract_id,
                ca[j].data.len(),
                cb[j].data.len()
            ));
            let pa = format!("/tmp/pi7_{spec_name}_h{h}_tx{i}_call{j}.a");
            let pb = format!("/tmp/pi7_{spec_name}_h{h}_tx{i}_call{j}.b");
            let wrote = std::fs::write(&pa, &ca[j].data).is_ok() && std::fs::write(&pb, &cb[j].data).is_ok();
            out.push_str(&format!(
                "\n    {}",
                if wrote {
                    format!("wrote {pa} and {pb}")
                } else {
                    format!("could not write {pa} / {pb}")
                }
            ));
            break;
        }
        break;
    }
    out
}

/// Negative control for `OBL-C206`'s diagnostic. The instrument exists to turn an
/// unreadable mismatch into a height, a transaction and a byte; this shows it
/// finds the divergence, and finds nothing when there is none — in milliseconds,
/// which is the whole reason the comparator is pure and separate from the runner.
#[cfg(test)]
mod pi7_diagnostic {
    use super::first_divergence;
    use blake3::Hash;

    fn h(b: u8) -> Option<Hash> {
        Some(Hash::from([b; 32]))
    }

    #[test]
    fn first_divergence_reports_the_first_mismatch() {
        // Agreement, over zero, one and several heights.
        assert_eq!(first_divergence(&[], &[]), None);
        assert_eq!(first_divergence(&[h(1)], &[h(1)]), None);
        assert_eq!(first_divergence(&[h(1), h(2), h(3)], &[h(1), h(2), h(3)]), None);
        // A divergence is reported at its FIRST offset even when later heights
        // agree again — the property the bisect in `pi7_divergence_report` relies on.
        assert_eq!(first_divergence(&[h(1), h(2), h(3)], &[h(1), h(9), h(3)]), Some(1));
        assert_eq!(first_divergence(&[h(9), h(2)], &[h(1), h(2)]), Some(0));
        // Unequal lengths diverge at the shorter run's end.
        assert_eq!(first_divergence(&[h(1)], &[h(1), h(2)]), Some(1));
        assert_eq!(first_divergence(&[h(1), h(2)], &[h(1)]), Some(1));
    }
}

#[cfg(test)]
mod expectation_rules {
    use super::*;

    fn spec_with(expectation: EndpointExpectation) -> EndpointSpec<'static> {
        EndpointSpec {
            name: "row",
            is_zk: false,
            expectation,
            generate_with_coinbase: None,
            verify_state: None,
            generate: Box::new(|| {
                Ok(EndpointResult { call_data: vec![], proofs: vec![], children: vec![] })
            }),
        }
    }

    /// R8 control for D3: the empty-needle rule is a check that can fail. A rejection row that names
    /// nothing — the bare "any failure will do" shape this class retired — is refused before the
    /// test body; one that names its reason, and a `Success` row, pass.
    #[test]
    fn a_rejection_row_must_name_its_reason() {
        for empty in [
            EndpointExpectation::RejectionNaming(&[]),
            EndpointExpectation::RejectionByEndpoint(&[]),
            EndpointExpectation::RejectionByChild(&[]),
        ] {
            assert!(
                spec_with(empty).validate_expectation().is_err(),
                "an empty-needle rejection row must be refused"
            );
        }
        assert!(spec_with(EndpointExpectation::RejectionByEndpoint(&["ContractError(Custom(3))"]))
            .validate_expectation()
            .is_ok());
        assert!(spec_with(EndpointExpectation::Success).validate_expectation().is_ok());
    }
}
