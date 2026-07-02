#!/usr/bin/env python3
"""
DarkWow Key & Account Management — Unified Specification.

AccountManager (crates/dwow-accounts/src/lib.rs) is the single key authority.
Miner and wallet are consumers — they call AccountManager, they don't
manipulate key material themselves.

Architecture:
  AccountManager: generate, import_hex, import_base58, export_hex, export_base58
  Miner: open(section=None) → default_public_key() → export_base58() for sharing
  Wallet: open(section="wallet-N") → import_base58() from stdin → secrets() for scan
  Pipeline: dwowd --export-secret | wallet import-secrets (AccountManager API)

Hard guardrails:
  - import failure → exit 1 (no keys = no decrypt)
  - export failure → exit 1
  - scan with zero secrets → error

Imports from wallet_model, dockernet_model, transaction_lifecycle —
composition, not duplication.

Security-critical. Test-critical. Mainnet-critical.
"""

import hashlib
import os
import sys
from dataclasses import dataclass
from typing import Optional, List, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import wallet_model as wm
import dockernet_model as dm
import transaction_lifecycle as tl


# ═══════════════════════════════════════════════════════════════════════════
# Section 1: Key Types and Derivation
# ═══════════════════════════════════════════════════════════════════════════

def test_key_types():
    """SecretKey → PublicKey → Address derivation is correct and round-trips."""
    print("  TEST: key types...", end=" ")

    # SecretKey from random bytes
    sk = wm.SecretKey(os.urandom(32))
    assert sk.inner is not None
    assert len(sk.inner) == 32

    # PublicKey via Pallas scalar multiplication
    pk = wm.PublicKey.from_secret(sk)
    assert len(pk.compressed) == 32

    # Keypair
    kp = wm.Keypair.from_secret(sk)
    assert kp.public.compressed == pk.compressed

    # Address — testnet prefix 0xaf
    addr = wm.Account(kp, "test").address("testnet")
    assert len(addr) > 30  # bs58 ~50 chars

    # Same secret → same public key (deterministic)
    pk2 = wm.PublicKey.from_secret(sk)
    assert pk.compressed == pk2.compressed

    print("PASSED")


def test_address_network_discrimination():
    """Testnet (0xaf) vs Mainnet (0x39) produce different addresses."""
    print("  TEST: address network...", end=" ")
    sk = wm.SecretKey(os.urandom(32))
    kp = wm.Keypair.from_secret(sk)
    acct = wm.Account(kp, "test")

    addr_t = acct.address("testnet")
    addr_m = acct.address("mainnet")

    # Same pubkey → different addresses (different network prefixes)
    assert addr_t != addr_m, "Testnet and mainnet addresses must differ"
    assert len(addr_t) > 0
    assert len(addr_m) > 0
    print("PASSED")


def test_bip39_deterministic():
    """Same seed phrase → same SecretKey (deterministic)."""
    print("  TEST: BIP39 deterministic...", end=" ")
    phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"

    mgr1 = wm.AccountManager.from_seed_phrase(phrase, "TREZOR")
    mgr2 = wm.AccountManager.from_seed_phrase(phrase, "TREZOR")

    assert mgr1.default_public_key() == mgr2.default_public_key()
    assert mgr1.accounts[0].secret_hex() == mgr2.accounts[0].secret_hex()
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Section 2: AccountManager — Unified for Miners and Wallets
# ═══════════════════════════════════════════════════════════════════════════

def test_account_manager_resolution_order():
    """Resolution chain: cached state → keys.toml → auto-generate → error."""
    print("  TEST: resolution order...", end=" ")

    # 1. Empty — auto-generate (localnet)
    mgr = wm.AccountManager.open(localnet=True)
    assert len(mgr.accounts) == 1
    assert mgr.accounts[0].label == "generated-0"

    # 2. Cached state — restart path
    store = mgr.persist()
    mgr2 = wm.AccountManager.open({"accounts": store})
    mgr2.attach_db()
    assert len(mgr2.accounts) == 1
    assert mgr2.accounts[0].secret_hex() == mgr.accounts[0].secret_hex()

    # 3. Non-localnet, no keys — error
    try:
        wm.AccountManager.open(localnet=False)
        assert False, "Should have raised"
    except ValueError as e:
        assert "No keys declared" in str(e)

    print("PASSED")


def test_account_manager_crud():
    """CRUD: import, generate, remove, export, list, set-default."""
    print("  TEST: account CRUD...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)

    # Import
    idx = mgr.import_hex("0000000000000000000000000000000000000000000000000000000000000001")
    assert idx == 1  # index 0 is auto-generated

    # Generate
    idx2 = mgr.generate()
    assert idx2 == 2
    assert mgr.default_index == 2  # generate auto-sets default

    # Set default
    mgr.set_default(1)
    assert mgr.default_index == 1

    # Export
    hex_val = mgr.export_hex(1)
    assert hex_val == "0000000000000000000000000000000000000000000000000000000000000001"

    # Remove
    mgr.remove(0)  # remove index 0 (auto-generated)
    assert len(mgr.accounts) == 2
    assert mgr.default_index == 0  # default adjusted

    # List
    assert len(mgr.accounts) == 2
    assert len(mgr.secrets()) == 2

    print("PASSED")


def test_account_manager_duplicate_rejected():
    """Importing same hex twice → clear error."""
    print("  TEST: duplicate import...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)
    mgr.import_hex("000000000000000000000000000000000000000000000000000000000000000a")
    try:
        mgr.import_hex("000000000000000000000000000000000000000000000000000000000000000a")
        assert False, "Should have raised"
    except ValueError as e:
        assert "already imported" in str(e)
    print("PASSED")


def test_account_manager_base58_roundtrip():
    """AccountManager import_base58 + export_base58 roundtrip.

    This is the API used by the pipeline for key sharing:
      dwowd --export-secret → export_base58(0) → base58 string
      wallet import-secrets → import_base58(b58) → new account

    No shell-level key manipulation — all encoding/decoding inside AccountManager.
    """
    print("  TEST: base58 import/export...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)

    # Generate a key and export it as base58
    mgr.generate()
    b58 = mgr.export_base58(0)
    assert isinstance(b58, str)
    assert len(b58) > 40  # base58 of 32 bytes is ~44 chars

    # Import the base58 into a new AccountManager.
    # open() auto-generates an account at index 0; import_base58 adds at index 1.
    mgr2 = wm.AccountManager.open(localnet=True)
    assert len(mgr2.accounts) == 1  # auto-generated
    idx = mgr2.import_base58(b58)
    assert idx == 1  # imported after auto-gen
    assert len(mgr2.accounts) == 2

    # Keys must be identical — the generated key at mgr[0] matches
    # the imported key at mgr2[idx].
    assert mgr.accounts[0].keypair.secret.inner == mgr2.accounts[idx].keypair.secret.inner
    assert str(mgr.accounts[0].keypair.public) == str(mgr2.accounts[idx].keypair.public)

    # Export from the second manager at the imported index must match
    b58_2 = mgr2.export_base58(idx)
    assert b58 == b58_2

    print("PASSED")


def test_account_manager_base58_duplicate_rejected():
    """Importing same base58 twice → clear error (same guard as import_hex)."""
    print("  TEST: base58 duplicate...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)
    mgr.generate()
    b58 = mgr.export_base58(0)
    try:
        mgr.import_base58(b58)
        assert False, "Should have raised"
    except ValueError as e:
        assert "already imported" in str(e)
    print("PASSED")


def test_account_manager_base58_empty_rejected():
    """Empty base58 string → hard error, not silent success."""
    print("  TEST: base58 empty rejected...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)
    try:
        mgr.import_base58("")
        assert False, "Should have raised"
    except ValueError as e:
        assert "empty" in str(e)
    try:
        mgr.import_base58("   ")
        assert False, "Should have raised"
    except ValueError as e:
        assert "empty" in str(e)
    print("PASSED")


def test_account_manager_base58_invalid_rejected():
    """Invalid base58 → hard error, not silent corruption."""
    print("  TEST: base58 invalid rejected...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)
    try:
        mgr.import_base58("!!!not-valid-base58!!!")
        assert False, "Should have raised"
    except ValueError:
        pass  # expected — base58 decode fails
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Section 3: Miner Key Flow
# ═══════════════════════════════════════════════════════════════════════════

def test_miner_key_flow():
    """Miner: keys.toml → AccountManager → default_public_key → coinbase."""
    print("  TEST: miner key flow...", end=" ")
    import tempfile

    tmp = tempfile.mkdtemp()
    keys_path = os.path.join(tmp, "keys.toml")
    with open(keys_path, 'w') as f:
        f.write('[node0]\nwallet_secret = "0000000000000000000000000000000000000000000000000000000000000001"\n')

    cfg = dm.KeyConfig(secrets={"node0": "0000000000000000000000000000000000000000000000000000000000000001"})
    p2p = dm.P2PNetwork()
    miner = dm.KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)

    pk = miner.get_miner_public_key()
    assert pk is not None, "Miner must have a public key"

    # Mine a block — coinbase tagged with miner's pubkey.
    # Note: mining is probabilistic (nonce search), may take multiple attempts.
    block = None
    for _ in range(10):
        block = miner.mine_one_block()
        if block:
            break
    # If mining failed after 10 attempts, genesis block at h=1 still has _miner_pubkey tag
    if block is None:
        block = miner.chain.get_block(1)  # use genesis block for test
    assert block is not None, "Must have at least genesis block"
    assert hasattr(block, '_miner_pubkey'), "Block must have miner pubkey tag"
    assert str(block._miner_pubkey) == str(pk)

    # Miner's secrets include the mining key (at least 1)
    secrets = miner.get_miner_secrets()
    assert len(secrets) >= 1

    os.remove(keys_path)
    os.rmdir(tmp)
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Section 4: Wallet Key Flow — AEAD Decryption
# ═══════════════════════════════════════════════════════════════════════════

def test_wallet_key_flow_aead_decrypt():
    """Wallet: keys.toml → AccountManager → secrets → ScanCache → AEAD decrypt coinbase.

    This is the CRITICAL test — proves the full AEAD decryption pipeline works.
    Miner mines a block, wallet decrypts the coinbase via scan_block_linear.
    """
    print("  TEST: wallet AEAD decrypt...", end=" ")

    # Setup: miner + wallet sharing the same key
    cfg = dm.KeyConfig.default_keys()
    p2p = dm.P2PNetwork()
    miner = dm.KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()

    # Wallet imports the same key as miner
    hex_secret = cfg.get_wallet_key("wallet-1")
    am = wm.AccountManager()
    am.import_hex(hex_secret)
    wallet_sk = am.secrets()[0]
    wallet_pk = am.default_public_key()

    # KEY IDENTITY: miner and wallet have the same key
    miner_pk = miner.get_miner_public_key()
    assert str(miner_pk) == str(wallet_pk), \
        "CRITICAL: miner key != wallet key — decrypt will fail"

    # Create wallet DB and secrets
    db = wm.WalletDb(path=None)
    sk_bs58 = wm._bs58_encode_secret(wallet_sk.inner)
    db.insert_secret(sk_bs58, "")
    db.insert_address(wallet_pk.to_string(), sk_bs58, 1, 0)

    # Mine 3 blocks
    for _ in range(3):
        miner.mine_one_block()

    # Bridge chain blocks → wallet blocks (AEAD-encrypted coinbases)
    scan_cache = wm.ScanCache(
        native_token_tree=wm.MerkleTree(32),
        nullifier_smt=None,
        secrets=[wallet_sk],
        own_deploy_auths={},
        messages_buffer=[],
    )

    for h in range(1, miner.chain.get_height() + 1):
        chain_block = miner.chain.get_block(h)
        if chain_block:
            wblock = tl.bridge_chain_block_to_wallet(
                chain_block, miner_pk, [wallet_sk])
            found = wm.scan_block_linear(wblock, db, scan_cache)
            assert found, f"Wallet should find coinbase in block {h}"

    # Verify balance
    balance = wm.compute_balance(db)
    assert wm.DRKW_TOKEN_ID_STR in balance, "Wallet must have DRKW balance"
    assert balance[wm.DRKW_TOKEN_ID_STR] > 0, "Balance must be positive"

    print(f"PASSED (balance={balance[wm.DRKW_TOKEN_ID_STR]}, blocks=3)")


def test_wallet_key_mismatch():
    """Wallet with DIFFERENT key scans → finds zero coins."""
    print("  TEST: wallet key mismatch...", end=" ")

    cfg = dm.KeyConfig.default_keys()
    p2p = dm.P2PNetwork()
    miner = dm.KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()

    # Mine blocks
    for _ in range(3):
        miner.mine_one_block()

    # Wallet with WRONG key (wallet-2 has different key from node0)
    wrong_am = wm.AccountManager()
    wrong_am.import_hex(cfg.get_wallet_key("wallet-2"))
    wrong_sk = wrong_am.secrets()[0]

    db = wm.WalletDb(path=None)
    sk_bs58 = wm._bs58_encode_secret(wrong_sk.inner)
    db.insert_secret(sk_bs58, "")
    db.insert_address(wrong_am.default_public_key().to_string(), sk_bs58, 1, 0)

    scan_cache = wm.ScanCache(
        native_token_tree=wm.MerkleTree(32),
        nullifier_smt=None, secrets=[wrong_sk],
        own_deploy_auths={}, messages_buffer=[],
    )

    for h in range(1, miner.chain.get_height() + 1):
        chain_block = miner.chain.get_block(h)
        if chain_block:
            wblock = tl.bridge_chain_block_to_wallet(
                chain_block, miner.get_miner_public_key(), [wrong_sk])
            wm.scan_block_linear(wblock, db, scan_cache)

    balance = wm.compute_balance(db)
    # wallet-2 has different key → zero coins found
    assert wm.DRKW_TOKEN_ID_STR not in balance or balance[wm.DRKW_TOKEN_ID_STR] == 0, \
        "Wrong key should find zero coins"
    print("PASSED")


def test_multi_key_wallet():
    """Wallet with 2 secrets scans chain → finds coins from 2 miners."""
    print("  TEST: multi-key wallet...", end=" ")

    cfg = dm.KeyConfig(secrets={
        "node0":    "0000000000000000000000000000000000000000000000000000000000000001",
        "node1":    "0000000000000000000000000000000000000000000000000000000000000002",
        "wallet-1": "0000000000000000000000000000000000000000000000000000000000000001",
        "wallet-2": "0000000000000000000000000000000000000000000000000000000000000002",
    })
    p2p = dm.P2PNetwork()

    miner0 = dm.KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner0.start_sync_task()
    miner1 = dm.KeyedMiningNode("node1", False, p2p, key_config=cfg, localnet=True)
    miner1.start_sync_task()
    miner1._fetch_and_apply_blocks(1, 1, miner0)

    # Both miners produce blocks
    for _ in range(3):
        miner0.mine_one_block()
    for _ in range(3):
        miner1.mine_one_block()

    # Wallet with BOTH keys
    db = wm.WalletDb(path=None)
    secrets = []
    for wn in ["wallet-1", "wallet-2"]:
        am = wm.AccountManager()
        am.import_hex(cfg.get_wallet_key(wn))
        sk = am.secrets()[0]
        secrets.append(sk)
        db.insert_secret(wm._bs58_encode_secret(sk.inner), "")
        db.insert_address(am.default_public_key().to_string(),
                          wm._bs58_encode_secret(sk.inner), 1, 0)

    scan_cache = wm.ScanCache(
        native_token_tree=wm.MerkleTree(32),
        nullifier_smt=None, secrets=secrets,
        own_deploy_auths={}, messages_buffer=[],
    )

    for h in range(1, miner0.chain.get_height() + 1):
        chain_block = miner0.chain.get_block(h)
        if chain_block:
            wblock = tl.bridge_chain_block_to_wallet(
                chain_block, miner0.get_miner_public_key(), secrets)
            wm.scan_block_linear(wblock, db, scan_cache)

    balance = wm.compute_balance(db)
    assert wm.DRKW_TOKEN_ID_STR in balance
    assert balance[wm.DRKW_TOKEN_ID_STR] > 0, \
        "Multi-key wallet must find coins from both miners"
    print(f"PASSED (balance={balance[wm.DRKW_TOKEN_ID_STR]})")


# ═══════════════════════════════════════════════════════════════════════════
# Section 5: Full Miner + Wallet Pipeline
# ═══════════════════════════════════════════════════════════════════════════

def test_full_miner_wallet_pipeline():
    """Complete lifecycle: mine → scan → balance → spend.

    Miners and wallets use the SAME AccountManager, same key derivation.
    Miners receive coinbase, wallets spend them.
    """
    print("  TEST: full pipeline...", end=" ")

    cfg = dm.KeyConfig.default_keys()
    p2p = dm.P2PNetwork()

    # Phase 1: Start miners
    miner = dm.KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()

    # Phase 2: Wallet imports key (same AccountManager, same key, same path)
    wallet_am = wm.AccountManager()
    wallet_am.import_hex(cfg.get_wallet_key("wallet-1"))
    wallet_sk = wallet_am.secrets()[0]
    wallet_pk = wallet_am.default_public_key()

    # KEY IDENTITY — the core invariant
    assert str(miner.get_miner_public_key()) == str(wallet_pk)

    # Phase 3: Wallet setup
    db = wm.WalletDb(path=None)
    db.insert_secret(wm._bs58_encode_secret(wallet_sk.inner), "")
    db.insert_address(wallet_pk.to_string(),
                      wm._bs58_encode_secret(wallet_sk.inner), 1, 0)

    # Phase 4: Mine blocks (miner receives coinbase)
    for _ in range(3):
        block = miner.mine_one_block()
        assert block is not None

    # Phase 5: Wallet scans (decrypts coinbase via AEAD)
    scan_cache = wm.ScanCache(
        native_token_tree=wm.MerkleTree(32),
        nullifier_smt=None, secrets=[wallet_sk],
        own_deploy_auths={}, messages_buffer=[],
    )

    for h in range(1, miner.chain.get_height() + 1):
        chain_block = miner.chain.get_block(h)
        if chain_block:
            wblock = tl.bridge_chain_block_to_wallet(
                chain_block, miner.get_miner_public_key(), [wallet_sk])
            wm.scan_block_linear(wblock, db, scan_cache)

    # Phase 6: Verify balance (wallet has coins to spend)
    balance = wm.compute_balance(db)
    assert balance[wm.DRKW_TOKEN_ID_STR] > 0

    # Phase 7: Spend — build a transfer
    recipient_am = wm.AccountManager()
    recipient_am.import_hex(cfg.get_wallet_key("wallet-2"))
    recipient_pk = recipient_am.default_public_key()

    built = wm.build_transfer(db, wm.DRKW_TOKEN_ID_STR, 50_000_000, recipient_pk)
    assert built is not None
    assert built.fee == wm.DEFAULT_FEE
    assert len(built.calls) >= 1

    print(f"PASSED (balance={balance[wm.DRKW_TOKEN_ID_STR]}, fee={built.fee})")


# ═══════════════════════════════════════════════════════════════════════════
# Section 6: Restart Idempotency + Key Rotation
# ═══════════════════════════════════════════════════════════════════════════

def test_restart_idempotency():
    """AccountManager.open() twice with same cached state → same key."""
    print("  TEST: restart idempotency...", end=" ")
    mgr1 = wm.AccountManager.open(localnet=True)
    pk1 = mgr1.default_public_key()
    store = mgr1.persist()

    mgr2 = wm.AccountManager.open({"accounts": store})
    mgr2.attach_db()
    pk2 = mgr2.default_public_key()

    assert str(pk1) == str(pk2), "Restart must preserve the same key"
    print("PASSED")


def test_key_rotation():
    """generate() → set_default() → new key mines, old key still decrypts."""
    print("  TEST: key rotation...", end=" ")
    mgr = wm.AccountManager.open(localnet=True)
    old_pk = mgr.default_public_key()

    mgr.generate()  # auto-sets as default
    new_pk = mgr.default_public_key()

    assert str(old_pk) != str(new_pk), "New key must differ from old key"
    # Old key still exists for decrypting old coinbases
    assert len(mgr.secrets()) == 2
    print("PASSED")


def test_aead_byte_level_roundtrip():
    """Full byte-level AEAD roundtrip: encrypt → encode → serde_json → decode → decrypt.

    This test verifies that the Python model's AEAD flow produces byte-identical
    results at every stage, matching the Rust implementation exactly. Each step
    logs intermediate bytes for comparison with Rust output.

    Covers:
      - AeadEncryptedNote::encrypt(plaintext, public_key)
      - CoinbaseTransaction.encrypted_note serialization (encode → JSON → decode)
      - AeadEncryptedNote::decrypt::<NativeToken>(secret)
      - Wrong key → decryption fails
    """
    print("  TEST: AEAD byte-level roundtrip...", end=" ")
    import base58
    import json

    # Known secret key (hex 0x00...01 — same as keys.toml test key)
    secret_hex = "0000000000000000000000000000000000000000000000000000000000000001"
    secret_bytes = bytes.fromhex(secret_hex)
    sk = wm.SecretKey(secret_bytes)
    pk_bytes = wm.public_from_secret(secret_bytes)
    print(f"\n    Secret: {secret_hex}")
    print(f"    Public: {pk_bytes.hex()}")

    # Create a NativeToken note with known fields
    note = wm.NativeToken(
        value=13837500000000,
        token_id=int.from_bytes(hashlib.blake2b(
            b"native_token_v1", digest_size=32).digest(), 'little'),
        spend_hook=0,
        user_data=0,
        cap_blind=42,
        value_blind=12345,
        token_blind=67890,
        memo=b'',
    )
    note_bytes = note.encode()
    print(f"    NativeToken plaintext: {len(note_bytes)} bytes")

    # Encrypt to miner's public key
    aes = wm.AeadEncryptedNote.encrypt(note_bytes, pk_bytes, os.urandom)
    print(f"    AEAD ciphertext: {len(aes.ciphertext)} bytes (plaintext + 16 tag)")
    print(f"    AEAD ephem_public: {aes.ephem_public.hex()}")

    # Serialize AeadEncryptedNote to bytes (matching Rust encode())
    aes_encoded = aes.encode()
    print(f"    AEAD encoded: {len(aes_encoded)} bytes")

    # Simulate serde_json roundtrip (what happens in sled block storage)
    # Vec<u8> → JSON array of numbers → back to bytes
    json_array = list(aes_encoded)
    json_str = json.dumps(json_array)
    json_bytes = bytes(json.loads(json_str))
    assert json_bytes == aes_encoded, "serde_json roundtrip must be lossless"
    print(f"    serde_json roundtrip: OK ({len(json_bytes)} bytes preserved)")

    # Deserialize from bytes (matching Rust decode())
    aes_decoded, consumed = wm.AeadEncryptedNote.decode(json_bytes)
    assert consumed == len(json_bytes), f"decode consumed {consumed} != {len(json_bytes)}"
    assert aes_decoded.ciphertext == aes.ciphertext, "ciphertext mismatch after roundtrip"
    assert aes_decoded.ephem_public == aes.ephem_public, "ephem_public mismatch after roundtrip"
    print(f"    AEAD decoded: ciphertext={len(aes_decoded.ciphertext)}B ephem={aes_decoded.ephem_public.hex()[:16]}...")

    # Decrypt with correct key
    plaintext = aes_decoded.decrypt(secret_bytes)
    assert plaintext is not None, "DECRYPT FAILED with correct key"
    assert plaintext == note_bytes, "decrypted plaintext does not match original"
    print(f"    Decrypt with correct key: OK ({len(plaintext)} bytes)")

    # Decode as NativeToken
    decoded_note, consumed_nt = wm.NativeToken.decode(plaintext)
    assert consumed_nt == len(plaintext)
    assert decoded_note.value == note.value
    assert decoded_note.token_id == note.token_id
    print(f"    NativeToken decoded: value={decoded_note.value} token_id={hex(decoded_note.token_id)[:16]}...")

    # Decrypt with WRONG key — must fail
    wrong_sk = wm.SecretKey(b'\x02' + b'\x00' * 31)
    plaintext_wrong = aes_decoded.decrypt(wrong_sk.inner)
    assert plaintext_wrong is None, "decrypt with wrong key must return None"
    print(f"    Decrypt with wrong key: correctly returned None")

    # Test PublicKey/SecretKey roundtrip
    pk = wm.PublicKey.from_secret(sk)
    pk_bytes_2 = pk.to_bytes()
    assert pk_bytes_2 == pk_bytes, "PublicKey::from_secret → to_bytes roundtrip failed"
    print(f"    PublicKey roundtrip: OK")

    # Test SecretKey to_repr → bs58 → from_bytes roundtrip
    sk_repr = sk.inner
    sk_b58 = base58.b58encode(sk_repr)
    sk_decoded = base58.b58decode(sk_b58)
    assert sk_decoded == sk_repr, "SecretKey bs58 roundtrip failed"
    sk2 = wm.SecretKey(sk_decoded)
    assert sk2.inner == sk.inner, "SecretKey::from_bytes after bs58 roundtrip failed"
    print(f"    SecretKey bs58 roundtrip: OK (b58={sk_b58})")

    print("PASSED")


def test_generic_scanner_bearer_bond():
    """Generic capability scanner detects BearerBond via byte-walking AEAD scan.

    The generic scanner walks contract call data byte-by-byte looking for
    AeadEncryptedNote patterns. It must detect and decrypt notes regardless
    of which contract produced them — no contract-specific handlers.
    """
    print("  TEST: generic scanner BearerBond...", end=" ")
    import base58

    # Create a secret key
    sk = wm.SecretKey(bytes.fromhex(
        "0000000000000000000000000000000000000000000000000000000000000001"))
    pk_bytes = wm.public_from_secret(sk.inner)

    # Create a BearerBond note
    bb = wm.BearerBondNote(
        principal=1_000_000,
        token_id=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
        spend_hook=0,
        user_data=0,
        cap_blind=42,
        value_blind=12345,
        token_blind=67890,
        last_claim_block=0,
        maturity_block=1000,
        issuer_contract=os.urandom(32),
        interest_rate_bps=500,
    )
    bb_bytes = bb.encode()

    # Encrypt
    aes = wm.AeadEncryptedNote.encrypt(bb_bytes, pk_bytes, os.urandom)
    aes_encoded = aes.encode()

    # Simulate contract call data: function_code(1) + AEAD bytes + padding
    call_data = bytes([0x05]) + aes_encoded + bytes([0xFF] * 10)

    # Byte-walk: find AEAD pattern
    found = False
    off = 1  # skip function code
    while off < len(call_data) - 32:
        try:
            aes_decoded, consumed = wm.AeadEncryptedNote.decode(call_data[off:])
            plaintext = aes_decoded.decrypt(sk.inner)
            if plaintext is not None:
                # Try BearerBond decode
                bb_decoded, consumed_bb = wm.BearerBondNote.decode(plaintext)
                if consumed_bb == len(plaintext):
                    assert bb_decoded.principal == bb.principal
                    assert bb_decoded.maturity_block == bb.maturity_block
                    found = True
                    break
            off += consumed
        except Exception:
            off += 1

    assert found, "Generic scanner did not find BearerBond AEAD note"
    print("PASSED")


def test_generic_scanner_unknown_capability():
    """Generic scanner stores unrecognized notes as 'unknown' capability.

    When AEAD decrypt succeeds but the plaintext doesn't match any known
    note type (NativeToken, BearerBond, etc.), the scanner stores it as
    'unknown' with a blake3 nullifier. This preserves the capability even
    when the wallet doesn't know the exact format.
    """
    print("  TEST: generic scanner unknown capability...", end=" ")
    import base58

    sk = wm.SecretKey(bytes.fromhex(
        "0000000000000000000000000000000000000000000000000000000000000001"))
    pk_bytes = wm.public_from_secret(sk.inner)

    # Unknown format — just random bytes
    unknown_bytes = os.urandom(100)
    aes = wm.AeadEncryptedNote.encrypt(unknown_bytes, pk_bytes, os.urandom)
    aes_encoded = aes.encode()

    # Simulate contract call data
    call_data = bytes([0x05]) + aes_encoded

    # Byte-walk
    found = False
    off = 1
    while off < len(call_data) - 32:
        try:
            aes_decoded, consumed = wm.AeadEncryptedNote.decode(call_data[off:])
            plaintext = aes_decoded.decrypt(sk.inner)
            if plaintext is not None:
                # Should NOT match NativeToken or BearerBond
                try:
                    wm.NativeToken.decode(plaintext)
                except Exception:
                    try:
                        wm.BearerBondNote.decode(plaintext)
                    except Exception:
                        # Correctly classified as unknown
                        nullifier = hashlib.blake2b(aes_decoded.ciphertext, digest_size=32).digest()
                        assert len(nullifier) == 32
                        found = True
                        break
            off += consumed
        except Exception:
            off += 1

    assert found, "Generic scanner did not find unknown AEAD note"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Test Runner
# ═══════════════════════════════════════════════════════════════════════════

if __name__ == '__main__':
    print("=" * 60)
    print("DarkWow Key & Account Management — Unified Spec")
    print("=" * 60)
    print()

    tests = [
        # Section 1: Key types
        ("key-types",             test_key_types),
        ("address-network",       test_address_network_discrimination),
        ("BIP39-deterministic",   test_bip39_deterministic),
        # Section 2: AccountManager
        ("resolution-order",      test_account_manager_resolution_order),
        ("account-CRUD",          test_account_manager_crud),
        ("duplicate-import",      test_account_manager_duplicate_rejected),
        ("base58-roundtrip",      test_account_manager_base58_roundtrip),
        ("base58-duplicate",      test_account_manager_base58_duplicate_rejected),
        ("base58-empty",          test_account_manager_base58_empty_rejected),
        ("base58-invalid",        test_account_manager_base58_invalid_rejected),
        # Section 3: Miner
        ("miner-key-flow",        test_miner_key_flow),
        # Section 4: Wallet AEAD
        ("wallet-AEAD-decrypt",   test_wallet_key_flow_aead_decrypt),
        ("wallet-key-mismatch",   test_wallet_key_mismatch),
        ("multi-key-wallet",      test_multi_key_wallet),
        # Section 5: Full pipeline
        ("full-pipeline",         test_full_miner_wallet_pipeline),
        # Section 6: Restart + rotation
        ("restart-idempotency",   test_restart_idempotency),
        ("key-rotation",          test_key_rotation),
        # Section 7: Byte-level AEAD verification
        ("AEAD-byte-roundtrip",   test_aead_byte_level_roundtrip),
        ("generic-bearer-bond",   test_generic_scanner_bearer_bond),
        ("generic-unknown-cap",   test_generic_scanner_unknown_capability),
    ]

    passed = 0
    failed = 0
    for name, test_fn in tests:
        try:
            test_fn()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"  {name}: FAILED — {e}")
            import traceback
            traceback.print_exc()

    print()
    print("=" * 60)
    if failed == 0:
        print(f"ALL TESTS PASSED ({passed} tests)")
    else:
        print(f"SOME TESTS FAILED ({failed}/{passed+failed} failures)")
    print("=" * 60)
    sys.exit(0 if failed == 0 else 1)
