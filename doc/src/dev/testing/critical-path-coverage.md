# Critical-Path Test-Coverage Specification

This is the **normative coverage specification** for the operations that move value or capability
on-chain — the critical path. It defines, in RFC 2119 terms, which production validation gates every
critical-path operation MUST have a *real* integration test witness, and it classifies every existing
test as **real** or **fake** so a synthetic-block test can never again be mistaken for coverage.

It SHALL be read with [Testing Overview](overview.md) (the Level 1–4 taxonomy and the Level 1.5
"Pre-Production Bridge"), [Production Test Standard](production-test-standard.md), and
[Test Suite Audit](test-audit.md).

## 1. The critical path

Two families move material on-chain:

| Family | Operations | Contracts |
|---|---|---|
| **Native token (DRKW)** | coinbase (mint), transfer/spend, fee | `native_token` |
| **Capability** | put/take (transfer), mint, redeem | `box`, `promissory_note`, `purse` |

The write-path operations (transfer/spend, put/take, redeem) are the consensus-critical seams: each
consumes a prior output by proving its nullifier and its Merkle inclusion in a historical commitment tree.

## 2. The production validation gate

Every write-path operation SHALL satisfy, at block `exec`, that the input's `merkle_root` is a key in
the contract's historical `coin_roots` tree:

| Contract | Gate | Location |
|---|---|---|
| `native_token` transfer | `db_contains_key(coin_roots_db, input.merkle_root.to_bytes())` else `TransferMerkleRootNotFound` | `src/contract/native_token/src/entrypoint/mod.rs` (`transfer_v1`, ~:699-702) |
| `native_token` fee | same check, `fee_v2` | `entrypoint/mod.rs` (~:228-232) |
| `promissory_note` transfer/redeem | same check | `src/contract/promissory_note/src/entrypoint/mod.rs` (~:732) |
| `box` put | `db_contains_key(roots_db, expected_root)` | `src/contract/box/src/entrypoint/mod.rs` (~:103) |

`coin_roots` is populated only by `merkle_add` during `apply_*` (`src/runtime/import/merkle.rs`), which
runs inside `accept_block`. A test that never calls `accept_block` cannot reach this gate.

## 3. The real-vs-fake rule

- **REAL** test: runs the operation through `accept_block` (full WASM `exec`→`apply`) and asserts
  **on-chain acceptance** — block height advances, the referenced `merkle_root` is in `coin_roots`
  (via `query_contract_state`), or the nullifier lands on-chain. A real test witnesses the gate in §2.
- **FAKE** test: builds the tx/note and feeds a hand-built synthetic `Block` straight to
  `scan_block_linear`, or never submits the tx at all. It exercises only the wallet receive/scan, never
  the write-path validation gate.

> **A FAKE test SHALL NOT count as coverage for a write-path operation.** It MAY count as coverage for
> the receive/scan path only. (This is the rule that let a synthetic-block "transfer" test pass while
> production rejected the transfer at `coin_roots_db`.)

## 4. Coverage matrix

Each row is a critical-path operation. `REAL` = reaches §2's gate; `FAKE` = does not.

### 4.1 Native token (DRKW)

| Operation | Existing test | Class | Reaches gate? |
|---|---|---|---|
| coinbase receive | `test_wallet_coinbase_scan_only` (`wallet_integration.rs`), `test_wallet_sync_pulls_blocks_to_balance`, `test_daemon_pull_sync_converges` | REAL | yes (`accept_block` → scan → DRKW) |
| transfer build | `wallet_integration.rs` phase 5b | FAKE | no (structural asserts only) |
| transfer receive | `test_transfer_receive_decrypt` | FAKE | no (synthetic block) |
| **transfer/spend accept** | `test_transfer_accepts_through_accept_block` (`wallet_transfer_integration.rs:398`) | REAL | yes (`accept_block` → height advances) |
| fee accept | heavyweight `fee_integration_spec` / `fee_collect_pipeline` | REAL | yes (harness path) |

### 4.2 Capability (Box, PromissoryNote, Purse)

| Operation | Existing test | Class | Reaches gate? |
|---|---|---|---|
| PN scan (receive) | `test_promissory_note_capability_scan` | FAKE | no (synthetic block) |
| Box scan (receive) | `test_box_send_receive` | FAKE | no (synthetic block) |
| **Box put accept** | `test_box_put_accepts_through_accept_block` (`capability_scan_integration.rs:359`) | REAL | yes (`box_roots` gate) |
| **Box take accept** | `test_box_take_accepts_through_accept_block` | REAL | yes (`box_roots` gate + nullifier) |
| **PN transfer accept** | `test_promissory_note_transfer_accepts_through_accept_block` | REAL | yes (`commitment_roots` gate) |
| **PN redeem accept** | `test_promissory_note_redeem_accepts_through_accept_block` | REAL | yes (`commitment_roots` gate) |
| Purse deposit/withdraw accept | `test_purse_deposit_withdraw_accepts_through_accept_block` (and heavyweight `purse` spec) | REAL | yes (`purse_roots` gate) |

## 5. Required real tests (the gaps)

Each is a **Level 1.5 Pre-Production Bridge** test (real ZK, real `accept_block`, real wallet scan,
`BlockTarget::MAX`, deterministic ZK), consistent with `overview.md` §"Pre-Production Integration
Tests". They MUST pass before the Docker pipeline (`genesis-is-sacred`: no mocking `accept_block`).

1. **Native transfer/spend acceptance** — `Dww::build_native_transfer` → assemble a real block
   `[coinbase, transfer, fee_collect]` → `accept_block` → assert height advanced and the transfer
   input's `merkle_root` is a `coin_roots` key. This is the regression guard for the wallet's
   `reconstruct_global_coin_map` (`bin/dww/src/walletdb.rs`).
   — **DONE:** `test_transfer_accepts_through_accept_block` (`wallet_transfer_integration.rs:398`).
2. **Capability transfer acceptance** — Box `put` (and PN transfer) → `accept_block` → assert the leaf
   root is in the contract's `coin_roots` tree and the recipient wallet discovers the capability.
   — **DONE:** `test_box_put_accepts_through_accept_block`, `test_box_take_accepts_through_accept_block`,
   `test_promissory_note_transfer_accepts_through_accept_block`,
   `test_promissory_note_redeem_accepts_through_accept_block`, and
   `test_purse_deposit_withdraw_accepts_through_accept_block` (`capability_scan_integration.rs`).

## 6. Conformance

A change to any write-path operation (native or capability) SHALL keep its row in §4 REAL, or the
change is a regression. The [Test Suite Audit](test-audit.md) records conformance against this matrix;
finding F-6 ("full spend/broadcast cycle untested") is **CLOSED** (both required real tests now exist).

## References

- [Testing Overview](overview.md) — Level 1.5 bridge definition, MoC gate
- [Production Test Standard](production-test-standard.md) — partitions A/B/C, anti-patterns
- [Test Suite Audit](test-audit.md) — F-6 and the conformance matrix
- [Wallet Architecture](../arch/wallet.md) §2 (scan) / §6 (write path)
