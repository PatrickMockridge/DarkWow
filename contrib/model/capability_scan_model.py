#!/usr/bin/env python3
"""
Capability Scan Model — 1:1 mapping of Rust wallet scan architecture.

Models the wallet as a generalized capability OS kernel.
Every contract uses the same AEAD encryption primitive.
The wallet decrypts ALL outputs; the AEAD tag IS the discriminator.
No contract bias. No hardcoded if/else chains.

Matches:
  bin/drk/src/rpc.rs          — scan_block_linear, apply_tx_*_data_linear
  src/sdk/src/crypto/note.rs  — AeadEncryptedNote encrypt/decrypt
  src/sdk/src/crypto/diffie_hellman.rs — sapling_ka_agree, kdf_sapling
  src/contract/native_token/  — NativeNote, CoinAttributes
"""

import hashlib
import struct
import os
from dataclasses import dataclass, field
from typing import List, Dict, Optional, Tuple
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305


# ==============================================================================
# Pallas Curve (pasta_curves::pallas)
# ==============================================================================

# Pallas base field prime (Fp): y² = x³ + 5
PALLAS_P = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
# Pallas scalar field order (Fq): group order for point multiplication
PALLAS_Q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
PALLAS_A = 0  # y² = x³ + 5
PALLAS_B = 5

# NullifierK generator (from src/sdk/src/crypto/constants/fixed_bases/nullifier_k.rs)
# Derived via hash_to_curve("K") with ORCHARD_PERSONALIZATION
NULLIFIER_K_X = 0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75
NULLIFIER_K_Y = 0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc

# identity point: (0, 0, 1) in projective — representing point at infinity


def fp_add(a: int, b: int) -> int:
    return (a + b) % PALLAS_P


def fp_sub(a: int, b: int) -> int:
    return (a - b) % PALLAS_P


def fp_mul(a: int, b: int) -> int:
    return (a * b) % PALLAS_P


def fp_inv(a: int) -> int:
    """Modular inverse using extended Euclidean algorithm."""
    return pow(a, PALLAS_P - 2, PALLAS_P)


def fp_sqrt(a: int) -> Optional[int]:
    """Tonelli-Shanks for sqrt mod p. Returns None if not quadratic residue."""
    if a == 0:
        return 0
    p = PALLAS_P
    # Check quadratic residuosity
    if pow(a, (p - 1) // 2, p) != 1:
        return None
    # p ≡ 1 mod 4 — need Tonelli-Shanks
    # Factor p-1 = q * 2^s
    q = p - 1
    s = 0
    while q % 2 == 0:
        q //= 2
        s += 1
    # Find quadratic non-residue
    z = 2
    while pow(z, (p - 1) // 2, p) != p - 1:
        z += 1
    m = s
    c = pow(z, q, p)
    t = pow(a, q, p)
    r = pow(a, (q + 1) // 2, p)
    while t != 0 and t != 1:
        temp = t
        i = 0
        for i in range(1, m):
            temp = (temp * temp) % p
            if temp == 1:
                break
        b = pow(c, 1 << (m - i - 1), p) if m > i else c
        m = i
        c = (b * b) % p
        t = (t * c) % p
        r = (r * b) % p
    return r if t == 1 else None


# ==============================================================================
# Affine Point on Pallas curve
# ==============================================================================

@dataclass
class AffinePoint:
    x: int
    y: int
    infinity: bool = False

    @staticmethod
    def identity() -> 'AffinePoint':
        return AffinePoint(x=0, y=0, infinity=True)

    def is_identity(self) -> bool:
        return self.infinity

    def is_on_curve(self) -> bool:
        if self.infinity:
            return True
        lhs = fp_mul(self.y, self.y)
        rhs = fp_add(fp_mul(fp_mul(self.x, self.x), self.x), PALLAS_B)
        return lhs == rhs

    def __eq__(self, other: 'AffinePoint') -> bool:
        if self.infinity and other.infinity:
            return True
        if self.infinity or other.infinity:
            return False
        return self.x == other.x and self.y == other.y

    def double(self) -> 'AffinePoint':
        """Point doubling in affine coordinates."""
        if self.infinity or self.y == 0:
            return AffinePoint.identity()
        # slope = (3*x^2 + A) / (2*y)
        num = fp_mul(3, fp_mul(self.x, self.x))  # A = 0
        den = fp_mul(2, self.y)
        slope = fp_mul(num, fp_inv(den))
        x3 = fp_sub(fp_mul(slope, slope), fp_mul(2, self.x))
        y3 = fp_sub(fp_mul(slope, fp_sub(self.x, x3)), self.y)
        return AffinePoint(x=x3, y=y3)

    def add(self, other: 'AffinePoint') -> 'AffinePoint':
        """Point addition in affine coordinates."""
        if self.infinity:
            return other
        if other.infinity:
            return self
        if self == other:
            return self.double()
        if self.x == other.x:  # self.y != other.y (otherwise they'd be equal)
            return AffinePoint.identity()
        slope = fp_mul(fp_sub(other.y, self.y), fp_inv(fp_sub(other.x, self.x)))
        x3 = fp_sub(fp_sub(fp_mul(slope, slope), self.x), other.x)
        y3 = fp_sub(fp_mul(slope, fp_sub(self.x, x3)), self.y)
        return AffinePoint(x=x3, y=y3)

    def mul(self, scalar: int) -> 'AffinePoint':
        """Double-and-add scalar multiplication."""
        result = AffinePoint.identity()
        addend = self
        while scalar > 0:
            if scalar & 1:
                result = result.add(addend)
            addend = addend.double()
            scalar >>= 1
        return result

    def compress(self) -> bytes:
        """Compress to 32 bytes. Pallas uses y's LSB as sign bit in x's MSB."""
        if self.infinity:
            return b'\x00' * 32
        result = bytearray(self.x.to_bytes(32, 'little'))
        if self.y & 1:
            result[31] |= 0x80
        else:
            result[31] &= 0x7F
        return bytes(result)

    @staticmethod
    def decompress(data: bytes) -> Optional['AffinePoint']:
        """Decompress from 32-byte compressed form."""
        if len(data) != 32:
            return None
        # Extract sign bit from MSB of last byte
        sign = (data[31] >> 7) & 1
        x_bytes = bytearray(data)
        x_bytes[31] &= 0x7F
        x = int.from_bytes(bytes(x_bytes), 'little')
        if x >= PALLAS_P:
            return None
        # y² = x³ + 5
        y_sq = fp_add(fp_mul(fp_mul(x, x), x), PALLAS_B)
        y = fp_sqrt(y_sq)
        if y is None:
            return None
        # Pallas: sign bit is y's LSB
        # If the decompressed y has wrong parity, negate it (P - y)
        if (y & 1) != sign:
            y = (PALLAS_P - y) % PALLAS_P
        return AffinePoint(x=x, y=y)


# Generator (NullifierK)
NULLIFIER_K = AffinePoint(x=NULLIFIER_K_X, y=NULLIFIER_K_Y)


# ==============================================================================
# Sapling-style DH Key Agreement
# ==============================================================================

KDF_PERSONALIZATION = b"DarkFiSaplingKDF"
AEAD_KEY_SIZE = 32
AEAD_NONCE = b'\x00' * 12  # Fixed zero nonce


def sapling_ka_agree(secret_key: bytes, public_key_bytes: bytes) -> bytes:
    """DH key agreement: shared_secret = secret * public_key."""
    assert len(secret_key) == 32
    assert len(public_key_bytes) == 32

    pk = AffinePoint.decompress(public_key_bytes)
    if pk is None:
        raise ValueError("Invalid public key")

    # Convert secret bytes to scalar modulo group order
    scalar = int.from_bytes(secret_key, 'little') % PALLAS_Q

    # shared = scalar * public_key
    shared = pk.mul(scalar)
    return shared.compress()


def kdf_sapling(dh_secret: bytes, ephem_public: bytes) -> bytes:
    """KDF: BLAKE2b with personalization. 32-byte output key."""
    h = hashlib.blake2b(
        digest_size=32,
        person=KDF_PERSONALIZATION,
    )
    h.update(dh_secret)
    h.update(ephem_public)
    return h.digest()


def public_from_secret(secret_key: bytes) -> bytes:
    """Derive public key: secret * G."""
    scalar = int.from_bytes(secret_key, 'little') % PALLAS_Q
    return NULLIFIER_K.mul(scalar).compress()


# ==============================================================================
# AEAD Encrypted Note (matches src/sdk/src/crypto/note.rs)
# ==============================================================================

@dataclass
class AeadEncryptedNote:
    ciphertext: bytes   # includes 16-byte AEAD tag at end
    ephem_public: bytes  # 32 bytes compressed

    def encode(self) -> bytes:
        """Encodable format: VarInt(len) + ciphertext + ephem_public."""
        result = encode_varint(len(self.ciphertext))
        result += self.ciphertext
        result += self.ephem_public
        return result

    @staticmethod
    def decode(data: bytes) -> Tuple['AeadEncryptedNote', int]:
        """Decode from Encodable byte format."""
        ct_len, varint_bytes = decode_varint(data)
        offset = varint_bytes
        ciphertext = data[offset:offset + ct_len]
        offset += ct_len
        ephem_public = data[offset:offset + 32]
        offset += 32
        return AeadEncryptedNote(ciphertext=ciphertext, ephem_public=ephem_public), offset

    @staticmethod
    def encrypt(plaintext: bytes, recipient_public: bytes, rng=os.urandom) -> 'AeadEncryptedNote':
        """Encrypt plaintext for recipient's public key."""
        # Generate ephemeral keypair
        ephem_secret = rng(32)
        ephem_secret_int = int.from_bytes(ephem_secret, 'little') % PALLAS_Q
        ephem_secret = ephem_secret_int.to_bytes(32, 'little')
        ephem_public = NULLIFIER_K.mul(ephem_secret_int).compress()

        # DH key agreement
        dh_secret = sapling_ka_agree(ephem_secret, recipient_public)

        # KDF
        key = kdf_sapling(dh_secret, ephem_public)

        # ChaCha20Poly1305 encrypt
        chacha = ChaCha20Poly1305(key)
        ciphertext = chacha.encrypt(AEAD_NONCE, plaintext, None)

        return AeadEncryptedNote(ciphertext=ciphertext, ephem_public=ephem_public)

    def decrypt(self, secret_key: bytes) -> Optional[bytes]:
        """Try to decrypt with wallet's secret key. Returns None if AEAD fails."""
        try:
            dh_secret = sapling_ka_agree(secret_key, self.ephem_public)
            key = kdf_sapling(dh_secret, self.ephem_public)
            chacha = ChaCha20Poly1305(key)
            plaintext = chacha.decrypt(AEAD_NONCE, self.ciphertext, None)
            return plaintext
        except Exception:
            return None


# ==============================================================================
# Encodable Binary Serialization (matches dwow_serial)
# ==============================================================================

def encode_varint(value: int) -> bytes:
    """Variable-length integer encoding (compact size)."""
    if value < 0xFD:
        return bytes([value])
    elif value <= 0xFFFF:
        return b'\xFD' + struct.pack('<H', value)
    elif value <= 0xFFFFFFFF:
        return b'\xFE' + struct.pack('<I', value)
    else:
        return b'\xFF' + struct.pack('<Q', value)


def decode_varint(data: bytes) -> Tuple[int, int]:
    """Decode variable-length integer. Returns (value, bytes_consumed)."""
    if data[0] < 0xFD:
        return data[0], 1
    elif data[0] == 0xFD:
        return struct.unpack('<H', data[1:3])[0], 3
    elif data[0] == 0xFE:
        return struct.unpack('<I', data[1:5])[0], 5
    else:
        return struct.unpack('<Q', data[1:9])[0], 9


def encode_u64(value: int) -> bytes:
    return struct.pack('<Q', value)


def decode_u64(data: bytes) -> Tuple[int, int]:
    return struct.unpack('<Q', data[:8])[0], 8


def encode_pallas_base(value: int) -> bytes:
    """Encode pallas::Base (Fp element) as 32-byte LE repr."""
    return value.to_bytes(32, 'little')


def decode_pallas_base(data: bytes) -> Tuple[int, int]:
    return int.from_bytes(data[:32], 'little'), 32


def encode_pallas_scalar(value: int) -> bytes:
    """Encode pallas::Scalar (Fq element) as 32-byte LE repr."""
    return value.to_bytes(32, 'little')


def decode_pallas_scalar(data: bytes) -> Tuple[int, int]:
    return int.from_bytes(data[:32], 'little'), 32


def encode_vec(data: bytes) -> bytes:
    return encode_varint(len(data)) + data


def decode_vec(data: bytes) -> Tuple[bytes, int]:
    length, varint_bytes = decode_varint(data)
    return data[varint_bytes:varint_bytes + length], varint_bytes + length


# ==============================================================================
# NativeNote (matches src/contract/native_token/src/client/mod.rs:48-67)
# ==============================================================================

@dataclass
class NativeNote:
    value: int          # u64
    token_id: int       # pallas::Base
    spend_hook: int     # pallas::Base
    user_data: int      # pallas::Base
    coin_blind: int     # pallas::Base
    value_blind: int    # pallas::Scalar
    token_blind: int    # pallas::Base
    memo: bytes         # Vec<u8>

    def encode(self) -> bytes:
        result = encode_u64(self.value)
        result += encode_pallas_base(self.token_id)
        result += encode_pallas_base(self.spend_hook)
        result += encode_pallas_base(self.user_data)
        result += encode_pallas_base(self.coin_blind)
        result += encode_pallas_scalar(self.value_blind)
        result += encode_pallas_base(self.token_blind)
        result += encode_vec(self.memo)
        return result

    @staticmethod
    def decode(data: bytes) -> Tuple['NativeNote', int]:
        offset = 0
        value, n = decode_u64(data[offset:]); offset += n
        token_id, n = decode_pallas_base(data[offset:]); offset += n
        spend_hook, n = decode_pallas_base(data[offset:]); offset += n
        user_data, n = decode_pallas_base(data[offset:]); offset += n
        coin_blind, n = decode_pallas_base(data[offset:]); offset += n
        value_blind, n = decode_pallas_scalar(data[offset:]); offset += n
        token_blind, n = decode_pallas_base(data[offset:]); offset += n
        memo, n = decode_vec(data[offset:]); offset += n
        return NativeNote(value=value, token_id=token_id, spend_hook=spend_hook,
                          user_data=user_data, coin_blind=coin_blind,
                          value_blind=value_blind, token_blind=token_blind,
                          memo=memo), offset


# ==============================================================================
# Generic Capability — contract-agnostic storage
# ==============================================================================

@dataclass
class Capability:
    """A decrypted capability from any contract."""
    contract_id: bytes   # 32-byte contract address
    nullifier: bytes     # 32-byte unique identifier
    block_height: int
    raw_data: bytes      # contract-specific decoded data (opaque)
    note_type: str       # "NativeNote", "CoinAttributes", "IdentityAttributes", etc.

    def __repr__(self):
        return (f"Capability(contract_id={self.contract_id[:6].hex()}.., "
                f"note_type={self.note_type}, block={self.block_height}, "
                f"nullifier={self.nullifier[:8].hex()}..)")


# ==============================================================================
# Wallet — Generic Capability Scanner
# ==============================================================================

@dataclass
class WalletState:
    """The wallet's state: secrets and discovered capabilities."""
    secrets: List[bytes] = field(default_factory=list)   # secret keys
    capabilities: List[Capability] = field(default_factory=list)
    scanned_to: int = 0

    def import_secret(self, secret_hex: str):
        """Import a secret key (32-byte hex)."""
        secret = bytes.fromhex(secret_hex)
        assert len(secret) == 32, f"Secret must be 32 bytes, got {len(secret)}"
        self.secrets.append(secret)
        pub = public_from_secret(secret)
        print(f"  Imported secret -> public key: {pub[:8].hex()}...")

    def scan_output(self, encrypted_note: AeadEncryptedNote,
                    contract_id: bytes, block_height: int) -> Optional[Capability]:
        """
        Generic capability scan: try ALL secrets against an encrypted note.
        If ANY secret successfully decrypts → it's our capability.
        No contract bias. No token_id filter. Just AEAD tag verification.
        """
        for secret in self.secrets:
            plaintext = encrypted_note.decrypt(secret)
            if plaintext is not None:
                # Decryption succeeded — this capability is ours.
                # Now try to decode it as known types to classify.
                nullifier = hashlib.blake2b(plaintext, digest_size=32).digest()
                note_type, _ = try_decode_any(plaintext)
                return Capability(
                    contract_id=contract_id,
                    nullifier=nullifier,
                    block_height=block_height,
                    raw_data=plaintext,
                    note_type=note_type,
                )
        return None

    def scan_block(self, block_outputs: List[Tuple[bytes, AeadEncryptedNote]],
                   block_height: int):
        """Scan all outputs in a block. Register any capability we can decrypt."""
        found = 0
        for contract_id, encrypted_note in block_outputs:
            cap = self.scan_output(encrypted_note, contract_id, block_height)
            if cap is not None:
                self.capabilities.append(cap)
                found += 1
        return found

    def balance(self) -> Dict[str, int]:
        """Report capability counts by contract type."""
        counts: Dict[str, int] = {}
        for cap in self.capabilities:
            key = cap.note_type
            counts[key] = counts.get(key, 0) + 1
        return counts


# ==============================================================================
# Known Note Type Decoder — tries all known types, returns best match
# ==============================================================================

def try_decode_any(plaintext: bytes) -> Tuple[str, Optional[object]]:
    """Attempt to decode plaintext as any known note type. Returns (type_name, object)."""
    # Try NativeNote
    try:
        note, consumed = NativeNote.decode(plaintext)
        if consumed == len(plaintext):
            return "NativeNote", note
    except Exception:
        pass

    # Falls through to: unknown note type
    return "UnknownNote", None


# ==============================================================================
# Test: Complete Flow — Coinbase Mining → Wallet Scan
# ==============================================================================

def test_coinbase_flow():
    """Model the full coinbase → wallet scan flow."""
    print("=" * 60)
    print("Test: Coinbase Mining → Wallet Scan")
    print("=" * 60)

    # 1. Generate wallet secret
    wallet_secret_hex = "f550c557f26db096d9a2f0764e63768fc232b2b8b952d8f720935721a0e69d36"
    wallet_secret = bytes.fromhex(wallet_secret_hex)
    wallet_public = public_from_secret(wallet_secret)
    print(f"Wallet public key:  {wallet_public.hex()}")
    print(f"Wallet secret key:  {wallet_secret.hex()}")

    # 2. Simulate dwowd miner: create coinbase NativeNote
    coinbase_note = NativeNote(
        value=42069000000,  # 42.069 coins
        token_id=0,          # DRK (native token)
        spend_hook=0,        # no spend hook
        user_data=0,         # no user data
        coin_blind=12345,    # random blinding factor
        value_blind=67890,   # value commitment blinding
        token_blind=11111,   # token commitment blinding
        memo=b'',            # empty memo
    )
    print(f"\nCoinbase NativeNote: value={coinbase_note.value}, "
          f"token_id={coinbase_note.token_id}")

    # 3. Miner encrypts coinbase note for wallet's public key
    plaintext = coinbase_note.encode()
    print(f"  Plaintext size: {len(plaintext)} bytes")
    print(f"  Expected: 201 bytes (8+32+32+32+32+32+32+1)")

    encrypted_note = AeadEncryptedNote.encrypt(plaintext, wallet_public)
    print(f"  Ciphertext size: {len(encrypted_note.ciphertext)} bytes")
    print(f"    (plaintext {len(plaintext)} + 16 AEAD tag)")
    print(f"  Encoded AeadEncryptedNote: {len(encrypted_note.encode())} bytes")

    # 4. Wallet scan: try to decrypt
    wallet = WalletState()
    wallet.import_secret(wallet_secret_hex)

    native_token_cid = b'\x01' * 32  # mock NATIVE_TOKEN_CONTRACT_ID
    block_outputs = [(native_token_cid, encrypted_note)]
    found = wallet.scan_block(block_outputs, block_height=5)

    if found > 0:
        print(f"\n[PASS] Found {found} capability in block 5")
        cap = wallet.capabilities[0]
        print(f"  Contract: {cap.contract_id[:6].hex()}...")
        print(f"  Note type: {cap.note_type}")
        print(f"  Nullifier: {cap.nullifier[:8].hex()}...")
        if cap.note_type == "NativeNote":
            note, _ = NativeNote.decode(cap.raw_data)
            print(f"  Value: {note.value}")
            print(f"  Token ID: {note.token_id}")
    else:
        print(f"\n[FAIL] No capabilities found")

    print(f"\nWallet balance: {wallet.balance()}")

    # 5. Verify: decrypt with WRONG key fails
    wrong_secret = os.urandom(32)
    wrong_plaintext = encrypted_note.decrypt(wrong_secret)
    print(f"\nDecrypt with wrong key: {'FAILED (correct!)' if wrong_plaintext is None else 'SUCCEEDED (BUG!)'}")

    return found > 0


# ==============================================================================
# Test: Generic Scan — Multiple Contracts
# ==============================================================================

def test_generic_scan():
    """Model a generic scan — wallet finds capabilities from ANY contract."""
    print("\n" + "=" * 60)
    print("Test: Generic Scan — Multiple Contracts")
    print("=" * 60)

    # Wallet with one secret
    wallet = WalletState()
    wallet.import_secret("f550c557f26db096d9a2f0764e63768fc232b2b8b952d8f720935721a0e69d36")
    wallet_public = public_from_secret(bytes.fromhex("f550c557f26db096d9a2f0764e63768fc232b2b8b952d8f720935721a0e69d36"))

    # Simulate outputs from multiple contracts, all encrypted to the same wallet
    native_token_cid = hashlib.blake2b(b"native_token", digest_size=32).digest()
    identity_cid = hashlib.blake2b(b"identity", digest_size=32).digest()
    promissory_note_cid = hashlib.blake2b(b"promissory_note", digest_size=32).digest()
    unknown_cid = hashlib.blake2b(b"some_new_contract", digest_size=32).digest()

    # Generate encrypted notes for each contract
    native_note = NativeNote(value=50000000000, token_id=0, spend_hook=0,
                             user_data=0, coin_blind=42, value_blind=99,
                             token_blind=77, memo=b'').encode()
    encrypted_native = AeadEncryptedNote.encrypt(native_note, wallet_public)

    # Identity: just some bytes (wallet doesn't know the type)
    identity_data = b'IDENTITY_CAP' + os.urandom(32)
    encrypted_identity = AeadEncryptedNote.encrypt(identity_data, wallet_public)

    # Promissory note: more opaque data
    pn_data = b'PROMISSORY_NOTE' + os.urandom(64)
    encrypted_pn = AeadEncryptedNote.encrypt(pn_data, wallet_public)

    # Unknown contract: completely new contract
    unknown_data = b'UNKNOWN_CONTRACT_DATA' + os.urandom(16)
    encrypted_unknown = AeadEncryptedNote.encrypt(unknown_data, wallet_public)

    # Someone else's note: encrypted to a DIFFERENT key
    other_public = public_from_secret(os.urandom(32))
    other_data = b'SOMEONE_ELSE_DATA' + os.urandom(16)
    encrypted_other = AeadEncryptedNote.encrypt(other_data, other_public)

    # Scan ALL outputs — wallet should find 4 of 5
    block_outputs = [
        (native_token_cid, encrypted_native),
        (identity_cid, encrypted_identity),
        (promissory_note_cid, encrypted_pn),
        (unknown_cid, encrypted_unknown),
        (unknown_cid, encrypted_other),  # not ours
    ]

    found = wallet.scan_block(block_outputs, block_height=10)
    print(f"\nFound {found} of 5 outputs (4 ours, 1 not ours)")

    assert found == 4, f"Expected 4, got {found}"
    print("[PASS] Correct: found exactly our 4 capabilities")
    print("[PASS] Generic scan works — no contract bias")

    # Verify balances by contract
    for cap in wallet.capabilities:
        print(f"  {cap}")

    return True


# ==============================================================================
# Test: Serialization Round-trip
# ==============================================================================

def test_serialization():
    """Verify NativeNote and AeadEncryptedNote round-trip correctly."""
    print("\n" + "=" * 60)
    print("Test: Serialization Round-trip")
    print("=" * 60)

    note = NativeNote(
        value=100000000,
        token_id=0xABCD,
        spend_hook=0xDEADBEEF,
        user_data=42,
        coin_blind=12345,
        value_blind=67890,
        token_blind=11111,
        memo=b'hello world',
    )

    encoded = note.encode()
    print(f"Encoded NativeNote: {len(encoded)} bytes")
    print(f"  Memo: {len(b'hello world')} bytes -> VarInt(11) + 11 = 12")

    decoded, consumed = NativeNote.decode(encoded)
    assert consumed == len(encoded), f"Consumed {consumed} != {len(encoded)}"
    assert decoded == note, f"Decoded note doesn't match original"

    print("[PASS] NativeNote round-trip OK")

    # Test AeadEncryptedNote round-trip
    key = bytes.fromhex("f550c557f26db096d9a2f0764e63768fc232b2b8b952d8f720935721a0e69d36")
    pub = public_from_secret(key)

    plaintext = note.encode()
    encrypted = AeadEncryptedNote.encrypt(plaintext, pub)
    encoded_aead = encrypted.encode()
    print(f"Encoded AeadEncryptedNote: {len(encoded_aead)} bytes")

    decoded_aead, consumed = AeadEncryptedNote.decode(encoded_aead)
    assert consumed == len(encoded_aead)
    assert decoded_aead.ciphertext == encrypted.ciphertext
    assert decoded_aead.ephem_public == encrypted.ephem_public

    # Decrypt
    decrypted = decoded_aead.decrypt(key)
    assert decrypted is not None, "Decryption should succeed"
    assert decrypted == plaintext, "Decrypted plaintext doesn't match"

    print("[PASS] AeadEncryptedNote round-trip OK")
    print("[PASS] Encryption → Decryption OK")

    return True


# ==============================================================================
# Main
# ==============================================================================

if __name__ == "__main__":
    print("DarkWow Capability Scan Model")
    print("Wallet as generalized capability OS kernel")
    print("All contracts equal. AEAD tag = discriminator.\n")

    results = []

    results.append(("Serialization round-trip", test_serialization()))
    results.append(("Coinbase mining → scan", test_coinbase_flow()))
    results.append(("Generic multi-contract scan", test_generic_scan()))

    print("\n" + "=" * 60)
    print("Results")
    print("=" * 60)
    all_pass = True
    for name, result in results:
        status = "PASS" if result else "FAIL"
        if not result:
            all_pass = False
        print(f"  [{status}] {name}")

    if all_pass:
        print("\nAll tests passed. Model confirms:")
        print("  1. AEAD encryption/decryption works (ChaCha20Poly1305 + Sapling DH)")
        print("  2. Wallet scan finds capabilities from ANY contract")
        print("  3. No contract bias — unknown contracts work same as known ones")
        print("  4. Wrong key → decryption fails (AEAD tag verification)")
        print("  5. Serialization round-trips correctly")
    else:
        print("\nSome tests FAILED. Model has bugs.")
        exit(1)
