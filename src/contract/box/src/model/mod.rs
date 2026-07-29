use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::pallas,
};

/// Box unique identifier — Poseidon hash of creator public key and nonce.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BoxId(pub pallas::Base);

impl BoxId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(BoxId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("BoxId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("BoxId: invalid field element".into()))
    }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> {
    Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap()))
        .ok_or_else(|| ContractError::IoError("invalid base".into()))
}

type MerklePath = [pallas::Base; 32];

// ============================================================================
// PUT
// ============================================================================

/// PutParams — all values needed by circuit + metadata.
/// Field order maps to circuit witness order for box_id through new_contents_commit,
/// then nullifier, expected_root, owner_secret, owner_pub, leaf_pos, path, tx_commitment, tx_nonce, tx_binding.
#[derive(Debug, Clone)]
pub struct PutParams {
    pub box_id: BoxId,
    pub old_state_nonce: pallas::Base,
    pub new_state_nonce: pallas::Base,
    pub old_contents_commit: pallas::Base,
    pub new_contents_commit: pallas::Base,
    pub new_leaf: pallas::Base,
    pub nullifier: pallas::Base,
    pub expected_root: pallas::Base,
    pub owner: PublicKey,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for PutParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutParams {
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let mut b = Vec::with_capacity(292 + path_bytes.len() + self.proof.len());
        b.extend_from_slice(&self.box_id.to_bytes());
        b.extend_from_slice(&self.old_state_nonce.to_repr());
        b.extend_from_slice(&self.new_state_nonce.to_repr());
        b.extend_from_slice(&self.old_contents_commit.to_repr());
        b.extend_from_slice(&self.new_contents_commit.to_repr());
        b.extend_from_slice(&self.new_leaf.to_repr());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.expected_root.to_repr());
        b.extend_from_slice(&self.owner.to_bytes());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.tx_binding.to_repr());
        b.extend_from_slice(&self.tx_nonce.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 292usize;
        if data.len() < hdr + 1024 { return Err(ContractError::IoError("PutParams: too short".into())); }
        let box_id = BoxId::decode(&data[0..32])?;
        let old_state_nonce = read_base(&data[32..64])?;
        let new_state_nonce = read_base(&data[64..96])?;
        let old_contents_commit = read_base(&data[96..128])?;
        let new_contents_commit = read_base(&data[128..160])?;
        let new_leaf = read_base(&data[160..192])?;
        let nullifier = read_base(&data[192..224])?;
        let expected_root = read_base(&data[224..256])?;
        let owner = PublicKey::from_bytes(data[256..288].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PutParams: invalid owner: {}", e)))?;
        let leaf_pos = u32::from_le_bytes(data[288..292].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = read_base(&data[hdr + i*32 .. hdr + (i+1)*32])?; }
        let path_end = hdr + 1024;
        let proof_len = data[path_end] as usize;
        if data.len() < path_end + 1 + proof_len + 64 { return Err(ContractError::IoError(format!("PutParams: too short for proof"))); }
        let proof = data[path_end+1..path_end+1+proof_len].to_vec();
        let pos2 = path_end + 1 + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?;
        let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(PutParams { box_id, old_state_nonce, new_state_nonce, old_contents_commit, new_contents_commit, new_leaf, nullifier, expected_root, owner, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)]
pub struct PutUpdate {
    pub nullifier: pallas::Base,
    pub new_leaf: pallas::Base,
}

impl dwow_serial::Encodable for PutUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutUpdate {
    pub fn encode(&self) -> Vec<u8> { let mut v = Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_repr()); v.extend_from_slice(&self.new_leaf.to_repr()); v }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 { return Err(ContractError::IoError(format!("PutUpdate: expected 64 bytes, got {}", data.len()))); }
        Ok(PutUpdate { nullifier: read_base(&data[0..32])?, new_leaf: read_base(&data[32..64])? })
    }
}

// ============================================================================
// TAKE
// ============================================================================

#[derive(Debug, Clone)]
pub struct TakeParams {
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub state_nonce: pallas::Base,
    pub nullifier: pallas::Base,
    pub expected_root: pallas::Base,
    pub owner: PublicKey,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TakeParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeParams {
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let mut b = Vec::with_capacity(196 + path_bytes.len() + self.proof.len());
        b.extend_from_slice(&self.box_id.to_bytes());
        b.extend_from_slice(&self.contents_commit.to_repr());
        b.extend_from_slice(&self.state_nonce.to_repr());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.expected_root.to_repr());
        b.extend_from_slice(&self.owner.to_bytes());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.tx_binding.to_repr());
        b.extend_from_slice(&self.tx_nonce.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr = 196usize;
        if data.len() < hdr + 1024 { return Err(ContractError::IoError("TakeParams: too short".into())); }
        let box_id = BoxId::decode(&data[0..32])?;
        let contents_commit = read_base(&data[32..64])?;
        let state_nonce = read_base(&data[64..96])?;
        let nullifier = read_base(&data[96..128])?;
        let expected_root = read_base(&data[128..160])?;
        let owner = PublicKey::from_bytes(data[160..192].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("TakeParams: invalid owner: {}", e)))?;
        let leaf_pos = u32::from_le_bytes(data[192..196].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = read_base(&data[hdr + i*32 .. hdr + (i+1)*32])?; }
        let path_end = hdr + 1024;
        let proof_len = data[path_end] as usize;
        if data.len() < path_end + 1 + proof_len + 64 { return Err(ContractError::IoError(format!("TakeParams: too short for proof"))); }
        let proof = data[path_end+1..path_end+1+proof_len].to_vec();
        let pos2 = path_end + 1 + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?;
        let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(TakeParams { box_id, contents_commit, state_nonce, nullifier, expected_root, owner, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)]
pub struct TakeUpdate {
    pub nullifier: pallas::Base,
}

impl dwow_serial::Encodable for TakeUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeUpdate {
    pub fn encode(&self) -> Vec<u8> { self.nullifier.to_repr().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("TakeUpdate: expected 32 bytes, got {}", data.len()))); }
        Ok(TakeUpdate { nullifier: read_base(&data[0..32])? })
    }
}
