//! Lightweight Monero anchor plausibility checks.
//!
//! Phase 4a implements basic sanity checks without monerod RPC dependency.
//! Phase 4b adds full monerod RPC verification when `monerod_url` is configured.
//! When no URL is set, the function falls back to Phase 4a behavior
//! (accept any non-zero hash).

use crate::monero::rpc;
use crate::monero::MoneroPowData;

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
    #[error("Monerod could not confirm the Monero block: {0}")]
    NotConfirmed(String),
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

/// Node-local admission policy for a **merge-mined** block (`OBL-C67`).
///
/// Why it exists. The three merge-mining receipts prove that our aux hash sits in a coinbase of
/// *some* serialized Monero block — `is_coinbase_valid_merkle_root` recomputes the coinbase hash from
/// fields the submitter supplied and compares it to a root the submitter also supplied — so on their
/// own they admit a block that was fabricated rather than mined. This asks the only party that can
/// settle it, a Monero node, whether the block the proof describes exists on the Monero chain.
///
/// It is **node-local, never consensus**: the answer depends on another chain and on the network, so
/// it cannot be a pure function of local data — the same reason `verify_monero_anchor` above is
/// uncallable from consensus (`OBL-C66`). It is called from `block_acceptor`, which is the node's own
/// admission path, and never from `check_pow_stage`/`check_block_header`/`connect_block`.
///
/// The block's Monero identity is the **hash derived from its own proof**, not a height: a merge-mined
/// block commits no Monero height (production writes `anchor_monero_height` zero, and the Monero
/// header carries no height to derive), so the lookup is by that hash and the depth is measured from
/// the height monerod reports for it. That is the stronger direction, since the lookup key comes from
/// the proof rather than from the submitter.
///
/// `monerod_url` is `None` by default, and then this returns `Ok(())` **having checked nothing**. The
/// caller must say so in its log — `block_acceptor` does — because a residual gap stated in a log is
/// honest and one implied away is not.
///
/// **Every failure to confirm is a rejection**: a transport error, a timeout, an unknown hash, too
/// few confirmations. That asymmetry is the register's decision ("admits … only if monerod confirms")
/// and not an accident, so it is stated rather than softened: a node whose operator set `monerod_url`
/// and whose monerod is unreachable stops admitting merge-mined blocks, which is the cost of the
/// option being on.
pub fn verify_monero_powdata(
    powdata: &MoneroPowData,
    monerod_url: Option<&str>,
    min_confirmations: u32,
) -> Result<(), MoneroVerifyError> {
    let Some(url) = monerod_url else {
        return Ok(());
    };

    let derived = powdata.block_hash();

    let (height, reported) = rpc::get_block_by_hash(url, &derived)
        .map_err(|e| MoneroVerifyError::NotConfirmed(format!("{e:?}")))?;

    // Monerod echoes the block's hash; a DIFFERENT one means the answer is not about the block this
    // proof describes — the query and the answer must agree before the answer means anything.
    if reported != derived {
        return Err(MoneroVerifyError::HashMismatch(
            hex::encode(derived),
            hex::encode(reported),
        ));
    }

    let tip = rpc::get_block_count(url).map_err(|e| MoneroVerifyError::NotConfirmed(format!("{e:?}")))?;

    // `get_block_count` returns the number of blocks — the tip's height plus one — so a block at
    // `height` is `tip - height` blocks deep and has `tip - height + 1` confirmations including
    // itself. `verify_monero_anchor` above expresses the same threshold as
    // `tip >= height + min_confirmations - 1`; this states it as a count, which is what the error
    // reports, and the two agree at every boundary.
    let confirmations = tip.saturating_sub(height).saturating_add(1);
    if confirmations < u64::from(min_confirmations) {
        return Err(MoneroVerifyError::InsufficientConfirmations {
            current: confirmations,
            required: u64::from(min_confirmations),
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

    // --- verify_monero_powdata: the merge-mining admission policy (OBL-C67) ----------------------
    //
    // These drive the policy against a stub monerod, which is why they live in this crate: the stub
    // helpers are `pub(crate)` here and do not exist in a non-test build, so `bin/dwowd` cannot reach
    // them. The fixture is the real testnet merge-mined block, so the hash the policy derives is a
    // genuine block id rather than a canned constant.

    /// The stub takes `&'static str`, so a body containing a runtime-derived hash must be leaked —
    /// the same shape the Phase 4b tests above use.
    fn body_with_hash(hash: [u8; 32], height: u64) -> &'static str {
        Box::leak(
            format!(
                r#"{{"result":{{"block_header":{{"hash":"{}","height":{}}}}}}}"#,
                hex::encode(hash),
                height
            )
            .into_boxed_str(),
        )
    }

    fn body_with_count(count: u64) -> &'static str {
        Box::leak(format!(r#"{{"result":{{"count":{count}}}}}"#).into_boxed_str())
    }

    #[test]
    fn test_powdata_confirmed_by_monerod() {
        // Positive control: monerod knows the block the proof describes, and the tip is deep enough.
        let powdata = crate::monero::tests::real_block_powdata();
        let derived = powdata.block_hash();
        let url = crate::monero::rpc::test_helpers::serve_sequence(vec![
            body_with_hash(derived, 2_900_000),
            body_with_count(2_900_005),
        ]);
        let result = verify_monero_powdata(&powdata, Some(&url), 3);
        assert!(result.is_ok(), "a real merge-mined block must be admitted: {result:?}");
    }

    #[test]
    fn test_powdata_rejected_when_monerod_does_not_know_the_block() {
        // THE NEGATIVE CONTROL THIS ROW LACKED. A fabricated block is one whose derived hash no
        // Monero node knows; monerod answers with an error object, and the policy must reject rather
        // than treat the unparseable reply as a pass.
        let powdata = crate::monero::tests::real_block_powdata();
        let url = crate::monero::rpc::test_helpers::serve_once(
            r#"{"error":{"code":-5,"message":"Failed to get block header"}}"#,
        );
        let err = verify_monero_powdata(&powdata, Some(&url), 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::NotConfirmed(_)), "got {err:?}");
    }

    #[test]
    fn test_powdata_rejected_when_monerod_reports_a_different_hash() {
        // The query and the answer must be about the same block; a node that answers about another
        // one is not confirmation of this one, whatever else it proves.
        let powdata = crate::monero::tests::real_block_powdata();
        let url = crate::monero::rpc::test_helpers::serve_once(body_with_hash([0xAB; 32], 2_900_000));
        let err = verify_monero_powdata(&powdata, Some(&url), 3).unwrap_err();
        assert!(matches!(err, MoneroVerifyError::HashMismatch(..)), "got {err:?}");
    }

    #[test]
    fn test_powdata_rejected_below_the_confirmation_depth() {
        let powdata = crate::monero::tests::real_block_powdata();
        let derived = powdata.block_hash();
        // Same block, one confirmation short of the three required (tip is the block itself).
        let url = crate::monero::rpc::test_helpers::serve_sequence(vec![
            body_with_hash(derived, 2_900_000),
            body_with_count(2_900_001),
        ]);
        let err = verify_monero_powdata(&powdata, Some(&url), 3).unwrap_err();
        match err {
            MoneroVerifyError::InsufficientConfirmations { current, required } => {
                assert_eq!(current, 2, "tip-height+1 for a block one below the tip");
                assert_eq!(required, 3);
            }
            other => panic!("expected a depth rejection, got {other:?}"),
        }
    }

    #[test]
    fn test_powdata_unreachable_monerod_is_a_rejection() {
        // Every failure to confirm is a rejection, transport included — the register's decision, and
        // the cost of the option being on. Stated as a test so it cannot be softened by accident.
        let powdata = crate::monero::tests::real_block_powdata();
        let err = verify_monero_powdata(&powdata, Some("http://127.0.0.1:19999/json_rpc"), 3)
            .unwrap_err();
        assert!(matches!(err, MoneroVerifyError::NotConfirmed(_)), "got {err:?}");
    }

    #[test]
    fn test_powdata_unchecked_without_a_monerod_url() {
        // The default configuration: nothing is consulted and the block is admitted as before. This
        // is the path that must stay byte-identical to the pre-policy behaviour, and the caller says
        // in its log that it checked nothing (see `block_acceptor`'s call site).
        let powdata = crate::monero::tests::real_block_powdata();
        assert!(verify_monero_powdata(&powdata, None, 3).is_ok());
    }
}
