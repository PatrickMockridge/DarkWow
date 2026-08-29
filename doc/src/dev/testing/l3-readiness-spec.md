# Level 3 Readiness Specification

This document is the **normative gate** for the Level 3 (L3) Docker test pipeline
(`contrib/docker/darkwow-testnet/test_pipeline.sh`). It defines, in RFC 2119 terms,
the preconditions that MUST hold before a run may be initiated (readiness), and the
criteria a run MUST satisfy to be considered a success (acceptance).

It complements, and does not replace, the [Production Test Standard](production-test-standard.md)
(all tests) and the [Level 2 Heavyweight Spec](heavyweight-spec.md) (Level 2). This
document is the sole authority on *when L3 may run* and *what a passing L3 run proves*.

The key words "MUST", "MUST NOT", "SHALL", "SHALL NOT", "SHOULD", and "MAY" in this
document are to be interpreted as described in RFC 2119.

## 1. Scope

L3 is the containerized localnet: a multi-node Docker testnet with real RandomX PoW,
real P2P over TLS, real build-from-source, real 120-second block times, and (in merge
mode) real xmrig + p2pool + monerod sidecars. It is the final gate before Level 4
(public devnet) and mainnet. It is slow by design and SHALL NOT be shortcut.

The pipeline SHALL be driven exclusively through `test_pipeline.sh` (never raw
`docker compose` or ad-hoc `docker` commands) — see `level-3-localnet.md`.

### 1.1 Genesis-contract scope

L3 fitness — the ability of the Docker pipeline to run and pass — is determined
**solely by the nine genesis contracts**. These are materialized in the genesis block
by `apply_genesis_deployments` (`src/linear/src/execution.rs:832`), in canonical
bootstrap order:

1. Deployooor
2. NativeToken
3. PromissoryNote
4. Identity
5. Oracle
6. Attestation
7. Purse
8. Box
9. MultiSig

Non-genesis contracts (gambling, dex, stablecoin, auction, escrow, bridge, and the
rest) are deployed **after** genesis via Deployooor. Their test/sweep status does NOT
block L3 fitness; it gates Level 4 and mainnet. The gates G1–G8 SHALL be read as
applying to the genesis contracts unless a gate explicitly names a broader surface.

## 2. Readiness Preconditions (Gates)

An L3 run SHALL NOT be initiated unless **all** of G1–G8 hold. Each gate names the
checkable criterion and the evidence that MUST be recorded.

### G1 — Level 1 green (genesis contracts)

Criterion: the nine genesis contracts' Level 1 suites pass — their `integration.rs`
/ unit tests, `zk_circuit_test.sh`, and the shared harness audit
`test-harness/tests/{zk_audit,encode_roundtrip}.rs`. The broader `cargo test
--workspace` (all 31 contract suites) is a hygiene check, not an L3 gate.

Evidence: a green run, with full output captured to a log file (no truncation).

### G2 — Level 1.5 MoC bridge green

Criterion: all four named bridge tests pass, none `#[ignore]`:

- `test_wallet_coinbase_scan_only`
- `test_canonical_call_failure_rejects_block`
- `test_merge_mined_block_acceptance`
- `test_merge_mined_block_deterministic`

Command:
`RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 cargo test --release -p dwowd --lib -- <the four names> --nocapture`

Evidence: green run, log captured. This is the last deterministic single-process
checkpoint before real networking and real PoW.

### G3 — Level 2 heavyweight green (genesis contracts)

Criterion: the nine genesis contracts' heavyweight tests pass through `accept_block`
(state transition verified, real ZK proofs, determinism PI-7, nullifier-replay
rejection), per `heavyweight-spec.md`. Non-genesis heavyweight tests are outside the
L3 gate (§1.1). Run `bin/dwowd/src/tests/heavyweight.sh` with the nine genesis
contract flags (`--deployooor --native-token --promissory-note --identity --oracle
--attestation --purse --box --multisig`).

Evidence: green run, per-test log files present.

### G4 — Python model is the specification

Criterion: all consensus/fee/wallet model tests pass **and** a line-by-line audit has
confirmed the Rust code matches the model 1:1. The pipeline is step 7 of 7, not a
substitute for the audit. A passing model alone is not sufficient.

Evidence: model test output + a record of the completed line-by-line audit.

### G5 — ZK-coverage audit green

Criterion: `test-harness/tests/zk_audit.rs` passes — every harness has a non-empty
`circuits()`, and every declared circuit has a `.zk.bin` and proving key.

Evidence: green run.

### G6 — Deterministic build from a committed + pushed commit

Criterion: the commit under test (`BUILD_COMMIT`, default `git rev-parse HEAD`) MUST
exist on `origin/linear-master`. The pipeline builds by cloning from origin and
`git reset --hard "$BUILD_COMMIT"`. A commit not pushed will fail the determinism gate.

Evidence: the commit SHA and the push confirmation.

### G7 — Clean tree

Criterion: no uncommitted build-affecting changes. The pipeline builds from origin,
so local uncommitted changes are invisible to it; running against a dirty tree silently
tests different code than what is committed. Work MUST be committed and pushed first.

Evidence: `git status` clean for all files under `src/`, `bin/`, and `contrib/`.

### G8 — No §2 anti-pattern violations

Criterion: no test exhibits a `production-test-standard.md` §2 anti-pattern
(coinbase-only accept_block for contract-function tests, proof-only verification,
synthetic manifests for production-path tests, silent `#[ignore]` without tracking
issue, Schnorr-signature bypass for ZK contracts).

Evidence: the [Test Suite Audit](test-audit.md) records no open `BLOCKING` finding.

## 3. L3 Run Acceptance Criteria

A run MAY proceed once G1–G8 hold. The run MUST then satisfy AC-1 through AC-4.

### AC-1 — Primary success criterion (wallet scan + decrypt + DRKW balance)

The pipeline SHALL demonstrate that the wallet scans blocks, decrypts coinbase output,
and reports a non-zero DRKW balance. This is the floor; it is not "blocks synced",
"peers connected", or "pipeline completed". Without it the run is a failure regardless
of any other PASS count. Implemented as a GATE in `phase_10_wallet_tests.sh`.

### AC-2 — All phase gates pass

The pipeline SHALL complete all phases for the selected mode with **0 FAIL** and the
phase gates (`phase_gate`) SHALL NOT stop the pipeline. Specifically:

- **Container presence gate** (`phase_06_verify.sh`): every expected container is
  running; a missing container is a `FAIL`, not a warning.
- **Synchronization gate** (`phase_09_blocks.sh`): node0 reaches height ≥ 2 within the
  poll budget.
- **Genesis-authority gate** (`phase_09_blocks.sh`): block-1 hash is identical on all
  nodes — only node0 creates; all others sync.

Per-mode PASS counts are defined and maintained in the
[`darkwow-testnet` README](../../../contrib/docker/darkwow-testnet/README.md); this
spec SHALL NOT hardcode them (they drift with the pipeline). The normative requirement
is structural: every check reports PASS or FAIL (a third `WARN` outcome is permitted
only for explicitly non-gating diagnostics), and the run exits 0 only if all gates pass.

### AC-3 — Zero container deaths

The pipeline's `docker events` monitor SHALL capture zero unexpected container deaths
(SIGSEGV, OOM-kill, exit). Any death is a failure and SHALL be investigated, not
retried away.

### AC-4 — Determinism

Block production SHALL be deterministic across the multi-node topology: the
genesis-authority check (block-1 hash equality) is the minimum. Contract
determinism is already enforced at Level 2 (PI-7); L3 SHALL NOT regress it.

### AC-5 — Capability-tests phase delivery (when `--capability-tests`)

When `--capability-tests` is passed, phase 98 SHALL be sound: the image's
`/app/dwowd_lib_tests` MUST be the libtest harness built by `cargo test --lib --no-run`
(NOT the clap-based `dwowd` daemon), and the phase MUST actually run
`test_box_put_wallet_driven_generic_prover` + `test_box_take_wallet_driven_generic_prover`,
so a phase failure reflects a real test failure rather than a binary-selection error. The
phase SHALL smoke-check the binary with `--list` before running (`phase_98_capability_tests.sh`),
and the Docker build SHALL select the harness path deterministically (`Dockerfile`).

## 4. Evidence Requirements

- Failures SHALL be recorded verbatim from run output — never reconstructed from
  memory.
- Full logs SHALL be written to a file (e.g. `/tmp/`), never truncated or filtered.
- No custom timeout SHALL be imposed on `cargo test` or `test_pipeline.sh`; the
  pipeline's own poll budgets govern.
- Only one pipeline run at a time SHALL be active (single-instance lock); confirm zero
  `dwow-*` containers before starting.

## 5. Known Gaps (non-blocking)

The following are recorded gaps that do not block L3 but SHALL be tracked:

- **Pattern C** — full spend cycle (build tx → P2P broadcast → mine → confirm) is not
  confirmed in CI (`level-3-localnet.md:447`).
- **Pattern D** — `p2p.seed().await` returns `()`, so a misconfigured seed reports
  "connected" with zero peers (`level-3-localnet.md:465`).
- **Fee matrix** — FI-GEN / FI-RISK (Rust) / FI-TIME / FI-WASM have zero or stubbed
  coverage (`fee-testing.md`).

These SHALL NOT be silently closed; remediation is tracked in
[Test Suite Audit](test-audit.md).

## 6. References

- [Testing Overview](overview.md)
- [Production Test Standard](production-test-standard.md)
- [Level 2 Heavyweight Spec](heavyweight-spec.md)
- [Level 3 Localnet](level-3-localnet.md)
- [Test Suite Audit](test-audit.md)
