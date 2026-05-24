# What's Different from Upstream DarkFi

This project is a fork of [DarkFi](https://codeberg.org/darkrenaissance/darkfi). It inherits the core zkVM, ZKAS circuit language, P2P networking stack, and WASM contract runtime.

## Comparison Table

| Feature | Upstream (DarkFi) | DarkWow (this fork) |
|---------|-------------------|---------------------|
| Native token control | DAO-governed | No governance surface |
| Privacy model | ACL (reveals identity) | ZK predicates (boolean only) |
| Token distribution | Contributor allocations | Pure PoW mining |
| Consensus | Overlay-DAG | Uncle Merkle |
| Opcodes | Basic set | + LessThanOrEqual, IsNotEqual, BaseDiv (Lean-verified) |
| Supply model | Premine + emission | Zero premine, continuous exponential decay |
| State model | Speculative (overlay/diff) | Deterministic (final at commit) |

## What's Inherited (From Upstream)

| Component | Description |
|-----------|-------------|
| **zkVM** | ZK virtual machine for proof generation and verification |
| **ZKAS** | Circuit language and compiler |
| **P2P stack** | Peer discovery, session management, protocol negotiation |
| **WASM runtime** | In-node WASM execution for smart contracts |
| **Halo2** | Proof system backend (Poseidon/Pallas) |

## Design Changes (This Fork)

### 1. Native Token — No Governance Coupling

Upstream's architecture ties the native token to DAO governance. Token holders can vote on operations including native token minting — the same token that pays block rewards and fees.

This fork decouples them. [NativeToken](../dev/contracts/native_token.md) has no governance surface — no DAO can freeze, restrict, or modify its operation. Block rewards and fee payment are consensus-critical functions; keeping them outside governance scope means miners and validators can't be voted out of their income. The governance use case is served separately by [DAO Escrow](../contract/dao_escrow.md), which uses ZK predicates rather than token-weighted ACLs.

### 2. Privacy — ZK Predicates Instead of ACLs

Upstream uses ACL-based voting where participants reveal their public key and token balance to prove eligibility. This exposes voter identity and wealth.

This fork uses ZK predicates: a voter proves they meet a condition (e.g., "holds >= 1000 tokens") without revealing their public key, exact balance, or any other identifying information. The verifier learns only the boolean result. This trades implementation simplicity for stronger privacy guarantees.

### 3. Token Distribution — Pure PoW, No Premine

Upstream's launch included token distributions to early contributors, investors, and SAFT participants.

This fork has zero premine. Every token in circulation was mined. This is the Bitcoin model: the only way to acquire the native token is to contribute proof-of-work. It trades early funding certainty for distribution fairness.

### 4. Consensus — Uncle Merkle Instead of Overlay/DAG

Upstream uses an overlay-diff architecture: a DAG of events where blocks are verified speculatively against an in-memory overlay that can be committed or rolled back.

This fork uses Uncle Merkle consensus: the canonical chain (most accumulated work) is obligated to offer competing uncle chains a one-time pin reward — a share of the block reward rather than zero. No overlay, no speculative state, no rollback — every state change is final. Both approaches prevent wasted miner work; this fork trades fork-arbitration flexibility for deterministic, testable state.

### 5. ZK Opcodes — Built and Formally Verified

Upstream's zkVM has no `LessThanOrEqual`, `IsNotEqual`, or `BaseDiv` opcodes. These were built on this fork:

- **LessThanOrEqual** (0x55) — enables conditional logic and O-Cap predicate evaluation in circuits
- **IsNotEqual** (0x62) — the first fully constrained pure Boolean operator in the zkVM (all witness values fully constrained in all cases)
- **BaseDiv** (0x58) — enables precise field division for cold-circuit governance operations (stablecoin interest accrual, governance ratio checks)

All three have been formally proven sound using the Lean4 proof assistant, with machine-checkable proofs of correctness in `proofs/lean/`.

See [Opcodes and Formal Verification](../arch/zk/opcodes.md) for the full verification analysis.

## See Also

- [Consensus Details](../arch/consensus/consensus.md)
- [Opcodes and Formal Verification](../arch/zk/opcodes.md)
- [Privacy Architecture (O-Cap)](../arch/ocap.md)
- [DAO Escrow Contract](../contract/dao_escrow.md)
