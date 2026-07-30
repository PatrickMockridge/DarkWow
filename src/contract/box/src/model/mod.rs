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

type MerklePath = [pallas::Base; 32];

// ============================================================================
// PUT
// ============================================================================

#[derive(Debug, Clone)]
pub struct PutParams {
    pub box_id: BoxId, pub old_state_nonce: pallas::Base, pub new_state_nonce: pallas::Base,
    pub old_contents_commit: pallas::Base, pub new_contents_commit: pallas::Base,
    pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub leaf_pos: u32, pub merkle_path: MerklePath, pub proof: Vec<u8>,
    pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for PutParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
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
        let hdr = 260usize; if data.len() <= hdr + 1024usize { return Err(ContractError::IoError("PutParams: too short".into())); }
        let box_id = BoxId::decode(&data[0..32])?; let old_state_nonce = read_base(&data[32..64])?;
        let new_state_nonce = read_base(&data[64..96])?; let old_contents_commit = read_base(&data[96..128])?;
        let new_contents_commit = read_base(&data[128..160])?; let nullifier = read_nullifier(&data[160..192])?;
        let expected_root = MerkleNode::from_bytes(data[192..224].try_into().unwrap()).ok_or_else(||ContractError::IoError("PutParams: invalid expected_root".into()))?; let new_leaf = MerkleNode::from_bytes(data[224..256].try_into().unwrap()).ok_or_else(||ContractError::IoError("PutParams: invalid new_leaf".into()))?;
        let leaf_pos = u32::from_le_bytes(data[256..260].try_into().map_err(|_| ContractError::IoError("leaf_pos".into()))?);
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = read_base(&data[hdr + i*32usize .. hdr + (i+1)*32usize])?; }
        let path_end = hdr + 1024usize; let proof_len = usize::from(data[path_end]);
        if data.len() < path_end + 1usize + proof_len + 64usize { return Err(ContractError::IoError("PutParams: too short for proof".into())); }
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
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError("PutUpdate: expected 64 bytes".into())); } Ok(PutUpdate{nullifier:read_nullifier(&data[0..32])?, new_leaf:MerkleNode::from_bytes(data[32..64].try_into().unwrap()).ok_or_else(||ContractError::IoError("PutUpdate: invalid new_leaf".into()))?}) }
}

// ============================================================================
// TAKE
// ============================================================================

#[derive(Debug, Clone)]
pub struct TakeParams {
    pub box_id: BoxId, pub contents_commit: pallas::Base, pub state_nonce: pallas::Base,
    pub nullifier: Nullifier, pub expected_root: MerkleNode,
    pub leaf_pos: u32, pub merkle_path: MerklePath, pub proof: Vec<u8>,
    pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TakeParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let hdr = 164usize; let mut b = Vec::with_capacity(hdr + path_bytes.len() + 1usize + self.proof.len() + 64usize);
        b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.contents_commit.to_repr());
        b.extend_from_slice(&self.state_nonce.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        b.push(u8::try_from(self.proof.len()).map_err(|_| ContractError::IoError("proof too long".into()))?);
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 164usize; if data.len() <= hdr + 1024usize { return Err(ContractError::IoError("TakeParams: too short".into())); }
        let box_id = BoxId::decode(&data[0..32])?; let contents_commit = read_base(&data[32..64])?;
        let state_nonce = read_base(&data[64..96])?; let nullifier = read_nullifier(&data[96..128])?;
        let expected_root = MerkleNode::from_bytes(data[128..160].try_into().unwrap()).ok_or_else(||ContractError::IoError("TakeParams: invalid expected_root".into()))?;
        let leaf_pos = u32::from_le_bytes(data[160..164].try_into().map_err(|_| ContractError::IoError("leaf_pos".into()))?);
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = read_base(&data[hdr + i*32usize .. hdr + (i+1)*32usize])?; }
        let path_end = hdr + 1024usize; let proof_len = usize::from(data[path_end]);
        if data.len() < path_end + 1usize + proof_len + 64usize { return Err(ContractError::IoError("TakeParams: too short for proof".into())); }
        let proof = data[path_end+1..path_end+1+proof_len].to_vec(); let pos2 = path_end + 1usize + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?; let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(TakeParams { box_id, contents_commit, state_nonce, nullifier, expected_root, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)] pub struct TakeUpdate { pub nullifier: Nullifier }
impl dwow_serial::Encodable for TakeUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeUpdate {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { Ok(self.nullifier.to_bytes().to_vec()) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError("TakeUpdate: expected 32 bytes".into())); } Ok(TakeUpdate{nullifier:read_nullifier(&data[0..32])?}) }
}
