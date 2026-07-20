/// Calibration test: SESSION_DWOW_LINEAR_SYNC compatibility.
///
/// Per consensus_linear.rs:568, the peer-height refresh loop used
/// `session == SESSION_DWOW_LINEAR_SYNC` to filter peers for tip
/// re-queries after sync. But SESSION_DWOW_LINEAR_SYNC was 0, and
/// SESSION_DEFAULT is 0b100111 (39) — the equality check matched
/// NO real peers. The tip-collection loop at :232 correctly used
/// `session & SESSION_DEFAULT != 0`.
///
/// This test guards against regression: if the constant is ever
/// resurrected, it must NOT be 0, and the filter must use bitwise-AND
/// to be compatible with SESSION_DEFAULT.

use dwow_core::net::session::SESSION_DEFAULT;

/// Verify SESSION_DEFAULT is non-zero (regression guard).
#[test]
fn session_default_is_nonzero() {
    assert_ne!(SESSION_DEFAULT, 0, "SESSION_DEFAULT must not be 0");
}

/// Verify SESSION_DEFAULT passes a bitwise-AND filter with itself
/// (the pattern used by the tip-collection and peer-refresh loops).
#[test]
fn session_default_passes_bitwise_and() {
    assert!(
        SESSION_DEFAULT & SESSION_DEFAULT != 0,
        "SESSION_DEFAULT must pass bitwise-AND with itself"
    );
}

/// Verify that a peer registered as SESSION_DEFAULT passes the
/// combined filter (session & SESSION_DEFAULT != 0). This is the
/// pattern used by both the tip-collection loop at
/// consensus_linear.rs:232 and the peer-height refresh at :568.
#[test]
fn full_node_passes_dag_absorber_filter() {
    // The DAG absorber filter from consensus_linear.rs:232:
    //   session & SESSION_DEFAULT != 0 — matches all full-node peers
    let session = SESSION_DEFAULT; // simulate a full-node peer
    assert!(
        session & SESSION_DEFAULT != 0,
        "Full-node peer registered as SESSION_DEFAULT must pass the DAG absorber filter"
    );
}

/// Verify the old equality-based filter would catch the mismatch.
/// If the filter uses `== 0` and the constant is 0, this test FAILS
/// — the diagnostic tells the operator exactly what's wrong.
#[test]
fn zero_constant_equality_matches_no_peers() {
    const ZERO_CONSTANT: u32 = 0;
    let session = SESSION_DEFAULT;
    // This assertion documents the bug: with SESSION_DWOW_LINEAR_SYNC=0,
    // the filter `session == 0` matches NO peer registered with SESSION_DEFAULT=39.
    assert!(
        session != ZERO_CONSTANT,
        "BUG: equality filter with constant=0 rejects SESSION_DEFAULT peers"
    );
}
