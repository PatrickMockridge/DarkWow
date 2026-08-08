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

use std::{
    fmt::{self, Debug},
    str::FromStr,
};

use dwow_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
#[cfg(feature = "async")]
use dwow_serial::{AsyncDecodable, AsyncEncodable};

use super::{
    crypto::{ContractId, SecretKey},
    ContractError, GenericResult,
};
use crate::crypto::{DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, SerialEncodable, SerialDecodable)]
// We have to introduce a type rather than using an alias so we can implement Display
pub struct TransactionHash(pub [u8; 32]);

impl TransactionHash {
    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
    }

    pub fn none() -> Self {
        Self([0; 32])
    }

    #[inline]
    pub fn inner(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn as_string(&self) -> String {
        blake3::Hash::from_bytes(self.0).to_string()
    }
}

impl FromStr for TransactionHash {
    type Err = ContractError;

    fn from_str(tx_hash_str: &str) -> GenericResult<Self> {
        let Ok(hash) = blake3::Hash::from_str(tx_hash_str) else {
            return Err(ContractError::HexFmtErr);
        };
        Ok(Self(*hash.as_bytes()))
    }
}

impl fmt::Display for TransactionHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

// ANCHOR: contractcall
/// A ContractCall is the part of a transaction that executes a certain
/// `contract_id` with `data` as the call's payload.
#[derive(Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ContractCall {
    /// ID of the contract invoked
    pub contract_id: ContractId,
    /// Call data passed to the contract
    pub data: Vec<u8>,
}
// ANCHOR_END: contractcall

impl ContractCall {
    /// Returns true if call is a deployoor deployment.
    pub fn is_deployment(&self) -> bool {
        self.matches_contract_call_type(*DEPLOYOOOR_CONTRACT_ID, 0x00)
    }

    /// Returns true if call is a native token FeeV2.
    /// `[domain: mass_balance + fee_signalling]`
    /// Uses the nominal `MassBalanceFeeV2CallData` type — no raw `data[0]` inspection.
    pub fn is_mass_balance_fee_v2(&self) -> bool {
        self.as_mass_balance_fee_v2().is_some()
    }

    /// Returns true if call is a native token PoW reward.
    /// `[domain: mass_balance]` — block-opening coinbase nullifier claim.
    /// Uses the nominal `MassBalanceCoinbaseV1CallData` type — no raw `data[0]` inspection.
    pub fn is_mass_balance_coinbase_v1(&self) -> bool {
        self.as_mass_balance_coinbase_v1().is_some()
    }

    /// Returns true if call is a legacy native token fee (FeeV1 — removed).
    /// Selector 0x00. Kept for legacy test compatibility only.
    #[deprecated(note = "FeeV1 is removed. Use is_mass_balance_fee_v2() which checks FeeV2 (0x08).")]
    pub fn is_native_token_fee(&self) -> bool {
        self.matches_contract_call_type(*NATIVE_TOKEN_CONTRACT_ID, 0x00)
    }

    /// Returns true if call matches provided contract id and function code.
    /// Prefer the typed accessors `as_mass_balance_fee_v2()`, `as_mass_balance_coinbase_v1()`,
    /// `as_mass_balance_fee_collect_v1()` over this raw-byte method.
    pub fn matches_contract_call_type(&self, contract_id: ContractId, func_code: u8) -> bool {
        !self.data.is_empty() && self.contract_id == contract_id && self.data[0] == func_code
    }
}

// Avoid showing the data in the debug output since often the calldata is very long.
impl Debug for ContractCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContractCall(id={:?}", self.contract_id.inner())?;
        let calldata = &self.data;
        if !calldata.is_empty() {
            write!(f, ", function_code={}", calldata[0])?;
        }
        write!(f, ")")
    }
}

/// This is a wrapper around [`ContractCall`] that also adds secret keys that
/// should sign the entire transaction and any relevant ZK proofs.
/// This is normally created by external smart contract clients, and used by
/// the wallet when creating transactions.
#[derive(Clone, Eq, PartialEq)]
pub struct ContractCallImport {
    /// Single contract call
    call: ContractCall,
    /// ZK proofs used for the call
    proofs: Vec<Vec<u8>>,
    /// Secret keys used to sign the tx
    secrets: Vec<SecretKey>,
}

impl ContractCallImport {
    /// Create a new `ContractCallImport` given a call and secret keys
    pub fn new(call: ContractCall, proofs: Vec<Vec<u8>>, secrets: Vec<SecretKey>) -> Self {
        Self { call, proofs, secrets }
    }

    /// Reference the inner `ContractCall`
    pub fn call(&self) -> &ContractCall {
        &self.call
    }

    /// Reference the inner ZK proofs
    pub fn proofs(&self) -> &[Vec<u8>] {
        &self.proofs
    }

    /// Reference the inner secret keys
    pub fn secrets(&self) -> &[SecretKey] {
        &self.secrets
    }
}

/// IPC pipe serialization — delegates to SecretKey's explicit Encodable impl.
impl Encodable for ContractCallImport {
    fn encode<S: std::io::Write>(&self, s: &mut S) -> std::io::Result<usize> {
        let mut len = self.call.encode(s)?;
        len += self.proofs.encode(s)?;
        len += self.secrets.encode(s)?;
        Ok(len)
    }
}

/// IPC pipe deserialization — delegates to SecretKey's explicit Decodable impl.
impl Decodable for ContractCallImport {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let call = ContractCall::decode(d)?;
        let proofs = Vec::<Vec<u8>>::decode(d)?;
        let secrets = Vec::<SecretKey>::decode(d)?;
        Ok(ContractCallImport { call, proofs, secrets })
    }
}

/// Async IPC pipe serialization — delegates to field AsyncEncodable impls.
#[cfg(feature = "async")]
#[dwow_serial::async_trait]
impl AsyncEncodable for ContractCallImport {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(
        &self, w: &mut W,
    ) -> std::io::Result<usize> {
        let mut len = self.call.encode_async(w).await?;
        len += self.proofs.encode_async(w).await?;
        len += self.secrets.encode_async(w).await?;
        Ok(len)
    }
}

/// Async IPC pipe deserialization — delegates to field AsyncDecodable impls.
#[cfg(feature = "async")]
#[dwow_serial::async_trait]
impl AsyncDecodable for ContractCallImport {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(
        d: &mut D,
    ) -> std::io::Result<Self> {
        let call = ContractCall::decode_async(d).await?;
        let proofs = Vec::<Vec<u8>>::decode_async(d).await?;
        let secrets = Vec::<SecretKey>::decode_async(d).await?;
        Ok(ContractCallImport { call, proofs, secrets })
    }
}
