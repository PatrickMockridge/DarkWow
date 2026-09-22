# Consensus Documentation

## Reading Order

1. [**consensus.md**](consensus.md) — Core consensus protocol: block structure, acceptance rules, fork choice
2. [**chain_architecture.md**](chain_architecture.md) — Chain state layout, sled trees, overlay semantics
3. [**uncle_merkle.md**](uncle_merkle.md) — Uncle Merkle inclusion proofs, reward schedule, depth penalties
4. [**linear_blockchain.md**](linear_blockchain.md) — Linear block format, coinbase structure, cumulative supply

## Specifications

| Document | Topic |
|----------|-------|
| [consensus-coinbase.md](../consensus-coinbase.md) | Coinbase construction, reward formulas, fee model, wallet integration |
| [fee-spec.md](fee-spec.md) | Fee payment and collection — formal specification |
| [transfer-spec.md](transfer-spec.md) | Capability exercise (transfer) specification |
| [stratum.md](stratum.md) | Stratum mining protocol |
| [merge-mining-ffi.md](merge-mining-ffi.md) | Monero merge-mining FFI interface |
| [node-startup-spec.md](node-startup-spec.md) | Node startup & role behavior — WYSIWYG specification |

## Safety & Audit

| Document | Topic |
|----------|-------|
| [safety.md](safety.md) | Fee system — cross-stack coordination safety |

The consensus HAZID report, the sync HAZOP pair and the two L1 capability traces were removed
on 2026-09-22. The root causes they identified are `RC1`–`RC12` in
[Contract Safety](../../dev/contracts/safety.md), and the items they left open are rows in the
[Verification Obligation Register](../verification-hazop.md).

## Design Exploration

| Document | Status |
|----------|--------|
| [scaling.md](scaling.md) | [VISION] Sharding design — not implemented |
| [linear_zkvm.md](linear_zkvm.md) | [VISION] ZKVM integration — types not yet implemented |
