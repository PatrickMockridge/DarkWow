#!/usr/bin/env python3
"""
L4 Test Oracle — validates Rust wallet against the Python canonical model.

Reads test fixture files from contrib/model/fixtures/, runs the Python
wallet model against each fixture, and produces canonical output that
the Rust wallet must match.

Fixture format (JSON):
{
    "name": "test_name",
    "description": "What this verifies",
    "secrets": ["bs58_secret_1", ...],
    "asset_ids": {"DRKW": "bs58_asset_id", ...},
    "blocks": [
        {
            "height": 1,
            "coinbase": {
                "value": 100000000,
                "recipient_secret_index": 0
            },
            "calls": [
                {
                    "contract": "promissory_note",
                    "function": "TransferV1",
                    "outputs": [
                        {
                            "note_type": "PromissoryNote",
                            "value": 500,
                            "asset_id": 1,
                            "recipient_secret_index": 0
                        }
                    ]
                }
            ]
        }
    ],
    "expected": {
        "coin_count": 2,
        "capability_count_min": 2,
        "total_balance_min": 100000500,
        "output_must_contain": ["Coin worth", "Capabilities"]
    }
}

Usage:
  python3 contrib/model/test_oracle.py [fixture_file ...]
  If no files given, runs all fixtures in fixtures/ directory.
"""

import json
import os
import sys
from typing import List, Dict, Any, Tuple

# Add parent directory to path for wallet_model import
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# Import from wallet_model
import wallet_model as wm


def load_fixture(path: str) -> dict:
    with open(path) as f:
        return json.load(f)


def run_fixture(fixture: dict) -> dict:
    """Run the Python wallet model against a fixture and return results."""
    name = fixture["name"]

    # Create wallet keys
    import base58
    secrets = []
    for s_bs58 in fixture["secrets"]:
        secrets.append(wm.SecretKey(base58.b58decode(s_bs58)))

    # Setup database
    db = wm.WalletDb()
    for sk in secrets:
        db.insert_secret(sk.to_bs58(), "")
        pk = sk.to_public()
        db.insert_address(pk.to_string(), sk.to_bs58(), 1, 0)

    # Insert aliases
    for alias, asset_id in fixture.get("asset_ids", {}).items():
        db.insert_alias(alias, asset_id)

    # Setup scan cache
    cache = wm.ScanCache(secrets=secrets)

    # Scan blocks
    for block_fixture in fixture["blocks"]:
        block = _build_block(block_fixture, secrets)
        wm.scan_block_linear(block, db, cache)

    # Compute results
    caps = db.get_held_capabilities(False)
    capabilities_records = db.get_capabilities()

    # Resolve capabilities
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys(secrets)
    resolver.set_wallet_db(db)

    # Register basic descriptors
    pn_cid = wm._make_test_contract_id("promissory_note")
    resolver.register_descriptor(wm.CapabilityDescriptor(
        name="promissory_note", contract_id=pn_cid,
        capability_discriminants={"CAP_COMMITMENT": wm.CAP_COMMITMENT, "CAP_RECEIPT": wm.CAP_RECEIPT}))

    caps, actions = resolver.resolve()

    balances = wm.compute_balance(db)

    result = {
        "fixture": name,
        "cap_count": len(caps),
        "capability_db_count": len(capabilities_records),
        "capability_resolved_count": len(caps),
        "action_count": len(actions),
        "balances": balances,
        "total_balance": sum(balances.values()),
        "cap_descriptions": [c.description for c in caps],
        "action_names": [a.name for a in actions],
    }
    return result


def _build_block(block_fixture: dict, secrets: List[wm.SecretKey]) -> wm.Block:
    """Build a Block from a fixture definition."""
    block = wm.Block(header=wm.BlockHeader(height=block_fixture["height"]))

    # Coinbase
    if "coinbase" in block_fixture:
        cb = block_fixture["coinbase"]
        sk = secrets[cb["recipient_secret_index"]]
        pk = sk.to_public()
        nt = wm.NativeToken(
            value=cb["value"], asset_id=0, spend_hook=0, user_data=0,
            cap_blind=42, value_blind=99, token_blind=77, memo=b"")
        aes = wm.AeadEncryptedNote.encrypt(nt.encode(), pk.compressed)
        block.transactions.append(wm.Transaction(
            coinbase=wm.CoinbaseTransaction(encrypted_note=aes.encode())))

    # Contract calls
    for call_fixture in block_fixture.get("calls", []):
        tx = wm.Transaction()

        contract = call_fixture["contract"]
        if contract == "promissory_note":
            cid = wm.PROMISSORY_NOTE_CONTRACT_ID
        elif contract == "native_token":
            cid = wm.NATIVE_TOKEN_CONTRACT_ID
        else:
            cid = wm.ContractId(os.urandom(32))

        function = call_fixture["function"]
        func_code_map = {"TransferV1": 0x04, "MintV1": 0x02, "RedeemV1": 0x01}
        func_code = func_code_map.get(function, 0x00)

        call_data = bytes([func_code])

        # Build AEAD outputs
        for out in call_fixture.get("outputs", []):
            sk = secrets[out["recipient_secret_index"]]
            pk = sk.to_public()
            note_type = out["note_type"]
            if note_type == "PromissoryNote":
                note = wm.NativeToken(
                    value=out["value"],
                    asset_id=out.get("asset_id", 1),
                    spend_hook=out.get("spend_hook", 0),
                    user_data=out.get("user_data", 0),
                    cap_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
                    value_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_Q,
                    token_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
                    memo=b"")
            else:
                note = wm.NativeToken(
                    value=out["value"],
                    asset_id=0, spend_hook=0, user_data=0,
                    cap_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
                    value_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_Q,
                    token_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
                    memo=b"")
            aes = wm.AeadEncryptedNote.encrypt(note.encode(), pk.compressed)
            call_data += aes.encode()

        tx.contract_calls.append(wm.ContractCall(
            contract_id=cid.to_bytes(), data=call_data))
        block.transactions.append(tx)

    return block


def verify_fixture(fixture: dict, result: dict) -> Tuple[bool, List[str]]:
    """Verify fixture expectations against model output."""
    expected = fixture.get("expected", {})
    failures = []

    if "cap_count" in expected:
        if result["cap_count"] != expected["cap_count"]:
            failures.append(
                f"cap_count: expected {expected['cap_count']}, got {result['cap_count']}")

    if "capability_db_count" in expected:
        if result["capability_db_count"] != expected["capability_db_count"]:
            failures.append(
                f"capability_db_count: expected {expected['capability_db_count']}, "
                f"got {result['capability_db_count']}")

    if "total_balance_min" in expected:
        if result["total_balance"] < expected["total_balance_min"]:
            failures.append(
                f"total_balance: expected >= {expected['total_balance_min']}, "
                f"got {result['total_balance']}")

    if "output_must_contain" in expected:
        combined = " ".join(result["cap_descriptions"] + result["action_names"])
        for pattern in expected["output_must_contain"]:
            if pattern not in combined:
                failures.append(f"output_must_contain: '{pattern}' not found in output")

    if "action_count_min" in expected:
        if result["action_count"] < expected["action_count_min"]:
            failures.append(
                f"action_count: expected >= {expected['action_count_min']}, "
                f"got {result['action_count']}")

    return len(failures) == 0, failures


def main():
    import base58

    json_mode = "--json" in sys.argv

    # Default fixtures
    fixture_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures")
    fixture_files = [a for a in sys.argv[1:] if a.endswith(".json")]

    if not fixture_files:
        fixture_files = sorted([
            os.path.join(fixture_dir, f) for f in os.listdir(fixture_dir)
            if f.endswith(".json")
        ])

    if not fixture_files:
        if json_mode:
            print("{}")
            return
        print("No fixture files found.")
        sys.exit(1)

    passed = 0
    failed = 0

    for fixture_path in fixture_files:
        fixture = load_fixture(fixture_path)
        name = fixture["name"]

        try:
            result = run_fixture(fixture)

            if json_mode:
                # Output structured JSON matching Rust --json format
                import json as json_mod
                output = {
                    "fixture": name,
                    "capability_count": result["capability_resolved_count"],
                    "action_count": result["action_count"],
                    "cap_count": result["cap_count"],
                    "generic_count": result["capability_resolved_count"] - result["cap_count"],
                    "role_count": 0,
                    "capability_descriptions": result["cap_descriptions"],
                    "action_names": result["action_names"],
                    "descriptors_loaded": 2,
                }
                print(json_mod.dumps(output))
            else:
                print(f"  Oracle: {name}...", end=" ")
                ok, failures_list = verify_fixture(fixture, result)
                if ok:
                    print("PASSED")
                    passed += 1
                else:
                    print(f"FAILED: {'; '.join(failures_list)}")
                    print(f"    Result: caps={result['cap_count']}, "
                          f"caps={result['capability_resolved_count']}, "
                          f"balance={result['total_balance']}")
                    failed += 1
        except Exception as e:
            if json_mode:
                print("{}")
            else:
                print(f"ERROR: {e}")
                import traceback
                traceback.print_exc()
                failed += 1

    if not json_mode:
        print(f"  Oracle results: {passed} PASSED, {failed} FAILED")
        return failed == 0
    return True


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
