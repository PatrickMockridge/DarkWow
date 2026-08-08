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

//! Nominal call data types for native token contract mass balance functions.
//!
//! # Process Engineering Context
//!
//! These types are the INSTRUMENTS on the transaction pipeline:
//!
//! | Type | Selector | Role | Analogy |
//! |------|----------|------|---------|
//! | `MassBalanceCoinbaseV1CallData` | `0x05` | Block-opening coinbase | Meter-open event — creates the coinbase UTXO, zeroes the totalizer |
//! | `MassBalanceFeeCollectV1CallData` | `0x06` | Fee accumulator reset | Meter-close event — reads totalizer, verifies, resets to Identity |
//! | `MassBalanceFeeV2CallData` | `0x08` | Hidden fee payment | Dual-domain instrument — carries `↓pay-fee` [mass_balance] for the meter AND `↓threshold-prove` [fee_signalling] for the valve |
//!
//! Domain annotations (`mass_balance`, `fee_signalling`) denote where these
//! types are verified. Mass balance types are verified during `accept_block`
//! (consensus-critical — meter fraud is hidden inflation). Fee signalling
//! types are verified at mempool admission (non-consensus — valve
//! misconfiguration degrades UX but cannot create money).
//!
//! See: `doc/src/arch/consensus/fee-spec.md §0.1` for the process engineering
//! analogy. See: `consensus.md §Supply Audit` for the flow meter specification.
//!
//! These types replace raw-byte dispatch (`data[0] == 0xNN`) with typed
//! accessors that carry domain-specific barbs at the type level.
//!
//! Domain annotations per fee-spec.md §0:
//! - `[domain: mass_balance]` — Consensus-critical Pedersen mass balance
//!   proof, verified during `accept_block`. Coinbase, fee collection.
//! - `[domain: mass_balance + fee_signalling]` — Dual-domain FeeV2:
//!   `↓pay-fee` is mass_balance, `↓threshold-prove` is fee_signalling.
//!
//! Spec: type-system.md §8.2, fee-spec.md §5.8, wallet.md §6.4.2.

use crate::crypto::NATIVE_TOKEN_CONTRACT_ID;

// ── Selector Witness Types ──────────────────────────────────────────────

/// Zero-sized witness type for the FeeV2 function selector (0x08).
///
/// This type exists solely to witness the `↓gate` barb at the type level.
/// It is constructible ONLY via `MassBalanceFeeV2Selector::new()` which hardcodes `0x08`.
/// No `From<u8>` impl exists — the selector is guaranteed by construction,
/// never recovered from `data[0]` at runtime.
///
/// Spec: type-system.md §8.2.3 (Dual-Domain — MassBalanceFeeV2Selector).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MassBalanceFeeV2Selector;

impl MassBalanceFeeV2Selector {
    /// The canonical selector byte for FeeV2.
    pub const SELECTOR: u8 = 0x08;

    /// Construct a MassBalanceFeeV2Selector. Always produces the canonical selector.
    /// No parameter — the value `0x08` is hardcoded.
    pub fn new() -> Self {
        Self
    }

    /// Returns the selector byte. Only needed at serialization boundaries
    /// per type-system.md §2.2.
    pub fn to_byte(self) -> u8 {
        Self::SELECTOR
    }
}

impl Default for MassBalanceFeeV2Selector {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-sized witness type for the PoWRewardV1 function selector (0x05).
/// `[domain: mass_balance]` — block-opening coinbase nullifier claim.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MassBalanceCoinbaseV1Selector;

impl MassBalanceCoinbaseV1Selector {
    pub const SELECTOR: u8 = 0x05;

    pub fn new() -> Self {
        Self
    }

    pub fn to_byte(self) -> u8 {
        Self::SELECTOR
    }
}

impl Default for MassBalanceCoinbaseV1Selector {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-sized witness type for the FeeCollectV1 function selector (0x06).
/// `[domain: mass_balance]` — fee accumulator verification + miner mint.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MassBalanceFeeCollectV1Selector;

impl MassBalanceFeeCollectV1Selector {
    pub const SELECTOR: u8 = 0x06;

    pub fn new() -> Self {
        Self
    }

    pub fn to_byte(self) -> u8 {
        Self::SELECTOR
    }
}

impl Default for MassBalanceFeeCollectV1Selector {
    fn default() -> Self {
        Self::new()
    }
}

// ── Nominal Call Data Types ─────────────────────────────────────────────

/// Nominal type for FeeV2 contract call data.
/// `[domain: mass_balance + fee_signalling]` — dual-domain.
///
/// Replaces the raw `Vec<u8>` pattern where callers prepend `0x08` and the
/// mempool matches on `data[0] == 0x08`. The type carries its own barbs:
/// `↓gate` (FeeV2 function), `↓pay-fee` [mass_balance] (Pedersen value conservation + nullifier),
/// `↓threshold-prove` [fee_signalling] (fee ≥ threshold ZK proof).
///
/// A process holding a `MassBalanceFeeV2CallData` is statically known to be on the FeeV2
/// path — no runtime byte matching.
///
/// Spec: fee-spec.md §5.8, type-system.md §8.2.3.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MassBalanceFeeV2CallData {
    /// The selector witness — guarantees `0x08` by construction.
    _selector: MassBalanceFeeV2Selector,
    /// The opaque encoded FeeParamsV2 payload (bytes AFTER the selector).
    /// Decoded by the contract crate's `FeeParamsV2::decode()`.
    params_bytes: Vec<u8>,
}

impl MassBalanceFeeV2CallData {
    /// Construct from pre-encoded FeeParamsV2 payload.
    ///
    /// The `params_bytes` are the output of `FeeParamsV2::encode()` —
    /// they do NOT include the selector byte. The selector is implicit
    /// in the TYPE.
    pub fn new(params_bytes: Vec<u8>) -> Self {
        Self {
            _selector: MassBalanceFeeV2Selector::new(),
            params_bytes,
        }
    }

    /// Absorber boundary: validate raw bytes and re-lift to the nominal type.
    ///
    /// Per type-system.md §10.5 obligation 1 (re-lift validation).
    /// Returns `None` if `data[0] != 0x08`. The caller SHALL NOT fall
    /// through to a FeeV2 path on `None`.
    ///
    /// Note: This validates the selector but does NOT decode FeeParamsV2 —
    /// that is done by the contract client crate when params are needed.
    /// This is intentional: the mempool can route on the selector without
    /// paying the full deserialization cost.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.first() != Some(&MassBalanceFeeV2Selector::SELECTOR) {
            return None;
        }
        Some(Self {
            _selector: MassBalanceFeeV2Selector::new(),
            params_bytes: data[1..].to_vec(),
        })
    }

    /// Encode for the contract call data buffer.
    ///
    /// Produces `[0x08][params_bytes]`. Only at persistence/wire boundaries
    /// per type-system.md §2.2. The byte sequence is identical to the
    /// pre-nominal encoding.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + self.params_bytes.len());
        v.push(MassBalanceFeeV2Selector::SELECTOR);
        v.extend_from_slice(&self.params_bytes);
        v
    }

    /// Access the raw params bytes (for FeeParamsV2::decode in the contract crate).
    pub fn params_bytes(&self) -> &[u8] {
        &self.params_bytes
    }
}

/// Nominal type for PoWRewardV1 contract call data.
/// `[domain: mass_balance]` — block-opening coinbase nullifier claim.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MassBalanceCoinbaseV1CallData {
    _selector: MassBalanceCoinbaseV1Selector,
    params_bytes: Vec<u8>,
}

impl MassBalanceCoinbaseV1CallData {
    pub fn new(params_bytes: Vec<u8>) -> Self {
        Self {
            _selector: MassBalanceCoinbaseV1Selector::new(),
            params_bytes,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.first() != Some(&MassBalanceCoinbaseV1Selector::SELECTOR) {
            return None;
        }
        Some(Self {
            _selector: MassBalanceCoinbaseV1Selector::new(),
            params_bytes: data[1..].to_vec(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + self.params_bytes.len());
        v.push(MassBalanceCoinbaseV1Selector::SELECTOR);
        v.extend_from_slice(&self.params_bytes);
        v
    }

    pub fn params_bytes(&self) -> &[u8] {
        &self.params_bytes
    }
}

/// Nominal type for FeeCollectV1 contract call data.
/// `[domain: mass_balance]` — fee accumulator verification + miner mint.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MassBalanceFeeCollectV1CallData {
    _selector: MassBalanceFeeCollectV1Selector,
    params_bytes: Vec<u8>,
}

impl MassBalanceFeeCollectV1CallData {
    pub fn new(params_bytes: Vec<u8>) -> Self {
        Self {
            _selector: MassBalanceFeeCollectV1Selector::new(),
            params_bytes,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.first() != Some(&MassBalanceFeeCollectV1Selector::SELECTOR) {
            return None;
        }
        Some(Self {
            _selector: MassBalanceFeeCollectV1Selector::new(),
            params_bytes: data[1..].to_vec(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + self.params_bytes.len());
        v.push(MassBalanceFeeCollectV1Selector::SELECTOR);
        v.extend_from_slice(&self.params_bytes);
        v
    }

    pub fn params_bytes(&self) -> &[u8] {
        &self.params_bytes
    }
}

// ── ContractCall Typed Accessors ────────────────────────────────────────

use crate::tx::ContractCall;

impl ContractCall {
    /// Attempt to decode this call as FeeV2 call data.
    /// `[domain: mass_balance + fee_signalling]`
    ///
    /// Returns `None` if the contract_id is not NATIVE_TOKEN_CONTRACT_ID
    /// or the selector byte is not `0x08`. This is the SINGLE site where
    /// FeeV2 dispatch is determined — all consumers use this method instead
    /// of inspecting `data[0]`.
    ///
    /// Replaces: `c.data.first() == Some(&0x08) && c.contract_id == NATIVE_TOKEN_CONTRACT_ID`
    pub fn as_mass_balance_fee_v2(&self) -> Option<MassBalanceFeeV2CallData> {
        if self.contract_id != *NATIVE_TOKEN_CONTRACT_ID {
            return None;
        }
        MassBalanceFeeV2CallData::from_bytes(&self.data)
    }

    /// Attempt to decode this call as PoWRewardV1 call data.
    /// `[domain: mass_balance]` — block-opening coinbase nullifier claim.
    ///
    /// Replaces: `c.data.first() == Some(&0x05) && c.contract_id == NATIVE_TOKEN_CONTRACT_ID`
    pub fn as_mass_balance_coinbase_v1(&self) -> Option<MassBalanceCoinbaseV1CallData> {
        if self.contract_id != *NATIVE_TOKEN_CONTRACT_ID {
            return None;
        }
        MassBalanceCoinbaseV1CallData::from_bytes(&self.data)
    }

    /// Attempt to decode this call as FeeCollectV1 call data.
    /// `[domain: mass_balance]` — fee accumulator verification + miner mint.
    ///
    /// Replaces: `c.data.first() == Some(&0x06) && c.contract_id == NATIVE_TOKEN_CONTRACT_ID`
    pub fn as_mass_balance_fee_collect_v1(&self) -> Option<MassBalanceFeeCollectV1CallData> {
        if self.contract_id != *NATIVE_TOKEN_CONTRACT_ID {
            return None;
        }
        MassBalanceFeeCollectV1CallData::from_bytes(&self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_v2_selector_byte() {
        assert_eq!(MassBalanceFeeV2Selector::SELECTOR, 0x08);
        assert_eq!(MassBalanceFeeV2Selector::new().to_byte(), 0x08);
    }

    #[test]
    fn test_fee_v2_call_data_roundtrip() {
        let params = vec![0xAA, 0xBB, 0xCC];
        let cd = MassBalanceFeeV2CallData::new(params.clone());
        let encoded = cd.encode();
        assert_eq!(encoded[0], 0x08);
        assert_eq!(&encoded[1..], &params);

        let decoded = MassBalanceFeeV2CallData::from_bytes(&encoded);
        assert!(decoded.is_some());
        assert_eq!(decoded.unwrap().params_bytes(), &params[..]);
    }

    #[test]
    fn test_fee_v2_call_data_rejects_wrong_selector() {
        let data = vec![0x09, 0xAA, 0xBB];
        assert!(MassBalanceFeeV2CallData::from_bytes(&data).is_none());
    }

    #[test]
    fn test_fee_v2_call_data_rejects_empty() {
        assert!(MassBalanceFeeV2CallData::from_bytes(&[]).is_none());
    }

    #[test]
    fn test_pow_reward_selector() {
        assert_eq!(MassBalanceCoinbaseV1Selector::SELECTOR, 0x05);
    }

    #[test]
    fn test_fee_collect_selector() {
        assert_eq!(MassBalanceFeeCollectV1Selector::SELECTOR, 0x06);
    }
}
