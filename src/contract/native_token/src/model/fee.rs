//! FeeV3 parameter types — public, plaintext fee payment.
//!
//! FeeV3 replaces FeeV2's privacy machinery (Pedersen `fee_value_commit`,
//! `FeeThreshold_V1` threshold proof, and `encrypted_fee_value` AEAD-to-miner)
//! with a **plaintext** fee. The wallet could not know the miner ahead of time,
//! so the encrypted-fee channel silently burned every production fee. The fee is
//! now `fee: FeeAmount` in the clear, with a three-tier priority selector.
//!
//! The `Fee_V2` mass-balance circuit (`fee.zk`) is retained — it still binds the
//! hidden input/output coin values to the now-public fee — via `FeeV2TxBinding`.
//!
//! Spec: fee-spec.md §12.4.

use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::blockchain::{FeeAmount, FeeTier};
use dwow_sdk::crypto::poseidon_hash;
use crate::error::NativeTokenError;
use dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING;
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::{group::GroupEncoding, pallas};

use super::{Input, Output};

// ============================================================
// §12.4 — Nominal tx_binding Type (retained for the Fee_V2 mass-balance proof)
// ============================================================

/// Tx binding for the retained Fee_V2 mass-balance proof (fee.zk).
///
/// Computed as `poseidon(DOMAIN_TX_BINDING=3, tx_commitment, tx_nonce)`.
/// Prevents Fee_V2 proof replay across different transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeV2TxBinding(pallas::Base);

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

// ============================================================

/// FeeV3 parameters — public, plaintext fee.
///
/// Differences from FeeV2:
/// - `fee_value_commit`/`fee_value_blind` (Pedersen) removed — `fee` is plaintext.
/// - `threshold_proof`/`threshold`/`ThresholdTxBinding` (FeeThreshold_V1) removed.
/// - `encrypted_fee_value` (AEAD-to-miner) removed.
/// - Adds `tier: FeeTier` — three-tier priority selector.
/// - `FeeV2TxBinding` retained for the mass-balance proof's anti-replay.
#[derive(Debug, Clone)]
pub struct FeeParamsV3 {
    pub input: Input,
    pub output: Output,
    /// Plaintext fee amount (wow).
    pub fee: FeeAmount,
    /// Three-tier priority selector (low/medium/high).
    pub tier: FeeTier,
    /// Pedersen commitment to the fee — KEPT so the host can verify the retained
    /// Fee_V2 mass-balance proof (whose public inputs include its coordinates).
    pub fee_value_commit: pallas::Point,
    /// Fee_V2 mass-balance proof tx_binding — poseidon(3, tx_commitment, tx_nonce).
    pub fee_v2_tx_binding: FeeV2TxBinding,
    pub tx_nonce: pallas::Base,
}

impl dwow_serial::Encodable for FeeParamsV3 {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        let b = self.encode();
        w.write_all(&b)?;
        Ok(b.len())
    }
}

impl dwow_serial::Decodable for FeeParamsV3 {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let mut b = vec![];
        d.read_to_end(&mut b)?;
        Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl FeeParamsV3 {
    pub fn encode(&self) -> Vec<u8> {
        let input_bytes = self.input.encode();
        let output_bytes = self.output.encode();
        let mut buf = Vec::with_capacity(input_bytes.len() + output_bytes.len() + 8 + 1 + 32 + 32 + 32);
        buf.extend_from_slice(&input_bytes);
        buf.extend_from_slice(&output_bytes);
        // fee: FeeAmount (8 bytes LE) — plaintext
        buf.extend_from_slice(&self.fee.to_le_bytes());
        // tier: u8 multiplier (1/2/4)
        buf.push(self.tier.tier_multiplier() as u8);
        // fee_value_commit: pallas::Point (32 bytes compressed) — kept for proof verification
        buf.extend_from_slice(&self.fee_value_commit.to_bytes());
        // fee_v2_tx_binding (32 bytes) + tx_nonce (32 bytes)
        buf.extend_from_slice(&self.fee_v2_tx_binding.inner().to_repr());
        buf.extend_from_slice(&self.tx_nonce.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        fn parse_err(_field: &str) -> ContractError {
            NativeTokenError::ParseError.into()
        }
        if data.len() < Input::ENCODED_SIZE + 130 {
            return Err(parse_err("FeeParamsV3: too short for input+output"));
        }
        let input = Input::decode(&data[..Input::ENCODED_SIZE])?;
        let input_len = Input::ENCODED_SIZE;
        let output_len = 130 + u16::from_le_bytes(
            data[input_len + 128..input_len + 130].try_into().unwrap()
        ) as usize;
        let output = Output::decode(&data[input_len..input_len + output_len])?;
        let mut pos = input_len + output_len;

        // fee: u64 LE (8 bytes)
        if data.len() < pos + 8 {
            return Err(parse_err("FeeParamsV3: too short for fee"));
        }
        let fee = FeeAmount::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // tier: u8 (1/2/4)
        if data.len() < pos + 1 {
            return Err(parse_err("FeeParamsV3: too short for tier"));
        }
        let tier = FeeTier::new(data[pos])
            .ok_or_else(|| parse_err("FeeParamsV3: invalid tier"))?;
        pos += 1;

        // fee_value_commit: pallas::Point (32 bytes compressed)
        if data.len() < pos + 32 {
            return Err(parse_err("FeeParamsV3: too short for fee_value_commit"));
        }
        let fee_value_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(&data[pos..pos + 32].try_into().unwrap())
        ).ok_or_else(|| parse_err("FeeParamsV3: invalid fee_value_commit"))?;
        pos += 32;

        // fee_v2_tx_binding (32 bytes) + tx_nonce (32 bytes)
        if data.len() < pos + 64 {
            return Err(parse_err("FeeParamsV3: too short for binding + nonce"));
        }
        let fee_v2_tx_binding = FeeV2TxBinding(Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[pos..pos + 32].try_into().unwrap())
        ).ok_or_else(|| parse_err("FeeParamsV3: invalid fee_v2_tx_binding"))?);
        let tx_nonce = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[pos + 32..pos + 64].try_into().unwrap())
        ).ok_or_else(|| parse_err("FeeParamsV3: invalid tx_nonce"))?;

        Ok(FeeParamsV3 {
            input,
            output,
            fee,
            tier,
            fee_value_commit,
            fee_v2_tx_binding,
            tx_nonce,
        })
    }
}
