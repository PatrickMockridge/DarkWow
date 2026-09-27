use crate::error::BoxError;
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, MerkleNode, Nullifier},
    error::ContractError,
    pasta::pallas,
};

// ============================================================================
// TOTAL BYTE READS
// ============================================================================
//
// Every `decode` below validates its buffer length before slicing, which makes each `data[a..b]`
// *provably* in-bounds. But provable is not free: the bounds check still compiles, and its panic
// location — `Location { file: &'static str, line: u32 }` — is a string and an integer in the
// contract artifact's data section, which neither `strip` nor `--release` removes. `get` +
// `try_into` removes the check rather than asserting it away, and `saturating_add` keeps the offset
// arithmetic itself total. Copied from native_token's model, which holds the same family.

/// Read exactly `N` bytes at `offset` — total: `get` + `try_into`, no index and no unwrap.
fn read_field<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], ContractError> {
    data.get(offset..offset.saturating_add(N))
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            ContractError::IoError(format!(
                "truncated field: need {N} bytes at offset {offset}, buffer has {}",
                data.len()
            ))
        })
}

// `read_byte` lived here, for the two one-byte proof-length prefixes. Both are `SerializedLen` now, so
// the only caller that needed a bare byte is gone and the helper with it.

/// Borrow exactly `len` bytes at `offset` — total, for the same reason as [`read_field`]. Borrowed
/// rather than copied, so a nested `decode` can take the sub-slice directly.
fn read_slice(data: &[u8], offset: usize, len: usize) -> Result<&[u8], ContractError> {
    data.get(offset..offset.saturating_add(len)).ok_or_else(|| {
        ContractError::IoError(format!(
            "truncated field: need {len} bytes at offset {offset}, buffer has {}",
            data.len()
        ))
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BoxId(pub pallas::Base);

impl BoxId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> { pallas::Base::from_repr(*bytes).into_option().map(BoxId) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("BoxId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(&read_field::<32>(data, 0)?).ok_or_else(|| ContractError::IoError("BoxId: invalid field element".into()))
    }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("read_base: expected 32 bytes, got {}", data.len()))); }
    let arr: [u8; 32] = read_field::<32>(data, 0)?;
    Option::<pallas::Base>::from(pallas::Base::from_repr(arr)).ok_or_else(|| ContractError::IoError("invalid base".into()))
}

fn read_nullifier(data: &[u8]) -> Result<Nullifier, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("nullifier: expected 32 bytes, got {}", data.len()))); }
    Nullifier::from_bytes(read_field::<32>(data, 0)?)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct MerklePosition(u32);
impl MerklePosition {
    pub fn new(v: u32) -> Self { Self(v) }
    pub fn inner(&self) -> u32 { self.0 }
    pub fn to_le_bytes(&self) -> [u8; 4] { self.0.to_le_bytes() }
    pub fn from_le_bytes(b: [u8; 4]) -> Self { Self(u32::from_le_bytes(b)) }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct StateNonce(pallas::Base);
impl StateNonce {
    pub fn new(v: pallas::Base) -> Self { Self(v) }
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_repr(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_repr(b: [u8; 32]) -> Option<Self> { pallas::Base::from_repr(b).into_option().map(Self) }
}

type MerklePath = [MerkleNode; 32];

fn read_merkle_node(data: &[u8]) -> Result<MerkleNode, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("read_merkle_node: expected 32 bytes, got {}", data.len()))); }
    let arr: [u8; 32] = data.try_into().map_err(|_| ContractError::IoError("read_merkle_node: slice conversion failed".into()))?;
    MerkleNode::from_bytes(arr).ok_or_else(|| ContractError::IoError("read_merkle_node: invalid MerkleNode".into()))
}

// ============================================================================
// PUT
// ============================================================================

/// Parameters for `PutV1`.
///
/// **Three fields left this struct, and the wire with them.** `new_state_nonce`,
/// `old_contents_commit` and `new_contents_commit` were plain params, so every put published them; all
/// three are witness-tagged now and travel in no call data. The circuit needed them as *witnesses* all
/// along — `put.zk:46` folds `old_contents_commit` into the old leaf, `:67` folds `new_contents_commit`
/// into the new one, and `:63-64` computes `new_state_nonce` as `base_add(old_state_nonce, ONE)` and
/// constrains it — and a witness does not have to be published for a verifier, which is the whole of
/// `privacy.md` §2's promise.
#[derive(Debug, Clone)]
pub struct PutParams {
    pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>,
    pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for PutParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|n| n.to_bytes()).collect();
        // hdr = 100 (was 196, and 260 before that): `box_id`, `old_state_nonce`, `new_state_nonce` and
        // both contents commitments are off the wire — the first two read from the wallet's record, the
        // nonce derived, the comments folded into leaves. See the struct's note.
        let hdr = 100usize;
        let mut b = Vec::with_capacity(hdr + path_bytes.len() + 1usize + self.proof.len() + 64usize);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.new_leaf.to_bytes());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes()); b.extend_from_slice(&path_bytes);
        // `SerializedLen`, not a bare `u8`: it is always four bytes, refuses a length that does not
        // fit, and its decoder is the exact inverse of its encoder. With one byte a proof of 256+
        // bytes could not be encoded at all, and a truncated frame could not be told from a short one
        // — the class `OBL-C150` records. `purse` was converted in the same pass, so this is now the
        // convention every contract in the tree follows and the one the SDK's manifest models.
        let pl = dwow_sdk::blockchain::SerializedLen::try_from_len(self.proof.len())?;
        b.extend_from_slice(&pl.to_le_bytes());
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 100usize; if data.len() <= hdr + 1024usize { return Err(BoxError::DecodeFailure{field:"PutParams".into()}.into()); }
        let nullifier = read_nullifier(read_slice(data, 0, 32)?)?;
        let expected_root = read_merkle_node(read_slice(data, 32, 32)?)?; let new_leaf = read_merkle_node(read_slice(data, 64, 32)?)?;
        let leaf_pos = MerklePosition::from_le_bytes(read_field::<4>(data, 96)?);
        let mut merkle_path = [MerkleNode::from_base(pallas::Base::zero()); 32];
        for (i, slot) in merkle_path.iter_mut().enumerate() { *slot = read_merkle_node(read_slice(data, hdr.saturating_add(i.saturating_mul(32)), 32)?)?; }
        let path_end = hdr + 1024usize;
        let proof_len = dwow_sdk::blockchain::SerializedLen::from_le_bytes(read_field::<4>(data, path_end)?).to_usize();
        let pos2 = path_end.saturating_add(dwow_sdk::blockchain::SerializedLen::ENCODED_SIZE).saturating_add(proof_len);
                // A minimum, not an equality: the payload is `selector ++ params ++ AEAD note`, so the
        // params are its front and something follows them. See `purse`'s note — the same shape was
        // measured there by a failing first deposit.
        if data.len() < pos2.saturating_add(64) { return Err(BoxError::DecodeFailure{field:"PutParams".into()}.into()); }
        let proof = read_slice(data, path_end+dwow_sdk::blockchain::SerializedLen::ENCODED_SIZE, proof_len)?.to_vec();
        let tx_binding = read_base(read_slice(data, pos2, 32)?)?; let tx_nonce = read_base(read_slice(data, pos2+32, 32)?)?;
        Ok(PutParams { nullifier, expected_root, new_leaf, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)] pub struct PutUpdate { pub nullifier: Nullifier, pub new_leaf: MerkleNode }
impl dwow_serial::Encodable for PutUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutUpdate {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v = Vec::with_capacity(64usize); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_bytes()); Ok(v) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(BoxError::DecodeFailure{field:"PutUpdate".into()}.into()); } Ok(PutUpdate{nullifier:read_nullifier(read_slice(data,0,32)?)?, new_leaf:read_merkle_node(read_slice(data,32,32)?)?}) }
}

// ============================================================================
// TAKE
//
// `box_id` and `state_nonce` are NOT on the wire — the same reduction as purse, on
// the same grounds: `privacy.md` §2 promises an observer learns "not which resource
// was operated on", §5.5 says `box_id` "is never a public input", and §2.4's table
// has Box hiding "which box … in Poseidon commitment". A value in `Call.data` is
// plaintext, committed to byte-for-byte by the transaction hash. Measured before
// removal: no host read either, and the wallet's prover takes both from its own
// record (`CapRecord.object_id`, `.state_nonce`) through `note:` witness sources.
// **Residue, declared in `scripts/check-l1-wire-conformance.sh`**: the two contents
// commitments and `new_state_nonce`, whose slot the *prover* must supply (the
// circuit constrains it to `old + 1`) and which no `note:` field can yield — purse's
// circuit derives its successor internally, so it has no such witness.
// ============================================================================

#[derive(Debug, Clone)]
pub struct TakeParams {
    pub nullifier: Nullifier, pub expected_root: MerkleNode,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>,
    pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TakeParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|n| n.to_bytes()).collect();
        // hdr = 68 (was 100, and 164 before that): `box_id`, `state_nonce` and the contents commitment
        // are off the wire.
        let hdr = 68usize; let mut b = Vec::with_capacity(hdr + path_bytes.len() + 1usize + self.proof.len() + 64usize);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        // `SerializedLen`, not a bare `u8`: it is always four bytes, refuses a length that does not
        // fit, and its decoder is the exact inverse of its encoder. With one byte a proof of 256+
        // bytes could not be encoded at all, and a truncated frame could not be told from a short one
        // — the class `OBL-C150` records. `purse` was converted in the same pass, so this is now the
        // convention every contract in the tree follows and the one the SDK's manifest models.
        let pl = dwow_sdk::blockchain::SerializedLen::try_from_len(self.proof.len())?;
        b.extend_from_slice(&pl.to_le_bytes());
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 68usize; if data.len() <= hdr + 1024usize { return Err(BoxError::DecodeFailure{field:"TakeParams".into()}.into()); }
        let nullifier = read_nullifier(read_slice(data, 0, 32)?)?;
        let expected_root = read_merkle_node(read_slice(data, 32, 32)?)?;
        let leaf_pos = MerklePosition::from_le_bytes(read_field::<4>(data, 64)?);
        let mut merkle_path = [MerkleNode::from_base(pallas::Base::zero()); 32];
        for (i, slot) in merkle_path.iter_mut().enumerate() { *slot = read_merkle_node(read_slice(data, hdr.saturating_add(i.saturating_mul(32)), 32)?)?; }
        let path_end = hdr + 1024usize;
        let proof_len = dwow_sdk::blockchain::SerializedLen::from_le_bytes(read_field::<4>(data, path_end)?).to_usize();
        let pos2 = path_end.saturating_add(dwow_sdk::blockchain::SerializedLen::ENCODED_SIZE).saturating_add(proof_len);
                // A minimum, not an equality: the payload is `selector ++ params ++ AEAD note`, so the
        // params are its front and something follows them. See `purse`'s note — the same shape was
        // measured there by a failing first deposit.
        if data.len() < pos2.saturating_add(64) { return Err(BoxError::DecodeFailure{field:"TakeParams".into()}.into()); }
        let proof = read_slice(data, path_end+dwow_sdk::blockchain::SerializedLen::ENCODED_SIZE, proof_len)?.to_vec();
        let tx_binding = read_base(read_slice(data, pos2, 32)?)?; let tx_nonce = read_base(read_slice(data, pos2+32, 32)?)?;
        Ok(TakeParams { nullifier, expected_root, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)] pub struct TakeUpdate { pub nullifier: Nullifier, pub current_root: MerkleNode }
impl dwow_serial::Encodable for TakeUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeUpdate {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v = Vec::with_capacity(64usize); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.current_root.to_bytes()); Ok(v) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(BoxError::DecodeFailure{field:"TakeUpdate".into()}.into()); } Ok(TakeUpdate{nullifier:read_nullifier(read_slice(data,0,32)?)?, current_root:read_merkle_node(read_slice(data,32,32)?)?}) }
}
