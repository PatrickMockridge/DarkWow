# Continuous Integration

The repository's CI is three Pareto lanes on a self-hosted GitHub Actions runner. The executor is
[`contrib/ci/run-ci-lane.sh`](../../../../contrib/ci/run-ci-lane.sh); the gate inventory remains
[`scripts/run-all-tests.sh`](../../../../scripts/run-all-tests.sh), and
[`contrib/ci/gate-groups.tsv`](../../../../contrib/ci/gate-groups.tsv) says which gate runs in
which lane and group.

## Lanes

| Lane | Trigger | Budget | Contents |
|---|---|---|---|
| `fast` | `pull_request` to `linear-master` | < 5 min typical | Path-selected static, circuit, manifest and doc instruments, compile guard, one production-path proof test; tracker advisory |
| `merge` | `push` to `linear-master` | 30–60 min | Tracker preflight, the lightweight majority of Tier 1, the L1.5 production-path controls |
| `nightly` | `schedule` and `workflow_dispatch` | 2–4 h across jobs | Nine-genesis heavyweight, `zk_audit`, the `dao_escrow` PoW-determinism repeat (`OBL-C279`), the Docker pipeline, and a cost report |

A push runs **merge only**: the umbrella already contains the fast instruments, so running both
would be duplicate work. The selector (`contrib/ci/select_groups.py`) maps changed paths to groups;
models, Lean, Docker and shared-sdk get their own fast jobs rather than being repeated in
`fast-static` (shared-sdk's pre-build guard compiles every contract, which is `fast-compile`'s job).
The jobs that run umbrella gates on a fresh checkout build what those gates assume: `fast-static`
builds zkas and the `proofs/core` circuits for the `validate_zk_bins.sh` control, and
`fast-compile` builds the contract circuits and the nine genesis WASMs the pre-build guard embeds.

## The `make test` split

`make test` is a full workspace `--release` sweep, and that sweep includes the heavyweight
functions and `zk_audit` (~82 minutes measured). CI passes `TEST_ARGS` to it: the merge lane skips
`test_all_harnesses_zk_coverage`, `test_heavyweight_` and `test_relayer_lifecycle_heavyweight`.
Nightly does **not** run that whole skipped set: it runs the nine genesis contracts, `zk_audit`,
and — because it is the only test that ever showed `OBL-C279` — `test_heavyweight_dao_escrow` with
`DWOW_POW_FORENSIC=1`. A local `make test` with no `TEST_ARGS` is unchanged.

## Tracker policy

While any finding is `OPEN`, `BLOCKED` or `DEFERRED`, the heavy lanes are blocked: `merge` and each
nightly job begin with `scripts/check-tracker-settled.sh --require-clean` and exit 78 with the
script's refusal if it fails. The fast lane runs the **instruments** only and prints the tracker
state as an advisory; it makes no verification verdict, and a green fast check must never be read as
one. As of 2026-11-01 `OBL-C279` is `DEFERRED`, so every heavy lane is blocked.

## Running locally

```bash
# The fast instruments, selected by changed paths
python3 -B contrib/ci/select_groups.py --base origin/linear-master --head HEAD
contrib/ci/run-ci-lane.sh fast-static --groups docs,contracts

# What a lane would do, without cargo/Lean/Docker
contrib/ci/run-ci-lane.sh merge --dry-run
contrib/ci/run-ci-lane.sh nightly-heavyweight --dry-run

# The gate inventory and the coverage ratchet
contrib/ci/run-ci-lane.sh --list-gates
python3 -B contrib/ci/check-gate-coverage.py
```

## Adding a gate

A new `run_gate` in the umbrella is automatically listed by `--list-gates`. It is **not** allowed to
run anywhere until it has a row in `contrib/ci/gate-groups.tsv` (`label, group, lanes, source,
budget_s`). `check-gate-coverage.py` runs in every fast lane and fails, naming the label, when a gate
is unclassified or a fast-only command has no row. A check the umbrella does not carry is defined in
`contrib/ci/run-ci-lane.sh` as a `gate "fast-only: …"` and classified with `source=fast-only`.

## Runner prerequisites

A 32 GB-class host with a systemd user session (for `systemd-run --user --scope`), lingering enabled,
Docker, the wasm32 target, full git history, and 20 GB free. Cargo's `target/` and registry live
under the runner's `~/.cache/darkwow/`; the checkout's `target/` is a symlink to that tree, because
the contract Makefiles copy artifacts from the workspace default path rather than from
`CARGO_TARGET_DIR`. Lean's `.lake` is a symlink into `$DARKWOW_LEAN_CACHE/<toolchain hash>`, so a
cold first elaboration is paid once and later builds are incremental. No cargo, Lean or
Docker command is wrapped in a second cgroup for Lean — `scripts/lean-build.sh` owns its own ceiling.

## Failure semantics

The fast lane exits non-zero only when a selected instrument fails. A heavy lane's tracker refusal is
exit 78 with `BLOCKED` in the summary: it is not a test failure, and it must not be reported as one.
