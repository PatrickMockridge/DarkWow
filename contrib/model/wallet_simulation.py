#!/usr/bin/env python3
"""
Wallet Simulation — bridges chain consensus model with wallet capability model.

Mines blocks on a simulated multi-node chain, translates chain-level blocks
(plain reward: int) into wallet-level blocks (AEAD-encrypted NativeToken
coinbase), feeds them to the wallet scanner, and verifies capability
discovery, balance computation, and reorg handling — all in Python, no Docker.

This closes the gap between consensus simulation and wallet verification.
The dockernet handles what can't be simulated here: P2P networking, WASM
deployment, RandomX mining. Transport architecture (Layer 9 of
wallet_model.py) specifies the two-layer model (built-in TCP vs optional
dwow_transport crate) but the actual network I/O lives in the dockernet.

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

    # Canonical coinbase: PoWRewardV1 contract call, minted to the per-block
    # derived key. Matches wallet_model._make_pow_tx (wallet_model.py:4749)
    # and the Rust PoWRewardCallBuilder — NOT the master public key.
    pow_tx = wm._make_pow_tx(sk, chain_block.height, value=chain_block.reward)

    return wm.Block(
        header=wm.BlockHeader(height=chain_block.height),
        transactions=[pow_tx])


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
    db.insert_alias("DRKW", base58.b58encode(b'\x00' * 32).decode('ascii'))

    chain = SimulationChain([sk])
    total_reward = 0
    for i in range(10):
        reward = 100_000_000 + i * 10_000
        total_reward += reward
        chain.mine_block(reward, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])
    cache = wm.ScanCache(secrets=[sk])

    for block in wallet_blocks:
        wm.scan_block_linear(block, db, cache)

    coins = db.get_held_capabilities(False)
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
    caches = [wm.ScanCache(secrets=[s]) for s in secrets]

    for i, sk in enumerate(secrets):
        import base58
        dbs[i].insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
        dbs[i].insert_secret(sk.to_bs58(), "")
        dbs[i].insert_alias("DRKW", base58.b58encode(b'\x00' * 32).decode('ascii'))

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

        coins = dbs[i].get_held_capabilities(False)
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
    db.insert_alias("DRKW", base58.b58encode(b'\x00' * 32).decode('ascii'))

    chain = SimulationChain([sk])
    for i in range(5):
        chain.mine_block(100_000_000, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])
    cache = wm.ScanCache(secrets=[sk])

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
    coin_caps = [c for c in caps if "Capability value" in c.description]
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
    db.insert_alias("DRKW", base58.b58encode(b'\x00' * 32).decode('ascii'))

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
            wm._make_pow_tx(sk, 1, value=100_000_000),
            wm.Transaction(contract_calls=[call]),
        ])

    cache = wm.ScanCache(secrets=[sk])
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
    db.insert_alias("DRKW", base58.b58encode(b'\x00' * 32).decode('ascii'))
    cache = wm.ScanCache(secrets=[sk])

    blocks_a = chain_to_wallet_blocks(chain_a.blocks, [sk])
    for b in blocks_a:
        wm.scan_block_linear(b, db, cache)

    coins_a = db.get_held_capabilities(False)
    assert len(coins_a) == 10, f"Chain A: expected 10 coins, got {len(coins_a)}"

    # Reorg: reset to height 5, rescan chain B
    wm.reset_to_height(db, 5)
    blocks_b = chain_to_wallet_blocks(chain_b.blocks, [sk])
    for b in blocks_b:
        if b.header.height > 5:
            wm.scan_block_linear(b, db, cache)

    coins_b = db.get_held_capabilities(False)
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
    db.insert_alias("DRKW", drkw_token)

    # Mine 3 blocks
    chain = SimulationChain([sk])
    for i in range(3):
        chain.mine_block(100_000_000, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])
    cache = wm.ScanCache(secrets=[sk])
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
    # 3 coin caps (held_capabilities) + 3 generic caps (capabilities table) = 6
    assert len(caps) == 6, f"Expected 6 caps (3 coin + 3 generic), got {len(caps)}"

    # Coin selection — use the actual stored token_id
    coins = db.get_held_capabilities(False)
    stored_token_id = coins[0].token_id
    selected = wm.select_coins(db, stored_token_id, 50_000_000)
    assert len(selected) >= 1
    assert selected[0].value >= 50_000_000

    # Spend detection — ocap vocabulary: mark_revoked / is_revoked
    coin_to_spend = db.get_held_capabilities(False)[0]
    wm.mark_revoked(db, coin_to_spend.cap_id, 10)
    assert wm.is_revoked(db, coin_to_spend.cap_id)

    unspent = db.get_held_capabilities(False)
    assert len(unspent) == 2

    db.close()
    print("PASSED")


# ============================================================================
# Test runner
# ============================================================================

# ============================================================================
# Multi-contract capability tests — prove the capability OS kernel
# ============================================================================

def _make_contract_call(contract_name: str, secrets: List[wm.SecretKey],
                         recipient_index: int = 0) -> wm.ContractCall:
    """Build a contract call with an AEAD-encrypted output for the given contract.
    The output is encrypted to the recipient's key, enabling Path 2 discovery."""
    sk = secrets[recipient_index]
    pk = sk.to_public()

    # Use deterministic ContractId per contract name
    cid = wm.ContractId(hashlib.blake2b(
        contract_name.encode(), digest_size=32, person=b"DarkFi_SimCID").digest())

    # Create an AEAD-encrypted payload (opaque — unknown note type)
    payload = f"capability_data_for_{contract_name}".encode()
    aes = wm.AeadEncryptedNote.encrypt(payload, pk.compressed)

    return wm.ContractCall(
        contract_id=cid.to_bytes(),
        data=bytes([0x00]) + aes.encode())


def _setup_wallet_with_secret(sk: wm.SecretKey) -> wm.WalletDb:
    """Create a wallet DB pre-loaded with one secret and alias."""
    import base58
    db = wm.WalletDb()
    db.insert_address(sk.to_public().to_string(), sk.to_bs58(), 1, 0)
    db.insert_secret(sk.to_bs58(), "")
    db.insert_alias("DRKW", base58.b58encode(b'\x00' * 32).decode('ascii'))
    return db


def test_multi_contract_path_2_discovery():
    """25 contracts produce AEAD outputs → ALL discovered via Path 2."""
    print("  Test 7: Multi-contract Path 2 discovery (25 contracts)...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = _setup_wallet_with_secret(sk)
    cache = wm.ScanCache(secrets=[sk])

    # 25 contracts — the full DarkWow smart contract suite
    contracts = [
        "escrow", "darkbet_exchange", "dao_escrow", "auction", "dex",
        "subscription", "relayer_endowment", "lottery", "otc_swap",
        "baccarat", "darktoshi_dice", "game_room", "roulette", "slot",
        "bearer_bond", "betting_stake", "pool_stake", "stablecoin",
        "bridge", "oracle", "attestation", "identity", "insurance_market",
        "labor_market", "tender",
    ]

    calls = [_make_contract_call(name, [sk]) for name in contracts]
    block = wm.Block(
        header=wm.BlockHeader(height=1),
        transactions=[wm.Transaction(contract_calls=calls)])

    found = wm.scan_block_linear(block, db, cache)
    assert found, "Should discover all 25 contract outputs"

    caps = db.get_capabilities()
    assert len(caps) == 25, f"Expected 25 capabilities, got {len(caps)}"

    # Every single one should have note_type="unknown" (opaque path)
    note_types = {c.note_type for c in caps}
    assert note_types == {"unknown"}, f"All should be 'unknown', got {note_types}"

    # All should be at block height 1
    for c in caps:
        assert c.block_height == 1

    db.close()
    print("PASSED")


def test_generic_fallback_surfaces_all_25():
    """All 25 capabilities auto-resolved via `_ =>` generic fallback."""
    print("  Test 8: Generic fallback surfaces all 25...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = _setup_wallet_with_secret(sk)
    cache = wm.ScanCache(secrets=[sk])

    contracts = [
        "escrow", "darkbet_exchange", "dao_escrow", "auction", "dex",
        "subscription", "relayer_endowment", "lottery", "otc_swap",
        "baccarat", "darktoshi_dice", "game_room", "roulette", "slot",
        "bearer_bond", "betting_stake", "pool_stake", "stablecoin",
        "bridge", "oracle", "attestation", "identity", "insurance_market",
        "labor_market", "tender",
    ]
    calls = [_make_contract_call(name, [sk]) for name in contracts]
    block = wm.Block(
        header=wm.BlockHeader(height=1),
        transactions=[wm.Transaction(contract_calls=calls)])
    wm.scan_block_linear(block, db, cache)

    # Register ALL 25 contracts as unknown descriptors — every one hits `_ =>`
    # But wait: the resolver has 17 named arms. Contracts matching those names
    # (escrow, auction, etc.) hit named resolvers that find nothing (no state trees).
    # We register them with DIFFERENT names to force `_ =>` path for all.
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys([sk])
    resolver.set_wallet_db(db)
    for i, name in enumerate(contracts):
        cid = wm.ContractId(hashlib.blake2b(
            name.encode(), digest_size=32, person=b"DarkFi_SimCID").digest())
        # Register under a distinct name to avoid hitting named resolver arms
        resolver.register_descriptor(wm.CapabilityDescriptor(
            name=f"sim_{name}", contract_id=cid))

    caps, actions = resolver.resolve()

    # Every capability from the DB should be surfaced via Generic source.
    # Each unknown descriptor triggers _ => which iterates ALL generic_caps.
    # With 25 descriptors × 25 caps, we'd get 625 entries. The _resolve_generic
    # doesn't deduplicate. We just verify generic caps are present.
    generic_caps = [c for c in caps
                    if c.source.source_type == wm.CapabilitySourceType.GENERIC]
    assert len(generic_caps) >= 25, \
        f"Expected at least 25 generic caps, got {len(generic_caps)}"

    # Verify each contains the right note_type and block_height
    unique_descriptions = set()
    for cap in generic_caps:
        assert cap.source.note_type == "unknown"
        assert cap.source.block_height == 1
        assert not cap.consumable
        unique_descriptions.add(cap.description)
    # All 25 contracts should have unique descriptions
    assert len(unique_descriptions) >= 25, \
        f"Expected 25 unique descriptions, got {len(unique_descriptions)}"

    db.close()
    print("PASSED")


def test_unknown_contract_zero_code_changes():
    """A completely new contract with NO descriptor → still auto-resolved."""
    print("  Test 9: Unknown contract — zero code changes...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = _setup_wallet_with_secret(sk)
    cache = wm.ScanCache(secrets=[sk])

    # A contract that did not exist when the wallet was written
    new_contract = "future_defi_protocol_v99"
    call = _make_contract_call(new_contract, [sk])
    block = wm.Block(
        header=wm.BlockHeader(height=42),
        transactions=[wm.Transaction(contract_calls=[call])])
    wm.scan_block_linear(block, db, cache)

    # Register one unknown descriptor to trigger the `_ =>` generic fallback.
    # The `_ =>` arm iterates ALL generic_caps from the DB, so even though
    # "future_defi_protocol_v99" has no registered descriptor, its capability
    # is surfaced when any unknown descriptor hits the `_ =>` arm.
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys([sk])
    resolver.set_wallet_db(db)
    # Register a different unknown contract — this triggers the generic fallback
    unknown_cid = wm.ContractId(os.urandom(32))
    resolver.register_descriptor(wm.CapabilityDescriptor(
        name="some_other_unknown_contract", contract_id=unknown_cid))

    caps, actions = resolver.resolve()
    generic_caps = [c for c in caps
                    if c.source.source_type == wm.CapabilitySourceType.GENERIC]
    # The `_ =>` arm surfaces ALL capabilities from the DB, including the
    # future_defi_protocol_v99 one. At least 1 generic cap must appear.
    assert len(generic_caps) >= 1, \
        f"New contract should auto-resolve via _ =>, got {len(generic_caps)} generic caps"
    # Verify the future protocol capability is among them
    descriptions = [c.description for c in generic_caps]
    found_future = any("Capability from" in d for d in descriptions)
    assert found_future, \
        f"Should find capability via generic fallback, got: {descriptions[:3]}"

    db.close()
    print("PASSED")


def test_mixed_coins_and_contract_caps():
    """Block with coinbase + 5 contract calls → coins + generic caps together."""
    print("  Test 10: Mixed coins + contract capabilities...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = _setup_wallet_with_secret(sk)
    cache = wm.ScanCache(secrets=[sk])

    # 5 contract calls (Path 2)
    contracts = ["escrow", "auction", "dex", "lottery", "subscription"]
    calls = [_make_contract_call(name, [sk]) for name in contracts]

    block = wm.Block(
        header=wm.BlockHeader(height=1),
        transactions=[
            wm._make_pow_tx(sk, 1, value=100_000_000),
            wm.Transaction(contract_calls=calls),
        ])
    wm.scan_block_linear(block, db, cache)

    # Verify DB state
    coins = db.get_held_capabilities(False)
    caps = db.get_capabilities()
    assert len(coins) == 1, f"Expected 1 coin, got {len(coins)}"
    assert len(caps) == 6, f"Expected 6 capabilities (1 NT + 5 unknown), got {len(caps)}"

    nt_caps = [c for c in caps if c.note_type == "NativeToken"]
    unknown_caps = [c for c in caps if c.note_type == "unknown"]
    assert len(nt_caps) == 1
    assert len(unknown_caps) == 5

    # Resolver: coin caps + generic caps for the 5 contracts
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys([sk])
    resolver.set_wallet_db(db)
    pn_cid = wm._make_test_contract_id("promissory_note")
    resolver.register_descriptor(wm.CapabilityDescriptor(
        name="promissory_note", contract_id=pn_cid,
        capability_discriminants={"CAP_COIN": wm.CAP_COIN, "CAP_RECEIPT": wm.CAP_RECEIPT}))
    # Register the 5 contracts as unknown descriptors (prefixed to avoid named arms)
    for name in contracts:
        cid = wm.ContractId(hashlib.blake2b(
            name.encode(), digest_size=32, person=b"DarkFi_SimCID").digest())
        resolver.register_descriptor(wm.CapabilityDescriptor(
            name=f"test_{name}", contract_id=cid))

    caps, actions = resolver.resolve()
    coin_caps = [c for c in caps
                 if c.source.source_type == wm.CapabilitySourceType.COIN]
    gen_caps = [c for c in caps
                if c.source.source_type == wm.CapabilitySourceType.GENERIC]
    assert len(coin_caps) == 1
    # Each unknown descriptor triggers _ => which iterates all generic_caps.
    # With 6 generic caps in DB × 5 unknown descriptors = 30 entries.
    assert len(gen_caps) >= 5, f"Expected at least 5 generic caps, got {len(gen_caps)}"
    unique_gen = len(set(c.description for c in gen_caps))
    assert unique_gen >= 5, f"Expected at least 5 unique generic caps, got {unique_gen}"

    db.close()
    print("PASSED")


def test_capability_kernel_property():
    """Prove 4 kernel properties from capability_kernel_model.py."""
    print("  Test 11: Capability kernel properties...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = _setup_wallet_with_secret(sk)
    cache = wm.ScanCache(secrets=[sk])

    # Property 1: Generic discovery works for ALL contracts
    all_contracts = [
        "escrow", "darkbet_exchange", "dao_escrow", "auction", "dex",
        "subscription", "relayer_endowment", "lottery", "otc_swap",
        "baccarat", "darktoshi_dice", "game_room", "roulette", "slot",
    ]
    calls = [_make_contract_call(name, [sk]) for name in all_contracts]
    block = wm.Block(
        header=wm.BlockHeader(height=1),
        transactions=[wm.Transaction(contract_calls=calls)])
    found = wm.scan_block_linear(block, db, cache)
    assert found, "Property 1 FAILED: no discovery"
    caps = db.get_capabilities()
    assert len(caps) == len(all_contracts), \
        f"Property 1 FAILED: expected {len(all_contracts)}, got {len(caps)}"

    # Property 2: Contract-specific handlers are optional — everything still
    # discovered via Path 2 even without handlers
    # (Proven by Property 1 — none of the 14 contracts above have handlers
    # registered, yet all were discovered)

    # Property 3: Discovery always persists — both structured + opaque
    for c in caps:
        assert c.raw_data is not None and len(c.raw_data) > 0, \
            "Property 3 FAILED: raw data not persisted"

    # Property 4: New contracts work with zero code changes
    future_cid = wm.ContractId(os.urandom(32))
    payload = b"future_protocol_v999_data"
    aes = wm.AeadEncryptedNote.encrypt(payload, sk.to_public().compressed)
    call = wm.ContractCall(
        contract_id=future_cid.to_bytes(),
        data=bytes([0x00]) + aes.encode())
    block2 = wm.Block(
        header=wm.BlockHeader(height=2),
        transactions=[wm.Transaction(contract_calls=[call])])
    found2 = wm.scan_block_linear(block2, db, cache)
    assert found2, "Property 4 FAILED: future contract not discovered"
    caps2 = db.get_capabilities()
    # Should have previous + 1 new
    assert len(caps2) == len(all_contracts) + 1, \
        f"Property 4 FAILED: expected {len(all_contracts) + 1}, got {len(caps2)}"

    db.close()
    print("PASSED")


def test_chain_mined_blocks_with_mixed_capabilities():
    """Mine blocks via SimulationChain, add contract calls → full end-to-end."""
    print("  Test 12: Chain-mined blocks with mixed capabilities...", end=" ")

    sk = wm.SecretKey(os.urandom(32))
    db = _setup_wallet_with_secret(sk)
    cache = wm.ScanCache(secrets=[sk])

    # Mine 5 blocks with coinbase rewards
    chain = SimulationChain([sk])
    for i in range(5):
        chain.mine_block(100_000_000, 0)

    wallet_blocks = chain_to_wallet_blocks(chain.blocks, [sk])

    # Add contract calls to blocks 2 and 4
    contract_names = ["escrow", "auction", "dex", "lottery"]
    for idx, blk in enumerate(wallet_blocks):
        if blk.header.height in (2, 4):
            contract_idx = blk.header.height // 2  # 1 or 2
            subset = contract_names[(contract_idx - 1) * 2:contract_idx * 2]
            calls = [_make_contract_call(name, [sk]) for name in subset]
            blk.transactions.append(wm.Transaction(contract_calls=calls))

    # Scan all blocks
    for blk in wallet_blocks:
        wm.scan_block_linear(blk, db, cache)

    # Verify
    coins = db.get_held_capabilities(False)
    caps = db.get_capabilities()
    assert len(coins) == 5, f"Expected 5 coins, got {len(coins)}"
    assert len(caps) == 9, \
        f"Expected 9 capabilities (5 NT + 4 unknown), got {len(caps)}"

    nt = [c for c in caps if c.note_type == "NativeToken"]
    unk = [c for c in caps if c.note_type == "unknown"]
    assert len(nt) == 5
    assert len(unk) == 4

    # Resolve — coins + generic caps
    resolver = wm.CapabilityResolver()
    resolver.set_user_keys([sk])
    resolver.set_wallet_db(db)
    pn_cid = wm._make_test_contract_id("promissory_note")
    resolver.register_descriptor(wm.CapabilityDescriptor(
        name="promissory_note", contract_id=pn_cid,
        capability_discriminants={"CAP_COIN": wm.CAP_COIN, "CAP_RECEIPT": wm.CAP_RECEIPT}))
    for name in contract_names:
        cid = wm.ContractId(hashlib.blake2b(
            name.encode(), digest_size=32, person=b"DarkFi_SimCID").digest())
        # Prefix name to avoid named resolver arms (escrow, auction, etc.)
        resolver.register_descriptor(wm.CapabilityDescriptor(
            name=f"test_{name}", contract_id=cid))

    caps_resolved, actions = resolver.resolve()
    coin_caps = [c for c in caps_resolved
                 if c.source.source_type == wm.CapabilitySourceType.COIN]
    gen_caps = [c for c in caps_resolved
                if c.source.source_type == wm.CapabilitySourceType.GENERIC]
    assert len(coin_caps) == 5
    assert len(gen_caps) >= 4, f"Expected at least 4 generic caps, got {len(gen_caps)}"
    unique_gen = len(set(c.description for c in gen_caps))
    assert unique_gen >= 4, f"Expected at least 4 unique generic caps, got {unique_gen}"

    db.close()
    print("PASSED")


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
        test_multi_contract_path_2_discovery,
        test_generic_fallback_surfaces_all_25,
        test_unknown_contract_zero_code_changes,
        test_mixed_coins_and_contract_caps,
        test_capability_kernel_property,
        test_chain_mined_blocks_with_mixed_capabilities,
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
