/// Block format translation between DarkWow headers and Monero-compatible blobs.
///
/// p2pool expects a Monero-format block template (with `blocktemplate_blob` and
/// `blockhashing_blob` fields). This module serializes DarkWow block headers into
/// a fixed-size byte array that miners can hash. The `blockhashing_blob` is exactly
/// the serialized DarkWow header with the nonce zeroed — so miners are hashing
/// valid DarkWow block data.
///
/// DarkWow linear header layout (from dwow_linear::BlockHeader::to_mining_blob):
/// ```text
/// Offset  Size  Field
/// 0       32    previous (blake3::Hash)
/// 32      8     height (u64, LE)
/// 40      4     nonce (u32, LE)  ← miners modify this
/// 44      4     difficulty_target (u32, LE)
/// 48      1     version (u8)
/// 49      32    merkle_root (blake3::Hash)
/// 81      8     timestamp (u64, LE)
/// 89      32    uncle_merkle_root ([u8; 32])
/// 121     8     total_reward (u64, LE)
/// 129     32    randomx_key ([u8; 32])
/// 161     32    coin_merkle_root ([u8; 32])
/// 193     32    nullifier_root ([u8; 32])
/// ─────────────────────────────────────────
/// 225     TOTAL
/// ```

use dwow_linear::BlockHeader;

/// Fixed size of the serialized DarkWow block header (225 bytes).
pub const HEADER_SERIALIZED_SIZE: usize = 225;

/// Offset of the nonce field in the serialized header.
/// Must match `BlockHeader::NONCE_OFFSET` (src/linear/src/block.rs).
pub const NONCE_OFFSET: usize = 40;

/// Serialize a DarkWow BlockHeader into a fixed-size byte array.
/// Layout matches `BlockHeader::to_mining_blob()` (src/linear/src/block.rs:138).
pub fn serialize_header(header: &BlockHeader) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SERIALIZED_SIZE);

    buf.extend_from_slice(header.previous.as_bytes());              // 0..32
    buf.extend_from_slice(&header.height.to_le_bytes());            // 32..40
    buf.extend_from_slice(&header.nonce.to_le_bytes());              // 40..44 (nonce)
    buf.extend_from_slice(&header.difficulty_target.to_le_bytes()); // 44..48
    buf.push(header.version);                                        // 48
    buf.extend_from_slice(header.merkle_root.as_bytes());            // 49..81
    buf.extend_from_slice(&header.timestamp.to_le_bytes());          // 81..89
    buf.extend_from_slice(&header.uncle_merkle_root);                // 89..121
    buf.extend_from_slice(&header.total_reward.to_le_bytes());       // 121..129
    buf.extend_from_slice(&header.randomx_key);                      // 129..161
    buf.extend_from_slice(&header.coin_merkle_root);                 // 161..193
    buf.extend_from_slice(&header.nullifier_root);                   // 193..225

    debug_assert_eq!(buf.len(), HEADER_SERIALIZED_SIZE);

    buf
}

/// Deserialize a byte array back into a DarkWow BlockHeader.
/// Layout must match `BlockHeader::to_mining_blob()`.
pub fn deserialize_header(data: &[u8]) -> Option<BlockHeader> {
    if data.len() < HEADER_SERIALIZED_SIZE {
        return None;
    }

    let previous = blake3::Hash::from_bytes(data[0..32].try_into().ok()?);
    let height = u64::from_le_bytes(data[32..40].try_into().ok()?);
    let nonce = u32::from_le_bytes(data[40..44].try_into().ok()?);
    let difficulty_target = u32::from_le_bytes(data[44..48].try_into().ok()?);
    let version = data[48];
    let merkle_root = blake3::Hash::from_bytes(data[49..81].try_into().ok()?);
    let timestamp = u64::from_le_bytes(data[81..89].try_into().ok()?);
    let uncle_merkle_root: [u8; 32] = data[89..121].try_into().ok()?;
    let total_reward = u64::from_le_bytes(data[121..129].try_into().ok()?);
    let randomx_key: [u8; 32] = data[129..161].try_into().ok()?;
    let coin_merkle_root: [u8; 32] = data[161..193].try_into().ok()?;
    let nullifier_root: [u8; 32] = data[193..225].try_into().ok()?;

    Some(BlockHeader {
        version,
        previous,
        merkle_root,
        timestamp,
        difficulty_target,
        nonce,
        height,
        uncle_merkle_root,
        total_reward,
        randomx_key,
        coin_merkle_root,
        nullifier_root,
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: 0,
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
    })
}

/// Build a Monero-compatible block template from a DarkWow header blob.
///
/// Returns a JSON object with the fields p2pool expects from `get_block_template`.
pub fn build_template_response(
    blob_hex: &str,
    height: u64,
    difficulty: u64,
    prev_hash: &str,
    reserved_offset: usize,
) -> serde_json::Value {
    serde_json::json!({
        "blocktemplate_blob": blob_hex,
        "blockhashing_blob": blob_hex,
        "difficulty": difficulty,
        "expected_reward": 0,
        "height": height,
        "prev_hash": prev_hash,
        "reserved_offset": reserved_offset,
        "status": "OK",
        "untrusted": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> BlockHeader {
        BlockHeader {
            version: 1,
            previous: blake3::Hash::from_bytes([0xAA; 32]),
            merkle_root: blake3::Hash::from_bytes([0xBB; 32]),
            timestamp: 1234567890,
            difficulty_target: 0x00FFFFFF,
            nonce: 0,
            height: 42,
            uncle_merkle_root: [0xCC; 32],
            total_reward: 1000000000,
            randomx_key: [0xDD; 32],
            coin_merkle_root: [0xEE; 32],
            nullifier_root: [0xFF; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        }
    }

    #[test]
    fn test_header_roundtrip() {
        let header = test_header();
        let bytes = serialize_header(&header);
        assert_eq!(bytes.len(), HEADER_SERIALIZED_SIZE);

        let deserialized = deserialize_header(&bytes).unwrap();
        assert_eq!(deserialized.version, header.version);
        assert_eq!(deserialized.previous, header.previous);
        assert_eq!(deserialized.timestamp, header.timestamp);
        assert_eq!(deserialized.difficulty_target, header.difficulty_target);
        assert_eq!(deserialized.nonce, header.nonce);
        assert_eq!(deserialized.height, header.height);
        assert_eq!(deserialized.uncle_merkle_root, header.uncle_merkle_root);
        assert_eq!(deserialized.total_reward, header.total_reward);
        assert_eq!(deserialized.randomx_key, header.randomx_key);
    }

    #[test]
    fn test_serialize_matches_to_mining_blob() {
        let header = test_header();
        let our_bytes = serialize_header(&header);
        let official_bytes = header.to_mining_blob();
        assert_eq!(our_bytes, official_bytes,
            "adaptor serialize_header must match BlockHeader::to_mining_blob()");
    }

    #[test]
    fn test_nonce_offset() {
        let mut header = test_header();
        header.nonce = 0xDEADBEEF;

        let bytes = serialize_header(&header);

        let nonce_bytes: [u8; 4] = bytes[NONCE_OFFSET..NONCE_OFFSET + 4]
            .try_into()
            .unwrap();
        let nonce = u32::from_le_bytes(nonce_bytes);
        assert_eq!(nonce, 0xDEADBEEF);
    }

    #[test]
    fn test_build_template_response_format() {
        let template = build_template_response(
            "abcd1234",
            42,
            1000,
            "deadbeef00000000000000000000000000000000000000000000000000000000",
            40,
        );

        assert_eq!(template["status"], "OK");
        assert_eq!(template["height"], 42);
        assert_eq!(template["difficulty"], 1000);
        assert_eq!(template["blocktemplate_blob"], "abcd1234");
        assert_eq!(template["blockhashing_blob"], "abcd1234");
        assert_eq!(template["prev_hash"], "deadbeef00000000000000000000000000000000000000000000000000000000");
        assert_eq!(template["reserved_offset"], 40);
        assert_eq!(template["untrusted"], false);
        assert_eq!(template["expected_reward"], 0);
    }
}
