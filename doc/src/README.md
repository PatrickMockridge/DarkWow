# DarkWow

A privacy-first blockchain rebuilt around six design commitments. Originally forked from the upstream DarkFi project, DarkWow diverges across every layer of the stack. See [Differences from Upstream](about/differences_from_upstream.md) for the complete comparison.

## Six Design Commitments

1. **[Composable O-Cap Primitives](arch/ocap.md)** — Authorization via object capabilities, formalized in the ρ-calculus with Lean4-verified type soundness and the Authorization Inversion Theorem.

2. **[Uncle Merkle Consensus](arch/consensus/consensus.md)** — Linear blockchain with Uncle Merkle pin rewards, Caribina finality, and Monero merge-mining via RandomX PoW.

3. **[Sovereign Keys](arch/key-management.md)** — Users hold their own keys. The wallet is a full node. No key material ever touches an RPC endpoint.

4. **[Lean4-Verified ZK Opcodes](arch/zk/opcodes.md)** — 32 ZK opcodes with formal soundness proofs in the Lean4 proof assistant. Zero `sorry` — no admitted axioms.

5. **[Zero Premine](arch/genesis.md)** — No premine. No founder allocation. Every commitment in circulation was mined or earned.

6. **[Per-Block Pedersen Mass Balance](arch/consensus/consensus.md#supply-audit-capability)** — Every block proves Σ outputs + Σ fees == Σ inputs. Cumulative supply commitment chain is verifiable without ZK proofs.

## Quick Navigation

- [Introduction](intro.md) — What DarkWow is and why this fork exists
- [Architecture](arch/overview.md) — System design and components
- [Consensus](arch/consensus/consensus.md) — Uncle Merkle with RandomX PoW
- [O-Cap Authorization](arch/ocap.md) — The central paradigm
- [Smart Contracts](contracts.md) — Complete 32-contract catalog
- [Developer Guide](for-contract-developers.md) — Building smart contracts
- [ZK Proofs](zkas/zkas.md) — Zero-knowledge proof system
- [Testnet Guide](testnet/node.md) — Running a node

## Build

```shell
git clone https://codeberg.org/PatrickM123/darkwow
# Mirror: git clone https://github.com/PatrickMockridge/DarkWow
cd darkwow
rustup target add wasm32-unknown-unknown
make
```

## License

AGPL-3.0-only. DarkWow is a tool for people and nations to establish sovereignty according to human rights law.
