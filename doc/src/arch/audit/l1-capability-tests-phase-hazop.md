# L1 Capability Write-Path — Capability-Tests Phase HAZOP

Guide-word deviation analysis over the delivery trace (`l1-capability-tests-phase-trace.md`). Guide words:
NO / NOT / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE, plus MORE / LESS (`sync-hazop.md:26-27`).
Each finding cites `file:line` on `linear-master`.

## Central root cause

**The test-delivery seam (`select!`) is non-deterministic and unverified.** `Dockerfile:161-162` selects the
`dwowd_lib_tests` binary with a loose prefix match `find target/release/deps -name 'dwowd-*' -executable
-exec cp {} /app/dwowd_lib_tests`. Both the daemon (`cargo build -p dwowd`, `Dockerfile:152`) and the libtest
harness (`cargo test -p dwowd --lib --no-run`, `Dockerfile:160`) emit a `target/release/deps/dwowd-<hash>`
executable, so the `find` matches **two** binaries and the last `cp` wins non-deterministically. Nothing checks
that the installed binary is the libtest harness, and no spec or e2e test covers this delivery seam.

This single gap is the root of V1–V5. The correct fix is a **deterministic binary selection** plus an
**in-pipeline smoke check**, not a one-off re-run.

---

## Findings

### V1 — OTHER THAN (select) → phase 98 delivery

- **Node:** `select!` binary selection (`Dockerfile:161-162`).
- **Deviation:** the `find 'dwowd-*'` predicate installs the **daemon** (clap-based `dwowd`) as
  `/app/dwowd_lib_tests` instead of the **libtest harness**.
- **Mechanism:** `cargo build -p dwowd` (daemon) and `cargo test -p dwowd --lib --no-run` (harness) both place a
  `target/release/deps/dwowd-<hash>` executable; `-exec cp {}` runs for every match, so the last copy wins.
  When the daemon wins, `phase_98_capability_tests.sh:37-44` hands the positional test names to the daemon's
  clap parser, which rejects them: `Found argument 'test_box_put_wallet_driven_generic_prover' which wasn't
  expected`. The two wallet-driven tests never run.
- **Invariant violated:** the delivery invariant that the installed binary MUST be the `cargo test --lib`
  harness (so a phase failure is a real test failure, not a binary-selection error).
- **Structural fix:** capture the libtest binary path deterministically via
  `cargo test --lib --no-run --message-format=json-render-diagnostics` (parse the `"executable"` field) and
  `cp` that exact path; do not use a loose `find 'dwowd-*'`.

### V2 — AS WELL AS (select) → Docker build

- **Node:** `find target/release/deps -name 'dwowd-*' -executable` (`Dockerfile:161-162`).
- **Deviation:** the predicate matches **two** `dwowd-*` executables (daemon + libtest harness); `-exec cp`
  copies both, and the last one (filesystem-order dependent) wins.
- **Mechanism:** binary-target and lib-test-target crate metadata produce different hashes, so both
  `dwowd-<hash>` names coexist in `target/release/deps/`. Nothing anchors the copy to the harness.
- **Invariant violated:** deterministic build — the image content must not depend on `find` ordering.
- **Structural fix:** as V1 — select the exact harness path, not a prefix glob.

### V3 — NO (verify / e2e) → delivery seam

- **Node:** the entire build→select→install→invoke chain (`Dockerfile:156-163,181`,
  `phase_98_capability_tests.sh:37-44`).
- **Deviation:** no spec states the correctness property and **no test** exercises the Docker build/selection
  or the phase-98 invocation. Repo-wide there is zero `docker` usage in any `*.rs` and no CI.
- **Mechanism:** the capability tests themselves are covered as in-process lib tests
  (`capability_scan_integration.rs`), but the *delivery mechanism* into the image is a coverage vacuum — the
  bug shipped in commit `f62652838c` and was only caught on a manual pipeline run.
- **Invariant violated:** e2e coverage of the delivery seam.
- **Structural fix:** add an in-pipeline smoke check (V1) that verifies the installed binary is the libtest
  harness before running the tests, plus a host-side mirror check.

### V4 — NO (spec) → `pipeline_spec.py`

- **Node:** `contrib/docker/darkwow-testnet/pipeline_spec.py` (declared source of truth, `:4-11`).
- **Deviation:** the spec omits `phase_98`, the global `CAPABILITY_TESTS`, and the `dwowd_lib_tests` binary —
  `phase_modules` jumps from `phase_12_bridge` to `phase_20_report`, `SOURCING_ORDER` has 18 modules without
  phase 98, and `GLOBALS` has no `CAPABILITY_TESTS`. This is a pre-existing spec↔implementation divergence.
- **Mechanism:** the phase was added in-tree (`test_pipeline.sh:55,309-311`) without updating the declared
  source of truth.
- **Invariant violated:** spec↔implementation conformance ("the bash implementation must match this model
  exactly", `pipeline_spec.py:9-11`).
- **Structural fix:** model `phase_98` (`phase_capability_tests`), `CAPABILITY_TESTS`, and `dwowd_lib_tests`
  in `pipeline_spec.py`; document the binary-selection invariant.

### V5 — PART OF (invoke) → phase 98 opt-in scope

- **Node:** `CAPABILITY_TESTS` default + phase guard (`config.sh:142`, `phase_98_capability_tests.sh:14-16`).
- **Deviation:** the phase is opt-in (default 0) and runs **only** `test_box_put_wallet_driven_generic_prover`
  and `test_box_take_wallet_driven_generic_prover` — not the purse or PN wallet-driven tests.
- **Mechanism:** a default pipeline run never reaches the phase, so a regression in the delivery seam is
  invisible until the flag is passed.
- **Invariant violated:** e2e coverage of the delivery seam is not exercised by the default path.
- **Structural fix:** keep the phase opt-in (it is slow), but make the smoke check (V1) fail fast and make the
  delivery deterministic so a manual `--capability-tests` run is trustworthy.

---

## Checklist (feeds the remediation)

1. Deterministic binary selection in `Dockerfile` (capture the `cargo test --lib` executable path) — V1/V2.
2. Phase-98 smoke check: `dwowd_lib_tests --list` must be libtest-format and contain the two test names — V3/V5.
3. Host-side mirror check (run the compiled `dwowd-* --list`) — V3.
4. `pipeline_spec.py`: add `phase_98` + `CAPABILITY_TESTS` + `dwowd_lib_tests` + the invariant — V4.
5. `level-3-localnet.md` flag list + `l3-readiness-spec.md` gate — V4.
