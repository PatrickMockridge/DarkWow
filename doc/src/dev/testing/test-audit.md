# Test Suite Audit

This document is a **static audit** (read + map, no test runs) of every test suite
the testing documentation claims exists, against what is actually present in the
tree. It is the evidence base for the [L3 Readiness Specification](l3-readiness-spec.md).

Audit date: 2026-08-16. Branch: `linear-master`. All file paths are repo-root-relative.
Findings are classified `BLOCKING` (must be remediated before the L3 pipeline is
declared ready to run) or `NON-BLOCKING` (recorded; remediation deferred).

**Re-verified 2026-08-17 (L1.5/L2 execution run):** G2 (L1.5 bridge) 5/5 passed; G3
(L2 heavyweight, 9 genesis) 9/9 passed; G5 (zk_audit + encode_roundtrip) 5/5 passed;
fee suite 18/18 passed. F-11 is closed. No BLOCKING findings remain: F-5 was already
remediated in commit `7a391757bc` (verified in-tree), F-11 closed 2026-08-17.

## 1. Ground-Truth Counts

Verified against the working tree (not documentation claims):

| Item | Count | Notes |
|------|-------|-------|
| Contract crates (`src/contract/*/Cargo.toml`) | 34 | 33 contracts + `test-harness` |
| Deployable contracts | 32 | `entropy` is a library, not a contract (see F-10) |
| `tests/integration.rs` | 31 | `native_token` uses `unit.rs`; `entropy` has no `tests/` dir |
| `tests/zk_circuit_test.sh` | 31 | `deployooor` (no ZK) and `entropy` (library) excluded |
| Harness modules (`test-harness/src/harness/*.rs`) | 32 | one per deployable contract |
| Contract spec files (`bin/dwowd/src/tests/specs/*_spec.rs`) | 33 | 32 contract specs + `fee_integration_spec.rs` |
| Contract `.zk.bin` files (`src/contract/*/proof/`) | 175 | docs claim "99 harness-loaded" |
| `sim/contracts/*.py` modules | 8 | `gaming.py` models 7 gambling contracts |
| `contrib/model/*.py` files | 21 | incl. `chain_model.py` and `chain_validation_model.py` |
| Pipeline `lib/*.sh` modules | 18 | matches `level-3-localnet.md` |
| `#[test]` fns in `heavyweight_pipeline.rs` | 59 | `heavyweight.sh --all` selects 43 |

### 1.1 Genesis contracts (L3-blocking scope)

The nine genesis contracts are the only contracts whose test status blocks L3 fitness:
**Deployooor, NativeToken, PromissoryNote, Identity, Oracle, Attestation, Purse, Box,
MultiSig** (`src/linear/src/execution.rs:832`). Non-genesis contracts (gambling, dex,
stablecoin, auction, escrow, bridge, …) are deployed post-genesis via Deployooor and do
not block L3. Findings below are classified BLOCKING only if they affect a genesis
contract or the pipeline itself.

## 2. Conformance Matrix

Each row maps a documentation claim to its actual implementation. Status:
**OK** (present and matches), **DRIFT** (present but docs' count/claim is stale),
**GAP** (absent or non-conforming).

### 2.1 Level 1 — Lightweight

| Doc claim (source) | Actual | Status | Finding |
|--------------------|--------|--------|---------|
| "all 32 contracts have `integration.rs`" (`overview.md:481`) | 31 `integration.rs`; `native_token` has `unit.rs`; `entropy` none | DRIFT | F-1 |
| "all 22 ZK-enabled contracts" have `zk_circuit_test.sh` (`overview.md:482`) | 31 `zk_circuit_test.sh` files | DRIFT | F-2 |
| `cargo test -p dwowd test_pipeline` (Deployooor) | `bin/dwowd/src/tests/pipeline.rs` | OK | — |
| `test-harness/tests/zk_audit.rs` decodes "99 harness-loaded `.zk.bin`" (`overview.md:260`) | `zk_audit.rs` runs `verify_zk_coverage()` over 32 harnesses; 175 `.zk.bin` in tree | DRIFT | F-3 |
| `test-harness/tests/encode_roundtrip.rs` | present | OK | — |
| Root crate tests `tests/*` | 8 files (`dyn_circuit`, `halo2_vk_ser`, `jsonrpc`, `network_transports`, `smt`, `socks5`, `vdf_eval`, `zkvm_opcodes`) | OK | — |
| `bin/dwowd/tests/*` | `consensus_coordination.rs`, `calibration_session_filter.rs` | OK | — |
| `bin/dww/tests/contract_metadata_tests.rs` | present | OK | — |

**Harness tier guard:** `zk_audit.rs` asserts every harness has a non-empty
`circuits()` (line 17), which enforces "no TIER C harness" (a circuit loader with zero
proof methods). TIER B detection (a declared circuit without a convenience proof
method) is not statically checked and requires a per-harness review; that review is
deferred.

### 2.2 Python Layers

| Doc claim (source) | Actual | Status | Finding |
|--------------------|--------|--------|---------|
| Consensus models "34/34 tests, 8 VM scenarios" (`overview.md:22`) | `contrib/model/chain_validation_model.py`, `vm_state_model.py`, `merge_mining_model.py` present | OK | — |
| "All 27 contracts are modeled" (`overview.md:165`) | 8 `sim/contracts/*.py` modules (`gaming.py` models 7 gambling contracts) | DRIFT | F-4 |
| Fee model = executable spec (70 tests) | `contrib/model/fee_window_model.py`, `fee_model.py` present | OK | — |
| Wallet model | `contrib/model/wallet_model.py`, `wallet_simulation.py` present | OK | — |
| `pipeline_model.py` + `supply_chain_model.py` run by `run-all-tests.sh` | both present | OK | — |

### 2.3 Level 1.5 — Pre-Production Bridge (MoC gate)

All four named tests are present and **none are `#[ignore]`**:

| Test | Location |
|------|----------|
| `test_wallet_coinbase_scan_only` | `bin/dwowd/src/tests/wallet_integration.rs:1239` |
| `test_canonical_call_failure_rejects_block` | `bin/dwowd/src/tests/wallet_integration.rs:2002` |
| `test_merge_mined_block_acceptance` | `bin/dwowd/src/tests/merge_mining.rs:160` |
| `test_merge_mined_block_deterministic` | `bin/dwowd/src/tests/merge_mining.rs:301` |

### 2.4 Level 2 — Heavyweight

| Doc claim (source) | Actual | Status | Finding |
|--------------------|--------|--------|---------|
| "all 32 contracts with exhaustive function coverage" (`overview.md:24`) | 32 contract specs + `run_heavyweight_test` (`uniform_runner.rs`) | OK | — |
| `heavyweight.sh --all` runs 43 tests | 32 contract + 8 block-execution + metadata/fee/recruitment/relayer | OK | — |
| `fee_collect_pipeline.rs` | present | OK | — |
| `#[ignore]` requires tracking issue (§2.6) | `H-TF-002` (uncle-merkle) + `H-TF-003` (harness-exercise) carry IDs | OK | — |

### 2.5 Level 3 — Containerized Localnet

| Doc claim (source) | Actual | Status | Finding |
|--------------------|--------|--------|---------|
| 18 `lib/*.sh` modules (`level-3-localnet.md`) | 18 present | OK | — |
| `pipeline_spec.py` = source of truth | present | OK | — |
| "Every check reports PASS or FAIL — no skipped or silent checks" (README) | Container-presence gate now `fail` (F-5 resolved); `phase_08_mining.sh` merge-mode + join-lifecycle `warn` remain (non-gating, spec-sanctioned) | OK | F-5 (resolved) |
| Success = wallet scan + decrypt + DRKW balance | `phase_10_wallet_tests.sh` is a GATE | OK | — |
| Full spend cycle (build→broadcast→mine→confirm) | documented as a known gap (Pattern C, `level-3-localnet.md:447`) | GAP | F-6 |

### 2.6 Level 4 — Devnet

`contrib/docker/darkwow-devnet/` present (relaxed-parameter variant). No drift found.

### 2.7 Wallet + Fee surfaces

| Doc claim | Actual | Status | Finding |
|-----------|--------|--------|---------|
| Wallet L1/L2/L3 | `bin/dww/test_capability_lightweight.sh`, `dwow_wallet --lib capability::tests`, `test-wallet.sh` | OK | — |
| Fee invariant matrix (`fee-testing.md:49`) | FI-GEN 0, FI-RISK 0, FI-WASM 1 (stub), FI-TIME 0 | GAP | F-7 |

## 3. Findings

### BLOCKING

**F-5 — RESOLVED (2026-08-16).** A missing expected container was reported as `warn`,
letting the pipeline proceed to a 20-min synchronization poll before surfacing the real
cause. Fixed in commit `7a391757bc` ("tighten phase-6 container gate"): `phase_06_verify.sh:45`
now reports `fail "$c not running"`, and `phase_gate` (the `verify_or_lifecycle` gate at
`test_pipeline.sh:189`) stops the pipeline on any new `FAIL`. Verified against the current
tree: `phase_06_verify.sh:45` is `fail`; `fail()` increments the global `FAIL` counter;
`phase_gate()` exits 1 when `FAIL` increases. `phase_08_mining.sh` merge-mode checks remain
`warn` as legitimate pre-readiness diagnostics (real gate is `phase_09_blocks.sh`).

**F-11 — RESOLVED (2026-08-17).** The six failing genesis contracts (`native_token`
`Custom(14)`, `identity` `Custom(29)`, `purse` `Custom(1)`, `oracle`/`attestation`
`CallerAccessDenied`, `multisig` `TEST-FAIL FinalizeV1_sufficient`) were swept in commits
`417590ca8f`..`0c747f30ce` (coin-transfer full-recipient + burn-sig + DZ-4; identity
bootstrap-lock removal + consolidation; purse balance-tree + DZ-4; oracle/attestation
exec/apply + DZ-4; multisig tombstone + DZ-4; nullifier claim-vs-spend tracking).
Re-verified 2026-08-17 via `heavyweight.sh` with the nine genesis flags — **9/9 passed**
(`test result: ok. 9 passed; 0 failed`). G3 is green; no BLOCKING finding remains against
G2/G3/G5.

### NON-BLOCKING

**F-1 — "all 32 contracts have `integration.rs`" is stale.** `native_token` has
`unit.rs` + `circuit_instance_counts.rs` (no `integration.rs`); `entropy` is a library
with no `tests/` dir. `overview.md:481` overstates.

**F-2 — "22 ZK-enabled contracts" is stale.** 31 contracts have `zk_circuit_test.sh`.
`overview.md:482` understates.

**F-3 — "99 harness-loaded `.zk.bin`" is stale.** The tree holds 175 contract
`.zk.bin` files. `overview.md:260` understates. (Not security-relevant; the audit
decodes whatever the harnesses load.)

**F-4 — "27 contracts are modeled" needs reconciliation.** `sim/contracts/` is 8 modules
(`gaming.py` models 7 gambling contracts, `infrastructure.py` models bridge + others).
The "27" figure is a logical count, not a file count; it should be stated as such or
recomputed.

**F-6 — Full spend/broadcast cycle untested.** Already documented as Pattern C
(`level-3-localnet.md:447`). `broadcast_tx` is wired but unconfirmed in CI. Partition-C;
deferred, but the spec records it as a known gap.

**F-7 — Fee invariant matrix has 0-test rows.** FI-GEN, FI-RISK (Rust), FI-TIME are
untested; FI-WASM is a stub. `fee-testing.md` itself documents these. L3 fee-window
tests (`L3-FW-1/2/3`) cover the multi-node window boundary, so these L1/L2 gaps do not
block L3; they are recorded for later remediation.

**F-9 — Stale §2.6 "current violation".** `production-test-standard.md:292` states
`test_wallet_integration` is `#[ignore]`. It is now active (`wallet_integration.rs:71`).
See remediation in §4.

**F-10 — `entropy` is a library, correctly exempt.** `src/contract/entropy/` is
`crate-type = ["rlib"]` with only `Cargo.toml` + `src/lib.rs` — no `manifest.toml`,
no `entrypoint.rs`, no `proof/`, no WASM target. It is not a deployable contract; it
is a shared `derive_seed` library consumed by the gambling contracts. Its 7 in-source
`#[cfg(test)]` unit tests cover `derive_seed` (determinism, ordering, known-vector).
This is the correct coverage for a partition-A/B library; it SHALL NOT be given a
harness/spec, and the "32 contracts" framing correctly excludes it.

**F-8 — Gambling sweep committed (non-genesis).** `game_room` (plus `betting_stake`,
`darktoshi_dice`, `lottery`) were swept and committed (`eb6563bd6e`, `2c31a83822`):
entrypoints, clients, harnesses, specs, and proofs (new `game_room/src/client/*`,
`game_room/src/entrypoint/create_pot.rs`, `game_room/proof/create_pot.zk`). These
contracts are **non-genesis** — deployed post-genesis via Deployooor — so this work
does NOT block L3 docker-pipeline fitness (§1.1). It must still be verified green through
`accept_block` before Level 4 / mainnet.
**Remediation:** separate effort, deferred for L3.

**F-12 — `zk_audit` "under a second (no proving key building)" claim is wrong.**
`overview.md:260` and `level-1-lightweight.md:30` state the ZK-coverage audit decodes the
harness `.zk.bin` files "in under a second (no proving key building)". `zk_audit.rs`'s
`zk_check!` macro calls `<$harness>::spawn()` for all 32 harnesses, and `spawn()` builds
ProvingKeys — the test is ZK-setup-bound, not decode-bound. Measured 2026-08-17:
`finished in 4963.29s` (~82 min). Non-blocking (the test passes 1/1); the runtime claim
SHALL be corrected in both docs.

## 4. Remediation Tracking

| Finding | Action | Where | Status |
|---------|--------|-------|--------|
| F-1 | Correct `overview.md:481` | doc | this pass |
| F-2 | Correct `overview.md:482` | doc | this pass |
| F-3 | Correct `overview.md:260` | doc | this pass |
| F-5 | `warn`→`fail` for missing container | `phase_06_verify.sh:45` | **closed (7a391757bc)** |
| F-11 | Sweep 6 failing genesis contracts (native_token, identity, purse, oracle, attestation, multisig) | contract entrypoints/clients | **closed (2026-08-17, 9/9 green)** |
| F-9 | Reconcile `production-test-standard.md` §2.6 | doc | this pass |
| F-10 | Document `entropy` as an exempt library | doc | this pass |
| F-8 | Verify gambling sweep green through accept_block (non-genesis; L4/mainnet only) | contract/spec/harness | deferred |
| F-12 | Correct `zk_audit` runtime claim in `overview.md` + `level-1-lightweight.md` | doc | this pass |
| F-4, F-6, F-7 | Reconcile / implement later | docs + code | deferred |

## 5. References

- [Testing Overview](overview.md)
- [Production Test Standard](production-test-standard.md)
- [Level 2 Heavyweight Spec](heavyweight-spec.md)
- [Level 3 Localnet](level-3-localnet.md)
- [L3 Readiness Specification](l3-readiness-spec.md)
