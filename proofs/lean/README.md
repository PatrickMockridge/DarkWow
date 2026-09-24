# Lean4 Formal Verification — DarkWow Type System & ZK Gadgets

Formal specification and verification of the DarkWow cryptographic type system
and zkVM opcode gadgets using Lean 4 (v4.12.0).

**Mathlib is a dependency**, pinned in `lakefile.lean` to `v4.12.0` and resolved in
`lake-manifest.json`. (This file used to claim "Zero Mathlib dependencies — all proofs use core
Lean 4"; `Field.lean`, `Arithmetic.lean`, `Gadgets.lean`, `CrossCutting.lean` and the
capability modules all `import Mathlib`, and several proofs cite mathlib lemmas by name.)

**To verify everything:**

```bash
cd proofs/lean && lake build DarkFi
```

Note the target. A bare `lake build` builds "the default facet of the root package", which for
this package is **nothing at all** — it exits 0 without compiling a single module. This README
said `lake build` for a long time, and the CI gate in `scripts/run-all-tests.sh` called it, so
the verification that was supposed to be happening was not. `lake build DarkFi` type-checks the
proofs.

## What the build currently says

Run `lake build DarkFi` before trusting anything below. As of 2026-09-20 it completes with **no
errors and no warnings**. It previously carried 22 `unused variable` warnings — a hypothesis or
binder no proof step consumed — and those turned out to be the same defect as the tautologies at a
smaller scale: `l1_combinatorial_asymmetry` was `l1_exceeds_l2` under a name whose `(c : Halo2L1Contract)
(hL1 : isL1 c)` parameters appeared in neither the statement's use nor the proof, `has_deadlock`
returned `false` for every input and ignored its argument, and eleven of the unused binders sat on
hypotheses that turned out to be removable — removing them *strengthened* the theorems.
`zero_cond_correct` no longer assumes `g.a = 0`, and `less_than_strict_sound` no longer assumes the
input `a` is range-checked, because `linarith` derives `a < b` from the offset bounds alone.

This section used to report errors in twelve modules and point at the axiom checker's `SKIP`. That
was accurate when written and became false when the build was repaired; the lesson is the one this
file keeps relearning — a claim about the current state decays, so state the date and re-run the
command. A red build and a clean assumption boundary are different claims, and this file does not
let the second stand in for the first.

## How to read a theorem here

Every theorem that depends on an assumption carries `@[axiom_budget N]`, where `N` is the
number of assumptions its proof reaches:

    @[axiom_budget 0]  theorem field_wraparound_safe   -- proved
    @[axiom_budget 1]  theorem some_classical_proof    -- Classical.choice, nothing else
    @[axiom_budget 1]  theorem nullifier_eq_poseidon_of_coin  -- rests on HashOps.poseidon_hash_output

`0` means proved. Anything above `0` is conditional, and the count is visible at the theorem
rather than inferable from reading the file. `script/check_lean_axioms.py` fails the build when
an annotation is missing or disagrees with what `Lean.collectAxioms` reports, and prints the
whole table.

Every assumption lives in `src/DarkFi/Axioms.lean` and carries four fields — what it assumes,
why it is not proved, what would discharge it, and what breaks if it is false. Nothing else in
`proofs/lean/` may contain an `axiom` or a value-less `opaque`.

## What Is Proved

### Part 1: Capability Type System (`Capability/`)

The type system formalizes the ρ-calculus barb model from `type-system.md`.
Every cryptographic primitive is a distinct behavioral type with a fixed barb
set. Capabilities are existential proofs that a list of primitives covers the
barbs required by a resource.

| Module | Content | Key Theorems |
|--------|---------|-------------|
| `Types.lean` | 17 primitive types with barb sets, 3 raw byte containers (for distinction proofs) | — (definitions) |
| `Pareto.lean` | All primitive pairs have distinct barb sets | `primitiveTypesAreParetoEfficient` (by `decide`), 15 pairwise lemmas, `barbEqualityImpliesTypeEquality` |
| `Distinction.lean` | 10 non-unifiable type pairs (e.g. nullifier ≠ `[u8;32]`) | All 10 proved by `decide` (kernel-checked), `allUnifiablePairsProved` |
| `Composition.lean` | 12 concrete capability types (native token transfer, DAO vote, tender bid, coinbase claim, purse balance/withdraw, identity credential, box take, multisig approval, attestation, bridge deposit/withdraw) | `barbPreservation` (induction over primitives list), `coversBarbs` for each type |
| `Wallet.lean` | Wallet capability construction function | `walletConstruct_sound`, `_complete`, `_preservesPrimitives`, `_deterministic`, `_rejects_emptyPrimitives` (`_idempotent` was `x = x` and is deleted) |
| `Axioms.lean` | **The assumption boundary** — the only file permitted to contain an `axiom` or a value-less `opaque`. Every assumption carries four fields, and `script/check_lean_axioms.py` enforces it |
| `Inversion.lean` | The circuit bridge as a *hypothesis* (`CircuitDerivable`) and `capabilityType_of_circuitDerivable` (one-directional). `authorizationInversion_TypeLevel` (bidirectional: type exists iff barbs covered — a claim about barb coverage, **not** about ZK proof systems), `verifierLearnsOnlyRequiredBarbs`. The former `circuitSoundnessBridge` axiom asserted that every capability type exists and is deleted |

**Capability types defined (12):** `nativeTokenTransferType`, `nativeTokenCoinbaseType`,
`daoVoteType`, `tenderBidType`, `purseBalanceType`, `purseWithdrawType`,
`identityCredentialType`, `boxCapType`, `multisigApprovalType`, `attestationType`,
`bridgeDepositType`, `bridgeWithdrawType`.

### Part 2: ZK Opcode Gadgets (`Gadgets.lean`, `Comparison.lean`, `Arithmetic.lean`)

Soundness theorems for zkVM opcode constraint systems. Each theorem proves: if the
constraint equations are satisfied, the output equals the mathematical function.

| Theorem | Opcode | Property |
|---------|--------|----------|
| `less_than_or_equal_sound` | 0x55 | out=1 iff a≤b for bounded inputs |
| `less_than_strict_sound` | 0x51 | Offset bounded in [0, 2^m) → a\<b (the input range check is not needed — see below) |
| `is_not_equal_fully_pure` | 0x62 | All witnesses fully constrained (no degrees of freedom) |
| `is_not_equal_pure_when_equal` | 0x62 | delta_invert forced to 1 when a=b |
| `is_not_equal_delta_invert_unique_when_unequal` | 0x62 | (a-b)\*delta_invert=1 unique when a≠b |
| `is_equal_bug_when_equal` | 0x54 | **Bug found:** delta_invert unconstrained when a=b |
| `is_equal_fixed_pure_when_equal` | 0x54 | Fix pattern: purity constraint forces delta_invert=1 |
| `boolcheck_sound` | 0x53 | value\*(value-1)=0 → value∈{0,1} |
| `cond_select_correct` | 0x60 | Correct conditional selection |
| `zero_cond_correct` | 0x61 | is_zero=1 → output=0 |
| `zero_cond_nonzero` | 0x61 | is_zero=0 → output=b |
| `base_add_correctness` | 0x30 | No wraparound for 64-bit inputs |
| `base_mul_correctness_bounded` | 0x31 | No wraparound for 64-bit inputs |
| `base_sub_ge_case` | 0x32 | No wraparound when a≥b≥0 |
| `base_div_by_zero` | 0x58 | Division by zero returns 0 |

### Part 3: Cross-Cutting & Arithmetic

| Module | Key Theorems |
|--------|-------------|
| `Field.lean` | `cross_mul_lt` (integer cross-multiplication soundness), `wraparound_safe` (bounded inputs preserve ordering) |
| `CrossCutting.lean` | `pedersen_sum_equality_implies_value_equality` (a `congrArg`, named for what it proves), `value_conservation_no_wraparound` (16×64-bit values fit in Pallas field) |
| `HashOps.lean` | Model scaffolding only — `MerklePath`, `SMTMembershipGadget`, `PoseidonHashGadget` and friends. Its former "theorems" `merkle_root_deterministic` (`x = x`) and `merkle_inclusion_soundness` (its own hypothesis, plus a `root = root` hypothesis) are deleted; `merkle_root_change_detection` is an assumption in `Axioms.lean` |
| `ECOps.lean` | `ec_add_inputs_must_be_distinct`, and model scaffolding (`ECMulGadget`, `ECAddGadget`). `fixed_base_mul_uses_constant` and `variable_base_mul_is_prover_chosen` are assumptions in `Axioms.lean`; `pedersen_commitment_binding` was a tautology declared as an axiom and is **deleted**, not re-proved — re-proving it would have produced a budget-0 theorem named for Pedersen binding whose statement says nothing about a commitment |
| `Soundness.lean` | `cross_mul_implies_ratio_bound` — a `theorem`, discharged from `Field.cross_mul_lt` rather than assumed. Its former `less_than_strict_sound` was `a < b → a < b` and is deleted |

### Part 4: Supply Chain Invariants (`SupplyChain.lean`)

Multi-block induction over cumulative supply commitments:

| Theorem | Property |
|---------|----------|
| `total_supply_theorem` | After H blocks, total_supply = expected_cumulative_supply(H) |
| `cumulative_commit_theorem` | Cumulative Pedersen commitment = sum of per-block commitments |
| `supply_chain_invariant` | Conjunction of both |
| `no_hidden_inflation` | Total supply exactly matches expected cumulative supply |

### Part 5: Combinations of L1 contracts (`Combinatorial/Combinations.lean`)

**The axis the rest of `Combinatorial/` does not have.** `Transitions.l1TrajectoryCount N K = N ^ K`
counts trajectories over N objects *within one contract* — it has no contract-count parameter, which
is why `CompositionBounds` came to carry `ocap_scaling (k : Nat) : True := by trivial` under a heading
about scaling. With C contracts offering `nᵢ` operations each, the number of distinct operation
combinations is `∏(nᵢ + 1) − 1`:

| Theorem | Property |
|---------|----------|
| `two_pow_sub_one_le_combinationCount` | `2^C − 1 ≤ count` for `nᵢ ≥ 1` — exponential, and independent of anything else about the contracts |
| `increment_ge_value` | `f C ≤ f (C+1) − f C`: the increment is at least the accumulated value. A linear function's increment is a *constant*; this one grows |
| `combinationsIncludingIdle_append` | appending a contract multiplies the count by `n+1` |
| `combinationsIncludingIdle_append_doubles` | `2 · f C ≤ f (C+1)` |
| `combinationCount_gt_sum` | the count exceeds `Σ nᵢ` at every `C ≥ 2` — the refutation of the additive reading, in Lean |
| `card_biUnion_le_sum` | **what *is* additive**: `\|⋃_{c∈S} B c\| ≤ Σ_{c∈S} \|B c\|` — the size of one composition, i.e. containment |
| `contractOps_combinationCount` | **615 192 791 076 863 999 999 999** over the 31 contracts / 166 circuits in the tree, by `norm_num`; see `genesisOps_combinationCount` (6911) for the genesis subset |

**This corrects the documents, and the correction is the point.** `privacy.md` §6, `safety.md`
Lesson 23, `contract-wasm-type-system.md` §C.7 and `ai-index.md` all state that o-cap composition is
additive — "the state spaces add, not multiply" — and cite
`CompositionBounds.ocap_additive_composition`. That theorem is a rewriting lemma: after
`rw [box_total_linear, purse_total_linear]` its two sides are syntactically identical, and no
operation composing two contracts appears anywhere in the tree. O-caps isolate contract *state*; they
do not divide the *number of ways to combine contracts*, which is a product with or without them.
What o-caps buy is containment — a bounded blast radius per composition — which is a different
quantity, and the exponential combination space is why compositional reasoning is needed rather than
enumeration.

## Axioms: What Is Assumed

All of them are in **`src/DarkFi/Axioms.lean`** and nowhere else. Each carries four fields
(`ASSUMES`, `NOT PROVED BECAUSE`, `DISCHARGED BY`, `IF FALSE`), and
`script/check_lean_axioms.py` fails the build if one is missing. What follows is a summary;
`Axioms.lean` is the source of truth.

### The assumption classes

| Class | Count | Members |
|-------|-------|---------|
| Cryptographic | 6 | `poseidon_hash_output` (value-less opaque), `poseidon_collision_resistance`, `commitment_binding`, `nullifier_binding`, `merkle_root_change_detection`, `purseNullifier_nonce_injective` |
| Arithmetic | 1 | `Arithmetic.base_div_mul_cancel` |
| Model-to-VM correspondence | 2 | `ECOps.fixed_base_mul_uses_constant`, `ECOps.variable_base_mul_is_prover_chosen` |
| Pallas group model | 8 | `PedersenPoint.add`, `PedersenIdentity`, `pedersen_commit`, `pedersen_add_comm`, `pedersen_add_assoc`, `pedersen_add_identity`, `pedersen_additive_homomorphism`, `compute_merkle_root` |
| Emission policy (free parameters) | 5 | `reward`, `MAX_SUPPLY`, `coinbase_blind`, `reward_monotone`, `total_reward_bounded` |
| ZK-to-type bridge | 1 | `NoFreeInstances` |

### What this file used to say, and why it was wrong

This section previously listed **28 axioms** across four tables — "6 cryptographic", "11
Circuit Audit Axioms", "3 hash & Merkle", "8 supply chain" — under a heading that said "None
have computational content". Every claim in it was wrong in a different way:

- **"Circuit Audit Axioms (11 axioms)"** described declarations that do not exist. The eleven
  names (`burn_v1_signature_binding`, `mint_v1_c1_fix`, `all_contracts_orchard_safe`,
  `bridge_circuits_orchard_safe`, `exchange_circuits_orchard_safe`, …) are **comment lines** of
  the form `-- ASSUMPTION (not proven): …`. `Circuits/All.lean`, `Circuits/Bridge.lean` and
  `Circuits/Exchange.lean` are comment-only files containing no declarations at all, and
  `redeem_v1_coin_value_enforced_by_host` appears nowhere in the tree. The comment lines now
  read `-- NOT DECLARED IN LEAN`.
- **`circuitSoundnessBridge`** asserted that every capability type exists, because its
  antecedent `∃ (circuit : String), True` was true for every resource and action (witness
  `""`). It is deleted and replaced by a hypothesis, `CircuitDerivable`.
- **`div_mul_cancel`** was a byte-identical duplicate of `base_div_mul_cancel` under a second
  name. It is deleted; the surviving copy is `Arithmetic.base_div_mul_cancel`.
- **`pedersen_additive_homomorphism`** appeared in two files as two different statements: an
  `ECOps` one that was a `: Prop` stub asserting nothing, and the real equality in
  `SupplyChain`. The stub is deleted.
- **`variable_base_without_binding_is_orchard_class`**, `nullifier_determinism`,
  `signature_binding_h2_fix`, `merkle_inclusion_foundation`, `smt_membership_sound` and
  `smt_membership_privacy` were all `: Prop`-valued axioms. An uninterpreted predicate *names*
  a claim without stating one, and no proof can consume one. All six are deleted, and where
  their names were cited as evidence (a "VERIFIED" table in `CrossCutting.lean`, and rows in
  `doc/src/arch/zk/opcodes.md`) those citations are corrected.
- **`reward_nonneg`** was listed as an assumption. `reward : Nat → Nat` makes `reward h ≥ 0` a
  consequence of `Nat.zero_le`; it is now a theorem with budget 0.
- **"None have computational content"** was false for the class of `: Prop` stubs it was
  describing — they had no content of any kind, which is worse.

The count is now **9**, and the honest framing is not "these are cheap" but: **five of them have no
consumer at all.** Measured, not asserted — the collector's per-theorem axiom sets give the
consumer count directly:

| assumption | consumers |
|---|---|
| `pallasPrime` | 14 (`Pedersen.*`, and everything downstream of the curve being a field) |
| `coinbase_blind` | 7 (`cumulative_auditable`, `cumulative_commit_theorem`, `no_hidden_inflation`, …) |
| `HashOps.poseidon_collision_resistance` | 6 (`commitment_binding`, `nullifier_binding`, `smtCrh_injective`, `purseNullifier_nonce_injective`, …) |
| `NoFreeInstances` | 1 — `capabilityType_of_circuitDerivable` |
| `Arithmetic.base_div_mul_cancel` | **none** |
| `ECOps.fixed_base_mul_uses_constant` | **none** |
| `ECOps.variable_base_mul_is_prover_chosen` | **none** |
| `HashOps.poseidon_hash_output` | **none** |
| `reward_monotone` | **none** |

`NoFreeInstances` used to be the register's example of an assumption "consumed by nothing". It now
has a consumer, which is what §3 of the rewrite was for: `capabilityType_of_circuitDerivable` takes
`CircuitDerivable r s` as a *hypothesis* rather than resting on a vacuous axiom, and that hypothesis
is `NoFreeInstances` in the type system's vocabulary. A theorem that names its own gap is one a
reader can act on; a `: Prop` axiom with a vacuous antecedent is not.
Six more are reached only because `SupplyChain.lean`'s proofs unfold definitions that mention
them. The rest cannot fail — their falsity would be undetectable here, because nothing depends
on them. `DarkFi.HAZOP.Elevated` records each one and collects them as
`silentAxiomFailures`: that list is the case for discharging them.

## What Is NOT Proved (Honest Scope)

- **Nothing below is a claim about a build you have not run.** `lake build DarkFi` completes clean
  as of 2026-09-22 (no errors, no warnings — see "What the build currently says" above), but the
  build is what makes every statement below true, so run it before quoting any of them.
- **The ρ-calculus is mechanized; its α-rule is not.** `Semantics/` defines the syntax, structural
  congruence, a labelled transition system and a substitution layer, and proves §1.2's parallel laws, the
  barb calculus — a barb is computable from the syntax, `barb_par_iff`, `barb_rep_iff`, `barb_nu_iff` —
  and the weak relation's algebra as theorems about processes. The binding convention is the one §0's
  quote/eval and canonical-bytes reading fixes: `FreeOccurs` (binders bind, `bang` seals) and a `subst`
  that arrests at both. Putting that convention against the observations yields one theorem and one
  refutation, and the pair is the point: `canBarb_has_free_name` — every barb is on a channel congruent
  to a name that occurs free in the term — holds, so the convention is not arbitrary; and
  `not_barb_of_freshFree_is_false` refutes its converse, because `ν0.0 ≡ 0` lets a term barb on a name
  it does not mention at all. Freeness is syntactic, and the congruence is exactly what it is not
  invariant under — which is why every proviso in the LTS tests a **channel** with `SCong` rather than a
  name, and why `Proc.lean`'s `Occurs` is not used as a proviso anywhere.

  What the layer does **not** have is an α-rule on `SCong`, so `Step.tau` carries `CaptureFree` as a
  proviso and `subst` relabels the channels that labels are made of (`subst_moves_the_label`) — adding
  the rule is a redesign of the label predicates rather than a constructor, which is what that theorem
  measures, and a corpus-wide search found no consumer for it. **How deep the gap goes is measured too**:
  the α-variant pair `νx.(out b x)` and `νy.(out b y)` are separated not only by the congruence but by
  the strong relation (`alpha_variants_not_strongbisim`) and by the barbed one
  (`alpha_variants_not_barbedEq`), because a label's channel and payload are *free* names and renaming a
  bound one is relabelling. So the fix is a *bound-output* label — the form `Label`'s docstring records
  as absent — not a rule, and the deferral is by measurement rather than by absence of demand. One repair
  failed and is kept as such:
  quantifying the restriction rule's freshness over the congruence produced `FreshUpToScong`, a
  condition that is **unsatisfiable** (`not_freshUpToScong`), so the extrusion rule could not fire for
  four commits while reading as available; that rule is deleted, and extrusion survives only in
  `SCong0`, the reachability relation. The restriction rule does not wait on the convention, because its
  condition belongs on the **label** (`¬ SCong x (subject μ)`), and its obligation is **discharged**:
  `barb_nu_iff` states `Barb (νx.P) a ↔ ¬ SCong x a ∧ Barb P a` — a restriction blocks exactly its own
  name and nothing else — and `not_barb_nu_self` is the case that was open. What closed it was
  `CanBarb`, a structural reading of the barb predicate that keeps the restriction's binder, where
  `CanStep` had to drop it; `Semantics/LTS.lean`'s scope note keeps all five answers the obligation
  took, because what each one got wrong is the reusable part. Also absent: `type-system.md` §9.2's
  `parallelMerge_correctness`, whose `≈` conclusion is not mechanized. `Semantics/Ledger.lean` proves
  the safety property it rests on instead — `exec_perm`, that every execution order of a list of
  pairwise-disjoint calls produces the same store — because §9.2's `parallel_execute` has no Rust
  counterpart: the schedule is a diagnostic and calls execute sequentially today.
- **Halo2 constraint system semantics are not modeled.** We prove properties of the
  mathematical functions the opcodes implement, not that the Halo2 gate/region/
  copy-constraint system correctly implements those functions.
- **Circuit-to-Lean correspondence is not mechanized.** The `Circuits/` directory documents a
  manual audit of the `.zk` files, not machine-verified extraction. The obligation that would
  make it mechanized is `Axioms.NoFreeInstances`, which is uninterpreted and unconsumed.
- **Poseidon is an opaque function, not the sponge.** `poseidon_hash_output` is a value-less
  `opaque`, so nothing about the P128Pow5T3 permutation is proved — not even determinism, which
  is a consequence of its being a function and needs no proof. (This section used to say it was
  defined as `inputs.head?.getOrElse 0 + 1`; that was true of a much older version and has been
  false since.)
- **Pedersen point addition is also opaque** — `PedersenPoint.add` has no value, so it is not
  `Nat` addition and not curve addition. The group laws
  (`pedersen_add_comm`/`_assoc`/`_identity`) are assumptions about an unmodelled operation, and
  the supply-chain inductions do not use them: they are `rfl`/`simp`-level. This section used
  to say the operation "is implemented as `Nat` addition", which understated the gap in one
  direction (nothing is implemented) and overstated the proofs in the other.
- **Fermat's Little Theorem is not the blocker.** `base_div_mul_cancel` needs
  `Nat.Prime PALLAS_PRIME` — a 254-bit Pratt certificate. Mathlib *is* a dependency (pinned at
  `v4.12.0` in `lakefile.lean`), contrary to what this section and the file header claimed.
- **The modulus was wrong, and that made an assumption *false* rather than unproved.**
  `Axioms.PALLAS_MODULUS` read `2^254 - 2^32 - 2^7 - 2^4 - 2 - 1` until 2026-09-20, which is
  divisible by 3. The real Pallas modulus is
  `0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`. Because
  `instance : Fact (Nat.Prime PALLAS_MODULUS)` derives `ZMod PALLAS_MODULUS`'s `Field` structure
  from it, every theorem in `Pedersen.lean` was proved from a falsehood — vacuous rather than
  conditional, which the budget table cannot distinguish. Four files spelled the same wrong
  expression independently, and the docstring that said the curve was "verified against the
  vendored implementation" was true of the *generator* and silent about the *modulus*. Corrected,
  with the tie to `pasta_curves` as a budget-0 theorem (`pallasModulus_eq_pasta_curves`) and the
  old value's compositeness proved (`oldPallasModulus_was_composite`). `pallasPrime` is now a true
  statement that remains unproved. See `verification-hazop.md`, "The assumption that was false".
- **The emission policy is not proved.** `reward`, `MAX_SUPPLY` and `total_reward_bounded` are
  declared, not derived. `total_supply_theorem` proves that a running total equals the sum of a
  schedule — for *every* schedule. It does not prove the schedule is capped, and
  `doc/src/arch/genesis.md` no longer presents the cap as a Lean result.
- **No verified compiler from `.zk` files.** The ZKAS compiler produces Halo2 circuits; there is
  no formal semantics for the ZKAS language in Lean4.

## Verification

```bash
cd proofs/lean

# Type-check the proofs. `DarkFi` is required — a bare `lake build` compiles nothing.
lake build DarkFi

# The assumption boundary: no sorry/admit, every assumption in Axioms.lean with its four
# fields, every theorem annotated with its budget, no native_decide anywhere, no tautology,
# and every `Recorded in DarkFi.HAZOP.X` citation resolving to an entry that exists.
cd ..
python3 script/check_lean_axioms.py

# The axiom table, straight from the compiled environment.
cd proofs/lean && lake env lean --run src/CheckAxioms.lean

# The Orchard-class rule over the 180 .zk circuit sources — a separate boundary.
bash scripts/check-circuit-instance-derivation.sh
```

`script/check_lean_axioms.py` prints a table of theorem → budget → the assumptions each
theorem rests on. Use `--emit-annotations` to write measured budgets back into the sources, and
`--require-collector` to make "the collector could not run" fatal rather than a `SKIP`.

**Check 7 — no tautologies.** This is the bar that is easiest to state and hardest to keep, so it
is measured on the *elaborated* term rather than on the text. `src/CheckAxioms.lean` emits, per
theorem, the constants its statement mentions and two structural flags, and the check fails on
either flag:

* **`trivial`** — the statement is `True`, or `a = b` / `a ≤ b` / `a < b` / `a ↔ a` with
  syntactically equal sides, or a `∧` of such;
* **`projection`** — the proof term is `fun … => <binder>`, i.e. the conclusion *is* one of the
  hypotheses. (`theorem t (h : P) : P := h` and `… := by exact h` both elaborate to this.)

The third signal — "mentions no constant this project declares" — is reported as a **warning, not
a failure**, and deliberately: it has real false positives, because `cross_mul_lt` states a genuine
fact about `Int` and mentions nothing of ours. A gate that fails on those gets turned off. The 11
tautologies this found — `zero_cond_burn_v1_sound`, `range_check_64_sound`,
`boolean_output_must_be_constrained`, `practical_anonymity_bound`, `base_ops_are_congruent`,
`walletConstruct_idempotent`, and five `: True` theorems in `Gossip`, `CeilingDerivation` and
`GeneralTheorem` — have none of that ambiguity, and all 11 are deleted.

**Why the arity details matter.** The first version of `isTrivialProp` matched `LE.le` at the wrong
arity and silently never fired, so `x ≤ x` survived it; `restatesHypothesis` compared the conclusion
against the binder *types*, which differ in de Bruijn depth from the conclusion even when they are
the same proposition, so `(cv : Int) (h : cv = 0) : cv = 0` survived that too. Both were found by
planting the shapes and watching the check *not* fire.

There also used to be an "expected output" block here, quoted from `lean --run src/Main.lean`,
reporting `Proved theorems: ~40`, `Axioms: ~43` and `HAZOP findings: 15`. Those numbers were
hardcoded in `Main.lean` and were wrong in every case (the counts are 222 theorem/lemma
declarations, of which 128 are at budget 0, and 9 assumptions). They are gone from
`Main.lean`: a summary that is typed by hand is a claim, not a measurement, and this file was
quoting it as evidence.

## Project Structure

```
proofs/lean/
├── lean-toolchain              # Lean 4.12.0
├── lakefile.lean               # Build configuration — requires Mathlib v4.12.0
├── lake-manifest.json          # Dependency manifest (mathlib, batteries, aesop, Qq, …)
├── README.md                   # This file
└── src/
    ├── Main.lean               # Verification suite entry point
    ├── Examples.lean           # Interactive examples (lean --run via lake run)
    └── DarkFi/
        ├── Field.lean          # Pallas field arithmetic foundations
        ├── Gadgets.lean        # LessThanOrEqual, IsEqual, IsNotEqual soundness
        ├── Soundness.lean      # Cross-multiplication theorems
        ├── Arithmetic.lean     # base_add, base_mul, base_sub, base_div correctness
        ├── Comparison.lean     # BoolCheck, CondSelect, ZeroCond, LessThanStrict
        ├── CrossCutting.lean   # Value conservation, nullifier determinism, signature binding
        ├── HashOps.lean        # Merkle root, Poseidon hash, SMT membership
        ├── ECOps.lean          # EC operations, Orchard-class vulnerability detection
        ├── SupplyChain.lean    # Multi-block cumulative supply induction
        ├── HAZOP.lean          # HAZOP risk matrix and cross-cutting patterns
        ├── Semantics/            # The ρ-calculus: syntax, congruence, substitution, transitions
        │   ├── Proc.lean         # Processes with names folded in; quote/eval
        │   ├── Congruence.lean   # Structural congruence, and the freshness it needs
        │   ├── Substitution.lean # The binding convention, and the substitution it defines
        │   ├── LTS.lean          # Transitions, barbs, and strong and weak bisimulation
        │   └── Ledger.lean       # The write set, the overlay diff, and disjoint calls commuting
        ├── Combinatorial/      # L1/L2 combinatorial state space
        │   ├── StateSpace.lean      # L1 state space types
        │   ├── Transitions.lean     # State transition combinatorics
        │   ├── ComplexityJump.lean  # L2→L1 complexity jump theorems
        │   ├── CompositionBounds.lean # O-cap composition bounds
        │   ├── CeilingDerivation.lean # Derivation of the L1 complexity ceiling
        │   ├── Combinations.lean    # Growth in the number of contracts
        │   ├── Limits.lean          # L1 practical limits
        │   ├── GeneralTheorem.lean  # Halo2 L1 contract complexity limits
        │   └── NullifierStorage.lean # Nullifier storage faithfulness
        ├── Capability/         # ρ-calculus type system
        │   ├── Types.lean      # 17 primitive types with barb sets
        │   ├── Pareto.lean     # Pareto-efficiency (all types pairwise distinct)
        │   ├── Distinction.lean # 10 non-unifiable pairs
        │   ├── Composition.lean # 12 capability type constructions
        │   ├── Wallet.lean     # walletConstruct soundness/completeness
        │   └── Inversion.lean  # Authorization Inversion Theorem
        ├── Circuits/           # constrain_instance manual audit (axioms)
        │   ├── Token.lean
        │   ├── Bridge.lean
        │   ├── Exchange.lean
        │   └── All.lean
        └── HAZOP/              # Structured audit findings
            ├── Critical.lean   # Risk ≥ 60
            ├── High.lean       # Risk 40-59
            └── Elevated.lean   # Risk 30-39
```

## Contributing

To add a new primitive type:
1. Define it in `Capability/Types.lean` with its barb set
2. Add it to `allPrimitiveTypes`
3. `lake build` — Pareto-efficiency is automatically checked for all new pairs

To add a new capability type:
1. Define `Resource` (required barbs) and `Action` in `Capability/Composition.lean`
2. Construct `CapabilityType` with `primitives` list and `coversBarbs` proof
3. Add an `IO.println` check to the `#eval do` block
4. `lake build` — the `coversBarbs` proof is checked by the Lean4 kernel

To add a new ZK opcode proof:
1. Model the constraint equations in `Gadgets.lean` or `Comparison.lean`
2. State and prove the soundness theorem
3. `lake build`

All Lean4 sources are AGPL-3.0-only.
