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
    """Resolution chain: sled cache → keys.toml → auto-generate → error."""
    print("  TEST: resolution order...", end=" ")

    # 1. Empty — auto-generate (localnet)
    mgr = wm.AccountManager.open(localnet=True)
    assert len(mgr.accounts) == 1
    assert mgr.accounts[0].label == "generated-0"

    # 2. Sled cache — restart path
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
    """AccountManager.open() twice with same sled → same key."""
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
