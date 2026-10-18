# DarkWow — Proofs

This directory contains all proof assets for the project:

| Directory | Content |
|-----------|---------|
| `core/` | 10 core system ZK circuits (.zk source + .zk.bin compiled binaries) |
| `lean/` | Lean 4 formal verification project — assumption boundary, opcode/gadget theorems, HAZOP. **Counts and the audit type are in the canonical table below, not in this line** (it read "39 opcodes + 120 contract circuits" until 2026-10-18) |

## Core ZK Circuits (`core/`)

The 10 core system circuits used by the zkVM for block validation, transaction execution,
and chain-level operations:

| Circuit | k | Purpose |
|---------|---|---------|
| `arithmetic` | 11 | Field arithmetic witness generation |
| `burn` | 13 | Coin burning with nullifier reveal |
| `encrypt` | 13 | AEAD encryption constraints |
| `inclusion_proof` | 13 | Merkle inclusion proof verification |
| `mint` | 13 | Coin minting with Pedersen commitments |
| `nested` | 11 | Nested circuit composition |
| `opcodes` | 13 | zkVM opcode dispatch and execution |
| `smt` | 14 | Sparse Merkle Tree operations |
| `tx` | 13 | Transaction verification |
| `voting` | 13 | Voting/consensus constraints |

Two circuits were one row lower in this table until 2026-09-24, and they are gone rather than
merely uncounted: `lead` (a Nov-2022 consensus prototype whose host, `consensus/leadcoin.rs`,
no longer exists in this repository — `script/circuit_comparison_exceptions.txt` even said so,
"the consumer is the consensus `lead_coin` rule, which lives outside this repository") and
`set_v1` (a 2023 `zkrunner` example). Neither was compiled by any build step
(`scripts/build-contract-zk.sh` loops `src/contract/*/proof/`; the `Makefile` compiles only
`bin/darkirc/proof/*.zk`), and neither was loaded by any crate, manifest or test, so removing
them changes nothing but the count. What it closes is the six obligation-register sites where
the mechanised audit found their exposures neither derived nor bindable and where no repair
existed: `lock`/`root`/`key`/`value` and `sigma1`/`sigma2`. See `OBL-Z16` in
`doc/src/arch/verification-hazop.md`.

All circuits use `field = "pallas"`. These are distinct from the contract-level circuits
in `src/contract/*/proof/`.

## Lean 4 Formal Verification (`lean/`)

Formal verification of the zkVM opcodes and gadgets, the cross-cutting theorems, and the
assumption boundary — organised in three layers. **The counts are in the canonical table below:
32 opcodes, 167 contract circuits, 6 assumptions.** This paragraph read "all 39 zkVM opcodes, all
120 contract ZK circuits" until 2026-10-18; those numbers do not match the tree, and the circuit
layer is a gate rather than a Lean proof.

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

### Canonical verification status (2026-10-18)

**One table. Every count is derived from the tree by the command in its last column, not carried
forward.** Where another document disagrees, this table and its command are the authority.

| What | Count | Scope — what the number covers | Audit type | Status | Command |
|---|---|---|---|---|---|
| zkVM opcodes | **32** (31 excluding the internal `Noop`) | the whole `define_opcodes!` block in `src/zkas/opcode.rs` | Lean theorems where `opcodes.md` names one per opcode; manual verdict elsewhere | **mixed** — per opcode, never a blanket "all verified" | `grep -cE '^\s+[A-Z][A-Za-z0-9]+ = 0x[0-9a-f]{2},' src/zkas/opcode.rs` |
| Gadgets | **10** files | `src/zk/gadget/*.rs` — 9 implementations + `mod.rs` | Lean: `Gadgets.lean`, `Comparison.lean` | per gadget, see `opcodes.md` | `ls src/zk/gadget/*.rs \| wc -l` |
| Contract circuits | **167** | `.zk` files under `src/contract/*/proof/` (34 contract dirs, 32 carrying proofs) | **gate**, not Lean — `script/circuit_instance_derivation.py` | **mechanized and passing** | `scripts/check-doc-canonical-counts.sh` |
| Circuits the gate walks | **179** | 167 contract + `proofs/core/` 10 + `bin/darkirc/proof/` 2 | the same gate | **0 unclassified, exits 0** | `scripts/check-circuit-instance-derivation.sh` |
| Transcribed circuits | **178** | the gate's set minus one; one `List Stmt` each, kernel-closed by `decide` | generated, freshness-gated | holds for the **transcription**, which is not the circuits | `python3 scripts/gen_circuit_transcription.py --check` |
| Assumptions | **6** | every `axiom`/`opaque` in `Axioms.lean` | gate — four-field contract + inventory | **explicit and checked** | `python3 script/check_lean_axioms.py` |

### Proof status, per claim

- **Pedersen additive homomorphism — PROVED.** `Pedersen.pedersen_additive_homomorphism`
  (`src/DarkFi/Pedersen.lean`), `@[axiom_budget 2]`: `simp only [commit, add_nsmul]; abel`, over a
  real `WeierstrassCurve.Affine.Point` for `y² = x³ + 5` over `ZMod PALLAS_MODULUS`. The seven
  structural assumptions it replaced are deleted from `Axioms.lean`.
- **`pallasPrime` — ASSUMED, the only arithmetic assumption.** `Nat.Prime PALLAS_MODULUS`; 14
  declarations reach it, the Pedersen group laws and homomorphism among them (budget 2 =
  `pallasPrime` + `Classical.choice`). A Lucas witness exists at `a = 5`; the factorisation is
  recorded in `Axioms.lean`; the certificate is the open work.
- **`poseidon_collision_resistance`, `aead_key_committing` — ASSUMED, cryptographic, and stronger
  than the deployment.** Both assert an unconditional injectivity the real sponge and the real AEAD
  do not have. `commitment_binding`, `nullifier_binding`, `merkle_root_change_detection` and
  `Net.decrypt_sound` are proved *from* them and say nothing about the deployed primitives.
- **`coinbase_blind` — ASSUMED** (a `Nat → Nat` typing, stronger than the specification).
- **`poseidon_hash_output`, `aead_open` — opaque**, carrying no content: the signatures the two
  assumptions above are stated in.
- **Nullifier determinism, signature binding (H2 fix), Merkle inclusion soundness, zero-cond
  soundness — NOT MODELLED.** The `: Prop` stubs that named them are deleted; the properties are
  enforced in the `.zk` constraints and audited by the gate, not proved in Lean.
- **`OBL-Z12` — PARTLY**, open work: a circuit proving a value is the *integer quotient* of two
  others proves it about the integers the field elements stand for, not their residues.

### Citable status block

```
DarkWow formal verification — canonical status, 2026-10-18
Opcodes 32 zkVM opcodes (31 excluding internal Noop) — mixed: Lean per-opcode theorems where named, manual verdicts elsewhere; no blanket "all verified".
Circuits 167 contract circuits under src/contract/*/proof/; the instance-derivation gate walks 179 (167 + proofs/core/ 10 + bin/darkirc/proof/ 2), 0 unclassified, exits 0 — a GATE, not a Lean proof.
Assumptions 6, all in Axioms.lean (pallasPrime the only arithmetic one): Pedersen homomorphism PROVED; nullifier determinism, signature binding, Merkle inclusion, zero-cond NOT MODELLED; OBL-Z12 PARTLY (open).
```

### Superseded: the claim this section used to carry

It read **"Layer 1 — 39 zkVM Opcodes: ALL VERIFIED … Layer 2 — 120 Contract Circuits: ALL VERIFIED
… Layer 3 … ALL VERIFIED."** That claim is **superseded as of 2026-10-18** and must not be quoted.
39 and 120 do not match the tree (32 opcodes, 167 contract circuits), and "ALL VERIFIED" was never
true of the circuit layer: `src/DarkFi/Circuits/` contains **no Lean declarations**, and the one
declaration it ever had, `burn_v1_no_free_instances`, had the statement `x = x ∧ y ∧ True` and is
deleted. Layer 3's Pedersen row said "ASSUMED" and is now **PROVED**; the four properties beneath it
were never modelled at all.

**HAZOP Tabletop**: the original population was 120 circuits; grading, the 15 flagged circuits and
the 7 cross-cutting patterns are historical findings and are unchanged by the recount above.

### Project Structure

```
proofs/lean/
├── lean-toolchain              # pinned toolchain (see the file; the skeleton here said 4.12.0)
├── lakefile.lean               # libraries: DarkFi, Transcribed (+ the generated CircuitIndex)
├── README.md                   # detailed results, including the honest-scope section
└── src/
    ├── Main.lean               # does NOT compile and is in no lean_lib; no gate runs it
    ├── Transcribed.lean        # generated per-circuit statement lists, freshness-gated
    ├── CircuitIndex.lean       # generated inhabitants of CircuitDerivable, freshness-gated
    └── DarkFi/
        ├── Axioms.lean         # THE ONLY file permitted an axiom or a value-less opaque — 6
        ├── AxiomBudget.lean    # the @[axiom_budget] attribute the collector reads
        ├── Pedersen.lean       # Pallas as a real curve: group laws + homomorphism PROVED
        ├── Field.lean / BaseDiv.lean / Arithmetic.lean / Soundness.lean
        ├── ECOps.lean / HashOps.lean / Gadgets.lean / Comparison.lean
        ├── CrossCutting.lean   # value conservation; the four unmodelled claims are deleted
        ├── Emission.lean       # the reward schedule, as a definition
        ├── Capability/         # the capability model (Purse, Inversion, Composition, …)
        ├── Net/                # the receive path, and the AEAD assumption boundary
        ├── HAZOP.lean          # risk matrix; HAZOP/{Critical,High,Elevated}.lean
        └── Circuits/           # COMMENT-ONLY — no declarations. The Orchard-class audit is a gate
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
