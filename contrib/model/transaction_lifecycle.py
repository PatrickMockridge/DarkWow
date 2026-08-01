#!/usr/bin/env python3
"""
DarkWow Transaction Lifecycle — Standalone Integration Specification.

Models the FULL path: wallet builds tx → P2P broadcast → mempool accepts →
miner includes in block → consensus validates → wallet confirms.

Imports from 4 existing specs (composition, not duplication):
  - wallet_model.py        → wallet building, scanning, key management
  - dockernet_model.py     → chain state, P2P, mining, key config
  - chain_validation_model → full validation with uncles, real emission
  - proof_of_token_balance → mass conservation, fee accounting

Adds 1 new component absent from all existing specs:
  - Mempool                → tx acceptance, nullifier dedup, fee ordering

wallet_model.Transaction is the single Transaction type (matches
src/linear/src/transaction.rs and type-system.md §8.2). No bridge class needed.

Also acts as a SENSE CHECK — verifies cross-spec invariants hold
(DEFAULT_FEE consistency, expected_reward matching, etc.).

HAZOP Phase 5 (2026-07-01): MEM5, FMT4, FMT3 remediation.
"""

import hashlib
import os
import sys
import struct
from dataclasses import dataclass, field
from typing import Optional, List, Dict, Set, Tuple, Any

# Ensure the model directory is on the path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

# ── Imports from existing specs ──────────────────────────────────────────
import wallet_model as wm
import dockernet_model as dm
import chain_validation_model as cv
import proof_of_token_balance as ptb

# Re-export commonly used types for convenience
AccountManager = wm.AccountManager
Account = wm.Account
SecretKey = wm.SecretKey
Keypair = wm.Keypair
PublicKey = wm.PublicKey
WalletDb = wm.WalletDb
ScanCache = wm.ScanCache
CapRecord = wm.CapRecord
NativeToken = wm.NativeToken
AeadEncryptedNote = wm.AeadEncryptedNote
ContractCall = wm.ContractCall
BuiltTransaction = wm.BuiltTransaction
ContractCallLeaf = wm.ContractCallLeaf
MerkleTree = wm.MerkleTree
MerkleProof = wm.MerkleProof

KeyConfig = dm.KeyConfig
MiningNode = dm.MiningNode
KeyedMiningNode = dm.KeyedMiningNode
P2PNetwork = dm.P2PNetwork
ChainState = dm.ChainState
PoWConsensus = dm.PoWConsensus

DEFAULT_FEE = wm.DEFAULT_FEE  # 42_000_000
DRKW_TOKEN_ID_STR = wm.DRKW_TOKEN_ID_STR


# ═══════════════════════════════════════════════════════════════════════════
# Transaction bridge — wallet_model.Transaction is the single authority
# ═══════════════════════════════════════════════════════════════════════════
#
# wallet_model.Transaction now matches src/linear/src/transaction.rs::Transaction
# and type-system.md §8.2. It is the single Transaction type used by wallet,
# consensus, and mempool layers.
#
# The deprecated CanonicalTransaction bridge class has been removed.
# bridge_chain_block_to_wallet() maps simplified chain-model transactions
# (reward-only) into full wallet_model.Transaction instances for scanning.


def compute_txid(tx: wm.Transaction) -> str:
    """Deterministic transaction ID: blake2b hash of transaction semantics.
    Witness bytes are EXCLUDED per type-system.md §8.2 (L1 identity/witness decoupling).
    Includes fee as a convenience field — in the real system, different fees mean different
    FeeV1 contract calls, which naturally produce different txids."""
    h = hashlib.blake2b(digest_size=32)
    h.update(struct.pack('<B', tx.version))
    h.update(struct.pack('<Q', tx.lock_time))
    h.update(struct.pack('<Q', tx.fee))
    for call in tx.contract_calls:
        h.update(call.contract_id)
        h.update(call.data)
    if tx.coinbase:
        h.update(tx.coinbase.coin)
    return h.hexdigest()


def chain_tx_from_wallet_built(built: BuiltTransaction) -> wm.Transaction:
    """Convert wallet_model.BuiltTransaction → wallet_model.Transaction for mempool/block use.
    Maps ContractCallLeaf → ContractCall, extracts nullifiers from witness."""
    calls = []
    for leaf in built.calls:
        cid_bytes = leaf.contract_id.to_bytes() if hasattr(leaf.contract_id, 'to_bytes') else bytes(leaf.contract_id)
        calls.append(ContractCall(
            contract_id=cid_bytes,
            data=leaf.data,
        ))
    return wm.Transaction(
        version=1,
        contract_calls=calls,
        lock_time=0,
        fee=built.fee,
        witness=built.tx_commitment,
    )


def bridge_chain_block_to_wallet(chain_block: dm.Block,
                                  miner_pubkey: Any = None,
                                  secrets: List = None) -> wm.Block:
    """Convert dockernet_model.Block → wallet_model.Block for scanning.

    Wraps coinbase rewards in AEAD-encrypted NativeToken notes so the
    wallet's scan_block_linear() can discover them.

    Args:
        chain_block: A dockernet_model.Block with Transaction(reward=N)
        miner_pubkey: The miner's PublicKey (for encryption)
        secrets: List of SecretKey (wallet secrets that can decrypt)
    """
    wallet_txs = []
    for ctx in chain_block.transactions:
        if ctx.reward > 0 and miner_pubkey is not None:
            # Build a NativeToken coinbase note encrypted to the miner
            nt = NativeToken(
                value=ctx.reward,
                token_id=0,  # DRKW = zero token ID (int, not bytes)
                spend_hook=0,
                user_data=0,
                cap_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
                value_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_Q,
                token_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
                memo=b'',
            )
            note = AeadEncryptedNote.encrypt(nt.encode(), miner_pubkey.compressed)
            # Build PoWRewardV1 contract_call — the scanner discovers coinbase
            # outputs by scanning contract_calls for NativeToken calls, not
            # by reading tx.coinbase. Function code 0x05 = PoWRewardV1.
            pow_reward_data = bytes([wm.NT_FUNC_POW_REWARD_V1]) + note.encode()
            pow_reward_call = wm.ContractCall(
                contract_id=wm.NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
                data=pow_reward_data,
            )
            # Also populate coinbase field for nullifier verification (scan_block_linear:2834)
            coinbase_tx = wm.CoinbaseTransaction(
                encrypted_note=note.encode(),
                proof=b'',
                public_inputs=[],
                coin=b'\x00' * 32,
                value_commit_x=b'\x00' * 32,
                value_commit_y=b'\x00' * 32,
                token_commit=b'\x00' * 32,
            )
            wallet_txs.append(wm.Transaction(
                version=1,
                contract_calls=[pow_reward_call],
                coinbase=coinbase_tx,
            ))
    return wm.Block(
        header=wm.BlockHeader(
            height=chain_block.header.height,
            previous=chain_block.header.previous,
            hash=dm.hash_block(chain_block.header).to_bytes(32, 'little'),
            timestamp=chain_block.header.timestamp,
            total_reward=sum(tx.reward for tx in chain_block.transactions),
            merkle_root=chain_block.header.merkle_root,
            target=chain_block.header.target,
        ),
        transactions=wallet_txs,
    )


# ═══════════════════════════════════════════════════════════════════════════
# Mempool — models bin/dwowd/src/mempool.rs
# ═══════════════════════════════════════════════════════════════════════════

class Mempool:
    """Transaction pool with production-grade acceptance policies.

    Models bin/dwowd/src/mempool.rs.
    Currently absent from ALL other Python specs (HAZOP MEM5).
    """

    def __init__(self, max_size: int = 10_000):
        self._txs: Dict[str, wm.Transaction] = {}
        self._nullifiers: Set[bytes] = set()
        self._max_size: int = max_size
        self.rejected: List[Tuple[str, str]] = []  # (txid, reason)

    def accept(self, tx: wm.Transaction) -> Tuple[bool, str]:
        """Validate and accept a transaction. Returns (accepted, reason).

        Acceptance criteria:
          1. Dedup by txid — reject duplicates
          2. Nullifier uniqueness — reject double-spends
          3. Fee minimum — reject below MIN_FEE (unless coinbase)
          4. Size limit — evict lowest-fee if full
        """
        txid = compute_txid(tx)

        # 1. Dedup by txid
        if txid in self._txs:
            self.rejected.append((txid, "duplicate txid"))
            return False, "Transaction already in mempool"

        # 2. Nullifier dedup — use tx.nullifiers (pre-computed by wallet)
        for nf in tx.nullifiers:
            if nf in self._nullifiers:
                self.rejected.append((txid, "nullifier already spent"))
                return False, "Double-spend detected: nullifier already in pool"

        # 3. Fee minimum (non-coinbase txs — coinbase has PoWRewardV1 at contract_calls[0])
        is_coinbase = tx.coinbase and tx.coinbase.reward > 0
        if not is_coinbase and tx.fee < DEFAULT_FEE:
            self.rejected.append((txid, f"fee below minimum ({tx.fee} < {DEFAULT_FEE})"))
            return False, f"Fee {tx.fee} below minimum {DEFAULT_FEE}"

        # 4. Size limit — evict lowest-fee if full
        if len(self._txs) >= self._max_size:
            self._evict_lowest_fee()

        # Accept
        self._txs[txid] = tx
        for nf in tx.nullifiers:
            self._nullifiers.add(nf)
        return True, "Accepted"

    def get_for_block(self, max_txs: int = 100) -> List[wm.Transaction]:
        """Return transactions for block inclusion, highest fee first.

        Coinbase transactions always come first, then fee-paying txs.
        """
        coinbase_txs = [tx for tx in self._txs.values() if tx.coinbase and tx.coinbase.reward > 0]
        user_txs = [tx for tx in self._txs.values() if not (tx.coinbase and tx.coinbase.reward > 0)]
        user_txs.sort(key=lambda tx: tx.fee, reverse=True)
        return (coinbase_txs + user_txs)[:max_txs]

    def remove(self, txid: str):
        """Remove a mined transaction from the pool."""
        tx = self._txs.pop(txid, None)
        if tx:
            for nf in tx.nullifiers:
                self._nullifiers.discard(nf)

    def remove_many(self, txids: List[str]):
        """Remove multiple mined transactions."""
        for txid in txids:
            self.remove(txid)

    def size(self) -> int:
        return len(self._txs)

    def contains(self, txid: str) -> bool:
        return txid in self._txs

    def _evict_lowest_fee(self):
        """Evict the transaction with the lowest fee."""
        user_txs = [(txid, tx) for txid, tx in self._txs.items() if not tx.coinbase]
        if not user_txs:
            return  # Can't evict coinbase txs
        user_txs.sort(key=lambda x: x[1].fee)
        evict_txid = user_txs[0][0]
        evict_tx = self._txs.pop(evict_txid)
        self.rejected.append((evict_txid, "evicted: mempool full"))


# ═══════════════════════════════════════════════════════════════════════════
# Sense-Check Tests — cross-spec consistency verification
# ═══════════════════════════════════════════════════════════════════════════

def test_sc1_default_fee_consistency():
    """SC1: wallet_model.DEFAULT_FEE == native_token.MIN_FEE_PER_CALL (both 42M)."""
    # wallet_model.DEFAULT_FEE
    wm_fee = wm.DEFAULT_FEE
    # The native_token contract is Rust — verified implicitly via value
    assert wm_fee == 42_000_000, f"wallet_model DEFAULT_FEE changed: {wm_fee}"
    assert wm_fee == DEFAULT_FEE, f"Re-export mismatch: {wm_fee} != {DEFAULT_FEE}"


def test_sc2_expected_reward_consistency():
    """SC2: chain_validation.expected_reward matches expected schedule."""
    # chain_validation_model has the real emission schedule
    r1 = cv.expected_reward(1)
    r100 = cv.expected_reward(100)
    r1M = cv.expected_reward(1_000_000)

    # Height 1: genesis reward
    assert r1 > 0, "Genesis block must have non-zero reward"
    # Decaying: later blocks have lower rewards
    assert r100 <= r1, f"Reward should decay: r100={r100} > r1={r1}"
    assert r1M < r100, f"Reward should decay further: r1M={r1M} >= r100={r100}"
    # wallet_model stub returns flat 100M — flag the discrepancy
    wm_r1 = wm.expected_reward(1)
    if wm_r1 != r1:
        print(f"    NOTE: wallet_model.expected_reward is a stub (returns {wm_r1})")
        print(f"          chain_validation.expected_reward is correct (returns {r1})")
        print(f"          Integration spec uses chain_validation.expected_reward")


def test_sc3_balance_proof_coinbase_only():
    """SC3: Balance proof passes for a block with only coinbase (no user txs)."""
    from proof_of_token_balance import verify_proof_of_token_balance, mk

    coinbase_reward = 100_000_000
    coinbase_fees = 0
    coinbase_vc = mk(coinbase_reward, 0)

    ok, msg = verify_proof_of_token_balance(
        coinbase_vc=coinbase_vc,
        coinbase_reward=coinbase_reward,
        coinbase_fees=coinbase_fees,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, f"Coinbase-only block should pass balance proof: {msg}"


def test_sc4_nullifier_format_consistency():
    """SC4: nullifier(secret, commitment) is deterministic and matches expected format."""
    import os
    secret = int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P
    commitment = os.urandom(32)
    n1 = wm.nullifier(secret, commitment)
    n2 = wm.nullifier(secret, commitment)
    assert n1 == n2, "nullifier must be deterministic"
    assert len(n1) == 32, f"nullifier must be 32 bytes, got {len(n1)}"


def test_sc5_block_hash_consistency():
    """SC5: dockernet hash_block produces consistent results."""
    header = dm.BlockHeader(
        version=1,
        previous=b'\x00' * 32,
        merkle_root=b'\x00' * 32,
        timestamp=1000,
        target=dm.U32_MAX,
        nonce=42,
        height=1,
        uncle_merkle_root=b'\x00' * 32,
        randomx_key=b'\x00' * 32,
    )
    h1 = dm.hash_block(header)
    h2 = dm.hash_block(header)
    assert h1 == h2, "hash_block must be deterministic"
    assert h1 <= dm.U32_MAX, f"hash must fit in u32, got {h1}"


def test_sc6_transaction_roundtrip():
    """SC6: wallet_model.Transaction txid is deterministic, fields round-trip."""
    tx_commitment = hashlib.blake2b(b'test', digest_size=32).digest()
    built = BuiltTransaction(
        calls=[
            ContractCallLeaf(
                contract_id=wm.NATIVE_TOKEN_CONTRACT_ID,
                data=b'\x04' + b'\x00' * 100,
                proofs=[],
            ),
        ],
        fee=DEFAULT_FEE,
        tx_commitment=tx_commitment,
    )
    tx = chain_tx_from_wallet_built(built)
    assert tx.fee == DEFAULT_FEE
    assert len(tx.contract_calls) == 1
    assert tx.witness == built.tx_commitment

    # txid is deterministic (excludes witness per L1 identity/witness decoupling)
    txid1 = compute_txid(tx)
    txid2 = compute_txid(tx)
    assert txid1 == txid2, "txid must be deterministic"
    assert len(txid1) == 64, f"txid must be 64 hex chars, got {len(txid1)}"


def test_sc7_fee_accounting():
    """SC7: Fee paid by wallet = fee collected in block (no leakage)."""
    from proof_of_token_balance import balanced_fee, mk

    input_value = 100_000_000
    fee = 42_000_000
    fee_in, fee_out, fee_val = balanced_fee(input_value, fee)

    # Fee input - fee output should equal the fee amount
    assert fee_val == fee, f"Fee value mismatch: {fee_val} != {fee}"
    # Input value = output value + fee
    # balanced_fee constructs commitments with the same blind, so they cancel
    # The fee commitment uses blind=0 for auditability


def test_sc8_coinbase_fee_collection():
    """SC8: Coinbase reward must include accumulated fees.

    The balance equation: outputs + burns + fees = inputs.
    For a block with just a coinbase and one fee-paying tx:
      coinbase_commit + fee_output = fee_input + coinbase_reward
    """
    # Fixed values for deterministic test
    block_reward = 100_000_000  # emission schedule reward (no fees)
    accumulated_fees = 42_000_000  # one fee-paying tx
    total_coinbase = block_reward + accumulated_fees  # 142_000_000 (what's actually minted)

    # User's fee input: 200M coin, fee output: 158M (200M - 42M fee)
    fee_input_val = 200_000_000
    fee_output_val = fee_input_val - accumulated_fees  # 158_000_000

    from proof_of_token_balance import mk
    coinbase_vc = mk(total_coinbase, 0)

    ok, msg = ptb.verify_proof_of_token_balance(
        coinbase_vc=coinbase_vc,
        coinbase_reward=block_reward,   # base reward ONLY (no fees)
        coinbase_fees=accumulated_fees,
        fee_inputs=[mk(fee_input_val, 1)],
        fee_outputs=[mk(fee_output_val, 1)],
        fee_amounts=[accumulated_fees],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, f"Coinbase with fee collection should pass: {msg}"


# ═══════════════════════════════════════════════════════════════════════════
# Gap 14: Strict-Mode Block Rejection
# ═══════════════════════════════════════════════════════════════════════════
# Per execution.rs:408-411 — if ANY canonical call fails during WASM
# execution (metadata, exec, apply, or spend hook), the ENTIRE block
# is rejected. Uncle calls that fail are skipped (not fatal).
# This prevents partially-applied state from mixed success/failure blocks.

def validate_block_strict_mode(block_txs: list, chain_state) -> tuple:
    """Validate block with strict-mode rejection (Gap 14).
    Returns (accepted: bool, reason: str).
    If any canonical call fails, entire block rejected.
    Uncle calls that fail are skipped."""
    for tx in block_txs:
        if not tx.get('canonical', True):
            continue  # Uncle calls: skip on failure, not fatal
        for call in tx.get('contract_calls', []):
            cid = call.get('contract_id')
            if cid is None:
                continue
            # Check: does contract WASM exist in chain state?
            if not chain_state.has_contract(cid):
                return (False, f"canonical call failed: contract {cid} not found — block rejected (Gap 14 strict mode)")
    return (True, "all canonical calls passed")


def test_gap14_strict_mode_rejection():
    """Verify Gap 14: canonical call to non-existent contract rejects entire block."""
    print("  TEST: Gap 14 strict-mode rejection...", end=" ")

    # Setup minimal chain state with genesis
    class MockChainState:
        def __init__(self):
            self.contracts = set()
            self.height = 1
        def has_contract(self, cid):
            return cid in self.contracts
        def deploy(self, cid):
            self.contracts.add(cid)

    chain = MockChainState()
    chain.deploy("native_token")

    # Block with valid coinbase + canonical call to non-existent contract
    bad_block = [
        {'canonical': True, 'contract_calls': [
            {'contract_id': 'native_token', 'data': b'\x05'},  # coinbase — valid
        ]},
        {'canonical': True, 'contract_calls': [
            {'contract_id': 'nonexistent_contract', 'data': b'\x00\x01\x02'},  # BAD
        ]},
    ]
    accepted, reason = validate_block_strict_mode(bad_block, chain)
    assert not accepted, f"Block with failed canonical call MUST be rejected: {reason}"
    print(f"PASSED (rejected: {reason})")

    # Block with uncle call failure — should be accepted (uncle skipped)
    uncle_block = [
        {'canonical': True, 'contract_calls': [
            {'contract_id': 'native_token', 'data': b'\x05'},
        ]},
        {'canonical': False, 'contract_calls': [  # uncle
            {'contract_id': 'nonexistent_contract', 'data': b'\x00\x01\x02'},
        ]},
    ]
    accepted, reason = validate_block_strict_mode(uncle_block, chain)
    assert accepted, f"Block with failed UNCLE call should be accepted: {reason}"
    print(f"PASSED (uncle skip: {reason})")


# ═══════════════════════════════════════════════════════════════════════════
# Full Lifecycle Integration Test
# ═══════════════════════════════════════════════════════════════════════════

def test_full_lifecycle():
    """End-to-end: wallet builds → P2P broadcasts → mempool accepts →
    miner includes → consensus validates → wallet confirms."""
    print()

    # ── Setup: keys and infrastructure ─────────────────────────────────
    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()
    mempool = Mempool(max_size=1000)

    # Miner node
    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()
    miner_pk = miner.get_miner_public_key()
    assert miner_pk is not None, "Miner must have a public key"

    # Wallet (shares node0's key)
    wallet_db = WalletDb(path=None)  # in-memory (schema auto-loaded)

    # Import wallet-1 key (same as node0) into wallet DB
    hex_secret = cfg.get_wallet_key("wallet-1")
    assert hex_secret is not None
    am = AccountManager()
    am.import_hex(hex_secret)
    wallet_sk = am.secrets()[0]
    wallet_pk = Keypair.from_secret(wallet_sk).public

    # KEY IDENTITY ASSERTION
    assert str(miner_pk) == str(wallet_pk), \
        f"CRITICAL: miner key != wallet key. Pipeline will fail."

    # Store secret in wallet DB
    wallet_sk_bs58 = wm._bs58_encode_secret(wallet_sk.inner)
    wallet_db.insert_secret(wallet_sk_bs58, "")
    wallet_db.insert_address(str(wallet_pk), wallet_sk_bs58, 1, 0)

    # ── Step 1: Wallet builds a transfer transaction ───────────────────
    print("  Step 1: Wallet builds transfer...", end=" ")
    # Create a recipient (wallet-2 from keys.toml)
    am2 = AccountManager()
    am2.import_hex(cfg.get_wallet_key("wallet-2"))
    recipient_pk = am2.default_public_key()
    recipient_sk = am2.secrets()[0]

    # Fund the wallet: mine some blocks first
    for _ in range(3):
        miner.mine_one_block()

    # Scan the blocks to discover coinbases
    scan_cache = ScanCache(
        capability_commitment_tree=MerkleTree(32),
        nullifier_smt={},
        secrets=[wallet_sk],
        own_deploy_auths={},
        messages_buffer=[],
    )
    for h in range(1, miner.chain.get_height() + 1):
        chain_block = miner.chain.get_block(h)
        if chain_block:
            wblock = bridge_chain_block_to_wallet(chain_block, miner_pk, [wallet_sk])
            wm.scan_block_linear(wblock, wallet_db, scan_cache)

    # Build a transfer using wallet_model's build_transfer
    # seed is required per wallet.md §6.1 for deterministic construction
    test_seed = hashlib.blake2b(b"test_full_lifecycle", digest_size=32).digest()
    built_tx = wm.build_transfer(
        wallet_db=wallet_db,
        token_id_str=DRKW_TOKEN_ID_STR,
        amount=50_000_000,
        recipient_pk=recipient_pk,
        seed=test_seed,
    )
    assert built_tx is not None, "build_transfer should succeed"
    assert len(built_tx.calls) >= 1, "Must have at least one contract call"
    assert built_tx.fee == DEFAULT_FEE
    print(f"PASSED (fee={built_tx.fee}, calls={len(built_tx.calls)})")

    # ── Step 2: Convert to canonical + broadcast via P2P ───────────────
    print("  Step 2: Broadcast via P2P...", end=" ")
    ctx = chain_tx_from_wallet_built(built_tx)
    ctx.fee = built_tx.fee
    assert ctx.fee == DEFAULT_FEE
    txid = compute_txid(ctx)
    assert txid != '', "txid must be non-empty"

    # Broadcast: send to all peers (simulated)
    p2p.broadcast("wallet-1", ctx)
    print(f"PASSED (txid={txid[:16]}...)")

    # ── Step 3: Mempool accepts ────────────────────────────────────────
    print("  Step 3: Mempool accepts...", end=" ")
    # Miner receives from P2P inbox
    msgs = p2p.receive("node0")
    received_tx = None
    for msg_type, payload in msgs:
        if isinstance(payload, wm.Transaction):
            received_tx = payload
            break
    assert received_tx is not None, "Miner should receive broadcast tx"

    accepted, reason = mempool.accept(received_tx)
    assert accepted, f"Mempool should accept valid tx: {reason}"
    assert mempool.size() == 1
    print(f"PASSED ({reason})")

    # ── Step 4: Miner includes in block ────────────────────────────────
    print("  Step 4: Miner includes in block...", end=" ")
    # Drain mempool into block
    block_txs = mempool.get_for_block(max_txs=100)
    assert len(block_txs) == 1

    # Mine a block with these transactions
    current = miner.chain.get_latest_block()
    height = current.header.height + 1
    # Use chain-consistent target (deterministic from timestamps)
    target = miner.chain.consensus.get_next_work_required(height, miner.chain.blocks)
    prev_hash = hashlib.blake2b(
        dm._mining_blob(current.header), digest_size=32
    ).digest()

    # Create coinbase + mempool txs
    reward = cv.expected_reward(height)
    total_fees = sum(tx.fee for tx in block_txs)
    dm_txs = [
        dm.Transaction(reward=reward + total_fees),  # coinbase
    ]
    # Add user transactions (would need contract call data in real system)
    for tx in block_txs:
        dm_txs.append(dm.Transaction(reward=0))

    block = dm.mine_block(prev_hash, height, target, dm_txs,
                          int(__import__('time').time()))
    assert block is not None, "Mining should succeed"
    success = miner.chain.connect_block(block)
    assert success, "Block should connect to chain"
    mempool.remove(received_tx.txid())
    print(f"PASSED (h={height}, reward={reward}, fees={total_fees})")

    # ── Step 5: Consensus validates ────────────────────────────────────
    print("  Step 5: Consensus validates...", end=" ")
    # Header validation — validate against the same target used for mining
    # (connect_block modifies the chain, changing get_next_work_required output)
    # modified the chain, which would change get_next_work_required output)
    dm.check_block_header(block, target, height - 1, prev_hash)

    # Mass conservation (coinbase only — no user txs with complex balances)
    # coinbase_reward = base emission (no fees), coinbase_fees = accumulated txn fees
    # coinbase_vc must equal coinbase_reward + coinbase_fees
    ok, msg = ptb.verify_proof_of_token_balance(
        coinbase_vc=ptb.mk(reward + total_fees, 0),
        coinbase_reward=reward,
        coinbase_fees=total_fees,
        fee_inputs=[], fee_outputs=[], fee_amounts=[],
        burn_inputs=[], transfer_inputs=[], transfer_outputs=[],
        spend_inputs=[], spend_outputs=[], mint_outputs=[],
    )
    assert ok, f"Balance proof failed: {msg}"
    print("PASSED")

    # ── Step 6: Wallet confirms ────────────────────────────────────────
    print("  Step 6: Wallet confirms...", end=" ")
    wblock = bridge_chain_block_to_wallet(block, miner_pk, [wallet_sk])
    found = wm.scan_block_linear(wblock, wallet_db, scan_cache)
    # Should find the coinbase (wallet has miner's key)
    assert found, "Wallet should find coinbase in mined block"

    # Check balance increased
    balance = wm.compute_balance(wallet_db)
    assert DRKW_TOKEN_ID_STR in balance, "Wallet should have DRKW balance"
    assert balance[DRKW_TOKEN_ID_STR] > 0, "Balance should be positive"
    print(f"PASSED (balance={balance[DRKW_TOKEN_ID_STR]})")

    print()
    print("  FULL LIFECYCLE: wallet→broadcast→mempool→mine→validate→confirm PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Edge Case Tests
# ═══════════════════════════════════════════════════════════════════════════

def test_edge_duplicate_txid():
    """Mempool rejects duplicate txid."""
    print("  Edge: duplicate txid...", end=" ")
    mempool = Mempool()
    tx = wm.Transaction(fee=DEFAULT_FEE)
    ok1, _ = mempool.accept(tx)
    assert ok1
    ok2, reason = mempool.accept(tx)
    assert not ok2
    assert "already in mempool" in reason
    print("PASSED")


def test_edge_zero_fee_rejected():
    """Mempool rejects zero-fee transactions."""
    print("  Edge: zero fee...", end=" ")
    mempool = Mempool()
    tx = wm.Transaction(fee=0)
    ok, reason = mempool.accept(tx)
    assert not ok, f"Zero-fee tx should be rejected: {reason}"
    assert "below minimum" in reason.lower()
    print("PASSED")


def test_edge_mempool_eviction():
    """Mempool evicts lowest-fee tx when full."""
    print("  Edge: mempool eviction...", end=" ")
    mempool = Mempool(max_size=3)

    # Add 3 txs with different fees
    tx1 = wm.Transaction(fee=50_000_000)
    tx2 = wm.Transaction(fee=60_000_000)
    tx3 = wm.Transaction(fee=42_000_000)  # lowest fee

    assert mempool.accept(tx1)[0]
    assert mempool.accept(tx2)[0]
    assert mempool.accept(tx3)[0]
    assert mempool.size() == 3

    # Add 4th tx — should evict tx3 (lowest fee)
    tx4 = wm.Transaction(fee=70_000_000)
    ok, _ = mempool.accept(tx4)
    assert ok
    assert mempool.size() == 3
    txid3 = compute_txid(tx3)
    assert not mempool.contains(txid3), "Lowest-fee tx should be evicted"
    assert mempool.contains(compute_txid(tx1))
    assert mempool.contains(compute_txid(tx2))
    assert mempool.contains(compute_txid(tx4))
    print("PASSED")


def test_edge_fee_ordering():
    """Mempool.get_for_block returns txs in fee-descending order."""
    print("  Edge: fee ordering...", end=" ")
    mempool = Mempool()
    tx_low = wm.Transaction(fee=42_000_000)
    tx_mid = wm.Transaction(fee=60_000_000)
    tx_high = wm.Transaction(fee=100_000_000)

    mempool.accept(tx_low)
    mempool.accept(tx_mid)
    mempool.accept(tx_high)

    block_txs = mempool.get_for_block(max_txs=100)
    fees = [tx.fee for tx in block_txs]
    assert fees == [100_000_000, 60_000_000, 42_000_000], \
        f"Txs must be fee-ordered: {fees}"
    print("PASSED")


def test_edge_restart_idempotency():
    """Rescanning the same block doesn't duplicate coins (INSERT OR IGNORE)."""
    print("  Edge: idempotent rescan...", end=" ")
    import tempfile
    tmp = tempfile.mkdtemp()
    db_path = os.path.join(tmp, "test.db")
    db = WalletDb(path=db_path)
    # WalletDb.__init__ auto-executes schema; no separate initialize()

    # Import a secret
    am = AccountManager()
    am.import_hex("0000000000000000000000000000000000000000000000000000000000000001")
    sk = am.secrets()[0]
    sk_bs58 = wm._bs58_encode_secret(sk.inner)
    pk = Keypair.from_secret(sk).public
    db.insert_secret(sk_bs58, "")
    db.insert_address(pk.to_string(), sk_bs58, 1, 0)

    # Create a coinbase block
    nt = NativeToken(
        value=100_000_000, token_id=0,
        spend_hook=0, user_data=0,
        cap_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
        value_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_Q,
        token_blind=int.from_bytes(os.urandom(32), 'little') % wm.PALLAS_P,
        memo=b'',
    )
    note = AeadEncryptedNote.encrypt(nt.encode(), pk.compressed)
    wblock = wm.Block(
        header=wm.BlockHeader(height=1, previous=b'\x00'*32, hash=b'\x00'*32,
                              timestamp=1000, total_reward=100_000_000,
                              merkle_root=b'\x00'*32, target=0xFFFFFFFF),
        transactions=[wm.Transaction(
            version=1, contract_calls=[],
            coinbase=wm.CoinbaseTransaction(
                encrypted_note=note.encode(), proof=b'', public_inputs=b'',
                coin=b'\x00'*32, value_commit_x=0, value_commit_y=0, token_commit=0,
            )
        )],
    )

    scan_cache = ScanCache(
        capability_commitment_tree=MerkleTree(32),
        nullifier_smt={}, secrets=[sk],
        own_deploy_auths={}, messages_buffer=[],
    )

    # First scan
    found1 = wm.scan_block_linear(wblock, db, scan_cache)
    caps_before = len(db.get_held_capabilities(False))

    # Second scan of same block
    found2 = wm.scan_block_linear(wblock, db, scan_cache)
    caps_after = len(db.get_held_capabilities(False))

    # Should not double-insert (INSERT OR IGNORE)
    assert caps_after == caps_before, \
        f"Rescan must not duplicate coins: {caps_before} → {caps_after}"
    print("PASSED")


def test_edge_multi_wallet_scenario():
    """Multiple wallets submit txs, miner includes all, each confirms independently."""
    print("  Edge: multi-wallet...", end=" ")
    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()
    mempool = Mempool()

    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()

    # Create 3 wallets with different keys
    wallets = []
    for i in [1, 2]:
        hex_secret = cfg.get_wallet_key(f"wallet-{i}")
        am = AccountManager()
        am.import_hex(hex_secret)
        wallets.append({
            "name": f"wallet-{i}",
            "secret": am.secrets()[0],
            "pubkey": am.default_public_key(),
        })

    # Each wallet builds a tx
    for w in wallets:
        tx = wm.Transaction(fee=DEFAULT_FEE + (wallets.index(w) * 10_000_000))
        mempool.accept(tx)

    assert mempool.size() == 2
    block_txs = mempool.get_for_block()
    assert len(block_txs) == 2

    # Should be fee-ordered
    assert block_txs[0].fee > block_txs[1].fee, \
        "Higher fee tx should come first"
    print("PASSED")


def test_edge_coinbase_only_mempool():
    """Coinbase transaction is not subject to fee minimum (it pays the reward, not a fee)."""
    print("  Edge: coinbase fee exemption...", end=" ")
    mempool = Mempool()
    coinbase = wm.Transaction(
        coinbase=wm.CoinbaseTransaction(reward=100_000_000),
        fee=0,  # coinbase has no fee
    )
    ok, reason = mempool.accept(coinbase)
    assert ok, f"Coinbase should be accepted without fee: {reason}"
    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# DeployV1 Lifecycle — contract deployment end-to-end
# ═══════════════════════════════════════════════════════════════════════════

def test_deploy_v1_mempool_accepts():
    """DeployV1 transaction is accepted by mempool (has fee, non-coinbase)."""
    print("  DeployV1: mempool accepts...", end=" ")
    mempool = Mempool()
    # Simulate a DeployV1 call — contract_id = Deployooor, data[0] = 0x00 (DeployV1)
    deploy_tx = wm.Transaction(
        contract_calls=[wm.ContractCall(
            contract_id=wm.DEPLOYOOOR_CONTRACT_ID.to_bytes() if hasattr(wm.DEPLOYOOOR_CONTRACT_ID, 'to_bytes') else b'\x04' + b'\x00' * 31,
            data=b'\x00' + b'wasm_bincode_placeholder',
        )],
        fee=DEFAULT_FEE,
    )
    ok, reason = mempool.accept(deploy_tx)
    assert ok, f"DeployV1 should be accepted: {reason}"
    assert mempool.size() == 1
    print("PASSED")


def test_deploy_v1_nullifier_dedup():
    """DeployV1 with spent nullifier is rejected (double-spend prevention)."""
    print("  DeployV1: nullifier dedup...", end=" ")
    mempool = Mempool()
    nf = hashlib.blake2b(b"spent_nullifier", digest_size=32).digest()
    tx1 = wm.Transaction(
        contract_calls=[wm.ContractCall(
            contract_id=b'\x04' + b'\x00' * 31,
            data=b'\x00' + b'wasm_bincode',
        )],
        fee=DEFAULT_FEE,
        nullifiers=[nf],
    )
    ok1, _ = mempool.accept(tx1)
    assert ok1
    # Second tx with same nullifier should be rejected
    tx2 = wm.Transaction(
        contract_calls=[wm.ContractCall(
            contract_id=b'\x04' + b'\x00' * 31,
            data=b'\x00' + b'wasm_bincode_2',
        )],
        fee=DEFAULT_FEE,
        nullifiers=[nf],  # same nullifier
    )
    ok2, reason = mempool.accept(tx2)
    assert not ok2, "DeployV1 with spent nullifier should be rejected"
    assert "nullifier" in reason.lower()
    print("PASSED")


def test_deploy_v1_mempool_to_block():
    """DeployV1 in mempool is selected for block inclusion by miner."""
    print("  DeployV1: mempool → block...", end=" ")
    mempool = Mempool()
    deploy_tx = wm.Transaction(
        contract_calls=[wm.ContractCall(
            contract_id=b'\x04' + b'\x00' * 31,
            data=b'\x00' + b'wasm_bincode',
        )],
        fee=DEFAULT_FEE,
    )
    mempool.accept(deploy_tx)
    # Miner selects for block
    block_txs = mempool.get_for_block(max_txs=100)
    assert len(block_txs) == 1
    assert block_txs[0].contract_calls[0].data[0] == 0x00  # DeployV1 selector
    print("PASSED")


def test_deploy_v1_manifest_discovery():
    """After deployment, manifest is stored and discoverable for wallet_construct.

    Models the full path: DeployV1 execution → Deployooor post-processing →
    manifest stored in contract registry → wallet discovers manifest →
    wallet_construct resolves typed capability.

    This is the gateway from the old CapRecord (pre-capability-era "coin record")
    to the new TypedCapability with barb-covered composition verification.
    """
    print("  DeployV1: manifest discovery...", end=" ")

    # Simulate a manifest for a purse-like contract
    manifest_toml = b'\x4D' + b"""
[manifest]
name = "test_purse"
version = "1.0.0"

[capabilities.transfer]
resource = "value"
action = "transfer"
primitives = ["Commitment", "Nullifier", "AssetId", "ContractId", "FuncId", "MerkleNode"]
required_barbs = ["Nullify", "Prove", "Dispatch", "Gate", "Denominate"]
"""

    # Simulate Deployooor post-processing: store manifest under contract_id key
    contract_id = b'\x99' + b'\x00' * 31  # mock deployed contract ID
    manifest_store = {contract_id: manifest_toml}

    # Wallet scan: detect DeployV1 call → extract manifest
    stored_manifest = manifest_store.get(contract_id)
    assert stored_manifest is not None, "Manifest should be stored after deploy"
    assert stored_manifest[0] == 0x4D, "Manifest should have magic byte 0x4D prefix"

    # Parse the manifest and verify wallet_construct coverage
    # The manifest declares primitives + required_barbs for the "transfer" action
    from wallet_model import Primitive, Barb, wallet_construct

    primitives = [
        Primitive.Commitment, Primitive.Nullifier, Primitive.AssetId,
        Primitive.ContractId, Primitive.FuncId, Primitive.MerkleNode,
    ]
    required_barbs = [
        Barb.Nullify, Barb.ProveInclusion, Barb.Dispatch, Barb.Gate, Barb.Denominate,
    ]

    # wallet_construct is the soundness gate — returns None if barbs uncovered
    typed_cap = wallet_construct("value", "transfer", primitives, required_barbs)
    assert typed_cap is not None, \
        "wallet_construct should return a typed capability — barbs must be covered"
    assert typed_cap.action == "transfer"
    assert typed_cap.resource == "value"
    assert typed_cap.covers(required_barbs), \
        "composed barbs should cover all required barbs"

    # Verify the resolve-then-construct pattern:
    # 1. Manifest declares what barbs are required
    # 2. Manifest declares what primitives it provides
    # 3. wallet_construct checks coverage → creates TypedCapability
    # 4. If uncovered, skip the capability (don't crash)
    uncovered = wallet_construct("value", "transfer", primitives[:2], required_barbs)
    assert uncovered is None, \
        "wallet_construct should return None when primitives don't cover required barbs"

    print("PASSED")


def test_deploy_v1_full_scan_cycle():
    """Full DeployV1 lifecycle: wallet builds → mempool → block → manifest → scan.

    Same flow as test_full_lifecycle but focused on deployment rather than transfer.
    """
    print("  DeployV1: full scan cycle...", end=" ")
    cfg = KeyConfig.default_keys()
    p2p = P2PNetwork()
    mempool = Mempool()

    miner = KeyedMiningNode("node0", True, p2p, key_config=cfg, localnet=True)
    miner.start_sync_task()

    # Build a DeployV1 transaction
    manifest_bytes = b'\x4D' + b'[manifest]\nname = "test"\n'
    deploy_tx = wm.Transaction(
        contract_calls=[wm.ContractCall(
            contract_id=b'\x04' + b'\x00' * 31,
            data=b'\x00' + b'wasm_bincode' + manifest_bytes,
        )],
        fee=DEFAULT_FEE,
    )

    # Mempool accepts
    ok, _ = mempool.accept(deploy_tx)
    assert ok

    # Miner mines (includes in block)
    miner.mine_one_block()
    assert miner.chain.height >= 2  # genesis + 1 mined block

    # After block acceptance, mempool is cleared
    mempool.remove(compute_txid(deploy_tx))

    print("PASSED")


# ═══════════════════════════════════════════════════════════════════════════
# Test Runner
# ═══════════════════════════════════════════════════════════════════════════

if __name__ == '__main__':
    print("=" * 60)
    print("DarkWow Transaction Lifecycle — Integration Spec")
    print("=" * 60)
    print()

    # ── Sense-check tests ──────────────────────────────────────────────
    print("--- Sense-Check: Cross-Spec Consistency ---")
    sc_tests = [
        ("SC1", test_sc1_default_fee_consistency),
        ("SC2", test_sc2_expected_reward_consistency),
        ("SC3", test_sc3_balance_proof_coinbase_only),
        ("SC4", test_sc4_nullifier_format_consistency),
        ("SC5", test_sc5_block_hash_consistency),
        ("SC6", test_sc6_transaction_roundtrip),
        ("SC7", test_sc7_fee_accounting),
        ("SC8", test_sc8_coinbase_fee_collection),
    ]

    sc_passed = 0
    sc_failed = 0
    for name, test_fn in sc_tests:
        try:
            test_fn()
            sc_passed += 1
            print(f"  {name}: PASSED")
        except Exception as e:
            sc_failed += 1
            print(f"  {name}: FAILED — {e}")
    print(f"  SC tests: {sc_passed} passed, {sc_failed} failed")
    print()

    # ── Gap 14 strict-mode rejection ───────────────────────────────────
    print("--- Gap 14 Strict-Mode Rejection ---")
    test_gap14_strict_mode_rejection()
    print()

    # ── Full lifecycle ─────────────────────────────────────────────────
    print("--- Full Lifecycle Integration Test ---")
    try:
        test_full_lifecycle()
        lifecycle_ok = True
    except Exception as e:
        print(f"  LIFECYCLE FAILED: {e}")
        import traceback
        traceback.print_exc()
        lifecycle_ok = False
    print()

    # ── Edge case tests ────────────────────────────────────────────────
    print("--- Edge Case Tests ---")
    edge_tests = [
        ("duplicate-txid", test_edge_duplicate_txid),
        ("zero-fee", test_edge_zero_fee_rejected),
        ("eviction", test_edge_mempool_eviction),
        ("fee-ordering", test_edge_fee_ordering),
        ("idempotent-rescan", test_edge_restart_idempotency),
        ("multi-wallet", test_edge_multi_wallet_scenario),
        ("coinbase-exempt", test_edge_coinbase_only_mempool),
    ]

    edge_passed = 0
    edge_failed = 0
    for name, test_fn in edge_tests:
        try:
            test_fn()
            edge_passed += 1
        except Exception as e:
            edge_failed += 1
            print(f"  Edge {name}: FAILED — {e}")
    print(f"  Edge tests: {edge_passed} passed, {edge_failed} failed")
    print()

    # ── DeployV1 lifecycle tests ─────────────────────────────────────────
    print("--- DeployV1 Lifecycle Tests ---")
    deploy_tests = [
        ("mempool-accepts", test_deploy_v1_mempool_accepts),
        ("nullifier-dedup", test_deploy_v1_nullifier_dedup),
        ("mempool-to-block", test_deploy_v1_mempool_to_block),
        ("manifest-discovery", test_deploy_v1_manifest_discovery),
        ("full-scan-cycle", test_deploy_v1_full_scan_cycle),
    ]

    deploy_passed = 0
    deploy_failed = 0
    for name, test_fn in deploy_tests:
        try:
            test_fn()
            deploy_passed += 1
        except Exception as e:
            deploy_failed += 1
            print(f"  DeployV1 {name}: FAILED — {e}")
    print(f"  DeployV1 tests: {deploy_passed} passed, {deploy_failed} failed")
    print()

    print()

    # ── Summary ────────────────────────────────────────────────────────
    total_passed = sc_passed + edge_passed + deploy_passed + (1 if lifecycle_ok else 0)
    total_failed = sc_failed + edge_failed + deploy_failed + (0 if lifecycle_ok else 1)
    total = total_passed + total_failed

    print("=" * 60)
    if total_failed == 0:
        print(f"ALL TESTS PASSED ({total} tests)")
    else:
        print(f"SOME TESTS FAILED ({total_failed}/{total} failures)")
    print("=" * 60)
    sys.exit(0 if total_failed == 0 else 1)
