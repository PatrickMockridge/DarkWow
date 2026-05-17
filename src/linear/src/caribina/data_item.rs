//! ANS-104 DataItem binary format (Ed25519, signature type 2)
//!
//! Implements the minimum subset of the ANS-104 specification needed
//! for Caribina block anchoring. No tags, no target, no anchor — just
//! a signed data payload that ArDrive Turbo accepts.
//!
//! Binary layout (Ed25519):
//!   Bytes   0-1:  signature_type = 2 (u16 LE)
//!   Bytes  2-65:  signature (64 bytes)
//!   Bytes 66-97:  owner/public key (32 bytes)
//!   Byte     98:  target presence (0)
//!   Byte     99:  anchor presence (0)
//!   Bytes100-107: tag count = 0 (u64 LE)
//!   Bytes108-115: tag bytes = 0 (u64 LE)
//!   Bytes  116+:  data payload

use sha2::{Digest, Sha384};

use super::wallet::CaribinaWallet;

/// Arweave signature type for Ed25519/Curve25519
pub const SIGNATURE_TYPE: u16 = 2;
pub const SIGNATURE_LENGTH: usize = 64;
pub const OWNER_LENGTH: usize = 32;

/// Fixed header overhead: 2 (type) + 64 (sig) + 32 (owner) + 1 (target) + 1
/// (anchor) + 8 (tag count) + 8 (tag bytes) = 116 bytes
pub const HEADER_LENGTH: usize = 116;

/// A tag for the DataItem (kept for API completeness but unused in Caribina).
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub value: String,
}

/// An ANS-104 DataItem with Ed25519 signing.
pub struct DataItem {
    /// Complete binary blob (header + data)
    bytes: Vec<u8>,
}

impl DataItem {
    /// Build a new unsigned DataItem from raw data bytes.
    ///
    /// The DataItem is NOT signed yet — call `sign()` to sign it before
    /// submitting to ArDrive Turbo.
    pub fn new(data: &[u8]) -> Self {
        let total_len = HEADER_LENGTH + data.len();
        let mut bytes = vec![0u8; total_len];

        // Signature type = 2 (little-endian u16)
        bytes[0] = (SIGNATURE_TYPE & 0xFF) as u8;
        bytes[1] = (SIGNATURE_TYPE >> 8) as u8;

        // Signature at bytes 2-65: left as zeros (will be filled by sign())
        // Owner at bytes 66-97: left as zeros (will be filled by sign())
        // Target presence at byte 98: already 0
        // Anchor presence at byte 99: already 0
        // Tag count at bytes 100-107: already 0 (u64 LE)
        // Tag bytes at bytes 108-115: already 0 (u64 LE)

        // Copy data
        bytes[HEADER_LENGTH..].copy_from_slice(data);

        Self { bytes }
    }

    /// Build a DataItem with tags (for ArDrive indexing).
    pub fn new_with_tags(data: &[u8], tags: &[Tag]) -> Self {
        let serialized_tags = serialize_tags(tags);
        let tags_len = serialized_tags.len();
        let tag_count = tags.len() as u64;

        let total_len = HEADER_LENGTH + tags_len + data.len();
        let mut bytes = vec![0u8; total_len];

        bytes[0] = (SIGNATURE_TYPE & 0xFF) as u8;
        bytes[1] = (SIGNATURE_TYPE >> 8) as u8;

        // Tag count at bytes 100-107
        bytes[100] = (tag_count & 0xFF) as u8;
        bytes[101] = ((tag_count >> 8) & 0xFF) as u8;
        bytes[102] = ((tag_count >> 16) & 0xFF) as u8;
        bytes[103] = ((tag_count >> 24) & 0xFF) as u8;
        bytes[104] = ((tag_count >> 32) & 0xFF) as u8;
        bytes[105] = ((tag_count >> 40) & 0xFF) as u8;
        bytes[106] = ((tag_count >> 48) & 0xFF) as u8;
        bytes[107] = ((tag_count >> 56) & 0xFF) as u8;

        // Tag bytes at bytes 108-115
        let tlen = tags_len as u64;
        bytes[108] = (tlen & 0xFF) as u8;
        bytes[109] = ((tlen >> 8) & 0xFF) as u8;
        bytes[110] = ((tlen >> 16) & 0xFF) as u8;
        bytes[111] = ((tlen >> 24) & 0xFF) as u8;
        bytes[112] = ((tlen >> 32) & 0xFF) as u8;
        bytes[113] = ((tlen >> 40) & 0xFF) as u8;
        bytes[114] = ((tlen >> 48) & 0xFF) as u8;
        bytes[115] = ((tlen >> 56) & 0xFF) as u8;

        // Copy serialized tags
        let tag_start = HEADER_LENGTH;
        bytes[tag_start..tag_start + tags_len].copy_from_slice(&serialized_tags);

        // Copy data after tags
        let data_start = tag_start + tags_len;
        bytes[data_start..].copy_from_slice(data);

        Self { bytes }
    }

    /// Sign this DataItem with a CaribinaWallet.
    ///
    /// Sets the owner (public key) and signature fields.
    pub fn sign(&mut self, wallet: &CaribinaWallet) {
        // Set owner (public key) at bytes 66-97
        let pk = wallet.public_key();
        self.bytes[66..98].copy_from_slice(&pk);

        // Compute deepHash signature data
        let sig_data = self.compute_signature_data();

        // Sign with Ed25519
        let sig = wallet.sign(&sig_data);

        // Set signature at bytes 2-65
        self.bytes[2..66].copy_from_slice(&sig);
    }

    /// Get the complete signed binary to POST to ArDrive Turbo.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Get the raw signature field.
    fn raw_signature(&self) -> &[u8] {
        &self.bytes[2..66]
    }

    /// Get the raw owner (public key) field.
    fn raw_owner(&self) -> &[u8] {
        &self.bytes[66..98]
    }

    /// Get the raw data payload.
    pub fn raw_data(&self) -> &[u8] {
        let data_start = self.data_start();
        &self.bytes[data_start..]
    }

    /// Compute the start offset of the data payload (after header + tags).
    fn data_start(&self) -> usize {
        let tag_bytes = read_u64_le(&self.bytes[108..116]) as usize;
        HEADER_LENGTH + tag_bytes
    }

    /// SHA-256 hash of the raw signature = the Arweave transaction ID.
    pub fn compute_id(&self) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update(self.raw_signature());
        let result = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&result);
        id
    }

    /// Verify this DataItem's signature (for node-side anchor verification).
    pub fn verify_signature(&self) -> bool {
        let sig_data = self.compute_signature_data();
        let mut sig = [0u8; 64];
        let mut pk = [0u8; 32];
        sig.copy_from_slice(self.raw_signature());
        pk.copy_from_slice(self.raw_owner());
        CaribinaWallet::verify(&pk, &sig_data, &sig)
    }

    /// Compute the deepHash of this DataItem for signing/verification.
    ///
    /// deepHash(["dataitem", "1", "2", owner, target, anchor, tags, data])
    ///
    /// See ANS-104 §2.1 — uses SHA-384 with a merkle-like construction:
    ///   - blob: SHA-384("blob" || len || data)
    ///   - list: SHA-384(hash("list" || count), then pair-wise with items)
    fn compute_signature_data(&self) -> Vec<u8> {
        let items: Vec<Vec<u8>> = vec![
            b"dataitem".to_vec(),
            b"1".to_vec(),
            b"2".to_vec(),
            self.raw_owner().to_vec(),
            self.raw_target(),
            self.raw_anchor(),
            self.raw_tags(),
            self.raw_data().to_vec(),
        ];
        deep_hash_list(&items)
    }

    fn raw_target(&self) -> Vec<u8> {
        Vec::new() // target not used for Caribina
    }

    fn raw_anchor(&self) -> Vec<u8> {
        Vec::new() // anchor not used for Caribina
    }

    fn raw_tags(&self) -> Vec<u8> {
        // Check if tags are present by reading tag count and tag bytes
        let tag_count = read_u64_le(&self.bytes[100..108]);
        if tag_count == 0 {
            return Vec::new();
        }
        let tag_bytes = read_u64_le(&self.bytes[108..116]) as usize;
        self.bytes[HEADER_LENGTH..HEADER_LENGTH + tag_bytes].to_vec()
    }

    /// Deserialize a DataItem from a raw binary blob (from Arweave gateway).
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_LENGTH {
            return None;
        }
        // Check signature type
        let sig_type = u16::from_le_bytes([bytes[0], bytes[1]]);
        if sig_type != SIGNATURE_TYPE {
            return None;
        }
        Some(Self {
            bytes: bytes.to_vec(),
        })
    }
}

/// Compute deep hash of a list of items.
///
/// deepHash(list) = pair-wise SHA-384 accumulation:
///   tag = SHA-384("list" + count)
///   acc = SHA-384(tag || deepHash(item[0]))
///   for i in 1..n: acc = SHA-384(acc || deepHash(item[i]))
fn deep_hash_list(items: &[Vec<u8>]) -> Vec<u8> {
    let tag = {
        let mut h = Sha384::new();
        h.update(b"list");
        h.update(items.len().to_string().as_bytes());
        h.finalize().to_vec()
    };

    if items.is_empty() {
        return tag;
    }

    let mut acc = {
        let mut h = Sha384::new();
        h.update(&tag);
        h.update(&deep_hash_blob(&items[0]));
        h.finalize().to_vec()
    };

    for item in &items[1..] {
        let mut h = Sha384::new();
        h.update(&acc);
        h.update(&deep_hash_blob(item));
        acc = h.finalize().to_vec();
    }

    acc
}

/// Compute deep hash of a blob.
///
/// deepHash(blob) = SHA-384(tag || SHA-384(data))
///   tag = SHA-384("blob" + length)
fn deep_hash_blob(data: &[u8]) -> Vec<u8> {
    let tag = {
        let mut h = Sha384::new();
        h.update(b"blob");
        h.update(data.len().to_string().as_bytes());
        h.finalize()
    };

    let data_hash = {
        let mut h = Sha384::new();
        h.update(data);
        h.finalize()
    };

    let mut h = Sha384::new();
    h.update(&tag[..]);
    h.update(&data_hash[..]);
    h.finalize().to_vec()
}

fn read_u64_le(bytes: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(buf)
}

/// Serialize tags using AVSCTap varint encoding.
fn serialize_tags(tags: &[Tag]) -> Vec<u8> {
    if tags.is_empty() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    // Tag count
    write_varint(&mut buf, tags.len() as i64);
    for tag in tags {
        write_varint(&mut buf, tag.name.len() as i64);
        buf.extend_from_slice(tag.name.as_bytes());
        write_varint(&mut buf, tag.value.len() as i64);
        buf.extend_from_slice(tag.value.as_bytes());
    }
    // Terminator
    write_varint(&mut buf, 0);
    buf
}

/// Write a signed variable-length integer in AVSCTap format.
///
/// Encodes as unsigned zigzag: positive n → n*2, negative n → ~n*2|1.
/// Each byte uses 7 bits for data and the high bit as continuation marker.
fn write_varint(buf: &mut Vec<u8>, n: i64) {
    let m: u64 = if n >= 0 {
        (n as u64) << 1
    } else {
        ((!n as u64) << 1) | 1
    };
    let mut val = m;
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_item_new_and_sign() {
        let data = b"test anchor data";
        let mut item = DataItem::new(data);
        let wallet = CaribinaWallet::generate();
        item.sign(&wallet);
        assert_eq!(item.as_bytes().len(), HEADER_LENGTH + data.len());
        assert!(item.verify_signature());
    }

    #[test]
    fn test_data_item_compute_id_consistent() {
        let mut item = DataItem::new(b"deterministic test");
        let wallet = CaribinaWallet::generate();
        item.sign(&wallet);
        let id1 = item.compute_id();
        let id2 = item.compute_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_data_item_verify_tampered_data_fails() {
        let mut item = DataItem::new(b"original");
        let wallet = CaribinaWallet::generate();
        item.sign(&wallet);
        // Tamper with the data
        let data_start = HEADER_LENGTH;
        item.bytes[data_start] ^= 0xFF;
        assert!(!item.verify_signature());
    }

    #[test]
    fn test_data_item_roundtrip_serialize_deserialize() {
        let mut item = DataItem::new(b"roundtrip payload");
        let wallet = CaribinaWallet::generate();
        item.sign(&wallet);
        let binary = item.as_bytes().to_vec();
        let deserialized = DataItem::deserialize(&binary).unwrap();
        assert!(deserialized.verify_signature());
        assert_eq!(deserialized.raw_data(), b"roundtrip payload");
    }

    #[test]
    fn test_data_item_empty_data() {
        let mut item = DataItem::new(b"");
        let wallet = CaribinaWallet::generate();
        item.sign(&wallet);
        assert_eq!(item.as_bytes().len(), HEADER_LENGTH);
        assert!(item.verify_signature());
    }

    #[test]
    fn test_data_item_with_tags() {
        let tags = vec![
            Tag {
                name: "App-Name".to_string(),
                value: "caribina-anchor".to_string(),
            },
            Tag {
                name: "Block-Height".to_string(),
                value: "42".to_string(),
            },
        ];
        let mut item = DataItem::new_with_tags(b"block_hash_data", &tags);
        let wallet = CaribinaWallet::generate();
        item.sign(&wallet);
        assert!(item.verify_signature());
        assert_eq!(item.raw_data(), b"block_hash_data");
    }
}
