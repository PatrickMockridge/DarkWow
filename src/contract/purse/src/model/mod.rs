use crate::error::PurseError;
use dwow_sdk::{crypto::{pasta_prelude::PrimeField, MerkleNode, Nullifier}, error::ContractError, pasta::{group::GroupEncoding, pallas}};

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct PurseId(pub pallas::Base);
impl PurseId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> { pallas::Base::from_repr(*bytes).into_option().map(PurseId) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("PurseId: expected 32 bytes, got {}", data.len()))); } Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("PurseId: invalid field element".into())) }
}

/// Amount transferred in a single Purse operation.
/// Non-zero by construction — zero amounts are rejected at decode.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Amount(u64);
impl Amount {
    pub fn new(v: u64) -> Result<Self, ContractError> {
        if v == 0 { return Err(ContractError::IoError("Amount: zero not allowed".into())); }
        Ok(Self(v))
    }
    pub fn inner(&self) -> u64 { self.0 }
    pub fn to_le_bytes(&self) -> [u8; 8] { self.0.to_le_bytes() }
    pub fn from_le_bytes(b: [u8; 8]) -> Result<Self, ContractError> { Amount::new(u64::from_le_bytes(b)) }
}

/// Current balance of a Purse. Zero is valid (empty purse).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Balance(u64);
impl Balance {
    pub fn new(v: u64) -> Self { Self(v) }
    pub fn inner(&self) -> u64 { self.0 }
    pub fn to_le_bytes(&self) -> [u8; 8] { self.0.to_le_bytes() }
    pub fn from_le_bytes(b: [u8; 8]) -> Self { Self(u64::from_le_bytes(b)) }
}

fn read_merkle_node(data: &[u8]) -> Result<MerkleNode, ContractError> {
    if data.len() != 32 { return Err(ContractError::IoError(format!("read_merkle_node: expected 32 bytes, got {}", data.len()))); }
    let arr: [u8; 32] = data.try_into().map_err(|_| ContractError::IoError("read_merkle_node: slice conversion failed".into()))?;
    MerkleNode::from_bytes(arr).ok_or_else(|| ContractError::IoError("read_merkle_node: invalid MerkleNode".into()))
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

#[derive(Debug, Clone)] pub struct Purse { pub version: u8, pub purse_id: PurseId, pub token_commit: pallas::Base, pub balance_commit: pallas::Point, pub owner_commit: pallas::Base }
impl Purse {
    pub const ENCODED_SIZE: usize = 129;
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut b=Vec::with_capacity(129); b.push(self.version); b.extend_from_slice(&self.purse_id.to_bytes()); b.extend_from_slice(&self.token_commit.to_repr()); b.extend_from_slice(&self.balance_commit.to_bytes()); b.extend_from_slice(&self.owner_commit.to_repr()); Ok(b) }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=129 { return Err(ContractError::IoError(format!("Purse: expected 129 bytes, got {}", data.len()))); } Ok(Purse{version:data[0],purse_id:PurseId::decode(&data[1..33])?,token_commit:Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(||ContractError::IoError("Purse: invalid token_commit".into()))?,balance_commit:Option::<pallas::Point>::from(pallas::Point::from_bytes(data[65..97].try_into().unwrap())).ok_or_else(||ContractError::IoError("Purse: invalid balance_commit".into()))?,owner_commit:Option::<pallas::Base>::from(pallas::Base::from_repr(data[97..129].try_into().unwrap())).ok_or_else(||ContractError::IoError("Purse: invalid owner_commit".into()))?}) }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { if data.len()!=32 { return Err(ContractError::IoError(format!("read_base: expected 32 bytes, got {}", data.len()))); } Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(||ContractError::IoError("invalid base".into())) }
type MerklePath = [MerkleNode; 32];

// ============================================================================
// DEPOSIT — hdr=316
// ============================================================================

#[derive(Debug, Clone)] pub struct DepositParams {
    pub purse_id: PurseId, pub old_balance: Balance, pub deposit_amount: Amount, pub new_balance: Balance,
    pub state_nonce: StateNonce, pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub old_commit_x: pallas::Base, pub old_commit_y: pallas::Base, pub new_commit_x: pallas::Base, pub new_commit_y: pallas::Base,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for DepositParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let hdr=316usize; let pb:Vec<u8>=self.merkle_path.iter().flat_map(|n|n.to_bytes()).collect();
        let mut b=Vec::with_capacity(hdr+pb.len()+1+self.proof.len()+64);
        b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.old_balance.to_le_bytes());
        b.extend_from_slice(&self.deposit_amount.to_le_bytes()); b.extend_from_slice(&self.new_balance.to_le_bytes());
        b.extend_from_slice(&self.state_nonce.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_bytes()); b.extend_from_slice(&self.new_leaf.to_bytes());
        b.extend_from_slice(&self.old_commit_x.to_repr()); b.extend_from_slice(&self.old_commit_y.to_repr());
        b.extend_from_slice(&self.new_commit_x.to_repr()); b.extend_from_slice(&self.new_commit_y.to_repr());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes()); b.extend_from_slice(&pb);
        b.push(u8::try_from(self.proof.len()).map_err(|_|ContractError::IoError("proof too long".into()))?);
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr=316usize; if data.len()<=hdr+1024usize { return Err(PurseError::DecodeFailure{field:"DepositParams".into()}.into()); }
        let pid=PurseId::decode(&data[0..32])?;
        let ob=Balance::from_le_bytes(data[32..40].try_into().map_err(|_|ContractError::IoError("old_balance: wrong size".into()))?);
        let da=Amount::from_le_bytes(data[40..48].try_into().map_err(|_|ContractError::IoError("deposit_amount: wrong size".into()))?)?;
        let nb=Balance::from_le_bytes(data[48..56].try_into().map_err(|_|ContractError::IoError("new_balance: wrong size".into()))?);
        let sn=StateNonce::from_repr(data[56..88].try_into().map_err(|_|ContractError::IoError("state_nonce: wrong size".into()))?).ok_or_else(||ContractError::IoError("DepositParams: invalid state_nonce".into()))?;
        let nf={let a:[u8;32]=data[88..120].try_into().map_err(|_|ContractError::IoError("nullifier: wrong size".into()))?; Nullifier::from_bytes(a)?};
        let er=read_merkle_node(&data[120..152])?; let nl=read_merkle_node(&data[152..184])?;
        let ocx=read_base(&data[184..216])?; let ocy=read_base(&data[216..248])?;
        let ncx=read_base(&data[248..280])?; let ncy=read_base(&data[280..312])?;
        let lp=MerklePosition::from_le_bytes(data[312..316].try_into().map_err(|_|ContractError::IoError("leaf_pos: wrong size".into()))?);
        let mut mp=[MerkleNode::from_base(pallas::Base::zero());32]; for i in 0..32 { mp[i]=read_merkle_node(&data[hdr+i*32..hdr+(i+1)*32])?; }
        let pe=hdr+1024usize; let pl=usize::from(data[pe]);
        if data.len()<pe+1usize+pl+64usize { return Err(PurseError::DecodeFailure{field:"DepositParams".into()}.into()); }
        let proof=data[pe+1..pe+1+pl].to_vec(); let p2=pe+1+pl;
        let tb=read_base(&data[p2..p2+32])?; let tn=read_base(&data[p2+32..p2+64])?;
        Ok(DepositParams{purse_id:pid,old_balance:ob,deposit_amount:da,new_balance:nb,state_nonce:sn,nullifier:nf,expected_root:er,new_leaf:nl,old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:lp,merkle_path:mp,proof,tx_binding:tb,tx_nonce:tn})
    }
}

#[derive(Debug, Clone)] pub struct DepositUpdate { pub nullifier: Nullifier, pub new_leaf: MerkleNode }
impl dwow_serial::Encodable for DepositUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdate { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v=Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_bytes()); Ok(v) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=64 { return Err(PurseError::DecodeFailure{field:"DepositUpdate".into()}.into()); } Ok(DepositUpdate{nullifier:{let a:[u8;32]=data[0..32].try_into().map_err(|_|ContractError::IoError("nullifier: wrong size".into()))?; Nullifier::from_bytes(a)?}, new_leaf:read_merkle_node(&data[32..64])?}) } }

// ============================================================================
// WITHDRAW
// ============================================================================

#[derive(Debug, Clone)] pub struct WithdrawParams {
    pub purse_id: PurseId, pub old_balance: Balance, pub withdraw_amount: Amount, pub new_balance: Balance,
    pub state_nonce: StateNonce, pub nullifier: Nullifier, pub expected_root: MerkleNode, pub new_leaf: MerkleNode,
    pub old_commit_x: pallas::Base, pub old_commit_y: pallas::Base, pub new_commit_x: pallas::Base, pub new_commit_y: pallas::Base,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl WithdrawParams { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { DepositParams{purse_id:self.purse_id,old_balance:self.old_balance,deposit_amount:self.withdraw_amount,new_balance:self.new_balance,state_nonce:self.state_nonce,nullifier:self.nullifier,expected_root:self.expected_root,new_leaf:self.new_leaf,old_commit_x:self.old_commit_x,old_commit_y:self.old_commit_y,new_commit_x:self.new_commit_x,new_commit_y:self.new_commit_y,leaf_pos:self.leaf_pos,merkle_path:self.merkle_path,proof:self.proof.clone(),tx_binding:self.tx_binding,tx_nonce:self.tx_nonce}.encode() } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { let dp = DepositParams::decode(data)?; Ok(WithdrawParams{purse_id:dp.purse_id,old_balance:dp.old_balance,withdraw_amount:dp.deposit_amount,new_balance:dp.new_balance,state_nonce:dp.state_nonce,nullifier:dp.nullifier,expected_root:dp.expected_root,new_leaf:dp.new_leaf,old_commit_x:dp.old_commit_x,old_commit_y:dp.old_commit_y,new_commit_x:dp.new_commit_x,new_commit_y:dp.new_commit_y,leaf_pos:dp.leaf_pos,merkle_path:dp.merkle_path,proof:dp.proof,tx_binding:dp.tx_binding,tx_nonce:dp.tx_nonce}) } }
impl dwow_serial::Encodable for WithdrawParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = DepositParams{purse_id:self.purse_id,old_balance:self.old_balance,deposit_amount:self.withdraw_amount,new_balance:self.new_balance,state_nonce:self.state_nonce,nullifier:self.nullifier,expected_root:self.expected_root,new_leaf:self.new_leaf,old_commit_x:self.old_commit_x,old_commit_y:self.old_commit_y,new_commit_x:self.new_commit_x,new_commit_y:self.new_commit_y,leaf_pos:self.leaf_pos,merkle_path:self.merkle_path,proof:self.proof.clone(),tx_binding:self.tx_binding,tx_nonce:self.tx_nonce}.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

#[derive(Debug, Clone)] pub struct WithdrawUpdate { pub nullifier: Nullifier, pub new_leaf: MerkleNode }
impl dwow_serial::Encodable for WithdrawUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdate { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let mut v=Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_bytes()); Ok(v) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=64 { return Err(PurseError::DecodeFailure{field:"WithdrawUpdate".into()}.into()); } Ok(WithdrawUpdate{nullifier:{let a:[u8;32]=data[0..32].try_into().map_err(|_|ContractError::IoError("nullifier: wrong size".into()))?; Nullifier::from_bytes(a)?}, new_leaf:read_merkle_node(&data[32..64])?}) } }

// ============================================================================
// BALANCE — hdr=268
// ============================================================================

#[derive(Debug, Clone)] pub struct BalanceParams {
    pub purse_id: PurseId, pub token_id: pallas::Base, pub balance: Balance, pub state_nonce: StateNonce,
    pub derived_purse_id: pallas::Base, pub expected_root: MerkleNode, pub token_commit: pallas::Base,
    pub balance_commit_x: pallas::Base, pub balance_commit_y: pallas::Base,
    pub leaf_pos: MerklePosition, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for BalanceParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BalanceParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl BalanceParams {
    pub fn encode(&self) -> Result<Vec<u8>, ContractError> {
        let hdr=268usize; let pb:Vec<u8>=self.merkle_path.iter().flat_map(|n|n.to_bytes()).collect();
        let mut b=Vec::with_capacity(hdr+pb.len()+1+self.proof.len()+64);
        b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.balance.to_le_bytes()); b.extend_from_slice(&self.state_nonce.to_repr());
        b.extend_from_slice(&self.derived_purse_id.to_repr()); b.extend_from_slice(&self.expected_root.to_bytes());
        b.extend_from_slice(&self.token_commit.to_repr()); b.extend_from_slice(&self.balance_commit_x.to_repr());
        b.extend_from_slice(&self.balance_commit_y.to_repr()); b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&pb);
        b.push(u8::try_from(self.proof.len()).map_err(|_|ContractError::IoError("proof too long".into()))?);
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); Ok(b)
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr=268usize; if data.len()<=hdr+1024usize { return Err(PurseError::DecodeFailure{field:"BalanceParams".into()}.into()); }
        let pid=PurseId::decode(&data[0..32])?; let tid=read_base(&data[32..64])?;
        let bal=Balance::from_le_bytes(data[64..72].try_into().map_err(|_|ContractError::IoError("balance: wrong size".into()))?);
        let sn=StateNonce::from_repr(data[72..104].try_into().map_err(|_|ContractError::IoError("state_nonce: wrong size".into()))?).ok_or_else(||ContractError::IoError("BalanceParams: invalid state_nonce".into()))?;
        let dpi=read_base(&data[104..136])?; let er=read_merkle_node(&data[136..168])?; let tc=read_base(&data[168..200])?;
        let bcx=read_base(&data[200..232])?; let bcy=read_base(&data[232..264])?;
        let lp=MerklePosition::from_le_bytes(data[264..268].try_into().map_err(|_|ContractError::IoError("leaf_pos: wrong size".into()))?);
        let mut mp=[MerkleNode::from_base(pallas::Base::zero());32]; for i in 0..32 { mp[i]=read_merkle_node(&data[hdr+i*32..hdr+(i+1)*32])?; }
        let pe=hdr+1024usize; let pl=usize::from(data[pe]);
        if data.len()<pe+1usize+pl+64usize { return Err(PurseError::DecodeFailure{field:"BalanceParams".into()}.into()); }
        let proof=data[pe+1..pe+1+pl].to_vec(); let p2=pe+1+pl;
        let tb=read_base(&data[p2..p2+32])?; let tn=read_base(&data[p2+32..p2+64])?;
        Ok(BalanceParams{purse_id:pid,token_id:tid,balance:bal,state_nonce:sn,derived_purse_id:dpi,expected_root:er,token_commit:tc,balance_commit_x:bcx,balance_commit_y:bcy,leaf_pos:lp,merkle_path:mp,proof,tx_binding:tb,tx_nonce:tn})
    }
}
