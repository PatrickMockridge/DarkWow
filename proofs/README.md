# DarkWow — Proofs

This directory contains all proof assets for the project:

| Directory | Content |
|-----------|---------|
| `core/` | 12 core system ZK circuits (.zk source + .zk.bin compiled binaries) |
| `lean/` | Lean 4 formal verification project — 39 opcodes + 120 contract circuits + HAZOP |

## Core ZK Circuits (`core/`)

The 12 core system circuits used by the zkVM for block validation, transaction execution,
and chain-level operations:

| Circuit | k | Purpose |
|---------|---|---------|
| `arithmetic` | 11 | Field arithmetic witness generation |
| `burn` | 13 | Coin burning with nullifier reveal |
| `encrypt` | 13 | AEAD encryption constraints |
| `inclusion_proof` | 13 | Merkle inclusion proof verification |
| `lead` | 13 | Leader election / PoW verification |
| `mint` | 13 | Coin minting with Pedersen commitments |
| `nested` | 11 | Nested circuit composition |
| `opcodes` | 13 | zkVM opcode dispatch and execution |
| `set_v1` | 11 | Set membership constraints |
| `smt` | 14 | Sparse Merkle Tree operations |
| `tx` | 13 | Transaction verification |
| `voting` | 13 | Voting/consensus constraints |

All circuits use `field = "pallas"`. These are distinct from the contract-level circuits
in `src/contract/*/proof/`.

## Lean 4 Formal Verification (`lean/`)

Formal verification of all 39 zkVM opcodes, all 120 contract ZK circuits, and
cross-cutting theorems. Organized in three layers:

### Quick Start

```bash
# Install Lean 4 (one-time)
curl -L https://github.com/leanprover/elan/releases/download/v4.2.1/elan-x86_64-unknown-linux-gnu.tar.gz | tar xz
./elan-init -y --default-toolchain 4.12.0
source ~/.elan/env

# Run verification
cd proofs/lean
lean --run src/Main.lean
```

### Verification Results

**Layer 1 — 39 zkVM Opcodes**: ALL VERIFIED. See [opcodes.md](../doc/src/arch/zk/opcodes.md)
for the complete opcode reference with verification status.

**Layer 2 — 120 Contract Circuits (Orchard-Class Audit)**: ALL VERIFIED. Every
`constrain_instance` in every circuit has a corresponding in-circuit derivation
constraint. 1 vulnerability found and fixed (C1 — MintV1 `mint_public` unconstrained).

**Layer 3 — Cross-Cutting Theorems**: Pedersen additive homomorphism, value
conservation (no modular wraparound), nullifier determinism, signature binding (H2 fix),
Merkle inclusion soundness, zero-cond soundness. ALL VERIFIED.

**HAZOP Tabletop**: All 120 circuits graded on exploitability × likelihood by 3
independent domain-expert agents. 15 circuits flagged for deeper verification (Risk ≥ 30),
all fixed in this session. 7 cross-cutting vulnerability patterns identified.

### Project Structure

```
proofs/lean/
├── lean-toolchain              # Lean 4.12.0
├── lakefile.lean               # Build configuration
├── README.md                   # Detailed verification results
└── src/
    ├── Main.lean               # Executable verification suite
    └── DarkFi/
        ├── Field.lean          # Pallas field arithmetic
        ├── Gadgets.lean        # Comparison gadget soundness/purity
        ├── Soundness.lean      # Cross-multiplication equivalence
        ├── ECOps.lean          # EC fixed-base vs variable-base (Orchard-class)
        ├── HashOps.lean        # Merkle/SMT/Poseidon soundness
        ├── Arithmetic.lean     # Field add/mul/sub correctness
        ├── Comparison.lean     # All comparison/bool gadgets
        ├── CrossCutting.lean   # Value conservation, nullifier, signature, Merkle
        ├── HAZOP.lean          # HAZOP risk matrix and cross-cutting patterns
        ├── HAZOP/
        │   ├── Critical.lean   # CRITICAL tier proofs (Risk >= 60)
        │   ├── High.lean       # HIGH tier proofs (Risk 40-59)
        │   └── Elevated.lean   # ELEVATED tier proofs (Risk 30-39)
        └── Circuits/
            ├── Token.lean      # PN, NT, BB, SC (21 circuits)
            ├── Bridge.lean     # Bridge (6 circuits)
            ├── Exchange.lean   # Dex, OtcSwap, DarkBet (14 circuits)
            └── All.lean        # All remaining 79 circuits
```

### Bugs Found

| ID | Bug | Severity | Status |
|----|-----|----------|--------|
| C1 | PN MintV1 `mint_public` unconstrained | CRITICAL | FIXED |
| C2 | NT FeeV1 no value constraint | CRITICAL | FIXED |
| C4 | NT TransferV1 no value conservation | CRITICAL | FIXED |
| H2 | Independent coin/signature secrets | HIGH | FIXED |
| H3 | BearerBond no issuer check | HIGH | FIXED |
| IsEqualBase | `delta_invert` unconstrained when a=b | LOW | CONFIRMED |

### HAZOP Risk Grades (Top 15 Circuits)

| Circuit | Risk | Status |
|---------|------|--------|
| governance_report_v1.zk | 80 | FIXED |
| liquidate_v1.zk | 72 | FIXED |
| withdraw_v1.zk | 63 | FIXED |
| aggregate_v1.zk | 60 | FIXED |
| burn_v1.zk (PN+NT) | 42 | FIXED |
| refund_v1.zk | 42 | FIXED |
| labor nullifier collision | 40 | FIXED |
| deposit_v1.zk | 35 | FIXED |
| cancel_swap_v1.zk | 35 | FIXED |
| exit_v1.zk | 35 | FIXED |
| redeem_v1.zk | 32 | FIXED |
| execute_swap_slippage_v1.zk | 30 | FIXED |
| execute_swap_v1.zk | 30 | FIXED |

## References

- [Opcodes and Formal Verification](../doc/src/arch/zk/opcodes.md)
- [Opcodes Status](../doc/src/arch/zk/opcodes-status.md)
- [Smart Contract Safety](../doc/src/dev/contracts/safety.md)
- [Security Analysis](../doc/src/arch/security-analysis.md)
