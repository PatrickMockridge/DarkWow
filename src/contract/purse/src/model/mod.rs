use dwow_sdk::{crypto::pasta_prelude::PrimeField, error::ContractError, pasta::{group::GroupEncoding, pallas}};

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct PurseId(pub pallas::Base);
impl PurseId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: &[u8; 32]) -> Option<Self> { pallas::Base::from_repr(*bytes).into_option().map(PurseId) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("PurseId: expected 32 bytes, got {}", data.len()))); } Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("PurseId: invalid field element".into())) }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)] pub struct Nullifier(pub pallas::Base);
impl Nullifier {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ContractError> { match pallas::Base::from_repr(bytes).into() { Some(v) => Ok(Nullifier(v)), None => Err(ContractError::IoError("Nullifier: invalid field element".into())) } }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("Nullifier: expected 32 bytes, got {}", data.len()))); } Self::from_bytes(data.try_into().unwrap()) }
}

#[derive(Debug, Clone)] pub struct Purse { pub version: u8, pub purse_id: PurseId, pub token_commit: pallas::Base, pub balance_commit: pallas::Point, pub owner_commit: pallas::Base }
impl Purse {
    pub const ENCODED_SIZE: usize = 129;
    pub fn encode(&self) -> Vec<u8> { let mut b=Vec::with_capacity(129); b.push(self.version); b.extend_from_slice(&self.purse_id.to_bytes()); b.extend_from_slice(&self.token_commit.to_repr()); b.extend_from_slice(&self.balance_commit.to_bytes()); b.extend_from_slice(&self.owner_commit.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=129 { return Err(ContractError::IoError(format!("Purse: expected 129 bytes, got {}", data.len()))); } Ok(Purse{version:data[0],purse_id:PurseId::decode(&data[1..33])?,token_commit:Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(||ContractError::IoError("Purse: invalid token_commit".into()))?,balance_commit:Option::<pallas::Point>::from(pallas::Point::from_bytes(data[65..97].try_into().unwrap())).ok_or_else(||ContractError::IoError("Purse: invalid balance_commit".into()))?,owner_commit:Option::<pallas::Base>::from(pallas::Base::from_repr(data[97..129].try_into().unwrap())).ok_or_else(||ContractError::IoError("Purse: invalid owner_commit".into()))?}) }
}

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { if data.len()!=32 { return Err(ContractError::IoError(format!("read_base: expected 32 bytes, got {}", data.len()))); } Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(||ContractError::IoError("invalid base".into())) }
type MerklePath = [pallas::Base; 32];

// ============================================================================
// DEPOSIT — hdr=316
// ============================================================================

#[derive(Debug, Clone)] pub struct DepositParams {
    pub purse_id: PurseId, pub old_balance: u64, pub deposit_amount: u64, pub new_balance: u64,
    pub state_nonce: pallas::Base, pub nullifier: Nullifier, pub expected_root: pallas::Base, pub new_leaf: pallas::Base,
    pub old_commit_x: pallas::Base, pub old_commit_y: pallas::Base, pub new_commit_x: pallas::Base, pub new_commit_y: pallas::Base,
    pub leaf_pos: u32, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for DepositParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParams {
    pub fn encode(&self) -> Vec<u8> {
        let hdr=316usize; let pb:Vec<u8>=self.merkle_path.iter().flat_map(|b|b.to_repr()).collect();
        let mut b=Vec::with_capacity(hdr+pb.len()+1+self.proof.len()+64);
        b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.old_balance.to_le_bytes());
        b.extend_from_slice(&self.deposit_amount.to_le_bytes()); b.extend_from_slice(&self.new_balance.to_le_bytes());
        b.extend_from_slice(&self.state_nonce.to_repr()); b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.expected_root.to_repr()); b.extend_from_slice(&self.new_leaf.to_repr());
        b.extend_from_slice(&self.old_commit_x.to_repr()); b.extend_from_slice(&self.old_commit_y.to_repr());
        b.extend_from_slice(&self.new_commit_x.to_repr()); b.extend_from_slice(&self.new_commit_y.to_repr());
        b.extend_from_slice(&self.leaf_pos.to_le_bytes()); b.extend_from_slice(&pb);
        b.push(u8::try_from(self.proof.len()).map_err(|_|ContractError::IoError("proof too long".into())).unwrap());
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr=316usize; if data.len()<=hdr+1024usize { return Err(ContractError::IoError("DepositParams: too short".into())); }
        let pid=PurseId::decode(&data[0..32])?; let ob=u64::from_le_bytes(data[32..40].try_into().unwrap());
        let da=u64::from_le_bytes(data[40..48].try_into().unwrap()); let nb=u64::from_le_bytes(data[48..56].try_into().unwrap());
        let sn=read_base(&data[56..88])?; let nf=Nullifier::decode(&data[88..120])?;
        let er=read_base(&data[120..152])?; let nl=read_base(&data[152..184])?;
        let ocx=read_base(&data[184..216])?; let ocy=read_base(&data[216..248])?;
        let ncx=read_base(&data[248..280])?; let ncy=read_base(&data[280..312])?;
        let lp=u32::from_le_bytes(data[312..316].try_into().unwrap());
        let mut mp=[pallas::Base::zero();32]; for i in 0..32 { mp[i]=read_base(&data[hdr+i*32..hdr+(i+1)*32])?; }
        let pe=hdr+1024usize; let pl=usize::from(data[pe]);
        if data.len()<pe+1usize+pl+64usize { return Err(ContractError::IoError("DepositParams: too short for proof".into())); }
        let proof=data[pe+1..pe+1+pl].to_vec(); let p2=pe+1+pl;
        let tb=read_base(&data[p2..p2+32])?; let tn=read_base(&data[p2+32..p2+64])?;
        Ok(DepositParams{purse_id:pid,old_balance:ob,deposit_amount:da,new_balance:nb,state_nonce:sn,nullifier:nf,expected_root:er,new_leaf:nl,old_commit_x:ocx,old_commit_y:ocy,new_commit_x:ncx,new_commit_y:ncy,leaf_pos:lp,merkle_path:mp,proof,tx_binding:tb,tx_nonce:tn})
    }
}

#[derive(Debug, Clone)] pub struct DepositUpdate { pub nullifier: Nullifier, pub new_leaf: pallas::Base }
impl dwow_serial::Encodable for DepositUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdate { pub fn encode(&self) -> Vec<u8> { let mut v=Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_repr()); v } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=64 { return Err(ContractError::IoError("DepositUpdate: expected 64 bytes".into())); } Ok(DepositUpdate{nullifier:Nullifier::decode(&data[0..32])?, new_leaf:read_base(&data[32..64])?}) } }

// ============================================================================
// WITHDRAW
// ============================================================================

#[derive(Debug, Clone)] pub struct WithdrawParams {
    pub purse_id: PurseId, pub old_balance: u64, pub withdraw_amount: u64, pub new_balance: u64,
    pub state_nonce: pallas::Base, pub nullifier: Nullifier, pub expected_root: pallas::Base, pub new_leaf: pallas::Base,
    pub old_commit_x: pallas::Base, pub old_commit_y: pallas::Base, pub new_commit_x: pallas::Base, pub new_commit_y: pallas::Base,
    pub leaf_pos: u32, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl WithdrawParams { pub fn decode(data: &[u8]) -> Result<Self, ContractError> { let dp = DepositParams::decode(data)?; Ok(WithdrawParams{purse_id:dp.purse_id,old_balance:dp.old_balance,withdraw_amount:dp.deposit_amount,new_balance:dp.new_balance,state_nonce:dp.state_nonce,nullifier:dp.nullifier,expected_root:dp.expected_root,new_leaf:dp.new_leaf,old_commit_x:dp.old_commit_x,old_commit_y:dp.old_commit_y,new_commit_x:dp.new_commit_x,new_commit_y:dp.new_commit_y,leaf_pos:dp.leaf_pos,merkle_path:dp.merkle_path,proof:dp.proof,tx_binding:dp.tx_binding,tx_nonce:dp.tx_nonce}) } }
impl dwow_serial::Encodable for WithdrawParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = DepositParams{purse_id:self.purse_id,old_balance:self.old_balance,deposit_amount:self.withdraw_amount,new_balance:self.new_balance,state_nonce:self.state_nonce,nullifier:self.nullifier,expected_root:self.expected_root,new_leaf:self.new_leaf,old_commit_x:self.old_commit_x,old_commit_y:self.old_commit_y,new_commit_x:self.new_commit_x,new_commit_y:self.new_commit_y,leaf_pos:self.leaf_pos,merkle_path:self.merkle_path,proof:self.proof.clone(),tx_binding:self.tx_binding,tx_nonce:self.tx_nonce}.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

#[derive(Debug, Clone)] pub struct WithdrawUpdate { pub nullifier: Nullifier, pub new_leaf: pallas::Base }
impl dwow_serial::Encodable for WithdrawUpdate { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdate { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdate { pub fn encode(&self) -> Vec<u8> { let mut v=Vec::with_capacity(64); v.extend_from_slice(&self.nullifier.to_bytes()); v.extend_from_slice(&self.new_leaf.to_repr()); v } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len()!=64 { return Err(ContractError::IoError("WithdrawUpdate: expected 64 bytes".into())); } Ok(WithdrawUpdate{nullifier:Nullifier::decode(&data[0..32])?, new_leaf:read_base(&data[32..64])?}) } }

// ============================================================================
// BALANCE — hdr=268
// ============================================================================

#[derive(Debug, Clone)] pub struct BalanceParams {
    pub purse_id: PurseId, pub token_id: pallas::Base, pub balance: u64, pub state_nonce: pallas::Base,
    pub derived_purse_id: pallas::Base, pub expected_root: pallas::Base, pub token_commit: pallas::Base,
    pub balance_commit_x: pallas::Base, pub balance_commit_y: pallas::Base,
    pub leaf_pos: u32, pub merkle_path: MerklePath, pub proof: Vec<u8>, pub tx_binding: pallas::Base, pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for BalanceParams { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for BalanceParams { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl BalanceParams {
    pub fn encode(&self) -> Vec<u8> {
        let hdr=268usize; let pb:Vec<u8>=self.merkle_path.iter().flat_map(|b|b.to_repr()).collect();
        let mut b=Vec::with_capacity(hdr+pb.len()+1+self.proof.len()+64);
        b.extend_from_slice(&self.purse_id.encode()); b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.balance.to_le_bytes()); b.extend_from_slice(&self.state_nonce.to_repr());
        b.extend_from_slice(&self.derived_purse_id.to_repr()); b.extend_from_slice(&self.expected_root.to_repr());
        b.extend_from_slice(&self.token_commit.to_repr()); b.extend_from_slice(&self.balance_commit_x.to_repr());
        b.extend_from_slice(&self.balance_commit_y.to_repr()); b.extend_from_slice(&self.leaf_pos.to_le_bytes());
        b.extend_from_slice(&pb);
        b.push(u8::try_from(self.proof.len()).map_err(|_|ContractError::IoError("proof too long".into())).unwrap());
        b.extend_from_slice(&self.proof); b.extend_from_slice(&self.tx_binding.to_repr()); b.extend_from_slice(&self.tx_nonce.to_repr()); b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let hdr=268usize; if data.len()<=hdr+1024usize { return Err(ContractError::IoError("BalanceParams: too short".into())); }
        let pid=PurseId::decode(&data[0..32])?; let tid=read_base(&data[32..64])?;
        let bal=u64::from_le_bytes(data[64..72].try_into().unwrap()); let sn=read_base(&data[72..104])?;
        let dpi=read_base(&data[104..136])?; let er=read_base(&data[136..168])?; let tc=read_base(&data[168..200])?;
        let bcx=read_base(&data[200..232])?; let bcy=read_base(&data[232..264])?;
        let lp=u32::from_le_bytes(data[264..268].try_into().unwrap());
        let mut mp=[pallas::Base::zero();32]; for i in 0..32 { mp[i]=read_base(&data[hdr+i*32..hdr+(i+1)*32])?; }
        let pe=hdr+1024usize; let pl=usize::from(data[pe]);
        if data.len()<pe+1usize+pl+64usize { return Err(ContractError::IoError("BalanceParams: too short for proof".into())); }
        let proof=data[pe+1..pe+1+pl].to_vec(); let p2=pe+1+pl;
        let tb=read_base(&data[p2..p2+32])?; let tn=read_base(&data[p2+32..p2+64])?;
        Ok(BalanceParams{purse_id:pid,token_id:tid,balance:bal,state_nonce:sn,derived_purse_id:dpi,expected_root:er,token_commit:tc,balance_commit_x:bcx,balance_commit_y:bcy,leaf_pos:lp,merkle_path:mp,proof,tx_binding:tb,tx_nonce:tn})
    }
}
