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

//! Monero — Merge Mining Proof of Work
//!
//! Cryptographic proof that a Monero block was mined with a DarkWow
//! merge mining tag embedded in its coinbase transaction extra field.

use std::{
    fmt,
    io::{self, Cursor, Error, Read, Write},
    iter,
};

use dwow_sdk::hex::decode_hex;
#[cfg(feature = "async")]
use dwow_serial::{async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite};
use dwow_serial::{Decodable, Encodable};
use monero::{
    blockdata::transaction::{ExtraField, RawExtraField, SubField},
    consensus::{Decodable as XmrDecodable, Encodable as XmrEncodable},
    cryptonote::hash::Hashable,
    util::ringct::{RctSigBase, RctType},
    BlockHeader,
};
use tiny_keccak::{Hasher, Keccak};
use tracing::warn;

use crate::{error::LinearError, Result};

// ============================================================================
// Typed newtypes — Monero types crossing the FFI boundary per type-system.md §2
// ============================================================================
// Per merge-mining-ffi.md §2.1: Monero types MUST be distinct DarkWow newtypes.
// No [u8; 32] crosses the boundary without validation. Bytes round-trip is
// forbidden per type-system.md §2.2.

/// Monero Keccak-256 hash. Distinct from DarkWow's blake3::Hash and Poseidon
/// commitments. MUST NOT be unified with any other 32-byte type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoneroHash([u8; 32]);

impl MoneroHash {
    /// Construct from a 32-byte array. All-zero is rejected.
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        if bytes == [0u8; 32] { return None; }
        Some(Self(bytes))
    }

    pub fn inner(&self) -> &[u8; 32] { &self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0 }
}

impl From<monero::Hash> for MoneroHash {
    fn from(h: monero::Hash) -> Self { Self(h.0) }
}

impl From<MoneroHash> for monero::Hash {
    fn from(h: MoneroHash) -> Self { monero::Hash(h.0) }
}

/// RandomX VM key (seed_hash from Monero). MUST be exactly 32 bytes.
/// Distinct from MoneroHash and DarkWow's randomx_key in BlockHeader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RandomXKey([u8; 32]);

impl RandomXKey {
    /// Construct from exactly 32 bytes. Rejects non-32-byte inputs.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 32 { return None; }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        if arr == [0u8; 32] { return None; }
        Some(Self(arr))
    }

    pub fn inner(&self) -> &[u8; 32] { &self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0 }
}

/// Merge mining job identifier. blake3 hash of template contents, used as the
/// aux_hash in the P2Pool protocol. MUST be non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId([u8; 32]);

impl JobId {
    pub fn from_bytes(bytes: [u8; 32]) -> Option<Self> {
        if bytes == [0u8; 32] { return None; }
        Some(Self(bytes))
    }

    pub fn from_monero_hash(h: &monero::Hash) -> Option<Self> {
        Self::from_bytes(h.0)
    }

    pub fn inner(&self) -> &[u8; 32] { &self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0 }
    pub fn to_hex(&self) -> String { hex::encode(self.0) }
}

pub mod fixed_array;
use fixed_array::{FixedByteArray, MaxSizeVec};

pub mod merkle_proof;
use merkle_proof::MerkleProof;

pub mod keccak;
use keccak::{keccak_from_bytes, keccak_to_bytes};

pub mod utils;
use utils::{create_blockhashing_blob, create_merkle_proof, tree_hash};

pub mod merkle_tree_parameters;
pub use merkle_tree_parameters::MerkleTreeParameters;

/// Monerod JSON-RPC client (anchor verification support).
pub mod rpc;

/// Monero anchor plausibility verification.
mod verify;

pub use rpc::{get_block_by_height, get_block_count, MonerodError};
pub use verify::{verify_monero_anchor, MoneroVerifyError};

pub type AuxChainHashes = MaxSizeVec<monero::Hash, 128>;

/// This struct represents all the Proof of Work information required
/// for merge mining.
#[derive(Clone)]
pub struct MoneroPowData {
    /// Monero Header fields
    pub header: BlockHeader,
    /// RandomX VM key - length varies to a max len of 60.
    pub randomx_key: FixedByteArray,
    /// The number of transactions included in this Monero block.
    /// This is used to produce the blockhashing_blob.
    pub transaction_count: u16,
    /// Transaction root
    pub merkle_root: monero::Hash,
    /// Coinbase Merkle proof hashes
    pub coinbase_merkle_proof: MerkleProof,
    /// Incomplete hashed state of the coinbase transaction
    pub coinbase_tx_hasher: Keccak,
    /// Extra field of the coinbase
    pub coinbase_tx_extra: RawExtraField,
    /// Aux chain Merkle proof hashes
    pub aux_chain_merkle_proof: MerkleProof,
}

impl MoneroPowData {
    /// Constructs the Monero PoW data from the given block and seed
    pub fn new(
        block: monero::Block,
        seed: FixedByteArray,
        aux_chain_merkle_proof: MerkleProof,
    ) -> Result<Self> {
        let hashes = create_ordered_tx_hashes_from_block(&block);
        let root = tree_hash(&hashes)?;
        let hash =
            hashes.first().ok_or(LinearError::MoneroMergeMineError("No hashes for Merkle proof".to_string()))?;

        let coinbase_merkle_proof = create_merkle_proof(&hashes, hash).ok_or_else(|| {
            LinearError::MoneroMergeMineError(
                "create_merkle_proof returned None because the block has no coinbase".to_string(),
            )
        })?;

        let coinbase = block.miner_tx.clone();

        let mut keccak = Keccak::v256();
        let mut encoder_prefix = vec![];
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.unlock_time.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix)?;
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix)?;
        keccak.update(&encoder_prefix);

        Ok(Self {
            header: block.header,
            randomx_key: seed,
            transaction_count: hashes.len() as u16,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_tx_extra: block.miner_tx.prefix.extra,
            coinbase_tx_hasher: keccak,
            aux_chain_merkle_proof,
        })
    }

    /// Returns `true` if the coinbase Merkle proof produces the `merkle_root`
    /// hash, otherwise `false`.
    pub fn is_coinbase_valid_merkle_root(&self) -> bool {
        let mut finalised_prefix_keccak = self.coinbase_tx_hasher.clone();
        let mut encoder_extra_field = vec![];

        #[expect(clippy::unwrap_used, reason = "consensus_encode into Vec<u8> is infallible")]
        self.coinbase_tx_extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0u8; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let final_prefix_hash = monero::Hash::from_slice(&prefix_hash);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };

        let hashes = vec![final_prefix_hash, rct_sig_base.hash(), monero::Hash::null()];

        let encoder_final: Vec<u8> =
            hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();

        let coinbase_hash = monero::Hash::new(encoder_final);

        let merkle_root = self.coinbase_merkle_proof.calculate_root(&coinbase_hash);
        (self.merkle_root == merkle_root) && self.coinbase_merkle_proof.check_coinbase_path()
    }

    /// Returns the block hashing blob for the Monero block.
    pub fn to_block_hashing_blob(&self) -> Vec<u8> {
        create_blockhashing_blob(&self.header, &self.merkle_root, u64::from(self.transaction_count))
    }
}

impl fmt::Debug for MoneroPowData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut digest = [0u8; 32];
        self.coinbase_tx_hasher.clone().finalize(&mut digest);
        f.debug_struct("MoneroPowData")
            .field("header", &self.header)
            .field("randomx_key", &self.randomx_key)
            .field("transaction_count", &self.transaction_count)
            .field("merkle_root", &self.merkle_root)
            .field("coinbase_merkle_proof", &self.coinbase_merkle_proof)
            .field("coinbase_tx_extra", &self.coinbase_tx_extra)
            .field("aux_chain_merkle_proof", &self.aux_chain_merkle_proof)
            .finish()
    }
}

impl Encodable for MoneroPowData {
    fn encode<S: Write>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        // Monero library encoding doesn't do async, so in order to
        // match our AsyncEncodable implementation, we will write
        // to an intermediate buffer here, as well as for any other
        // fields that use Monero consensus encoding.
        let mut buf = vec![];
        self.header.consensus_encode(&mut buf)?;
        n += buf.encode(s)?;

        n += self.randomx_key.encode(s)?;
        n += self.transaction_count.encode(s)?;

        let mut buf = vec![];
        self.merkle_root.consensus_encode(&mut buf)?;
        n += buf.encode(s)?;

        n += self.coinbase_merkle_proof.encode(s)?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode(s)?;

        n += self.coinbase_tx_extra.0.encode(s)?;
        n += self.aux_chain_merkle_proof.encode(s)?;

        Ok(n)
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncEncodable for MoneroPowData {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> io::Result<usize> {
        let mut n = 0;

        // We write to an intermediate buffer since the Monero
        // consensus encoding library doesn't do async writing.
        let mut buf = vec![];
        self.header.consensus_encode(&mut buf)?;
        n += buf.encode_async(s).await?;

        n += self.randomx_key.encode_async(s).await?;
        n += self.transaction_count.encode_async(s).await?;

        let mut buf = vec![];
        self.merkle_root.consensus_encode(&mut buf)?;
        n += buf.encode_async(s).await?;

        n += self.coinbase_merkle_proof.encode_async(s).await?;

        // This is an incomplete hasher. Dump it from memory
        // and write it down. We can restore it the same way.
        let buf = keccak_to_bytes(&self.coinbase_tx_hasher);
        n += buf.encode_async(s).await?;

        n += self.coinbase_tx_extra.0.encode_async(s).await?;
        n += self.aux_chain_merkle_proof.encode_async(s).await?;

        Ok(n)
    }
}

impl Decodable for MoneroPowData {
    fn decode<D: Read>(d: &mut D) -> io::Result<Self> {
        let buf: Vec<u8> = Decodable::decode(d)?;
        let mut buf = Cursor::new(buf);
        let header = BlockHeader::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: FixedByteArray = Decodable::decode(d)?;
        let transaction_count: u16 = Decodable::decode(d)?;

        let buf: Vec<u8> = Decodable::decode(d)?;
        let mut buf = Cursor::new(buf);
        let merkle_root = monero::Hash::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR hash"))?;

        let coinbase_merkle_proof: MerkleProof = Decodable::decode(d)?;

        let buf: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf)?;

        let coinbase_tx_extra: Vec<u8> = Decodable::decode(d)?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);
        let aux_chain_merkle_proof: MerkleProof = Decodable::decode(d)?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

#[cfg(feature = "async")]
#[async_trait]
impl AsyncDecodable for MoneroPowData {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> io::Result<Self> {
        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let header = BlockHeader::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR header"))?;

        let randomx_key: FixedByteArray = AsyncDecodable::decode_async(d).await?;
        let transaction_count: u16 = AsyncDecodable::decode_async(d).await?;

        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let mut buf = Cursor::new(buf);
        let merkle_root = monero::Hash::consensus_decode(&mut buf)
            .map_err(|_| Error::other("Invalid XMR hash"))?;

        let coinbase_merkle_proof: MerkleProof = AsyncDecodable::decode_async(d).await?;

        let buf: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let coinbase_tx_hasher = keccak_from_bytes(&buf)?;

        let coinbase_tx_extra: Vec<u8> = AsyncDecodable::decode_async(d).await?;
        let coinbase_tx_extra = RawExtraField(coinbase_tx_extra);
        let aux_chain_merkle_proof: MerkleProof = AsyncDecodable::decode_async(d).await?;

        Ok(Self {
            header,
            randomx_key,
            transaction_count,
            merkle_root,
            coinbase_merkle_proof,
            coinbase_tx_hasher,
            coinbase_tx_extra,
            aux_chain_merkle_proof,
        })
    }
}

/// Create a set of ordered transaction hashes from a Monero block
pub fn create_ordered_tx_hashes_from_block(block: &monero::Block) -> Vec<monero::Hash> {
    iter::once(block.miner_tx.hash()).chain(block.tx_hashes.clone()).collect()
}

/// Try to decode a `monero::Block` given a hex blob
pub fn monero_block_deserialize(blob: &str) -> Result<monero::Block> {
    let bytes: Vec<u8> = decode_hex(blob)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut reader = Cursor::new(bytes);

    match monero::Block::consensus_decode(&mut reader) {
        Ok(v) => Ok(v),
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e).into()),
    }
}

/// Parsing an extra field from bytes will always return an extra field with
/// subfields that could be read even if it does not represent the original
// extra field.
/// As per Monero consensus rules, an error here will not represent failure
/// to deserialize a block, so no need to error here.
fn parse_extra_field_truncate_on_error(raw_extra_field: &RawExtraField) -> ExtraField {
    match ExtraField::try_parse(raw_extra_field) {
        Ok(v) => v,
        Err(v) => {
            warn!(
                target: "blockchain::monero::parse_extra_field_truncate_on_error",
                "[BLOCKCHAIN] Some Monero tx_extra subfields could not be parsed",
            );
            v
        }
    }
}

/// Extract the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root_from_block(monero: &monero::Block) -> Result<Option<monero::Hash>> {
    extract_aux_merkle_root(&monero.miner_tx.prefix.extra)
}

/// Extract the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root(extra_field: &RawExtraField) -> Result<Option<monero::Hash>> {
    let extra_field = parse_extra_field_truncate_on_error(extra_field);
    // Only one merge mining tag is allowed
    let merge_mining_hashes: Vec<monero::Hash> = extra_field
        .0
        .iter()
        .filter_map(|item| {
            if let SubField::MergeMining(_depth, merge_mining_hash) = item {
                Some(*merge_mining_hash)
            } else {
                None
            }
        })
        .collect();

    if merge_mining_hashes.len() > 1 {
        return Err(LinearError::MoneroMergeMineError("More than one MM tag found in coinbase".to_string()))
    }

    if let Some(merge_mining_hash) = merge_mining_hashes.into_iter().next() {
        Ok(Some(merge_mining_hash))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── The three merge-mining receipts ─────────────────────────────────
    //
    // Receipts 1 and 3 (`extract_aux_merkle_root`, `is_coinbase_valid_merkle_root`) had **no test at
    // all** until 2026-09-22: grep found them called only from `validation.rs:82`,
    // `block_acceptor.rs:194` and `mm_rpc.rs:509`/`:561`, never from a `#[cfg(test)]` block. They are the
    // whole security argument for merge mining — Receipt 1 says the Monero coinbase committed to *our*
    // aux merkle root, Receipt 3 says the proof's coinbase really is that block's coinbase — so a defect
    // in either is a defect in the claim that a DarkWow block is backed by Monero work.
    //
    // The only existing coverage was `test_monero_powdata_serde`, which is `#[cfg(feature = "async")]`
    // and exercises them incidentally, in the default feature set not at all.

    /// The real Monero testnet block used by the serde test: height 2912484, merge-mined DarkFi.
    const XMR_BLOCK: &str = "1010f881efca0644a1185eeccb2629b316ec0d41659111299ad1b736a3b0d8eac8bbc6384dc5c84bb6010002a0e2b10101ffe4e1b1010180e0a596bb1103f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2be14a010b204d874ed5087b649c711dd4479434a85dbf7e9bdfae26f5bc785964d4b45c0204751b43e10321082d5f403be836d45d026fbaa2a8e4b4a9d0d821f29d709321f8d764f32d446fa80000";
    const XMR_SEED: &str = "f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2b";

    /// A merge-mining sub-field in the wire form **this crate's parser** expects.
    ///
    /// Tag `0x3`, then a `u8` size, then a `VarInt` depth, then the 32-byte merkle root. That is three
    /// fields before the hash, not two: the arm at `monero` crate `blockdata/transaction.rs:846` reads
    /// `Decodable::consensus_decode::<u8>` into a discarded `_size`, then a `VarInt`, then the `Hash`.
    /// The first version of this helper emitted `[0x03, depth, hash]`, so the `VarInt` read began inside
    /// the hash bytes (whose first byte `0xAA` has the continuation bit set), the sub-field failed to
    /// parse, and the extraction returned `Ok(None)` — the positive control below caught it. Documented
    /// rather than left as a magic byte string, because the next reader will make the same assumption.
    ///
    /// A depth below 128 is a single `VarInt` byte. `size` is ignored by the parser; 1 matches what the
    /// real merge-mined testnet block carries.
    fn merge_mining_subfield(depth: u8, root: [u8; 32]) -> Vec<u8> {
        let mut v = vec![0x03, 0x01, depth];
        v.extend_from_slice(&root);
        v
    }

    /// Receipt 1, against a real merge-mined block: the aux merkle root is recovered from the Monero
    /// coinbase's `tx_extra`.
    ///
    /// This block really does carry a DarkFi merge-mining tag, so it is the strongest positive control
    /// available offline — and it is the one the merge-mining path itself depends on, since
    /// `mm_submit_solution` compares the submitted aux proof's root against exactly this value
    /// (`mm_rpc.rs:508-541`).
    #[test]
    fn extract_aux_merkle_root_recovers_the_root_from_a_real_merge_mined_block() {
        let block = monero_block_deserialize(XMR_BLOCK).expect("the testnet fixture must deserialize");

        match extract_aux_merkle_root_from_block(&block) {
            Ok(Some(root)) => {
                assert_ne!(root, monero::Hash::null(),
                    "a real merge-mining tag must carry a non-zero aux merkle root");
            }
            other => panic!(
                "expected the real block's merge-mining tag to yield a root, got {:?} — if this block \
                 has no MM tag the Receipt 1 tests below have no positive control",
                other
            ),
        }
    }

    /// Receipt 1's extraction, on inputs built here: the returned root is the tag's own hash, an empty
    /// extra field yields no root, and two tags are refused.
    ///
    /// The ambiguity case is the one worth having. `mm_submit_solution` takes the *submitted* aux hash
    /// and its proof and checks them against the root extracted here, so a coinbase carrying two tags
    /// would let a submitter choose which one to satisfy. The code refuses that, and nothing tested it.
    /// The cost of the refusal being absent is not a crash but a weakened receipt — the kind of defect
    /// that leaves every existing test green.
    #[test]
    fn extract_aux_merkle_root_handles_absent_and_ambiguous_tags() {
        // No tag at all.
        let empty = RawExtraField(vec![]);
        assert!(
            matches!(extract_aux_merkle_root(&empty), Ok(None)),
            "an extra field with no sub-fields must yield no aux root"
        );

        // A non-merge-mining sub-field only: tag 0x0 (padding) with a zero length.
        let padding_only = RawExtraField(vec![0x00, 0x00]);
        assert!(
            matches!(extract_aux_merkle_root(&padding_only), Ok(None)),
            "a padding-only extra field must yield no aux root"
        );

        // Exactly one tag: the root comes back byte-identical to what was embedded.
        let root = [0xAAu8; 32];
        let one = RawExtraField(merge_mining_subfield(1, root));
        match extract_aux_merkle_root(&one) {
            Ok(Some(extracted)) => assert_eq!(
                extracted.to_bytes(), root,
                "the extracted root must be the tag's own hash, not some other field's"
            ),
            other => panic!("expected the embedded root, got {:?}", other),
        }

        // Two tags: refused, because a submitter could otherwise satisfy whichever one suited them.
        let mut two = merge_mining_subfield(1, [0xAAu8; 32]);
        two.extend_from_slice(&merge_mining_subfield(1, [0xBBu8; 32]));
        let ambiguous = RawExtraField(two);
        assert!(
            extract_aux_merkle_root(&ambiguous).is_err(),
            "two merge-mining tags must be refused rather than resolved by picking one"
        );
    }

    /// Build `MoneroPowData` from the real testnet block, with the synthetic aux proof the serde test
    /// uses (the real aux hash is not recoverable offline, and Receipt 3 does not depend on it).
    fn real_block_powdata() -> MoneroPowData {
        use std::str::FromStr;

        let block = monero_block_deserialize(XMR_BLOCK).expect("the testnet fixture must deserialize");
        let seed = FixedByteArray::from_bytes(&hex::decode(XMR_SEED).expect("seed is hex"))
            .expect("seed fits the fixed array");
        let tx_hashes = [
            "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
            "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
        ]
        .iter()
        .map(|h| monero::Hash::from_str(h).expect("fixture hash is hex"))
        .collect::<Vec<_>>();
        let aux_proof = create_merkle_proof(&tx_hashes, &tx_hashes[0])
            .expect("the fixture proof must build");
        MoneroPowData::new(block, seed, aux_proof).expect("the fixture must construct")
    }

    /// Receipt 3, positive control: a real merge-mined block satisfies its own coinbase proof.
    ///
    /// This is the check `block_acceptor.rs:194` runs on every merge-mined block and the check
    /// `validation.rs:82` runs on the competing-fork path. If it did not hold for a real block, every
    /// merge-mined block would be rejected — so the assertion is load-bearing in both directions, and
    /// the negatives below only mean something because this passes.
    #[test]
    fn is_coinbase_valid_merkle_root_accepts_a_real_merge_mined_block() {
        let powdata = real_block_powdata();
        assert!(
            powdata.is_coinbase_valid_merkle_root(),
            "a real merge-mined block must satisfy its own coinbase merkle proof — if this fails, the \
             acceptance path rejects genuinely valid blocks"
        );
    }

    /// Receipt 3, negative controls: tampering with either the coinbase's extra field or the claimed
    /// merkle root must break the receipt.
    ///
    /// The two mutations attack the two halves of the check — the reconstructed coinbase hash and the
    /// root it is proved against — so a version of the function that compared only one of them would be
    /// caught by whichever control it ignored. `coinbase_tx_extra` is the field an attacker would
    /// actually choose: it is raw bytes from a peer, and it is what carries the merge-mining tag.
    #[test]
    fn is_coinbase_valid_merkle_root_rejects_tampering() {
        let honest = real_block_powdata();
        assert!(honest.is_coinbase_valid_merkle_root(), "control: the untampered data must verify");

        // Tamper the coinbase's extra field.
        let mut extra_tampered = real_block_powdata();
        assert!(
            !extra_tampered.coinbase_tx_extra.0.is_empty(),
            "control: the fixture's extra field must be non-empty, or the mutation below is a no-op"
        );
        extra_tampered.coinbase_tx_extra.0[0] ^= 0xFF;
        assert!(
            !extra_tampered.is_coinbase_valid_merkle_root(),
            "a coinbase whose extra field does not match its prefix hash must be rejected"
        );

        // Claim a different merkle root.
        let mut root_tampered = real_block_powdata();
        root_tampered.merkle_root = monero::Hash::null();
        assert!(
            !root_tampered.is_coinbase_valid_merkle_root(),
            "a coinbase proof against the wrong merkle root must be rejected"
        );
    }

    // Test that both sync and async serialization formats match.
    // We do some hacks because Monero lib doesn't do async.
    #[test]
    #[cfg(feature = "async")]
    fn test_monero_powdata_serde() {
        use std::str::FromStr;

        // Blob from Monero testnet, height 2912484, mergemined DarkFi.
        const XMR_BLOCK: &str = "1010f881efca0644a1185eeccb2629b316ec0d41659111299ad1b736a3b0d8eac8bbc6384dc5c84bb6010002a0e2b10101ffe4e1b1010180e0a596bb1103f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2be14a010b204d874ed5087b649c711dd4479434a85dbf7e9bdfae26f5bc785964d4b45c0204751b43e10321082d5f403be836d45d026fbaa2a8e4b4a9d0d821f29d709321f8d764f32d446fa80000";
        const SEED_HASH: &str = "f1d23951bd28ce2bfad791f2350e2ac348e4620e19af3418653a1839cc5c8f2b";

        let block = monero_block_deserialize(XMR_BLOCK).unwrap();
        let seed = FixedByteArray::from_bytes(&hex::decode(SEED_HASH).unwrap()).unwrap();

        // The Merkle proof is fake to keep it simple.
        let tx_hashes = &[
            "d96756959949db23764592fea0bfe88c790e1fd131dabb676948b343aa9ecc24",
            "77d1a87df131c36da4832a7ec382db9b8fe947576a60ec82cc1c66a220f6ee42",
        ]
        .iter()
        .map(|hash| monero::Hash::from_str(hash).unwrap())
        .collect::<Vec<_>>();

        let aux_chain_merkle_proof = create_merkle_proof(tx_hashes, &tx_hashes[0]).unwrap();

        // Construct PowData
        let mut powdata = MoneroPowData::new(block, seed, aux_chain_merkle_proof).unwrap();

        let local_ex = smol::LocalExecutor::new();

        let ser_sync = dwow_serial::serialize(&powdata);
        let ser_async = smol::future::block_on(
            local_ex.run(async { dwow_serial::serialize_async(&powdata).await }),
        );

        assert_eq!(ser_sync, ser_async);

        let mut de_sync: MoneroPowData = dwow_serial::deserialize(&ser_async).unwrap();
        let mut de_async: MoneroPowData = smol::future::block_on(
            local_ex.run(async { dwow_serial::deserialize_async(&ser_async).await.unwrap() }),
        );

        assert_eq!(de_sync.header, powdata.header);
        assert_eq!(de_sync.randomx_key, powdata.randomx_key);
        assert_eq!(de_sync.transaction_count, powdata.transaction_count);
        assert_eq!(de_sync.merkle_root, powdata.merkle_root);
        assert_eq!(de_sync.coinbase_merkle_proof.branch(), powdata.coinbase_merkle_proof.branch());
        assert_eq!(de_sync.coinbase_merkle_proof.path(), powdata.coinbase_merkle_proof.path());
        assert_eq!(de_sync.coinbase_tx_extra, powdata.coinbase_tx_extra);
        assert_eq!(
            de_sync.aux_chain_merkle_proof.branch(),
            powdata.aux_chain_merkle_proof.branch()
        );
        assert_eq!(de_sync.aux_chain_merkle_proof.path(), powdata.aux_chain_merkle_proof.path());

        assert_eq!(de_async.header, powdata.header);
        assert_eq!(de_async.randomx_key, powdata.randomx_key);
        assert_eq!(de_async.transaction_count, powdata.transaction_count);
        assert_eq!(de_async.merkle_root, powdata.merkle_root);
        assert_eq!(de_async.coinbase_merkle_proof.branch(), powdata.coinbase_merkle_proof.branch());
        assert_eq!(de_async.coinbase_merkle_proof.path(), powdata.coinbase_merkle_proof.path());
        assert_eq!(de_async.coinbase_tx_extra, powdata.coinbase_tx_extra);
        assert_eq!(
            de_async.aux_chain_merkle_proof.branch(),
            powdata.aux_chain_merkle_proof.branch()
        );
        assert_eq!(de_async.aux_chain_merkle_proof.path(), powdata.aux_chain_merkle_proof.path());

        // Keccak state
        powdata.coinbase_tx_hasher.update(b"hi");
        let mut powdata_digest = vec![];
        powdata.coinbase_tx_hasher.finalize(&mut powdata_digest);

        de_sync.coinbase_tx_hasher.update(b"hi");
        let mut de_sync_digest = vec![];
        de_sync.coinbase_tx_hasher.finalize(&mut de_sync_digest);

        de_async.coinbase_tx_hasher.update(b"hi");
        let mut de_async_digest = vec![];
        de_async.coinbase_tx_hasher.finalize(&mut de_async_digest);

        assert_eq!(de_sync_digest, powdata_digest);
        assert_eq!(de_async_digest, powdata_digest);
    }

    // ── Newtype validation tests (merge-mining-ffi.md §2.1) ─────────

    #[test]
    fn monero_hash_rejects_zero() {
        assert!(MoneroHash::from_bytes([0u8; 32]).is_none());
    }

    #[test]
    fn monero_hash_accepts_nonzero() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x42;
        assert!(MoneroHash::from_bytes(bytes).is_some());
    }

    #[test]
    fn randomx_key_rejects_wrong_length() {
        assert!(RandomXKey::from_bytes(&[0u8; 31]).is_none());
        assert!(RandomXKey::from_bytes(&[0u8; 33]).is_none());
    }

    #[test]
    fn randomx_key_rejects_zero() {
        assert!(RandomXKey::from_bytes(&[0u8; 32]).is_none());
    }

    #[test]
    fn job_id_rejects_zero() {
        assert!(JobId::from_bytes([0u8; 32]).is_none());
    }

    #[test]
    fn newtype_roundtrip_monero_hash() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x13;
        let mh = MoneroHash::from_bytes(bytes).unwrap();
        let xmr: monero::Hash = mh.into();
        let back: MoneroHash = xmr.into();
        assert_eq!(mh, back);
    }
}
