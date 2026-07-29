use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
    error::ContractError,
    pasta::pallas,
};

/// Box unique identifier — Poseidon hash of creator public key and nonce.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BoxId(pub pallas::Base);

impl BoxId {
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(BoxId)
    }

    pub fn encode(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "BoxId: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("BoxId: invalid field element".into()))
    }
}

/// On-chain Box record.
#[derive(Debug, Clone)]
pub struct BoxRecord {
    pub version: u8,
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub is_empty: bool,
}

impl BoxRecord {
    pub const ENCODED_SIZE: usize = 66; // 1 + 32 + 32 + 1

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.box_id.to_bytes());
        buf.extend_from_slice(&self.contents_commit.to_repr());
        buf.push(self.is_empty as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "BoxRecord: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let box_id = BoxId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("BoxRecord: invalid box_id".into()))?;
        let contents_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[33..65].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("BoxRecord: invalid contents_commit".into()))?;
        let is_empty = data[65] != 0;
        Ok(BoxRecord { version, box_id, contents_commit, is_empty })
    }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

/// Put parameters.
#[derive(Debug, Clone,)]
pub struct PutParamsV1 {
    pub box_id: BoxId,
    pub old_contents_commit: pallas::Base,
    pub new_contents_commit: pallas::Base,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for PutParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(161+self.proof.len()); b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.old_contents_commit.to_repr()); b.extend_from_slice(&self.new_contents_commit.to_repr()); b.extend_from_slice(&self.owner.to_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 161 { return Err(ContractError::IoError("PutParamsV1: too short".into())); } let box_id = BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("PutParamsV1: invalid box_id".into()))?; let old_contents_commit = read_base(&data[32..64])?; let new_contents_commit = read_base(&data[64..96])?; let owner = PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PutParamsV1: invalid owner: {}", e)))?; let proof_len = data[128] as usize; if data.len() != 129+proof_len+64 { return Err(ContractError::IoError(format!("PutParamsV1: expected {} bytes, got {}", 129+proof_len+64, data.len()))); } let proof = data[129..129+proof_len].to_vec(); let pos = 129+proof_len; let tx_binding = read_base(&data[pos..pos+32])?; let tx_nonce = read_base(&data[pos+32..pos+64])?; Ok(PutParamsV1 { box_id, old_contents_commit, new_contents_commit, owner, proof, tx_binding, tx_nonce }) } }

/// Put update.
#[derive(Debug, Clone)]
pub struct PutUpdateV1 {
    pub box_id: BoxId,
    pub new_contents_commit: pallas::Base,
}

impl dwow_serial::Encodable for PutUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.new_contents_commit.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("PutUpdateV1: expected 64 bytes, got {}", data.len()))); } Ok(PutUpdateV1 { box_id: BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("PutUpdateV1: invalid box_id".into()))?, new_contents_commit: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PutUpdateV1: invalid new_contents_commit".into()))? }) }
}

/// Take parameters.
#[derive(Debug, Clone,)]
pub struct TakeParamsV1 {
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub nullifier: Nullifier,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TakeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(161+self.proof.len()); b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.contents_commit.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.owner.to_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 161 { return Err(ContractError::IoError("TakeParamsV1: too short".into())); } let box_id = BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("TakeParamsV1: invalid box_id".into()))?; let contents_commit = read_base(&data[32..64])?; let nullifier = Nullifier::from_bytes(data[64..96].try_into().unwrap())?; let owner = PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("TakeParamsV1: invalid owner: {}", e)))?; let proof_len = data[128] as usize; if data.len() != 129+proof_len+64 { return Err(ContractError::IoError(format!("TakeParamsV1: expected {} bytes, got {}", 129+proof_len+64, data.len()))); } let proof = data[129..129+proof_len].to_vec(); let pos = 129+proof_len; let tx_binding = read_base(&data[pos..pos+32])?; let tx_nonce = read_base(&data[pos+32..pos+64])?; Ok(TakeParamsV1 { box_id, contents_commit, nullifier, owner, proof, tx_binding, tx_nonce }) } }

/// Take update.
#[derive(Debug, Clone)]
pub struct TakeUpdateV1 {
    pub box_id: BoxId,
    pub nullifier: Nullifier,
}

impl dwow_serial::Encodable for TakeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.box_id.to_bytes()); b.extend_from_slice(&self.nullifier.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("TakeUpdateV1: expected 64 bytes, got {}", data.len()))); } Ok(TakeUpdateV1 { box_id: BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("TakeUpdateV1: invalid box_id".into()))?, nullifier: Nullifier::from_bytes(data[32..64].try_into().unwrap())? }) }
}

// ============================================================================
// V3 TYPES — Hard path with Merkle inclusion proofs
// ============================================================================

/// Merkle path for MerkleTree depth-32 inclusion proofs.
pub type MerklePath = [pallas::Base; 32];

/// Put parameters V3 — hard path with Merkle inclusion + nullifier for old state.
#[derive(Debug, Clone)]
pub struct PutParamsV3 {
    pub box_id: BoxId,
    pub old_state_nonce: pallas::Base,
    pub new_state_nonce: pallas::Base,
    pub old_contents_commit: pallas::Base,
    pub new_contents_commit: pallas::Base,
    pub owner: PublicKey,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for PutParamsV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutParamsV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutParamsV3 {
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let mut b = Vec::with_capacity(161 + path_bytes.len() + self.proof.len());
        b.extend_from_slice(&self.box_id.to_bytes());
        b.extend_from_slice(&self.old_state_nonce.to_repr());
        b.extend_from_slice(&self.new_state_nonce.to_repr());
        b.extend_from_slice(&self.old_contents_commit.to_repr());
        b.extend_from_slice(&self.new_contents_commit.to_repr());
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
        if data.len() < 161 + 32*32 { return Err(ContractError::IoError("PutParamsV3: too short".into())); }
        let box_id = BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("PutParamsV3: invalid box_id".into()))?;
        let old_state_nonce = read_base(&data[32..64])?;
        let new_state_nonce = read_base(&data[64..96])?;
        let old_contents_commit = read_base(&data[96..128])?;
        let new_contents_commit = read_base(&data[128..160])?;
        let owner = PublicKey::from_bytes(data[160..192].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PutParamsV3: invalid owner: {}", e)))?;
        let leaf_pos = u32::from_le_bytes(data[192..196].try_into().unwrap());
        let path_end = 196 + 32*32;
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 {
            merkle_path[i] = read_base(&data[196 + i*32 .. 196 + (i+1)*32])?;
        }
        let proof_len = data[path_end] as usize;
        let pos = path_end + 1;
        if data.len() < pos + proof_len + 64 { return Err(ContractError::IoError(format!("PutParamsV3: expected {} bytes, got {}", pos + proof_len + 64, data.len()))); }
        let proof = data[pos..pos+proof_len].to_vec();
        let pos2 = pos + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?;
        let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(PutParamsV3 { box_id, old_state_nonce, new_state_nonce, old_contents_commit, new_contents_commit, owner, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

/// Put update V3 — nullifier + new contents commit for Merkle tree append.
#[derive(Debug, Clone)]
pub struct PutUpdateV3 {
    pub nullifier: pallas::Base,
    pub new_contents_commit: pallas::Base,
}

impl dwow_serial::Encodable for PutUpdateV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PutUpdateV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PutUpdateV3 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.nullifier.to_repr()); b.extend_from_slice(&self.new_contents_commit.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("PutUpdateV3: expected 64 bytes, got {}", data.len()))); } Ok(PutUpdateV3 { nullifier: read_base(&data[0..32])?, new_contents_commit: read_base(&data[32..64])? }) }
}

/// Take parameters V3 — hard path with Merkle inclusion + nullifier for old state.
#[derive(Debug, Clone)]
pub struct TakeParamsV3 {
    pub box_id: BoxId,
    pub contents_commit: pallas::Base,
    pub state_nonce: pallas::Base,
    pub owner: PublicKey,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for TakeParamsV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeParamsV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeParamsV3 {
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let mut b = Vec::with_capacity(129 + path_bytes.len() + self.proof.len());
        b.extend_from_slice(&self.box_id.to_bytes());
        b.extend_from_slice(&self.contents_commit.to_repr());
        b.extend_from_slice(&self.state_nonce.to_repr());
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
        if data.len() < 129 + 32*32 { return Err(ContractError::IoError("TakeParamsV3: too short".into())); }
        let box_id = BoxId::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("TakeParamsV3: invalid box_id".into()))?;
        let contents_commit = read_base(&data[32..64])?;
        let state_nonce = read_base(&data[64..96])?;
        let owner = PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("TakeParamsV3: invalid owner: {}", e)))?;
        let leaf_pos = u32::from_le_bytes(data[128..132].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 {
            merkle_path[i] = read_base(&data[132 + i*32 .. 132 + (i+1)*32])?;
        }
        let proof_len_pos = 132 + 32*32;
        let proof_len = data[proof_len_pos] as usize;
        let pos = proof_len_pos + 1;
        if data.len() < pos + proof_len + 64 { return Err(ContractError::IoError(format!("TakeParamsV3: expected {} bytes, got {}", pos + proof_len + 64, data.len()))); }
        let proof = data[pos..pos+proof_len].to_vec();
        let pos2 = pos + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?;
        let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(TakeParamsV3 { box_id, contents_commit, state_nonce, owner, leaf_pos, merkle_path, proof, tx_binding, tx_nonce })
    }
}

/// Take update V3 — just a nullifier (Merkle tree append handled in apply).
#[derive(Debug, Clone)]
pub struct TakeUpdateV3 {
    pub nullifier: pallas::Base,
}

impl dwow_serial::Encodable for TakeUpdateV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for TakeUpdateV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl TakeUpdateV3 {
    pub fn encode(&self) -> Vec<u8> { self.nullifier.to_repr().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("TakeUpdateV3: expected 32 bytes, got {}", data.len()))); } Ok(TakeUpdateV3 { nullifier: read_base(&data[0..32])? }) }
}
