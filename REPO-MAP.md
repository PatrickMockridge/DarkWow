# REPO-MAP.md — the verified map of this repository

Read [`AGENTS.md`](AGENTS.md) first; this file is the second read. `AGENTS.md` and
[`doc/src/dev/hazop-rules.md`](doc/src/dev/hazop-rules.md) hold the doctrine — how to fix, what
counts as evidence, and the binding operating constraints. This file only maps the tree. Every path
and count below was re-derived from the working tree on 2026-11-01; if the tree moves, re-derive
them, do not hand-copy.

Development happens on the **`linear-master`** branch.

## What this is

DarkWow is a privacy-preserving, proof-of-work blockchain built around object capabilities and
promissory-note value, with deterministic Uncle Merkle consensus. Read
[`doc/src/intro.md`](doc/src/intro.md) for the project and
[`doc/src/arch/formal-specification.md`](doc/src/arch/formal-specification.md) for the one-page
specification. The README lists the six design commitments.

## Top level

| Path | What it is |
|---|---|
| `src/` | The `dwow_core` crate (root `Cargo.toml`, `src/lib.rs`) and its modules, plus the sibling crates below |
| `src/contract/` | 34 directories: 32 contracts + the `entropy` library + `test-harness` (see Contracts) |
| `bin/` | Executable crates (see Binaries) |
| `crates/` | `dwow-accounts` (key/account manager) and `dwow-mempool` |
| `proofs/` | `proofs/core/*.zk` and the Lean 4 verification tree `proofs/lean/` |
| `contrib/` | Python consensus models (`contrib/model/`) and the Docker test pipeline (`contrib/docker/`) |
| `doc/` | mdBook source (`doc/src/`); `doc/book/` is generated and gitignored |
| `scripts/` | Gate scripts, including every documentation gate in this file |
| `hooks/` | `hooks/pre-commit`, the circuit/tracker pre-commit gates |
| `reports/` | Dated status, ALARP and audit records — evidence, not live claims |
| `plans/` | Process artefacts; never cite one from a repository file |
| `vendor/` | Vendored dependencies (notably `vendor/halo2`, pinned — see AGENTS.md) |
| `example/`, `fuzz/`, `bench/`, `test-*-data/` | Examples, fuzzing, benchmarks, test fixtures |

## Core crate modules (`src/`)

| Module | What it is |
|---|---|
| `sdk/` | `dwow-sdk`: contract SDK — crypto, WASM host interface, tx, capability types, serialization |
| `serial/` | `dwow-serial`: the repository's binary serialization (no serde on the wire) |
| `linear/` | `dwow_chain`: consensus — chain state, block production/acceptance, fee window, RandomX |
| `transport/` | `dwow_transport`: TCP/TLS/Tor/Nym/QUIC/Unix transports |
| `runtime/` | WASM runtime: host imports, execution phases, gas and bulk-memory metering |
| `zk/` | The zkVM (opcodes, proofs, gadgets). **`src/zk/vm.rs` is off-limits.** |
| `zkas/` | The ZK assembler/compiler backend |
| `net/`, `transport/`, `rpc/` | P2P sessions, transports, JSON-RPC client/server plumbing |
| `event_graph/`, `dht/`, `geode/` | P2P messaging DAG (darkirc), DHT, chunked storage |
| `tx/`, `blockchain/` | Transaction types and the chain-facing re-exports |
| `core/`, `util/`, `concurrency/` | Shared primitives, helpers, StoppableTask/executor utilities |
| `contract/` | The contract crates (below) |

## Supporting crates

| Crate | Path | What it is |
|---|---|---|
| `dwow_chain` | `src/linear/` | Consensus engine and fee window |
| `dwow-sdk` | `src/sdk/` | Shared SDK used by contracts and applications |
| `dwow-serial` | `src/serial/` | Wire serialization |
| `dwow_transport` | `src/transport/` | Transport abstraction |
| `dwow-accounts` | `crates/dwow-accounts/` | AccountManager — the single key/account authority |
| `dwow_mempool` | `crates/dwow-mempool/` | Fee-indexed multi-queue mempool |

## Binaries (`bin/`)

| Binary | What it is |
|---|---|
| `dwowd` | Node daemon: chain, P2P sync, mempool, JSON-RPC/stratum, opt-in built-in miner |
| `dww` | `dwow_wallet` — CLI wallet client |
| `darkwow` | Top-level CLI: launch a node/observer, a wallet, or account operations |
| `darkirc` | P2P IRC daemon over the event-graph stack |
| `tau` | `taud` — encrypted P2P task daemon; `tau_pallas` is the Pallas-native variant |
| `lilith` | Seed-node spawner |
| `dnet` | P2P traffic viewer/debugger |
| `deg`, `genev` | Event-graph debugger and daemon/CLI |
| `fud`, `fu` | DHT file-sharing daemon and its client |
| `explorer` | Block explorer |
| `zkas`, `zkrunner` | ZK compiler; ZK debugger/renderer |
| `bridge_test_helper` | Generates bridge proofs and encoded calls for pipeline tests |
| `app` | Desktop UI application |

## Contracts

`src/contract/` holds **34 directories**:

- **32 contracts**. Nine are genesis: `native_token`, `deployooor`, `promissory_note`, `box`,
  `purse`, `identity`, `oracle`, `attestation`, `multisig`. The other 23 are post-genesis.
- **31 of the 32 carry a `manifest.toml`**; `deployooor` is deliberate plaintext infrastructure
  with none.
- **`entropy` is a library**, not a contract; **`test-harness` is test infrastructure**.

The canonical catalog is [`doc/src/contracts.md`](doc/src/contracts.md); genesis is
[`doc/src/arch/genesis.md`](doc/src/arch/genesis.md). Each contract directory is self-contained:
`src/`, `proof/*.zk`, `manifest.toml`, `tests/`, and a `README.md`. `src/sdk/**` and
`src/serial/**` are in every contract's `SOURCE_MANIFEST` — **one edit there stales all 32
artifacts and can move the genesis pin**; see `AGENTS.md`.

## Find by task

| Task | Start here | Code |
|---|---|---|
| Understand the project | `doc/src/intro.md`, `doc/src/arch/formal-specification.md` | — |
| Consensus, fees, mining | `doc/src/arch/consensus/consensus.md`, `consensus-coinbase.md` | `src/linear/` |
| Wallet and capabilities | `doc/src/arch/wallet.md` | `bin/dww/`, `crates/dwow-accounts/` |
| Contract manifest / deploy | `doc/src/arch/manifest.md`, `genesis.md` | `src/contract/`, `src/sdk/` |
| Write or audit a contract | `doc/src/dev/contracts/journey.md`, `safety.md` | `src/contract/<name>/` |
| Debug a ZK circuit | `doc/src/dev/zk-circuit-troubleshooting.md` | `src/zk/`, `src/zkas/` |
| Test at a given level | `doc/src/dev/testing/overview.md` | `src/contract/test-harness/` |
| Run the Docker pipeline | `doc/src/dev/testing/level-3-localnet.md` | `contrib/docker/darkwow-testnet/` |
| Formal verification | `proofs/lean/README.md` | `proofs/lean/` |
| Python consensus model | `doc/src/dev/testing/python-simulations.md` | `contrib/model/` |
| What is broken | `doc/src/arch/verification-hazop.md` | the row's locus/control |
| Accepted risk / interpretations | `doc/src/arch/decisions.md`, `residual-risk.md` | — |

## Documentation system

- [`doc/src/SUMMARY.md`](doc/src/SUMMARY.md) is the canonical table of contents; every `.md` under
  `doc/src` must be listed and every listed file must exist.
- The normative safety document is [`doc/src/dev/contracts/safety.md`](doc/src/dev/contracts/safety.md)
  (RC1–RC12); the normative working rules are `doc/src/dev/hazop-rules.md`.
- `doc/src/arch/ai-index.md` is **retired**; this file replaces it.
- `doc/src/arch/audit/` and `reports/` are **dated records**. They are evidence of what was true
  when written, never a live claim, and are not edited by maintenance.
- `doc/book/` is generated HTML, gitignored, and never committed.

## Gates

Run from the repository root; no cargo is required:

```bash
scripts/check-doc-index.sh                 # SUMMARY ↔ files, links, includes, OBL ids, deleted docs
scripts/check-doc-code-paths.sh            # backticked paths in doc/src resolve
scripts/check-doc-canonical-counts.sh      # population counts match the tree
scripts/check-contract-doc-functions.sh    # contract pages vs manifest functions
scripts/check-contract-doc-circuits.sh     # contract pages vs proof/ circuits
scripts/check-manifest-entrypoint-agreement.sh
scripts/check-repo-doc-links.sh            # relative links in READMEs/top-level docs
scripts/check-authority-resolves.sh
scripts/check-defect-register.sh
scripts/check-ledger-promotion.sh --check
```

`scripts/check-tracker-settled.sh` **refuses** while any finding is undecided; as of 2026-11-01
`OBL-C279` is DEFERRED, so the tracker refuses and every verification surface is blocked. That is a
tracker state, not a documentation defect. Build the book with `mdbook build` from `doc/` (or
`make -C doc book`); **never** `make -C doc all`, which runs cargo/rustdoc.

### CI

CI is three Pareto lanes, executed by `contrib/ci/run-ci-lane.sh` and selected by
[`contrib/ci/gate-groups.tsv`](contrib/ci/gate-groups.tsv): `fast` on a PR (path-selected
instruments, tracker advisory, no verdict), `merge` on a push (tracker-gated lightweight Tier 1),
and `nightly` (nine-genesis heavyweight, `zk_audit`, the `dao_escrow` PoW-determinism repeat,
Docker). `scripts/run-all-tests.sh --list-gates`
is the inventory; `contrib/ci/check-gate-coverage.py` refuses a gate with no lane/group row. See
[`doc/src/dev/testing/ci.md`](doc/src/dev/testing/ci.md). While the tracker is undecided, only the
fast instruments run; merge and nightly exit 78 as blocked.

## Source of truth

1. **Code** decides behaviour. A document, a HAZOP, an audit and an agent summary are hypotheses
   (AGENTS.md R6). Names lie: verify the symbol, not the label.
2. **`verification-hazop.md`** is the defect register: `OPEN`/`BLOCKED`/`DEFERRED` are undecided;
   `CLOSED` and `RESIDUAL` are the only exits.
3. **`safety.md` + `hazop-rules.md`** are the normative rules.
4. **`decisions.md` + `residual-risk.md`** are accepted risk and interpretation — neither can green
   a defect.
5. A **dated record** (`reports/`, `plans/`, `arch/audit/`) is evidence about its date, not about
   today.

## Maintenance

Re-derive the counts (32 contracts, 31 manifests, 9 genesis, 34 contract directories, 165/177
circuits) with the gates above when the tree changes. Add a row here only for a new top-level area
or a new entrypoint; do not hand-copy per-file trees — `SUMMARY.md` is the chapter list.
