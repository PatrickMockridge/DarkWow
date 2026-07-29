use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, Nullifier, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

/// Purse unique identifier — Poseidon hash of owner and instance data.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PurseId(pub pallas::Base);

impl PurseId {
    pub const ENCODED_SIZE: usize = 32;
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(*bytes).into_option().map(PurseId)
    }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("PurseId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("PurseId: invalid".into()))
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
    /// Canonical byte size of an encoded Purse.
    pub const ENCODED_SIZE: usize = 129; // 1 + 32 + 32 + 32 + 32

    /// Encode to canonical fixed-offset bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.purse_id.to_bytes());
        buf.extend_from_slice(&self.token_commit.to_repr());
        buf.extend_from_slice(&self.balance_commit.to_bytes());
        buf.extend_from_slice(&self.owner_commit.to_repr());
        buf
    }

    /// Decode from canonical fixed-offset bytes (ρ-calculus: eval).
    /// Every field validates through its named constructor.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Purse: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let purse_id = PurseId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Purse: invalid purse_id".into()))?;
        let token_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[33..65].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Purse: invalid token_commit".into()))?;
        let balance_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(data[65..97].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Purse: invalid balance_commit".into()))?;
        let owner_commit = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[97..129].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Purse: invalid owner_commit".into()))?;
        Ok(Purse { version, purse_id, token_commit, balance_commit, owner_commit })
    }
}

/// Deposit parameters.
#[derive(Debug, Clone,)]
pub struct DepositParamsV1 {
    pub purse_id: PurseId,
    pub deposit_amount: u64,
    pub old_balance_commit: pallas::Point,
    pub new_balance_commit: pallas::Point,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for DepositParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(169+self.proof.len()); b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.deposit_amount.to_le_bytes()); b.extend_from_slice(&self.old_balance_commit.to_bytes()); b.extend_from_slice(&self.new_balance_commit.to_bytes()); b.extend_from_slice(&self.owner.to_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 169 { return Err(ContractError::IoError("DepositParamsV1: too short".into())); } let purse_id = PurseId::decode(&data[0..32])?; let deposit_amount = u64::from_le_bytes(data[32..40].try_into().unwrap()); let old_balance_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[40..72].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV1: invalid old_balance_commit".into()))?; let new_balance_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[72..104].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV1: invalid new_balance_commit".into()))?; let owner = PublicKey::from_bytes(data[104..136].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositParamsV1: invalid owner: {}", e)))?; let proof_len = data[136] as usize; if data.len() != 137+proof_len+64 { return Err(ContractError::IoError(format!("DepositParamsV1: expected {} bytes, got {}", 137+proof_len+64, data.len()))); } let proof = data[137..137+proof_len].to_vec(); let pos = 137+proof_len; let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV1: invalid tx_binding".into()))?; let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV1: invalid tx_nonce".into()))?; Ok(DepositParamsV1 { purse_id, deposit_amount, old_balance_commit, new_balance_commit, owner, proof, tx_binding, tx_nonce }) } }

/// Deposit update.
#[derive(Debug, Clone,)]
pub struct DepositUpdateV1 {
    pub purse_id: PurseId,
    pub new_balance_commit: pallas::Point,
    pub deposit_amount: u64,
}

impl dwow_serial::Encodable for DepositUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdateV1 { pub const ENCODED_SIZE: usize = 72; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(72); b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.new_balance_commit.to_bytes()); b.extend_from_slice(&self.deposit_amount.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 72 { return Err(ContractError::IoError(format!("DepositUpdateV1: expected 72 bytes, got {}", data.len()))); } Ok(DepositUpdateV1 { purse_id: PurseId::decode(&data[0..32])?, new_balance_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositUpdateV1: invalid new_balance_commit".into()))?, deposit_amount: u64::from_le_bytes(data[64..72].try_into().unwrap()) }) } }

/// Withdraw parameters.
#[derive(Debug, Clone,)]
pub struct WithdrawParamsV1 {
    pub purse_id: PurseId,
    pub withdraw_amount: u64,
    pub old_balance_commit: pallas::Point,
    pub new_balance_commit: pallas::Point,
    pub nullifier: Nullifier,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for WithdrawParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(169+self.proof.len()); b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.withdraw_amount.to_le_bytes()); b.extend_from_slice(&self.old_balance_commit.to_bytes()); b.extend_from_slice(&self.new_balance_commit.to_bytes()); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.owner.to_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 169 { return Err(ContractError::IoError("WithdrawParamsV1: too short".into())); } let purse_id = PurseId::decode(&data[0..32])?; let withdraw_amount = u64::from_le_bytes(data[32..40].try_into().unwrap()); let old_balance_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[40..72].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawParamsV1: invalid old_balance_commit".into()))?; let new_balance_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[72..104].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawParamsV1: invalid new_balance_commit".into()))?; let nullifier = Nullifier::from_bytes(data[104..136].try_into().unwrap())?; let owner = PublicKey::from_bytes(data[136..168].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("WithdrawParamsV1: invalid owner: {}", e)))?; let proof_len = data[168] as usize; if data.len() != 169+proof_len+64 { return Err(ContractError::IoError(format!("WithdrawParamsV1: expected {} bytes, got {}", 169+proof_len+64, data.len()))); } let proof = data[169..169+proof_len].to_vec(); let pos = 169+proof_len; let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawParamsV1: invalid tx_binding".into()))?; let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawParamsV1: invalid tx_nonce".into()))?; Ok(WithdrawParamsV1 { purse_id, withdraw_amount, old_balance_commit, new_balance_commit, nullifier, owner, proof, tx_binding, tx_nonce }) } }

/// Withdraw update.
#[derive(Debug, Clone,)]
pub struct WithdrawUpdateV1 {
    pub purse_id: PurseId,
    pub nullifier: Nullifier,
    pub new_balance_commit: pallas::Point,
    pub withdraw_amount: u64,
}

impl dwow_serial::Encodable for WithdrawUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdateV1 { pub const ENCODED_SIZE: usize = 104; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(104); b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.new_balance_commit.to_bytes()); b.extend_from_slice(&self.withdraw_amount.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 104 { return Err(ContractError::IoError(format!("WithdrawUpdateV1: expected 104 bytes, got {}", data.len()))); } Ok(WithdrawUpdateV1 { purse_id: PurseId::decode(&data[0..32])?, nullifier: Nullifier::from_bytes(data[32..64].try_into().unwrap())?, new_balance_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawUpdateV1: invalid new_balance_commit".into()))?, withdraw_amount: u64::from_le_bytes(data[96..104].try_into().unwrap()) }) } }

/// Balance parameters.
#[derive(Debug, Clone,)]
pub struct BalanceParamsV1 {
    pub purse_id: PurseId,
    pub token_id: pallas::Base,
    pub balance_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for BalanceParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BalanceParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl BalanceParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(161+self.proof.len()); b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.token_id.to_repr()); b.extend_from_slice(&self.balance_commit.to_bytes()); b.extend_from_slice(&self.token_commit.to_repr()); b.extend_from_slice(&self.owner.to_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 161 { return Err(ContractError::IoError("BalanceParamsV1: too short".into())); } let purse_id = PurseId::decode(&data[0..32])?; let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV1: invalid token_id".into()))?; let balance_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV1: invalid balance_commit".into()))?; let token_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[96..128].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV1: invalid token_commit".into()))?; let owner = PublicKey::from_bytes(data[128..160].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("BalanceParamsV1: invalid owner: {}", e)))?; let proof_len = data[160] as usize; if data.len() != 161+proof_len+64 { return Err(ContractError::IoError(format!("BalanceParamsV1: expected {} bytes, got {}", 161+proof_len+64, data.len()))); } let proof = data[161..161+proof_len].to_vec(); let pos = 161+proof_len; let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV1: invalid tx_binding".into()))?; let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV1: invalid tx_nonce".into()))?; Ok(BalanceParamsV1 { purse_id, token_id, balance_commit, token_commit, owner, proof, tx_binding, tx_nonce }) } }

// ============================================================================
// V3 TYPES — Hard path with Merkle inclusion proofs
// ============================================================================

type MerklePath = [pallas::Base; 32];

/// DepositParamsV3 — hard path Deposit with Merkle inclusion.
#[derive(Debug, Clone)]
pub struct DepositParamsV3 {
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

impl dwow_serial::Encodable for DepositParamsV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParamsV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParamsV3 {
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
        if data.len() < 137 + 32*32 { return Err(ContractError::IoError("DepositParamsV3: too short".into())); }
        let purse_id = PurseId::decode(&data[0..32])?;
        let old_balance = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let deposit_amount = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let new_balance = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let state_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[56..88].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV3: invalid state_nonce".into()))?;
        let leaf_pos = u32::from_le_bytes(data[88..92].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = Option::<pallas::Base>::from(pallas::Base::from_repr(data[92 + i*32 .. 92 + (i+1)*32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV3: invalid merkle_path entry".into()))?; }
        let path_end = 92 + 32*32;
        let owner = PublicKey::from_bytes(data[path_end..path_end+32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositParamsV3: invalid owner: {}", e)))?;
        let proof_len = data[path_end+32] as usize;
        let pos = path_end + 33;
        let proof = data[pos..pos+proof_len].to_vec();
        let pos2 = pos + proof_len;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos2..pos2+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV3: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos2+32..pos2+64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositParamsV3: invalid tx_nonce".into()))?;
        Ok(DepositParamsV3 { purse_id, old_balance, deposit_amount, new_balance, state_nonce, leaf_pos, merkle_path, owner, proof, tx_binding, tx_nonce })
    }
}

/// DepositUpdateV3 — nullifier for old state + new balance commitment.
#[derive(Debug, Clone)]
pub struct DepositUpdateV3 {
    pub nullifier: pallas::Base,
    pub new_balance_commit: pallas::Point,
}

impl dwow_serial::Encodable for DepositUpdateV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdateV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdateV3 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.nullifier.to_repr()); b.extend_from_slice(&self.new_balance_commit.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("DepositUpdateV3: expected 64 bytes, got {}", data.len()))); } Ok(DepositUpdateV3 { nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositUpdateV3: invalid nullifier".into()))?, new_balance_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DepositUpdateV3: invalid new_balance_commit".into()))? }) }
}

/// WithdrawParamsV3 — hard path Withdraw with Merkle inclusion.
#[derive(Debug, Clone)]
pub struct WithdrawParamsV3 {
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

impl dwow_serial::Encodable for WithdrawParamsV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParamsV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawParamsV3 {
    pub fn encode(&self) -> Vec<u8> { DepositParamsV3 { purse_id: self.purse_id, old_balance: self.old_balance, deposit_amount: self.withdraw_amount, new_balance: self.new_balance, state_nonce: self.state_nonce, leaf_pos: self.leaf_pos, merkle_path: self.merkle_path, owner: self.owner, proof: self.proof.clone(), tx_binding: self.tx_binding, tx_nonce: self.tx_nonce }.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { let d = DepositParamsV3::decode(data)?; Ok(WithdrawParamsV3 { purse_id: d.purse_id, old_balance: d.old_balance, withdraw_amount: d.deposit_amount, new_balance: d.new_balance, state_nonce: d.state_nonce, leaf_pos: d.leaf_pos, merkle_path: d.merkle_path, owner: d.owner, proof: d.proof, tx_binding: d.tx_binding, tx_nonce: d.tx_nonce }) }
}

/// WithdrawUpdateV3 — nullifier for consumed state.
#[derive(Debug, Clone)]
pub struct WithdrawUpdateV3 {
    pub nullifier: pallas::Base,
}

impl dwow_serial::Encodable for WithdrawUpdateV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdateV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdateV3 {
    pub fn encode(&self) -> Vec<u8> { self.nullifier.to_repr().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("WithdrawUpdateV3: expected 32 bytes, got {}", data.len()))); } Ok(WithdrawUpdateV3 { nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawUpdateV3: invalid nullifier".into()))? }) }
}

/// BalanceParamsV3 — hard path Balance with Merkle inclusion (read-only).
#[derive(Debug, Clone)]
pub struct BalanceParamsV3 {
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

impl dwow_serial::Encodable for BalanceParamsV3 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BalanceParamsV3 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl BalanceParamsV3 {
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
        if data.len() < 137 + 32*32 { return Err(ContractError::IoError("BalanceParamsV3: too short".into())); }
        let purse_id = PurseId::decode(&data[0..32])?;
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV3: invalid token_id".into()))?;
        let balance = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let state_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[72..104].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV3: invalid state_nonce".into()))?;
        let leaf_pos = u32::from_le_bytes(data[104..108].try_into().unwrap());
        let mut merkle_path = [pallas::Base::zero(); 32];
        for i in 0..32 { merkle_path[i] = Option::<pallas::Base>::from(pallas::Base::from_repr(data[108 + i*32 .. 108 + (i+1)*32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV3: invalid merkle_path entry".into()))?; }
        let path_end = 108 + 32*32;
        let owner = PublicKey::from_bytes(data[path_end..path_end+32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("BalanceParamsV3: invalid owner: {}", e)))?;
        let proof_len = data[path_end+32] as usize;
        let pos = path_end + 33;
        let proof = data[pos..pos+proof_len].to_vec();
        let pos2 = pos + proof_len;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos2..pos2+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV3: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos2+32..pos2+64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BalanceParamsV3: invalid tx_nonce".into()))?;
        Ok(BalanceParamsV3 { purse_id, token_id, balance, state_nonce, leaf_pos, merkle_path, owner, proof, tx_binding, tx_nonce })
    }
}
