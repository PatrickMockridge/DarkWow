# L1 Capability Write-Path — Capability-Tests Phase Delivery Trace

This is the ρ-calculus trace of the **delivery path** for the opt-in `--capability-tests` pipeline phase
(phase 98): how the wallet-driven box `put`/`take` tests are compiled into a binary, selected, installed into
the Docker image, invoked, and how the invoked harness verifies the write path. Notation per
`ocap.md §6` / `wallet.md §6.4.1`.

It is the **input to the HAZOP** (`l1-capability-tests-phase-hazop.md`): the seam the trace makes visible is
the deviation point the HAZOP then analyzes.

The delivery path is distinct from the write path it verifies. The write path
(`l1-capability-write-path-trace.md`) is the ZK capability transfer; this trace covers the *test-delivery*
machinery that runs it in-Docker.

## The delivery path

```
Delivery =
  νimage.(
    build!(daemon)                                      — cargo build -p dwowd (Dockerfile:152)
      . build!(harness)                                 — cargo test -p dwowd --lib --no-run (Dockerfile:160)
      . select!(find 'dwowd-*' → /app/dwowd_lib_tests)  — Dockerfile:161-162   ← SEAM (wrong-binary)
      . install!(COPY /app/dwowd_lib_tests)             — Dockerfile:181
      . invoke!(docker run --entrypoint /app/dwowd_lib_tests <filters>)  — phase_98_capability_tests.sh:37-44
      . verify!(
          box put:  ↓spend → ↓nullify → ↓prove-inclusion → ↓commit → ↓encrypt
          box take: ↓spend → ↓nullify → ↓prove-inclusion → ↓commit
        )
  )
```

`verify!` is the actual wallet-driven tests `test_box_put_wallet_driven_generic_prover`
(`capability_scan_integration.rs:479`) and `test_box_take_wallet_driven_generic_prover`
(`capability_scan_integration.rs:606`), each submitting a `ManifestContractClient::build(…)` proof through
`accept_block` and asserting the `box_roots`/nullifier gate.

**SEAM (binary-selection):** `select!` must choose the **libtest harness** (`dwowd-<hash>` produced by
`cargo test --lib --no-run`, `Dockerfile:160`), not the **daemon** (`dwowd-<hash>` produced by `cargo build`,
`Dockerfile:152`). Both land as `target/release/deps/dwowd-<hash>` executables, so the `find -name 'dwowd-*'`
predicate matches **two** binaries and the last `-exec cp` wins non-deterministically. When the daemon wins,
`invoke!` hands the two positional test names to the daemon's clap parser, which rejects them
(`Found argument … which wasn't expected`), and `verify!` never runs — a spurious phase failure, not a real
test failure.

---

## Seam summary (→ HAZOP input)

| # | Seam | Trace step |
|---|---|---|
| S1 | `select!` may install the daemon instead of the libtest harness | `select!` |

This seam is the deviation point the HAZOP (`l1-capability-tests-phase-hazop.md`) analyzes with guide words.
