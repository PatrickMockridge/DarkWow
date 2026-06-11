#!/usr/bin/env python3
"""
Contract Invoke Model — 1:1 mapping of dwow_wallet contract invoke flow.

Models the full lookup chain from CLI invocation to call data construction.
Matches bin/drk/src/lib.rs:invoke_contract() and contract_metadata.rs.

The bug: `or_else` closure parses Base58 ContractId, then `and_then(|_| None)`
discards it. This model proves the fix before Rust implementation.

Usage:
  python3 contrib/model/contract_invoke_model.py
"""

import hashlib
import json
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple


# ============================================================================
# Contract Metadata — matches contract_metadata.rs CONTRACT_METADATA_REGISTRY
# ============================================================================

@dataclass
class FunctionSignature:
    name: str
    code: int           # function opcode byte
    requires_proof: bool
    proof_circuit: Optional[str] = None


@dataclass
class ContractMetadata:
    name: str
    functions: Dict[str, FunctionSignature]  # function_name -> sig


# Static compile-time registry (simplified subset matching Rust)
METADATA_REGISTRY: Dict[str, ContractMetadata] = {
    "promissory_note": ContractMetadata("promissory_note", {
        "TokenMintV1": FunctionSignature("TokenMintV1", 0x00, True, "token_mint_v1"),
        "RedeemV1":    FunctionSignature("RedeemV1", 0x01, True, "redeem_v1"),
        "MintV1":      FunctionSignature("MintV1", 0x02, True, "mint_v1"),
        "BurnV1":      FunctionSignature("BurnV1", 0x03, True, "burn_v1"),
        "TransferV1":  FunctionSignature("TransferV1", 0x04, True, "blind_output_v1"),
        "OtcSwapV1":   FunctionSignature("OtcSwapV1", 0x05, True, None),
    }),
    "escrow": ContractMetadata("escrow", {
        "CreateEscrow":  FunctionSignature("CreateEscrow", 0x00, True, "create_escrow_v1"),
        "FundEscrow":    FunctionSignature("FundEscrow", 0x01, True, "fund_v1"),
        "ClaimEscrow":   FunctionSignature("ClaimEscrow", 0x02, True, "claim_v1"),
        "RefundEscrow":  FunctionSignature("RefundEscrow", 0x03, True, "refund_v1"),
        "CancelEscrow":  FunctionSignature("CancelEscrow", 0x04, False, None),
    }),
    "auction": ContractMetadata("auction", {
        "CreateAuction":  FunctionSignature("CreateAuction", 0x00, True, "create_auction_v1"),
        "PlaceBid":       FunctionSignature("PlaceBid", 0x01, True, "place_bid_v1"),
        "CloseAuction":   FunctionSignature("CloseAuction", 0x02, False, None),
        "SettleAuction":  FunctionSignature("SettleAuction", 0x03, False, None),
        "ClaimAuction":   FunctionSignature("ClaimAuction", 0x04, True, "claim_winnings_v1"),
        "RefundBid":      FunctionSignature("RefundBid", 0x05, True, "refund_bid_v1"),
    }),
    "dex": ContractMetadata("dex", {
        "CreateSwap":   FunctionSignature("CreateSwap", 0x00, True, "create_swap_v1"),
        "AcceptSwap":   FunctionSignature("AcceptSwap", 0x01, True, "accept_swap_v1"),
        "ExecuteSwap":  FunctionSignature("ExecuteSwap", 0x02, True, "execute_swap_v1"),
        "CancelSwap":   FunctionSignature("CancelSwap", 0x03, False, None),
    }),
    "subscription": ContractMetadata("subscription", {
        "Subscribe":         FunctionSignature("Subscribe", 0x00, True, None),
        "CancelSubscription": FunctionSignature("CancelSubscription", 0x01, False, None),
    }),
    "dao_escrow": ContractMetadata("dao_escrow", {
        "Initialize":       FunctionSignature("Initialize", 0x00, True, "init_v1"),
        "PayPremium":       FunctionSignature("PayPremium", 0x02, True, "pay_premium_v1"),
        "ProposeClaim":     FunctionSignature("ProposeClaim", 0x07, True, "propose_claim_v1"),
        "VoteClaim":        FunctionSignature("VoteClaim", 0x08, True, "vote_claim_v1"),
        "EnableDrainProtection": FunctionSignature("EnableDrainProtection", 0x0B, False, None),
    }),
    "drain_protection": ContractMetadata("drain_protection", {
        "Initialize": FunctionSignature("Initialize", 0x00, False, None),
    }),
    "bearer_bond": ContractMetadata("bearer_bond", {
        "IssueStake":        FunctionSignature("IssueStake", 0x00, True, None),
        "TransferStake":     FunctionSignature("TransferStake", 0x01, True, None),
        "RequestInterest":   FunctionSignature("RequestInterest", 0x02, True, None),
        "EmergencyUnstake":  FunctionSignature("EmergencyUnstake", 0x03, True, None),
        "Unstake":           FunctionSignature("Unstake", 0x04, True, None),
    }),
}


# ============================================================================
# ContractId Registry — matches contract_imports.rs OnceLock pattern
# ============================================================================

class ContractIdRegistry:
    """Emulates OnceLock<ContractId> per contract name.
    Matches contract_imports.rs register_contract_id()."""

    def __init__(self):
        self._ids: Dict[str, bytes] = {}  # name -> 32-byte ContractId

    def register(self, name: str, cid: bytes):
        if len(cid) != 32:
            raise ValueError("ContractId must be 32 bytes")
        self._ids[name] = cid

    def get(self, name: str) -> Optional[bytes]:
        return self._ids.get(name)

    def reverse_lookup(self, cid: bytes) -> Optional[str]:
        """Find contract name by ContractId. Used by the fix."""
        for name, registered_id in self._ids.items():
            if registered_id == cid:
                return name
        return None


# ============================================================================
# invoke_contract() — models lib.rs Drk::invoke_contract()
# ============================================================================

class UnknownContract(Exception):
    def __init__(self, id_or_name: str):
        super().__init__(f"Unknown contract: {id_or_name}")


class UnknownFunction(Exception):
    def __init__(self, function: str, contract: str):
        super().__init__(f"Unknown function: {function} on contract {contract}")


class ContractNotRegistered(Exception):
    def __init__(self, contract: str):
        super().__init__(f"Contract {contract} not registered in runtime")


class ZkProofRequired(Exception):
    def __init__(self, contract: str, function: str):
        super().__init__(f"ZK proof required: {contract} function '{function}' requires a ZK proof")


@dataclass
class InvokeResult:
    """Result of a successful invoke_contract call."""
    contract_name: str
    contract_id: bytes
    function_name: str
    function_code: int
    call_data: bytes
    requires_proof: bool


def invoke_contract(id_or_name: str, function: str,
                    registry: ContractIdRegistry,
                    params: Optional[dict] = None,
                    proofs: Optional[List[bytes]] = None) -> InvokeResult:
    """Full lookup chain matching lib.rs:1255-1435.

    Path A: Look up by contract name (e.g., "escrow") in metadata registry.
    Path B: Parse as Base58 ContractId, reverse-lookup name from registry.
    Then: resolve ContractId from runtime registry, look up function signature,
    build call data.

    The FIX: Path B now actually works — it reverse-lookups the name from
    the ContractIdRegistry instead of discarding the parsed ID.
    """
    import base58

    if proofs is None:
        proofs = []

    # --- Step 1: Look up metadata (Path A: by name, Path B: by ID) ---

    metadata = METADATA_REGISTRY.get(id_or_name)

    if metadata is None:
        # Path B: Try to parse as Base58 ContractId
        try:
            cid_bytes = base58.b58decode(id_or_name)
            if len(cid_bytes) == 32:
                # FIX: Reverse-lookup the contract name from the registry
                name = registry.reverse_lookup(cid_bytes)
                if name:
                    metadata = METADATA_REGISTRY.get(name)
        except Exception:
            pass

    if metadata is None:
        raise UnknownContract(id_or_name)

    # --- Step 2: Look up function signature ---

    func_sig = metadata.functions.get(function)
    if func_sig is None:
        raise UnknownFunction(function, metadata.name)

    # --- Step 3: Resolve ContractId from runtime registry ---

    cid = registry.get(metadata.name)
    if cid is None:
        raise ContractNotRegistered(metadata.name)

    # --- Step 4: Check ZK proof requirement ---

    if func_sig.requires_proof and not proofs:
        raise ZkProofRequired(metadata.name, function)

    # --- Step 5: Build call data ---

    call_data = bytes([func_sig.code])

    # Encode parameters based on contract + function
    if metadata.name == "escrow" and function == "CreateEscrow":
        # Escrow CreateEscrow needs: value, token_id, seller_pubkey, timeout
        if params:
            value = params.get("value", 1000)
            token_id = params.get("token_id", 0)
            seller_pk = params.get("seller_pk", "default")
            timeout = params.get("timeout", 100)
            # Simplified encoding
            call_data += value.to_bytes(8, 'little')
            call_data += token_id.to_bytes(32, 'little')
            call_data += seller_pk.encode()[:32].ljust(32, b'\x00')
            call_data += timeout.to_bytes(8, 'little')

    # Placeholder proofs (not actually validated in model)
    call_data += b"PROOF_PLACEHOLDER" * len(proofs)

    return InvokeResult(
        contract_name=metadata.name,
        contract_id=cid,
        function_name=function,
        function_code=func_sig.code,
        call_data=call_data,
        requires_proof=func_sig.requires_proof,
    )


# ============================================================================
# Tests
# ============================================================================

def test_invoke_by_name():
    """invoke by known contract name → success"""
    print("  Test 1: Invoke by name...", end=" ")
    registry = ContractIdRegistry()
    escrow_id = hashlib.blake2b(b"test_escrow_cid", digest_size=32).digest()
    registry.register("escrow", escrow_id)

    result = invoke_contract("escrow", "CancelEscrow", registry)
    assert result.contract_name == "escrow"
    assert result.contract_id == escrow_id
    assert result.function_code == 0x04
    assert not result.requires_proof
    print("PASSED")


def test_invoke_by_base58_id():
    """invoke by Base58 ContractId → reverse lookup → success"""
    print("  Test 2: Invoke by Base58 ID...", end=" ")
    import base58
    registry = ContractIdRegistry()
    escrow_id = hashlib.blake2b(b"test_escrow_cid_v2", digest_size=32).digest()
    registry.register("escrow", escrow_id)

    escrow_bs58 = base58.b58encode(escrow_id)
    if isinstance(escrow_bs58, bytes):
        escrow_bs58 = escrow_bs58.decode('ascii')

    # THIS WAS THE BUG — previously returned UnknownContract
    result = invoke_contract(escrow_bs58, "CreateEscrow", registry,
                             proofs=[b"mock_proof"])
    assert result.contract_name == "escrow"
    assert result.function_code == 0x00
    print("PASSED")


def test_unknown_contract_name():
    """unknown contract name → UnknownContract"""
    print("  Test 3: Unknown contract name...", end=" ")
    registry = ContractIdRegistry()
    try:
        invoke_contract("nonexistent_contract", "SomeFunction", registry)
        assert False, "should have raised"
    except UnknownContract:
        pass
    print("PASSED")


def test_unknown_function():
    """known contract, wrong function → UnknownFunction"""
    print("  Test 4: Unknown function...", end=" ")
    registry = ContractIdRegistry()
    escrow_id = hashlib.blake2b(b"test_escrow_cid_v3", digest_size=32).digest()
    registry.register("escrow", escrow_id)

    try:
        invoke_contract("escrow", "NonExistentFunction", registry)
        assert False, "should have raised"
    except UnknownFunction:
        pass
    print("PASSED")


def test_contract_not_registered():
    """known metadata, no runtime ID → ContractNotRegistered"""
    print("  Test 5: Contract not registered...", end=" ")
    registry = ContractIdRegistry()
    # escrow is in METADATA_REGISTRY but NOT in the runtime registry

    try:
        invoke_contract("escrow", "CancelEscrow", registry)
        assert False, "should have raised"
    except ContractNotRegistered:
        pass
    print("PASSED")


def test_zk_proof_required():
    """function requires ZK proof, none provided → ZkProofRequired"""
    print("  Test 6: ZK proof required...", end=" ")
    registry = ContractIdRegistry()
    escrow_id = hashlib.blake2b(b"test_escrow_cid_v4", digest_size=32).digest()
    registry.register("escrow", escrow_id)

    try:
        # CreateEscrow requires_proof=True, no proofs provided
        invoke_contract("escrow", "CreateEscrow", registry, proofs=[])
        assert False, "should have raised"
    except ZkProofRequired:
        pass
    print("PASSED")


def test_reverse_lookup_unknown_id():
    """Base58 ID not in registry → UnknownContract (not crash)"""
    print("  Test 7: Unknown Base58 ID...", end=" ")
    import base58
    registry = ContractIdRegistry()
    unknown_id = hashlib.blake2b(b"nonexistent_contract_cid", digest_size=32).digest()
    unknown_bs58 = base58.b58encode(unknown_id)
    if isinstance(unknown_bs58, bytes):
        unknown_bs58 = unknown_bs58.decode('ascii')

    try:
        invoke_contract(unknown_bs58, "SomeFunction", registry)
        assert False, "should have raised"
    except UnknownContract:
        pass
    print("PASSED")


# ============================================================================
# Test runner
# ============================================================================

def run_all_tests():
    print("=" * 60)
    print("Contract Invoke Model — Python Specification")
    print("=" * 60)

    tests = [
        test_invoke_by_name,
        test_invoke_by_base58_id,
        test_unknown_contract_name,
        test_unknown_function,
        test_contract_not_registered,
        test_zk_proof_required,
        test_reverse_lookup_unknown_id,
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
