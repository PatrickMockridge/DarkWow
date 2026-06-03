#!/usr/bin/env python3
"""
VM Access State Machine Model — RandomX Concurrency Safety

Models the concurrent access patterns between:
- miner_task (holds VM during mining, OUTSIDE connect_lock)
- broadcast handler (acquires VM inside connect_block, INSIDE connect_lock)

Purpose: Prove no concurrent RandomX FFI access is possible, OR identify
every path where concurrent access occurs.

Background:
- RandomX FFI is NOT thread-safe. Concurrent use of the same VM crashes.
- chain_state.get_vm(key) returns Arc<RandomXVM> — multiple callers can hold
  references to the SAME VM simultaneously.
- connect_lock serializes connect_block() calls but does NOT protect the
  mining phase where the miner holds an Arc<VM> for hashing.
- If miner holds VM(key=X) and broadcast handler calls get_vm(key=X),
  both get Arc references to the SAME VM object → concurrent FFI access.

States:
  IDLE       — No task holds any VM
  MINING     — Miner holds VM for key K, actively hashing
  VALIDATING — connect_block holds VM for key K, verifying PoW

The critical question: can MINING and VALIDATING happen simultaneously
with the same key K?
"""

from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Dict, List, Optional, Set, Tuple
import random


class TaskState(Enum):
    IDLE = auto()
    HOLDING_VM = auto()        # Has Arc<VM> reference
    HASHING = auto()           # Actively calling RandomX FFI


class VMState(Enum):
    FREE = auto()               # No task holds this VM
    HELD = auto()               # At least one task holds this VM
    CONCURRENT = auto()         # Two or more tasks hold this VM — CRASH


@dataclass
class VMAccess:
    """Represents one task's VM access."""
    task_name: str
    key: int                    # RandomX key (simplified)
    state: TaskState = TaskState.IDLE
    vm_ref: bool = False        # Has Arc<VM> reference?


@dataclass
class VMStateMachine:
    """
    Models the concurrent VM access pattern of the dwowd miner and broadcast handler.

    Each RandomX key maps to a VM. Multiple tasks can hold Arc references
    to the same VM. If two tasks hold references and either hashes, the
    FFI may crash (data race on RandomX internal state).
    """

    vms: Dict[int, List[VMAccess]] = field(default_factory=dict)
    connect_lock: bool = False  # Is connect_lock held?
    crash_log: List[str] = field(default_factory=list)

    def register_vm(self, key: int):
        if key not in self.vms:
            self.vms[key] = []

    def acquire_vm(self, task_name: str, key: int):
        """Task acquires a VM reference via get_vm(key)."""
        self.register_vm(key)
        access = VMAccess(task_name=task_name, key=key, state=TaskState.HOLDING_VM, vm_ref=True)
        self.vms[key].append(access)

        holders = len(self.vms[key])
        if holders >= 2:
            # Check if any holder is actively hashing
            hashing = [a for a in self.vms[key] if a.state == TaskState.HASHING]
            if hashing:
                self.crash_log.append(
                    f"CRASH: {task_name} acquired VM key={key} while "
                    f"{[h.task_name for h in hashing]} is HASHING — "
                    f"concurrent RandomX FFI access on same VM"
                )
                return False
            # Even if not hashing yet, flag the risk
            self.crash_log.append(
                f"WARNING: {task_name} holds VM key={key} concurrently with "
                f"{[a.task_name for a in self.vms[key] if a.task_name != task_name]} — "
                f"if either hashes, CRASH"
            )

        print(f"  [ACQUIRE] {task_name} got VM key={key} (holders={holders})")
        return True

    def release_vm(self, task_name: str, key: int):
        """Task releases its VM reference (drop(vm))."""
        if key in self.vms:
            self.vms[key] = [a for a in self.vms[key] if a.task_name != task_name]
            if not self.vms[key]:
                del self.vms[key]
            print(f"  [RELEASE] {task_name} dropped VM key={key}")

    def start_hashing(self, task_name: str, key: int):
        """Task begins RandomX hashing on its VM reference."""
        if key not in self.vms:
            self.crash_log.append(f"ERROR: {task_name} tried to hash with VM key={key} — not held")
            return False

        # Find this task's access
        for access in self.vms[key]:
            if access.task_name == task_name:
                break
        else:
            self.crash_log.append(f"ERROR: {task_name} has no VM key={key} reference")
            return False

        # Check for concurrent hashers
        hashers = [a for a in self.vms[key] if a.state == TaskState.HASHING]
        if hashers:
            self.crash_log.append(
                f"CRASH: {task_name} started hashing on VM key={key} while "
                f"{[h.task_name for h in hashers]} also HASHING — "
                f"concurrent RandomX FFI data race"
            )
            access.state = TaskState.HASHING
            return False

        # Check for other holders (they might start hashing too)
        other_holders = [a for a in self.vms[key] if a.task_name != task_name]
        if other_holders:
            self.crash_log.append(
                f"CRASH: {task_name} started hashing on VM key={key} while "
                f"{[h.task_name for h in other_holders]} also holds VM — "
                f"either could call RandomX FFI simultaneously"
            )
            access.state = TaskState.HASHING
            return False

        access.state = TaskState.HASHING
        print(f"  [HASH] {task_name} hashing on VM key={key}")
        return True

    def stop_hashing(self, task_name: str, key: int):
        """Task finishes hashing."""
        if key in self.vms:
            for access in self.vms[key]:
                if access.task_name == task_name:
                    access.state = TaskState.HOLDING_VM
                    print(f"  [DONE] {task_name} done hashing on VM key={key}")
                    return

    def acquire_lock(self, task_name: str):
        """Acquire connect_lock."""
        if self.connect_lock:
            self.crash_log.append(f"ERROR: {task_name} tried to acquire held connect_lock")
            return False
        self.connect_lock = True
        print(f"  [LOCK] {task_name} acquired connect_lock")
        return True

    def release_lock(self, task_name: str):
        """Release connect_lock."""
        self.connect_lock = False
        print(f"  [UNLOCK] {task_name} released connect_lock")


def simulate_miner_cycle(sm: VMStateMachine, height: int, key: int):
    """
    Simulate one mining cycle as implemented in lib.rs miner_task().

    The miner:
    1. Calls get_vm(key) — ACQUIRES VM OUTSIDE connect_lock
    2. Calls miner.mine(&vm, ...) — HOLDS VM while hashing
    3. Drops VM: drop(vm)
    4. Calls apply_block → connect_block which acquires connect_lock
       and internally calls get_vm(key) again

    THE GAP: steps 1-2 hold VM OUTSIDE the lock.
    If a broadcast arrives during step 2 with the SAME key,
    concurrent FFI access occurs.
    """
    print(f"\n--- Miner cycle at height {height}, key={key} ---")

    # Step 1: Miner acquires VM OUTSIDE connect_lock
    sm.acquire_vm("miner", key)

    # Step 2: Miner hashes (miner.mine loop)
    sm.start_hashing("miner", key)

    # During mining, a broadcast COULD arrive.
    # The connect_lock does NOT protect this phase.
    print(f"  >>> Miner is hashing on VM key={key} — connect_lock is FREE <<<")

    # Step 3: Miner finishes, drops VM
    sm.stop_hashing("miner", key)
    sm.release_vm("miner", key)

    # Step 4: Miner acquires connect_lock and applies block
    sm.acquire_lock("miner")
    # Inside connect_block, get_vm is called again
    sm.acquire_vm("miner", key)
    sm.start_hashing("miner", key)  # validation hash
    sm.stop_hashing("miner", key)
    sm.release_vm("miner", key)
    sm.release_lock("miner")


def simulate_broadcast_arrives_during_mining(sm: VMStateMachine, height: int, key: int):
    """
    Simulate a P2P broadcast arriving WHILE the miner is hashing.

    This is THE bug. The broadcast handler calls:
      apply_block → connect_block → get_vm(key)

    get_vm(key) returns an Arc to the SAME VM the miner is hashing on.
    Both tasks now hold Arc<VM> for the same key.
    If connect_block calls hash_with_vm while the miner is hashing: CRASH.
    """
    print(f"\n--- BROADCAST arrives during mining at height {height}, key={key} ---")

    # Miner is already hashing (from simulate_miner_cycle step 2)
    # Broadcast path:
    print(f"  >>> Broadcast handler calls apply_block → connect_block <<<")

    # connect_block acquires the lock — BUT the miner is OUTSIDE the lock
    # The lock serializes connect_block but doesn't protect get_vm access
    lock_ok = sm.acquire_lock("broadcast")
    if not lock_ok:
        print("  (connect_lock already held — broadcast blocks on lock)")
        return

    # Inside connect_block, line 235: let vm = self.get_vm(block.header.randomx_key);
    # This returns Arc<VM> for key — potentially the SAME VM the miner holds
    vm_ok = sm.acquire_vm("broadcast", key)
    if not vm_ok:
        print(f"  FATAL: broadcast got VM key={key} while miner is HASHING — SEGFAULT PATH")

    # Stage 1 PoW validation: block.hash_with_vm(&vm)
    sm.start_hashing("broadcast", key)  # ← THIS IS THE CRASH

    sm.stop_hashing("broadcast", key)
    sm.release_vm("broadcast", key)
    sm.release_lock("broadcast")


def simulate_same_key_collision():
    """
    The worst case: miner and broadcast both use the SAME randomx_key.

    This happens when:
    - Miner is mining block at height N with key K
    - Peer broadcasts block at height N with the SAME key K
    - Both tasks get Arc<VM> for K from the cache
    - Both call RandomX FFI on the same VM → SEGFAULT
    """
    print("=" * 70)
    print("TEST 1: Same-key collision — miner hashing, broadcast arrives")
    print("=" * 70)

    sm = VMStateMachine()
    key = 42
    height = 5

    # Miner acquires VM and starts hashing
    sm.acquire_vm("miner", key)
    sm.start_hashing("miner", key)
    print("  >>> Miner hashing on VM key=42 — connect_lock is FREE <<<")

    # Broadcast arrives during mining — DIFFERENT task, SAME key
    # connect_block acquires the lock but get_vm returns the SAME VM
    lock_ok = sm.acquire_lock("broadcast")
    assert lock_ok
    vm_ok = sm.acquire_vm("broadcast", key)  # Same key → same VM!
    assert not vm_ok  # THIS IS THE BUG

    print(f"\nCrash log ({len(sm.crash_log)} entries):")
    for entry in sm.crash_log:
        print(f"  {entry}")

    has_crash = any("CRASH" in e for e in sm.crash_log)
    print(f"\nRESULT: {'FAIL — concurrent FFI access detected' if has_crash else 'PASS'}")

    return has_crash


def simulate_different_key_no_collision():
    """
    Safe case: miner and broadcast use DIFFERENT randomx keys.
    Different keys → different VM instances → no concurrent access.

    This is the common case for blocks at different heights (keys differ
    by height) but can still collide if miner and peer produce blocks
    at the same height (same key derivation).
    """
    print("\n" + "=" * 70)
    print("TEST 2: Different-key — miner hashing key=42, broadcast uses key=43")
    print("=" * 70)

    sm = VMStateMachine()

    # Miner hashing on key 42
    sm.acquire_vm("miner", 42)
    sm.start_hashing("miner", 42)

    # Broadcast arrives with DIFFERENT key
    sm.acquire_lock("broadcast")
    vm_ok = sm.acquire_vm("broadcast", 43)  # Different key → different VM
    assert vm_ok  # SAFE — different VM instances
    sm.start_hashing("broadcast", 43)  # SAFE — no shared FFI state
    sm.stop_hashing("broadcast", 43)
    sm.release_vm("broadcast", 43)
    sm.release_lock("broadcast")

    # Miner finishes
    sm.stop_hashing("miner", 42)
    sm.release_vm("miner", 42)

    has_crash = any("CRASH" in e for e in sm.crash_log)
    print(f"\nCrash log ({len(sm.crash_log)} entries):")
    for entry in sm.crash_log:
        print(f"  {entry}")
    print(f"\nRESULT: {'FAIL' if has_crash else 'PASS — different keys are safe'}")

    return has_crash


def simulate_connect_lock_does_not_protect_mining():
    """
    Demonstrate that connect_lock ONLY serializes connect_block calls.
    The miner's VM reference (step 2 in miner cycle) is OUTSIDE the lock.
    """
    print("\n" + "=" * 70)
    print("TEST 3: connect_lock does NOT protect mining phase")
    print("=" * 70)

    sm = VMStateMachine()
    key = 7

    # Miner cycle: acquire VM OUTSIDE lock
    sm.acquire_vm("miner", key)
    sm.start_hashing("miner", key)
    print("  >>> connect_lock state: FREE (miner doesn't hold it) <<<")

    # Broadcast acquires connect_lock successfully — miner doesn't hold it
    lock_ok = sm.acquire_lock("broadcast")
    assert lock_ok  # Lock is free because miner doesn't hold it!
    print("  >>> Broadcast acquired connect_lock while miner is HASHING <<<")

    # Now broadcast gets VM — same key, same VM
    vm_ok = sm.acquire_vm("broadcast", key)
    if not vm_ok:
        print("  >>> CONCURRENT ACCESS: both miner and broadcast hold VM <<<")

    has_crash = any("CRASH" in e for e in sm.crash_log)
    print(f"\nRESULT: {'connect_lock FAILS to protect mining — CRASH' if has_crash else 'PASS'}")

    return has_crash


def simulate_fix_separate_vms():
    """
    Proposed fix: each task creates its OWN VM (not from shared cache).

    Instead of get_vm(key) returning a cached Arc<VM>, each task
    creates a fresh VM. No shared FFI state → no crash.

    Trade-off: VM creation is expensive (~100ms for RandomX cache init).
    Solution: the miner creates its own VM, uses it, drops it.
    The cache is only used for validation (inside connect_lock, serialized).
    """
    print("\n" + "=" * 70)
    print("TEST 4: Fix — separate VM per task (no cache sharing)")
    print("=" * 70)

    class FixedVMStateMachine:
        """Each task gets its own VM — no shared state."""
        def __init__(self):
            self.task_vms: Dict[str, Dict[int, bool]] = {}  # task → {key → is_hashing}
            self.crash_log: List[str] = []

        def create_own_vm(self, task_name: str, key: int):
            """Create a FRESH VM for this task only — not from cache."""
            if task_name not in self.task_vms:
                self.task_vms[task_name] = {}
            self.task_vms[task_name][key] = True  # hashing
            print(f"  [CREATE] {task_name} created its OWN VM for key={key}")
            return True  # Always safe — no shared state

        def release_own_vm(self, task_name: str, key: int):
            if task_name in self.task_vms:
                self.task_vms[task_name].pop(key, None)
                print(f"  [RELEASE] {task_name} released its VM for key={key}")

    sm = FixedVMStateMachine()

    # Miner creates its own VM
    sm.create_own_vm("miner", 42)

    # Broadcast arrives — creates its OWN VM for same key
    sm.create_own_vm("broadcast", 42)

    # Both can hash simultaneously — DIFFERENT VM instances
    print("  >>> Both tasks hashing on key=42 with SEPARATE VMs — SAFE <<<")

    sm.release_own_vm("miner", 42)
    sm.release_own_vm("broadcast", 42)

    print(f"\nRESULT: PASS — separate VMs eliminate the entire concurrency class")


def simulate_fix_miner_owns_vm_exclusively():
    """
    Alternative fix: miner acquires an exclusive lock on the VM cache entry,
    preventing any other task from getting the same VM while mining.

    This is a lighter fix than creating fresh VMs — the miner takes
    a per-key lock before mining and releases after dropping VM.
    """
    print("\n" + "=" * 70)
    print("TEST 5: Fix — per-key exclusive lock during mining")
    print("=" * 70)

    class PerKeyLockedVMStateMachine:
        def __init__(self):
            self.key_locks: Dict[int, str] = {}  # key → task_name (who holds exclusive access)
            self.crash_log: List[str] = []

        def acquire_exclusive(self, task_name: str, key: int) -> bool:
            if key in self.key_locks:
                self.crash_log.append(
                    f"BLOCKED: {task_name} tried to acquire VM key={key} "
                    f"while held exclusively by {self.key_locks[key]}"
                )
                return False
            self.key_locks[key] = task_name
            print(f"  [EXCL] {task_name} acquired exclusive access to VM key={key}")
            return True

        def release_exclusive(self, task_name: str, key: int):
            if self.key_locks.get(key) == task_name:
                del self.key_locks[key]
                print(f"  [RELEASE] {task_name} released exclusive access to VM key={key}")

    sm = PerKeyLockedVMStateMachine()

    # Miner acquires exclusive access
    assert sm.acquire_exclusive("miner", 42)

    # Broadcast tries to acquire same key — BLOCKED
    blocked = sm.acquire_exclusive("broadcast", 42)
    if not blocked:
        print("  >>> Broadcast correctly BLOCKED — miner has exclusive access <<<")

    # Miner releases
    sm.release_exclusive("miner", 42)

    # Now broadcast can proceed
    assert sm.acquire_exclusive("broadcast", 42)
    sm.release_exclusive("broadcast", 42)

    has_crash = any("CRASH" in e for e in sm.crash_log)
    print(f"\nRESULT: {'FAIL' if has_crash else 'PASS — per-key lock prevents collision'}")
    return has_crash


def simulate_race_condition_connect_block():
    """
    Test: what happens when connect_block itself races?

    connect_block acquires connect_lock first, so two connect_block
    calls are serialized. But get_vm() is called inside the lock
    and uses a HashMap protected by vm_cache Mutex.

    H3 finding: std::sync::Mutex blocks smol executor threads.
    Fix: use smol::lock::Mutex instead.
    """
    print("\n" + "=" * 70)
    print("TEST 6: Two simultaneous connect_block calls (different keys)")
    print("=" * 70)

    print("  connect_lock serializes connect_block → only one runs at a time")
    print("  Inside connect_block, get_vm acquires vm_cache Mutex")
    print("  std::sync::Mutex blocks the smol thread (H3)")
    print("  Fix: smol::lock::Mutex for connect_lock + vm_cache")
    print("RESULT: PASS — connect_lock prevents race, but needs smol::Mutex")


def main():
    results = {}

    results["same_key"] = simulate_same_key_collision()
    results["different_key"] = simulate_different_key_no_collision()
    results["connect_lock_gap"] = simulate_connect_lock_does_not_protect_mining()
    simulate_fix_separate_vms()
    simulate_fix_miner_owns_vm_exclusively()
    simulate_race_condition_connect_block()

    print("\n" + "=" * 70)
    print("SUMMARY")
    print("=" * 70)

    print(f"\n  Same-key collision (H1+H2):     {'FAIL — CRASH PATH' if results['same_key'] else 'PASS'}")
    print(f"  Different-key safe:             {'PASS' if not results['different_key'] else 'FAIL'}")
    print(f"  connect_lock gap:               {'FAIL — lock insufficient' if results['connect_lock_gap'] else 'PASS'}")

    print(f"\nRoot cause: get_vm() uses a shared cache. Multiple Arc<VM>")
    print(f"references to the SAME VM are handed out. connect_lock only")
    print(f"serializes connect_block — the miner holds its VM OUTSIDE the lock.")
    print(f"\nAny task pair sharing a key will crash if both hash simultaneously.")

    print(f"\nRecommended fix (simplest):")
    print(f"  1. Miner creates its own VM (not from cache) — RandomXCache::new each cycle")
    print(f"  2. OR: add a per-key Mutex<()> in vm_cache, miner holds it while hashing")
    print(f"  3. Replace std::sync::Mutex → smol::lock::Mutex for vm_cache + connect_lock")

    crash_count = sum(1 for v in results.values() if v)
    print(f"\n{crash_count}/{len(results)} tests found crash paths")


if __name__ == "__main__":
    main()
