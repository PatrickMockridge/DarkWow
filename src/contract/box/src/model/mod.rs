use crate::error::BoxError;
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, MerkleNode, Nullifier},
    error::ContractError,
    pasta::pallas,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BoxId(pub pallas::Base);

impl BoxId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> { pallas::Base::from_repr(*bytes).into_option().map(BoxId) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("BoxId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("BoxId: invalid field element".into()))
    }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("read_base: expected 32 bytes, got {}", data.len()))); }
    let arr: [u8; 32] = data.try_into().unwrap();
    Option::<pallas::Base>::from(pallas::Base::from_repr(arr)).ok_or_else(|| ContractError::IoError("invalid base".into()))
}

fn read_nullifier(data: &[u8]) -> Result<Nullifier, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("nullifier: expected 32 bytes, got {}", data.len()))); }
    Nullifier::from_bytes(data.try_into().unwrap())
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

#[derive(Debug, Clone)]
pub struct PutParams {
    pub box_id: BoxId, pub old_state_nonce: StateNonce, pub new_state_nonce: StateNonce,
    pub old_contents_commit: pallas::Base, pub new_contents_commit: pallas::Base,
    pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>,
    pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for PutParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|n| n.to_bytes()).collect();
        let hdr = 260usize;
        let mut b = Vec::with_capacity(hdr + path_bytes.len() + 1usize + self.proof.len() + 64usize);
        b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.old_state_nonce.to_repr());
        b.extend_from_slice(&self.new_state_nonce.to_repr()); b.extend_from_slice(&self.old_contents_commit.to_repr());
        b.extend_from_slice(&self.new_contents_commit.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.new_leaf.to_bytes());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes()); b.extend_from_slice(&path_bytes);
        b.push(u8::try_from(self.proof.len()).map_err(|_| ContractError::IoError("proof too long".into()))?);
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 260usize; if data.len() <= hdr + 1024usize { return Err(BoxError::DecodeFailure{field:"PutParams".into()}.into()); }
        let box_id = BoxId::decode(&data[0..32])?;
        let old_state_nonce = StateNonce::from_repr(data[32..64].try_into().map_err(|_| ContractError::IoError("old_state_nonce".into()))?).ok_or_else(|| ContractError::IoError("PutParams: invalid old_state_nonce".into()))?;
        let new_state_nonce = StateNonce::from_repr(data[64..96].try_into().map_err(|_| ContractError::IoError("new_state_nonce".into()))?).ok_or_else(|| ContractError::IoError("PutParams: invalid new_state_nonce".into()))?;
        let old_contents_commit = read_base(&data[96..128])?;
        let new_contents_commit = read_base(&data[128..160])?; let nullifier = read_nullifier(&data[160..192])?;
        let expected_root = read_merkle_node(&data[192..224])?; let new_leaf = read_merkle_node(&data[224..256])?;
        let leaf_pos = MerklePosition::from_le_bytes(data[256..260].try_into().map_err(|_| ContractError::IoError("leaf_pos".into()))?);
        let mut merkle_path = [MerkleNode::from_base(pallas::Base::zero()); 32];
        for i in 0..32 { merkle_path[i] = read_merkle_node(&data[hdr + i*32usize .. hdr + (i+1)*32usize])?; }
        let path_end = hdr + 1024usize; let proof_len = usize::from(data[path_end]);
        if data.len() < path_end + 1usize + proof_len + 64usize { return Err(BoxError::DecodeFailure{field:"PutParams".into()}.into()); }
        let proof = data[path_end+1..path_end+1+proof_len].to_vec(); let pos2 = path_end + 1usize + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?; let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(PutParams { box_id, old_state_nonce, new_state_nonce, old_contents_commit, new_contents_commit, nullifier, expected_root, new_leaf, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)] pub struct PutUpdate { pub nullifier: Nullifier, pub new_leaf: MerkleNode }
impl dwow_serial::Encodable for PutUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutUpdate {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v = Vec::with_capacity(64usize); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_bytes()); Ok(v) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(BoxError::DecodeFailure{field:"PutUpdate".into()}.into()); } Ok(PutUpdate{nullifier:read_nullifier(&data[0..32])?, new_leaf:read_merkle_node(&data[32..64])?}) }
}

// ============================================================================
// TAKE
// ============================================================================

#[derive(Debug, Clone)]
pub struct TakeParams {
    pub box_id: BoxId, pub contents_commit: pallas::Base, pub state_nonce: StateNonce,
    pub nullifier: Nullifier, pub expected_root: MerkleNode,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>,
    pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TakeParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|n| n.to_bytes()).collect();
        let hdr = 164usize; let mut b = Vec::with_capacity(hdr + path_bytes.len() + 1usize + self.proof.len() + 64usize);
        b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.contents_commit.to_repr());
        b.extend_from_slice(&self.state_nonce.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        b.push(u8::try_from(self.proof.len()).map_err(|_| ContractError::IoError("proof too long".into()))?);
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 164usize; if data.len() <= hdr + 1024usize { return Err(BoxError::DecodeFailure{field:"TakeParams".into()}.into()); }
        let box_id = BoxId::decode(&data[0..32])?; let contents_commit = read_base(&data[32..64])?;
        let state_nonce = StateNonce::from_repr(data[64..96].try_into().map_err(|_| ContractError::IoError("state_nonce".into()))?).ok_or_else(|| ContractError::IoError("TakeParams: invalid state_nonce".into()))?;
        let nullifier = read_nullifier(&data[96..128])?;
        let expected_root = read_merkle_node(&data[128..160])?;
        let leaf_pos = MerklePosition::from_le_bytes(data[160..164].try_into().map_err(|_| ContractError::IoError("leaf_pos".into()))?);
        let mut merkle_path = [MerkleNode::from_base(pallas::Base::zero()); 32];
        for i in 0..32 { merkle_path[i] = read_merkle_node(&data[hdr + i*32usize .. hdr + (i+1)*32usize])?; }
        let path_end = hdr + 1024usize; let proof_len = usize::from(data[path_end]);
        if data.len() < path_end + 1usize + proof_len + 64usize { return Err(BoxError::DecodeFailure{field:"TakeParams".into()}.into()); }
        let proof = data[path_end+1..path_end+1+proof_len].to_vec(); let pos2 = path_end + 1usize + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?; let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(TakeParams { box_id, contents_commit, state_nonce, nullifier, expected_root, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)] pub struct TakeUpdate { pub nullifier: Nullifier, pub current_root: MerkleNode }
impl dwow_serial::Encodable for TakeUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeUpdate {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v = Vec::with_capacity(64usize); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.current_root.to_bytes()); Ok(v) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(BoxError::DecodeFailure{field:"TakeUpdate".into()}.into()); } Ok(TakeUpdate{nullifier:read_nullifier(&data[0..32])?, current_root:read_merkle_node(&data[32..64])?}) }
}
