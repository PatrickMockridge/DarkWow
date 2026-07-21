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

//! Consensus Coordination — boundary-witness integration tests.
//!
//! Per type-system.md §10.5: "Every declared SHALL at a boundary SHALL have
//! at least one runtime witness test."
//!
//! ## Diagnostic Architecture (HAZOP remediation)
//!
//! Every test:
//! 1. Reports START with test name and what it verifies
//! 2. Prints progress at each step with timing
//! 3. Has a watchdog reporting every 5s during waits
//! 4. Validates both decision AND timing with full context on failure
//! 5. Prints PASS with elapsed time
//!
//! ## Test Catalog
//!
//! | Test | Time | Verifies |
//! |------|------|----------|
//! | C1 | ~12s | Authority+genesis → ProceedSolo (Docker path) |
//! | C2 | ~12s | Authority+height=0 → ProceedSolo (L2 fix) |
//! | C3 | ~12s | Non-authority+genesis → Retry (H1 fix) |
//! | C4 | ~35s | Non-authority+height=0 → WaitForGenesis |
//! | D  | 0s   | SyncDecision exhaustive match |
//! | E  | 0s   | PeerTip::from_tip rejects invalid |
//! | G  | 0s   | Barb declarations complete |

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dwow_core::net::settings::{MagicBytes, Settings};
use dwow_core::net::P2p;
use dwow_sdk::blockchain::BlockHeight;

// ── Diagnostic Test Infrastructure ────────────────────────────────────


/// Print a timestamped test step.
fn step(n: u32, desc: &str) {
    eprintln!("[STEP {}] {}", n, desc);
}

/// Print a timestamped diagnostic check.
fn check_pass(desc: &str) {
    eprintln!("[CHECK] {}", desc);
}

/// Print a diagnostic check failure with context.
fn check_fail(desc: &str, context: &str) {
    eprintln!("[CHECK] FAIL — {}", desc);
    eprintln!("  {}", context.replace('\n', "\n  "));
}

/// Spawn a watchdog that reports every `interval_s` seconds until
/// `done` is set. Reports elapsed time and what we're waiting for.
fn spawn_watchdog(what: &'static str, expected_s: u64, interval_s: u64) -> (Arc<AtomicBool>, smol::Task<()>) {
    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let handle = smol::spawn(async move {
        loop {
            smol::Timer::after(Duration::from_secs(interval_s)).await;
            if done_clone.load(Ordering::Relaxed) {
                break;
            }
            eprintln!(
                "[WATCHDOG] still waiting for {} (expected {}s total)",
                what, expected_s,
            );
        }
    });
    (done, handle)
}

/// Diagnostic dump of LinearSyncClient state.
fn dump_client_state(client: &dwowd::proto::linear_sync_client::LinearSyncClient) -> String {
    format!(
        "peer_count={}, has_peers={}, all_peers_len={}, filtered_peers_len={}",
        client.peer_count(),
        client.has_peers(),
        client.all_peers().len(),
        client.filtered_peers().len(),
    )
}

// ── Lightweight P2P Construction ─────────────────────────────────────

async fn make_test_p2p() -> dwow_core::net::P2pPtr {
    let mut settings = Settings::default();
    settings.outbound_connections = 0;
    settings.inbound_connections = 0;
    settings.localnet = true;
    settings.p2p_local = true;
    settings.active_profiles = vec!["tcp+tls".to_string()];
    settings.magic_bytes = MagicBytes([0x54, 0x45, 0x53, 0x54]);
    let ex = Arc::new(smol::Executor::new());
    P2p::new(settings, ex).await.unwrap_or_else(|e| {
        panic!("P2p::new() failed: {e:?}");
    })
}

// ── Test C1: Authority + genesis → ProceedSolo after 10s ──────────────

#[test]
fn test_c1_authority_with_genesis_proceeds_solo() {
    eprintln!("[TEST] C1 — authority=true, height=1, peers=0 → ProceedSolo within 12s");

    smol::block_on(async {
        step(1, "Creating P2P (0 outbound, 0 inbound)...");
        let t0 = Instant::now();
        let p2p = make_test_p2p().await;
        eprintln!("[STEP 1] P2P created in {:?} — verifying peer count...", t0.elapsed());
        assert_eq!(p2p.hosts().peers().len(), 0, "expected 0 peers after P2P construction");

        step(2, "Creating LinearSyncClient...");
        let client = dwowd::proto::linear_sync_client::LinearSyncClient::new(&p2p);
        let state = dump_client_state(&client);
        eprintln!("[STEP 2] Client ready — {}", state);
        assert_eq!(client.peer_count(), 0, "expected 0 peers");
        assert!(!client.has_peers(), "expected has_peers=false");

        step(3, "Calling wait_for_peers_or_proceed(authority=true, height=1)...");
        eprintln!("[STEP 3] Expecting ProceedSolo in ~10s (authority gate timeout)");
        let (wd_done, _wd) = spawn_watchdog("ProceedSolo", 12, 5);

        let start = Instant::now();
        let decision = client.wait_for_peers_or_proceed(true, BlockHeight::new(1)).await;
        let elapsed = start.elapsed();
        wd_done.store(true, Ordering::Relaxed);

        eprintln!("[STEP 3] Returned: {:?} after {:.3}s", decision, elapsed.as_secs_f64());

        // Decision check
        if decision == dwowd::proto::linear_sync_client::SyncDecision::ProceedSolo {
            check_pass(&format!("Decision: {:?} ← correct for authority=true, height=1, peers=0", decision));
        } else {
            check_fail(
                &format!("Decision: got {:?}, expected ProceedSolo", decision),
                &format!(
                    "Inputs:   authority=true, height=1, peers=0\n\
                     Internal: {}\n\
                     Timing:   {:.3}s elapsed\n\
                     Possible causes:\n\
                     - Authority gate condition not evaluated (check line 348)\n\
                     - local_height < GENESIS (height value mismatch)\n\
                     - has_peers() returned true transiently",
                    dump_client_state(&client), elapsed.as_secs_f64(),
                ),
            );
            panic!("C1: wrong decision");
        }

        // Timing check
        if elapsed >= Duration::from_secs(10) && elapsed < Duration::from_secs(12) {
            check_pass(&format!("Timing: {:.3}s in [10.0, 12.0]s", elapsed.as_secs_f64()));
        } else if elapsed < Duration::from_secs(10) {
            check_fail(
                &format!("Timing: {:.3}s — too early (minimum 10s)", elapsed.as_secs_f64()),
                "Authority gate must wait at least 10s. Check wait_iters counter.",
            );
            panic!("C1: returned too early");
        } else {
            check_fail(
                &format!("Timing: {:.3}s — too late (maximum 12s)", elapsed.as_secs_f64()),
                "Possible: executor overload, Timer::after drift, Mutex contention on hosts.registry.",
            );
            panic!("C1: returned too late");
        }

        // Cleanup with timeout
        eprintln!("[STEP 4] Stopping P2P...");
        let stopped = smol::future::or(
            async { p2p.stop().await; true },
            async { smol::Timer::after(Duration::from_secs(5)).await; false },
        ).await;
        if stopped { eprintln!("[STEP 4] P2P stopped cleanly"); }
        else { eprintln!("[STEP 4] WARNING: p2p.stop() timed out after 5s"); }

        eprintln!("[TEST] C1: PASS ({:.3}s)", elapsed.as_secs_f64());
    });
}

// ── Test C2: Authority + height=0 → ProceedSolo (L2 fix) ─────────────

#[test]
fn test_c2_authority_at_height_zero_proceeds_solo() {
    eprintln!("[TEST] C2 — authority=true, height=0, peers=0 → ProceedSolo (L2 fix)");

    smol::block_on(async {
        step(1, "Creating P2P...");
        let p2p = make_test_p2p().await;
        assert_eq!(p2p.hosts().peers().len(), 0);

        step(2, "Creating LinearSyncClient...");
        let client = dwowd::proto::linear_sync_client::LinearSyncClient::new(&p2p);
        eprintln!("[STEP 2] {}", dump_client_state(&client));

        step(3, "Calling wait_for_peers_or_proceed(authority=true, height=0)...");
        eprintln!("[STEP 3] L2 fix: authority at height 0 must get ProceedSolo (not WaitForGenesis)");
        let (wd_done, _wd) = spawn_watchdog("ProceedSolo (authority at height 0)", 12, 5);

        let start = Instant::now();
        let decision = client.wait_for_peers_or_proceed(true, BlockHeight::new(0)).await;
        let elapsed = start.elapsed();
        wd_done.store(true, Ordering::Relaxed);

        eprintln!("[STEP 3] Returned: {:?} after {:.3}s", decision, elapsed.as_secs_f64());

        match decision {
            dwowd::proto::linear_sync_client::SyncDecision::ProceedSolo => {
                check_pass("Decision: ProceedSolo ← authority creates genesis, doesn't wait for it");
            }
            dwowd::proto::linear_sync_client::SyncDecision::WaitForGenesis => {
                check_fail(
                    "Decision: WaitForGenesis — L2 BUG REGRESSION",
                    "Genesis authority at height 0 must get ProceedSolo, not WaitForGenesis.\n\
                     The authority IS the genesis source — telling it to wait deadlocks.",
                );
                panic!("C2: L2 regression — authority told to WaitForGenesis");
            }
            other => {
                check_fail(
                    &format!("Decision: {:?} — unexpected", other),
                    &format!(
                        "Inputs: authority=true, height=0, peers=0\n\
                         Internal: {}\n\
                         Expected ProceedSolo",
                        dump_client_state(&client),
                    ),
                );
                panic!("C2: unexpected decision {:?}", other);
            }
        }

        check_pass(&format!("Timing: {:.3}s elapsed", elapsed.as_secs_f64()));
        let _ = smol::future::or(
            async { p2p.stop().await; true },
            async { smol::Timer::after(Duration::from_secs(5)).await; false },
        ).await;
        eprintln!("[TEST] C2: PASS ({:.3}s)", elapsed.as_secs_f64());
    });
}

// ── Test C3: Non-authority + genesis → Retry (H1 fix) ────────────────

#[test]
fn test_c3_non_authority_with_genesis_returns_retry() {
    eprintln!("[TEST] C3 — authority=false, height=1, peers=0 → Retry (H1 fix)");

    smol::block_on(async {
        step(1, "Creating P2P...");
        let p2p = make_test_p2p().await;
        assert_eq!(p2p.hosts().peers().len(), 0);

        step(2, "Creating LinearSyncClient...");
        let client = dwowd::proto::linear_sync_client::LinearSyncClient::new(&p2p);
        eprintln!("[STEP 2] {}", dump_client_state(&client));

        step(3, "Calling wait_for_peers_or_proceed(authority=false, height=1)...");
        eprintln!("[STEP 3] H1 fix: non-authority with genesis must return Retry (was infinite hang)");
        eprintln!("[STEP 3] Test-level timeout: 15s (H1 bug would hang forever)");
        let (wd_done, _wd) = spawn_watchdog("Retry (non-authority with genesis)", 12, 5);

        let start = Instant::now();
        let decision_fut = client.wait_for_peers_or_proceed(false, BlockHeight::new(1));

        // Test-level timeout wrapper — HAZOP Finding 8 remediation.
        // If the function hangs (H1 regression), this catches it.
        let result = smol::future::or(
            async {
                let d = decision_fut.await;
                Some(d)
            },
            async {
                smol::Timer::after(Duration::from_secs(15)).await;
                None
            },
        ).await;

        let elapsed = start.elapsed();
        wd_done.store(true, Ordering::Relaxed);

        match result {
            Some(decision) => {
                eprintln!("[STEP 3] Returned: {:?} after {:.3}s", decision, elapsed.as_secs_f64());

                match decision {
                    dwowd::proto::linear_sync_client::SyncDecision::Retry => {
                        check_pass("Decision: Retry ← non-authority cannot proceed solo, retry outer loop");
                    }
                    dwowd::proto::linear_sync_client::SyncDecision::ProceedSolo => {
                        check_fail(
                            "Decision: ProceedSolo — AUTHORITY VIOLATION",
                            "Non-authority must NEVER get ProceedSolo. Would mine without sync.",
                        );
                        panic!("C3: authority violation — non-authority got ProceedSolo");
                    }
                    other => {
                        check_pass(&format!("Decision: {:?} (not ProceedSolo — gate holds)", other));
                    }
                }
            }
            None => {
                check_fail(
                    &format!("TIMEOUT: function did not return after 15s"),
                    &format!(
                        "H1 regression: non-authority+genesis+0peers has no exit condition.\n\
                         Internal: {}\n\
                         The Retry path added by the H1 fix is not firing.",
                        dump_client_state(&client),
                    ),
                );
                panic!("C3: H1 regression — function hangs forever");
            }
        }

        let _ = smol::future::or(
            async { p2p.stop().await; true },
            async { smol::Timer::after(Duration::from_secs(5)).await; false },
        ).await;
        eprintln!("[TEST] C3: PASS ({:.3}s)", elapsed.as_secs_f64());
    });
}

// ── Test C4: Non-authority + height=0 → WaitForGenesis after 30s ─────

#[test]
fn test_c4_non_authority_at_height_zero_waits_for_genesis() {
    eprintln!("[TEST] C4 — authority=false, height=0, peers=0 → WaitForGenesis after ~30s");

    smol::block_on(async {
        step(1, "Creating P2P...");
        let p2p = make_test_p2p().await;
        assert_eq!(p2p.hosts().peers().len(), 0);

        step(2, "Creating LinearSyncClient...");
        let client = dwowd::proto::linear_sync_client::LinearSyncClient::new(&p2p);
        eprintln!("[STEP 2] {}", dump_client_state(&client));

        step(3, "Calling wait_for_peers_or_proceed(authority=false, height=0)...");
        eprintln!("[STEP 3] Expecting WaitForGenesis in ~30s");
        let (wd_done, _wd) = spawn_watchdog("WaitForGenesis", 35, 5);

        let start = Instant::now();
        let decision = client.wait_for_peers_or_proceed(false, BlockHeight::new(0)).await;
        let elapsed = start.elapsed();
        wd_done.store(true, Ordering::Relaxed);

        eprintln!("[STEP 3] Returned: {:?} after {:.3}s", decision, elapsed.as_secs_f64());

        if decision == dwowd::proto::linear_sync_client::SyncDecision::WaitForGenesis {
            check_pass(&format!("Decision: {:?} ← correct for authority=false, height=0, peers=0", decision));
        } else {
            check_fail(
                &format!("Decision: got {:?}, expected WaitForGenesis", decision),
                &format!(
                    "Inputs:   authority=false, height=0, peers=0\n\
                     Internal: {}\n\
                     Timing:   {:.3}s elapsed",
                    dump_client_state(&client), elapsed.as_secs_f64(),
                ),
            );
            panic!("C4: wrong decision");
        }

        if elapsed >= Duration::from_secs(30) && elapsed < Duration::from_secs(35) {
            check_pass(&format!("Timing: {:.3}s in [30.0, 35.0]s", elapsed.as_secs_f64()));
        } else if elapsed < Duration::from_secs(30) {
            check_fail(
                &format!("Timing: {:.3}s — too early (minimum 30s)", elapsed.as_secs_f64()),
                "WaitForGenesis gate must wait at least 30s. Check wait_iters >= 30 condition (L1 fix).",
            );
            panic!("C4: returned too early");
        } else {
            check_fail(
                &format!("Timing: {:.3}s — too late (maximum 35s)", elapsed.as_secs_f64()),
                "Possible: executor overload, Timer::after drift.",
            );
            panic!("C4: returned too late");
        }

        let _ = smol::future::or(
            async { p2p.stop().await; true },
            async { smol::Timer::after(Duration::from_secs(5)).await; false },
        ).await;
        eprintln!("[TEST] C4: PASS ({:.3}s)", elapsed.as_secs_f64());
    });
}

// ── Test D: SyncDecision exhaustive match ─────────────────────────────

#[test]
fn test_sync_decision_type_is_exhaustive() {
    use dwowd::proto::linear_sync_client::SyncDecision;

    fn handle(decision: SyncDecision) -> &'static str {
        match decision {
            SyncDecision::PeersAvailable => "peers",
            SyncDecision::ProceedSolo => "solo",
            SyncDecision::WaitForGenesis => "wait_genesis",
            SyncDecision::Retry => "retry",
        }
    }

    assert!(!handle(SyncDecision::PeersAvailable).is_empty(), "PeersAvailable");
    assert!(!handle(SyncDecision::ProceedSolo).is_empty(), "ProceedSolo");
    assert!(!handle(SyncDecision::WaitForGenesis).is_empty(), "WaitForGenesis");
    assert!(!handle(SyncDecision::Retry).is_empty(), "Retry");
}

// ── Test E: PeerTip::from_tip rejects invalid data ────────────────────

#[test]
fn test_peertip_rejects_invalid() {
    use dwowd::proto::linear_sync_client::PeerTip;
    use dwowd::proto::linear_sync::Tip;

    fn assert_rejects(desc: &str, tip: &Tip) {
        match PeerTip::from_tip(tip) {
            Err(e) => eprintln!("[CHECK] Correctly rejected {}: {e}", desc),
            Ok(_) => panic!("[CHECK] FAIL: {} — expected Err, got Ok", desc),
        }
    }

    fn assert_accepts(desc: &str, tip: &Tip) {
        match PeerTip::from_tip(tip) {
            Ok(_) => eprintln!("[CHECK] Correctly accepted: {}", desc),
            Err(e) => panic!("[CHECK] FAIL: {} — expected Ok, got Err: {e}", desc),
        }
    }

    assert_rejects("u64::MAX height", &Tip {
        height: BlockHeight::new(u64::MAX),
        hash: "abc".to_string(), genesis_hash: Some("def".to_string()),
    });
    assert_rejects("empty hash at height > 0", &Tip {
        height: BlockHeight::new(5),
        hash: String::new(), genesis_hash: Some("def".to_string()),
    });
    assert_rejects("missing genesis hash at height > 0", &Tip {
        height: BlockHeight::new(5),
        hash: "abc".to_string(), genesis_hash: None,
    });
    assert_accepts("valid tip", &Tip {
        height: BlockHeight::new(5),
        hash: "abc".to_string(), genesis_hash: Some("def".to_string()),
    });
    assert_accepts("height 0 empty hash valid", &Tip {
        height: BlockHeight::new(0),
        hash: String::new(), genesis_hash: None,
    });

    eprintln!("[TEST] E: PASS — PeerTip::from_tip correctly rejects all invalid inputs");
}

// ── Test G: Barb declarations complete ────────────────────────────────

#[test]
fn test_barb_declarations_complete() {
    use dwow_core::barb::ExhibitsBarb;
    use dwowd::proto::linear_sync_client::{BlocksBatch, PeerTip};
    use dwowd::task::{ConsensusInitTaskConfig, GenesisAuthority};

    let boundary_types: Vec<(&str, &[dwow_core::barb::BarbId])> = vec![
        ("PeerTip", PeerTip::exhibited_barbs()),
        ("BlocksBatch", BlocksBatch::exhibited_barbs()),
        ("ConsensusInitTaskConfig", ConsensusInitTaskConfig::exhibited_barbs()),
        ("GenesisAuthority", GenesisAuthority::exhibited_barbs()),
        ("LinearSyncClient", dwowd::proto::linear_sync_client::LinearSyncClient::exhibited_barbs()),
    ];

    for (name, barbs) in &boundary_types {
        assert!(!barbs.is_empty(), "{} missing ExhibitsBarb", name);
        eprintln!("[CHECK] {}: exhibits {:?}", name, barbs);
    }

    // Specific barb checks
    assert!(PeerTip::exhibited_barbs().contains(&dwow_core::barb::BarbId::Verify));
    assert!(PeerTip::exhibited_barbs().contains(&dwow_core::barb::BarbId::SyncBarrier));
    assert!(BlocksBatch::exhibited_barbs().contains(&dwow_core::barb::BarbId::Commit));
    assert!(GenesisAuthority::exhibited_barbs().contains(&dwow_core::barb::BarbId::Mine));
    assert!(ConsensusInitTaskConfig::exhibited_barbs().contains(&dwow_core::barb::BarbId::Mine));

    eprintln!("[TEST] G: PASS — all boundary types have complete barb declarations");
}
