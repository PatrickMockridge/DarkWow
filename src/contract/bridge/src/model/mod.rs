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

//! Data structures for bridge contract calls
//!
//! Security Model: Object Capability Security (No VSS)

use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::pallas;
#[allow(unused_imports)]
use dwow_sdk::pasta::group::GroupEncoding;
use dwow_sdk::crypto::{IntentCommitment, IntentNullifier, PublicKey};

/// Deterministic bridge address: poseidon_hash(recipient_pub.xy(), nonce)
#[derive(Debug, Clone, Copy, PartialEq, Eq,)]
pub struct BridgeAddress(pallas::Base);
impl BridgeAddress {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(b: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(b).into_option().map(Self)
    }
}

/// Namespace for bridge intents (used with generic intent primitives)
pub const BRIDGE_NAMESPACE: u64 = 0x0002;

/// External chain identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChain {
    Ethereum,
    Monero,
    Zcash,
    Aztec,
    Litecoin,
    // Future chains can be added here
    // Bitcoin,
}

impl TryFrom<u8> for ExternalChain {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Ethereum),
            1 => Ok(Self::Monero),
            2 => Ok(Self::Zcash),
            3 => Ok(Self::Aztec),
            4 => Ok(Self::Litecoin),
            _ => Err(ContractError::IoError(
                format!("ExternalChain: unknown discriminant {}", b),
            )),
        }
    }
}

impl dwow_serial::Encodable for ExternalChain {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let b = *self as u8;
        w.write_all(&[b])?;
        Ok(1)
    }
}

impl dwow_serial::Decodable for ExternalChain {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 1];
        d.read_exact(&mut buf)?;
        Self::try_from(buf[0]).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncEncodable for ExternalChain {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let b = *self as u8;
        use dwow_serial::AsyncWriteExt;
        w.write_slice_async(&[b]).await?;
        Ok(1)
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncDecodable for ExternalChain {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 1];
        use dwow_serial::AsyncReadExt;
        d.read_slice_async(&mut buf).await?;
        Self::try_from(buf[0]).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

/// Chain-specific deposit proof data.
#[derive(Debug, Clone,)]
pub enum ExternalChainProof {
    Monero(XmrDepositProof),
    Zcash(ZcashDepositProof),
    Aztec(AztecDepositProof),
    Litecoin(LitecoinDepositProof),
    /// Ethereum deposits use the merkle_proof field on DepositParams
    /// instead of a chain-specific proof structure.
    Ethereum,
}

impl dwow_serial::Encodable for ExternalChainProof { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = match self { Self::Monero(p) => { let mut v = vec![0u8]; v.extend_from_slice(&dwow_serial::serialize(p)); v } Self::Zcash(p) => { let mut v = vec![1u8]; v.extend_from_slice(&dwow_serial::serialize(p)); v } Self::Aztec(p) => { let mut v = vec![2u8]; v.extend_from_slice(&dwow_serial::serialize(p)); v } Self::Litecoin(p) => { let mut v = vec![3u8]; v.extend_from_slice(&dwow_serial::serialize(p)); v } Self::Ethereum => vec![4u8], }; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ExternalChainProof { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut buf = vec![]; d.read_to_end(&mut buf)?; if buf.is_empty() { return Err(std::io::Error::other("ExternalChainProof: empty")); } Ok(match buf[0] { 0 => Self::Monero(dwow_serial::deserialize(&buf[1..]).map_err(|e| std::io::Error::other(format!("{:?}", e)))?), 1 => Self::Zcash(dwow_serial::deserialize(&buf[1..]).map_err(|e| std::io::Error::other(format!("{:?}", e)))?), 2 => Self::Aztec(dwow_serial::deserialize(&buf[1..]).map_err(|e| std::io::Error::other(format!("{:?}", e)))?), 3 => Self::Litecoin(dwow_serial::deserialize(&buf[1..]).map_err(|e| std::io::Error::other(format!("{:?}", e)))?), 4 => Self::Ethereum, _ => return Err(std::io::Error::other("ExternalChainProof: unknown variant")), }) } }

impl dwow_serial::Encodable for XmrDepositProof { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let dp = self.dleq_proof.encode(); let mut buf = Vec::with_capacity(57 + self.coinbase_merkle_proof.len()*32 + dp.len()); buf.extend_from_slice(&self.tx_hash); buf.extend_from_slice(&self.block_height.to_le_bytes()); buf.extend_from_slice(&self.output_index.to_le_bytes()); buf.extend_from_slice(&self.amount.to_le_bytes()); buf.extend_from_slice(&self.ephemeral_pub); buf.extend_from_slice(&dp); buf.push(self.coinbase_merkle_proof.len() as u8); for h in &self.coinbase_merkle_proof { buf.extend_from_slice(h); } buf.extend_from_slice(&self.confirmations.to_le_bytes()); w.write_all(&buf)?; Ok(buf.len()) } }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl dwow_serial::Decodable for XmrDepositProof { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut buf = vec![]; d.read_to_end(&mut buf)?; if buf.len() < 57 { return Err(std::io::Error::other("XmrDepositProof: too short")); } let tx_hash: [u8;32] = buf[0..32].try_into().map_err(|e| std::io::Error::other(format!("{:?}", e)))?; let block_height = u64::from_le_bytes(buf[32..40].try_into().unwrap()); let output_index = u64::from_le_bytes(buf[40..48].try_into().unwrap()); let amount = u64::from_le_bytes(buf[48..56].try_into().unwrap()); let ephemeral_pub: [u8;32] = buf[56..88].try_into().unwrap(); let dleq_proof = DleqProof::decode(&buf[88..184]).map_err(|e| std::io::Error::other(format!("{:?}", e)))?; let mp_count = buf[184] as usize; let mp_end = 185+mp_count*32; if buf.len() < mp_end+8 { return Err(std::io::Error::other("XmrDepositProof: truncated")); } let mut coinbase_merkle_proof = Vec::with_capacity(mp_count); for i in 0..mp_count { coinbase_merkle_proof.push(buf[185+i*32..185+(i+1)*32].try_into().unwrap()); } let confirmations = u64::from_le_bytes(buf[mp_end..mp_end+8].try_into().unwrap()); Ok(XmrDepositProof { tx_hash, block_height, output_index, amount, ephemeral_pub, dleq_proof, coinbase_merkle_proof, confirmations }) } }

impl dwow_serial::Encodable for ZcashDepositProof { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let mut buf = Vec::with_capacity(122 + self.merkle_path.len()*32 + self.spend_proof.len() + self.output_proof.len()); buf.extend_from_slice(&self.nullifier); buf.extend_from_slice(&self.commitment); buf.extend_from_slice(&self.anchor); buf.push(self.merkle_path.len() as u8); for h in &self.merkle_path { buf.extend_from_slice(h); } buf.push(self.spend_proof.len() as u8); buf.extend_from_slice(&self.spend_proof); buf.push(self.output_proof.len() as u8); buf.extend_from_slice(&self.output_proof); buf.extend_from_slice(&self.randomized_pub_key); buf.extend_from_slice(&self.randomness); buf.extend_from_slice(&self.amount.to_le_bytes()); buf.extend_from_slice(&self.block_height.to_le_bytes()); buf.extend_from_slice(&self.confirmations.to_le_bytes()); w.write_all(&buf)?; Ok(buf.len()) } }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl dwow_serial::Decodable for ZcashDepositProof { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut buf = vec![]; d.read_to_end(&mut buf)?; if buf.len() < 122 { return Err(std::io::Error::other("ZcashDepositProof: too short")); } let nullifier: [u8;32] = buf[0..32].try_into().unwrap(); let commitment: [u8;32] = buf[32..64].try_into().unwrap(); let anchor: [u8;32] = buf[64..96].try_into().unwrap(); let mp_count = buf[96] as usize; let mut pos = 97+mp_count*32; if buf.len() < pos+2 { return Err(std::io::Error::other("ZcashDepositProof: merkle_path truncated")); } let mut merkle_path = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_path.push(buf[97+i*32..97+(i+1)*32].try_into().unwrap()); } let sp_len = buf[pos] as usize; pos += 1; if buf.len() < pos+sp_len { return Err(std::io::Error::other("ZcashDepositProof: spend_proof truncated")); } let spend_proof = buf[pos..pos+sp_len].to_vec(); pos += sp_len; let op_len = buf[pos] as usize; pos += 1; if buf.len() < pos+op_len { return Err(std::io::Error::other("ZcashDepositProof: output_proof truncated")); } let output_proof = buf[pos..pos+op_len].to_vec(); pos += op_len; if buf.len() < pos+88 { return Err(std::io::Error::other("ZcashDepositProof: trailing truncated")); } let randomized_pub_key: [u8;32] = buf[pos..pos+32].try_into().unwrap(); pos += 32; let randomness: [u8;32] = buf[pos..pos+32].try_into().unwrap(); pos += 32; let amount = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); let block_height = u64::from_le_bytes(buf[pos+8..pos+16].try_into().unwrap()); let confirmations = u64::from_le_bytes(buf[pos+16..pos+24].try_into().unwrap()); Ok(ZcashDepositProof { nullifier, commitment, anchor, merkle_path, spend_proof, output_proof, randomized_pub_key, randomness, amount, block_height, confirmations }) } }

impl dwow_serial::Encodable for AztecDepositProof { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let mut buf = Vec::with_capacity(98 + self.merkle_path.len()*32 + self.proof_bytes.len()); buf.extend_from_slice(&self.nullifier); buf.extend_from_slice(&self.commitment); buf.extend_from_slice(&self.anchor); buf.push(self.merkle_path.len() as u8); for h in &self.merkle_path { buf.extend_from_slice(h); } buf.push(self.proof_bytes.len() as u8); buf.extend_from_slice(&self.proof_bytes); buf.extend_from_slice(&self.value.to_le_bytes()); buf.extend_from_slice(&self.asset_id.to_le_bytes()); buf.extend_from_slice(&self.rollup_height.to_le_bytes()); buf.extend_from_slice(&self.eth_block_height.to_le_bytes()); buf.extend_from_slice(&self.confirmations.to_le_bytes()); buf.extend_from_slice(&self.rollup_tx_hash); w.write_all(&buf)?; Ok(buf.len()) } }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl dwow_serial::Decodable for AztecDepositProof { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut buf = vec![]; d.read_to_end(&mut buf)?; if buf.len() < 97 { return Err(std::io::Error::other("AztecDepositProof: too short")); } let nullifier: [u8;32] = buf[0..32].try_into().unwrap(); let commitment: [u8;32] = buf[32..64].try_into().unwrap(); let anchor: [u8;32] = buf[64..96].try_into().unwrap(); let mp_count = buf[96] as usize; let mut pos = 97+mp_count*32; if buf.len() < pos+1 { return Err(std::io::Error::other("AztecDepositProof: merkle_path truncated")); } let mut merkle_path = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_path.push(buf[97+i*32..97+(i+1)*32].try_into().unwrap()); } let pb_len = buf[pos] as usize; pos += 1; if buf.len() < pos+pb_len+68 { return Err(std::io::Error::other("AztecDepositProof: truncated")); } let proof_bytes = buf[pos..pos+pb_len].to_vec(); pos += pb_len; let value = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); let asset_id = u32::from_le_bytes(buf[pos+8..pos+12].try_into().unwrap()); let rollup_height = u64::from_le_bytes(buf[pos+12..pos+20].try_into().unwrap()); let eth_block_height = u64::from_le_bytes(buf[pos+20..pos+28].try_into().unwrap()); let confirmations = u64::from_le_bytes(buf[pos+28..pos+36].try_into().unwrap()); let rollup_tx_hash: [u8;32] = buf[pos+36..pos+68].try_into().unwrap(); Ok(AztecDepositProof { nullifier, commitment, anchor, merkle_path, proof_bytes, value, asset_id, rollup_height, eth_block_height, confirmations, rollup_tx_hash }) } }

impl dwow_serial::Encodable for LitecoinDepositProof { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let rp = if let Some(ref v) = self.range_proof { v.clone() } else { vec![] }; let mut buf = Vec::with_capacity(52 + self.merkle_proof.len()*32 + rp.len()); buf.extend_from_slice(&self.tx_hash); buf.extend_from_slice(&self.output_index.to_le_bytes()); buf.extend_from_slice(&self.amount.to_le_bytes()); buf.push(self.merkle_proof.len() as u8); for h in &self.merkle_proof { buf.extend_from_slice(h); } buf.extend_from_slice(&self.block_merkle_root); buf.extend_from_slice(&self.block_height.to_le_bytes()); buf.extend_from_slice(&self.confirmations.to_le_bytes()); buf.push(self.confidential_commitment.is_some() as u8); if let Some(ref cc) = self.confidential_commitment { buf.extend_from_slice(cc); } buf.push(self.range_proof.is_some() as u8); buf.extend_from_slice(&rp); buf.push(self.is_confidential as u8); w.write_all(&buf)?; Ok(buf.len()) } }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl dwow_serial::Decodable for LitecoinDepositProof { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut buf = vec![]; d.read_to_end(&mut buf)?; if buf.len() < 52 { return Err(std::io::Error::other("LitecoinDepositProof: too short")); } let tx_hash: [u8;32] = buf[0..32].try_into().unwrap(); let output_index = u64::from_le_bytes(buf[32..40].try_into().unwrap()); let amount = u64::from_le_bytes(buf[40..48].try_into().unwrap()); let mp_count = buf[48] as usize; let mut pos = 49+mp_count*32; if buf.len() < pos+50 { return Err(std::io::Error::other("LitecoinDepositProof: truncated")); } let mut merkle_proof = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_proof.push(buf[49+i*32..49+(i+1)*32].try_into().unwrap()); } let block_merkle_root: [u8;32] = buf[pos..pos+32].try_into().unwrap(); pos += 32; let block_height = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); pos += 8; let confirmations = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); pos += 8; let has_cc = buf[pos] != 0; pos += 1; let confidential_commitment = if has_cc { let cc: [u8;32] = buf[pos..pos+32].try_into().unwrap(); pos += 32; Some(cc) } else { None }; let has_rp = buf[pos] != 0; pos += 1; let range_proof = if has_rp { let rp = buf[pos..].to_vec(); Some(rp) } else { None }; let is_confidential = if has_rp { true } else { buf[pos] != 0 }; Ok(LitecoinDepositProof { tx_hash, output_index, amount, merkle_proof, block_merkle_root, block_height, confirmations, confidential_commitment, range_proof, is_confidential }) } }

/// Bridge deposit parameters
#[derive(Debug, Clone,)]
pub struct DepositParams {
    /// Commitment hash from user's secret (uses generic PrivateIntent commitment)
    pub commitment: IntentCommitment,

    /// Recipient public key for address derivation
    pub recipient_pub: PublicKey,

    /// Nonce ensures fresh address per deposit (temporal privacy)
    pub bridge_nonce: u64,

    /// The external chain where the deposit was made
    pub chain: ExternalChain,

    /// Hash of the external block containing the deposit
    pub external_block_hash: [u8; 32],

    /// Merkle proof of deposit inclusion in external chain (Ethereum)
    pub merkle_proof: Vec<[u8; 32]>,

    /// Merkle root of external chain state at block
    pub external_state_root: [u8; 32],

    /// Bridge fee paid by depositor
    pub fee: u64,

    /// Amount deposited (smallest unit of the external chain asset)
    pub amount: u64,

    /// ZK proof demonstrating deposit validity
    pub proof: Vec<u8>,

    /// Chain-specific deposit proof (Monero, Zcash, Aztec, or Litecoin).
    pub chain_proof: ExternalChainProof,
}

impl dwow_serial::Encodable for DepositParams {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        let b = self.encode();
        w.write_all(&b)?;
        Ok(b.len())
    }
}
impl dwow_serial::Decodable for DepositParams {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let mut b = vec![];
        d.read_to_end(&mut b)?;
        Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

impl DepositParams {
    pub fn encode(&self) -> Vec<u8> {
        let cp_bytes = dwow_serial::serialize(&self.chain_proof);
        let mut b = Vec::with_capacity(107 + self.merkle_proof.len()*32 + self.proof.len() + cp_bytes.len());
        b.extend_from_slice(&self.commitment.to_bytes());
        b.extend_from_slice(&self.recipient_pub.to_bytes());
        b.extend_from_slice(&self.bridge_nonce.to_le_bytes());
        b.extend_from_slice(&(self.chain as u8).to_le_bytes());
        b.extend_from_slice(&self.external_block_hash);
        b.push(self.merkle_proof.len() as u8);
        for h in &self.merkle_proof { b.extend_from_slice(h); }
        b.extend_from_slice(&self.external_state_root);
        b.extend_from_slice(&self.fee.to_le_bytes());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&(cp_bytes.len() as u32).to_le_bytes());
        b.extend_from_slice(&cp_bytes);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 107 { return Err(ContractError::IoError("DepositParams: too short".into())); }
        let commitment = IntentCommitment::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("DepositParams: invalid commitment".into()))?;
        let recipient_pub = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositParams: invalid recipient_pub: {}", e)))?;
        let bridge_nonce = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let chain = ExternalChain::try_from(data[72]).map_err(|_| ContractError::IoError("DepositParams: invalid chain".into()))?;
        let external_block_hash: [u8;32] = data[73..105].try_into().unwrap();
        let mp_count = data[105] as usize; let mp_end = 106+mp_count*32;
        if data.len() < mp_end+32+8+1 { return Err(ContractError::IoError("DepositParams: truncated".into())); }
        let mut merkle_proof = Vec::with_capacity(mp_count);
        for i in 0..mp_count { merkle_proof.push(data[106+i*32..106+(i+1)*32].try_into().unwrap()); }
        let external_state_root: [u8;32] = data[mp_end..mp_end+32].try_into().unwrap();
        let fee = u64::from_le_bytes(data[mp_end+32..mp_end+40].try_into().unwrap());
        let proof_len = data[mp_end+40] as usize; let p = mp_end+41+proof_len;
        if data.len() < p+4 { return Err(ContractError::IoError("DepositParams: proof truncated".into())); }
        let proof = data[mp_end+41..p].to_vec();
        let cp_len = u32::from_le_bytes(data[p..p+4].try_into().unwrap()) as usize;
        if data.len() != p+4+cp_len+8 { return Err(ContractError::IoError(format!("DepositParams: expected {} bytes, got {}", p+4+cp_len+8, data.len()))); }
        let chain_proof = dwow_serial::deserialize(&data[p+4..p+4+cp_len]).map_err(|e| ContractError::IoError(format!("DepositParams: invalid chain_proof: {:?}", e)))?;
        let amount = u64::from_le_bytes(data[p+4+cp_len..p+4+cp_len+8].try_into().unwrap());
        Ok(DepositParams { commitment, recipient_pub, bridge_nonce, chain, external_block_hash, merkle_proof, external_state_root, fee, proof, amount, chain_proof })
    }
}

/// Bridge withdrawal parameters
#[derive(Debug, Clone,)]
pub struct WithdrawParams {
    /// Nullifier = H(secret) - proves deposit exists and hasn't been withdrawn
    pub nullifier: IntentNullifier,

    /// Recipient address hash on external chain
    pub recipient_hash: [u8; 32],

    /// Amount to withdraw
    pub amount: u64,

    /// ZK proof demonstrating withdrawal authorization
    pub proof: Vec<u8>,

    /// Bridge fee paid by withdrawer
    pub fee: u64,

    /// Block height after which the withdrawal can be cancelled if not executed
    pub timeout_height: u64,

    /// Feed mode: 0 = standard (fee only), 1 = guaranteed (fee + premium)
    pub feed_mode: u8,

    /// Optional user-specified max fee in basis points (0 = use contract default)
    pub max_fee_bp: Option<u64>,

    /// Token-aware minimum withdrawal amount (anti-dust)
    pub token_minimum: u64,
}

impl dwow_serial::Encodable for WithdrawParams {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        let b = self.encode();
        w.write_all(&b)?;
        Ok(b.len())
    }
}
impl dwow_serial::Decodable for WithdrawParams {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let mut b = vec![];
        d.read_to_end(&mut b)?;
        Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

impl WithdrawParams {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(102 + self.proof.len());
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.recipient_hash);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&(self.proof.len() as u32).to_le_bytes());
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.fee.to_le_bytes());
        b.extend_from_slice(&self.timeout_height.to_le_bytes());
        b.push(self.feed_mode);
        b.push(self.max_fee_bp.is_some() as u8);
        if let Some(v) = self.max_fee_bp { b.extend_from_slice(&v.to_le_bytes()); }
        b.extend_from_slice(&self.token_minimum.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 102 { return Err(ContractError::IoError("WithdrawParams: too short".into())); }
        let nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("WithdrawParams: invalid nullifier".into()))?;
        let recipient_hash: [u8;32] = data[32..64].try_into().unwrap();
        let amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let proof_len = u32::from_le_bytes(data[72..76].try_into().unwrap()) as usize; let p = 76+proof_len;
        if data.len() < p+8+8+1+1+8 { return Err(ContractError::IoError("WithdrawParams: proof truncated".into())); }
        let proof = data[76..p].to_vec();
        let fee = u64::from_le_bytes(data[p..p+8].try_into().unwrap());
        let timeout_height = u64::from_le_bytes(data[p+8..p+16].try_into().unwrap());
        let feed_mode = data[p+16];
        let has_mfb = data[p+17] != 0;
        let max_fee_bp = if has_mfb { if data.len() < p+26+8 { return Err(ContractError::IoError(format!("WithdrawParams: expected {} bytes, got {}", p+26+8, data.len()))); } Some(u64::from_le_bytes(data[p+18..p+26].try_into().unwrap())) } else { None };
        let token_pos = if has_mfb { p+26 } else { p+18 };
        let token_minimum = u64::from_le_bytes(data[token_pos..token_pos+8].try_into().unwrap());
        Ok(WithdrawParams { nullifier, recipient_hash, amount, proof, fee, timeout_height, feed_mode, max_fee_bp, token_minimum })
    }
}


/// Stored deposit record
#[derive(Debug, Clone)]
pub struct Deposit {
    pub version: u8,
    pub commitment: IntentCommitment,
    pub amount: u64,
    pub chain: ExternalChain,
    pub external_height: u64,
    pub claimed: bool,
    pub registered_at: u64,
}

/// Stored withdrawal record
#[derive(Debug, Clone)]
pub struct Withdrawal {
    pub version: u8,
    pub nullifier: IntentNullifier,
    pub recipient_hash: [u8; 32],
    pub amount: u64,
    pub executed: bool,
    pub external_tx_hash: Option<[u8; 32]>,
    pub withdrawn_at: u64,
}

// ================================================================
// XMR (MONERO) BRIDGING SUPPORT
// ================================================================

/// XMR deposit proof data for Monero bridging
#[derive(Debug, Clone,)]
pub struct XmrDepositProof {
    pub tx_hash: [u8; 32],
    pub block_height: u64,
    pub output_index: u64,
    pub amount: u64,
    pub ephemeral_pub: [u8; 32],
    pub dleq_proof: DleqProof,
    pub coinbase_merkle_proof: Vec<[u8; 32]>,
    pub confirmations: u64,
}

/// Discrete Logarithm Equality proof structure
#[derive(Debug, Clone,)]
pub struct DleqProof {
    pub challenge_response_1: [u8; 32],
    pub challenge_response_2: [u8; 32],
    pub challenge: [u8; 32],
}
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl DleqProof { pub const ENCODED_SIZE: usize = 96; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(96); b.extend_from_slice(&self.challenge_response_1); b.extend_from_slice(&self.challenge_response_2); b.extend_from_slice(&self.challenge); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 96 { return Err(ContractError::IoError(format!("DleqProof: expected 96 bytes, got {}", data.len()))); } Ok(DleqProof { challenge_response_1: data[0..32].try_into().unwrap(), challenge_response_2: data[32..64].try_into().unwrap(), challenge: data[64..96].try_into().unwrap() }) } }

// ================================================================
// ZCASH (SAPLING) BRIDGING SUPPORT
// ================================================================

/// Zcash Sapling deposit proof data
#[derive(Debug, Clone,)]
pub struct ZcashDepositProof {
    pub nullifier: [u8; 32],
    pub commitment: [u8; 32],
    pub anchor: [u8; 32],
    pub merkle_path: Vec<[u8; 32]>,
    pub spend_proof: Vec<u8>,
    pub output_proof: Vec<u8>,
    pub randomized_pub_key: [u8; 32],
    pub randomness: [u8; 32],
    pub amount: u64,
    pub block_height: u64,
    pub confirmations: u64,
}

/// Zcash withdrawal parameters
#[derive(Debug, Clone,)]
pub struct ZcashWithdrawParams {
    pub nullifier: IntentNullifier,
    pub recipient_hash: [u8; 32],
    pub is_shielded: bool,
    pub amount: u64,
    pub timeout_height: u64,
    pub proof: Vec<u8>,
}

// ================================================================
// AZTEC (PRIVATE ROLLUP) BRIDGING SUPPORT
// ================================================================

/// Aztec deposit proof data
#[derive(Debug, Clone,)]
pub struct AztecDepositProof {
    pub nullifier: [u8; 32],
    pub commitment: [u8; 32],
    pub anchor: [u8; 32],
    pub merkle_path: Vec<[u8; 32]>,
    pub proof_bytes: Vec<u8>,
    pub value: u64,
    pub asset_id: u32,
    pub rollup_height: u64,
    pub eth_block_height: u64,
    pub confirmations: u64,
    pub rollup_tx_hash: [u8; 32],
}

/// Aztec withdrawal parameters
#[derive(Debug, Clone,)]
pub struct AztecWithdrawParams {
    pub nullifier: IntentNullifier,
    pub recipient_hash: [u8; 32],
    pub amount: u64,
    pub asset_id: u32,
    pub timeout_height: u64,
    pub proof: Vec<u8>,
}

// ================================================================
// LITECOIN (TRANSPARENT + MIMBLEWIMBLE) BRIDGING SUPPORT
// ================================================================

/// Litecoin deposit proof data
#[derive(Debug, Clone,)]
pub struct LitecoinDepositProof {
    pub tx_hash: [u8; 32],
    pub output_index: u64,
    pub amount: u64,
    pub merkle_proof: Vec<[u8; 32]>,
    pub block_merkle_root: [u8; 32],
    pub block_height: u64,
    pub confirmations: u64,
    pub confidential_commitment: Option<[u8; 32]>,
    pub range_proof: Option<Vec<u8>>,
    pub is_confidential: bool,
}

/// Litecoin withdrawal parameters
#[derive(Debug, Clone,)]
pub struct LitecoinWithdrawParams {
    pub nullifier: IntentNullifier,
    pub recipient_hash: [u8; 32],
    pub is_mweb: bool,
    pub amount: u64,
    pub timeout_height: u64,
    pub proof: Vec<u8>,
}

/// XMR withdrawal parameters
#[derive(Debug, Clone,)]
pub struct XmrWithdrawParams {
    pub nullifier: IntentNullifier,
    pub recipient_hash: [u8; 32],
    pub amount: u64,
    pub timeout_height: u64,
    pub proof: Vec<u8>,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

impl Deposit {
    pub const ENCODED_SIZE: usize = 59;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(59);
        b.push(self.version);
        b.extend_from_slice(&self.commitment.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.chain as u8);
        b.extend_from_slice(&self.external_height.to_le_bytes());
        b.push(self.claimed as u8);
        b.extend_from_slice(&self.registered_at.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 59 { return Err(ContractError::IoError(format!("Deposit: expected 59 bytes, got {}", data.len()))); }
        Ok(Deposit { version: data[0], commitment: IntentCommitment::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Deposit: invalid commitment: {}", e)))?, amount: u64::from_le_bytes(data[33..41].try_into().unwrap()), chain: ExternalChain::try_from(data[41])?, external_height: u64::from_le_bytes(data[42..50].try_into().unwrap()), claimed: data[50] != 0, registered_at: u64::from_le_bytes(data[51..59].try_into().unwrap()) })
    }
}

impl Withdrawal {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 82 + if self.external_tx_hash.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.recipient_hash);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.executed as u8);
        b.push(self.external_tx_hash.is_some() as u8);
        if let Some(ref h) = self.external_tx_hash { b.extend_from_slice(h); }
        b.extend_from_slice(&self.withdrawn_at.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 82 { return Err(ContractError::IoError(format!("Withdrawal: expected at least 82 bytes, got {}", data.len()))); }
        let version = data[0];
        let nullifier = IntentNullifier::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Withdrawal: invalid nullifier: {}", e)))?;
        let recipient_hash: [u8; 32] = data[33..65].try_into().unwrap();
        let amount = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let executed = data[73] != 0;
        let has_tx = data[74] != 0;
        let (external_tx_hash, pos) = if has_tx {
            (Some(data[75..107].try_into().unwrap()), 107usize)
        } else { (None, 75usize) };
        let withdrawn_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        Ok(Withdrawal { version, nullifier, recipient_hash, amount, executed, external_tx_hash, withdrawn_at })
    }
}
