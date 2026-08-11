//! FeeV2 parameter types — privacy-preserving fee payment.
//!
//! FeeV2 replaces FeeV1's clear-text `fee: u64` with a Pedersen commitment
//! (`fee_value_commit: pallas::Point`) and a FeeThreshold_V1 proof
//! (`threshold_proof: Vec<u8>`). The fee amount is hidden from validators;
//! only the miner (who holds the witness decryption key) learns individual fees.
//!
//! Spec: fee-spec.md §5.

use dwow_sdk::crypto::pasta_prelude::{Curve, CurveAffine, PrimeField};
use dwow_sdk::blockchain::FeeAmount;
use dwow_sdk::crypto::{BaseBlind, Blind, pedersen_commitment_u64};
use dwow_sdk::crypto::poseidon_hash;
use crate::error::NativeTokenError;
use dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING;
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::{group::GroupEncoding, pallas};

use super::{Input, Output};

// ============================================================
// §5.5.1 — Nominal tx_binding Types (Type Contract)
// ============================================================

/// Tx binding for Fee_V2 proof (fee.zk).
///
/// Computed as `poseidon(DOMAIN_TX_BINDING=3, tx_commitment, tx_nonce)`.
/// Prevents Fee_V2 proof replay across different transactions.
///
/// Per fee-spec.md §5.5.1: [
/// domain: mass_balance](https://docs.rs/poseidon/3, tx_commitment, tx_nonce)
///   purpose: anti-replay — binds Fee_V2 proof to a specific transaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeV2TxBinding(pallas::Base);

/// Tx binding for FeeThreshold_V1 proof (fee_threshold_v1.zk).
///
/// Computed as `poseidon(DOMAIN_TX_BINDING=3, tx_commitment, threshold)`.
/// Prevents replay of a premium-tier proof against the general threshold
/// (or vice versa).
///
/// Per fee-spec.md §5.5.1: [
/// domain: fee_signalling](https://docs.rs/poseidon/3, tx_commitment, threshold)
///   purpose: anti-replay — binds FeeThreshold_V1 proof to a specific threshold tier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThresholdTxBinding(pallas::Base);

impl FeeV2TxBinding {
    /// Compute the Fee_V2 tx binding from tx_commitment and tx_nonce.
    ///
    /// `poseidon(DRK_POSEIDON_DOMAIN_TX_BINDING=3, tx_commitment, tx_nonce)`
    pub fn compute(tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Self {
        Self(poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            tx_commitment,
            tx_nonce,
        ]))
    }

    /// Extract the inner `pallas::Base` value.
    /// Use this at ZK proof public-input boundaries only.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

impl ThresholdTxBinding {
    /// Compute the FeeThreshold_V1 tx binding from tx_commitment and threshold.
    ///
    /// `poseidon(DRK_POSEIDON_DOMAIN_TX_BINDING=3, tx_commitment, threshold)`
    pub fn compute(tx_commitment: pallas::Base, threshold: FeeAmount) -> Self {
        let threshold_base = pallas::Base::from(threshold.get());
        Self(poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            tx_commitment,
            threshold_base,
        ]))
    }

    /// Extract the inner `pallas::Base` value.
    /// Use this at ZK proof public-input boundaries only.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

impl dwow_serial::Encodable for FeeV2TxBinding {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        self.0.encode(w)
    }
}

impl dwow_serial::Decodable for FeeV2TxBinding {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let inner = pallas::Base::decode(d)?;
        Ok(Self(inner))
    }
}

impl dwow_serial::Encodable for ThresholdTxBinding {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        self.0.encode(w)
    }
}

impl dwow_serial::Decodable for ThresholdTxBinding {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let inner = pallas::Base::decode(d)?;
        Ok(Self(inner))
    }
}

// ============================================================

/// FeeV2 parameters — replaces FeeParamsV1 for privacy-preserving fees.
///
/// Key differences from FeeParamsV1:
/// - `fee: u64` replaced by `fee_value_commit: pallas::Point` (Pedersen commitment)
/// - Adds `threshold_proof: Vec<u8>` (FeeThreshold_V1 ZK proof)
/// - Same Input/Output/tx_binding/tx_nonce semantics
#[derive(Debug, Clone)]
pub struct FeeParamsV2 {
    pub input: Input,
    pub output: Output,
    pub fee_value_commit: pallas::Point,
    /// Fee_value_commit x-coordinate (convenience — extracted from fee_value_commit).
    pub fee_value_commit_x: pallas::Base,
    /// Fee_value_commit y-coordinate (convenience — extracted from fee_value_commit).
    pub fee_value_commit_y: pallas::Base,
    pub threshold_proof: Vec<u8>,
    /// Threshold used in the FeeThreshold_V1 proof (needed for metadata).
    pub threshold: FeeAmount,
    /// Fee amount encrypted to the block-producing miner's public key.
    /// AEAD ciphertext: [ephemeral_public (32B) || nonce (12B) || encrypted_blob (8B) || tag (16B)].
    /// Validators and mempool CANNOT decrypt — only the miner with the matching secret key.
    /// Per red-team guardrail G7: NO plaintext fee on-chain.
    pub encrypted_fee_value: Vec<u8>,
    pub fee_value_blind: pallas::Scalar,
    /// Fee token blind — typed BaseBlind per spec §8.1.
    pub fee_token_blind: BaseBlind,
    /// Fee_V2 proof tx_binding — poseidon(3, tx_commitment, tx_nonce).
    /// Anti-replay for the Fee_V2 proof's public inputs.
    /// [domain: mass_balance] per fee-spec.md §5.5.1.
    pub fee_v2_tx_binding: FeeV2TxBinding,
    /// FeeThreshold_V1 proof tx_binding — poseidon(3, tx_commitment, threshold).
    /// Anti-replay for the FeeThreshold_V1 proof's public inputs.
    /// [domain: fee_signalling] per fee-spec.md §5.5.1.
    pub threshold_tx_binding: ThresholdTxBinding,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for FeeParamsV2 {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        let b = self.encode();
        w.write_all(&b)?;
        Ok(b.len())
    }
}

impl dwow_serial::Decodable for FeeParamsV2 {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let mut b = vec![];
        d.read_to_end(&mut b)?;
        Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

impl FeeParamsV2 {
    pub fn encode(&self) -> Vec<u8> {
        let input_bytes = self.input.encode();
        let output_bytes = self.output.encode();
        let proof_len = self.threshold_proof.len() as u32;
        let cap = input_bytes.len() + output_bytes.len() + 72 + 4 + proof_len as usize
            + 4 + self.encrypted_fee_value.len() + 160;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&input_bytes);
        buf.extend_from_slice(&output_bytes);
        // fee_value_commit: pallas::Point (32 bytes compressed)
        buf.extend_from_slice(&self.fee_value_commit.to_bytes());
        // threshold: FeeAmount (8 bytes LE) — needed by mempool/metadata for FeeThreshold_V1
        buf.extend_from_slice(&self.threshold.to_le_bytes());
        // threshold_proof: length-prefixed bytes
        buf.extend_from_slice(&proof_len.to_le_bytes());
        buf.extend_from_slice(&self.threshold_proof);
        // encrypted_fee_value: length-prefixed AEAD ciphertext (4 + N bytes)
        let enc_len = self.encrypted_fee_value.len() as u32;
        buf.extend_from_slice(&enc_len.to_le_bytes());
        buf.extend_from_slice(&self.encrypted_fee_value);
        // blinds + bindings (160 bytes: scalar 32 + blind 32 + fee_v2_binding 32 + threshold_binding 32 + nonce 32)
        buf.extend_from_slice(&self.fee_value_blind.to_repr());
        buf.extend_from_slice(&self.fee_token_blind.inner().to_repr());
        buf.extend_from_slice(&self.fee_v2_tx_binding.inner().to_repr());
        buf.extend_from_slice(&self.threshold_tx_binding.inner().to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        fn parse_err(_field: &str) -> ContractError {
            // H4 fix: return spec error code Custom(2) = ParseError.
            // Diagnostic context is provided by the caller (fee_v2 exec)
            // which logs via msg!() before returning this error.
            NativeTokenError::ParseError.into()
        }
        if data.len() < Input::ENCODED_SIZE + 130 {
            return Err(parse_err("FeeParamsV2: too short for input+output"));
        }
        let input = Input::decode(&data[..Input::ENCODED_SIZE])?;
        let input_len = Input::ENCODED_SIZE;
        let output_len = 130 + u16::from_le_bytes(
            data[input_len + 128..input_len + 130].try_into().unwrap()
        ) as usize;
        let output = Output::decode(&data[input_len..input_len + output_len])?;
        let mut pos = input_len + output_len;

        // fee_value_commit: pallas::Point (32 bytes compressed)
        if data.len() < pos + 32 {
            return Err(parse_err("FeeParamsV2: too short for fee_value_commit"));
        }
        let fee_value_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(&data[pos..pos + 32].try_into().unwrap())
        ).ok_or_else(|| parse_err("FeeParamsV2: invalid fee_value_commit"))?;

        // Extract affine coordinates for metadata convenience
        let coords = fee_value_commit.to_affine().coordinates();
        let (fee_value_commit_x, fee_value_commit_y) = if coords.is_none().into() {
            return Err(parse_err("FeeParamsV2: fee_value_commit is identity"));
        } else {
            let c = coords.unwrap();
            (*c.x(), *c.y())
        };
        pos += 32;

        // threshold: u64 LE (8 bytes)
        if data.len() < pos + 8 {
            return Err(parse_err("FeeParamsV2: too short for threshold"));
        }
        let threshold = FeeAmount::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // threshold_proof: length-prefixed bytes
        if data.len() < pos + 4 {
            return Err(parse_err("FeeParamsV2: too short for proof length".into()));
        }
        let proof_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if data.len() < pos + proof_len {
            return Err(parse_err("FeeParamsV2: too short for threshold_proof".into()));
        }
        let threshold_proof = data[pos..pos + proof_len].to_vec();
        pos += proof_len;

        // encrypted_fee_value: length-prefixed AEAD ciphertext
        if data.len() < pos + 4 {
            return Err(parse_err("FeeParamsV2: too short for encrypted_fee_value length"));
        }
        let enc_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if data.len() < pos + enc_len {
            return Err(parse_err("FeeParamsV2: too short for encrypted_fee_value"));
        }
        let encrypted_fee_value = data[pos..pos + enc_len].to_vec();
        // FI-ENCRYPT-1: encrypted_fee_value SHALL NOT be empty.
        // AEAD ciphertext: [ephemeral_pk(32B)][nonce(12B)][ciphertext+tag(24B)] = 68 bytes.
        const MIN_AEAD_LEN: usize = 68;
        const MAX_AEAD_LEN: usize = 4096;
        if enc_len < MIN_AEAD_LEN {
            return Err(parse_err("FeeParamsV2: encrypted_fee_value too short"));
        }
        if enc_len > MAX_AEAD_LEN {
            return Err(parse_err("FeeParamsV2: encrypted_fee_value exceeds maximum"));
        }
        pos += enc_len;

        // blinds + bindings (160 bytes: scalar 32 + blind 32 + fee_v2_binding 32 + threshold_binding 32 + nonce 32)
        if data.len() < pos + 160 {
            return Err(parse_err("FeeParamsV2: too short for blinds"));
        }
        let fee_value_blind = Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
            data[pos..pos + 32].try_into().unwrap()
        )).ok_or_else(|| parse_err("FeeParamsV2: invalid fee_value_blind".into()))?;

        // Reject fee=0 with non-zero blind. Identity check above catches Pedersen(0,0),
        // but Pedersen(0, b!=0) would pass as a valid non-Identity point.
        if fee_value_blind != pallas::Scalar::zero() {
            let zero_commit = pedersen_commitment_u64(0, Blind(fee_value_blind));
            if zero_commit == fee_value_commit {
                return Err(parse_err("FeeParamsV2: zero-fee with non-zero blind rejected".into()));
            }
        }

        let fee_token_blind = Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 32..pos + 64].try_into().unwrap()
        )).ok_or_else(|| parse_err("FeeParamsV2: invalid fee_token_blind".into()))?);
        let fee_token_blind: BaseBlind = fee_token_blind;
        let fee_v2_tx_binding = FeeV2TxBinding(Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 64..pos + 96].try_into().unwrap()
        )).ok_or_else(|| parse_err("FeeParamsV2: invalid fee_v2_tx_binding".into()))?);
        let threshold_tx_binding = ThresholdTxBinding(Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 96..pos + 128].try_into().unwrap()
        )).ok_or_else(|| parse_err("FeeParamsV2: invalid threshold_tx_binding".into()))?);
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 128..pos + 160].try_into().unwrap()
        )).ok_or_else(|| parse_err("FeeParamsV2: invalid tx_nonce".into()))?;

        Ok(FeeParamsV2 {
            input, output, fee_value_commit,
            fee_value_commit_x, fee_value_commit_y,
            threshold, encrypted_fee_value, threshold_proof,
            fee_value_blind, fee_token_blind,
            fee_v2_tx_binding, threshold_tx_binding, tx_nonce,
        })
    }
}
