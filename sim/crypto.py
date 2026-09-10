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

def nullifier(spend_secret: bytes, commitment: bytes) -> bytes:
    """Nullifier = poseidon_hash(spend_secret, commitment).

    Proves commitment ownership without revealing which commitment is spent.
    """
    return poseidon_hash(spend_secret, commitment)


# ---- Per-burn Signature Derivation (H2 fix) ----

def derive_signature_secret(spend_secret: bytes, nullifier_val: bytes) -> bytes:
    """Per-burn signature secret: poseidon_hash(spend_secret, nullifier).

    Cryptographically bound to spend_secret (prevents separation attack)
    but unique per burn (nullifier is unique per commitment — preserves privacy).
    """
    return poseidon_hash(spend_secret, nullifier_val)


# ---- Commitment ----

def commitment(
    public_key: bytes,
    value: int,
    token_id: bytes,
    spend_hook: bytes = b'\x00' * 32,
    user_data: bytes = b'\x00' * 32,
    blind: bytes = b'\x00' * 32,
) -> bytes:
    """Commitment = poseidon_hash(pub, value, token_id, spend_hook, user_data, blind).

    The commitment hides all attributes behind a Poseidon hash.
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

# Fixed-point decay constant: floor(2^(-1/H) * 2^32) for H = HALF_LIFE_BLOCKS.
# Must match src/sdk/src/blockchain.rs::fixed_pow_decay exactly — both use the
# same closed-form binary exponentiation.
DECAY_FP = 4_294_964_465
DECAY_FP_SHIFT = 32


def _fixed_pow_decay(exp: int) -> int:
    """Closed-form binary exponentiation: DECAY_FP^exp / 2^(32*exp).

    Mirrors src/sdk/src/blockchain.rs::fixed_pow_decay (u128 intermediates
    there; Python ints are unbounded so the products are exact).
    """
    result = 1 << DECAY_FP_SHIFT  # 1.0 in fixed-point
    base = DECAY_FP
    while exp > 0:
        if exp & 1 == 1:
            result = (result * base) >> DECAY_FP_SHIFT
        base = (base * base) >> DECAY_FP_SHIFT
        exp >>= 1
    return result


def expected_reward(height: int) -> int:
    """Block reward at a given height (integer-only fixed-point).

    Matches dwow_sdk::blockchain::expected_reward exactly: exponential decay
    R(h) = R0 * DECAY_FP^(h-1) / 2^(32*(h-1)), floored at TAIL_REWARD.
    """
    if height == 0:
        return 0
    if height == 1:
        return INITIAL_REWARD_R0
    exp = height - 1
    decay = _fixed_pow_decay(exp)
    reward = (INITIAL_REWARD_R0 * decay) >> DECAY_FP_SHIFT
    if reward <= TAIL_REWARD:
        return TAIL_REWARD
    return reward


def expected_cumulative_supply(height: int) -> int:
    """Cumulative total supply at a given height.

    Sum of expected_reward(h) for h = 1..height.
    """
    total = 0
    for h in range(1, height + 1):
        total += expected_reward(h)
    return total


def coinbase_blind(prev_commitment: bytes, height: int) -> bytes:
    """Derive the deterministic coinbase blind for a block at the given height.

    blind_H = blake2b("native_token_coinbase_blind" || prev_commitment || height)

    Returns 32-byte deterministic blinding factor.

    Sim-local demo helper for the Pedersen chain test: the deployed
    derivation is poseidon from the miner's private per-block key
    (src/contract/native_token/src/client/pow_reward.rs, domains 1-3) and is
    NOT publicly recomputable. The former Rust SDK blake3 mirror was deleted
    (2026-09); the chain's stored S_H/blind are read via RPC instead.
    """
    import hashlib, struct
    h = hashlib.blake2b()
    h.update(b"native_token_coinbase_blind")
    h.update(prev_commitment)
    h.update(struct.pack("<I", height))
    return h.digest()[:32]


def verify_cumulative_supply(cumulative_commits: list) -> bool:
    """Verify the cumulative supply commitment chain from genesis to tip.

    Returns True if for every block at height h:
      S_h == S_{h-1} + pedersen_commit(expected_reward(h), coinbase_blind(prev_commitment, h))

    This is a passive audit capability — any node can independently recompute
    all blinds and commitments from the emission schedule and verify the chain
    without trusting a single ZK proof.

    Sim-local demo: demonstrates the Pedersen chain identity with the
    blake2b demo blinds. The former Rust SDK mirror was deleted (2026-09) —
    the deployed blind is private-key-derived, so on-chain verification reads
    the stored state via blockchain.get_cumulative_supply.
    """
    expected_point = PedersenCommitment(0, 0)  # identity point
    prev_commitment = b'\x00' * 32  # genesis: zero
    expected_h = 1

    for height, commit in cumulative_commits:
        if height != expected_h:
            return False  # heights must be sequential
        reward = expected_reward(height)
        blind = coinbase_blind(prev_commitment, height)
        commitment_vc = pedersen_commit(reward, blind)
        expected_point = pedersen_add(expected_point, commitment_vc)
        if not pedersen_eq(expected_point, commit):
            return False  # chain break — hidden inflation detected!
        expected_h += 1
    return True
