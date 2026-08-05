# Consensus Documentation

## Reading Order

1. [**consensus.md**](consensus.md) — Core consensus protocol: block structure, acceptance rules, fork choice
2. [**chain_architecture.md**](chain_architecture.md) — Chain state layout, sled trees, overlay semantics
3. [**uncle_merkle.md**](uncle_merkle.md) — Uncle Merkle inclusion proofs, reward schedule, depth penalties
4. [**linear_blockchain.md**](linear_blockchain.md) — Linear block format, coinbase structure, cumulative supply

## Specialized Topics

| Document | Topic |
|----------|-------|
| [consensus-coinbase.md](../consensus-coinbase.md) | Coinbase construction, reward formulas, fee model, wallet integration |
| [stratum.md](stratum.md) | Stratum mining protocol |
| [merge-mining-ffi.md](merge-mining-ffi.md) | Monero merge-mining FFI interface |
| [hazid-report.md](hazid-report.md) | HAZID analysis of consensus edge cases |

## Design Exploration

| Document | Status |
|----------|--------|
| [scaling.md](scaling.md) | [VISION] Sharding design — not implemented |
| [linear_zkvm.md](linear_zkvm.md) | [VISION] ZKVM integration — types not yet implemented |
