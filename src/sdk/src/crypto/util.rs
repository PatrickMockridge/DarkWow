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

use dwow_serial::ReadExt;
use halo2_gadgets::poseidon::primitives as poseidon;
use pasta_curves::{
    group::ff::{FromUniformBytes, PrimeField},
    pallas,
};
use std::io::Cursor;
use subtle::CtOption;

use crate::{
    error::{ContractError, GenericResult},
    hex::{decode_hex_arr, hex_from_iter},
};

#[inline]
fn hash_to_field_elem<F: FromUniformBytes<64>>(persona: &[u8], vals: &[&[u8]]) -> F {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).personal(persona).to_state();

    for v in vals {
        hasher.update(v);
    }

    F::from_uniform_bytes(hasher.finalize().as_array())
}

/// Hash a slice of values together with a prefix `persona` using BLAKE2b
/// and return a `pallas::Scalar` element from the digest.
pub fn hash_to_scalar(persona: &[u8], vals: &[&[u8]]) -> pallas::Scalar {
    hash_to_field_elem(persona, vals)
}

/// Hash a slice of values together with a prefix `persona` using BLAKE2b
/// and return a `pallas::Scalar` element from the digest.
pub fn hash_to_base(persona: &[u8], vals: &[&[u8]]) -> pallas::Base {
    hash_to_field_elem(persona, vals)
}

/// Persona for the transaction commitment's derivation. BLAKE2b bounds the
/// personalization at 16 bytes, which is why the `DRK_SCHNORR_*_DOMAIN`
/// constants beside this one are exactly that length.
pub const TX_COMMITMENT_PERSONA: &[u8] = b"DarkWow_TxCommit";

/// The transaction commitment, as the **field element** every circuit witnesses as
/// `tx_commitment`.
///
/// ## Why a field element rather than the blake3 digest
///
/// A circuit hashes this value with `poseidon_hash`, whose arguments are `pallas::Base`. A
/// 32-byte digest is not one: `pallas::Base::from_repr` rejects roughly three of every four
/// uniform 32-byte strings as non-canonical, so *some* map from bytes to field was always
/// required and none existed. The wallet filled the gap by substituting a seed-derived
/// **random** field element in each proof-building path, which is why no proof in this tree
/// has ever been bound to a real transaction — the value it committed to was invented at
/// the prover.
///
/// ## One derivation, one home
///
/// `safety.md` RC5, and the lesson `OBL-C104` records: the transaction builder, the host
/// runtime that exposes this to a contract, and the node's verifier all call **this**
/// function. A second derivation anywhere would be a second value, and the two would drift
/// silently — the failure mode presenting as a proof that does not verify rather than as a
/// disagreement about a hash.
///
/// ## The encoding
///
/// Each call is `dwow_serial`-encoded, and those encodings are **self-delimiting** (a
/// `ContractCall` is a fixed 32-byte id plus a length-prefixed calldata), so concatenating
/// them cannot be rearranged into a different call set with the same digest. Proofs are
/// excluded deliberately: they are created after this value is known, so including them
/// would be circular.
pub fn tx_commitment<'a>(
    calls: impl IntoIterator<Item = &'a crate::tx::ContractCall>,
) -> pallas::Base {
    let encoded: Vec<Vec<u8>> = calls.into_iter().map(dwow_serial::serialize).collect();
    let refs: Vec<&[u8]> = encoded.iter().map(Vec::as_slice).collect();
    hash_to_base(TX_COMMITMENT_PERSONA, &refs)
}

/// Converts from pallas::Base to pallas::Scalar (aka $x \pmod{r_\mathbb{P}}$).
///
/// This requires no modular reduction because Pallas' base field is smaller than its
/// scalar field. The conversion is mathematically guaranteed to succeed for all valid
/// `pallas::Base` elements — `from_repr` failure would indicate memory corruption,
/// not a normal operational condition.
///
/// Returns `Err` rather than panicking per type-system.md §2.3.2 (no `unwrap()` on
/// consensus-critical paths).
pub fn fp_mod_fv(val: pallas::Base) -> GenericResult<pallas::Scalar> {
    Ok(Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(val.to_repr()))
        .ok_or_else(|| ContractError::IoError("Base field element out of scalar range".into()))?)
}

/// Converts from pallas::Scalar to pallas::Base (aka $x \pmod{r_\mathbb{P}}$).
///
/// This call is unsafe and liable to fail. Use with caution.
/// The Pallas scalar field is bigger than the field we're converting to here.
pub fn fv_mod_fp_unsafe(val: pallas::Scalar) -> CtOption<pallas::Base> {
    pallas::Base::from_repr(val.to_repr())
}

/// Wrapper around poseidon in `halo2_gadgets`
pub fn poseidon_hash<const N: usize>(messages: [pallas::Base; N]) -> pallas::Base {
    // TODO: it's possible to make this function simply take a slice, by using the lower level
    // sponge defined in halo2 lib. Simply look how the function hash() is defined.
    // Why is this needed? Simply put we are often working with dynamic data such as Python
    // or with other interpreted environments. We don't always know the length of input data
    // at compile time.
    poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<N>, 3, 2>::init()
        .hash(messages)
}

pub fn fp_to_u64(value: pallas::Base) -> Option<u64> {
    let repr = value.to_repr();
    if !repr[8..].iter().all(|&b| b == 0u8) {
        return None
    }
    let mut cur = Cursor::new(&repr[0..8]);
    let uint = ReadExt::read_u64(&mut cur).ok()?;
    Some(uint)
}

// Not allowed to implement external traits for external crates
pub trait FieldElemAsStr: PrimeField<Repr = [u8; 32]> {
    fn to_string(&self) -> String {
        // We reverse repr since it is little endian encoded
        "0x".to_string() + &hex_from_iter(self.to_repr().iter().cloned().rev())
    }

    fn from_str(hex: &str) -> GenericResult<Self> {
        if hex.len() != 33 * 2 {
            return Err(ContractError::HexFmtErr)
        }

        let hex = hex.strip_prefix("0x").ok_or(ContractError::HexFmtErr)?;

        let mut bytes = decode_hex_arr(hex)?;
        bytes.reverse();

        Option::from(Self::from_repr(bytes)).ok_or(ContractError::HexFmtErr)
    }
}

impl FieldElemAsStr for pallas::Base {}
impl FieldElemAsStr for pallas::Scalar {}

// gated at the function rather than the module: this file is `pub mod util;` in an
// always-compiled parent, so without the `cfg(test)` the crate's unconditional
// `deny(clippy::unwrap_used)` would lint these tests' `.unwrap()` on a non-test build.
#[cfg(test)]
#[test]
fn test_fp_to_u64() {
    use super::pasta_prelude::Field;

    let fp = pallas::Base::from(u64::MAX);
    assert_eq!(fp_to_u64(fp), Some(u64::MAX));
    assert_eq!(fp_to_u64(fp + pallas::Base::ONE), None);
}

#[cfg(test)]
#[test]
fn test_fp_to_str() {
    use self::FieldElemAsStr;
    let fpstr = "0x227ae0da79929f3e23f8d5bc9992f5f140f5198932378731e1b49b67fdc296c8";
    assert_eq!(pallas::Base::from_str(fpstr).unwrap().to_string(), fpstr);

    let fpstr = "0x000000000000000000000000000000000000000000000000ffffffffffffffff";
    let fp = pallas::Base::from(u64::MAX);
    assert_eq!(fp.to_string(), fpstr);
    assert_eq!(pallas::Base::from_str(fpstr).unwrap(), fp);
}
