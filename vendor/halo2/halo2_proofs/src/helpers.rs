//! Miscellaneous library helper methods

use group::{ff::PrimeField, GroupEncoding};
use pasta_curves::{arithmetic::CurveAffine, EqAffine, Fp};
use std::io::{self, Read, Write};

use crate::poly::Polynomial;

/// Trait for converting raw bytes to/from the internal representation of a type.
/// For example, field elements are represented in Montgomery form and
/// serialized/deserialized without Montgomery reduction.
pub trait SerdeObject: Sized {
    /// The purpose of unchecked functions is to read the internal memory representation
    /// of a type from bytes as quickly as possible. No sanitization checks are performed
    /// to ensure the bytes represent a valid object. As such this function should only be
    /// used internally as an extension of machine memory. It should not be used to deserialize
    /// externally provided data.
    fn from_raw_bytes_unchecked(bytes: &[u8]) -> Self;

    /// Return `Self` from raw bytes, and also perform a check
    fn from_raw_bytes(bytes: &[u8]) -> Option<Self>;

    /// Convert `self` into raw bytes
    fn to_raw_bytes(&self) -> Vec<u8>;

    /// The purpose of unchecked functions is to read the internal memory representation
    /// of a type from disk as quickly as possible. No sanitization checks are performed
    /// to ensure the bytes represent a valid object. This function should only be used
    /// internally when some machine state cannot be kept in memory (e.g., between runs)
    /// and needs to be reloaded as quickly as possible.
    fn read_raw_unchecked<R: Read>(reader: &mut R) -> Self;

    /// Read `Self` from a reader
    fn read_raw<R: Read>(reader: &mut R) -> io::Result<Self>;

    /// Write `Self` to a writer
    fn write_raw<W: Write>(&self, writer: &mut W) -> io::Result<()>;
}

impl SerdeObject for Fp {
    fn from_raw_bytes_unchecked(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), 32);
        Self::from_repr(bytes.try_into().unwrap()).unwrap()
    }

    fn from_raw_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 32 {
            return None;
        }

        Self::from_repr(bytes.try_into().unwrap()).into()
    }

    fn to_raw_bytes(&self) -> Vec<u8> {
        self.to_repr().to_vec()
    }

    fn read_raw_unchecked<R: io::Read>(reader: &mut R) -> Self {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf).unwrap();
        Self::from_repr(buf).unwrap()
    }

    fn read_raw<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        match Self::from_repr(buf).into() {
            Some(x) => Ok(x),
            None => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Fp repr",
            )),
        }
    }

    fn write_raw<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.to_repr())?;
        Ok(())
    }
}

impl SerdeObject for EqAffine {
    fn from_raw_bytes_unchecked(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), 32);
        Self::from_bytes(bytes.try_into().unwrap()).unwrap()
    }

    fn from_raw_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 32 {
            return None;
        }

        Self::from_bytes(bytes.try_into().unwrap()).into()
    }

    fn to_raw_bytes(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }

    fn read_raw_unchecked<R: io::Read>(reader: &mut R) -> Self {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf).unwrap();
        Self::from_bytes(&buf).unwrap()
    }

    fn read_raw<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        match Self::from_bytes(&buf).into() {
            Some(x) => Ok(x),
            None => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid EqAffine repr",
            )),
        }
    }

    fn write_raw<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.to_bytes())?;
        Ok(())
    }
}

/// This enum specifies how various types are serialized and deserialized.
#[derive(Clone, Copy, Debug)]
pub enum SerdeFormat {
    /// Curve elements are serialized in compressed form.
    /// Field elements are serialized in standard form, with endianness specified by the
    /// `PrimeField` implementation.
    Processed,
    /// Curve elements are serialized in uncompressed form. Field elements are serialized
    /// in their internal Montgomery representation.
    /// When deserializing, checks are performed to ensure curve elements indeed lie on
    /// the curve and field elements are less than modulus.
    RawBytes,
    /// Serialization is the same as `RawBytes`, but no checks are performed.
    RawBytesUnchecked,
}

pub(crate) trait CurveRead: CurveAffine {
    /// Reads a compressed element from the buffer and attempts to parse it
    /// using `from_bytes`.
    fn read<R: io::Read>(reader: &mut R) -> io::Result<Self> {
        let mut compressed = Self::Repr::default();
        reader.read_exact(compressed.as_mut())?;
        Option::from(Self::from_bytes(&compressed))
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "invalid point encoding in proof"))
    }
}

impl<C: CurveAffine> CurveRead for C {}

/// Read and Write methods for `CurveAffine`
pub trait SerdeCurveAffine: CurveAffine + SerdeObject {
    /// Reads an element from the buffer and parses it according to the `format`:
    /// - `Processed`: Reads a compressed curve element and decompress it
    /// - `RawBytes`: Reads an uncompressed curve element with coordinates in Montgomery form.
    /// Checks that field elements are less than modulus, and then checks that the point is on the curve.
    /// - `RawBytesUnchecked`: Reads an uncompressed curve element with coordinates in Montgomery form;
    /// does not perform any checks
    fn read<R: io::Read>(reader: &mut R, format: SerdeFormat) -> io::Result<Self> {
        match format {
            SerdeFormat::Processed => <Self as CurveRead>::read(reader),
            SerdeFormat::RawBytes => <Self as SerdeObject>::read_raw(reader),
            SerdeFormat::RawBytesUnchecked => Ok(<Self as SerdeObject>::read_raw_unchecked(reader)),
        }
    }
    /// Writes a curve element according to `format`:
    /// - `Processed`: Writes a compressed curve element
    /// - Otherwise: Writes an uncompressed curve element with coordinates in Montgomery form
    fn write<W: io::Write>(&self, writer: &mut W, format: SerdeFormat) -> io::Result<()> {
        match format {
            SerdeFormat::Processed => writer.write_all(self.to_bytes().as_ref()),
            _ => self.write_raw(writer),
        }
    }
}
impl<C: CurveAffine + SerdeObject> SerdeCurveAffine for C {}

/// Read and Write methods for `PrimeField`
pub trait SerdePrimeField: PrimeField + SerdeObject {
    /// Reads a field element as bytes from the buffer according to the `format`:
    /// - `Processed`: Reads a field element in standard form, with endianness specified by the
    /// `PrimeField` implementation, and checks that the element is less than the modulus.
    /// - `RawBytes`: Reads a field element from raw bytes in its internal Montgomery representations,
    /// and checks that the element is less than the modulus.
    /// - `RawBytesUnchecked`: Reads a field element in Montgomery form and performs no checks.
    fn read<R: io::Read>(reader: &mut R, format: SerdeFormat) -> io::Result<Self> {
        match format {
            SerdeFormat::Processed => {
                let mut compressed = Self::Repr::default();
                reader.read_exact(compressed.as_mut())?;
                Option::from(Self::from_repr(compressed)).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Other, "Invalid prime field point encoding")
                })
            }
            SerdeFormat::RawBytes => <Self as SerdeObject>::read_raw(reader),
            SerdeFormat::RawBytesUnchecked => Ok(<Self as SerdeObject>::read_raw_unchecked(reader)),
        }
    }

    /// Writes a field element as bytes to the buffer according to the `format`:
    /// - `Processed`: Writes a field element in standard form, with endianness specified by the
    /// `PrimeField` implementation.
    /// - Otherwise: Writes a field element into raw bytes in its internal Montgomery representation,
    /// WITHOUT performing the expensive Montgomery reduction.
    fn write<W: io::Write>(&self, writer: &mut W, format: SerdeFormat) -> io::Result<()> {
        match format {
            SerdeFormat::Processed => writer.write_all(self.to_repr().as_ref()),
            _ => self.write_raw(writer),
        }
    }
}

impl<F: PrimeField + SerdeObject> SerdePrimeField for F {}

/// Convert a slice of `bool` into a `u8`.
///
/// Panics if the slice has length greater than 8.
pub fn pack(bits: &[bool]) -> u8 {
    let mut value = 0u8;
    assert!(bits.len() <= 8);
    for (bit_index, bit) in bits.iter().enumerate() {
        value |= (*bit as u8) << bit_index;
    }
    value
}

/// Writes the first `bits.len()` bits of a `u8` into `bits`.
pub fn unpack(byte: u8, bits: &mut [bool]) {
    for (bit_index, bit) in bits.iter_mut().enumerate() {
        *bit = (byte >> bit_index) & 1 == 1;
    }
}

/// Reads a vector of polynomials from buffer
pub(crate) fn read_polynomial_vec<R: io::Read, F: SerdePrimeField, B>(
    reader: &mut R,
    format: SerdeFormat,
) -> io::Result<Vec<Polynomial<F, B>>> {
    let mut len = [0u8; 4];
    reader.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len);

    (0..len)
        .map(|_| Polynomial::<F, B>::read(reader, format))
        .collect::<io::Result<Vec<_>>>()
}

/// Writes a slice of polynomials to buffer
pub(crate) fn write_polynomial_slice<W: io::Write, F: SerdePrimeField, B>(
    slice: &[Polynomial<F, B>],
    writer: &mut W,
    format: SerdeFormat,
) -> io::Result<()> {
    writer.write_all(&(slice.len() as u32).to_be_bytes())?;
    for poly in slice.iter() {
        poly.write(writer, format)?;
    }
    Ok(())
}

/// Gets the total number of bytes of a slice of polynomials, assuming all polynomials are the same length
pub(crate) fn polynomial_slice_byte_length<F: PrimeField, B>(slice: &[Polynomial<F, B>]) -> usize {
    let field_len = F::default().to_repr().as_ref().len();
    4 + slice.len() * (4 + field_len * slice.get(0).map(|poly| poly.len()).unwrap_or(0))
}
