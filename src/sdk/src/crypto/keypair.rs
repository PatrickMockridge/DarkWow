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

use core::str::FromStr;

use dwow_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
#[cfg(feature = "async")]
use dwow_serial::{AsyncDecodable, AsyncEncodable};
use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{
        ff::{Field, PrimeField},
        Curve, Group, GroupEncoding,
    },
    pallas,
};
use rand_core::{CryptoRng, RngCore};

use super::{constants, constants::NullifierK, util::fp_mod_fv, poseidon_hash, ContractId};
use crate::error::ContractError;

/// Keypair structure holding a `SecretKey` and its respective `PublicKey`.
/// Copy removed per C3 — key material SHALL NOT be implicitly duplicated.
/// SerialEncodable/SerialDecodable removed per C2 (key-safety) — manual impls below
/// gate serialization behind explicit IPC-pipe-only WARNING documentation.
#[derive(Clone, PartialEq, Eq)]
pub struct Keypair {
    pub secret: SecretKey,
    pub public: PublicKey,
}

impl core::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("Keypair")
            .field("secret", &"<redacted>")
            .field("public", &self.public)
            .finish()
    }
}

impl Keypair {
    /// Instantiate a new `Keypair` given a `SecretKey`
    pub fn new(secret: SecretKey) -> Self {
        let public = PublicKey::from_secret(secret.clone());
        Self { secret, public }
    }

    /// Generate a new `Keypair` object given a source of randomness
    pub fn random(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        Self::new(SecretKey::random(rng))
    }
}

/// WARNING: Writes raw 32 secret bytes to the wire.
/// Only used for IPC pipe between client binary and wallet daemon.
/// Not for network/P2P/RPC — every call site must be audited.
impl Encodable for Keypair {
    fn encode<S: std::io::Write>(&self, s: &mut S) -> std::io::Result<usize> {
        let mut len = self.secret.encode(s)?;
        len += self.public.encode(s)?;
        Ok(len)
    }
}

/// Reads a full Keypair from wire format (32 secret bytes + 32 public key bytes).
/// Only used for IPC pipe between client binary and wallet daemon.
impl Decodable for Keypair {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let secret = SecretKey::decode(d)?;
        let public = PublicKey::decode(d)?;
        Ok(Keypair { secret, public })
    }
}

/// Async IPC pipe serialization — delegates to SecretKey/PublicKey AsyncEncodable.
/// See sync Encodable impl WARNING for security constraints.
#[cfg(feature = "async")]
#[dwow_serial::async_trait]
impl AsyncEncodable for Keypair {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(
        &self, w: &mut W,
    ) -> std::io::Result<usize> {
        let mut len = self.secret.encode_async(w).await?;
        len += self.public.encode_async(w).await?;
        Ok(len)
    }
}

/// Async IPC pipe deserialization — delegates to SecretKey/PublicKey AsyncDecodable.
#[cfg(feature = "async")]
#[dwow_serial::async_trait]
impl AsyncDecodable for Keypair {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(
        d: &mut D,
    ) -> std::io::Result<Self> {
        let secret = SecretKey::decode_async(d).await?;
        let public = PublicKey::decode_async(d).await?;
        Ok(Keypair { secret, public })
    }
}

// `impl Default for Keypair` (secret = 42) was removed as a key-safety measure.
// A `Default` identity key is a non-owner-declared, publicly-known secret that can
// be produced silently via `Default::default()`, `unwrap_or_default()`, derived
// `Default` on containing structs, or serde defaults — an unrepresentable-by-review
// hazard. Any site that needs a fixed key must construct it explicitly and visibly
// from a declared secret. The live genesis path uses `SecretKey::from(zero)`
// explicitly and never relied on this impl.

/// Structure holding a secret key, wrapping a `pallas::Base` element.
/// Copy removed per C3 — key material SHALL NOT be implicitly duplicated.
/// Drop zeroizes the inner field element's raw memory.
/// Closes: C3 (no secure memory for key material).
/// Enforces: type-system.md §5 (authority-through-possession — Copy
/// enables ambient duplication of authority).
// HAZOP C14 fix: Debug is removed from the derive — a manual impl below
// renders "<redacted>" to prevent accidental key leakage through formatting.
// Display (bs58-encoded full secret) is kept for intentional CLI key export.
// SerialEncodable/SerialDecodable removed per C2 (key-safety) — manual impls
// below gate serialization behind explicit IPC-pipe-only WARNING documentation.
// This breaks the auto-derive chain: any struct containing SecretKey can no
// longer silently derive Encodable — it must opt in explicitly.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey(pallas::Base);

impl core::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        // Zeroize the raw limb representation of the pallas::Base field
        // element, not just its byte repr (which would zeroize a copy).
        // write_bytes sets each byte to 0x00 — the compiler cannot
        // optimize this away because it goes through a raw pointer.
        unsafe {
            core::ptr::write_bytes(
                &mut self.0 as *mut pallas::Base as *mut u8,
                0,
                core::mem::size_of::<pallas::Base>(),
            );
        }
    }
}

impl SecretKey {
    /// Get a reference to the inner field element.
    /// Returns a reference (not a copy) — key material SHALL NOT be
    /// implicitly duplicated. Callers that need an owned value must
    /// explicitly dereference or clone.
    pub fn inner(&self) -> &pallas::Base {
        &self.0
    }

    /// Generate a new `SecretKey` given a source of randomness
    pub fn random(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        Self(pallas::Base::random(rng))
    }

    /// Instantiate a `SecretKey` given 32 bytes. Returns an error
    /// if the representation is noncanonical.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(bytes).into() {
            Some(k) => Ok(Self(k)),
            None => Err(ContractError::IoError("Could not convert bytes to SecretKey".to_string())),
        }
    }

    /// Derive a per-instance secret key.
    ///
    /// Produces a deterministic `SecretKey` unique to the tuple
    /// (wallet_secret, contract_id, instance_id). The same wallet
    /// using the same contract in different instances gets a different
    /// derived key, breaking cross-instance identity linking.
    ///
    /// Returns an error if `instance_id` encodes a non-canonical field element
    /// (value >= Pallas base field modulus). For typical callers using small
    /// instance IDs (block heights, counter values) this is unreachable.
    pub fn derive_instance(
        &self,
        contract_id: &ContractId,
        instance_id: &[u8],
    ) -> Result<Self, ContractError> {
        let mut id_bytes = [0u8; 32];
        let len = instance_id.len().min(32);
        id_bytes[..len].copy_from_slice(&instance_id[..len]);
        let instance_elem = match pallas::Base::from_repr(id_bytes).into_option() {
            Some(e) => e,
            None => {
                return Err(ContractError::IoError(
                    "Non-canonical instance_id in derive_instance".to_string(),
                ))
            }
        };

        let hash = poseidon_hash([constants::DRK_POSEIDON_DOMAIN_KEY_DERIVE, self.0, contract_id.inner(), instance_elem]);
        Ok(Self(hash))
    }
}

impl SecretKey {
    /// Construct a SecretKey from a pallas::Base field element.
    /// Named constructor per §8.5 — no From<pallas::Base> impl.
    /// Per §5: SecretKey exhibits ↓spend and ↓derive. Any field element
    /// can be a secret key — validation is at the constructor call site,
    /// not in the type conversion.
    pub fn from_base(x: pallas::Base) -> Self {
        Self(x)
    }
}

/// WARNING: Writes raw 32 secret bytes to the wire.
/// Only used for IPC pipe between client binary and wallet daemon.
/// Not for network/P2P/RPC — every call site must be audited via grep.
/// Prefer `inner()` (reference) for in-process crypto operations.
impl Encodable for SecretKey {
    fn encode<S: std::io::Write>(&self, s: &mut S) -> std::io::Result<usize> {
        // Delegate to pallas::Base's Encodable — writes 32 raw bytes.
        self.0.encode(s)
    }
}

/// Reads 32 bytes. Rejects non-canonical field elements.
/// Only used for IPC pipe between client binary and wallet daemon.
impl Decodable for SecretKey {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let inner = pallas::Base::decode(d)?;
        Ok(SecretKey(inner))
    }
}

/// Async IPC pipe serialization — delegates to pallas::Base's AsyncEncodable.
/// See sync Encodable impl WARNING for security constraints.
#[cfg(feature = "async")]
#[dwow_serial::async_trait]
impl AsyncEncodable for SecretKey {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(
        &self, w: &mut W,
    ) -> std::io::Result<usize> {
        self.0.encode_async(w).await
    }
}

/// Async IPC pipe deserialization — delegates to pallas::Base's AsyncDecodable.
#[cfg(feature = "async")]
#[dwow_serial::async_trait]
impl AsyncDecodable for SecretKey {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(
        d: &mut D,
    ) -> std::io::Result<Self> {
        let inner = pallas::Base::decode_async(d).await?;
        Ok(SecretKey(inner))
    }
}

impl FromStr for SecretKey {
    type Err = ContractError;

    /// Tries to create a `SecretKey` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding SecretKey from bytes, len is not 32".to_string(),
            ))
        }

        Self::from_bytes(decoded.try_into().unwrap())
    }
}

/// HAZOP C-11 fix: Display is gated behind `unsafe-display-secret` feature.
/// Only binaries that explicitly need key export (e.g., CLI wallet) should
/// enable this. Accidental `{}` formatting in production code is a compile error.
#[cfg(feature = "unsafe-display-secret")]
impl core::fmt::Display for SecretKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{disp}")
    }
}

/// Structure holding a public key, wrapping a `pallas::Point` element.
#[derive(Copy, Clone, PartialEq, Eq, Debug, SerialEncodable, SerialDecodable)]
pub struct PublicKey(pallas::Point);

impl PublicKey {
    /// Get the inner object wrapped by `PublicKey`
    pub fn inner(&self) -> pallas::Point {
        self.0
    }

    /// Derive a new `PublicKey` object given a `SecretKey`
    pub fn from_secret(s: SecretKey) -> Self {
        // spec dispensation: type-system.md §2.3 — base field < scalar field, conversion guaranteed valid.
        let scalar = fp_mod_fv(*s.inner())
            .expect("SecretKey to Scalar: mathematically guaranteed valid");
        let p = NullifierK.generator() * scalar;
        Self(p)
    }

    /// Instantiate a `PublicKey` given 32 bytes. Returns an error
    /// if the representation is noncanonical.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ContractError> {
        match <subtle::CtOption<pallas::Point> as Into<Option<pallas::Point>>>::into(
            pallas::Point::from_bytes(&bytes),
        ) {
            Some(k) => {
                if bool::from(k.is_identity()) {
                    return Err(ContractError::IoError(
                        "Could not convert bytes to PublicKey".to_string(),
                    ))
                }

                Ok(Self(k))
            }
            None => Err(ContractError::IoError("Could not convert bytes to PublicKey".to_string())),
        }
    }

    /// Downcast the `PublicKey` to 32 bytes of `pallas::Point`
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Fetch the `x` coordinate of this `PublicKey`.
    /// Returns `None` if the point is the identity element (which has
    /// no affine coordinates). This is unreachable for validated keys
    /// since `from_bytes` rejects identity.
    pub fn x(&self) -> Option<pallas::Base> {
        Option::from(self.0.to_affine().coordinates().map(|c| *c.x()))
    }

    /// Fetch the `y` coordinate of this `PublicKey`.
    /// Returns `None` if the point is the identity element.
    pub fn y(&self) -> Option<pallas::Base> {
        Option::from(self.0.to_affine().coordinates().map(|c| *c.y()))
    }

    /// Fetch the `x` and `y` coordinates of this `PublicKey` as a tuple.
    /// Returns `None` if the point is the identity element.
    pub fn xy(&self) -> Option<(pallas::Base, pallas::Base)> {
        Option::from(self.0.to_affine().coordinates().map(|c| (*c.x(), *c.y())))
    }
}

impl TryFrom<pallas::Point> for PublicKey {
    type Error = ContractError;

    fn try_from(x: pallas::Point) -> Result<Self, Self::Error> {
        if bool::from(x.is_identity()) {
            return Err(ContractError::IoError(
                "Could not convert identity point to PublicKey".to_string(),
            ))
        }

        Ok(Self(x))
    }
}

impl FromStr for PublicKey {
    type Err = ContractError;

    /// Tries to create a `PublicKey` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding PublicKey from bytes, len is not 32".to_string(),
            ))
        }

        Self::from_bytes(decoded.try_into().unwrap())
    }
}

impl core::fmt::Display for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_bytes()).into_string();
        write!(f, "{disp}")
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    pub fn is_testnet(self) -> bool {
        self == Network::Testnet
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AddressPrefix {
    MainnetStandard = 0x39,
    TestnetStandard = 0xaf,
}

impl AddressPrefix {
    pub fn network(&self) -> Network {
        match self {
            Self::MainnetStandard => Network::Mainnet,
            Self::TestnetStandard => Network::Testnet,
        }
    }
}

impl TryFrom<u8> for AddressPrefix {
    type Error = ContractError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x39 => Ok(Self::MainnetStandard),
            0xaf => Ok(Self::TestnetStandard),
            _ => Err(ContractError::IoError("Invalid address type".to_string())),
        }
    }
}

/// Defines a standard DarkWow pasta curve address containing prefix and pubkey.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StandardAddress {
    network: Network,
    spending_key: PublicKey,
}

impl StandardAddress {
    pub fn prefix(&self) -> AddressPrefix {
        match self.network {
            Network::Mainnet => AddressPrefix::MainnetStandard,
            Network::Testnet => AddressPrefix::TestnetStandard,
        }
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.spending_key
    }

    pub fn from_public(network: Network, public_key: PublicKey) -> Self {
        Self { network, spending_key: public_key }
    }
}

impl From<StandardAddress> for Address {
    fn from(v: StandardAddress) -> Self {
        Address::Standard(v)
    }
}

/// The address checksum is the first four bytes of the hashed data.
const ADDR_CHECKSUM_LEN: usize = 4;

/// Standard address consist of `[prefix][public_key][checksum]`.
const STANDARD_ADDR_LEN: usize = 1 + 32 + ADDR_CHECKSUM_LEN;

/// Addresses defined on DarkWow. Catch-all enum.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Address {
    Standard(StandardAddress),
}

impl Address {
    pub fn network(&self) -> Network {
        match self {
            Self::Standard(addr) => addr.network,
        }
    }

    pub fn public_key(&self) -> &PublicKey {
        match self {
            Self::Standard(addr) => addr.public_key(),
        }
    }
}

impl FromStr for Address {
    type Err = ContractError;

    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let dec = bs58::decode(enc).into_vec()?;
        if dec.is_empty() {
            return Err(ContractError::IoError("Empty address".to_string()))
        }

        let r_addrtype = AddressPrefix::try_from(dec[0])?;
        match r_addrtype {
            AddressPrefix::MainnetStandard | AddressPrefix::TestnetStandard => {
                // Standard addresses consist of [prefix][public_key][checksum].
                // Prefix is 1 byte, key is 32 bytes, and checksum is 4 bytes.
                // This should total to 37 bytes for standard addresses.
                if dec.len() != STANDARD_ADDR_LEN {
                    return Err(Self::Err::IoError("Invalid address length".to_string()))
                }

                let r_spending_key = PublicKey::from_bytes(
                    dec[1..STANDARD_ADDR_LEN - ADDR_CHECKSUM_LEN].try_into().unwrap(),
                )?;
                let r_checksum = &dec[STANDARD_ADDR_LEN - ADDR_CHECKSUM_LEN..];

                let checksum = blake3::hash(&dec[..STANDARD_ADDR_LEN - ADDR_CHECKSUM_LEN]);
                if r_checksum != &checksum.as_bytes()[..ADDR_CHECKSUM_LEN] {
                    return Err(Self::Err::IoError("Invalid address checksum".to_string()))
                }

                let addr =
                    StandardAddress { network: r_addrtype.network(), spending_key: r_spending_key };

                Ok(Self::Standard(addr))
            }
        }
    }
}

impl core::fmt::Display for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let payload = match self {
            Self::Standard(addr) => {
                let mut payload = Vec::with_capacity(STANDARD_ADDR_LEN);
                payload.push(addr.prefix() as u8);
                payload.extend_from_slice(&addr.spending_key.to_bytes());
                let checksum = blake3::hash(&payload);
                payload.extend_from_slice(&checksum.as_bytes()[..ADDR_CHECKSUM_LEN]);
                payload
            }
        };

        // DarkWow uses blake3 checksum inside payload — no redundant BTC base58check
        write!(f, "{}", bs58::encode(payload).into_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand::rngs::OsRng;

    #[test]
    fn test_standard_address_encoding() {
        let s_kp = Keypair::random(&mut OsRng);

        let s_addr = StandardAddress { network: Network::Mainnet, spending_key: s_kp.public };

        let addr: Address = s_addr.into();
        let encoded = addr.to_string();
        let decoded = Address::from_str(&encoded).unwrap();

        assert_eq!(addr, decoded);

        println!("{encoded}");
    }
}
