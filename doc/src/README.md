# DarkWow

A DarkWow fork rebuilt around four rejections of upstream: no governance DAO (pure PoW, no token-holder voting), no overlay/diff consensus (deterministic Uncle Merkle with stateless verification), LessThanOrEqual and BaseDiv opcodes built and proven sound in Lean4 on this fork (additions to upstream's zkVM — not inherited), and no premine (every coin mined). Zero vendor lock-in. Hard forks are a feature, not a threat.

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
