# DarkWow

A DarkFi fork rebuilt around [four core design changes](about/differences_from_upstream.md): no governance DAO, deterministic Uncle Merkle consensus, formally verified ZK opcodes, and zero premine. Hard forks are a feature, not a threat.

## Quick Navigation

- [Introduction](intro.md) — What DarkWow is and why this fork exists
- [Architecture](arch/overview.md) — System design and components
- [Consensus](arch/consensus/consensus.md) — Uncle Merkle with RandomX PoW
- [O-Cap Authorization](arch/ocap.md) — The central paradigm
- [Smart Contracts](contract/README.md) — Complete contract catalog
- [Developer Guide](dev/contrib/contrib.md) — Contributing and building
- [ZK Proofs](zkas/index.md) — Zero-knowledge proof system
- [Testnet Guide](testnet/node.md) — Running a node

## Build

```shell
git clone https://codeberg.org/PatrickM123/darkwow
cd darkwow
rustup target add wasm32-unknown-unknown
make
```

## License

AGPL-3.0-only. DarkWow is a tool for people and nations to establish sovereignty according to human rights law.
