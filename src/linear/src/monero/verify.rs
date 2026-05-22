//! Lightweight Monero anchor plausibility checks.
//!
//! Phase 4a implements basic sanity checks without monerod RPC dependency.
//! Phase 4b adds full monerod RPC verification when `monerod_url` is configured.
//! When no URL is set, the function falls back to Phase 4a behavior
//! (accept any non-zero hash).

use crate::monero::rpc;

/// Maximum plausible Monero block height (well beyond current chain tip).
/// Monero launched April 2014 at ~1 block/2min, so by 2030: ~4.2M blocks.
const MAX_PLAUSIBLE_MONERO_HEIGHT: u64 = 5_000_000;

/// Error type for Monero anchor verification.
#[derive(Debug, thiserror::Error)]
pub enum MoneroVerifyError {
    #[error("No Monero anchor (height=0)")]
    NoAnchor,
    #[error("Monero height implausible: {0} exceeds max {1}")]
    HeightImplausible(u64, u64),
    #[error("Monero hash mismatch: expected {0}, got {1}")]
    HashMismatch(String, String),
    #[error("Monero block not found at height {0}")]
    BlockNotFound(u64),
    #[error("Insufficient Monero confirmations: need {required}, have {current}")]
    InsufficientConfirmations { current: u64, required: u64 },
}

/// Verify Monero anchor plausibility with optional full monerod RPC verification.
///
/// Always runs Phase 4a lightweight checks (height=0, implausible height).
/// When `monerod_url` is `Some`, additionally queries monerod to:
/// - Fetch the block hash at the claimed height and verify it matches
/// - Check that the block has at least `min_confirmations` confirmations
///
/// When `monerod_url` is `None`, falls back to Phase 4a behavior
/// (accept any non-zero hash without full verification).
pub fn verify_monero_anchor(
    height: u64,
    hash: &[u8; 32],
    _timestamp: u64,
    monerod_url: Option<&str>,
    min_confirmations: u32,
) -> Result<(), MoneroVerifyError> {
    if height == 0 {
        return Err(MoneroVerifyError::NoAnchor);
    }
    if height > MAX_PLAUSIBLE_MONERO_HEIGHT {
        return Err(MoneroVerifyError::HeightImplausible(height, MAX_PLAUSIBLE_MONERO_HEIGHT));
    }

    let Some(url) = monerod_url else {
        return Ok(());
    };

    // Query monerod for the block at the claimed height
    let (_, monero_hash) = rpc::get_block_by_height(url, height)
        .map_err(|e| match e {
            rpc::MonerodError::BlockNotFound(_) => MoneroVerifyError::BlockNotFound(height),
            _ => MoneroVerifyError::BlockNotFound(height),
        })?;

    // Verify the hash matches
    if monero_hash != *hash {
        return Err(MoneroVerifyError::HashMismatch(
            hex::encode(hash),
            hex::encode(monero_hash),
        ));
    }

    // Check confirmation depth
    let tip = rpc::get_block_count(url).map_err(|_| {
        MoneroVerifyError::BlockNotFound(height)
    })?;
    let required = height.saturating_add(min_confirmations as u64).saturating_sub(1);
    if tip < required {
        return Err(MoneroVerifyError::InsufficientConfirmations {
            current: tip,
            required,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_no_url() {
        let hash = [0xAB; 32];
        assert!(verify_monero_anchor(3_000_000, &hash, 0, None, 3).is_ok());
    }

    #[test]
    fn test_zero_height() {
        let hash = [0xAB; 32];
        let err = verify_monero_anchor(0, &hash, 0, None, 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::NoAnchor));
    }

    #[test]
    fn test_implausible_height() {
        let hash = [0xAB; 32];
        let err = verify_monero_anchor(10_000_000, &hash, 0, None, 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::HeightImplausible(..)));
    }

    #[test]
    fn test_zero_hash_ok_without_url() {
        // Phase 4a accepts zero hash — full verification is deferred
        assert!(verify_monero_anchor(3_000_000, &[0u8; 32], 0, None, 3).is_ok());
    }

    #[test]
    fn test_boundary_heights() {
        let hash = [0xAB; 32];
        assert!(verify_monero_anchor(MAX_PLAUSIBLE_MONERO_HEIGHT, &hash, 0, None, 3).is_ok());
        assert!(verify_monero_anchor(MAX_PLAUSIBLE_MONERO_HEIGHT + 1, &hash, 0, None, 3).is_err());
    }

    // --- Phase 4b tests: monerod RPC verification with mock HTTP server ---

    #[test]
    fn test_success_with_url() {
        let hash_hex = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let hash_bytes: [u8; 32] = hex::decode(hash_hex).unwrap().try_into().unwrap();
        let get_block = Box::leak(
            format!(
                r#"{{"result":{{"block_header":{{"hash":"{}","height":3000000}}}}}}"#,
                hash_hex
            )
            .into_boxed_str(),
        );
        let get_count = Box::leak(
            "{\"result\":{\"count\":3000005}}"
                .to_string()
                .into_boxed_str(),
        );

        let url = crate::monero::rpc::test_helpers::serve_sequence(vec![get_block, get_count]);
        assert!(verify_monero_anchor(3000000, &hash_bytes, 0, Some(&url), 3).is_ok());
    }

    #[test]
    fn test_hash_mismatch_with_url() {
        let claimed_hash: [u8; 32] =
            hex::decode("1111111111111111111111111111111111111111111111111111111111111111")
                .unwrap()
                .try_into()
                .unwrap();
        let response = Box::leak(
            r#"{"result":{"block_header":{"hash":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789","height":3000000}}}"#
                .to_string()
                .into_boxed_str(),
        );

        let url = crate::monero::rpc::test_helpers::serve_once(response);
        let err =
            verify_monero_anchor(3000000, &claimed_hash, 0, Some(&url), 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::HashMismatch(..)));
    }

    #[test]
    fn test_insufficient_confirmations_with_url() {
        let hash_hex = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let hash_bytes: [u8; 32] = hex::decode(hash_hex).unwrap().try_into().unwrap();
        let get_block = Box::leak(
            format!(
                r#"{{"result":{{"block_header":{{"hash":"{}","height":3000000}}}}}}"#,
                hash_hex
            )
            .into_boxed_str(),
        );
        let get_count = Box::leak(
            "{\"result\":{\"count\":3000001}}"
                .to_string()
                .into_boxed_str(),
        );

        let url = crate::monero::rpc::test_helpers::serve_sequence(vec![get_block, get_count]);
        let err =
            verify_monero_anchor(3000000, &hash_bytes, 0, Some(&url), 3).unwrap_err();
        assert!(matches!(
            err,
            MoneroVerifyError::InsufficientConfirmations { .. }
        ));
    }

    #[test]
    fn test_block_not_found_with_url() {
        let hash_bytes = [0xAB; 32];
        let response = Box::leak(
            r#"{"result":{"block_header":{"hash":"","height":3000000}}}"#
                .to_string()
                .into_boxed_str(),
        );

        let url = crate::monero::rpc::test_helpers::serve_once(response);
        let err =
            verify_monero_anchor(3000000, &hash_bytes, 0, Some(&url), 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::BlockNotFound(3000000)));
    }

    #[test]
    fn test_connection_error_with_url() {
        let hash_bytes = [0xAB; 32];
        let url = "http://127.0.0.1:19999/json_rpc";
        let err =
            verify_monero_anchor(3000000, &hash_bytes, 0, Some(url), 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::BlockNotFound(_)));
    }
}
