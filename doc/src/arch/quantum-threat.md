# Quantum Threat Model

*Written as if a Halo2 engineer sat down with a quantum computing researcher. The
conclusion is not alarming, but it is specific: the window is unknown, the
response belongs to the ecosystem, and DarkWow's architecture makes that possible.*

---

## The Mathematical Core

Halo2 proofs rest on a single hardness assumption: the **Elliptic Curve Discrete
Logarithm Problem** (ECDLP) on the Pasta curves—Pallas and Vesta. These are
255-bit prime-order curves forming a cycle-of-curves, enabling recursive proof
composition without a trusted setup.

Shor's algorithm solves the discrete logarithm problem in polynomial time on a
sufficiently large, fault-tolerant quantum computer. The core operation is a
quantum period-finding subroutine applied to the curve's scalar multiplication
group. For a curve of order ~2^255, the quantum circuit requires on the order of
2n + 2 ≈ 1500–3000 **logical** qubits for the elliptic curve point addition
circuit, which translates to millions of **physical** qubits once error
correction (surface codes) is accounted for.

The asymmetry is stark:

| | Classical | Quantum (CRQC) |
|---|---|---|
| Proof verification | ~3ms (Pallas scalar mult) | Still fast |
| Proof forgery | Infeasible (ECDLP) | Polynomial time (Shor) |

A single party with a cryptographically relevant quantum computer (CRQC) can
extract witnesses from any Halo2 proof and forge proofs that spend arbitrary
coins. Every ZK guarantee collapses: privacy, ownership, identity, attestation.

---

## Why Halo2 Is Particularly Exposed

This is not a Halo2-specific weakness — it applies to **all** practical ZK
proving systems in production today:

| Proving System | Underlying Hardness | Quantum-Vulnerable |
|---|---|---|
| Halo2 | ECDLP (Pasta curves) | Yes |
| Groth16 | Pairings (BN254, BLS12-381) | Yes |
| PLONK | ECDLP (arbitrary curve) | Yes |
| Marlin | Pairings | Yes |
| STARKs | Hash collision resistance | **No** |
| Lattice ZK | LWE / SIS | **No** (but nascent) |

Every elliptic-curve-based ZK system — which is every system deployed at scale
today — derives its soundness from the discrete log assumption. There is no
known Halo2 variant that replaces the curve with a quantum-resistant primitive
while preserving the proving efficiency and recursion properties that make Halo2
practical.

**For DarkWow specifically:** the zkVM's 39 opcodes, all 120 contract circuits,
the Pedersen commitments, the Sinsemilla hashes, the Merkle inclusion proofs —
all of them bottom out on ECDLP over Pallas. A CRQC does not find a clever bug.
It breaks the mathematical lock the entire system hangs on.

---

## Where the Industry Is Today

Quantum computing is real and advancing, but the gap to a CRQC is measured in
**error-corrected logical qubits**, not raw physical qubits.

**Physical qubits (2024–2026):**
- IBM: 1,121 qubits (Condor, 2023); targeting 100,000 by 2033
- Google: 105 qubits (Willow, 2024); demonstrated below-threshold error correction
- QuEra / Harvard: 48–256 logical qubits via neutral atoms (2024–2025)
- Quantinuum: 56 high-fidelity trapped-ion qubits

**The gap:** Shor on a 255-bit curve requires on the order of 10^7 to 10^8
physical qubits once error correction and gate fidelity are accounted for. The
largest integer factored via Shor to date is 21 (3 × 7). Extrapolating from
current trajectories, expert estimates for a CRQC range from **10 to 50+
years**, with NIST and NSA internal planning documents clustering around the
2035–2050 window for high-confidence migration deadlines.

The honest answer is that nobody knows. The trajectory could be punctured by a
breakthrough in error correction, or it could plateau against physical limits.
What is certain is that the window exists, and its length is unknown.

---

## What a CRQC Enables in Practice

An attacker with a CRQC and access to DarkWow's P2P network can:

- **Forge any ZK proof:** Extract witnesses from any proof on-chain, construct
  proofs that spend coins they do not own, mint tokens without authority,
  satisfy any predicate without knowing the secret.
- **Drain shielded pools:** All Pedersen-committed coins in the Merkle tree are
  recoverable — the blinding factors and values are hidden only by ECDLP.
- **Sybil at will:** Identity attestations, credential proofs, competency DAGs —
  all forgeable. Any ZK-based access control becomes void.
- **Break privacy retroactively:** All on-chain proofs ever broadcast become
  transparent. Historical shielded transactions are de-anonymized.

This is not a "funds are safu" scenario. It is a mathematical collapse of
the privacy and security model. The chain itself — blocks, headers, PoW —
continues to function (RandomX is hash-based, quantum-resistant in the
Grover-quadratic sense), but every ZK contract becomes a decrypted ledger.

**What a CRQC does NOT break immediately:** RandomX PoW (hash functions are
quantum-resistant up to Grover's quadratic speedup), the linear blockchain
structure, the P2P network, the WASM runtime, and any contract logic that
does not depend on ZK proofs.

---

## Upgrade Paths

There is no drop-in replacement today. The viable paths, in order of maturity:

### 1. STARKs (short-to-medium term)

STARKs (Scalable Transparent ARguments of Knowledge) use collision-resistant
hash functions — typically Keccak or Poseidon with a large security parameter —
instead of elliptic curves. They are **quantum-resistant** by construction (hash
collision-finding gets only a Grover quadratic speedup, mitigated by doubling
the hash output width). They require no trusted setup.

**Trade-offs:** STARK proofs are 10–100× larger than Halo2 proofs (50–200 KB vs
~1–3 KB), verification is somewhat slower, and the recursion story is less
mature (though STARK-to-STARK recursion and Circle STARKs are progressing
rapidly). A STARK-based zkVM — replacing Halo2's PLONK arithmetization with an
AIR (Algebraic Intermediate Representation) — is the most realistic near-term
fork target.

### 2. Lattice-based ZK (medium-to-long term)

Zero-knowledge proofs from lattice assumptions (Learning With Errors,
Short Integer Solution) offer quantum resistance from a fundamentally different
hardness class. Several constructions exist in the literature (e.g., Lyubashevsky
style proofs, lattice Bulletproofs), but none have reached the prover
efficiency or proof size required for a blockchain VM. This is an active
research area; practical systems are likely 5–15 years out.

### 3. Hybrid approaches (any timeline)

A transitional architecture could run STARK-based proofs for value-transfer
circuits while retaining Halo2 for non-financial privacy use cases, or use a
STARK-verified Halo2 proof composition (wrapping curve-based proofs inside a
STARK). This lets the chain migrate contract-by-contract rather than all at
once.

### 4. NIST PQC migration context

The NIST post-quantum cryptography standardization process (Round 4, 2022–2025)
has produced quantum-resistant signature schemes (CRYSTALS-Dilithium, FALCON,
SPHINCS+) and KEMs (CRYSTALS-Kyber). None of these are ZK proving systems. They
protect the P2P transport layer and wallet signatures, but do nothing for
on-chain ZK proofs. NIST standardization covers the communication channel, not
the state transition function.

---

## Why DarkWow's Architecture Fits This Problem

The quantum threat is not a bug to fix — it is an **exogenous uncertainty** to
structure around. DarkWow's design makes the right thing possible when the time
comes:

**Fair launch, no premine.** There is no foundation treasury to protect, no
insider allocation to dump, no central party with privileged incentives. When the
ecosystem debates a quantum-resistant fork, there is no room full of insiders
deciding whose coins get preserved and whose get diluted. Everyone faces the
same risk, everyone has the same vote (hashrate).

**Hard forks are a feature, not a crisis.** In Uncle Merkle PoW consensus, the
canonical chain is the one with the most accumulated work. If the community
migrates to a quantum-resistant fork — one that replaces Halo2 with a STARK
verifier, for example — that fork accumulates work because miners follow it.
There is no DAO to petition, no governance token to capture, no committee to
override. The chain tips when the hashrate tips. This is the Bitcoin model:
BTC/BCH split on a protocol rule, and both chains continued under their own
accumulated work. A quantum-resistant fork is the same pattern, applied to a ZK
primitive instead of a block size.

**The decision belongs to the ecosystem.** Users assess their own quantum risk
tolerance. Cryptographers publish estimates of the CRQC timeline. Miners signal
fork readiness in coinbase. Contract developers port their circuits to STARKs or
whatever system matures. When the collective judgment is that the quantum window
is closing — or that the cost of a proactive fork is lower than the cost of
waiting — the migration happens. Nobody can force it early. Nobody can block it.

---

## The Time Window

The quantum computing timeline is the single largest exogenous uncertainty for
any ZK blockchain. There is no inside information, no special capability, no
cleverness that resolves it. The window between "ZK proofs are sound" and "ZK
proofs are forgeable" is unknown in length and unknowable in advance.

What a project can do is decide: who chooses the response, and what mechanism
executes it. DarkWow chooses the ecosystem, and the mechanism is a hard fork
driven by accumulated PoW.

No special committee. No emergency governance. No bailout. Just the same
process that handles every protocol upgrade: miners, users, and devs making
a collective bet about the future, expressed through the chain they build on.

---

## Further Reading

- [NIST Post-Quantum Cryptography](https://csrc.nist.gov/projects/post-quantum-cryptography) — Standardization process and timeline
- [Shor's Algorithm (1994)](https://arxiv.org/abs/quant-ph/9508027) — Original paper establishing polynomial-time quantum factoring and discrete log
- [STARKs](https://eprint.iacr.org/2018/046) — Scalable, transparent, post-quantum proof system
- [NIST IR 8547 (2024)](https://csrc.nist.gov/pubs/ir/2024/transition-to-pqc-standards) — Transition guidance for post-quantum cryptography
- [DarkWow Consensus](../arch/consensus/consensus.md) — Uncle Merkle PoW consensus and fork mechanics
