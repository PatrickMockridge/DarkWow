# Compile / Build Fragilities — HAZOP

Guide-word deviation analysis over the build-and-test pipeline of the linear chain crates (`dwow_chain`
= `src/linear`, `dwow_core`, `dwow_wallet` = `bin/dww`, `dwowd` = `bin/dwowd`). Guide words: NO / NOT /
PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE, plus MORE / LESS. Each finding cites
`file:line` on `linear-master` and maps to a build invariant. Scope: the compile-time and test-time
out-of-memory (OOM) failures observed 2026-09-14/15 running `cargo test --workspace --all-features`.

## Central root cause

**The wallet's dependency closure compiles the full consensus/VM stack it never exercises, and the
default build/test parallelism turns that closure into ~3–4× the available memory.** The wallet
(`bin/dww`) depends on `dwow_chain` (`bin/dww/Cargo.toml:22`), which hard-depends on `randomx`
(`src/linear/Cargo.toml:34`) and `dwow_core` `wasm-runtime` (→ `wasmer`) and `dwow_native_token_contract`
`client` (→ `halo2`). The wallet's scan path deliberately does **not** re-hash PoW (`bin/dww/src/lib.rs:359,380`)
and does not execute WASM — it trusts the synced block hash. Yet it compiles all three heavy crates.
Two defaults then multiply that closure's codegen into OOM: cargo's `-j` (= `nproc` = 24 concurrent
`rustc`) and `RAYON_NUM_THREADS=10` (codegen parallelism), and libtest's `--test-threads=24` for the
node-spawning tests. The machine is a shared 47 GiB desktop; only ~34 GiB is actually free, and it
fluctuates under microk8s / browsers / other Claude sessions.

## Findings

### F1 — AS WELL AS (randomx) → the wallet compiles the PoW stack it never runs

- **Node:** `dwow_chain` hard dependency `randomx` (`src/linear/Cargo.toml:34`, non-optional).
- **Deviation:** `randomx` is the block's PoW hash — `Block::hash_with_vm` (`src/linear/src/block.rs:303`),
  `UncleBlock::hash_with_vm` (`block.rs:160`), `verify_uncle_proof` (`block.rs:406`), `consensus::verify_proof`
  (`consensus.rs:326`), `validation::check_block_header` / `check_uncles` (`validation.rs:110,238`), the
  `Miner` (`miner.rs`), the `CChainState` VM cache (`chain_state.rs:151,158,584`), and the execution context's
  `vm: Arc<RandomXVM>` (`execution.rs:104`). The wallet's `dwow_chain` import closure
  (`bin/dww` `dwow_chain::` sites) uses only data types + merkle + sync + fee-window — never `Miner`,
  `verify_proof`, `hash_with_vm`, `check_block_header`, or `CChainState`. So `randomx` is compiled into the
  wallet without being used.
- **Invariant violated:** build bloat — a crate SHALL NOT compile consensus machinery its code path never
  reaches (`phantom-code-removed-first`).
- **Structural fix:** make `randomx` optional behind a `pow` feature in `dwow_chain`, gate the RandomX
  items, and have `dwowd` opt in. **Measured scope: ~6 files, ~50 gate sites** (see §Action A1). Not a leaf
  removal — the VM cache and execution context are structural.

### F2 — AS WELL AS (wasmer) → `wasm-runtime` is a phantom, and larger than randomx

- **Node:** `dwow_chain` → `dwow_core` `features = ["wasm-runtime"]` (`src/linear/Cargo.toml:48`).
- **Deviation:** `wasm-runtime` pulls `wasmer` (the WASM VM). The wallet scans blocks (parse + AEAD decrypt),
  it does not execute contract WASM. `wasmer` is therefore compiled into the wallet without being used, and
  is a larger compile-memory contributor than `randomx`.
- **Invariant violated:** same build-bloat invariant as F1.
- **Structural fix:** separate the WASM-execution surface of `dwow_core` so `dwow_chain`'s wallet-facing
  dependency does not enable `wasm-runtime` (or split the sync/scan path from the execution path). **This is
  the higher-value removal** — likely the dominant phantom in the wallet's closure.

### F3 — NOT removable → `halo2` is genuinely required by the wallet

- **Node:** `dwow_native_token_contract` `features = ["client"]` (`bin/dww/Cargo.toml`, `src/linear/Cargo.toml:49`).
- **Deviation:** the wallet generates `Mint_V2`/`Spend_V2`/`Transfer_V2` proofs to spend its notes; that is
  `halo2`. It is the largest single compile-memory contributor in the closure and **cannot** be removed.
- **Invariant violated:** none — this is the legitimate floor. The OOM budget must accommodate it.

### F4 — MORE (codegen parallelism) → the compile-phase OOM

- **Node:** `cargo test --workspace` with default `-j` (= `nproc` = 24 concurrent `rustc`) and
  `RAYON_NUM_THREADS=10`.
- **Deviation:** 24 concurrent `rustc` processes, each running 10-way LLVM codegen over the halo2/wasmer
  monomorphized generics, exceed ~34 GiB free — **before any test runs**. OOM'd three times (with
  `--test-threads=24`, `=2`, `=1`) at the compile phase, always around `Compiling dwow_wallet` / `dwowd` /
  `dwow-contract-test-harness`.
- **Invariant violated:** build determinism / resource budget — a build SHALL NOT require more memory than
  the host can provide; parallelism is a knob, not a fixed default.
- **Structural fix:** cap cargo build jobs (`-j 1` or `-j 4`) for heavy sweeps. `-j 1` (one `rustc` at a time)
  is what finally let the compile complete. Note `-j` is a *different* knob from `RAYON_NUM_THREADS`; lowering
  `-j` does not lower `RAYON_NUM_THREADS`.

### F5 — MORE (test parallelism) → the test-phase OOM

- **Node:** libtest default `--test-threads=24` on a 24-core host.
- **Deviation:** the `daemon_sync_integration` / `heavyweight_pipeline` tests each spawn full nodes (WASM VM
  + halo2 verifying keys + sled); 24 in parallel exceeded 47 GiB and OOM-killed the run, surfacing 11 bogus
  `daemon_sync_integration` FAILED lines (memory-pressure noise, not regressions — all green at `--test-threads=1`).
- **Invariant violated:** verdict integrity — a FAILED line is only evidence if the test environment was not
  memory-starved (`failures-are-recorded`).
- **Structural fix:** pass `-- --test-threads=1` for heavy sweeps; treat FAILED lines from an OOM'd run as
  suspect and re-run memory-safe before believing them.

### F6 — NO (fail-fast off) / PART OF (result set) → the sweep aborts at the first failure

- **Node:** `cargo test --workspace` default (fail-fast).
- **Deviation:** `quic_transport` (`dwow_core` `tests/network_transports.rs`) fails under `--all-features`
  because both rustls `aws-lc-rs` and `ring` features are enabled — "exactly one of 'aws-lc-rs' and 'ring'".
  That single upstream test aborts the whole workspace sweep at ~13 of ~45 test binaries, hiding every
  subsequent result (including `dwowd`/`daemon_sync`).
- **Invariant violated:** verdict completeness — the sweep SHALL report every binary's result.
- **Structural fix:** add `--no-fail-fast`; separately resolve the rustls feature conflict (the `--all-features`
  union enables both providers).

### F7 — MORE (feature union) → `--all-features` enables `sharding` scaffolding

- **Node:** `dwow_chain` `sharding = ["dwow-sdk/sharding"]` (`src/linear/Cargo.toml:18`), all bodies `todo!()`.
- **Deviation:** `--all-features` enables the post-mainnet scaffolding, which is dead (`todo!()`) and, per its
  own comment, "DO NOT ENABLE before mainnet". It also masks the local `shard.rs` cfg gap (E0432 under
  package-scoped `--all-features`).
- **Invariant violated:** build hygiene — a feature union SHALL NOT enable not-yet-landed scaffolding.
- **Structural fix:** none for this OOM (compile cost is trivial); record that `--all-features` is not a
  meaningful "production feature set" for `dwow_chain`.

### F8 — NO (guard) → no compile-memory budget, on a shared host

- **Node:** no memory guard or budget anywhere in the build/test path; the host's ~34 GiB free is shared and
  fluctuating (microk8s ~600 MiB, browsers, other Claude sessions).
- **Deviation:** OOM is the first (and only) signal, delivered as a task kill mid-compile, leaving a
  half-written `target/` that forces a recompile next run (the vicious cycle observed).
- **Invariant violated:** resource predictability.
- **Structural fix:** adopt the standing rule (run heavy compiles/tests sequentially; check `free -h` before a
  second heavy task; `-j 1` + `--test-threads=1` + `--no-fail-fast` for sweeps); on a kill, clean the partial
  datadir (`/tmp/dwow-genesis-repin` and similar).

## Action items (all-or-nothing, per `hazop-completion`)

- **A1 — randomx out of the wallet (F1).** Feature-gate `pow` in `dwow_chain`; `dwowd` opts in, wallet does
  not. **Large (~6 files, ~50 sites incl. `execution.rs` and `CChainState`), consensus-critical, and LOW
  OOM payoff** (randomx is the smallest of the three heavy crates). Recommend a dedicated, carefully-sequenced
  session rather than a rushed in-line change.
- **A2 — wasmer out of the wallet (F2).** Separate the `wasm-runtime` surface from `dwow_chain`'s wallet-facing
  path. **Higher OOM payoff than A1.**
- **A3 — halo2 is the floor (F3).** No action; budget around it. If the OOM must be eliminated outright, the
  lever is codegen parallelism (A4/A5), not dependency removal.
- **A4 — cap codegen (F4).** `-j 1`/`-j 4` for heavy sweeps (already adopted). If the compile still OOMs on a
  single `rustc`, lower `RAYON_NUM_THREADS` below 10 — that conflicts with the standing `RAYON_NUM_THREADS=10`
  rule and needs explicit sign-off.
- **A5 — cap test parallelism (F5).** `--test-threads=1` for heavy sweeps (already adopted).
- **A6 — `--no-fail-fast` + rustls conflict (F6).** Add `--no-fail-fast` to sweeps; fix the `aws-lc-rs`/`ring`
  `--all-features` conflict so `quic_transport` stops aborting the sweep.
