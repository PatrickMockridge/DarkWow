#!/usr/bin/env python3
"""Coinbase Scan Specification — Python reference for Rust wallet scan.

Specification ONLY. No Rust implementation until this spec is verified.

Models the EXACT flow:
  dwowd miner creates native_token note → AEAD encrypts for miner's
  public key → serialized into block.coinbase.encrypted_note →
  wallet scan decodes AeadEncryptedNote → AEAD decrypts with wallet
  secret → decodes native_token note → stores coin record.

Native token is the only token-based capability — a cryptocoin in the
Bitcoin sense. The coinbase produces native_token outputs as the mining
reward. Every other contract produces capabilities in the Mark Miller
sense (bearer instruments, authorization proofs, permissions).

Guardrail: this spec must pass before any Rust change to the scan.
"""

import sys, os, hashlib
sys.path.insert(0, os.path.dirname(__file__))
from capability_scan_model import *


def spec_coinbase_scan(encrypted_note_bytes: bytes,
                        wallet_secrets: List[SecretKey]) -> Optional[dict]:
    """
    SPECIFICATION: Scan a coinbase encrypted_note for native_token coins.

    This is the canonical reference for rpc.rs coinbase handler.
    The Rust code MUST produce the same result for the same input.

    Args:
        encrypted_note_bytes: Raw bytes from block.coinbase.encrypted_note
        wallet_secrets: Wallet's secret keys to try

    Returns:
        dict with 'value', 'token_id', 'block_height' if found, None if not ours.
    """
    # Step 1: Decode AeadEncryptedNote from wire format
    #   Rust: AeadEncryptedNote::decode(&mut Cursor::new(&coinbase.encrypted_note))
    aes_note, consumed = AeadEncryptedNote.decode(encrypted_note_bytes)
    assert consumed == len(encrypted_note_bytes), "All bytes must be consumed"

    # Step 2: Try each wallet secret
    #   Rust: for secret in &scan_cache.notes_secrets { ... }
    for secret in wallet_secrets:
        # Step 3: AEAD decrypt — the discriminator
        #   Rust: aes_note.decrypt::<NativeNote>(secret)
        #   If tag verifies → this capability IS ours
        plaintext = aes_note.decrypt(secret.inner)
        if plaintext is None:
            continue  # Not ours — wrong key

        # AEAD SUCCEEDED. The capability IS ours.
        # Step 4: Decode as native_token note
        #   Rust: NativeNote::decode(&plaintext)
        coin, consumed = NativeNote.decode(plaintext)

        # Step 5: Store coin record
        #   Rust: CoinAttributes{version:0, public_key, value, token_id,
        #          spend_hook, user_data, blind} → coin → CoinRecord → insert_coin
        return {
            "value": coin.value,
            "token_id": coin.token_id,
            "spend_hook": coin.spend_hook,
            "user_data": coin.user_data,
            "coin_blind": coin.coin_blind,
            "value_blind": coin.value_blind,
            "token_blind": coin.token_blind,
            "memo": coin.memo,
        }

    # No secret matched — coinbase is not ours
    return None


def test_spec():
    """Verify the specification with the dockernet wallet secret."""
    print("Coinbase Scan Specification — Verification")
    print("=" * 50)

    # Real dockernet pipeline wallet secret
    wallet_secret_hex = "f550c557f26db096d9a2f0764e63768fc232b2b8b952d8f720935721a0e69d36"
    wallet_secret = SecretKey(bytes.fromhex(wallet_secret_hex))
    wallet_public = wallet_secret.to_public()
    print(f"Wallet secret: {wallet_secret_hex}")
    print(f"Wallet public: {wallet_public.to_string()}")

    # Miner creates coinbase note
    coinbase_note = NativeNote(
        value=42069000000,
        token_id=0,
        spend_hook=0,
        user_data=0,
        coin_blind=12345,
        value_blind=67890,
        token_blind=11111,
        memo=b'',
    )
    plaintext = coinbase_note.encode()
    assert len(plaintext) == 201

    # Miner encrypts for wallet's public key
    encrypted_note = AeadEncryptedNote.encrypt(plaintext, wallet_public.compressed)
    encoded_note = encrypted_note.encode()

    # Wallet scan
    result = spec_coinbase_scan(encoded_note, [wallet_secret])

    assert result is not None, "FAIL: wallet should find its own coinbase note"
    assert result["value"] == 42069000000, f"FAIL: value mismatch: {result['value']}"
    assert result["token_id"] == 0, f"FAIL: token_id mismatch: {result['token_id']}"
    print(f"Found coin: value={result['value']}, token_id={result['token_id']}")

    # Wrong key test
    wrong_secret = SecretKey(os.urandom(32))
    wrong_result = spec_coinbase_scan(encoded_note, [wrong_secret])
    assert wrong_result is None, "FAIL: wrong key should not find coin"

    # Wrong key in the list before correct key
    both_result = spec_coinbase_scan(encoded_note, [wrong_secret, wallet_secret])
    assert both_result is not None, "FAIL: should find coin even if wrong key tried first"
    assert both_result["value"] == 42069000000

    print()
    print("[PASS] Specification verified.")
    print("The wallet CAN find native_token coinbase notes.")
    print("AEAD decrypt + decode path is correct.")
    print("Rust handler must match this specification.")
    return True


if __name__ == "__main__":
    success = test_spec()
    sys.exit(0 if success else 1)
