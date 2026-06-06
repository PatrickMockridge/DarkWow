"""Cryptographic primitives for ZK circuit constraint simulation.

Pure Python, zero dependencies, deterministic. Models the RELATIONSHIPS
that ZK circuits enforce (additive homomorphism, collision resistance,
key derivation) without actual elliptic curve cryptography.

These are SIMULATED primitives — they model constraint behavior, not
cryptographic security. The assertions check that constraints hold;
they do not prove soundness of the underlying crypto.
"""

import hashlib
from typing import Tuple

# ---- Poseidon Hash (simulated) ----

def poseidon_hash(*fields) -> bytes:
    """Deterministic hash modeling Poseidon's collision resistance.

    Uses blake2b with domain separator 'poseidon' to produce
    deterministic 32-byte output from arbitrary field inputs.
    Same inputs always produce same output.
    """
    h = hashlib.blake2b(b'poseidon', digest_size=32)
    for f in fields:
        if isinstance(f, bytes):
            h.update(f)
        elif isinstance(f, int):
            h.update(str(f).encode())
        elif isinstance(f, str):
            h.update(f.encode())
        else:
            h.update(repr(f).encode())
    return h.digest()


# ---- Pedersen Commitment (simulated, additive homomorphism) ----

# Generator constants (markers — not real EC points)
_Gv = b'GV_GENERATOR_VALUE_000000000000000000'
_Gr = b'GR_GENERATOR_RANDOM_000000000000000000'


class PedersenCommitment:
    """Simulated Pedersen commitment: C = v*Gv + blind*Gr.

    Encoded as (value * _Gv_marker, blind * _Gr_marker) tuple so that
    additive homomorphism holds: C(v1,b1) + C(v2,b2) = C(v1+v2, b1+b2).
    """
    __slots__ = ('v_part', 'r_part')

    def __init__(self, v_part: int, r_part: int):
        self.v_part = v_part
        self.r_part = r_part

    def __eq__(self, other) -> bool:
        if not isinstance(other, PedersenCommitment):
            return NotImplemented
        return self.v_part == other.v_part and self.r_part == other.r_part

    def __add__(self, other: 'PedersenCommitment') -> 'PedersenCommitment':
        return PedersenCommitment(
            self.v_part + other.v_part,
            self.r_part + other.r_part,
        )

    def __repr__(self) -> str:
        return f"PC(v={self.v_part}, r={self.r_part})"

    def to_bytes(self) -> bytes:
        """Deterministic serialization for use as a dict key."""
        return self.v_part.to_bytes(32, 'big') + self.r_part.to_bytes(32, 'big')


def pedersen_commit(value: int, blind: bytes) -> PedersenCommitment:
    """Create a Pedersen commitment for a value with a blinding factor.

    The blind is hashed to produce a deterministic integer scalar.
    """
    blind_int = int.from_bytes(
        hashlib.blake2b(b'pedersen_blind' + blind, digest_size=8).digest(), 'big'
    )
    return PedersenCommitment(int(value), blind_int)


def pedersen_add(a: PedersenCommitment, b: PedersenCommitment) -> PedersenCommitment:
    """Add two Pedersen commitments (additive homomorphism)."""
    return a + b


def pedersen_eq(a: PedersenCommitment, b: PedersenCommitment) -> bool:
    """Check equality of two Pedersen commitments."""
    return a == b


# ---- Key Derivation (simulated EC mul_base) ----

def ec_mul_base(secret: bytes) -> bytes:
    """Simulate ec_mul_base(secret, NULLIFIER_K).

    In the real circuit this derives a public key from a secret.
    Modeled as a deterministic hash so that different secrets
    produce different public keys.
    """
    return hashlib.blake2b(b'ec_mul_base' + secret, digest_size=32).digest()


# ---- Nullifier ----

def nullifier(coin_secret: bytes, coin: bytes) -> bytes:
    """Nullifier = poseidon_hash(coin_secret, coin).

    Proves coin ownership without revealing which coin is spent.
    """
    return poseidon_hash(coin_secret, coin)


# ---- Per-burn Signature Derivation (H2 fix) ----

def derive_signature_secret(coin_secret: bytes, nullifier_val: bytes) -> bytes:
    """Per-burn signature secret: poseidon_hash(coin_secret, nullifier).

    Cryptographically bound to coin_secret (prevents separation attack)
    but unique per burn (nullifier is unique per coin — preserves privacy).
    """
    return poseidon_hash(coin_secret, nullifier_val)


# ---- Coin Commitment ----

def coin_commitment(
    public_key: bytes,
    value: int,
    token_id: bytes,
    spend_hook: bytes = b'\x00' * 32,
    user_data: bytes = b'\x00' * 32,
    blind: bytes = b'\x00' * 32,
) -> bytes:
    """Coin = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind).

    The coin commitment hides all attributes behind a Poseidon hash.
    """
    return poseidon_hash(
        public_key, value, token_id, spend_hook, user_data, blind
    )


# ---- Token Registry Root (simulated Merkle root) ----

def token_registry_root(token_entries: list) -> bytes:
    """Simulate a Merkle root of the token registry.

    Models the root that mint_v1 verifies against to prevent replay
    of stale proofs after the registry has changed.
    """
    h = hashlib.blake2b(b'token_registry_root', digest_size=32)
    for entry in sorted(token_entries, key=lambda x: x if isinstance(x, bytes) else repr(x)):
        if isinstance(entry, bytes):
            h.update(entry)
        else:
            h.update(repr(entry).encode())
    return h.digest()


# ---- Token Auth Parent ----

def token_auth_parent(backing_secret: bytes) -> bytes:
    """token_auth_parent = poseidon_hash(backing_secret).

    Stored on-chain in the token registry. The Mint_V1 circuit must
    prove that the prover knows the backing_secret whose hash matches
    the stored token_auth_parent.
    """
    return poseidon_hash(backing_secret)


# ---- Supply / Emission Schedule ----

# Constants matching dwow_sdk::blockchain::expected_reward
INITIAL_REWARD_R0 = 1_383_764_049
HALF_LIFE_BLOCKS = 1_051_920
TAIL_REWARD = 79_853_981

# Fixed-point decay constant
# Fixed-point scale factor: 2^32 for deterministic integer-only arithmetic.
# Must match src/sdk/src/blockchain.rs exactly — both use the same algorithm.
DECAY_FP = 4_294_967_296  # 2^32
DECAY_FP_SHIFT = 32


def expected_reward(height: int) -> int:
    """Block reward at a given height (integer-only fixed-point).

    Matches dwow_sdk::blockchain::expected_reward exactly.
    """
    if height == 0:
        return 0
    if height <= HALF_LIFE_BLOCKS:
        h = height - 1
        numerator = INITIAL_REWARD_R0 - TAIL_REWARD
        decay = (DECAY_FP * h) // HALF_LIFE_BLOCKS
        pre_reward = (numerator * (DECAY_FP - decay)) // DECAY_FP
        return TAIL_REWARD + pre_reward
    return TAIL_REWARD


def expected_cumulative_supply(height: int) -> int:
    """Cumulative total supply at a given height.

    Sum of expected_reward(h) for h = 1..height.
    """
    total = 0
    for h in range(1, height + 1):
        total += expected_reward(h)
    return total
