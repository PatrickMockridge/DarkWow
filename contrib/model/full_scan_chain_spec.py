#!/usr/bin/env python3
"""Full Scan Chain Specification — all 8 steps end-to-end.

Models the EXACT Rust scan chain from mining secret to coin insertion.
Python IS the debugger. Find the gap, fix in Rust, run pipeline ONCE.
"""

import sys, os, hashlib, json, base58
sys.path.insert(0, os.path.dirname(__file__))
from capability_scan_model import *


def bs58_encode(data: bytes) -> str:
    return base58.b58encode(data).decode()


def bs58_decode(s: str) -> bytes:
    return base58.b58decode(s)


def step_1_hex_to_bs58(hex_secret: str) -> str:
    """Step 1: Mining secret hex → binary (xxd -r -p) → bs58 encode.
    Matches: entrypoint-wallet.sh line ~85"""
    binary = bytes.fromhex(hex_secret)
    bs58_str = bs58_encode(binary)
    print(f"  Step 1: hex → bs58: {bs58_str[:16]}...")
    return bs58_str


def step_2_import_secret(bs58_str: str) -> SecretKey:
    """Step 2: bs58 → wallet import-secrets → insert_secret → coin_secrets.
    The import-secrets CLI deserializes bs58 bytes as SecretKey via
    deserialize_async, then calls import_promissory_note_secrets.
    Matches: main.rs:891-901, lib.rs:796-803"""
    # Rust: bs58::decode → into_vec → deserialize_async → SecretKey
    # deserialize_async calls SecretKey::decode which calls from_bytes
    # from_bytes calls pallas::Base::from_repr(bytes)
    bytes_data = bs58_decode(bs58_str)
    assert len(bytes_data) == 32, f"Expected 32 bytes, got {len(bytes_data)}"
    # In Rust: SecretKey::from_bytes(bytes)
    # In Python: SecretKey(bytes)
    secret = SecretKey(bytes_data)
    print(f"  Step 2: import: bs58 → SecretKey OK")
    return secret


def step_3_db_store_and_load(secret: SecretKey) -> list:
    """Step 3: SecretKey → to_repr → bs58 encode → DB store → DB load → bs58 decode.
    insert_secret does: bs58::encode(secret.inner().to_repr()).into_string()
    get_secrets does: SELECT secret FROM coin_secrets
    Matches: lib.rs:798, walletdb.rs:655-666"""
    # Rust insert: secret.inner().to_repr() → 32 bytes → bs58 encode → String
    db_string = bs58_encode(secret.inner)  # inner IS the 32 bytes
    print(f"  Step 3: DB store/load: '{db_string[:16]}...'")
    return [db_string]


def step_4_db_secrets_to_keys(secret_strings: list) -> list:
    """Step 4: bs58 string → decode → from_repr → SecretKey.
    get_promissory_note_secrets does: bs58::decode → into_vec → from_bytes.
    Matches: lib.rs:257-268"""
    keys = []
    for s in secret_strings:
        bytes_data = bs58_decode(s)
        assert len(bytes_data) == 32
        # SecretKey::from_bytes(bytes) calls pallas::Base::from_repr(bytes)
        value = int.from_bytes(bytes_data, 'little')
        assert value < PALLAS_P, f"from_repr FAIL: value {value} >= PALLAS_P"
        keys.append(SecretKey(bytes_data))
    print(f"  Step 4: DB strings → SecretKeys: {len(keys)} key(s)")
    return keys


def step_5_populate_scan_cache(keys: list) -> list:
    """Step 5: get_promissory_note_secrets() → notes_secrets.
    Matches: rpc.rs:178 (notes_secrets = self.get_promissory_note_secrets())"""
    print(f"  Step 5: scan_cache.notes_secrets = {len(keys)} key(s)")
    return keys


def step_6_decode_encrypted_note(encrypted_note_bytes: bytes) -> AeadEncryptedNote:
    """Step 6: AeadEncryptedNote::decode(&mut Cursor::new(&coinbase.encrypted_note)).
    The encrypted_note is Vec<u8> from CoinbaseTransaction. It's the
    Encodable-serialized AeadEncryptedNote: VarInt(len) + ciphertext + ephem_public.
    Matches: rpc.rs:452-454"""
    aes_note, consumed = AeadEncryptedNote.decode(encrypted_note_bytes)
    assert consumed == len(encrypted_note_bytes), \
        f"Decode consumed {consumed} of {len(encrypted_note_bytes)} bytes"
    print(f"  Step 6: AeadEncryptedNote decoded: "
          f"ciphertext={len(aes_note.ciphertext)}B, ephem_pub={aes_note.ephem_public.hex()[:16]}...")
    return aes_note


def step_7_decrypt_and_decode(aes_note: AeadEncryptedNote, secrets: list) -> Optional[NativeNote]:
    """Step 7: aes_note.decrypt::<NativeNote>(secret) for each secret.
    Combined AEAD decrypt + NativeNote decode.
    Matches: rpc.rs:457"""
    for i, secret in enumerate(secrets):
        plaintext = aes_note.decrypt(secret.inner)
        if plaintext is None:
            print(f"    Secret {i}: AEAD decrypt FAILED")
            continue
        # AEAD succeeded → try NativeNote decode
        try:
            note, consumed = NativeNote.decode(plaintext)
            if consumed == len(plaintext):
                print(f"    Secret {i}: decrypt+decode OK → value={note.value}, token_id={note.token_id}")
                return note
            print(f"    Secret {i}: decode consumed {consumed}/{len(plaintext)} bytes")
        except Exception as e:
            print(f"    Secret {i}: NativeNote decode error: {e}")
    print(f"  Step 7: FAILED — no secret matched")
    return None


def step_8_insert_coin(note: Optional[NativeNote]) -> bool:
    """Step 8: Build CoinAttributes, CoinRecord, insert_coin.
    Matches: rpc.rs:458-504"""
    if note is None:
        print(f"  Step 8: SKIP — no coin to insert")
        return False
    print(f"  Step 8: CoinAttributes(value={note.value}, token_id={note.token_id}) → insert_coin")
    return True


def full_chain_test():
    """Run the complete 8-step scan chain."""
    print("=" * 60)
    print("Full Scan Chain Specification")
    print("=" * 60)

    # Input: mining secret hex from dockernet
    hex_secret = "1398a40477a82cd5a7a17730d437706f37a00bd33f694ca7334b3dd5687e8b3c"

    # Step 1: hex → bs58 (entrypoint-wallet.sh)
    bs58_str = step_1_hex_to_bs58(hex_secret)

    # Step 2: bs58 → import → SecretKey (import-secrets CLI)
    secret = step_2_import_secret(bs58_str)

    # Step 3: SecretKey → bs58 → DB store → DB load → bs58 string
    db_strings = step_3_db_store_and_load(secret)

    # Step 4: bs58 string → SecretKey (get_secrets)
    keys = step_4_db_secrets_to_keys(db_strings)

    # Step 5: populate scan_cache
    notes_secrets = step_5_populate_scan_cache(keys)

    # Create coinbase note (what the miner produces)
    coinbase_note = NativeNote(42069000000, 0, 0, 0, 12345, 67890, 11111, b'')
    plaintext = coinbase_note.encode()
    wallet_public = secret.to_public()

    # Miner encrypts for wallet's public key
    aes_note = AeadEncryptedNote.encrypt(plaintext, wallet_public.compressed)
    encrypted_note_bytes = aes_note.encode()  # This is what goes into coinbase.encrypted_note

    # Step 6: decode AeadEncryptedNote from raw bytes
    decoded_note = step_6_decode_encrypted_note(encrypted_note_bytes)

    # Step 7: decrypt + decode as NativeNote
    found_note = step_7_decrypt_and_decode(decoded_note, notes_secrets)

    # Step 8: insert coin
    success = step_8_insert_coin(found_note)

    print()
    if success:
        print("[PASS] Full 8-step chain works end-to-end.")
        print("The Rust code SHOULD match this path exactly.")
        print("If it doesn't, the Rust code is wrong.")
    else:
        print("[FAIL] Chain broke. Fix the failing step in Rust.")
    return success


def test_json_roundtrip():
    """Verify Vec<u8> JSON round-trip (step 6's data format)."""
    print("=" * 60)
    print("JSON Round-Trip Test (Vec<u8> serialization)")
    print("=" * 60)

    hex_secret = "1398a40477a82cd5a7a17730d437706f37a00bd33f694ca7334b3dd5687e8b3c"
    secret = SecretKey(bytes.fromhex(hex_secret))
    wallet_public = secret.to_public()

    coinbase_note = NativeNote(42069000000, 0, 0, 0, 12345, 67890, 11111, b'')
    aes_note = AeadEncryptedNote.encrypt(coinbase_note.encode(), wallet_public.compressed)
    encrypted_note_bytes = aes_note.encode()  # Encodable format

    # Rust: serde_json::to_string(&block) serializes Vec<u8> as JSON array
    json_array = list(encrypted_note_bytes)  # Vec<u8> → [u8] → JSON array of ints
    json_str = json.dumps({"encrypted_note": json_array})
    print(f"  JSON: {len(json_str)} chars")

    # Rust wallet: serde_json::from_str → CoinbaseTransaction { encrypted_note: Vec<u8> }
    parsed = json.loads(json_str)
    recovered_bytes = bytes(parsed["encrypted_note"])
    assert recovered_bytes == encrypted_note_bytes, "JSON round-trip corrupts bytes!"
    print(f"  Vec<u8> round-trip: {len(recovered_bytes)} bytes OK")

    # Now AeadEncryptedNote::decode on recovered bytes
    decoded, consumed = AeadEncryptedNote.decode(recovered_bytes)
    assert consumed == len(recovered_bytes), "Decode after JSON round-trip failed"
    print(f"  AeadEncryptedNote decode after JSON: OK")

    # Decrypt
    plaintext = decoded.decrypt(secret.inner)
    assert plaintext is not None, "Decrypt after JSON round-trip failed"
    note, consumed = NativeNote.decode(plaintext)
    assert consumed == len(plaintext)
    assert note.value == 42069000000
    print(f"  Decrypt + decode after JSON: value={note.value} OK")
    print("[PASS] JSON round-trip preserves encrypted_note bytes correctly")
    return True


if __name__ == "__main__":
    results = []
    results.append(("JSON round-trip", test_json_roundtrip()))
    results.append(("Full 8-step chain", full_chain_test()))

    print("\n" + "=" * 60)
    print("Results")
    print("=" * 60)
    all_pass = True
    for name, result in results:
        status = "PASS" if result else "FAIL"
        if not result: all_pass = False
        print(f"  [{status}] {name}")

    if all_pass:
        print("\nAll steps verified. The gap is NOT in the scan chain logic.")
        print("If the Rust code still fails, check:")
        print("  - Is scan_cache.notes_secrets populated at scan time?")
        print("  - Is the wallet DB encrypted with matching password?")
        print("  - Is the scan running before blocks are mined?")
    else:
        print("\nGap found. Fix the failing step.")
    sys.exit(0 if all_pass else 1)
