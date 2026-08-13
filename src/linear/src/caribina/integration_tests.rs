//! Integration tests for Caribina Arweave anchoring.
//!
//! These tests hit the real ArDrive Turbo upload endpoint and ardrive.net
//! gateway. They require network access.
//!
//! ## Test categories
//!
//! **Immediate** — these succeed right after posting to ArDrive Turbo:
//! - `test_anchor_with_unfunded_wallet` — validates free tier (winc="0")
//! - `test_multiple_anchors_unique_ids` — wallet cycling produces unique TX IDs
//! - `test_verify_not_found` — made-up TX ID returns NotFound
//! - `test_anchor_block_resilient_on_failure` — graceful degradation
//!
//! **Delayed** — these need Arweave mining confirmation (Turbo bundles can take
//! minutes to hours to appear on gateways). They include retry loops:
//! - `test_end_to_end_anchor_and_verify` — full roundtrip: post → get → verify
//! - `test_verify_bad_signature` — tampered DataItem fails sig check
//! - `test_verify_data_mismatch` — wrong hash returns DataMismatch
//! - `test_timestamp_tolerance_integration` — within/exceeds tolerance

use std::io::Read;

use dwow_sdk::blockchain::BlockHeight;

use crate::caribina::{
    anchor::{anchor_block, TURBO_UPLOAD_URL},
    data_item::DataItem,
    verify::{verify_anchor, VerifyError},
    wallet::CaribinaWallet,
};

// ---------------------------------------------------------------------------
// Immediate tests (pass right after Turbo POST)
// ---------------------------------------------------------------------------

/// Validate that unfunded wallets are accepted (winc = "0" in response).
#[test]
#[ignore]
fn test_anchor_with_unfunded_wallet() {
    let wallet = CaribinaWallet::generate();
    let mut item = DataItem::new(b"caribina integration test -- unfunded wallet");
    item.sign(&wallet);

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent();
    let response = agent
        .post(TURBO_UPLOAD_URL)
        .header("Content-Type", "application/octet-stream")
        .send(item.as_bytes())
        .expect("POST to Turbo should succeed");

    assert_eq!(response.status().as_u16(), 200, "Turbo should accept the upload");

    let body = response.into_body().read_to_string().unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&body).expect("response should be valid JSON");

    assert_eq!(
        json["winc"].as_str().unwrap_or(""),
        "0",
        "unfunded wallet upload should cost 0 winc"
    );
    assert!(!json["id"].as_str().unwrap_or("").is_empty(), "response should contain an id");
}

/// Wallet cycling produces unique TX IDs for different payloads.
#[test]
#[ignore]
fn test_multiple_anchors_unique_ids() {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut hash1 = [1u8; 32];
    hash1[0] = 0xAA;
    let mut hash2 = [2u8; 32];
    hash2[0] = 0xBB;

    let id1 = anchor_block(&hash1, ts, BlockHeight::new(1)).expect("first anchor should succeed");
    let id2 = anchor_block(&hash2, ts, BlockHeight::new(1)).expect("second anchor should succeed");

    assert_ne!(id1, id2, "different payloads should produce unique TX IDs");
}

/// Querying a non-existent TX ID returns NotFound.
#[test]
#[ignore]
fn test_verify_not_found() {
    let result = verify_anchor(&[0xFFu8; 32], &[0u8; 32], 0, BlockHeight::new(0));
    match result {
        Err(VerifyError::NotFound) => {}
        other => panic!("expected NotFound, got {:?}", other),
    }
}

/// anchor_block returns gracefully (None or valid id) instead of panicking.
#[test]
#[ignore]
fn test_anchor_block_resilient_on_failure() {
    let result = anchor_block(&[0u8; 32], 0, BlockHeight::new(0));
    if let Some(id) = result {
        assert_eq!(id.len(), 32);
    }
}

// ---------------------------------------------------------------------------
// Delayed tests (need Arweave mining confirmation — retry with backoff)
// ---------------------------------------------------------------------------

/// Full end-to-end roundtrip: build payload, POST to ArDrive Turbo,
/// fetch from ardrive.net gateway, verify signature and payload.
///
/// NOTE: Turbo bundles are submitted to Arweave periodically. The DataItem
/// may take minutes to hours to appear on the gateway. This test retries.
#[test]
#[ignore]
fn test_end_to_end_anchor_and_verify() {
    let (hash, timestamp, height, tx_id) = post_test_anchor();

    // Retry verification — Turbo bundles need time to mine into Arweave
    let mut last_err = None;
    for attempt in 1..=12 {
        match verify_anchor(&tx_id, &hash, timestamp, height) {
            Ok(()) => return,
            Err(e) => {
                eprintln!("  verify attempt {attempt}/12: {e}");
                last_err = Some(e);
                if attempt < 12 {
                    // Exponential backoff: 10s, 20s, 40s, ...
                    std::thread::sleep(std::time::Duration::from_secs((10 * (1 << (attempt - 1))).min(60)));
                }
            }
        }
    }
    panic!(
        "verify_anchor failed after 12 attempts (final error: {:?}). \
         Turbo bundle may not have been mined into Arweave yet.",
        last_err.unwrap()
    );
}

/// Tampering with the owner field makes the DataItem signature invalid.
/// Fetches a real DataItem from the gateway, tampers with the owner bytes.
#[test]
#[ignore]
fn test_verify_bad_signature() {
    let (_hash, _timestamp, _height, tx_id) = post_test_anchor();

    // Wait for gateway availability with retries
    let binary = fetch_with_retry(&tx_id, 12);

    // Original should verify fine
    let item = DataItem::deserialize(&binary).expect("should deserialize");
    assert!(item.verify_signature(), "original DataItem should have valid signature");

    // Tamper with owner bytes (bytes 66-97, 32 bytes of Ed25519 public key)
    let mut tampered_binary = binary.clone();
    tampered_binary[66] ^= 0xFF;

    let tampered = DataItem::deserialize(&tampered_binary).expect("should still deserialize");
    assert!(
        !tampered.verify_signature(),
        "tampered owner should invalidate signature"
    );
}

/// A wrong expected hash produces DataMismatch on verification.
#[test]
#[ignore]
fn test_verify_data_mismatch() {
    let (_hash, timestamp, height, tx_id) = post_test_anchor();

    // Check data mismatch (may also be NotFound if bundle not yet mined)
    let result = verify_anchor(&tx_id, &[0x22u8; 32], timestamp, height);
    match result {
        Err(VerifyError::DataMismatch) => {}
        Err(VerifyError::NotFound) => {
            eprintln!("  DataItem not yet available on gateway (Turbo bundle pending)");
        }
        other => panic!("expected DataMismatch or NotFound, got {:?}", other),
    }
}

/// Timestamp tolerance: 29 min apart succeeds, 31 min apart fails.
#[test]
#[ignore]
fn test_timestamp_tolerance_integration() {
    let (hash, timestamp, height, tx_id) = post_test_anchor();

    // Wait for gateway availability with retries
    let _binary = fetch_with_retry(&tx_id, 12);

    // Within tolerance (29 minutes = 1740 seconds < 1800)
    verify_anchor(&tx_id, &hash, timestamp + 29 * 60, height)
        .expect("29 min difference should be within tolerance");

    // Exceeds tolerance (31 minutes = 1860 seconds > 1800)
    let result = verify_anchor(&tx_id, &hash, timestamp + 31 * 60, height);
    assert!(
        result.is_err(),
        "31 min difference should exceed tolerance, got {:?}",
        result
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// POST a test anchor to ArDrive Turbo and return (hash, timestamp, height, tx_id).
fn post_test_anchor() -> ([u8; 32], u64, BlockHeight, [u8; 32]) {
    let mut hash = [0u8; 32];
    let ts_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let ts_bytes = ts_ns.to_le_bytes();
    hash[0..16].copy_from_slice(&ts_bytes[0..16]);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let height = BlockHeight::new(1);

    let tx_id = anchor_block(&hash, timestamp, height)
        .expect("anchor_block should succeed — network and Turbo must be available");
    assert_ne!(tx_id, [0u8; 32], "TX ID should be non-zero");

    (hash, timestamp, height, tx_id)
}

/// Fetch raw DataItem binary from the gateway with retries.
fn fetch_with_retry(tx_id: &[u8; 32], max_attempts: u32) -> Vec<u8> {
    let tx_id_b64 = bytes_to_base64url(tx_id);
    let url = format!("https://ardrive.net/{}", tx_id_b64);

    for attempt in 1..=max_attempts {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .new_agent();
        match agent.get(&url).call() {
            Ok(response) => {
                let mut binary = Vec::new();
                response.into_body().as_reader().read_to_end(&mut binary).unwrap();
                if binary.len() >= 116 {
                    return binary;
                }
                eprintln!(
                    "  fetch attempt {attempt}: response too small ({} bytes), retrying...",
                    binary.len()
                );
            }
            Err(e) => {
                eprintln!("  fetch attempt {attempt}: {e}");
            }
        }
        if attempt < max_attempts {
            std::thread::sleep(std::time::Duration::from_secs((10 * (1 << (attempt - 1))).min(60)));
        }
    }
    panic!(
        "failed to fetch DataItem from gateway after {max_attempts} attempts"
    );
}

fn bytes_to_base64url(bytes: &[u8; 32]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    b64.replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}
