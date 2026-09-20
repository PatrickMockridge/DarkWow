# Verification Obligation Register

> **What this is.** Every property this system needs to hold, stated as a proposition, with where it
> is enforced, whether anything checks it today, and what would prove it. It is the specification
> that the Lean work in `proofs/lean/` is written against — the obligation set is derived from the
> *system*, not from the existing Lean files.

> **What this is not.** It does not reconcile the existing HAZOP corpus. There are ~7 independent
> finding-ID schemes in this repository (`V1`–`V7` means four different things in four documents;
> `C1` means six), >500 raw findings, and only `dev/contracts/safety.md` as a partial ledger.
> Those stay where they are and are *cited* here. Merging them would rewrite many audit documents
> and add nothing to what gets proved.

## Method

The instrument is the one `proofs/lean/src/DarkFi/HAZOP.lean` already defines: five adversarial
perspectives (Alice-defender, Mallory-attacker, Eve-eavesdropper, Sybil-replay, Olivia-insider),
each finding graded exploitability × likelihood = risk (1–100), risk ≥ 30 triggering deeper
verification, with the seven recurring `patternN_*` failure modes. It is applied here over three
surfaces — code, ZK circuits, and the type system — with one guideword added:

> **"What would have to be proved for this to be guaranteed?"**

Recorded as a *proposition*. A finding that cannot be stated as a proposition is not yet a finding.

## ID naming, and why it is a new scheme

Entries are `OBL-C` (code), `OBL-Z` (ZK circuits), `OBL-T` (type system). This is deliberately not
one of the seven existing schemes: those collide with each other, so reusing one would make a
citation ambiguous. Each entry instead names the historical findings it subsumes.

**Severity**: **C** = a violation mints, destroys or steals consensus value; **H** = a violation
breaks a stated safety property; **M** = defence-in-depth or liveness.

---

## The assumption inventory, and the check that keeps it honest

`proofs/lean/src/DarkFi/Axioms.lean` is the only file in `proofs/lean/` permitted to contain an
`axiom` or a value-less `opaque`. What it contains is declared here, and
`script/check_lean_axioms.py` compares the two **in both directions**:

* an assumption in `Axioms.lean` that is not listed below → the build fails (a new assumption
  cannot be sneaked in);
* an assumption listed below that is *not* in `Axioms.lean` → the build fails.

The second direction exists because it was needed. While consolidating the Pedersen boundary, a
block replacement silently deleted `reward_monotone` as well as the declarations it was aiming at,
and **nothing caught it**: the four-field check validates the assumptions that are present, so a
missing one is invisible. The inventory below is what makes absence a failure too.

<!-- assumption-inventory -->

```text
base_div_mul_cancel
coinbase_blind
NoFreeInstances
pallasPrime
poseidon_collision_resistance
poseidon_hash_output
reward_monotone
```

### The assumptions that were false or inconsistent — four of the nine, until 2026-09-20

The inventory above is a list of *unproved* statements. It is not a list of *true* ones, and this
section exists because **four** of the nine were worse than unproved. Two were **false**; two made
the axiom set **inconsistent**, which is strictly worse than false and a category this register had
not previously had to name:

| | condition | what follows |
|---|---|---|
| **unproved** | not known to hold | consumers are *conditional* — the budget table says so |
| **false** | known not to hold (or refutable) | consumers are *vacuous* — the budget table says nothing |
| **inconsistent** | its negation is derivable | **every** theorem is derivable, and every budget is meaningless |

Both latter cases showed a non-zero budget at their consumers, indistinguishable from the first.
That is the blind spot: the discipline measures *dependency*, not truth, and it cannot see either.

| assumption | why | what it needed |
|---|---|---|
| `pallasPrime` | `PALLAS_MODULUS` was composite — divisible by 3 | the *right modulus* |
| `reward_monotone` | `reward 0 = 0` is a pre-genesis sentinel, so the schedule jumps at height 1: `0 ≤ 1` but `reward 1 ≤ reward 0` is `1383764049 ≤ 0` | `1 ≤ h₁`, which the neighbouring `reward_tail_floor` already had |
| `fixed_base_mul_uses_constant`, `variable_base_mul_is_prover_chosen` | stated over a structure with a free `Bool` field, so a counterexample gadget is constructible and `False` follows | **removing the field** — see below |

All four are fixed. The first two are now true and remain unproved; the last two are gone, replaced
by theorems. Every refutation is machine-checked: `Pedersen.oldPallasModulus_was_composite`,
`Emission.reward_monotone_unbounded_is_false`, and `ECOps.lean`'s history note records the
inconsistency derivation.

The lesson is not "check harder". It is that an assumption whose *truth* is never tested is not a
weaker kind of theorem, it is an untested claim. `reward_monotone` shows how small the gap can be:
making `reward` a definition rather than an opaque function is what made the claim checkable, and it
took one `intro` and one `omega` to refute it. The inconsistency is worse still and took three lines:

    def rogueGadget : ECMulGadget :=
      { kind := ECMulKind.fixed_short, scalar := 0, base_is_constant := false, … }

    theorem inconsistency_via_axiom : False :=
      Bool.false_ne_true (fixed_base_mul_uses_constant rogueGadget (by intro h; cases h))

    theorem anything_at_all : (1 : Int) = 2 := inconsistency_via_axiom.elim

#### The inconsistency in detail

`ECMulGadget` carried `base_is_constant : Bool` as a field, and two axioms asserted the field could
not disagree with `kind`:

    axiom fixed_base_mul_uses_constant (g : ECMulGadget)
      (hkind : g.kind ≠ ECMulKind.var_base) : g.base_is_constant
    axiom variable_base_mul_is_prover_chosen (g : ECMulGadget)
      (hkind : g.kind = ECMulKind.var_base) : ¬ g.base_is_constant

The structure is freely constructible and nothing relates the two fields, so the first axiom applied
to `⟨fixed_short, 0, false, …⟩` yields `false = true`. **`False` was derivable, and so was everything
else** — the whole of `proofs/lean/`, at budget 0 or any other number.

The fix is a modelling change, not a restatement: `base_is_constant` is no longer a field.
`ECMulKind.baseIsConstant` derives it from the kind, `ECMulGadget.baseIsConstant` reads it off, and
both statements are now `@[axiom_budget 0]` **theorems** in `ECOps.lean`, proved by case split. With
no free field there is no counterexample to construct.

The general lesson, and the reason this is a register entry rather than a code review comment: **an
axiom over a freely-constructible structure is a claim about every element of that structure, not
about the elements the author had in mind.** `hkind` narrowed the domain by kind and left the other
field unconstrained; a model whose fields can disagree has an axiom set that can prove anything.

What those two were reaching for — that the model's kind-to-constancy mapping is the one the zkas VM
implements — is still open, and it is expressible only against a model of the VM's `.zk` `constant`
and `witness` blocks. Same class as `NoFreeInstances`.

#### `pallasPrime` in detail

`Axioms.PALLAS_MODULUS` read

    2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1

which is `0x3fff…ffed…6d` — and is divisible by 3, by 7 and by 109. It is composite. The real
Pallas base field modulus, from the vendored `pasta_curves-0.5.2/src/fields/fp.rs:32`, is

    0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
      = 2^254 + 45560315531419706090280762371685220353

The consequences were not confined to that one axiom. `instance : Fact (Nat.Prime PALLAS_MODULUS)`
derives `ZMod PALLAS_MODULUS`'s `Field` structure from it, so on the old constant every theorem in
`Pedersen.lean` — `pallas_coefficients`, `generator_on_curve`, `pedersen_add_comm`,
`pedersen_add_assoc`, `pedersen_add_identity`, `pedersen_additive_homomorphism` — was proved from a
falsehood, i.e. vacuous. `Arithmetic.base_div_mul_cancel` rests on the same fact, so it too.

**How it survived.** `Pedersen.lean`'s docstring said the curve had been "verified against the
vendored implementation — `pasta_curves-0.5.2/src/curves.rs`". That was true of the *generator*
(`NEGATIVE_ONE, TWO`, i.e. `(-1, 2)`) and false of the *modulus*, and the sentence did not distinguish
them. Nothing in the tree tied the Lean constant to the published one. `Arithmetic.lean`,
`Field.lean`, `Main.lean` and `Axioms.lean` each spelled the same wrong expression independently, so
the four definitions agreed with each other and disagreed with Pallas.

**What now prevents it.** Two kernel-checked theorems in `Pedersen.lean`:

* `pallasModulus_eq_pasta_curves : PALLAS_MODULUS = 0x4000…0001` — a transcription of the modulus
  from any source that spells it in hex now fails the build;
* `oldPallasModulus_was_composite : ¬ Nat.Prime (2 ^ 254 - 2 ^ 32 - 2 ^ 7 - 2 ^ 4 - 2 - 1)` —
  exhibiting 3 as a divisor, so the correction's central claim is checked rather than asserted.

Both are budget 0. The modulus itself is now correct, and `pallasPrime` is a **true** statement that
remains unproved: its `NOT PROVED BECAUSE` gives the reason, and `p − 1 = 2^32 · 3 · 463 · q` with `q`
a 64-digit cofactor means a Pratt certificate still needs that cofactor factored. The distinction
between *unproved* and *false* is the one this register has to keep, and the budget table is what
loses it.

**A third axiom was false for the same reason and is easy to miss.** `Arithmetic.base_div_mul_cancel`
is Fermat's little theorem in disguise —

    axiom base_div_mul_cancel (a b : Int) (hb : b % PALLAS_PRIME ≠ 0) :
      ((a * (b ^ (PALLAS_PRIME.toNat - 2))) % PALLAS_PRIME * b) % PALLAS_PRIME = a % PALLAS_PRIME

— and it is true exactly when the modulus is prime, since that is what makes `b^(p−2)` the inverse
of `b`. Under the composite modulus it was **false**: `a = 1`, `b = 3` gives
`21704357606889010529132860968972197894113948139101757424378608715565078302204` on the left and `1`
on the right. With the real modulus both sides are `1`. So one wrong constant made *three*
assumptions false — the primality claim itself, this one, and (via the `Fact` instance) every
theorem in `Pedersen.lean`. That is what a shared wrong constant does, and why
`pallasModulus_eq_pasta_curves` is worth more than its two lines suggest.

#### `reward_monotone` in detail

Stated as `∀ h₁ h₂, h₁ ≤ h₂ → reward h₂ ≤ reward h₁`. At `(0, 1)` that is `reward 1 ≤ reward 0`, and
`Emission.lean` gives `reward 0 = 0` and `reward 1 = INITIAL_REWARD = 1383764049`. The assumption
asserted `1383764049 ≤ 0`.

The corrected form carries `1 ≤ h₁`, which is the range the schedule is defined over — `reward 0` is
a sentinel, not a schedule value, and `reward_tail_floor` next door had always carried exactly that
hypothesis. The proof is still open, and the obstruction is now stated exactly rather than as "the
parity analysis is missing": in the induction, the case `e₁ = 2q₁ + 1` (odd) against `e₂ = 2q₂`
(even, `q₁ < q₂`) compares `fixedPowDecayGo q₁ (fpMul r b) (fpMul b b)` with
`fixedPowDecayGo q₂ r (fpMul b b)`, and the hypothesis compares the `q`s at a *common* accumulator —
so it yields a bound one multiplication too weak. Kernel-checked today:
`reward_nonincreasing_first_step`, covering the step across the sentinel and the first real step.

**Seven**, down from 34 — and down from nine earlier in this session, because two of the nine turned
out to make the theory *inconsistent* and could not stay. Each carries its four fields in
`Axioms.lean`; the classes are:

| assumption | why it is not proved | disposition |
|---|---|---|
| `poseidon_hash_output` / `poseidon_collision_resistance` | the sponge is not formalised | the two cryptographic assumptions; four binding theorems are proved *from* them |
| `pallasPrime` | `Nat.Prime` of a 254-bit modulus needs a Pratt certificate | **the** arithmetic assumption — replaced seven Pedersen postulates. It was **false** until 2026-09-20, because the modulus it quantified over was composite; see "The assumptions that were false" above |
| `base_div_mul_cancel` | same `pallasPrime` fact, stated over `Int` | candidate for discharge once `pallasPrime` lands |
| `coinbase_blind` | the real blind is `f(prev_commitment, H)`; `f` is an implementation detail | free parameter |
| `reward_monotone` | needs monotonicity of `fixedPowDecay`'s bit-loop in `exp`, which truncates at every squaring | falsifiable claim about a computable function. It was **false** until 2026-09-20, because it lacked the `1 ≤ h₁` hypothesis; see "The assumptions that were false" above |
| `NoFreeInstances` | Halo2 semantics are not modelled | names the ZK obligation; **one consumer** — `Capability.Inversion.capabilityType_of_circuitDerivable`, which takes it as the `CircuitDerivable r s` hypothesis. It used to be consumed by nothing, which is what §3 of this rewrite changed |

---

## Surface 1 — Code and consensus (`OBL-C`)

| ID | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-C1 | For every block, over non-coinbase native-token calls: `Σ output_commits + Σ burn_commits + Σ fee_commits == Σ input_commits` | `src/linear/src/proof_of_token_balance.rs` (`verify_proof_of_token_balance`) | Rust unit tests; `contrib/model/proof_of_token_balance.py` | C |
| OBL-C2 | The coinbase value equals `expected_reward(height)` **exactly** — not "at most" | `src/contract/native_token/src/entrypoint/mod.rs` (`pow_reward_v1`) | Rust; the Python model | C |
| OBL-C3 | `S_H = S_{H-1} + C_H` for the cumulative Pedersen commitment, and `TOTAL_SUPPLY_H = expected_cumulative_supply(H)` | `entrypoint/mod.rs` (cumulative chain writes); `blockchain.rs` (`expected_cumulative_supply`) | `verify_cumulative_supply.sh` (plaintext supply only, not the commitment); `SupplyChain.total_supply_theorem` and `cumulative_commit_theorem` are Lean theorems **conditional on the Pedersen assumptions** | C |
| OBL-C4 | The spendable coinbase note commits `effective_value` with `effective_value + total_pin == expected_reward(H)`, and `commitment == to_commitment(preimage)` | `entrypoint/mod.rs`; `src/contract/native_token/src/model/mod.rs` | Rust only | C |
| OBL-C5 | `expected_reward` is non-increasing and never below `TAIL_REWARD`; `R(0) = 0`, `R(1) = INITIAL_REWARD`; there is **no** supply cap | `src/sdk/src/blockchain.rs` (`expected_reward`, `fixed_pow_decay`) | **partly closed.** `DarkFi/Emission.lean` transcribes the schedule — constants, the exponentiation-by-squaring loop and `mul_fixed_point` — so `reward` is a **definition**, and it proves `reward 0 = 0`, `reward 1 = INITIAL_REWARD`, the **tail floor** for `h ≥ 1`, and the contraction `fpMul_le_left` (from `DECAY_FP < FP_ONE`). Still assumed: **non-increase**, which needs monotonicity of the bit-loop in `exp` (the loop truncates at every squaring, so it is not a closed form) | C |
| OBL-C6 | Per `token_commit` group, `Σ input value_commits == Σ output value_commits` | PN `entrypoint/mod.rs` (`verify_value_conservation`); native `entrypoint/mod.rs` (transfer; SpendV1 1-in-1-out) | Rust only; the native and PN implementations are **duplicated, not shared** | C |
| OBL-C7 | `canonical_reward + Σ uncle_rewards == base_reward` | `src/linear/src/supply_chain.rs` (`verify_uncle_split`); `contrib/model/chain_model.py` | Python model; Rust | C |
| OBL-C8 | A spend nullifier appears at most once chain-wide | `src/linear/src/chain_state.rs` (`spent_nullifiers`, the authoritative gate) + `db_contains_key` at each contract (defence-in-depth) | Rust + Python (`nullifier_lifecycle.py`); `Combinatorial/NullifierStorage.lean` mechanizes the *storage* half | C |
| OBL-C9 | A nullifier is non-zero and canonically encoded; decode rejects zero and non-canonical | `src/sdk/src/crypto/nullifier.rs` | Rust tests | H |
| OBL-C10 | Coinbase output is spendable only after `COINBASE_MATURITY` (100) blocks | `src/linear/src/chain_state.rs` | Rust + Python | M |
| OBL-C11 | No two outputs in one call carry the same commitment | `entrypoint/mod.rs` (mint, transfer) | Rust only | H |
| OBL-C12 | Secret keys are not `Copy`, not `Debug`-printable, zeroized on drop; `derive_instance` is deterministic and scopes keys per `(wallet, contract, instance)` | `src/sdk/src/crypto/keypair.rs` | Rust (compile-time for `Copy`) + Python model | H |
| OBL-C13 | Mint authority: PN `issue_v1` requires `poseidon_hash(backing_secret)` bound in-circuit, and rejects a stale registry root | `src/contract/promissory_note/src/entrypoint/mod.rs`; `proof/issue.zk` | zkas + `hooks/pre-commit` | C |
| OBL-C14 | The number of distinct L1 operation combinations over C contracts is `∏(nᵢ + 1) − 1`, hence `≥ 2^C − 1` for `nᵢ ≥ 1` — exponential in the number of contracts, and a **product**, not a sum | `proofs/lean/src/DarkFi/Combinatorial/Combinations.lean` | **proved.** `two_pow_sub_one_le_combinationCount` (exponential floor), `increment_ge_value` (`f(C+1) ≥ 2·f(C)`: the increment is at least the accumulated value, so no constant can be its increment), `combinationsIncludingIdle_append` (appending multiplies by `n+1`), `combinationCount_gt_sum` (refutes the additive reading at every `C ≥ 2`), and the instance `615192791076863999999999` over 31 contracts / 166 circuits by `norm_num` | C |
| OBL-C15 | The *size* of one composed capability is additive — barbs compose by union, so `|⋃_{c∈S} B c| ≤ Σ_{c∈S} |B c|` | `Capability/Composition.compose`; `ocap.md` §5.1 containment | **proved.** `Combinations.card_biUnion_le_sum`. This is the additive law that bounds blast radius, and it is a *different quantity* from OBL-C14 | H |

### The one that was silently false

**OBL-C1 was inert until recently.** The token-commit filter in
`verify_proof_of_token_balance` used `poseidon_hash([0,0])`, which matches no real DRKW call, so the
mass-balance anti-inflation check **passed vacuously** — every block satisfied it because nothing
was ever included. That is the failure mode this register exists to make visible: the check ran,
reported success, and verified nothing. The filter was fixed; the obligation remains worth stating
because the *silence* is what a regression would reproduce.

### The combinatorial claim, restated and proved — OBL-C14 / OBL-C15

The layer had **no contract-count axis at all**. `l1TrajectoryCount N K = N ^ K` counts K-step
trajectories over N anonymous objects *within one contract*; `boxTotalTransitionCount N M` is the
per-operation branching. Neither mentions how many contracts there are — which is why
`CompositionBounds.lean` came to carry `ocap_scaling (k : Nat) : True := by trivial` under the
heading "the formal statement of why DarkWow's architecture scales". A claim about scaling needs a
parameter to scale *in*, and the file had none.

`Combinatorial/Combinations.lean` supplies it. With C contracts offering `nᵢ` operations each, the
number of distinct operation combinations is `∏(nᵢ + 1) − 1`, and for `nᵢ ≥ 1` that is at least
`2^C − 1`:

| | |
|---|---|
| `two_pow_sub_one_le_combinationCount` | `2^C − 1 ≤ count` — exponential floor, needing only `nᵢ ≥ 1`, and so independent of whether contracts are disjoint or their barbs distinguishable |
| `increment_ge_value` | `f C ≤ f (C+1) − f C` — the increment is at least the accumulated value, i.e. `f(C+1) ≥ 2·f(C)`. A linear function's increment is a *constant*; this one grows, so no constant bounds it |
| `combinationsIncludingIdle_append` | appending a contract multiplies the count by `n+1` |
| `combinationCount_gt_sum` | the count exceeds `Σ nᵢ` at every `C ≥ 2` — the refutation of the additive reading, in Lean rather than in prose |
| `contractOps_combinationCount` | **615 192 791 076 863 999 999 999** over the 31 contracts / 166 circuits in the tree, by `norm_num` (kernel-checked) against a `Σ nᵢ = 166` additive reading |

**The correction this carries, and it is the point.** The documents assert that o-cap composition is
additive — `T(A ∘ B) = T(A) + T(B)`, "the state spaces add, not multiply", "prevents cross-contract
combinatorial explosion" (`privacy.md` §6, `safety.md` Lesson 23,
`contract-wasm-type-system.md` §C.7, `ai-index.md`). Two things are wrong with that chain:

* **`ocap_additive_composition` does not establish it.** After `rw [box_total_linear,
  purse_total_linear]` the two sides are syntactically identical: it restates the two per-contract
  count functions in closed form and adds them. The `+` is stipulated by the statement; no operation
  composing two contracts exists anywhere in the tree. It is a `ring` identity wearing a composition
  law's name — the same family as the tautologies check 7 rejects, one step subtler, which is why the
  detector does not fire on it.
* **The additive reading is false for the quantity that matters.** O-caps isolate contract *state*;
  they do not divide the *number of ways to combine contracts*. A transaction touching several
  contracts chooses one operation per contract simultaneously, and the count of such choices is the
  product. So there is no cross-contract combinatorial explosion that o-caps prevent in the count —
  the count is exponential with or without them.

What o-caps do give is **containment** (OBL-C15): the barbs a composed capability exhibits are a
union, so `|⋃_{c∈S} B c| ≤ Σ_{c∈S} |B c|`, and that is genuinely additive. Blast radius is bounded
by a sum. The *number* of combinations is not, and the reason the type system is needed is precisely
that the combination space is exponential and cannot be enumerated — which is an argument *for*
compositional reasoning, and it was being made as an argument that no such space exists.

---

## Surface 2 — ZK circuits (`OBL-Z`)

**Inventory: 180 circuits.** `src/contract/*/proof/` holds **166** across **31** contracts (34
contract directories, 3 without a `proof/`), plus 12 in `proofs/core/` and 2 in
`bin/darkirc/proof/`. The figure "120 across 26 contracts" in `arch/zk/opcodes.md`,
`opcodes-status.md` and `security-analysis.md` is wrong, and the docs' own per-contract table sums
to 139. All 180 contain at least one `constrain_instance`; 122 use `constrain_equal_base`.

| ID | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-Z1 | For every circuit and every `constrain_instance(X)`: `X` is either a pure opcode expression over witness bindings, or bound by `constrain_equal_base(derived, X)` before the expose, or **redundant** with another exposed determination, or a **declared** free witness | every `.zk` under `src/contract/*/proof/`, `proofs/core/`, `bin/darkirc/proof/` | **mechanized** — `script/circuit_instance_derivation.py`, gated by `scripts/check-circuit-instance-derivation.sh`. Of **868** instances over **180** circuits: 353 derived, 198 bound, 54 derived-inline, 208 redundant, 22 declared free, **33 unclassified across 22 circuits** | C |
| OBL-Z2 | Each circuit's public-input metadata matches its `constrain_instance` set — **position for position**, not merely in count | `scripts/check-circuit-metadata-alignment.sh` vs the entrypoint's `zk_inputs.push` | the script, which compares counts only, covers 8 of 31 contracts, and currently **FAILS** on `native_token/fee` (15 `constrain_instance`, no `ZKAS_FEE_NS` push) | C |
| OBL-Z3 | Every `poseidon_hash` call in a circuit is domain-separated, and by the *right* constant | `scripts/check-circuit-domain-separation.sh` | the script checks that *some* `DOMAIN_`/`witness_base` prefix is present, never that it is the correct one for the hash's purpose | H |
| OBL-Z4 | A pubkey derived by `ec_mul_base` + `ec_get_x/y` is bound by `constrain_equal_base` before being exposed | `hooks/pre-commit` | the hook — line-anchored, single-assignment, staged files only, and it cannot see an inline `constrain_instance(ec_get_x(pk))` | C |
| OBL-Z5 | The deliberately free witnesses are enumerated and each carries its host-side obligation | `script/circuit_free_instances.txt` | **partly.** 22 entries, each naming the Rust mechanism that constrains it (file + condition), all read rather than inferred. The `tx_nonce` case turned out not to need an entry at all — see "the redundant class" below | H |
| OBL-Z6 | The Orchard-tree hash is **Sinsemilla**: 10-bit altitude ‖ two 255-bit halves under `"z.cash:Orchard-MerkleCRH"`, depth 32, empty leaf **2** | `src/zk/vm.rs` (`MerkleRoot`) → `MerklePath`/`MerkleNode::combine`; `src/sdk/src/crypto/sinsemilla.rs` | **partly closed.** `HashOps.{merkleDepth, orchEmptyLeaf, sinsemillaCrh, computeMerkleRoot}` now carry the altitude in the CRH domain and use depth 32 / empty leaf 2, and `merkle_root_change_detection` is **proved** by induction rather than assumed. The remaining gap is the *primitive*: `sinsemillaCrh` substitutes the model's hash for Sinsemilla | H |
| OBL-Z7 | The SMT root is rate-2 Poseidon with **no** domain prefix, depth 255, empty leaf **0** — sharing neither primitive nor constants with the Orchard tree | `src/zk/gadget/smt.rs`; `src/sdk/src/crypto/smt/` | **closed in model.** `HashOps.{smtCrh, smtDepth, smtEmptyLeaf}` state the raw-pair Poseidon and the distinct constants, and `smtCrh_injective` is **proved** from `poseidon_collision_resistance` — no substitution needed, because the SMT really does use Poseidon | H |
| OBL-Z8 | ZK binaries are well-formed | `scripts/validate_zk_bins.sh` | the script — structural validity only, not that `.zk.bin` matches the current `.zk` source | H |

`set_membership` (0x59) is implemented and forces `expected_root` into the instance column, but
**no deployed `.zk` calls it** — only a comment in `oracle/proof/push_value_commitment.zk` warns
against it. `opcodes-status.md` lists it as "SOUND ✓" regardless.

### What the existing gates do and do not cover

The first three scripts are worth keeping as cheap structural checks. **None of them implements
OBL-Z1**, and the gap between them and it is where the Orchard bug lives. The fourth now does:
`check-circuit-instance-derivation.sh` (over the first two, which stay as fast filters).

* `check-circuit-metadata-alignment.sh` — **count parity**. It compares the number of
  `constrain_instance` calls against the number of values pushed for that circuit's namespace. Its
  own tail text says the real invariant is positional; it never checks position or names.
* `check-circuit-domain-separation.sh` — **prefix presence**. `witness_base` anywhere in the
  argument list satisfies it.
* `hooks/pre-commit` — **one binding pattern**, on staged files.

### OBL-Z1: what the mechanized check actually found

`script/circuit_instance_derivation.py` walks all 180 circuits and classifies every
`constrain_instance`. It is conservative by construction — anything it cannot classify is a
FAIL, because a false alarm is triageable and a false pass is not — and it reads `.zk` source
rather than the `.zk.bin` that actually ships (`validate_zk_bins.sh` covers binary validity).

Three classifications are mechanical and need no manifest: **derived** (the exposed name was
assigned a pure opcode expression), **bound** (a `constrain_equal_base(derived, X)` precedes the
expose) and **derived-inline**. Two are not, and the distinction between them is the whole
finding:

**The redundant class — 208 instances, and the reason the first count was wrong.** The initial
run reported 263 unclassified, 166 of them `tx_nonce`. The dominant `tx_binding` pattern is

```
tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce);
constrain_instance(tx_binding);
constrain_instance(tx_nonce);
```

Exposing `tx_nonce` beside a *derived* `tx_binding` grants the prover **no freedom it did not
already have** — it was going to choose `tx_nonce` regardless; the expose discloses that choice
to the host so the host can check it. That is a sound information-flow argument, and it makes
`tx_nonce` redundant rather than free. Getting there required two fixes to the checker that are
themselves the point: a `bound` exposure stores the *name* it is bound to, so the redundancy test
must follow the assignment chain (`computed_nullifier` → `poseidon_hash(DOMAIN_NULLIFIER, bet_id,
house_secret)`), and it must follow it *transitively*, because `root = merkle_root(pos, path,
coin_incl)` pins `coin_incl`, which pins `coin`, which pins `coin_spend_hook`. A one-level scan
misses both and reported 166 phantom findings. Fixing them removed 208 sites — including every
`coin_spend_hook`, `escrow_id`, `job_id` and `subscription_id` — and left the residue below.

**What the redundant class does NOT say.** It says the expose adds no freedom, not that the
witness is safe. `tx_nonce` is still prover-chosen; whether that matters depends on what the host
does with `tx_binding`, which is a property of the entrypoint and invisible to a `.zk` reader.
The checker records, for each redundancy, the exposed expression that discharges it, so the
residual obligation is addressable by name rather than diffuse.

**The unclassified residue — 33 instances across 22 circuits.** The 22 with a verified host
mechanism are in `script/circuit_free_instances.txt`. The rest are open, and several have the
shape of real defects rather than paperwork:

* `roulette/proof/settle_bet.zk` and `slot/proof/settle_bet.zk` — `payout` is exposed with **no
  constraint of any kind** in the circuit, under a comment reading *"Payout is calculated
  externally and verified here"*. The two differ in the host, and the difference is the point:
  * `slot` is **safe**. `slot/src/entrypoint.rs:596-607` recomputes
    `crate::model::calculate_payout(spin.bet_value, &wins, spin.house_edge)` and rejects
    `params.payout != payout`. The circuit's comment is decorative, the host check is real.
    Manifest entry.
  * `roulette` is **not**. `roulette/src/entrypoint.rs:130-138` pushes `params.payout` straight
    into the instance vector and never compares it to anything; the plaintext path computes
    `house_payout` from `bet.check_win` and moves *that* (`entrypoint.rs:593-611`,
    `validate_child_value_commit(..., house_payout, ...)`). So nothing is stolen — the proof's
    `payout` is inert — but the comment at `entrypoint.rs:129` claims *"The ZK circuit
    constrains payout as a public instance — this catches bugs in the entrypoint logic"*, and it
    does not: the binding runs the wrong way, from a value nobody reads. A circuit whose
    public-input claim is inert is not a defence-in-depth layer, and reading it as one is what
    the Orchard-class rule exists to prevent.
* `tender/proof/reveal_bid.zk` — `tender_id`, `bid_id`, `revealed_amount` are all exposed
  undetermined, so the circuit does not tie a revealed amount to the sealed bid. The entrypoint
  does (`entrypoint.rs:635`), which is why this is H and not C: the check exists, but the
  circuit's own comment claims *"Revealed amount matches the sealed bid"* as a circuit property.
* `stablecoin/proof/governance_report.zk` — the ratio check (`crb_times_debt <= coll_times_bps`)
  and the interest check are arithmetic over prover-supplied numbers, so **the circuit's checks
  prove nothing on their own**: they constrain the prover's own values against each other. What
  makes them mean anything is `entrypoint.rs:1331`, which compares `total_collateral`,
  `total_debt` and `interest_accrued` each against the config DB and rejects a mismatch — the
  third carrying an explicit HAZOP CRIT-1 comment naming itself as the defence-in-depth. Those
  three are manifest entries. `report_timestamp` is not: it appears **only** in `get_metadata`
  (`entrypoint.rs:398`) and is never validated or even stored, so an expose that nothing consumes
  remains in the residue. Same shape, same disposition at `accrue_interest.zk` — `old_total_debt`
  is checked at `entrypoint.rs:1483`.
* `multisig/proof/{create_group,sign,finalize}.zk` — `group_id`/`threshold`/`total_keys` are
  host-supplied, and `threshold` **is** bounded: `entrypoint/mod.rs:260`
  `if params.threshold == 0 || params.threshold as usize > params.pubkeys.len()` (the
  `InvalidThreshold` message in `error.rs:12` names the same rule). `total_keys` is *not* checked
  against `pubkeys.len()` at any of the sites read. The live item is `sign`/`finalize`: both
  expose `group_id` and `message_hash` from `params` with nothing in-circuit binding the signer
  to that group, so the group-membership check is entirely the entrypoint's to make.
* `oracle/proof/attest_value.zk` — `attestation_id`, `predicate` and `threshold` are exposed
  undetermined; the entrypoint stores `attestation_id` without checking it against the
  attestation contract (`entrypoint.rs:363`).
* `labor_market/proof/milestone_payment.zk` — `milestone_payment_amount` is range-checked and
  passed to the child transfer, but nothing compares it to the job's schedule.
* `dex/proof/execute_swap.zk` — resolved (host-derived FuncRefs, see the manifest).
* `proofs/core/set_v1.zk`, `proofs/core/lead.zk`, `bin/darkirc/proof/rlnv2-diff-signal.zk` —
  these are not contract circuits and their free inputs are protocol parameters, but they are
  unclassified all the same and belong in the same triage.

**Adversarially tested.** Deleting `constrain_equal_base(tx_binding_circuit, tx_binding)` from
`box/proof/put.zk` makes the checker name exactly `put.zk: tx_binding` **and** `tx_nonce` — the
second because its pin was the now-unexposed `tx_binding_circuit`. Adding both to the manifest
clears it. A gate that has never failed is not a gate; this one has.

### Why the residue is a triage list and not a bug list

Writing the residue up required reading the entrypoint for each candidate, and **four of the
findings did not survive contact with the Rust.** `slot`'s `payout` is recomputed and compared
(`entrypoint.rs:596`); `multisig`'s threshold is bounds-checked (`mod.rs:260`); `roulette`'s
`payout` binding runs the wrong way rather than being absent; and `stablecoin`'s three reports are
each compared against the config DB. Each had been asserted from the `.zk` source alone.

That is the structural point of OBL-Z1, and it belongs in the method rather than in a footnote:
**the `.zk` source cannot tell you whether a free public input is safe.** It can tell you that the
input is not determined in-circuit, which is a necessary condition for the Orchard bug, not a
sufficient one. The sufficiency question — does anything downstream pin this value? — lives in the
entrypoint, and no amount of care with the circuit text substitutes for reading it. A checker that
reported the residue as *vulnerabilities* would have been wrong four times out of the fifteen
cases examined here. It reports the residue as *what needs a host-side answer*, and the manifest
is where the answers go once someone has read the Rust.

**Two cross-cutting findings fell out of that triage**, and neither is visible from a single
circuit:

* **`tx_binding` is a constant in 11 of the 31 contracts that expose it.** `auction`, `baccarat`,
  `bridge`, `escrow`, `identity`, `lottery`, `otc_swap`, `relayer_endowment`, `roulette`, `slot`
  and `stablecoin` push `poseidon_hash([3, 0, 0])` into the `tx_binding` instance slot and
  `Base::zero()` into `tx_nonce`, so the prover must set `tx_commitment = tx_nonce = 0` and the
  pair is a fixed constant, identical in every transaction. `lottery/src/entrypoint.rs:89` says
  this is deliberate — *"tx fields are zero in heavyweight; the V2 clients commit to
  `poseidon_hash([3, 0, 0])`"* — so it is an unimplemented mechanism rather than a mistake. But
  it means the 49 circuits in those contracts have a `constrain_instance(tx_binding)` that binds
  nothing, and any document describing that pair as cross-transaction replay protection is
  asserting something the code does not do. **Counted, not fixed: 49 circuits.**
* **`dao_escrow`'s placeholder is not the same shape and is unsatisfiable.** It passes a bare
  `pallas::Base::zero()` (`entrypoint.rs:189`, commented *"Pattern A: pass-through placeholder"*,
  12 occurrences), while seven of its circuits — `init`, `pay_premium`, `propose_claim`,
  `resolve_dispute`, `set_governance_config`, `verify_member_capability`, `vote_claim` — assign
  `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` and expose it. The
  instance must equal that assignment, so verification requires `H(3, txc, txn) = 0`: a Poseidon
  preimage. Unlike the `H(3, 0, 0)` convention, whose constant is at least *reachable* by setting
  both fields to zero, this one has no satisfying witness. It is consistent with the standing
  `v1-vs-v2-client-hazard` note that `dao_escrow`'s clients are V1 against V2 circuits. **Not
  fixed here: it needs a decision about the convention, not a patch.**

---

## Surface 3 — Type system (`OBL-T`)

The normative document is `arch/type-system.md`; §7 lists seven "compiler-enforced invariants" and
§11 the proved properties. The barb alphabet is §1.1.

| ID | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-T1 | No barb distinction among the primitives is redundant: distinct names ⇒ distinct barb sets, and equal barb sets ⇒ equal names | `Types.lean` (`allPrimitiveTypes`, `typesDistinct`) | `Capability/Pareto.lean` — **proved**, and it was **false** until `↓shard` was added (see below) | C |
| OBL-T2 | Two types are distinct iff their barb sets differ (§2) | `Types.lean` (`typesDistinct`); 10 pair theorems in `Distinction.lean` | `decide` in the Lean; the criterion is *sufficient* for distinctness but does not by itself separate every pair — which is exactly how OBL-T1 failed | C |
| OBL-T3 | The five representations of the barb alphabet agree as intended (doc == Lean == core `BarbId`; sdk ⊆ doc; python == sdk) | `Types.lean`, `src/barb.rs`, `src/sdk/src/capability.rs`, `contrib/model/wallet_model.py`, §1.1 | `contrib/barb_alphabet_diff.sh` — now checks all five, including Python, which was previously asserted rather than checked | H |
| OBL-T4 | A "spent" barb is faithfully encoded iff its witness is a distinguished non-empty element (§0.1) | `Combinatorial/NullifierStorage.lean` | **proved** — `markSpent_faithful`, `markSpent_sound`, `markEmpty_not_adds`, `faithful_iff_nonempty`, … | C |
| OBL-T5 | Barb coverage: `requiredBarbs ⊆ compose(primitives)` for each capability type | `Capability/Composition.lean` | **proved** — all twelve by `decide` | H |
| OBL-T6 | If a circuit is derivable for `(r, s)` then `CapabilityType r s` is inhabited | `Capability/Inversion.lean` | **proved**, and **one-directional** — the converse is false (anonymous credentials, blind signatures and MAC tokens authorize with no proof system) | H |
| OBL-T7 | Every public input of the circuit for `(r, s)` is `constrain_instance`-derived | named as `Axioms.NoFreeInstances` | **nothing.** Uninterpreted, unconsumed — this is OBL-Z1 in type-system vocabulary | C |
| OBL-T8 | Genesis is a pure function of its inputs — no ambient authority, stages are pure transitions, embedded contract bytes are a quoted argument | `doc/src/arch/genesis.md` §"Genesis Is A Pure Function"; `bin/dwowd` `init_genesis` (hard-error on hash mismatch) | the pinned hash in `bin/dwowd/genesis_hash.txt` — an *observable witness*, not a proof; the audit's B-series shows the pin was broken by build non-reproducibility | C |
| OBL-T9 | The barb alphabet distinguishes the types it claims to: distinct capability types have distinct, non-subsumed barb sets, so a barb set identifies a capability | `Capability/Types.lean` (`allPrimitiveTypes`, 33 barbs); `Capability/Composition.lean` (13 `Resource`s) | **FALSE as it stands, and measured.** Of 13 capability resources, **12** have a barb set contained in the union of the others, so they contribute nothing to a composition the others do not; `purse_deposit` and `purse_withdrawal` have **identical** barb sets; of 16 primitives, **11** carry no barb of their own (`nullifier = {nullify}` ⊆ `intentNullifier = {gate, nullify}`). The consequence: **38** distinct barb-set unions over all `2^13 = 8192` subsets — the type count is *sublinear* in the resources, not exponential. The same defect that made `primitiveTypesAreParetoEfficient` false before `↓shard` was added | H |
| OBL-T10 | No proof in the tree is checked by the *compiled code generator* rather than the kernel — i.e. no proof rests on `Lean.ofReduceBool` or `Lean.trustCompiler` | `proofs/lean/src/**/*.lean`; `script/check_lean_axioms.py` check 5 | **closed (2026-09-20).** Every `native_decide` is gone: 15 in `Pareto.lean` and 10 in `Distinction.lean` are `decide` (kernel-checked), the four in `Combinatorial/{CeilingDerivation,ComplexityJump,Limits}.lean` are `decide` / `unfold …; decide`, and the four `-- DECLARED:` lines are removed with them. Measured: **zero** occurrences of `ofReduceBool`/`trustCompiler` in the budget table, over 237 theorems. This is the mechanism by which a budget could silently under-report, so its absence is worth an entry | H |

### The obligation that was false until it was checked — OBL-T1

`primitiveTypesAreParetoEfficient` was listed as PROVED in §11.1 and had never been checked: the
file did not parse (`∀ (t1 ∈ …) (t2 ∈ …)` is not Lean 4 syntax), so its `native_decide` never ran.
When the statement was repaired, `decide` reported the proposition **false**:

```
ContractId    : barbs := {Barb.dispatch}
ExternalChain : barbs := {Barb.dispatch}
```

Two "distinct" primitive types with identical barb sets — the same behavioural type under the
system's own §2 criterion. It is fixed by giving `ExternalChain` the `↓shard` barb (§1.1 row 33:
`↓denominate` names the asset, `↓shard` names the name-space; §9.5 already writes the shards as
ρ-processes), which also makes the bridge's **namespace convergence** visible as a property of the
composition rather than a label: a bridge is `externalChain ⊗ chainDepositProof ⊗ bridgeAddress ⊗
dleqProof`, and its convergence is that barb set, not a barb of its own.

The generalisation is the entry's real content: **the barb alphabet was not separating the
primitives it was supposed to separate, and nothing checked.** OBL-T3's five-way diff is the
regression guard.

---

## What this register implies

Ordered by what unblocks the most:

1. **OBL-Z1 has no check at all**, and it is the one whose violation mints value. It is structural,
   so it is mechanizable over the `.zk` sources — that is the deliverable, and it subsumes OBL-Z4.
2. **OBL-C5 and OBL-C3 are the supply chain**, and both are only partly proved: the Lean
   `SupplyChain` theorems are structural inductions that hold *for any* `reward`, conditional on the
   Pedersen assumptions. Making `reward` a definition transcribed from `blockchain.rs` and proving
   non-increase plus the tail floor is what turns them into statements about the real schedule.
3. **OBL-Z6/OBL-Z7 are model errors, not gaps.** The Lean merkle model is the wrong primitive on
   the wrong tree with the wrong base case; correcting it is a prerequisite for anything built on
   `compute_merkle_root`.
4. **OBL-T7 is a name, not a proof.** `NoFreeInstances` is uninterpreted and unconsumed; it becomes
   consumable once OBL-Z1's checker certifies the circuits and the opcode semantics in
   `proofs/lean/` are in place.

## Cross-references

* Finding corpus: `dev/contracts/safety.md` (the only partial ledger), `arch/audit/*`,
  `arch/security-analysis.md`, `arch/hazid-report.md`, `arch/sync-hazop.md`,
  `arch/consensus/node-sync-hazop.md`, `arch/audit/l1-write-path-hazop.md`
* Lean HAZOP register: `proofs/lean/src/DarkFi/HAZOP.lean` + `HAZOP/{Critical,High,Elevated}.lean`
  (the circuit pass, and the assumption pass added alongside it)
* Assumption boundary: `proofs/lean/src/DarkFi/Axioms.lean`; checker `script/check_lean_axioms.py`
