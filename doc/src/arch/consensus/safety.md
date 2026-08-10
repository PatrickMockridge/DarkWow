# Fee System — Cross-Stack Coordination Safety

This document analyzes the unique safety challenges of the fee signalling
system: a universal coordination mechanism where every node, wallet, miner,
and contract MUST produce identical results from the same chain state.

The general testing taxonomy and safety patterns are defined in
`doc/src/dev/testing/overview.md` and `doc/src/arch/type-system.md`. This
document focuses on challenges specific to the fee system.

## 1. Sympatico Requirement

The fee signalling system is the **universal coordination mechanism** across
the DarkWow stack. It has no isolated components:

- The **wallet** computes fees from `fee_window_flags` in the block header
- The **mempool** admits transactions based on thresholds set by the miner
- The **miner** adjusts thresholds at window boundaries based on mempool
  queue depths, decrypts encrypted fees, and builds FeeCollectV1
- The **contract** (native_token) verifies the Pedersen accumulator matches
  the miner's claimed `total_fees` and resets it to Identity

A divergence in **any** of these components IS a consensus failure. A wallet
computing different CFs than the miner expected produces a fee below the
admission threshold → transaction rejected. A miner computing different
`total_fees` than another miner produces a different FeeCollectV1 →
different block hash → chain fork.

**No component has "local" fee parameters.** Every value that affects fee
computation must be derivable from chain state that all nodes agree on.
This is the architectural principle defined in `fee-spec.md` §13.1.

## 2. Genesis-Initiated, Window-Updated

All fee parameters start at genesis and evolve through the PID controller
every 20 blocks (`WINDOW_SIZE`). No node has private fee parameters.

This means:
- A node joining the network at height 1000 reads `fee_window_flags` from
  block 1000's header and derives the correct CFs — no "catch-up" logic needed
- A wallet that goes offline for 3 windows re-syncs to the current flags on
  its next block — the flags in the header ARE the current state
- A miner that restarts reads the `FeeWindowState` from its persisted sled
  database — the state was saved at the last window boundary

A node that cannot sync to the current window's parameters cannot participate.
There is no "offline mode" for fee computation — `FeeWindowFlags::default()`
(identity CF) is correct only at genesis.

## 3. Privacy-Preserving Dual Channel

Fees travel through two channels with different visibility:

**Public channel — `fee_window_flags` (block header).** Encodes congestion
direction (hold/+10%/-10%) for both circuit execution CF and WASM storage CF.
All nodes can read these. They are advisory signalling, not consensus-validated
(G5: `accept_block` does not reject invalid flags). They are excluded from the
block hash to prevent circular dependency (flags depend on mempool state, block
hash depends on header).

**Private channel — `encrypted_fee_value` (FeeParamsV2).** The exact fee
amount encrypted to the miner's per-block public key via ECDH + ChaCha20-Poly1305.
68-byte format: `[ephemeral_public(32)] [nonce(12)] [ciphertext+tag(24)]`.
Only the miner can decrypt. The miner's key is per-block derived
(`derive_instance(NATIVE_TOKEN, height)`) so encrypted fees cannot be
correlated across blocks by public key.

**The private channel MUST be functional.** An empty `encrypted_fee_value` is
a malformed transaction (fee-spec.md SPEC-5). Without it:
- The miner cannot learn exact fee amounts → `total_fees` is wrong →
  FeeCollectV1 Pedersen check fails → block rejected
- Fee privacy collapses: all fees are either revealed (if the fallback is
  used) or unknown (if neither the encrypted channel nor the fallback works)

**Testing the dual channel requires:**
- L1 unit: encrypt/decrypt roundtrip with real AEAD (G2 test)
- L1.5 bridge: FeeV2 with non-empty `encrypted_fee_value` through accept_block
- L3 Docker: wallet encrypts → miner decrypts → FeeCollectV1 verifies

## 4. Risk Transfer — Miners Underwrite Execution Risk

The `RiskFactor` system transfers contract execution risk from users to miners.
A contract with "self_declared" status (no attestation, no endowment) pays a
1.5× risk multiplier on its circuit component. A genesis contract pays 1.0×.
Miners are compensated for underwriting execution risk through higher fees.

This requires:
- **Per-contract CostProfile resolution.** The miner must look up each
  contract's `[[cost_profiles]]` from its manifest to determine
  `circuit_difficulty`, `k_value`, and `wasm_kb`.
- **Risk factor tracking.** The `ContractRiskTracker` records observed-vs-declared
  cost deviations per contract per window. Contracts that systematically
  under-declare circuit difficulty face escalating risk factors (1.25× → 1.5× →
  2.0× capped).
- **Dynamic escalation.** Risk factors update at window boundaries based on
  the tracker's `evaluate_window()` output.

**Current state (2026-08):** The `RiskFactor` type is specified in
`type-system.md` §2.3.1. `compute_total_fee()` (the risk-aware formula) is
implemented and tested in `fee_window.rs` but has zero production call sites.
`ContractRiskTracker` exists only in the Python reference model
(`contrib/model/fee_window_model.py`). The miner uses hardcoded
`compute_fee(&[1000], 1, ...)` for all thresholds. This is tracked as
red team findings H8, H9, H10, M2.

## 5. Loud Failures — Diagnostic Surface

Every divergence from expected behavior in the fee system SHALL produce a
diagnostic. Silent failures are the primary attack vector identified by the
2026-08 red team audit.

**Anti-pattern:** `decrypt_fee_for_miner() -> Option<u64>`
All failure modes (empty ciphertext, wrong key, corrupted data, AEAD tag
mismatch) collapse to `None` with zero diagnostic information. The caller
cannot distinguish "fee encrypted to a different miner" from "wallet hasn't
wired encryption yet" from "ciphertext corrupted in transit."

**Required pattern:** `decrypt_fee_for_miner() -> Result<u64, FeeDecryptError>`
with distinct variants:
- `EmptyCiphertext` — `encrypted_fee_value.len() < 68` (not yet wired)
- `InvalidEphemeralKey` — cannot parse ephemeral public key
- `DecryptionFailed` — AEAD tag verification failed (wrong key or corrupted)
- `InvalidFeeBytes` — decrypted bytes cannot be parsed as u64

The caller logs: `warn!("FeeV2 decrypt failed for tx {}: {:?}", tx_hash, err)`

**Other diagnostic requirements (fee-spec.md SPEC-3):**
- `FeeParamsV2::decode` failure → `warn!` with transaction hash
- Congestion measurement returning 0 due to lock contention → `warn!`
- `verify_threshold_proof()` rejection → reason logged (threshold mismatch,
  proof verification failure, malformed params)
- `FeeCollectV1` accumulator mismatch → hard error with expected vs actual

## 6. Testing Strategy

The fee system requires testing at every level of the taxonomy
(`doc/src/dev/testing/overview.md`):

| Level | What It Tests | Fee-Specific Concerns |
|-------|--------------|----------------------|
| **Python Model** | Executable specification — 69 tests in `fee_window_model.py` | Full lifecycle scenarios, feedback loop, edge cases |
| **L1 (Unit)** | Pure functions: `compute_fee()`, `CongestionFactor`, `WindowSignalling`, `encrypt_fee_for_miner()` | Deterministic integer arithmetic, no floats |
| **L1.5 (Bridge)** | Production path: real ZK proofs + AEAD + accept_block + wallet scan | Full fee lifecycle: wallet→mempool→miner→FeeCollectV1 |
| **L2 (Heavyweight)** | Multi-block chain: window boundaries, cross-window CF propagation, multi-contract fee differential | 20+ blocks to trigger window boundary, real ZK coinbases |
| **L3 (Docker)** | End-to-end: wallet container → mining nodes → block production → wallet scan | Real RandomX, real P2P, 120s block times |
| **Benchmark** | Proof timing: FeeThreshold_V1, Fee_V2, FeeCollect_V1 | Confirm proof generation < window boundary deadline |

**The Python model is the specification** (`python-model-is-the-spec`).
Every Rust implementation SHALL match a Python model scenario. Changes to
fee logic SHALL update the Python model first, then the Rust implementation.

**The L1.5 bridge is the MoC gate.** All L1.5 tests SHALL pass before any
code proceeds to the Docker pipeline. They enforce that the production code
path (real ZK proofs, real AEAD, real accept_block, real wallet scan) is
functional before introducing real networking and real PoW.

## 7. Red Team Findings (2026-08 Summary)

A combined wallet + miner red team audit identified 3 CONSENSUS-CRITICAL,
~10 HIGH, ~7 MEDIUM, and ~5 LOW anti-patterns. The root cause across all
findings: **compile-time constants substituting for chain-synced values,
silent fallbacks on consensus-critical paths, and functionality defined
but unwired with fallback defaults.**

Full findings are documented in the implementation plan. Key remediation
items:

1. Wire `encrypted_fee_value` — paired wallet + miner change (C1)
2. Remove `#[cfg(feature = "fee-window")]` feature gate (C3)
3. Unify fee estimate paths — single chain-derived value (C2)
4. Replace `try_lock().unwrap_or(0)` congestion measurement (H4)
5. Add diagnostic surface to `decrypt_fee_for_miner()` (H1, H3)
6. Wire `compute_total_fee()` and `resolve_cost_profile()` (H8, H9)
7. Implement `extract_tx_wasm_kb()` for DeployV1 (H5)
8. Port `ContractRiskTracker` from Python (M2)
