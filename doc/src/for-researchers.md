# Researcher Guide

You want to understand DarkWow's cryptographic architecture. Here's your path.

## ZK architecture

DarkWow uses **Halo2** with transparent setup — polynomial commitments over
the Pasta cycle of curves (Pallas/Vesta). No trusted setup ceremony required.
Circuits are written in **ZKAS** (zero-knowledge assembly), compiled to
Halo2 constraints, and executed inside the zkVM.

- **39 opcodes** in the zkVM. Three added by DarkWow: `LessThanOrEqual`,
  `IsNotEqual`, `BaseDiv` — all formally verified in Lean4.
- **228 ZK circuits** across 32 contracts.
- **Poseidon hash** for all commitments (Pallas base field), replacing
  EC Pedersen to eliminate EC heap bugs found in upstream circuits.

See [Opcodes](arch/zk/opcodes.md), [Opcodes Status](arch/zk/opcodes-status.md),
and [Opcode Universe](arch/zk/opcode_universe.md) for the complete opcode catalog.

## Type system: ρ-calculus

DarkWow's type system derives from the **ρ-calculus** (reflective higher-order
π-calculus). Every cryptographic identifier is a newtype wrapper with declared
**barbs** (observable actions):

| Primitive Type | Wraps | Barb | Prevents |
|---------------|-------|------|----------|
| `Nullifier` | `pallas::Base` | ↓nullify | Confusion with CoinCommitment; zero-as-nullifier injection |
| `CoinCommitment` | `pallas::Base` | ↓commit | Confusion with Nullifier; non-canonical field elements |
| `ContractId` | `pallas::Base` | ↓dispatch | Confusion with TokenId, FuncId |
| `TokenId` | `pallas::Base` | ↓denominate | Confusion with ContractId |
| `FuncId` | `pallas::Base` | ↓gate | Confusion with ContractId |
| `PublicKey` | `pallas::Point` | ↓verify | (x,y) pair fragmentation; identity point injection |
| `SecretKey` | `pallas::Base` | ↓spend, ↓derive | Confusion with Nullifier |

The **Type Distinction Principle**: two types unify only if their barbs
match. `Nullifier` and `CoinCommitment` are both `pallas::Base` but the
compiler rejects swapping them — they are not bisimilar.

See [Type System Specification](arch/type-system.md) for the formal
specification (RFC 2119 SHALL/MUST) and the Authorization Inversion Theorem.
See [Capability Composition](arch/composition.md) for how primitives compose
into capabilities.

## Formal verification: Lean4

DarkWow has 26 Lean4 proof files verifying:
- **36 capability-type pairs** are pairwise non-bisimilar (zero `sorry`)
- `LessThanOrEqual`, `IsNotEqual`, `BaseDiv` opcodes are sound
- The Authorization Inversion Theorem: `∀a,b. bisimilar(a,b) ⟹ barbs(a) = barbs(b)`

Proofs are at `proofs/lean/src/DarkFi/Capability/`. All proofs verified
with zero `sorry` — no admitted axioms, no hand-waving.

## Supply audit

DarkWow implements **per-block Pedersen mass balance**: every block must satisfy
Σ outputs + Σ burns + Σ fees == Σ inputs via additive homomorphism. The
cumulative supply commitment chain is verifiable without ZK proofs — you
can audit the entire monetary supply from genesis with simple arithmetic.

This architecture would have caught the Zcash Orchard exploit (May 2026) — a
silent inflation bug that printed coins for years because there was no
block-level mass balance check.

See [Consensus Specification](arch/consensus/consensus.md#supply-audit-capability).

## Privacy model

- **ZK predicates (boolean only)**: The verifier learns that a statement is
  true — nothing else. No identity revelation, no metadata leakage.
- **Deterministic wallet**: Same keys + same chain = identical wallet state.
  The wallet is a pure mathematical function of identity and chain data.
  No server-side state, no RPC delegation.
- **AEAD-encrypted notes**: Capabilities are discovered by decrypting notes
  on-chain. Only the holder of the correct key can discover a capability.

See [O-Cap & Composable Privacy](arch/ocap.md) and
[Wallet Architecture](arch/wallet.md).

## Trust model

Three independent trust layers:
1. **Consensus** (Uncle Merkle) — deterministic, stateless verification
2. **Execution** (zkVM) — every state transition verified in ZK
3. **Finality** (Caribina + Monero merge-mining) — external temporal anchoring

See [Contract Trust Model](arch/contract-trust-model.md) and
[Formal Specification](arch/formal-specification.md).

## Quantum threat model

See [Quantum Threat Analysis](arch/quantum-threat.md) and
[Quantum OS](arch/quantum-os.md).

## Reference

- [Crypto Schemes](spec/crypto-schemes.md) — Formal cryptographic specification
- [ZK Engineering Posture](arch/zk-engineering-posture.md)
- [Field Arithmetic](arch/zk/field_arithmetic.md)
- [Merkle Depth](arch/zk/merkle_depth.md)
- [ZKVM Primitives](arch/zk/zkvm_primitives.md)
- [ZK Verification](arch/zk/zk_verification.md)
- [Spend Hooks](arch/zk/spend_hook.md)
