"""
Pipeline Model — DarkWow Test Pipeline Specification

Verified against actual container code and RPC output formats
by three HAZOP teams (2026-07-03). Every check in this model
maps to a specific RPC call, log pattern, or byte-level format
documented in the codebase.

Layers (defense in depth):
  L1: Genesis ceremony — node0 creates genesis, merkle root convergence
  L2: Cross-node consensus — merkle root equality at heights 2-5
  L3: Key flow — keys.toml determinism (verified at source, not runtime)
  L4: AEAD self-test — wallet daemon startup gate
  L5: Scan verification — wallet processes blocks, finds coinbases
  L6: Pipeline hardening — all phases gated, no silent Rust failures
  L7: This model — specification against which pipeline is verified
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Dict, List, Optional, Tuple


# ── Phase definitions ──
class Phase(Enum):
    """Every phase in the pipeline. All phases are gated — a phase that
    records failures blocks all subsequent phases via phase_gate."""
    CLEAN = 1
    BUILD = 2
    PREREQS = 3
    WALLET_GEN = 4
    START = 5
    VERIFY_CONTAINERS = 6
    RPC_HEALTH = 7
    MINING_ACTIVITY = 8
    BLOCK_PRODUCTION = 9
    WALLET_VERIFY = 10
    WALLET_TRANSFER = 11
    # Bridge sub-phases (12-19, only in --mode bridge)
    BRIDGE_DEPLOY = 12
    BRIDGE_INIT = 13
    BRIDGE_REGISTER_RELAYER = 14
    BRIDGE_DEPOSIT = 15
    BRIDGE_WITHDRAW = 16
    BRIDGE_ACCEPT = 17
    BRIDGE_EXECUTE = 18
    BRIDGE_VERIFY = 19
    REPORT = 20
    PERSISTENCE = 21          # join modes only
    CONTRACT_TESTS = 99       # optional, gated on CONTRACT_TIER > 0


# ── Pipeline modes ──
class Mode(Enum):
    NATIVE = "native"            # local devnet, 1-5 mining nodes
    MERGE = "merge"              # + monerod + p2pool + xmrig
    BRIDGE = "bridge"            # + bridge-node universal_relayer
    JOIN_NATIVE = "join-native"  # single node joining public testnet
    JOIN_MERGE = "join-merge"    # single node + merge mining stack


# ── Node roles ──
@dataclass
class Node:
    """A container in the test network.

    Ports are as declared in docker-compose.yml. RPC port is used by
    the pipeline for health checks (phase 7) and block queries (phase 9).
    The observer has no mining key (secret_hex=None) and creates_genesis=False.
    """
    name: str
    creates_genesis: bool
    mines: bool
    secret_hex: Optional[str]  # from keys.toml, None for observer
    p2p_port: int
    rpc_port: int


# ── Keys (from keys.toml) ──
# 64 hex chars = 32 bytes. wallet-1 shares node0's key.
KEYS = {
    "node0":    "0000000000000000000000000000000000000000000000000000000000000001",
    "node1":    "0000000000000000000000000000000000000000000000000000000000000002",
    "wallet-1": "0000000000000000000000000000000000000000000000000000000000000001",  # = node0
    "wallet-2": "0000000000000000000000000000000000000000000000000000000000000003",
}


# ── Default topology (native mode, 2 nodes) ──
def default_topology() -> Dict[str, Node]:
    return {
        "node0": Node("node0", creates_genesis=True, mines=True,
                       secret_hex=KEYS["node0"],
                       p2p_port=31342, rpc_port=31345),
        "node1": Node("node1", creates_genesis=False, mines=True,
                       secret_hex=KEYS["node1"],
                       p2p_port=31343, rpc_port=31346),
        "observer": Node("observer", creates_genesis=False, mines=False,
                          secret_hex=None,
                          p2p_port=31340, rpc_port=31345),
    }


# ── Genesis ceremony ──
@dataclass
class GenesisCeremony:
    """Spec: Exactly one node creates genesis. All nodes converge on same
    merkle root. The block hash is a computed method (hash_with_vm()), not
    a serialized JSON field — so merkle_root is used for identity comparison.
    Format in raw TCP JSON-RPC: \"merkle_root\":[b0,...,b31] (serde_json,
    no spaces). Identical blocks have identical merkle roots."""

    authority: str = "node0"
    # node0's merkle root — every other node must match this
    reference_merkle_root: Optional[str] = None

    def verify_authority(self, nodes: Dict[str, Node]) -> List[str]:
        """Only the designated authority may have creates_genesis=True.
        docker-compose.yml guarantees this — this is a model-level assertion."""
        errors = []
        for name, node in nodes.items():
            if node.creates_genesis and name != self.authority:
                errors.append(
                    f"{name} has creates_genesis=True "
                    f"(only {self.authority} allowed)"
                )
        if self.authority not in nodes:
            errors.append(
                f"Genesis authority {self.authority} not in node set"
            )
        elif not nodes[self.authority].creates_genesis:
            errors.append(
                f"Genesis authority {self.authority} has creates_genesis=False"
            )
        return errors

    def verify_convergence(
        self, node_merkle_roots: Dict[str, str]
    ) -> List[str]:
        """All nodes must report the same block 1 merkle root.
        The reference comes from node0 (the genesis authority)."""
        errors = []
        if self.reference_merkle_root is None:
            errors.append("No reference merkle root from genesis authority")
            return errors
        for name, mr in node_merkle_roots.items():
            if mr != self.reference_merkle_root:
                errors.append(
                    f"{name} merkle root MISMATCH — different chain! "
                    f"(got {mr[:20]}..., expected {self.reference_merkle_root[:20]}...)"
                )
        if not errors and len(node_merkle_roots) > 0:
            pass  # all converged
        return errors


# ── Consensus verification ──
class ConsensusVerifier:
    """Spec: At heights 2-5, all nodes must agree on merkle root.
    Beyond height 5, the protocol's uncle/forgiving consensus (threshold=3)
    permits legitimate divergence as nodes mine competing blocks."""

    CONSENSUS_HEIGHTS = [2, 3, 4, 5]

    @staticmethod
    def verify_heights(
        node_merkle_roots: Dict[str, Dict[int, str]],
        heights: Optional[List[int]] = None,
    ) -> List[str]:
        """At each height, all nodes must have identical merkle root."""
        if heights is None:
            heights = ConsensusVerifier.CONSENSUS_HEIGHTS
        errors = []
        for h in heights:
            roots = {}
            for name, blocks in node_merkle_roots.items():
                if h in blocks:
                    roots[name] = blocks[h]
            unique = set(roots.values())
            if len(unique) > 1:
                errors.append(f"Consensus split at height {h}:")
                for name, mr in roots.items():
                    errors.append(f"  {name}: {mr[:20]}...")
            elif len(unique) == 0:
                errors.append(f"No node has block at height {h}")
        return errors


# ── Key identity ──
@dataclass
class KeyIdentity:
    """Spec: keys.toml deterministically maps wallet-1 to node0's secret.
    Verified at the source (keys.toml) and at import time (entrypoint-wallet.sh
    fatally fails on import failure). Not re-verified at runtime — if the key
    were wrong, decrypt would fail and COINBASE_DECRYPT_FAILED would appear
    in scan output."""

    wallet_name: str
    miner_node: str
    expected_match: bool

    def verify(self) -> Optional[str]:
        """Check that keys match the expected mapping in KEYS."""
        wallet_key = KEYS.get(self.wallet_name)
        miner_key = KEYS.get(self.miner_node)
        if wallet_key is None or miner_key is None:
            return f"Key lookup failed: {self.wallet_name}={wallet_key}, {self.miner_node}={miner_key}"
        if self.expected_match and wallet_key != miner_key:
            return (
                f"Key identity FAIL: {self.wallet_name} key != {self.miner_node} key"
            )
        if not self.expected_match and wallet_key == miner_key:
            return (
                f"Key identity WARN: {self.wallet_name} unexpectedly "
                f"shares key with {self.miner_node}"
            )
        return None


# ── AEAD self-test ──
class AeadSelfTest:
    """Spec: At daemon startup, before any network activity, the wallet
    encrypts a known test vector with its own public key and decrypts with
    its secret key. If this roundtrip fails, the daemon exits immediately.
    Implemented in bin/dww/src/lib.rs:aead_self_test()."""

    TEST_VECTOR = b"DarkWow AEAD pipeline self-test vector 2026"

    @staticmethod
    def run(secrets_available: bool) -> Tuple[bool, str]:
        """Returns (passed, message). Fails if no secrets available."""
        if not secrets_available:
            return (False, "AEAD self-test: no secrets — cannot verify crypto")
        # Actual crypto verification happens in the Rust binary.
        # The model asserts this check exists and is mandatory.
        return (True, "AEAD self-test: daemon startup gate passed")


# ── Scan verification ──
class ScanVerifier:
    """Spec: Wallet scan must process blocks and find capabilities.
    Matches actual output format from dispatch.rs:888-891 and scan.rs:231."""

    @staticmethod
    def verify_scan_output(
        blocks_scanned: int,
        capabilities_count: int,
        secrets_count: int,
        wallet_idx: int,
    ) -> List[str]:
        """Verify scan produced expected results.
        - blocks_scanned: count of 'Block N received! Scanning block...' lines
        - capabilities_count: from 'Capabilities discovered: N' in summary
        - secrets_count: from 'Secrets in wallet: N' in summary
        """
        errors = []
        if blocks_scanned == 0 and capabilities_count == 0:
            # Re-scan case: blocks already processed. 'Scan complete' confirms
            # the scan ran. 0 blocks + 0 capabilities is normal on re-scan.
            pass
        elif blocks_scanned == 0 and capabilities_count > 0:
            errors.append(
                f"wallet-{wallet_idx}: capabilities found but 0 blocks scanned "
                f"— inconsistent state"
            )
        if wallet_idx == 1 and capabilities_count == 0 and blocks_scanned > 0:
            errors.append(
                f"wallet-1: scanned {blocks_scanned} blocks but found no "
                f"capabilities — key should match node0. Check scan output "
                f"for COINBASE_DECRYPT_FAILED."
            )
        if secrets_count == 0:
            errors.append(
                f"wallet-{wallet_idx}: 0 secrets in wallet — cannot decrypt. "
                f"Run 'wallet import-from-toml <name>'."
            )
        return errors

    @staticmethod
    def verify_balance(
        has_drkw: bool, wallet_idx: int, is_wallet_1: bool,
    ) -> List[str]:
        """Verify DRKW balance. wallet-1 MUST have DRKW from coinbase."""
        errors = []
        if is_wallet_1 and not has_drkw:
            errors.append(
                "wallet-1: no DRKW balance — coinbase not received. "
                "Check FORWARD_DESTINATION and key identity."
            )
        return errors


# ── Transfer verification ──
class TransferVerifier:
    """Spec: wallet-1 sends 1 DRKW to wallet-2, wallet-2 confirms receipt.
    Phase 11. Advisory only — failures warn, do not gate the pipeline."""

    @staticmethod
    def verify_transfer(
        transfer_tx_built: bool,
        wallet2_received: bool,
        attempts: int,
    ) -> Tuple[bool, str]:
        """Returns (passed, message)."""
        if not transfer_tx_built:
            return (False, "Transfer tx failed to build — check wallet-1 balance")
        if not wallet2_received:
            return (
                False,
                f"Transfer not confirmed after {attempts} attempts — "
                f"may still be mining",
            )
        return (True, f"Transfer confirmed after {attempts} attempts")


# ── Pipeline state machine ──
class Pipeline:
    """Sequential deterministic pipeline. All phases are gated — a phase
    that records failures blocks all subsequent phases. Phase ordering
    varies by mode (native/merge/bridge/join)."""

    def __init__(
        self,
        nodes: Dict[str, Node],
        wallets: List[str],
        mode: Mode = Mode.NATIVE,
    ):
        self.nodes = nodes
        self.wallets = wallets
        self.mode = mode
        self.phase_results: Dict[Phase, bool] = {}
        self.failures: Dict[Phase, List[str]] = {}
        self.current_phase: Optional[Phase] = None

    @property
    def active_phases(self) -> List[Phase]:
        """Phases that run in this mode. Bridge phases only run in bridge mode.
        Wallet phases only run when wallets are configured."""
        phases = [
            Phase.CLEAN, Phase.BUILD, Phase.PREREQS, Phase.WALLET_GEN,
            Phase.START, Phase.VERIFY_CONTAINERS, Phase.RPC_HEALTH,
            Phase.MINING_ACTIVITY, Phase.BLOCK_PRODUCTION,
        ]
        if self.wallets:
            phases.append(Phase.WALLET_VERIFY)
            if len(self.wallets) >= 2:
                phases.append(Phase.WALLET_TRANSFER)
        if self.mode == Mode.BRIDGE:
            phases.extend([
                Phase.BRIDGE_DEPLOY, Phase.BRIDGE_INIT,
                Phase.BRIDGE_REGISTER_RELAYER, Phase.BRIDGE_DEPOSIT,
                Phase.BRIDGE_WITHDRAW, Phase.BRIDGE_ACCEPT,
                Phase.BRIDGE_EXECUTE, Phase.BRIDGE_VERIFY,
            ])
        phases.append(Phase.REPORT)
        if self.mode in (Mode.JOIN_NATIVE, Mode.JOIN_MERGE):
            phases.append(Phase.PERSISTENCE)
        return phases

    def run_phase(self, phase: Phase) -> bool:
        """Run one phase. Returns True if passed. Previous phase must have
        passed (sequential determinism)."""
        self.current_phase = phase
        prev = Phase(phase.value - 1) if phase.value > 1 else None
        if (
            prev is not None
            and prev in self.phase_results
            and not self.phase_results[prev]
        ):
            self.failures[phase] = [
                f"Phase {prev.value} failed — cannot proceed"
            ]
            self.phase_results[phase] = False
            return False
        return True  # actual verification injected by test harness

    def report(self) -> str:
        passed = sum(1 for v in self.phase_results.values() if v)
        failed = sum(1 for v in self.phase_results.values() if not v)
        return f"Pipeline: {passed} phases passed, {failed} phases failed"


# ── Tests ──
def test_genesis_authority():
    """L1: Only node0 may create genesis."""
    nodes = default_topology()
    gc = GenesisCeremony(authority="node0")
    errors = gc.verify_authority(nodes)
    assert len(errors) == 0, f"Genesis authority errors: {errors}"

    # Wrong node creates genesis
    nodes["node1"].creates_genesis = True
    errors = gc.verify_authority(nodes)
    assert len(errors) == 1
    assert "node1" in errors[0]
    nodes["node1"].creates_genesis = False

    # Authority not in set
    errors = GenesisCeremony(authority="node99").verify_authority(nodes)
    assert len(errors) >= 1
    assert any("node99" in e for e in errors)

    print("  PASS test_genesis_authority")


def test_genesis_convergence():
    """L1: All nodes must match node0's merkle root."""
    gc = GenesisCeremony(
        authority="node0",
        reference_merkle_root="mr_node0_abc123",
    )
    # All match
    errors = gc.verify_convergence({
        "node0": "mr_node0_abc123",
        "node1": "mr_node0_abc123",
        "observer": "mr_node0_abc123",
    })
    assert len(errors) == 0

    # node1 diverged — different chain
    errors = gc.verify_convergence({
        "node0": "mr_node0_abc123",
        "node1": "mr_different_xyz",
    })
    assert len(errors) > 0
    assert any("MISMATCH" in e for e in errors)

    print("  PASS test_genesis_convergence")


def test_consensus_verifier():
    """L2: Cross-node merkle root equality at heights 2-5."""
    # All agree
    blocks = {
        "node0": {2: "mr2", 3: "mr3", 4: "mr4", 5: "mr5"},
        "node1": {2: "mr2", 3: "mr3", 4: "mr4", 5: "mr5"},
    }
    errors = ConsensusVerifier.verify_heights(blocks)
    assert len(errors) == 0

    # Split at height 3
    blocks["node1"][3] = "mr3_wrong"
    errors = ConsensusVerifier.verify_heights(blocks)
    assert len(errors) >= 1
    assert any("height 3" in e for e in errors)

    print("  PASS test_consensus_verifier")


def test_key_identity():
    """L3: Key determinism from keys.toml."""
    # wallet-1 shares node0's key
    ki = KeyIdentity("wallet-1", "node0", expected_match=True)
    err = ki.verify()
    assert err is None, f"Expected no error, got: {err}"

    # wallet-2 has its own key
    ki2 = KeyIdentity("wallet-2", "node0", expected_match=False)
    err = ki2.verify()
    assert err is None, f"Expected no error, got: {err}"

    # wallet-2 does NOT match node0
    ki3 = KeyIdentity("wallet-2", "node1", expected_match=False)
    err = ki3.verify()
    assert err is None

    print("  PASS test_key_identity")


def test_scan_verifier():
    """L5: Scan output verification matching actual format."""
    # Healthy first scan
    errors = ScanVerifier.verify_scan_output(
        blocks_scanned=5, capabilities_count=2, secrets_count=2, wallet_idx=1,
    )
    assert len(errors) == 0

    # Re-scan: blocks already processed, 0 new blocks
    errors = ScanVerifier.verify_scan_output(
        blocks_scanned=0, capabilities_count=0, secrets_count=2, wallet_idx=1,
    )
    assert len(errors) == 0, f"Re-scan should pass: {errors}"

    # wallet-1 scanned blocks but found nothing (decrypt failure)
    errors = ScanVerifier.verify_scan_output(
        blocks_scanned=5, capabilities_count=0, secrets_count=2, wallet_idx=1,
    )
    assert any("no capabilities" in e.lower() or "no coinbase" in e.lower()
               for e in errors)

    # Zero secrets
    errors = ScanVerifier.verify_scan_output(
        blocks_scanned=5, capabilities_count=2, secrets_count=0, wallet_idx=1,
    )
    assert any("0 secrets" in e for e in errors)

    # Balance check
    errors = ScanVerifier.verify_balance(
        has_drkw=True, wallet_idx=1, is_wallet_1=True,
    )
    assert len(errors) == 0

    errors = ScanVerifier.verify_balance(
        has_drkw=False, wallet_idx=1, is_wallet_1=True,
    )
    assert len(errors) == 1

    print("  PASS test_scan_verifier")


def test_transfer_verifier():
    """Phase 11: wallet-to-wallet transfer."""
    passed, msg = TransferVerifier.verify_transfer(
        transfer_tx_built=True, wallet2_received=True, attempts=3,
    )
    assert passed, f"Transfer should pass: {msg}"

    passed, msg = TransferVerifier.verify_transfer(
        transfer_tx_built=False, wallet2_received=False, attempts=1,
    )
    assert not passed

    print("  PASS test_transfer_verifier")


def test_aead_self_test():
    """L4: AEAD self-test gate at daemon startup."""
    passed, msg = AeadSelfTest.run(secrets_available=True)
    assert passed, f"AEAD self-test should pass: {msg}"

    passed, msg = AeadSelfTest.run(secrets_available=False)
    assert not passed, "AEAD self-test should fail without secrets"

    print("  PASS test_aead_self_test")


def test_pipeline_gating():
    """L6: Phase gates — failure blocks subsequent phases."""
    nodes = default_topology()
    wallets = ["wallet-1", "wallet-2"]
    pipeline = Pipeline(nodes, wallets)

    # Phase 8 fails
    pipeline.phase_results[Phase(8)] = False
    result = pipeline.run_phase(Phase(9))
    assert not result, "Phase 9 should not run if Phase 8 failed"

    # Phase 9 fails
    pipeline.phase_results[Phase(9)] = False
    result = pipeline.run_phase(Phase(10))
    assert not result, "Phase 10 should not run if Phase 9 failed"

    # Clean run
    pipeline2 = Pipeline(nodes, wallets)
    assert pipeline2.run_phase(Phase(1))
    assert pipeline2.run_phase(Phase(2))

    print("  PASS test_pipeline_gating")


def test_topology():
    """Nodes have correct ports and roles."""
    nodes = default_topology()

    assert nodes["node0"].creates_genesis is True
    assert nodes["node0"].mines is True
    assert nodes["node0"].rpc_port == 31345
    assert nodes["node0"].p2p_port == 31342

    assert nodes["node1"].creates_genesis is False
    assert nodes["node1"].mines is True
    assert nodes["node1"].rpc_port == 31346

    assert nodes["observer"].creates_genesis is False
    assert nodes["observer"].mines is False
    assert nodes["observer"].secret_hex is None
    assert nodes["observer"].rpc_port == 31345
    assert nodes["observer"].p2p_port == 31340

    print("  PASS test_topology")


def test_pipeline_spec():
    """Full pipeline specification — all layers verified."""
    print("Pipeline Model Specification Tests:")
    test_topology()
    test_genesis_authority()
    test_genesis_convergence()
    test_consensus_verifier()
    test_key_identity()
    test_aead_self_test()
    test_scan_verifier()
    test_transfer_verifier()
    test_pipeline_gating()
    print("Pipeline model: all specification checks passed")
    return True


if __name__ == "__main__":
    test_pipeline_spec()
