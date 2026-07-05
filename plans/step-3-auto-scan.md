# Step 3: Wallet Daemon Auto-Scans During Sync

**Status:** Planning | **Date:** 2026-07-01 | **Parent:** Steps 1-5 continuation

## Root Cause

The wallet daemon (`dispatch.rs:684-757`) spawns a continuous sync task (`run_wallet_sync`) that fetches blocks from peers and inserts them into the local `LinearStore`. But **nothing triggers `scan_blocks()` automatically**. Scanning is manual-only: CLI `scan` command or RPC `wallet.scan`. This means:

1. Pipeline must wait for sync, then manually trigger scan — slow and race-prone
2. Blocks accumulate in LinearStore unscanned — wallet doesn't discover owned coins until explicit scan
3. The scan creates a fresh `ScanCache` each invocation (sled reads, SQLite secrets query) — wasteful when called repeatedly

The fix: spawn a background scan-poll task in the daemon dispatch that periodically checks whether `last_scanned_height < chain_height` and runs `scan_blocks()` when behind.

## Recon Summary (3 agents, independent line-walks)

### Current Architecture

```
daemon dispatch (dispatch.rs:684-757)
├── spawns run_wallet_sync()  — continuous, every 10s, fetches+inserts blocks
├── spawns RPC server         — unix socket at /tmp/dww-{net}.sock
└── smol::future::pending()   — blocks forever

run_wallet_sync (sync_task.rs:184-324)
├── Phase 1: check peers, read local chain height
├── Phase 2: GetTip → all peers, update highest_peer_tip
├── Phase 3: GetBlocks (batch 20) → insert_synced_block() for each
└── sleep 10s, repeat

scan_blocks (scan.rs:160-262) — MANUAL ONLY
├── get_last_scanned_block() from sled _scanned_blocks tree
├── scan_cache() — creates fresh ScanCache (sled MerkleTree + SQLite secrets)
├── while height <= chain_height: scan_block_linear()
└── returns when caught up
```

### Key Files

| File | Lines | What |
|---|---|---|
| `bin/dww/src/dispatch.rs` | 684-757 | Daemon command — where auto-scan task must be spawned |
| `bin/dww/src/sync_task.rs` | 184-324 | `run_wallet_sync()` — sync loop (no scan) |
| `bin/dww/src/scan.rs` | 63-86 | `ScanCache` struct — created fresh each scan |
| `bin/dww/src/scan.rs` | 135-155 | `Dww::scan_cache()` — ScanCache factory |
| `bin/dww/src/scan.rs` | 160-262 | `Dww::scan_blocks()` — existing scan logic, reusable as-is |
| `bin/dww/src/scan.rs` | 271-798 | `Dww::scan_block_linear()` — per-block scan |
| `bin/dww/src/lib.rs` | 156-177 | `Dww` struct — needs no new fields |
| `bin/dww/src/lib.rs` | 301-324 | `Dww::is_synced()` — already exists, useful for scan gate |
| `bin/dww/src/lib.rs` | 330-345 | `Dww::insert_synced_block()` — called by sync, no changes needed |

### Concurrency Safety

- Sync and scan both acquire `dww.read().await` — RwLock allows multiple concurrent readers ✓
- `scan_blocks` acquires `verified_anchor_height` mutex lock internally ✓
- Sync writes to `chain_db` sled, scan writes to `cache_db` sled — different databases ✓
- SQLite uses WAL mode, concurrent reads (scan) + reads (sync doesn't touch wallet) — no conflict ✓
- **No deadlock risk**: both tasks acquire read lock independently, release between iterations ✓

## Plan

### Change 1: Spawn auto-scan task in daemon dispatch

**File:** `bin/dww/src/dispatch.rs`, after line 721 (after sync task spawn, before RPC setup)

Add a background task that:
1. Sleeps 2s (let sync get started first)
2. Loops: checks if `last_scanned < chain_height`
3. If behind, calls `dww.read().await.scan_blocks(&mut vec![], None, &false)`
4. Sleeps 5s, repeats

```rust
// Spawn auto-scan task — polls for new blocks and scans them automatically.
// Eliminates the need for manual scan trigger (CLI or RPC) in the pipeline.
{
    let dww3 = dww.clone();
    smol::spawn(async move {
        // Let sync start first before polling
        smol::Timer::after(std::time::Duration::from_secs(2)).await;
        loop {
            let dww_r = dww3.read().await;
            let chain_h = dww_r.chain_height().unwrap_or(0);
            let (last_scanned, _) = dww_r.get_last_scanned_block().unwrap_or((0, [0u8; 32]));
            if chain_h > 0 && (last_scanned as u64) < chain_h {
                drop(dww_r);
                // scan_blocks acquires its own read lock internally
                let mut output = vec![];
                if let Err(e) = dww3.read().await.scan_blocks(&mut output, None, &false).await {
                    tracing::error!(target: "dww::wallet::autoscan",
                        "Auto-scan failed: {}", e);
                }
            } else {
                drop(dww_r);
            }
            smol::Timer::after(std::time::Duration::from_secs(5)).await;
        }
    }).detach();
}
```

### Change 2: Make `scan_blocks` usable without output params

**File:** `bin/dww/src/scan.rs`, line 160

No changes needed — `scan_blocks` already accepts `output: &mut Vec<String>`, `sender: Option<&Sender<...>>`, `print: &bool`. Passing `&mut vec![]`, `None`, `&false` works as-is. The function already drops output silently when `sender` is None and `print` is false.

### What We Don't Change

- **`Dww` struct** — no new fields. The ScanCache recreation cost is negligible compared to block processing time.
- **`run_wallet_sync`** — unchanged. Sync and scan are independent tasks.
- **`scan_blocks`** — unchanged. Reused as-is.
- **`scan_block_linear`** — unchanged.
- **`Cache` / `BlockScanner`** — unchanged.

### Why Not Inline Scan in run_wallet_sync?

Alternative considered: modify `run_wallet_sync` to call `scan_block_linear` after each `insert_synced_block`. Rejected because:

1. `scan_block_linear` requires `&mut ScanCache` — would need to thread it through `run_wallet_sync` parameter list
2. `ScanCache` creation is expensive (sled reads, SQLite queries) — can't create it per-block
3. Would couple sync and scan, making the sync loop slower and harder to debug
4. Separate task is the mining node pattern — `consensus_linear_init_task` is a one-shot sync, not a continuous loop, but the principle of separation holds

### Why Not Persistent ScanCache on Dww?

Alternative considered: add `scan_cache: Mutex<Option<ScanCache>>` to Dww. Rejected because:

1. `ScanCache` contains `MerkleTree` and `CacheSmt` which are complex types — interior mutability adds complexity
2. Secrets can change (key import) — would need cache invalidation logic
3. ScanCache recreation cost is ~100ms (one sled read + one SQLite query) vs scan_blocks runtime of seconds-to-minutes — not worth optimizing
4. The separate-task approach reuses `scan_blocks` unchanged — zero risk of breaking existing scan correctness

## Verification

After implementation:
1. `cargo check -p dwow_wallet` — compiles
2. `cargo test -p dwow_wallet` — existing tests pass
3. Pipeline: daemon starts, syncs blocks, and scan happens automatically without manual `scan` command
4. Confirm `wallet.balance` RPC returns correct balances after auto-scan completes

## Failure Modes

| Failure | Detection | Recovery |
|---|---|---|
| Scan task panics | tracing error log | Next loop iteration spawns fresh — no state to corrupt |
| ScanCache creation fails | `scan_blocks` returns Err | Logged, retried next poll cycle |
| Chain height regresses (reorg) | `get_last_scanned_block` returns height > chain | `scan_blocks` handles this — re-scans from last_scanned |
| Sled lock contention | sled::Error | Only one process opens sled; within process, sled handles concurrent access |
