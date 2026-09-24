# Lean4 Formal Verification — DarkWow Type System & ZK Gadgets

Formal specification and verification of the DarkWow cryptographic type system
and zkVM opcode gadgets using Lean 4 (v4.12.0).

**Mathlib is a dependency**, pinned in `lakefile.lean` to `v4.12.0` and resolved in
`lake-manifest.json`. (This file used to claim "Zero Mathlib dependencies — all proofs use core
Lean 4"; `Field.lean`, `Arithmetic.lean`, `Gadgets.lean`, `CrossCutting.lean` and the
capability modules all `import Mathlib`, and several proofs cite mathlib lemmas by name.)

**To verify everything:**

```bash
scripts/lean-build.sh build DarkFi Transcribed CircuitIndex
```

Note the target. A bare `lake build` builds "the default facet of the root package", which for
this package is **nothing at all** — it exits 0 without compiling a single module. This README
said `lake build` for a long time, and the CI gate in `scripts/run-all-tests.sh` called it, so
the verification that was supposed to be happening was not. `lake build DarkFi` type-checks the
proofs.

**There are three libraries, not two, and the third arrived on 2026-09-24.** `Transcribed` is the
circuit transcription and `CircuitIndex` is the `(r, s) -> circuit` index built on top of it
(`scripts/gen_circuit_index.py`), each a `lean_lib` of its own so that neither rides on the default
path of a `DarkFi` build. The command above named only the first two for a few hours after
`CircuitIndex` landed, which is the failure this README has recorded twice already in other forms:
**a documented verification command that does not reach the module is a verification nobody ran.**

**And run it through `scripts/lean-build.sh`, never `lake` directly.** The wrapper adds the cgroup
memory ceiling whose absence froze this host on 2026-09-24 — the `LEAN_NUM_THREADS=4` this README
used to document bounds thread count and not memory. See "Never call `lake` directly" under
Verification below, which also gives the axiom-boundary command.

## What the build currently says

Run `scripts/lean-build.sh build DarkFi Transcribed` before trusting anything below. As of 2026-09-20 it completes with **no
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
| `Composition.lean` | 14 concrete capability types (native token transfer, DAO vote, tender bid, coinbase claim, purse balance/withdraw/deposit, identity credential, box take, multisig approval, attestation, bridge deposit/withdraw, oracle operator) | `barbPreservation` (induction over primitives list), `coversBarbs` for each type |
| `Wallet.lean` | Wallet capability construction function | `walletConstruct_sound`, `_complete`, `_preservesPrimitives`, `_deterministic`, `_rejects_emptyPrimitives` (`_idempotent` was `x = x` and is deleted) |
| `KeyScope.lean` | `deriveInstance`, and the §7.3 scope restriction | `scopeRestriction`, `scopeRestriction_is_false_for_unscopedDerive` (the falsifier) |
| `Selection.lean` | §6.2's coverage predicate and the spent-capability exclusion | the coverage monotonicity laws, and the refutation §6.2 asks for |
| `WritePath.lean` | §6.1's `f(SelectedCapabilities, Action, Params, Secrets, Seed)` | `construct_sound` (from `walletConstruct_sound`), `construct_deterministic`, `params_are_not_read`, `nullifier_completeness` |
| `WalletState.lean` | §1's rescan fold, §6.5's provisional layer, the spend lifecycle | `insertOrIgnore_idempotent`, `scan_order_is_load_bearing`, `provisional_never_mutates_confirmed`, `spent_is_entered_only_from_processing`, `repair_restores_consistency` |
| `Axioms.lean` | **The assumption boundary** — the only file permitted to contain an `axiom` or a value-less `opaque`. Every assumption carries four fields, and `script/check_lean_axioms.py` enforces it |
| `Inversion.lean` | The circuit bridge as a *hypothesis* (`CircuitDerivable`) and `capabilityType_of_circuitDerivable` (one-directional). `authorizationInversion_TypeLevel` (bidirectional: type exists iff barbs covered — a claim about barb coverage, **not** about ZK proof systems), `verifierLearnsOnlyRequiredBarbs`. The former `circuitSoundnessBridge` axiom asserted that every capability type exists and is deleted |

**Capability types defined (14):** `nativeTokenTransferType`, `nativeTokenCoinbaseType`,
`daoVoteType`, `tenderBidType`, `purseBalanceType`, `purseWithdrawType`, `purseDepositType`,
`identityCredentialType`, `boxCapType`, `multisigApprovalType`, `attestationType`,
`bridgeDepositType`, `bridgeWithdrawType`, `oracleOperatorType`.

The count is **measured, not asserted**: `contrib/capability_type_diff.sh` extracts the
`CapabilityType` defs from `Composition.lean`, the positive `wallet_construct` calls from
`src/sdk/src/capability.rs`'s test module and the capability tables from the Python model, joins
them on `(resource, action)` and fails on any pair whose primitive sets disagree. This file said
**12** for as long as there were 14 — `purseDepositType` and `oracleOperatorType` were added to
`Composition.lean` and the two lists here were not — and nothing could see the drift, which is what
the gate is for. It also reports the four types Rust cannot construct at all and the one it can and
does not test; the numbers and the reasons are in the gate's output and in the Honest Scope entry
below.

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
| `base_lt_strict_sound` | 0x57 | The returned bit is 1 exactly when `a < b` — the opcode's soundness theorem, which did not exist until 2026-09-24 (`OBL-Z20`) |
| `not_base_correct` | 0x56 | Boolean negation of a boolean operand, resting on `boolcheck_sound` rather than restating it (`OBL-Z21`) |
| `chunkSum_lt_pow_of_short_last` | 0x50 | A value the deployed **windowed** range check accepts is `< 2^NUM_BITS` as an integer — the short check on the last chunk is what makes the bound exact |
| `range_check_64_gives_bounded_bits` | 0x50 | The deployed `<10, 64>` instance hands the division bridge its `(bits, bits < 2^64, a = ↑bits)` triple, so `FieldLessThanOrEqual`'s bounds are obtainable rather than assumed |
| `operand_products_fit_the_offset_window` | 0x55 | 64-bit-backed operands have `a·b − c·d` inside the `2^253` window the offset's check supplies |

### Part 3: Cross-Cutting & Arithmetic

| Module | Key Theorems |
|--------|-------------|
| `Field.lean` | `cross_mul_lt` (integer cross-multiplication soundness), `wraparound_safe` (bounded inputs preserve ordering) |
| `CrossCutting.lean` | `pedersen_sum_equality_implies_value_equality` (a `congrArg`, named for what it proves), `value_conservation_no_wraparound` (16×64-bit values fit in Pallas field) |
| `HashOps.lean` | Model scaffolding only — `MerklePath`, `SMTMembershipGadget`, `PoseidonHashGadget` and friends. Its former "theorems" `merkle_root_deterministic` (`x = x`) and `merkle_inclusion_soundness` (its own hypothesis, plus a `root = root` hypothesis) are deleted; `merkle_root_change_detection` is an assumption in `Axioms.lean` |
| `ECOps.lean` | `ec_add_inputs_must_be_distinct`, and model scaffolding (`ECMulGadget`, `ECAddGadget`). `fixed_base_mul_uses_constant` and `variable_base_mul_is_prover_chosen` are theorems in `ECOps.lean`; `pedersen_commitment_binding` was a tautology declared as an axiom and is **deleted**, not re-proved — re-proving it would have produced a budget-0 theorem named for Pedersen binding whose statement says nothing about a commitment |
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
| Cryptographic | 4 | `poseidon_hash_output` (value-less `opaque`), `poseidon_collision_resistance`, `aead_open` (value-less `opaque`), `aead_key_committing` |
| Arithmetic | 1 | `pallasPrime` |
| Emission policy (free parameters) | 1 | `coinbase_blind` |
| **Total** | **6** | |

**Two rows left this table on 2026-09-24, and they left for different reasons — which is the
distinction to keep.** `reward_monotone` was **discharged by proof**: it is
`Emission.reward_nonincreasing` now, and this file's row was stale from the moment that landed.
`NoFreeInstances` was **deleted**, its premise relocated into `Capability.Inversion.CircuitDerivable`
as a computation over a circuit's transcribed data; **the rule now proved there is weaker than the
axiom's name**, and `Axioms.lean`'s DISCHARGED entry and `OBL-T7` both say by how much (the checker
admits a `redundant` exposure and a `declared-free` one, and the axiom's name admitted neither). So
six is the count and it is *not* six of the same kind of thing as the four above it.

The AEAD pair was added 2026-09-24, and it was added *by removal*: `Net/Receive.lean`'s
`decrypt` had been `if k = n.recipient then some … else none`, so its `decrypt_sound` was a fact
about that branch — budget 0, because the definition assumed everything — while `wallet.md` §2.1
cited it as the receive path's soundness. The key comparison is now an opening of an opaque
ciphertext and the property is assumed, so the count rose by two and the receive path's soundness
appears in a budget for the first time. `Axioms.aead_key_committing` is the unconditional
strengthening of a ~2⁻¹²⁸ computational guarantee, the same species as
`poseidon_collision_resistance`, and the honest-scope section below records it beside that one.

This table is the one that produced the wrong number, so its history is worth keeping. Until 2026-09-24 it
listed **23** members across six classes — a Cryptographic six that still named `commitment_binding`,
`nullifier_binding`, `merkle_root_change_detection` and `purseNullifier_nonce_injective` (now theorems, or
discharged, in `HashOps.lean` and `Capability/Purse.lean`), an Arithmetic row for
`Arithmetic.base_div_mul_cancel` (a theorem since `BaseDiv.lean:56`), a Model-to-VM row for two `ECOps`
declarations (theorems at `ECOps.lean:127` and `:134`), and a Pallas row of eight names whose seven
postulates `DarkFi/Pedersen.lean` replaced with real curve theory. And it never listed `pallasPrime` at all,
which is the one arithmetic fact the whole layer still rests on. Five documents under `doc/src/` had copied
the total, so one stale table became five wrong statements about how much this layer assumes.

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

The count is now **6**, and the honest framing is not "these are cheap" but: **two of them have no
consumer at all.** Measured, not asserted — the collector's per-theorem axiom sets give the
consumer count directly, and these are the figures the table `script/check_lean_axioms.py` prints
today:

| assumption | consumers |
|---|---|
| `pallasPrime` | 14 (`Pedersen.*`, and everything downstream of the curve being a field) |
| `coinbase_blind` | 7 (`cumulative_auditable`, `cumulative_commit_theorem`, `no_hidden_inflation`, …) |
| `HashOps.poseidon_collision_resistance` | 6 (`commitment_binding`, `nullifier_binding`, `smtCrh_injective`, …) |
| `HashOps.poseidon_hash_output` | **none** |
| `reward_monotone` | **none** *(not an assumption any more — a theorem since 2026-09-24)* |

Three rows left this table on 2026-09-24 by ceasing to be assumptions: `Arithmetic.base_div_mul_cancel`
(a theorem in `BaseDiv.lean`), and `ECOps.fixed_base_mul_uses_constant` and
`variable_base_mul_is_prover_chosen` (theorems in `ECOps.lean`). **Two more left it the same day, and for
different reasons — the distinction is the point of keeping this paragraph.** `reward_monotone` was
discharged *by proof*. `NoFreeInstances` was **deleted**, its premise relocated into
`Capability.Inversion.CircuitDerivable` as a computation over transcribed data; its former consumer
`capabilityType_of_circuitDerivable` now reads budget **0**, where it read 1 — and that 1 was never the
proof using it, it was a `structure`-projection charge (HIGH-16). **The rule now supplied is weaker than
the axiom's name**, which the honest-scope bullet below and `OBL-T7` both state. And one count moved for a reason worth
recording: `pallasPrime` read **19** before that date and reads **14** after it, and the difference is
not the tree growing — it is that `Axioms.lean` no longer declares a **global**
`instance : Fact (Nat.Prime PALLAS_MODULUS)`. Five declarations were reaching the assumption by
instance *resolution* rather than through their proofs, because `NatPow` preferred the `Field` path over
the unconditional `ZMod.commRing`; they take the fact locally now, where they need it, and the budget-2
count fell 14 → 9. (The `14` an earlier version of this table carried was not this measurement — it
predates it, and its provenance is not recorded.)

`NoFreeInstances` used to be the register's example of an assumption "consumed by nothing". It now
has a consumer, which is what §3 of the rewrite was for: `capabilityType_of_circuitDerivable` takes
`CircuitDerivable r s` as a *hypothesis* rather than resting on a vacuous axiom, and that hypothesis
is `NoFreeInstances` in the type system's vocabulary. A theorem that names its own gap is one a
reader can act on; a `: Prop` axiom with a vacuous antecedent is not.

**And it has since left the assumptions altogether.** Deleted 2026-09-24, with its premise relocated
into `CircuitDerivable` as a computation over transcribed data — so what has a consumer is that
*field*, and the declaration the register used to point at no longer exists. So the contrast this
paragraph draws survives the deletion and gets sharper: a gap that is *named* can be closed by
supplying data, and a `: Prop` axiom that nothing reads can sit there for months looking answered.
The two with **no** consumer cannot fail — their falsity would be undetectable here, because nothing
depends on them. `DarkFi.HAZOP.Elevated` records each one and collects them as
`silentAxiomFailures`: that list is the case for discharging them.

## What Is NOT Proved (Honest Scope)

- **The range check is modelled from its `Int` content outward, and four things are outside it.**
  `Comparison.lean` now transcribes `src/zk/gadget/native_range_check.rs` — the windowed running-sum
  decomposition, its short check on the last chunk, and the bound those imply — which is the content
  `Comparison.lean` itself had recorded as missing when it deleted `range_check_64_sound` for
  assuming its own conclusion. What that model does **not** cover, stated here rather than left to
  the module note: the gates constrain the running sum over `ZMod p` and the transcription is of the
  *integer* equation the chip's comment writes, with the step between them being
  `BaseDivGadget.zmod_eq_int_of_bounded` rather than anything in that section; the `k_values_table`
  lookup is taken as its content (a chunk is `< 2^w`), so the table's own construction is not
  modelled; `decompose_value`'s bit plumbing (`to_le_bits`, `chunks_exact`, the padding) is not
  transcribed, which is why `exists_chunkSum_eq` proves decomposability arithmetically instead of
  from that construction; and **nothing in these theorems reads a `.zk` file**, so the passage from a
  circuit's `range_check(64, ·)` call to a chunk list of that shape is still the transcription
  `Circuits/InstanceDerivation.lean` records for `OBL-T7`. What is proved is that *given* the
  witness, the bound follows — one level of statement further in than the arithmetic, and no
  further.

  **Corrected 2026-09-24: the last of those four moved one step further out, and the first sentence
  above is now three things rather than four.** That final clause read "*given* the witness, the
  bound follows". `Circuits/InstanceDerivation.lean`'s `Satisfies` gained a range-check conjunct —
  `∀ w e, Stmt.rangeCheck w e ∈ cs → eval opVal v e < 2 ^ w` — and
  `rangeCheck_operand_is_bounded` / `rangeCheck_64_bounds_the_operand` eliminate from it, so the
  operand bound that `BaseDivGadget.less_than_or_equal_integer_reading` consumes is now **derived
  from the circuit's statement list** instead of being supplied as a hypothesis. `Transcribed.lean`
  is what supplies those lists — so what remains is exactly the `(r, s) ↦ a circuit` mapping and
  nothing else, which is the residue `OBL-T7` records; that is why `OBL-Z12` stays `PARTLY` rather
  than closing, since only one row should claim a single missing step. The conjunct's cost is
  recorded where it was paid: a stronger `Satisfies` is a weaker `soundness`, and for a valuation of
  a real circuit it is no extra obligation, because the chip enforces the bound
  (`Comparison.range_check_64_is_bounded`). Measured after the change: 705 theorems, every budget
  matching its axiom set, the two new ones at budget 0 as annotated, and no theorem a tautology.
- **Nothing below is a claim about a build you have not run.** `scripts/lean-build.sh build DarkFi Transcribed` completes clean
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

  **The replication equations are omitted, and the reason the tree gave for adding them was measured
  and does not hold.** `Congruence.lean`'s note expected `!0 ≡ 0` and `!(P | Q) ≡ !P | !Q` to make
  `!!P ≡ !P` derivable by turning `!!P` into `!P | !P` and absorbing a copy; no derivation does. Every
  route through the merge law stops at the *fixpoint* equation `!!P ≡ !P | !!P`, which `rep_unfold` at
  `!P` already gives (`rep_rep_fixpoint`, no new rule), and "absorb one copy" is `!P | !P ≡ !P`, whose
  reverse needs `P | P ≡ P` — which this calculus does not have, and which no **observable** here can
  supply either: `Barb`, `CanBarb`, `CanStep` and `ActionFree` are all idempotent under duplicating a
  parallel component (`barb_par_self`, `canBarb_par_self`, `canStep_par_self`, `actionFree_par_self`).
  What is unobservable is the collapse itself — `!!P` and `!P` agree on every observable the layer
  defines — so the observation level is *ahead* of the equation level rather than behind it. Register
  row `OBL-T14`. The row does not claim `!!P ≡ !P` is underivable; it claims this route does not exist.

  The substitution layer's own side condition is mechanized too, and it is *not* freeness: `NoSub` is
  subterm-freeness defined in lockstep with `subst`'s recursion — so `inp` visits its channel and not its
  binder, and `nu` does not visit its binder at all — and under it the identity law
  `subst_eq_self_of_noSub` holds for every `y`, where the tempting `FreshFree`-based form is **false**
  (`subst_of_freshFree_is_false`) and the converse `FreshFree → NoSub` is refuted on the same witness
  (`noSub_not_of_freshFree`). Register row `OBL-T13`. What remains of the α-unit is the other three
  parts — the label's channel, the constructor, the invariance re-proofs — none of which is a missing
  lemma.

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

- **The wallet's write path is modelled at the *type* level, and the empty layer below it is named
  rather than implied.** `KeyScope.lean`, `Selection.lean`, `WritePath.lean` and `WalletState.lean`
  landed 2026-09-24 and §7.8's three obligations are now discharged **by name**. Two of them
  (`construct_sound`, `construct_deterministic`) were declared nowhere in the tree; the third is
  subtler and is the reason this entry exists — `nullifier_completeness` *did* exist, in
  `Exercise.lean:115`, where it is about a **consume**, and §7.8 asks it of a **transaction**, which
  is the object §6.3 step 4 is about. A reader who grepped the name and found it would have concluded
  the obligation was met. §7.3's scope restriction, §6.2's coverage predicate with its spent-capability
  exclusion, §1's rescan fold and §6.5's provisional layer all have models, and the spend lifecycle has
  a theorem for the direction that matters (`spent_is_entered_only_from_processing`). What none of them
  reaches, stated here so that a reader does not infer it from a theorem name:

  * **`construct_sound` is derived, and what it derives from is a composition rule, not a proof
    system.** It is proved *from* `walletConstruct_sound`, which unfolds `walletConstruct` and returns
    the `if`'s own condition — the reason F3 recorded it as content-free. What the pair establishes is
    that a capability whose primitives cover its resource's barbs constructs, and that the write path
    cannot construct one that does not. The ZK half — that a circuit *inhabits* the type — is the
    hypothesis `CircuitDerivable` in `Inversion.lean`, and it is an assumption, not a theorem.
  * **`deriveInstance` is a function on `Nat`s.** The real `derive_instance(secret, contract_id,
    instance)` is a hash; what `KeyScope.lean` proves is the *discipline* — a key derived for one
    instance is not the key for another — not anything about the curve.
  * **`construct_deterministic` is determinism, not byte-identity.** Two calls agreeing on the
    selection and the `Seed` agree, whatever else they were given. §1's *"byte-identical state"* and
    §0.1.5's purity rules are **not** mechanized: `WritePath.Transaction` and `WalletState`'s state are
    structures of `Nat`s that are not encoded into any of `Wire.lean`'s schemas, so there is no theorem
    here about bytes, and none about the Rust or Python writers agreeing with this model.
  * **Nothing in these four modules reads a `wallet_db`, a `.zk` file, or the Rust wallet.** §6.3's
    steps 5–7 (encoding, signature, fee) are outside them; `WalletState`'s Merkle root is a `Nat`
    field rather than a computed tree, which is why §6.4.0's obligation is stated against
    `PerContractTree.findPos` instead; and the selection predicate is stated over a capability list,
    not over what discovery finds.

  **And the correspondence the type system's Rust side claims is now measured, with the gap
  registered rather than closed.** `src/sdk/src/capability.rs:509-510` says *"Every construction that
  is proved in Lean4 must also succeed here"*. `contrib/capability_type_diff.sh` (wired into
  `scripts/run-all-tests.sh`) extracts the `CapabilityType` defs, that file's positive
  `wallet_construct` calls and the Python model's tables, and diffs them: **9 of the 14** are
  constructed in Rust, **4 cannot be** — `tenderBidType`, `bridgeDepositType`, `bridgeWithdrawType`
  and `oracleOperatorType` name `dleqProof`, `bridgeAddress`, `chainDepositProof` and
  `bridgeCapNullifier`, and `Primitive` has no variant carrying those barbs, so no input can build
  them — and **1 is untested** (`purseDepositType`, which Rust could construct). By decision that is
  gated and registered, not fixed in code: the barb alphabet is held (`OBL-T9`). Two further facts the
  gate reports and this file repeats rather than smooths: the Python model states a table for **1 of
  the 14**, and for that one pair it requires `prove-inclusion` where Lean and Rust require six barbs —
  the composition covers it either way, so nothing is unconstructible, but the three implementations do
  not state the same requirement. A register row for the whole finding is owed; the rows beside it
  (`OBL-T14`–`T16`) are a peer session's.
- **Halo2 constraint system semantics are not modeled.** We prove properties of the
  mathematical functions the opcodes implement, not that the Halo2 gate/region/
  copy-constraint system correctly implements those functions.
- **Circuit-to-Lean correspondence is not mechanized, and one half of it now is.** The `Circuits/`
  directory documents a manual audit of the `.zk` files, not machine-verified extraction —
  `Circuits/InstanceDerivation.lean` is that directory's first module with actual content, and it models
  the *property* rather than the extraction. `NoFreeInstance` is now **defined** over a circuit's
  statement list, with `soundness` (two satisfying valuations agreeing on the held inputs give the same
  public vector) and its converse (a bare-witness instance is free, so the predicate is falsifiable rather
  than true by construction), plus a worked circuit closed by `decide` and its negative control. Two
  things it taught that a reader should know: the checking script distinguishes a value that is
  *derivable* from one that is *determined* (a bare witness is the former and not the latter, because
  `constrain_equal_base(w, X)` re-exposes a variable the prover held), and a soundness proof needs the
  valuations to **satisfy** the circuit — a fact about the valuation that a source analysis has no reason
  to state. `Axioms.NoFreeInstances (r : Resource) (s : Action)` **was** uninterpreted and unconsumed
  until 2026-09-24, and is now **deleted**: the function `(r, s) ↦ the circuit source` is a generated,
  freshness-gated module (`CircuitIndex.lean`), `Capability.Inversion.CircuitDerivable` carries the data
  instead of a predicate, and a Lean term still cannot read a `.zk` file — which is why the source is
  *generated* rather than read. **What closed it is a strength change and not a proof**, and that
  sentence is the one to carry away: the rule supplied at the twelve pairs is the checker's, which is
  weaker than the axiom's name.
  **The transcription that bridge needs now exists, and its verdicts are the tree's most surprising
  number.** `Transcribed.lean` is generated from the `.zk` sources by
  `scripts/gen_circuit_transcription.py` and freshness-gated (`--check`, wired in `run-all-tests.sh`), so
  it is machine-transcribed rather than hand-typed: **181 circuits, 2747 statements, one `decide`
  theorem each**, the kernel closing every verdict. Under the model's rule **170 of 181 refute the
  property**, and the generator decomposes those 170 by asking the checker what it made of the same
  exposure — 155 are the checker's `redundant` class (pinned by another exposed determination; the
  model's rule is sequential and does not follow it), 11 its `declared-free`, 3 its own failures
  (`OBL-Z16`), and 1 the model's declared-constant boundary. So the model **refines** the checker rather
  than contradicting it, and `NoFreeInstances`' *name* is a strict reading of this tree rather than a
  description of it — the property the tree enforces is the checker's four-verdict rule. **The bridge is
  now in the tree**: `(r, s) ↦ a circuit` is `CircuitIndex.lean`, generated and gated, carrying one
  inhabitant of `Capability.Inversion.CircuitDerivable` per pair for the twelve pairs this layer
  instantiates. **The figures in this paragraph are the transcription's older readings** — it is
  **178 circuits over 2677 statements** now (not 181/2747), **167** refute the strict property (not
  170), decomposed **154 `redundant` / 12 `declared-free` / 1** the model's constant boundary; the
  three that were the checker's own `OBL-Z16` failures are gone with the sites that caused them, and
  the checker itself now exits 0.
- **The consensus state core is only partly modelled here, and what is missing is named rather than
  implied.** Five mechanisms have models now, all at **budget 0**: the block-level Pedersen mass-balance
  rule (`Consensus/MassBalance.lean`, ten theorems, transcribed from
  `contrib/model/proof_of_token_balance.py`), the nullifier lifecycle
  (`Consensus/NullifierLifecycle.lean`, seven, which feeds the existing maturity gate from the store's
  recorded height and refuses a second spend), the chain-level commitment set
  (`Consensus/CommitmentSet.lean`, seven — the *prune*, and the proof that it neither creates nor
  destroys a maturity refusal), and the coinbase split
  (`Consensus/CoinbaseSplit.lean`, sixteen — the five equations three enforcement sites impose, with the
  finding that of the five checks the **uncle-note sum** is the load-bearing one and the three value
  checks are over-determined). **And a fifth, which is a composition of the first, the fourth and a
  third** — the two trees that `OBL-C45` says are reconciled nowhere:
  `Consensus/SupplyReconciliation.lean` (seven laws, **all at budget 0**) states that a block's net
  creation of commitment value *is* the emission schedule's value at that height, and that summed over a
  chain it is `Σ expected_reward(H)` — with the fee leg **proved load-bearing** rather than decorative
  (`Balanced` alone leaves the net creation at `issuance − Σ fees`, because the fees are parked rather
  than destroyed, and the witness that fails the identity has every value check in both models passing),
  and with the bridge the two fee models need — `FeeCollect`'s rule counts FeeV3 *calls*, `MassBalance`
  carries the fee *amounts*, and no code check ties them — taken as an explicit hypothesis and named as
  that unit's residue. **Nothing in the code checks this property**: no Rust and no script reads the
  contracts tree against the supply chain, so the module is a statement a later reconciliation would be
  a check *of*. **Partly** modelled: the **validity predicates** — `Consensus/BlockTimestamp.lean`
  states the median-of-11 timestamp rule and its interface (four laws; the
  security bound the rule exists for is stated in its note and *not* proved, and nothing checks the rule
  on a concrete window because neither of Mathlib's sorts reduces in the kernel), and
  `Consensus/UncleRules.lean` states the uncle depth window, the re-derived pin and the alignment guard
  (fifteen laws, eleven at budget 0), and `Consensus/BlockHeader.lean` the two-stage proof-of-work rule
  and the Monero anchor rule (ten laws, all at budget 0), and `Consensus/FeeCollect.lean` the fee-collect
  decision table — proved equivalent to the two-clause rule it presents (five laws, four at budget 1
  because `cases`/`simp` over `Bool` equalities reaches `Classical.choice` where an explicit `rw` does
  not), and `Consensus/CoinbaseStructure.lean` the coinbase's structural rules — the four opening checks,
  proved to collapse to one shape, with the payoff that the mass-balance rule's blind spot has size one
  and the refutation showing what dropping the single-call rule costs (thirteen laws, **all at budget
  0**). **`src/linear/src/validation.rs` is now modelled except for header continuity**, which is a
  deliberate omission rather than a gap: `height == current + 1` and `previous == prev` are two
  equalities with nothing to state, and a unit for them would have only completeness as its
  justification. As are the parts of these mechanisms their models stop
  short of — including, in the coinbase split, the *per-note* key binding, which the model's sum equation
  is necessary but not sufficient for, and in the uncle rules the RandomX verification itself and the
  dedup key's blake3 form, for which the model substitutes header distinctness and says so. The agreed
  shape of each, with its Rust, its Python
  specification and its non-vacuity witness, is `doc/src/arch/consensus-core-map.md` — and a unit that
  departs from it amends it in the same commit, which all six did (the fifth by *adding* a mechanism the
  map did not have). Separately: `Semantics/Ledger.lean`
  proves that disjoint calls commute without saying what a write set *is*, which is how `OBL-C100` was
  found; that gap is still open and is not part of the map.
- **The tautology arm's reach, stated because completeness is now relied on.** `script/check_lean_axioms.py`
  refuses a statement that is true of nothing, and as of 2026-09-24 it also refuses one that merely
  **reduces** to a trivial statement and one whose proof ignores every explicit binder. The second
  tightening was not cosmetic: three theorems in `Capability/Prover.lean` were `∀ …, True` in disguise —
  `bindable`'s catch-all sends the intrinsic witness sources to `True`, so `bindable txCommitment nf pf`
  *is* `True`, and the syntactic test saw an unreduced `def` application. All three were consumer-less and
  were deleted, with their claim restated once in a form that has content. Measured across all 454
  theorems, the tightened arm catches exactly those three and no others. **What it still does not
  reach**: a statement whose head is an inductive — `(∀ …, True) ∧ (∀ …, True)` written literally is not
  reduced further — and the two *soft* signals beside it (a statement mentioning no constant of this
  project, and a theorem with some binder unused) are soft because they have real false positives, which
  the arm's own docstring gives as the test for which signals may block.
- **Poseidon is an opaque function, not the sponge.** `poseidon_hash_output` is a value-less
  `opaque`, so nothing about the P128Pow5T3 permutation is proved — not even determinism, which
  is a consequence of its being a function and needs no proof. (This section used to say it was
  defined as `inputs.head?.getOrElse 0 + 1`; that was true of a much older version and has been
  false since.)
- **AEAD opening is opaque too, and the receive path's soundness is assumed rather than built
  in.** `aead_open` has no value, so no cipher, KDF or nonce derivation is modelled, and
  `Axioms.aead_key_committing` asserts that a ciphertext authenticates under at most one key.
  It is the same species of over-statement as the Poseidon entry above: the deployment is
  Sapling DH → `kdf_sapling` → `ChaCha20Poly1305` with the nonce derived from `ephem_public`
  (`src/sdk/src/crypto/note.rs:113-135`), whose guarantee is ~2⁻¹²⁸ *per attempt*, while the axiom
  asserts no such pair exists — false of the real construction by counting, and satisfiable only
  because `aead_open` is opaque and `Int` is countable. The shape is falsifiable
  (`Net.key_committing_is_false_for_leaky_open` exhibits an opening that ignores the key, for
  which the statement is false), so the assumption is load-bearing rather than decorative — but
  the *satisfiable* half is not built: no model in this tree exhibits a tag-based `aead_open`.
  **What changed on 2026-09-24 is where the assumption sits, not how strong it is.** `decrypt`
  used to *be* the property (`if k = n.recipient then some … else none`), so `decrypt_sound` read
  budget 0 and was a fact about a branch; it is now budget 1 and the dependency is visible.
- **Pedersen point addition is also opaque** — `PedersenPoint.add` has no value, so it is not
  `Nat` addition and not curve addition. The group laws
  (`pedersen_add_comm`/`_assoc`/`_identity`) are assumptions about an unmodelled operation, and
  the supply-chain inductions do not use them: they are `rfl`/`simp`-level. This section used
  to say the operation "is implemented as `Nat` addition", which understated the gap in one
  direction (nothing is implemented) and overstated the proofs in the other.
- **The Orchard Merkle CRH is substituted, and the substitution is now two gaps rather than one.**
  The tree's per-level compression is Sinsemilla over `⟨altitude⟩₁₀ ‖ ⟨left⟩₂₅₅ ‖ ⟨right⟩₂₅₅` under
  the domain `"z.cash:Orchard-MerkleCRH"` (`src/sdk/src/crypto/merkle_node.rs:149-171`), and
  `HashOps.sinsemillaCrh` hashes three `Int`s with Poseidon instead. What has changed is that the
  *width structure* is now modelled and half its injectivity is **proved** with no cryptography:
  `merkleCrhMessage` carries the deployed message, `merkleCrhMessage_length` pins it at 520 bits
  (budget 0), and `merkleCrhMessage_injective` proves it injective in altitude and both children on
  the tree's own domain. So the residue is **(a)** the primitive — Poseidon for Sinsemilla — and
  **(b)** the model's hash has no **codomain** bound, which is why the faithful message cannot be
  wired into the fold at all: the fold's intermediate values are CRH outputs, and after one level
  "the children are below `2^255`" is unavailable, where real Sinsemilla lands in `pallas::Base`.
  (b) needs a *new* assumption about `poseidon_hash_output`'s range, and is not taken. Register row
  `OBL-Z6`.
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
- **The emission policy is only partly proved.** `reward` *is* a definition now, transcribed from the
  implementation (`Emission.lean`, from `src/sdk/src/blockchain.rs:1032-1069`), and `reward_tail_floor`
  proves it never falls below `TAIL_REWARD` from genesis on. What is **not** proved is that it never
  *increases*: `Axioms.reward_monotone` assumes that, and `Emission.lean`'s `RewardNonIncreasing` states
  it with no theorem attached. It is not a missing routine — the exponentiation-by-squaring loop
  truncates at every squaring, so the cumulative error in the decay passes the local gap once the
  exponent exceeds about 5.5·10⁴ and no absolutely-bounded sandwich survives the middle range; a proof
  has to compare the errors of *adjacent* exponents, which nearly cancel. Measured non-increasing
  exhaustively over `e ∈ [0, 3·10⁵]` and over 200 000 sampled exponents in `[1, 3.4·10⁷]`, and
  kernel-checked for the first step (`reward_nonincreasing_first_step`) — **checked over a range, not
  proved.** Two of the five ruled-out routes are machine-checked rather than argued
  (`fixedPowDecayGo_step_bound_is_false`, `fpMul_nested_bound_is_false`, each with a witness off by
  one), and a kernel check over a useful range is not available either: a fuel-indexed restatement of
  the loop was tried and abandoned, because a range check costs about 1.8s per 500 blocks and **aborts
  with a kernel stack overflow** between 500 and 2000, against the 3·10⁵ exponents the scan covers.
  `total_supply_theorem` does not reach it: it proves a running total equals the sum of a schedule *for
  every* schedule. `MAX_SUPPLY` and `total_reward_bounded` are deleted rather than
  declared, and `doc/src/arch/genesis.md` no longer presents a supply cap as a Lean result.
- **No verified compiler from `.zk` files.** The ZKAS compiler produces Halo2 circuits; there is
  no formal semantics for the ZKAS language in Lean4.

## Verification

```bash
# From the repository root. EVERY Lean invocation goes through scripts/lean-build.sh — it cd's to
# proofs/lean itself, and it is not a convenience wrapper but the only thing between an unbounded
# elaboration and this machine. Read "Never call `lake` directly" below before using it.

# Type-check the proofs. BOTH targets are required: `DarkFi` is the library, and `Transcribed` is a
# library of its own, deliberately not on `DarkFi`'s path.
scripts/lean-build.sh build DarkFi Transcribed

# The assumption boundary: no sorry/admit, every assumption in Axioms.lean with its four
# fields, every theorem annotated with its budget, no native_decide anywhere, no tautology,
# and every `Recorded in DarkFi.HAZOP.X` citation resolving to an entry that exists.
# It invokes the collector through the guard itself; no `lake` is called here.
python3 script/check_lean_axioms.py

# The axiom table, straight from the compiled environment. `--stream` carries **stdout only** — the
# collector's stderr (its summary line, and any name it cannot resolve) goes to $LEAN_BUILD_LOG — and
# `check_lean_axioms.py` keeps the raw rows in /tmp/check_lean_axioms.collector.out. The split is not
# cosmetic: `--stream` used to merge the two, and because Lean's stdout flushes at 4096 bytes while its
# stderr does not, the summary landed inside a row and renamed it. `supply_chain_invariant` came back as
# `nvariant`, its budget was never checked, and the row count stayed right (measured 2026-09-24; the
# account is in `scripts/lean-build.sh`).
scripts/lean-build.sh --stream env lean --run src/CheckAxioms.lean

# The axiom gate's own detector, exercised: the corruption above, rebuilt from synthetic bytes. Hermetic
# (no Lean, no files) and wired into `scripts/run-all-tests.sh`, because a check that has never been
# shown to fail is a claim rather than a check.
python3 script/check_lean_axioms.py --self-test

# The Orchard-class rule over the 180 .zk circuit sources — a separate boundary.
bash scripts/check-circuit-instance-derivation.sh
```

`script/check_lean_axioms.py` prints a table of theorem → budget → the assumptions each
theorem rests on. Use `--emit-annotations` to write measured budgets back into the sources, and
`--require-collector` to make "the collector could not run" fatal rather than a `SKIP`.

### Never call `lake` directly — the memory ceiling is not optional

`LEAN_NUM_THREADS=4` was this tree's whole guardrail, and it is **not sufficient**. It bounds how many
`lean` processes run at once; it says nothing about how much memory any one of them uses. On
2026-09-24 a `LEAN_NUM_THREADS=4 lake build DarkFi` — the command this section used to document, and
the gate `scripts/run-all-tests.sh` used to run — exhausted this 47 GiB host's memory and froze it,
taking every open window with it. The previous boot's journal ends mid-chatter with **no shutdown
sequence and no OOM-killer line**: swap thrash, not a clean OOM. The failed scope is still visible as
`systemctl --user list-units --type=scope | grep lean`.

The module responsible is `src/Transcribed.lean` — a generated module of `decide` verdicts over the
transcribed circuits, and the single most expensive elaboration in the tree. It was 181 `decide` proofs
over 2747 statements when this paragraph was written; **measured 2026-09-24 by its own freshness gate,
`python3 scripts/gen_circuit_transcription.py --check`, it is 178 circuits / 2677 statements / 11 strictly
holding, 167 failing** — the figures here have moved twice with the sources, so quote them from the gate.
Until that day it was imported by `src/DarkFi.lean`, so it sat on the default path of *every* library
build; it is now
`lean_lib Transcribed`, built only when a gate asks for it.

**But the explosion was fixed, not worked around.** The transcription exceeded 24 GiB in one `lean`
process, and no `.olean` had ever been produced for it — its 181 verdicts were unverified in the only
sense that counts, the kernel having closed none of them. Extracting one arm of the model's `boundWalk`
into `bindAssign` (`Circuits/InstanceDerivation.lean`) removed it: the **whole artefact builds in ~71 s
and 743 MB** as one module, and the sharding tried while the cause was unknown has been withdrawn.

**Which circuits were expensive, and why, is deliberately not asserted here.** An adversarial audit
falsified the first explanation this file carried — "only the eleven circuits whose property *holds*
were affected, because a refuted circuit short-circuits in `List.all`". A refuted circuit still forces
the walk for every statement *before* its first undetermined exposure, and `purse/withdraw` (refuted,
failing at statement 46 of 49) forces a deeper chain than the circuit that was actually measured. The
measured fact is that the extraction removed the blow-up in a controlled comparison; the mechanism is
open, and `bindAssign`'s docstring records what was measured, what was retracted, and what would settle
it. The ceiling below still protects the machine; it is no longer what makes this artefact fit under it.

`scripts/lean-build.sh` therefore bounds **both** axes: the thread cap, and a cgroup `MemoryMax`
(default 16 GiB, with `MemorySwapMax=0`) under which the *build* is killed with an explanation while
the desktop is untouched. It writes full output to `/tmp/lean-build.log` rather than the terminal, and
holds a lock so two Lean lanes cannot run at once. It refuses to run at all if it cannot establish a
ceiling — there is no bypass flag, deliberately.

**The default ceiling is a guardrail, not a measured peak**, and saying so is the point: if a target
legitimately exceeds it, raise `LEAN_MEMORY_MAX` in a commit that records the measured peak. If a
*single* module cannot fit under any ceiling that is safe on this host, the answer is to partition it
across several modules rather than to keep raising the wall.

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
hardcoded in `Main.lean` and were wrong in every case (the counts, measured 2026-09-24, are 362
`theorem`/`lemma` declarations — 357 of them gate-visible, since the gate's scanner does not match
`private` — of which 261 are at budget 0, and 8 assumptions). They are gone from
`Main.lean`: a summary that is typed by hand is a claim, not a measurement, and this file was
quoting it as evidence.

**And the file they were removed from had never run either, which the paragraph above did not
know.** `src/Main.lean` was in no `lean_lib` and no `lean_exe`, so `lake build` never compiled it,
no gate invoked it, and it did not compile at all — so this README quoted an "expected output"
block from a file that had never produced output. Repaired and gated 2026-09-24 (`OBL-T18`): it is
`lean_exe Main` now, `scripts/check-lean-suite.sh` runs it and is wired into
`scripts/run-all-tests.sh`, and every check inside it that used to print its result and exit 0 —
four counterexample scans reporting `Bugs found: N`, eight combinatorial expectations reporting
✓/✗ — throws instead. The HAZOP counts in it are read off `riskMatrix` rather than typed, and the
EC classification reads `ECMulKind.baseIsConstant` rather than carrying its own booleans.

**Re-measured 2026-09-24, later the same day, because the figures above had moved**: 741
`theorem`/`lemma` declarations, **736 gate-visible** (five are `private`), **738
`@[axiom_budget]` annotations** — 736 on the visible theorems and 2 on private ones — of which
**605 are at budget 0**, and the same **8** assumptions. The scanner is the gate's own
(`script/check_lean_axioms.py`'s `qualified_theorems()` over `lean_sources()`), so the two
counts are the same measurement rather than two that look alike; the collector is fed exactly
`sorted(qualified_theorems())`, which is why a green run reconciles at that number and no other.
Where the 741 are, measured in the same pass rather than argued: `Transcribed.lean` 178,
`Capability/` 142, `Semantics/` 121, the top-level `DarkFi/` modules 97, `Consensus/` 94,
`Combinatorial/` 59, `Genesis/` 31, `Circuits/` 11, `Net/` 6, `Fee/` 2. **Nothing in this file
claims a cause for the 357 → 736 difference**: both endpoints were measured and the step between
them was not, so what the earlier scan covered is unknown rather than inferable, and a plausible
story told here would be the same defect as the `#eval` output this section is about. The lesson
is the one line 40 states: the number belongs to the tree, so quote it with the date you ran the
command, or not at all.

## Project Structure

```
proofs/lean/
├── lean-toolchain              # Lean 4.12.0
├── lakefile.lean               # Build configuration — requires Mathlib v4.12.0
├── lake-manifest.json          # Dependency manifest (mathlib, batteries, aesop, Qq, …)
├── README.md                   # This file
└── src/
    ├── DarkFi.lean             # Library root — 57 imports. `lake build DarkFi` is the gate; a bare
    │                           #   `lake build` compiles the default facet and builds nothing.
    ├── Main.lean               # `lean_exe Main`: IO simulation suite, NOT proofs — its checks fail
    │                           #   the run (`scripts/check-lean-suite.sh`, OBL-T18)
    ├── Examples.lean           # `lean --run` examples — not in the library
    ├── CheckAxioms.lean        # `lean --run` collector: the fact base for `@[axiom_budget]`
    ├── Transcribed.lean        # GENERATED (scripts/gen_circuit_transcription.py, freshness-gated):
    │                           #   the account, and every circuit's `List Stmt` + one `decide`
    │                           #   verdict each (178 circuits as of 2026-09-24), as one module. A
    │                           #   library of its own (`lean_lib Transcribed`), NOT on
    │                           #   `lake build DarkFi`'s path
    ├── CircuitIndex.lean       # GENERATED (scripts/gen_circuit_index.py, freshness-gated): the
    │                           #   (r, s) -> circuit index, one `DisclosureRule` theorem per pair
    │                           #   (12) plus the 2 that resolve to no circuit, as data. A library
    │                           #   of its own (`lean_lib CircuitIndex`) because it imports
    │                           #   `Transcribed`
    └── DarkFi/
        ├── Axioms.lean         # THE ASSUMPTION BOUNDARY — the one file allowed `axiom` and
        │                       #   value-less `opaque` (6 live; see "The assumption classes")
        ├── AxiomBudget.lean    # Registers the `@[axiom_budget N]` attribute the gate reads back
        ├── Field.lean          # Pallas field arithmetic foundations
        ├── Gadgets.lean        # LessThanOrEqual, IsEqual, IsNotEqual soundness
        ├── Soundness.lean      # Cross-multiplication theorems
        ├── Arithmetic.lean     # base_add, base_mul, base_sub, base_div correctness
        ├── BaseDiv.lean        # base_div_mul_cancel, discharged (Fermat over ZMod)
        ├── BaseDivGadget.lean  # The base_div exponentiation loop, and its quotient-remainder bound
        ├── Comparison.lean     # BoolCheck, CondSelect, ZeroCond, LessThanStrict
        ├── CrossCutting.lean   # Value conservation, nullifier determinism, signature binding
        ├── ECOps.lean          # EC operations, Orchard-class vulnerability detection
        ├── Emission.lean       # The emission schedule, transcribed from blockchain.rs
        ├── HashOps.lean        # Merkle root, Poseidon hash, SMT membership
        ├── Pedersen.lean       # Pallas as a real curve: group laws + additive homomorphism
        ├── SupplyChain.lean    # Multi-block cumulative supply induction
        ├── HAZOP.lean          # HAZOP risk matrix and cross-cutting patterns
        ├── Semantics/            # The ρ-calculus: syntax, congruence, substitution, transitions
        │   ├── Proc.lean         # Processes with names folded in; quote/eval
        │   ├── Congruence.lean   # Structural congruence, and the freshness it needs
        │   ├── Substitution.lean # The binding convention, and the substitution it defines
        │   ├── LTS.lean          # Transitions, barbs, and strong and weak bisimulation
        │   └── Ledger.lean       # The write set, the overlay diff, and disjoint calls commuting
        ├── Combinatorial/      # L1/L2 combinatorial state space
        │   ├── StateSpace.lean        # L1 state space types — definitions only, no theorems
        │   ├── Transitions.lean       # State transition combinatorics
        │   ├── ComplexityJump.lean    # L2→L1 complexity jump theorems
        │   ├── CompositionBounds.lean # O-cap composition bounds
        │   ├── CeilingDerivation.lean # Derivation of the L1 complexity ceiling
        │   ├── Combinations.lean      # Growth in the number of contracts
        │   ├── Limits.lean            # L1 practical limits
        │   ├── GeneralTheorem.lean    # Halo2 L1 contract complexity limits
        │   └── NullifierStorage.lean  # Nullifier storage faithfulness
        ├── Capability/         # ρ-calculus type system — 23 modules
        │   ├── Types.lean          # 17 primitive types with barb sets (definitions only)
        │   ├── Composition.lean    # `compose`, and 14 capability type constructions
        │   ├── Pareto.lean         # Pareto-efficiency (all primitives pairwise distinct)
        │   ├── Distinction.lean    # The 10 non-unifiable pairs of §8.4
        │   ├── Wallet.lean         # walletConstruct soundness/completeness/determinism
        │   ├── Inversion.lean      # Authorization Inversion Theorem, and `CircuitDerivable`
        │   ├── KeyScope.lean       # `deriveInstance`, and the §7.3 scope restriction
        │   ├── Selection.lean      # §6.2's coverage predicate, and the spent-capability exclusion
        │   ├── WritePath.lean      # §6.1's `f`, over the argument order §6.1 gives
        │   ├── WalletState.lean    # §1's rescan fold, §6.5's provisional layer, spend lifecycle
        │   ├── DerivedChain.lean   # Intermediate-referencing witness DAGs
        │   ├── Exercise.lean       # Single-use consume, nullifier completeness
        │   ├── MultiProof.lean     # Transfer/redeem value conservation across burn+mint
        │   ├── NativeToken.lean    # The coinbase maturity gate
        │   ├── PerContractTree.lean # Zero-seeded contract-tree leaf positions
        │   ├── PromissoryNote.lean # The RevokeV2 nested chain
        │   ├── Prover.lean         # Generic-prover soundness, undeclared-field blocking
        │   ├── PublicInputs.lean   # Public-input order congruence
        │   ├── Purse.lean          # Purse nonce chaining, nullifier injectivity
        │   ├── Value.lean          # Value-denominated capability conservation
        │   ├── Wire.lean           # Manifest wire-schema congruence
        │   ├── Concurrency.lean    # Record of the deleted parallel-composition layer — no theorems
        │   └── Gossip.lean         # Network definitions; its two theorems were `True` and are deleted
        ├── Consensus/          # The consensus state core — ten modules, from the spec and the code
        │   ├── MassBalance.lean          # The block-level Pedersen balance, from the spec
        │   ├── NullifierLifecycle.lean   # The replay gate and the maturity gate, as one state
        │   ├── CommitmentSet.lean        # The maturity window's prune
        │   ├── BlockTimestamp.lean       # The median-of-11 timestamp rule
        │   ├── CoinbaseSplit.lean        # The coinbase split's five equations
        │   ├── UncleRules.lean           # The depth window, the re-derived pin, the alignment guard
        │   ├── BlockHeader.lean          # The two-stage target rule, and the anchor rule
        │   ├── FeeCollect.lean           # The fee-collect decision table
        │   ├── CoinbaseStructure.lean    # The coinbase's structural rules
        │   └── SupplyReconciliation.lean # The two trees, reconciled — a composition
        ├── Circuits/           # constrain_instance: one model, checked against a worked circuit
        │   ├── InstanceDerivation.lean # The statement model, `NoFreeInstance`, and its soundness
        │   ├── Token.lean      # Witness/public-input `structure`s, whose claims are still the
        │   ├── Bridge.lean     #   `-- NOT DECLARED IN LEAN` comments; the transcription now exists
        │   ├── Exchange.lean   #   for every circuit, so what these structures add is the naming
        │   └── All.lean
        ├── Fee/                # Fee-window boundary emission
        │   └── Window.lean
        ├── Genesis/            # Genesis as a pure single-valued relation
        │   └── Ceremony.lean
        ├── Net/                # Frame alignment, and note-decrypt soundness
        │   ├── Framing.lean
        │   └── Receive.lean
        └── HAZOP/              # Structured audit findings — data registers, no theorems
            ├── Critical.lean   # Risk ≥ 60
            ├── High.lean       # Risk 40-59
            └── Elevated.lean   # Risk 30-39
```

## Contributing

To add a new primitive type:
1. Define it in `Capability/Types.lean` with its barb set
2. Add it to `allPrimitiveTypes`
3. `scripts/lean-build.sh build DarkFi Transcribed` — Pareto-efficiency is automatically checked for all new pairs

To add a new capability type:
1. Define `Resource` (required barbs) and `Action` in `Capability/Composition.lean`
2. Construct `CapabilityType` with `primitives` list and `coversBarbs` proof
3. Add an `IO.println` check to the `#eval do` block
4. `scripts/lean-build.sh build DarkFi Transcribed` — the `coversBarbs` proof is checked by the Lean4 kernel

To add a new ZK opcode proof:
1. Model the constraint equations in `Gadgets.lean` or `Comparison.lean`
2. State and prove the soundness theorem
3. `lake build`

All Lean4 sources are AGPL-3.0-only.
