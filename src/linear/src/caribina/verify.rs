//! Verify a Caribina anchor by fetching from an Arweave gateway.
//!
//! Fetches the DataItem by ID from an Arweave gateway, verifies the
//! Ed25519 signature, and checks that the embedded data matches the
//! expected block hash, timestamp, and height.

use std::io::Read;

use dwow_sdk::blockchain::BlockHeight;

use super::data_item::DataItem;

/// Default Arweave gateway for verification.
pub const ARWEAVE_GATEWAY: &str = "https://ardrive.net";

/// Tolerance window for timestamp verification (minutes).
/// The Arweave block timestamp should be within this window of the
/// DarkWow block timestamp.
pub const TIMESTAMP_TOLERANCE_MINUTES: i64 = 30;

/// Error type for anchor verification.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("HTTP fetch failed: {0}")]
    Http(String),
    #[error("DataItem not found on Arweave")]
    NotFound,
    #[error("Invalid DataItem format: {0}")]
    InvalidFormat(String),
    #[error("Signature verification failed")]
    BadSignature,
    #[error("Embedded data mismatch")]
    DataMismatch,
}

/// Verify a Caribina anchor.
///
/// Fetches the DataItem from an Arweave gateway by its ID, verifies the
/// Ed25519 signature, and checks that the embedded payload matches the
/// expected block_hash, timestamp, and height.
pub fn verify_anchor(
    tx_id: &[u8; 32],
    expected_hash: &[u8; 32],
    expected_timestamp: u64,
    expected_height: BlockHeight,
) -> Result<(), VerifyError> {
    // Fetch from Arweave gateway
    let tx_id_b64 = bytes_to_base64url(tx_id);
    let url = format!("{}/{}", ARWEAVE_GATEWAY, tx_id_b64);

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent();
    let response = match agent.get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(404)) => return Err(VerifyError::NotFound),
        Err(e) => return Err(VerifyError::Http(e.to_string())),
    };

    let mut binary = Vec::new();
    response
        .into_body()
        .as_reader()
        .read_to_end(&mut binary)
        .map_err(|e| VerifyError::Http(e.to_string()))?;

    // Deserialize DataItem
    let item = DataItem::deserialize(&binary)
        .ok_or_else(|| VerifyError::InvalidFormat("too small or wrong signature type".into()))?;

    // Verify Ed25519 signature
    if !item.verify_signature() {
        return Err(VerifyError::BadSignature);
    }

    // Verify embedded data matches expected block
    let raw = item.raw_data();
    verify_payload(raw, expected_hash, expected_timestamp, expected_height)
}

/// Verify that the embedded payload matches the expected block.
fn verify_payload(
    payload: &[u8],
    expected_hash: &[u8; 32],
    expected_timestamp: u64,
    expected_height: BlockHeight,
) -> Result<(), VerifyError> {
    if payload.len() < 48 {
        return Err(VerifyError::DataMismatch);
    }

    let stored_hash = &payload[0..32];
    let stored_timestamp = u64::from_le_bytes([
        payload[32], payload[33], payload[34], payload[35],
        payload[36], payload[37], payload[38], payload[39],
    ]);
    let stored_height = BlockHeight::from_le_bytes([
        payload[40], payload[41], payload[42], payload[43],
        payload[44], payload[45], payload[46], payload[47],
    ]);

    if stored_hash != expected_hash {
        return Err(VerifyError::DataMismatch);
    }
    if stored_height != expected_height {
        return Err(VerifyError::DataMismatch);
    }

    // Timestamp tolerance check
    let ts_diff = (stored_timestamp as i64 - expected_timestamp as i64).abs();
    let tolerance_secs = TIMESTAMP_TOLERANCE_MINUTES * 60;
    if ts_diff > tolerance_secs as i64 {
        return Err(VerifyError::DataMismatch);
    }

    Ok(())
}

/// Encode a 32-byte array to base64url (no padding).
fn bytes_to_base64url(bytes: &[u8; 32]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    b64.replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_verify_success() {
        let hash = [1u8; 32];
        let ts: u64 = 1000;
        let height = BlockHeight::new(42);
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash);
        payload.extend_from_slice(&ts.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        assert!(verify_payload(&payload, &hash, ts, height).is_ok());
    }

    #[test]
    fn test_payload_verify_hash_mismatch() {
        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];
        let ts: u64 = 1000;
        let height = BlockHeight::new(42);
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash1);
        payload.extend_from_slice(&ts.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        assert!(verify_payload(&payload, &hash2, ts, height).is_err());
    }

    #[test]
    fn test_payload_verify_too_short() {
        assert!(verify_payload(b"short", &[0u8; 32], 0, BlockHeight::new(0)).is_err());
    }

    #[test]
    fn test_base64url_roundtrip() {
        let id = [0xABu8; 32];
        let encoded = bytes_to_base64url(&id);
        // Should be base64url, no padding
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.ends_with('='));
    }

    #[test]
    fn test_payload_verify_height_mismatch() {
        let hash = [1u8; 32];
        let ts: u64 = 1000;
        let height = BlockHeight::new(42);
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash);
        payload.extend_from_slice(&ts.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        // Wrong expected height
        assert!(verify_payload(&payload, &hash, ts, BlockHeight::new(99)).is_err());
    }

    #[test]
    fn test_payload_verify_timestamp_within_tolerance() {
        let hash = [1u8; 32];
        let ts: u64 = 1000;
        let height = BlockHeight::new(42);
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash);
        // 29 minutes later (1740 seconds) — within 30 min tolerance
        let stored_ts = ts + 29 * 60;
        payload.extend_from_slice(&stored_ts.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        assert!(verify_payload(&payload, &hash, ts, height).is_ok());
    }

    #[test]
    fn test_payload_verify_timestamp_exceeds_tolerance() {
        let hash = [1u8; 32];
        let ts: u64 = 1000;
        let height = BlockHeight::new(42);
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash);
        // 31 minutes later (1860 seconds) — exceeds 30 min tolerance
        let stored_ts = ts + 31 * 60;
        payload.extend_from_slice(&stored_ts.to_le_bytes());
        payload.extend_from_slice(&height.to_le_bytes());
        assert!(verify_payload(&payload, &hash, ts, height).is_err());
    }

    #[test]
    fn test_base64url_encoding_known_vector() {
        // All zeros should produce a predictable base64url output
        let id = [0u8; 32];
        let encoded = bytes_to_base64url(&id);
        // 32 zero bytes → 43 base64 'A' chars (no padding in base64url)
        assert_eq!(encoded.len(), 43);
        assert!(encoded.chars().all(|c| c == 'A'));
    }
}
