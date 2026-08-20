# Researcher Guide

You want to understand DarkWow's cryptographic architecture. Here's your path.

## ZK architecture

DarkWow uses **Halo2** with transparent setup — polynomial commitments over
the Pasta cycle of curves (Pallas/Vesta). No trusted setup ceremony required.
Circuits are written in **ZKAS** (zero-knowledge assembly), compiled to
Halo2 constraints, and executed inside the zkVM.

- **32 opcodes** in the zkVM. Three added by DarkWow: `LessThanOrEqual`,
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
| `ContractId` | `pallas::Base` | ↓dispatch | Confusion with AssetId, FuncId |
| `AssetId` | `pallas::Base` | ↓denominate | Confusion with ContractId |
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

Proofs are in the `proofs/lean/` directory (the `DarkFi` namespace in module
paths is a historical artifact from the upstream fork — all proofs have been
extended and verified for DarkWow). All proofs verified
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

## Quantum and Post-Quantum Security

DarkWow's quantum security posture is documented across three specifications:

- **[Quantum Threat Model](arch/quantum-threat.md)** — Formal threat specification
  with quantified qubit requirements, Grover impact on hash widths, migration
  trigger criteria (T1-T5 with coinbase signaling), retroactive privacy analysis,
  and circuit inventory for migration planning. Start here to understand the threat
  landscape.

- **[Post-Quantum Proving System Requirements](arch/zk/post-quantum-proving-system.md)** —
  Formal specification of 18 functional requirements (FR-1 through FR-18) a
  post-Halo2 proving system must satisfy for like-for-like replacement. The
  "swap-out spec" — maps each of Halo2's 12 properties to a functional requirement
  and defines the API surface, constraint system interface, arithmetization
  requirements, and quantum resistance requirements.

- **[ZK Engineering Posture](arch/zk-engineering-posture.md)** — Three-tier circuit
  classification (Tier 1: Schnorr-sufficient, Tier 2: mixed, Tier 3: genuinely
  needs ZK) that informs migration priority.

**Current posture:** Halo2 proofs rely on ECDLP (Shor-vulnerable). Hash functions
(Poseidon, Sinsemilla, Blake2b) are quantum-safe up to Grover's quadratic speedup.
P2P signatures (ed25519) are ECDLP-vulnerable. Note encryption (X25519) is ECDLP-
vulnerable. The architecture is designed for community-coordinated hard fork when
quantum timeline clarifies. No emergency governance — PoW-driven migration.

**PQXDH research:** A Signal PQXDH implementation (Kyber-1024 + X25519 + Double
Ratchet) exists at `script/research/pqxdh/` and provides the note encryption
migration path.

See also [Quantum OS](arch/quantum-os.md) for the ZFA-algebra design comparison.

## Reference

- [Crypto Schemes](spec/crypto-schemes.md) — Formal cryptographic specification
- [ZK Engineering Posture](arch/zk-engineering-posture.md)
- [Field Arithmetic](arch/zk/field_arithmetic.md)
- [Merkle Depth](arch/zk/merkle_depth.md)
- [ZKVM Primitives](arch/zk/zkvm_primitives.md)
- [ZK Verification](arch/zk/zk_verification.md)
- [Spend Hooks](arch/zk/spend_hook.md)
