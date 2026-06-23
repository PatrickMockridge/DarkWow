# DarkWow

> **Genesis contracts are sound and proven.** Post-genesis contracts are
> experimental and unaudited. Use at your own risk.

DarkWow is a privacy-first blockchain built on a proven chassis — Satoshi's
supply model (21M DRKW hard cap), Monero's RandomX mining, continuous
exponential decay, and Uncle Merkle consensus — plus a novel zkVM that proves
WASM contract execution. Every smart contract runs inside ZK proofs by default.

It is a fork of DarkFi that makes five architectural refutations: composable
O-Cap governance primitives instead of a monolithic DAO, ZK predicates instead
of ACLs, zero-premine PoW instead of contributor allocations, deterministic
Uncle Merkle consensus instead of overlay-DAG, and a manifest-first wallet with
zero hardcoded contract ABIs.

## Start Here

- **New to DarkWow?** Read the [Formal Specification](arch/formal-specification.md) — one page covering everything.
- **Want to build?** Start with [Developer Quick Start](dev/quickstart.md).
- **Deep dive?** The [Architecture](arch/README.md) index maps every subsystem.
- **Philosophy?** See [Philosophy](philosophy/philosophy.md) for the political-economic context.

## Canonical References

- [Genesis Configuration](arch/genesis.md) — 9 genesis contracts, ContractId derivation, bootstrap sequence
- [Smart Contracts](contracts.md) — full catalog (32 contracts), maturity status, per-contract docs
- [What's Different from Upstream](about/differences_from_upstream.md) — complete fork comparison, privacy architecture
- [Mining Tokenomics](arch/mining-tokenomics.md) — supply model, reward schedule, emission curve
- [Wallet Architecture](arch/wallet.md) — manifest-first capability engine, Box/Purse/MultiSig primitives
- [Opcodes & Formal Verification](arch/zk/opcodes.md) — all 39 opcodes, Lean4-verified additions
- [Security Analysis](arch/security-analysis.md) — known issues, ZK circuit troubleshooting

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
