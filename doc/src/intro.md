# DarkWow

> **This document describes the active `linear-master` branch**, which uses
> Uncle Merkle consensus with RandomX Proof-of-Work. The legacy overlay-DAG
> architecture was fully replaced — `src/validator/` has been removed. See
> [What's Different from Upstream](about/differences_from_upstream.md) for
> the fork rationale and architectural divergence.
>
> **Genesis contracts are sound and proven.** Post-genesis contracts are
> experimental and unaudited. Use at your own risk.

DarkWow is a privacy-first blockchain with six architectural commitments:
composable O-Cap governance primitives, Uncle Merkle consensus with
stateless verification, sovereign keys with a wallet that is a pure
mathematical function, Lean4-verified ZKVM opcodes, zero premine, and
per-block Pedersen mass balance for supply audit. It ships 32 deployable
smart contracts, nine deployed at genesis, all built on the same o-cap +
ZK substrate with a five-level deterministic test pipeline.

Built on a proven chassis — Satoshi's supply model (21M DRKW hard cap),
Monero's RandomX mining, continuous exponential decay — plus a novel zkVM
that proves WASM contract execution. Every smart contract runs inside ZK
proofs by default.

## Start Here

- **Building contracts?** See the [Contract Developer guide](for-contract-developers.md)
  — dao_escrow case study, O-Cap primitives, testing pipeline.
- **Running a node or mining?** See the [Node Operator guide](for-node-operators.md)
  — Uncle Merkle consensus, merge mining, monetary policy.
- **Researching the cryptography?** See the [Researcher guide](for-researchers.md)
  — ZK circuits, ρ-calculus type system, Lean4 formal verification.
- **One page to read:** The [Formal Specification](arch/formal-specification.md)
  covers the entire system.

## What Ships vs. What's Planned

### [SHIPPING] — Code exists, tests pass

| Component | Status | Location |
|-----------|--------|----------|
| Uncle Merkle consensus (RandomX PoW) | [IMPLEMENTED] | `src/linear/` |
| Exponential reward schedule | [IMPLEMENTED] | `src/sdk/src/blockchain.rs` |
| Pedersen uncle coinbase split | [IMPLEMENTED] | `src/linear/src/chain_state.rs` |
| Caribina (Arweave) anchoring finality | [IMPLEMENTED] | `src/linear/src/caribina/` |
| Monero merge-mining + anchoring | [IMPLEMENTED] | `src/linear/src/monero/` |
| nullifier_root block header verification | [IMPLEMENTED] | `src/linear/src/chain_state.rs` |
| Supply audit (Pedersen mass balance) | [IMPLEMENTED] | `src/linear/src/proof_of_token_balance.rs` |
| 9 genesis contracts | [IMPLEMENTED] | `src/contract/<name>/` |
| 32 deployable contracts (code + manifests) | [IMPLEMENTED] | `src/contract/<name>/` |
| 32 zkVM opcodes | [IMPLEMENTED] | `src/zkas/opcode.rs` |
| dwowd daemon (mining, P2P, RPC) | [IMPLEMENTED] | `bin/dwowd/` |
| dwow_wallet — DRKW scan + transfer | [IMPLEMENTED] | `bin/dww/` |
| dwow_wallet — manifest discovery (read) | [IMPLEMENTED] | `bin/dww/src/scan.rs` |
| wallet_construct (Rust + Lean4) | [IMPLEMENTED] | `src/sdk/src/capability.rs` |
| AccountManager (declared identity) | [IMPLEMENTED] | `crates/dwow-accounts/` |
| 5-level test pipeline | [IMPLEMENTED] | `contrib/docker/` |
| Mempool with on-chain nullifier check | [IMPLEMENTED] | `crates/dwow-mempool/` |
| Universal relayer | [IMPLEMENTED] | `bin/universal_relayer/` |
| Manifest-driven generic prover (ZK) | [IMPLEMENTED] | `src/sdk/src/prover.rs`, `bin/dww/src/prover_impl.rs` — zkas_binaries store, witness-binding, manifest-driven proof construction for any contract |
| Manifest parameter encoding (non-ZK) | [IMPLEMENTED] | `src/sdk/src/manifest.rs` — `encode_params_by_schema`, write-path dual of `decode_note_by_schema` |
| Capability selection by asset_id (write path) | [IMPLEMENTED] | `bin/dww/src/lib.rs` — `resolve_transfer_contract`, dispatch + RPC routing through manifest-driven `invoke_contract` |

### [PARTIAL] — Core works, limitations listed

| Component | Status | Limitation |
|-----------|--------|-----------|
| P2P three-tier feature gate | [PARTIAL] | net-wallet (dww) and net-full (dwowd) active in production; net-node middle tier defined in Cargo.toml but unused by any binary, not compile-tested, structured-gossip behavior not implemented |

### [VISION] — Long-term design direction

| Component | Status | Notes |
|-----------|--------|-------|
| ρ-calculus wallet composition (full) | [VISION] | Provisional state, Seed discipline, barb-cover write-path selection pending |
| Sharding via uncle merkle topology | [VISION] | Design exploration; see [scaling.md](arch/consensus/scaling.md) |
| Parallel contract execution | [VISION] | Gated on wasmer thread safety |
| Quantum-resistant migration | [VISION] | Research prototype at `script/research/pqxdh/` |

## Canonical References

- [Genesis Configuration](arch/genesis.md) — 9 genesis contracts, ContractId derivation, bootstrap sequence
- [Smart Contracts](contracts.md) — full catalog (32 contracts), maturity status, per-contract docs
- [Consensus & Coinbase](arch/consensus-coinbase.md) — exponential supply model, reward schedule, emission curve
- [Wallet Architecture](arch/wallet.md) — manifest-first capability engine, Box/Purse/MultiSig primitives
- [Opcodes & Formal Verification](arch/zk/opcodes.md) — all 32 opcodes, Lean4-verified additions
- [Security Analysis](arch/security-analysis.md) — known issues, ZK circuit troubleshooting

DarkWow began as a fork of DarkFi. See [Architecture Divergence](about/differences_from_upstream.md)
for the comparison and [Philosophy](philosophy/philosophy.md) for the design rationale.

## Community

Weekly dev chat: Monday 14:00 UTC (DST) / 15:00 UTC (ST) in #dev.
See [DarkIRC Guide](misc/darkirc/darkirc.md) for joining the anonymous p2p chat.

## Building

```bash
git clone https://codeberg.org/PatrickM123/darkwow
cd darkwow
make
```

[Docker-based development →](dev/quickstart.md)
