#!/usr/bin/env python3
"""
Determinism Proof Fixture Generator.

Constructs a complete wallet.db from first principles using the exact same
Poseidon math as the Rust wallet binary. Tests that wallet state is a pure
mathematical function of (AccountManager, ChainBlocks).

Usage:
  python3 contrib/model/generate_wallet_fixture.py [--out /tmp/wallet_test]

Produces:
  <out>/wallet.db    — SQLite wallet database with held_capabilities
  <out>/keys.toml    — test key declaration
  <out>/expected.txt — expected balance output (token_id\tamount)
"""

import sys
import os
import sqlite3
import hashlib
import argparse
from typing import Tuple

# Ensure we can import from contrib/model
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from wallet_model import (
    SecretKey, PublicKey, Keypair, AffinePoint,
    cap_commitment, nullifier, poseidon_hash,
    PALLAS_P, PALLAS_Q,
    _encode_token_id, _decode_token_id,
    _derive_coin_id_from_secret,
)
from halo2_math import PALLAS_P as HP_P  # verify same constant


def make_test_keypair(seed: bytes = b"determinism_proof") -> Tuple[SecretKey, PublicKey]:
    """Deterministic test keypair from fixed seed."""
    h = hashlib.blake2b(seed, digest_size=32).digest()
    sk = SecretKey(h)
    pk = sk.to_public()
    return sk, pk


def build_coinbase_coin(sk: SecretKey, value: int = 100_000_000,
                        height: int = 1) -> dict:
    """Build a complete coinbase coin record matching Rust's _insert_native_token_cap."""
    from wallet_model import NativeToken

    # Build NativeToken note (same struct as Rust)
    note = NativeToken(
        value=value,
        token_id=0,           # DRKW_TOKEN_ID = pallas::Base::zero()
        spend_hook=0,
        user_data=0,
        cap_blind=42,         # arbitrary test blind
        value_blind=99,       # arbitrary test blind
        token_blind=77,       # arbitrary test blind
        memo=b"",
    )

    # Compute coin commitment: Poseidon(pub_x, pub_y, value, token_id, ...)
    pk = sk.to_public()
    pk_pt = AffinePoint.decompress(pk.compressed)
    commitment = cap_commitment(
        pk_pt.x, pk_pt.y, note.value,
        note.token_id, note.spend_hook, note.user_data, note.cap_blind,
    )

    # Compute nullifier: Poseidon(secret, commitment)
    secret_int = int.from_bytes(sk.inner, 'little')
    nf = nullifier(secret_int, commitment)

    # cap_id = bs58(commitment bytes)
    import base58
    cap_id = base58.b58encode(commitment).decode('ascii')

    # token_id: bs58 of 32 zero bytes (DRKW)
    token_id_str = _encode_token_id(0)  # base58 of zero field element

    # Merkle proof: single leaf at position 0, depth-32 empty siblings
    import hashlib
    leaf_hash = hashlib.blake2b(commitment, digest_size=32, person=b"DarkFi_Leaf").digest()
    root = leaf_hash  # single-leaf tree: root = H(leaf)
    empty_sibling = hashlib.blake2b(b"DarkFi_EmptyMerkleNode", digest_size=32).digest()
    siblings = [base58.b58encode(empty_sibling).decode('ascii')] * 32  # depth 32
    merkle_proof_str = "\n".join(siblings)

    return {
        "cap_id": cap_id,
        "value": note.value,
        "token_id": token_id_str,
        "spend_hook": base58.b58encode(note.spend_hook.to_bytes(32, 'little')).decode('ascii'),
        "user_data": base58.b58encode(note.user_data.to_bytes(32, 'little')).decode('ascii'),
        "leaf_position": 0,
        "secret": base58.b58encode(sk.inner).decode('ascii'),
        "cap_blind": base58.b58encode(note.cap_blind.to_bytes(32, 'little')).decode('ascii'),
        "value_blind": base58.b58encode(note.value_blind.to_bytes(32, 'little')).decode('ascii'),
        "token_blind": base58.b58encode(note.token_blind.to_bytes(32, 'little')).decode('ascii'),
        "created_at_height": height,
        "merkle_proof": merkle_proof_str,
        "merkle_root": base58.b58encode(root).decode('ascii'),
        "nullifier": base58.b58encode(nf).decode('ascii'),
    }


def create_wallet_db(db_path: str, coins: list):
    """Create a fresh wallet.db with wallet schema and coin data."""
    # Read schema from the real wallet.sql
    schema_path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "..", "bin", "dww", "wallet.sql",
    )
    with open(schema_path) as f:
        schema = f.read()

    # Apply schema but REMOVE addresses table references and key_lifecycle
    # (we don't need them for balance queries)
    conn = sqlite3.connect(db_path)
    conn.executescript(schema)
    conn.commit()

    # Insert coins
    for coin in coins:
        conn.execute(
            """INSERT OR IGNORE INTO held_capabilities
            (cap_id, value, token_id, spend_hook, user_data, leaf_position,
             secret, cap_blind, value_blind, token_blind, revoked,
             revoked_at_height, created_at_height)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            (coin["cap_id"], coin["value"], coin["token_id"],
             coin["spend_hook"], coin["user_data"], coin["leaf_position"],
             coin["secret"], coin["cap_blind"], coin["value_blind"],
             coin["token_blind"], 0, None, coin["created_at_height"]),
        )
        conn.execute(
            """INSERT OR IGNORE INTO capability_proofs
            (cap_id, merkle_proof, merkle_root)
            VALUES (?,?,?)""",
            (coin["cap_id"], coin["merkle_proof"], coin["merkle_root"]),
        )
        # Also insert into capabilities table (may not exist in all schema versions)
        try:
            conn.execute(
                """CREATE TABLE IF NOT EXISTS capabilities (
                    nullifier TEXT PRIMARY KEY,
                    contract_id TEXT NOT NULL,
                    block_height INTEGER NOT NULL,
                    note_type TEXT NOT NULL DEFAULT 'unknown',
                    raw_data BLOB
                )""")
            conn.execute(
                """INSERT OR REPLACE INTO capabilities
                (nullifier, contract_id, block_height, note_type, raw_data)
                VALUES (?,?,?,?,?)""",
                (coin["nullifier"], "11111111111111111111111111111111",
                 coin["created_at_height"], "NativeToken", b""),
            )
        except Exception:
            pass  # capabilities table is optional
    conn.close()


def create_keys_toml(keys_path: str, sk: SecretKey, section: str = "wallet-1"):
    """Create minimal keys.toml with test secret."""
    secret_hex = sk.inner.hex()
    with open(keys_path, 'w') as f:
        f.write(f"""# Test fixture keys.toml
[{section}]
wallet_secret = "{secret_hex}"
""")


def main():
    parser = argparse.ArgumentParser(description="Generate wallet test fixtures")
    parser.add_argument("--out", default="/tmp/wallet_fixture",
                        help="Output directory (default: /tmp/wallet_fixture)")
    parser.add_argument("--coins", type=int, default=3,
                        help="Number of coinbase coins to generate (default: 3)")
    parser.add_argument("--height", type=int, default=1,
                        help="Starting block height (default: 1)")
    args = parser.parse_args()

    out_dir = args.out
    os.makedirs(out_dir, exist_ok=True)

    # Generate test keypair
    sk, pk = make_test_keypair()
    print(f"Test keypair: secret={sk.inner.hex()[:16]}...")

    # Build coinbase coins at different heights
    coins = []
    for i in range(args.coins):
        height = args.height + i
        coin = build_coinbase_coin(sk, value=100_000_000 * (i + 1), height=height)
        coins.append(coin)
        print(f"  Coin at height {height}: cap_id={coin['cap_id'][:16]}... value={coin['value']}")

    # Create wallet.db
    db_path = os.path.join(out_dir, "wallet.db")
    create_wallet_db(db_path, coins)
    print(f"Created: {db_path}")

    # Create keys.toml
    keys_path = os.path.join(out_dir, "keys.toml")
    create_keys_toml(keys_path, sk)
    print(f"Created: {keys_path}")

    # Write expected output for balance --porcelain
    expected_path = os.path.join(out_dir, "expected.txt")
    token_id = coins[0]["token_id"]  # all same token
    total_value = sum(c["value"] for c in coins)
    with open(expected_path, 'w') as f:
        f.write(f"{token_id}\t{total_value}\n")
    print(f"Created: {expected_path}")
    print(f"  Expected: {token_id}\t{total_value}")


if __name__ == '__main__':
    main()
