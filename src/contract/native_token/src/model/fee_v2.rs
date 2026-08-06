//! FeeV2 parameter types — privacy-preserving fee payment.
//!
//! FeeV2 replaces FeeV1's clear-text `fee: u64` with a Pedersen commitment
//! (`fee_value_commit: pallas::Point`) and a FeeThreshold_V1 proof
//! (`threshold_proof: Vec<u8>`). The fee amount is hidden from validators;
//! only the miner (who holds the witness decryption key) learns individual fees.
//!
//! Spec: fee-spec.md §5.

use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::crypto::{BaseBlind, Blind};
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::{group::GroupEncoding, pallas};

use super::{Input, Output};

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
    pub threshold_proof: Vec<u8>,
    pub fee_value_blind: pallas::Scalar,
    /// Fee token blind — typed BaseBlind per spec §8.1.
    pub fee_token_blind: BaseBlind,
    pub tx_binding: pallas::Base,
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
        let cap = input_bytes.len() + output_bytes.len() + 64 + 4 + proof_len as usize + 128;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&input_bytes);
        buf.extend_from_slice(&output_bytes);
        // fee_value_commit: pallas::Point (32 bytes compressed)
        buf.extend_from_slice(&self.fee_value_commit.to_bytes());
        // threshold_proof: length-prefixed bytes
        buf.extend_from_slice(&proof_len.to_le_bytes());
        buf.extend_from_slice(&self.threshold_proof);
        // blinds + bindings
        buf.extend_from_slice(&self.fee_value_blind.to_repr());
        buf.extend_from_slice(&self.fee_token_blind.inner().to_repr());
        buf.extend_from_slice(&self.tx_binding.to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let input = Input::decode(&data[..Input::ENCODED_SIZE])?;
        let input_len = Input::ENCODED_SIZE;
        if data.len() < input_len + 130 {
            return Err(ContractError::IoError(format!(
                "FeeParamsV2: expected at least {} bytes, got {}",
                input_len + 130, data.len()
            )));
        }
        let output_len = 130 + u16::from_le_bytes(
            data[input_len + 128..input_len + 130].try_into().unwrap()
        ) as usize;
        let output = Output::decode(&data[input_len..input_len + output_len])?;
        let mut pos = input_len + output_len;

        // fee_value_commit: pallas::Point (32 bytes compressed)
        if data.len() < pos + 32 {
            return Err(ContractError::IoError("FeeParamsV2: too short for fee_value_commit".into()));
        }
        let fee_value_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(&data[pos..pos + 32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("FeeParamsV2: invalid fee_value_commit".into()))?;
        pos += 32;

        // threshold_proof: length-prefixed bytes
        if data.len() < pos + 4 {
            return Err(ContractError::IoError("FeeParamsV2: too short for proof length".into()));
        }
        let proof_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if data.len() < pos + proof_len {
            return Err(ContractError::IoError("FeeParamsV2: too short for threshold_proof".into()));
        }
        let threshold_proof = data[pos..pos + proof_len].to_vec();
        pos += proof_len;

        // blinds + bindings (128 bytes: scalar 32 + blind 32 + binding 32 + nonce 32)
        if data.len() < pos + 128 {
            return Err(ContractError::IoError(format!(
                "FeeParamsV2: expected at least {} bytes, got {}",
                pos + 128, data.len()
            )));
        }
        let fee_value_blind = Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
            data[pos..pos + 32].try_into().unwrap()
        )).ok_or_else(|| ContractError::IoError("FeeParamsV2: invalid fee_value_blind".into()))?;
        let fee_token_blind = Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 32..pos + 64].try_into().unwrap()
        )).ok_or_else(|| ContractError::IoError("FeeParamsV2: invalid fee_token_blind".into()))?);
        let fee_token_blind: BaseBlind = fee_token_blind;
        let tx_binding = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 64..pos + 96].try_into().unwrap()
        )).ok_or_else(|| ContractError::IoError("FeeParamsV2: invalid tx_binding".into()))?;
        let tx_nonce = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[pos + 96..pos + 128].try_into().unwrap()
        )).ok_or_else(|| ContractError::IoError("FeeParamsV2: invalid tx_nonce".into()))?;

        Ok(FeeParamsV2 {
            input, output, fee_value_commit, threshold_proof,
            fee_value_blind, fee_token_blind, tx_binding, tx_nonce,
        })
    }
}
