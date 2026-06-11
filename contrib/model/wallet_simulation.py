#!/usr/bin/env python3
"""
Wallet Simulation — bridges chain consensus model with wallet capability model.

Mines blocks on a simulated multi-node chain, translates chain-level blocks
(plain reward: int) into wallet-level blocks (AEAD-encrypted NativeToken
coinbase), feeds them to the wallet scanner, and verifies capability
discovery, balance computation, and reorg handling — all in Python, no Docker.

This closes the gap between consensus simulation and wallet verification.
The dockernet handles what can't be simulated here: P2P networking, WASM
deployment, RandomX mining.

Matches:
  contrib/model/chain_model.py        — PoW consensus, mining, block production
  contrib/model/wallet_model.py       — scan_block_linear, capability resolution

Usage:
  python3 contrib/model/wallet_simulation.py
"""

import hashlib
import os
import struct
from dataclasses import dataclass, field
from typing import List, Dict, Optional, Tuple

import wallet_model as wm


# ============================================================================
# Minimal PoW miner — produces blocks with coinbase rewards
# ============================================================================

DIFFICULTY_TARGET = 0x207FFFFF  # Easy target for simulation (no real mining)

@dataclass
class ChainBlock:
    """Simplified chain block — matches chain_model.py Block."""
    height: int = 1
    previous: bytes = b'\x00' * 32
    timestamp: int = 0
    reward: int = 0
    coinbase_recipient_index: int = 0  # index into wallet_secrets list

    def hash(self) -> bytes:
        h = hashlib.blake2b(digest_size=32, person=b"DarkFi_SimHash")
        h.update(struct.pack('<Q', self.height))
        h.update(self.previous)
        h.update(struct.pack('<Q', self.reward))
        return h.digest()


class SimulationChain:
    """Produces blocks with coinbase rewards, 1:1 mapping to chain_model.Miner."""

    def __init__(self, secrets: List[wm.SecretKey]):
        self.secrets = secrets
        self.blocks: List[ChainBlock] = []
        genesis = ChainBlock(
            height=0,
            previous=b'\x00' * 32,
            timestamp=1000,
            reward=0,
        )
        self.blocks.append(genesis)

    def mine_block(self, reward: int, recipient_index: int = 0) -> ChainBlock:
        """Mine a new block on top of the current tip. No real PoW."""
        prev = self.blocks[-1]
        block = ChainBlock(
            height=prev.height + 1,
            previous=prev.hash(),
            timestamp=prev.timestamp + 60,
            reward=reward,
            coinbase_recipient_index=recipient_index,
        )
        self.blocks.append(block)
        return block

    def tip(self) -> ChainBlock:
        return self.blocks[-1]

    def height(self) -> int:
        return len(self.blocks) - 1  # genesis is height 0


# ============================================================================
# Bridge: chain block → wallet block
# ============================================================================

def chain_to_wallet_block(chain_block: ChainBlock,
                           secrets: List[wm.SecretKey]) -> wm.Block:
    """Translate a chain block into a wallet block.
    Wraps the reward in an AEAD-encrypted NativeToken coinbase."""
    if chain_block.reward == 0:
        # Genesis or non-reward block — empty
        return wm.Block(header=wm.BlockHeader(height=chain_block.height))

    sk = secrets[chain_block.coinbase_recipient_index]
    pk = sk.to_public()

    # Create NativeToken with the reward
    nt = wm.NativeToken(
        value=chain_block.reward,
        token_id=0,  # DRKW
        spend_hook=0,
        user_data=0,
        coin_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
        value_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_Q,
        token_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
        memo=b'')
    aes = wm.AeadEncryptedNote.encrypt(nt.encode(), pk.compressed)

    return wm.Block(
        header=wm.BlockHeader(height=chain_block.height),
        transactions=[
            wm.Transaction(coinbase=wm.CoinbaseTransaction(
                encrypted_note=aes.encode()))
        ])


def chain_to_wallet_blocks(chain_blocks: List[ChainBlock],
                            secrets: List[wm.SecretKey]) -> List[wm.Block]:
    """Translate all chain blocks to wallet blocks (skips genesis)."""
    return [chain_to_wallet_block(b, secrets) for b in chain_blocks[1:]]


# ============================================================================
# Test scenarios
# ============================================================================

def test_single_wallet_mining():
    """Mine 10 blocks with coinbase rewards → verify wallet discovers all coins."""
    print("  Test 1: Single wallet mining...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = wm.WalletDb()
    import base58
    db.insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
    db.insert_secret(sk.to_bs58(), "")
    db.insert_alias("DRK", base58.b58encode(b'\x00' * 32).decode('ascii'))

    chain = SimulationChain([sk])
    total_reward = 0
    for i in range(10):
        reward = 100_000_000 + i * 10_000
        total_reward += reward
        chain.mine_block(reward, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])
    cache = wm.ScanCache(notes_secrets=[sk])

    for block in wallet_blocks:
        wm.scan_block_linear(block, db, cache)

    coins = db.get_coins(False)
    assert len(coins) == 10, f"Expected 10 coins, got {len(coins)}"

    caps = db.get_capabilities()
    assert len(caps) == 10, f"Expected 10 capabilities, got {len(caps)}"

    balance = wm.compute_balance(db)
    total = sum(balance.values())
    assert total == total_reward, f"Expected balance {total_reward}, got {total}"

    db.close()
    print("PASSED")


def test_multi_wallet_mining():
    """Mine to 3 different wallets → each wallet discovers its own coins."""
    print("  Test 2: Multi-wallet mining...", end=" ")

    secrets = [wm.SecretKey(os.urandom(32)) for _ in range(3)]
    dbs = [wm.WalletDb() for _ in range(3)]
    caches = [wm.ScanCache(notes_secrets=[s]) for s in secrets]

    for i, sk in enumerate(secrets):
        import base58
        dbs[i].insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
        dbs[i].insert_secret(sk.to_bs58(), "")
        dbs[i].insert_alias("DRK", base58.b58encode(b'\x00' * 32).decode('ascii'))

    chain = SimulationChain(secrets)
    rewards = [0, 0, 0]

    # Alternate rewards between wallets
    for i in range(15):
        wallet_idx = i % 3
        reward = 50_000_000 + i * 5_000
        rewards[wallet_idx] += reward
        chain.mine_block(reward, wallet_idx)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, secrets)

    for i in range(3):
        for block in wallet_blocks:
            # Each wallet scans all blocks, but only decrypts its own
            wm.scan_block_linear(block, dbs[i], caches[i])

        coins = dbs[i].get_coins(False)
        assert len(coins) == 5, f"Wallet {i}: expected 5 coins, got {len(coins)}"

        balance = wm.compute_balance(dbs[i])
        total = sum(balance.values())
        assert total == rewards[i], f"Wallet {i}: expected {rewards[i]}, got {total}"

    for db in dbs:
        db.close()
    print("PASSED")


def test_capability_resolution_after_mining():
    """Mine blocks → scan → resolve capabilities → assert coin caps present."""
    print("  Test 3: Capability resolution after mining...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = wm.WalletDb()
    import base58
    db.insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
    db.insert_secret(sk.to_bs58(), "")
    db.insert_alias("DRK", base58.b58encode(b'\x00' * 32).decode('ascii'))

    chain = SimulationChain([sk])
    for i in range(5):
        chain.mine_block(100_000_000, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])
    cache = wm.ScanCache(notes_secrets=[sk])

    for block in wallet_blocks:
        wm.scan_block_linear(block, db, cache)

    # Resolve capabilities
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys([sk])
    resolver.set_wallet_db(db)
    pn_cid = wm._make_test_contract_id("promissory_note")
    resolver.register_descriptor(wm.CapabilityDescriptor(
        name="promissory_note", contract_id=pn_cid,
        capability_discriminants={"CAP_COIN": wm.CAP_COIN, "CAP_RECEIPT": wm.CAP_RECEIPT}))

    caps, actions = resolver.resolve()
    coin_caps = [c for c in caps if "Coin worth" in c.description]
    assert len(coin_caps) == 5, f"Expected 5 coin caps, got {len(coin_caps)}"

    db.close()
    print("PASSED")


def test_generic_aead_path_2():
    """Unknown contract produces AEAD output → wallet discovers via Path 2."""
    print("  Test 4: Generic AEAD Path 2...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    pk = sk.to_public()
    db = wm.WalletDb()
    import base58
    db.insert_secret(sk.to_bs58(), "")
    db.insert_alias("DRK", base58.b58encode(b'\x00' * 32).decode('ascii'))

    # Mine a coinbase block first (Path 1)
    chain = SimulationChain([sk])
    chain.mine_block(100_000_000, 0)

    # Build an unknown contract call with AEAD-encrypted output (Path 2)
    unknown_cid = wm.ContractId(os.urandom(32))
    arbitrary_payload = b"generic_arbitrary_payload_for_Path2_testing_42"
    aes = wm.AeadEncryptedNote.encrypt(arbitrary_payload, pk.compressed)
    call = wm.ContractCall(
        contract_id=unknown_cid.to_bytes(),
        data=bytes([0x00]) + aes.encode())

    wallet_block = wm.Block(
        header=wm.BlockHeader(height=1),
        transactions=[
            wm.Transaction(coinbase=wm.CoinbaseTransaction(
                encrypted_note=wm.AeadEncryptedNote.encrypt(
                    wm.NativeToken(value=100_000_000, token_id=0, spend_hook=0,
                                   user_data=0, coin_blind=1, value_blind=2,
                                   token_blind=3, memo=b"").encode(),
                    pk.compressed).encode())),
            wm.Transaction(contract_calls=[call]),
        ])

    cache = wm.ScanCache(notes_secrets=[sk])
    found = wm.scan_block_linear(wallet_block, db, cache)
    assert found, "Should discover coinbase + generic AEAD"

    caps = db.get_capabilities()
    assert len(caps) == 2, f"Expected 2 caps (coinbase + unknown), got {len(caps)}"
    note_types = {c.note_type for c in caps}
    assert "NativeToken" in note_types
    assert "unknown" in note_types, f"Expected 'unknown' in note_types, got {note_types}"

    db.close()
    print("PASSED")


def test_reorg_handling():
    """Simulate a reorg: fork → wallet rescans → old coins removed, new coins inserted."""
    print("  Test 5: Reorg handling...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    import base58

    # Main chain: mine 10 blocks at 100M each
    chain_a = SimulationChain([sk])
    for _ in range(10):
        chain_a.mine_block(100_000_000, 0)

    # Fork at height 5: mine 3 different blocks
    chain_b = SimulationChain([sk])
    chain_b.blocks = chain_a.blocks[:6].copy()  # fork at height 5 (keep 0..5)
    for _ in range(3):
        chain_b.mine_block(75_000_000, 0)  # different reward

    # Scan chain A first
    db = wm.WalletDb()
    db.insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
    db.insert_secret(sk.to_bs58(), "")
    db.insert_alias("DRK", base58.b58encode(b'\x00' * 32).decode('ascii'))
    cache = wm.ScanCache(notes_secrets=[sk])

    blocks_a = chain_to_wallet_blocks(chain_a.blocks, [sk])
    for b in blocks_a:
        wm.scan_block_linear(b, db, cache)

    coins_a = db.get_coins(False)
    assert len(coins_a) == 10, f"Chain A: expected 10 coins, got {len(coins_a)}"

    # Reorg: reset to height 5, rescan chain B
    wm.reset_to_height(db, 5)
    blocks_b = chain_to_wallet_blocks(chain_b.blocks, [sk])
    for b in blocks_b:
        if b.header.height > 5:
            wm.scan_block_linear(b, db, cache)

    coins_b = db.get_coins(False)
    # Coins from heights 1-5 survive (5 blocks), heights 6-8 from chain B (3 blocks)
    assert len(coins_b) == 8, f"Chain B after reorg: expected 8 coins, got {len(coins_b)}"

    # Verify chain B coins have the right values
    b_values = sorted([c.value for c in coins_b])
    expected_values = sorted([100_000_000] * 5 + [75_000_000] * 3)
    assert b_values == expected_values, f"Wrong values: {b_values}"

    db.close()
    print("PASSED")


def test_full_pipeline():
    """End-to-end: mine → scan → resolve → balance → select → transfer → spend."""
    print("  Test 6: Full pipeline...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    sk2 = wm.SecretKey(os.urandom(32))
    import base58
    # DRKW token_id = bs58(pallas::Base::zero() encoded as 32 bytes LE)
    zero_bytes = (0).to_bytes(32, 'little')
    drkw_token = base58.b58encode(zero_bytes)
    if isinstance(drkw_token, bytes):
        drkw_token = drkw_token.decode('ascii')

    db = wm.WalletDb()
    db.insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
    db.insert_secret(sk.to_bs58(), "")
    db.insert_alias("DRK", drkw_token)

    # Mine 3 blocks
    chain = SimulationChain([sk])
    for i in range(3):
        chain.mine_block(100_000_000, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])
    cache = wm.ScanCache(notes_secrets=[sk])
    for b in wallet_blocks:
        wm.scan_block_linear(b, db, cache)

    # Balance
    balance = wm.compute_balance(db)
    total = sum(balance.values())
    assert total == 300_000_000, f"Expected 300M, got {total}"

    # Capability resolution
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys([sk])
    resolver.set_wallet_db(db)
    pn_cid = wm._make_test_contract_id("promissory_note")
    resolver.register_descriptor(wm.CapabilityDescriptor(
        name="promissory_note", contract_id=pn_cid,
        capability_discriminants={"CAP_COIN": wm.CAP_COIN, "CAP_RECEIPT": wm.CAP_RECEIPT}))
    caps, actions = resolver.resolve()
    assert len(caps) == 3

    # Coin selection — use the actual stored token_id
    coins = db.get_coins(False)
    stored_token_id = coins[0].token_id
    selected = wm.select_coins(db, stored_token_id, 50_000_000)
    assert len(selected) >= 1
    assert selected[0].value >= 50_000_000

    # Spend detection
    coin_to_spend = db.get_coins(False)[0]
    wm.mark_spent(db, coin_to_spend.coin_id, 10)
    assert wm.is_spent(db, coin_to_spend.coin_id)

    unspent = db.get_coins(False)
    assert len(unspent) == 2

    db.close()
    print("PASSED")


# ============================================================================
# Test runner
# ============================================================================

def run_all_tests():
    print("=" * 60)
    print("DarkWow Wallet Simulation — Chain→Wallet Bridge Tests")
    print("=" * 60)

    tests = [
        test_single_wallet_mining,
        test_multi_wallet_mining,
        test_capability_resolution_after_mining,
        test_generic_aead_path_2,
        test_reorg_handling,
        test_full_pipeline,
    ]

    passed = 0
    failed = 0
    for test in tests:
        try:
            test()
            passed += 1
        except Exception as e:
            failed += 1
            print(f"FAILED: {e}")
            import traceback
            traceback.print_exc()

    print("=" * 60)
    print(f"Results: {passed} PASSED, {failed} FAILED out of {len(tests)}")
    if failed == 0:
        print("ALL TESTS PASSED")
    else:
        print("SOME TESTS FAILED")
    print("=" * 60)
    return failed == 0


if __name__ == "__main__":
    success = run_all_tests()
    exit(0 if success else 1)
