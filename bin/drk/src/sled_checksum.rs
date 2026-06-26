// Sled Checksum Wrapper
//
// Sled 0.34.7 doesn't support page-level checksumming (added in 1.0).
// This module wraps sled reads/writes with a blake3 content checksum
// to detect torn pages and silent data corruption on crash recovery.
//
// Format: [32-byte blake3 hash][value bytes]
// On write: hash = blake3(value), store (hash || value)
// On read:  split at byte 32, verify hash(value) == stored hash

use blake3::Hash;

/// Prefix `value` with its blake3 hash. Returns (hash || value).
pub fn checksum_encode(value: &[u8]) -> Vec<u8> {
    let hash = blake3::hash(value);
    let mut out = Vec::with_capacity(32 + value.len());
    out.extend_from_slice(hash.as_bytes());
    out.extend_from_slice(value);
    out
}

/// Verify the blake3 hash prefix and return the inner value.
/// Returns `Err` if the stored value is shorter than 32 bytes
/// or the hash doesn't match.
pub fn checksum_decode(stored: &[u8]) -> Result<Vec<u8>, ChecksumError> {
    if stored.len() < 32 {
        return Err(ChecksumError::TooShort);
    }
    let (hash_bytes, value) = stored.split_at(32);
    let expected = Hash::from_bytes(hash_bytes.try_into().unwrap());
    let actual = blake3::hash(value);
    if expected != actual {
        return Err(ChecksumError::Mismatch);
    }
    Ok(value.to_vec())
}

#[derive(Debug)]
pub enum ChecksumError {
    TooShort,
    Mismatch,
}

impl std::fmt::Display for ChecksumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChecksumError::TooShort => write!(f, "sled checksum: value too short (< 32 bytes)"),
            ChecksumError::Mismatch => write!(f, "sled checksum: content hash mismatch — data may be corrupted"),
        }
    }
}
