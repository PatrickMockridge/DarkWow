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
| [hazid-report.md](hazid-report.md) | HAZID analysis of consensus edge cases |
| [sync-audit-hazop.md](sync-audit-hazop.md) | Miner sync — adversarial audit + HAZOP |
| [node-sync-hazop.md](node-sync-hazop.md) | Node sync HAZOP + guide-word study |

## Historical Snapshots

| Document | Topic |
|----------|-------|
| [l1-capability-tests-phase-trace.md](l1-capability-tests-phase-trace.md) | [HISTORICAL] L1 capability write-path — capability-tests phase delivery trace |
| [l1-capability-write-path-trace.md](l1-capability-write-path-trace.md) | [HISTORICAL] L1 capability write-path — ρ-calculus trace |

## Design Exploration

| Document | Status |
|----------|--------|
| [scaling.md](scaling.md) | [VISION] Sharding design — not implemented |
| [linear_zkvm.md](linear_zkvm.md) | [VISION] ZKVM integration — types not yet implemented |
