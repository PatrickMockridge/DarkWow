# DarkWow

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

## Canonical References

- [Genesis Configuration](arch/genesis.md) — 9 genesis contracts, ContractId derivation, bootstrap sequence
- [Smart Contracts](contracts.md) — full catalog (32 contracts), maturity status, per-contract docs
- [Consensus & Coinbase](arch/consensus-coinbase.md) — supply model, reward schedule, emission curve
- [Wallet Architecture](arch/wallet.md) — manifest-first capability engine, Box/Purse/MultiSig primitives
- [Opcodes & Formal Verification](arch/zk/opcodes.md) — all 39 opcodes, Lean4-verified additions
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
