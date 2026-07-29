use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

/// Purse unique identifier — Poseidon hash of owner and instance data.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PurseId(pub pallas::Base);

impl PurseId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(PurseId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("PurseId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("PurseId: invalid field element".into()))
    }
}

/// On-chain Purse record.
#[derive(Debug, Clone)]
pub struct Purse {
    pub version: u8,
    pub purse_id: PurseId,
    pub token_commit: pallas::Base,
    pub balance_commit: pallas::Point,
    pub owner_commit: pallas::Base,
}

impl Purse {
    pub const ENCODED_SIZE: usize = 129;
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(129);
        buf.push(self.version);
        buf.extend_from_slice(&self.purse_id.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.balance_commit.to_bytes());
        buf.extend_from_slice(&self.owner_commit.to_repr());
        buf
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 129 { return Err(ContractError::IoError(format!("Purse: expected 129 bytes, got {}", data.len()))); }
        let version = data[0];
        let purse_id = PurseId::decode(&data[1..33])?;
        let token_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Purse: invalid token_commit".into()))?;
        let balance_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[65..97].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Purse: invalid balance_commit".into()))?;
        let owner_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[97..129].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Purse: invalid owner_commit".into()))?;
        Ok(Purse { version, purse_id, token_commit, balance_commit, owner_commit })
    }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> {
    Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap()))
        .ok_or_else(|| ContractError::IoError("invalid base".into()))
}

type MerklePath = [pallas::Base; 32];

// ============================================================================
// DEPOSIT
// ============================================================================

#[derive(Debug, Clone)]
pub struct DepositParams {
    pub purse_id: PurseId,
    pub old_balance: u64,
    pub deposit_amount: u64,
    pub new_balance: u64,
    pub state_nonce: pallas::Base,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for DepositParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParams {
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let mut b = Vec::with_capacity(137 + path_bytes.len() + self.proof.len());
        b.extend_from_slice(&self.purse_id.encode());
        b.extend_from_slice(&self.old_balance.to_le_bytes());
        b.extend_from_slice(&self.deposit_amount.to_le_bytes());
        b.extend_from_slice(&self.new_balance.to_le_bytes());
        b.extend_from_slice(&self.state_nonce.to_repr());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        b.extend_from_slice(&self.owner.to_bytes());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.tx_binding.to_repr());
        b.extend_from_slice(&self.tx_nonce.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 137 + 32*32 { return Err(ContractError::IoError("DepositParams: too short".into())); }
        let purse_id = PurseId::decode(&data[0..32])?;
        let old_balance = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let deposit_amount = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let new_balance = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let state_nonce = read_base(&data[56..88])?;
        let leaf_pos = u32::from_le_bytes(data[88..92].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = read_base(&data[92 + i*32 .. 92 + (i+1)*32])?; }
        let path_end = 92 + 32*32;
        let owner = PublicKey::from_bytes(data[path_end..path_end+32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositParams: invalid owner: {}", e)))?;
        let proof_len = data[path_end+32] as usize;
        let pos = path_end + 33;
        let proof = data[pos..pos+proof_len].to_vec();
        let pos2 = pos + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?;
        let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(DepositParams { purse_id, old_balance, deposit_amount, new_balance, state_nonce, leaf_pos, merkle_path, owner, proof, tx_binding, tx_nonce })
    }
}

#[derive(Debug, Clone)]
pub struct DepositUpdate {
    pub nullifier: pallas::Base,
    pub new_balance_commit: pallas::Point,
}

impl dwow_serial::Encodable for DepositUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdate {
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.nullifier.to_repr()); b.extend_from_slice(&self.new_balance_commit.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 { return Err(ContractError::IoError(format!("DepositUpdate: expected 64 bytes, got {}", data.len()))); }
        Ok(DepositUpdate { nullifier: read_base(&data[0..32])?, new_balance_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositUpdate: invalid new_balance_commit".into()))? })
    }
}

// ============================================================================
// WITHDRAW
// ============================================================================

#[derive(Debug, Clone)]
pub struct WithdrawParams {
    pub purse_id: PurseId,
    pub old_balance: u64,
    pub withdraw_amount: u64,
    pub new_balance: u64,
    pub state_nonce: pallas::Base,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for WithdrawParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawParams {
    pub fn encode(&self) -> Vec<u8> { DepositParams { purse_id: self.purse_id, old_balance: self.old_balance, deposit_amount: self.withdraw_amount, new_balance: self.new_balance, state_nonce: self.state_nonce, leaf_pos: self.leaf_pos, merkle_path: self.merkle_path, owner: self.owner, proof: self.proof.clone(), tx_binding: self.tx_binding, tx_nonce: self.tx_nonce }.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { let d = DepositParams::decode(data)?; Ok(WithdrawParams { purse_id: d.purse_id, old_balance: d.old_balance, withdraw_amount: d.deposit_amount, new_balance: d.new_balance, state_nonce: d.state_nonce, leaf_pos: d.leaf_pos, merkle_path: d.merkle_path, owner: d.owner, proof: d.proof, tx_binding: d.tx_binding, tx_nonce: d.tx_nonce }) }
}

#[derive(Debug, Clone)]
pub struct WithdrawUpdate {
    pub nullifier: pallas::Base,
}

impl dwow_serial::Encodable for WithdrawUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdate {
    pub fn encode(&self) -> Vec<u8> { self.nullifier.to_repr().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("WithdrawUpdate: expected 32 bytes, got {}", data.len()))); }
        Ok(WithdrawUpdate { nullifier: read_base(&data[0..32])? })
    }
}

// ============================================================================
// BALANCE
// ============================================================================

#[derive(Debug, Clone)]
pub struct BalanceParams {
    pub purse_id: PurseId,
    pub token_id: pallas::Base,
    pub balance: u64,
    pub state_nonce: pallas::Base,
    pub leaf_pos: u32,
    pub merkle_path: MerklePath,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for BalanceParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BalanceParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl BalanceParams {
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes: Vec<u8> = self.merkle_path.iter().flat_map(|b| b.to_repr()).collect();
        let mut b = Vec::with_capacity(137 + path_bytes.len() + self.proof.len());
        b.extend_from_slice(&self.purse_id.encode());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.balance.to_le_bytes());
        b.extend_from_slice(&self.state_nonce.to_repr());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&path_bytes);
        b.extend_from_slice(&self.owner.to_bytes());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.tx_binding.to_repr());
        b.extend_from_slice(&self.tx_nonce.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 137 + 32*32 { return Err(ContractError::IoError("BalanceParams: too short".into())); }
        let purse_id = PurseId::decode(&data[0..32])?;
        let token_id = read_base(&data[32..64])?;
        let balance = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let state_nonce = read_base(&data[72..104])?;
        let leaf_pos = u32::from_le_bytes(data[104..108].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = read_base(&data[108 + i*32 .. 108 + (i+1)*32])?; }
        let path_end = 108 + 32*32;
        let owner = PublicKey::from_bytes(data[path_end..path_end+32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("BalanceParams: invalid owner: {}", e)))?;
        let proof_len = data[path_end+32] as usize;
        let pos = path_end + 33;
        let proof = data[pos..pos+proof_len].to_vec();
        let pos2 = pos + proof_len;
        let tx_binding = read_base(&data[pos2..pos2+32])?;
        let tx_nonce = read_base(&data[pos2+32..pos2+64])?;
        Ok(BalanceParams { purse_id, token_id, balance, state_nonce, leaf_pos, merkle_path, owner, proof, tx_binding, tx_nonce })
    }
}
