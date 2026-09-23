# Verification Obligation Register

> **What this is.** Every property this system needs to hold, stated as a proposition, with where it
> is enforced, whether anything checks it today, and what would prove it. It is the specification
> that the Lean work in `proofs/lean/` is written against — the obligation set is derived from the
> *system*, not from the existing Lean files.

> **What this is not.** It does not merge the raw findings. This repository accumulated ~7 independent
> finding-ID schemes (`V1`–`V7` means four different things in four documents; `C1` means four, one of
> them an unwrap-audit tier), >500 raw findings, and only `dev/contracts/safety.md` as a ledger.
> Rewriting every audit document to one ID space would add nothing to what gets proved.
>
> **What changed on 2026-09-22.** The *root causes* are now stated once: `safety.md` was rewritten
> around twelve of them, and every legacy scheme — the old `Lesson 1`–`25`, the circuit HAZOP's
> `RC1`–`RC5`, the consensus HAZID report's colliding `RC1`–`RC6`, `HAZOP.lean`'s seven patterns, the
> red-team `RC-A`–`RC-I` — maps into them through the alias table at that document's foot. The
> *findings documents* whose items are all resolved were removed, and everything they left open was
> promoted into the section above. So the corpus no longer "stays where it is" for those; the
> remaining audit documents still do, and are cited here as before.

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

### The assumption that is true of the model and false of the thing it names — `poseidon_collision_resistance`

A fourth failure mode, distinct from the three above and from being unproved. `poseidon_collision_resistance`
is **consistent** — no `False` follows from it, and I checked. It is also **not a hypothesis about
Poseidon**, and the four theorems derived from it therefore say nothing about the deployed system.

The axiom reads

    axiom poseidon_collision_resistance :
      ∀ (x y : List Int), x ≠ y → poseidon_hash_output x ≠ poseidon_hash_output y

which is **injectivity**. The docstring called it "collision-resistant — no two distinct input lists
share an output; equivalently `poseidon_hash_output` is injective", and the two are not equivalent:

* injectivity is **strictly stronger** than collision-resistance. CR permits collisions that are hard
  to *find*; injectivity forbids them outright. Injectivity implies CR, never the converse.
* injectivity is **false of the real Poseidon**, by pigeonhole. `src/zk/vm.rs:1123` dispatches
  `poseidon_hash` for 1 to 24 input elements — `vla!(args, a, b, c, 1 2 3 … 24)` — and
  `P128Pow5T3` over the Pallas base field returns one field element. The domain has cardinality at
  least `2^254 · 24` bits of entropy and the range has `2^254`, so distinct inputs sharing an output
  exist necessarily.

The axiom is satisfiable only because the model has none of the real function's structure: the domain
is `List Int` and `Int` is countable, so an injection between them exists and consistency holds. What
the model describes is an injective function `List Int → Int` — which no sponge of this shape can be.

**Consequence for the four consumers.** `HashOps.commitment_binding`, `nullifier_binding`,
`smtCrh_injective` and `merkle_root_change_detection` each derive a *hash inequality* from an input
inequality — the injectivity direction, visible in every one of their proof terms. They are sound as
statements about an injective `h`. Poseidon is not injective, so they do not transfer to it, and the
budget table cannot express that: it records that the proofs *cite* the assumption, not that the
assumption is false of the object the theorems name.

Closing it needs the standard-model form — "no efficient adversary finds a collision" — which is a
statement about adversaries rather than about a function, and there is no computational model in this
tree to state it in. Recorded rather than papered over: this is the same class as `pallasPrime` and
`reward_monotone` (assumptions that were false) and the `ECOps` pair (which were inconsistent), but
one level subtler — the statement is fine, the *model* is fine, and it is the relationship between
them that does not hold.

**The four consumers are split, so the two are distinguishable.** Each was the same derivation — a
differing component forces a differing hash, given injectivity — with the assumption folded in, and
all four therefore carried a budget that said nothing about which part was proved. Now:

| budget 0 — the content, no assumption | budget 1–2 — the instantiation, citing the axiom |
|---|---|
| `hash_ne_of_component_ne` (the engine) | `commitment_binding` |
| `commitment_binding_of_injective` | `nullifier_binding` |
| `nullifier_binding_of_injective` | `smtCrh_injective` |
| `smtCrh_injective_of_injective` | `merkle_root_change_detection` |
| `foldMerkleRoot_change_detection` | |

The Merkle one is the clearest gain: `computeMerkleRoot` is now `foldMerkleRoot sinsemillaCrh` with
the compression a **parameter**, and `foldMerkleRoot_change_detection` proves that changing a leaf at
a fixed position changes the root for *any* CRH injective at a fixed altitude — a two-line structural
induction, budget 0, no cryptography. The assumption is now visible as exactly one thing: that
`sinsemillaCrh` is such a CRH, in a model that also substitutes Poseidon for Sinsemilla (OBL-Z6).

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

**Six**, down from 34 — and down from nine earlier in this session: two made the theory
*inconsistent* and could not stay, and `base_div_mul_cancel` turned out to be `pallasPrime`
restated over `Int` and is now a theorem (`DarkFi/BaseDiv.lean`). Each carries its four fields in
`Axioms.lean`; the classes are:

| assumption | why it is not proved | disposition |
|---|---|---|
| `poseidon_hash_output` / `poseidon_collision_resistance` | the sponge is not formalised | the two cryptographic assumptions; four binding theorems are proved *from* them |
| `pallasPrime` | `Nat.Prime` of a 254-bit modulus needs a Pratt certificate | **the** arithmetic assumption — replaced seven Pedersen postulates. It was **false** until 2026-09-20, because the modulus it quantified over was composite; see "The assumptions that were false" above |
| `coinbase_blind` | the real blind is `f(prev_commitment, H)`; `f` is an implementation detail | free parameter |
| `reward_monotone` | needs monotonicity of `fixedPowDecay`'s bit-loop in `exp`, which truncates at every squaring | falsifiable claim about a computable function. It was **false** until 2026-09-20, because it lacked the `1 ≤ h₁` hypothesis; see "The assumptions that were false" above |
| `NoFreeInstances` | Halo2 semantics are not modelled | names the ZK obligation; **one consumer** — `Capability.Inversion.capabilityType_of_circuitDerivable`, which takes it as the `CircuitDerivable r s` hypothesis. It used to be consumed by nothing, which is what §3 of this rewrite changed |

---

## Surface 1 — Code and consensus (`OBL-C`)

| ID | Status | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-C1 | **SATISFIED** | For every block, over non-coinbase native-token calls: `Σ output_commits + Σ burn_commits + Σ fee_commits == Σ input_commits` | `src/linear/src/proof_of_token_balance.rs` (`verify_proof_of_token_balance`) | Rust unit tests; `contrib/model/proof_of_token_balance.py` | C | **SATISFIED, enforced on three live paths and pinned by negative controls — but the code's own `UNVERIFIED` tag for the devnet half still stands. Read 2026-09-22.** The check is `left != right` on `pallas::Point` (`proof_of_token_balance.rs:169-175`: `total_outputs + burn_aggregate + fee_aggregate` vs `total_inputs`), an exact Pedersen equality, so value *and* blind sums must match — not a bound, nothing saturating. It is called from three production paths, not a verifier without a caller: `block_acceptor.rs:184`, the P2P pre-filter `linear_broadcast.rs:402`, and the sync pre-filter `consensus_linear.rs:389`. **Negative controls exist**: `test_secret_mint_rejected` (`:399`, output 1,000,000 against input 100), `test_one_unit_inflation_rejected` (`:567`), `test_one_unit_inflation_different_blinds_rejected` (`:575`), `test_value_match_but_blind_mismatch_fails` (`:595`), and `test_micropayment_siphon_over_many_blocks` (`:604`, five sizes ascending to 1,000,000→1,000,100) — every one asserting rejection, with positives beside them (`test_balanced_transfer_with_same_blind_passes`, `:584`), so the pair is two-sided. *Two precisions the row's wording omits.* **(i)** The sums are over `BurnV1`, `TransferV1`, `SpendV1` and `FeeV3` only — `MintV1`, `PoWRewardV1`, `FeeCollectV1` and `UncleMintV1` are skipped by selector regardless of position (`:124-127`), and the coinbase is skipped only when it is tx 0 *and* `first_call_is_pow_reward()` (`:101-106`). **(ii) The devnet half is still tagged unverified in the code**: `proof_of_token_balance.rs:80-87` carries `UNVERIFIED(POTB-1): needs cargo test … AND a full devnet-history sync validation before enabling in production`, and that tag was not removed when the filter was fixed in `271876fd83`. That matters more than it looks: the fix *activates* a check over historical blocks, so a single historical block whose blind sums do not balance would be rejected on sync — the tag is the code asking for exactly the validation that would rule this out, and it is still open |
| OBL-C2 | **CLOSED** | The coinbase value equals `expected_reward(height)` **exactly** — not "at most" | `src/contract/native_token/src/entrypoint/mod.rs` (`pow_reward_v1`) | Rust; the Python model | C | **CLOSED 2026-09-22 — one of the rows with no recorded status; closed from a measurement rather than a reading.** The distinguishing case is over-minting, which a lower-bound-only check would allow, so `test_heavyweight_coinbase_rejects_wrong_reward` builds a coinbase with `expected_reward + 1` and asserts `accept_block` refuses it (`heavyweight_pipeline.rs:1160-1180`, whose own comment names the F1 exact-equality check at `entrypoint/mod.rs:818`). **Observed passing** in the workspace sweep re-run on 2026-09-22 — `cargo test --release --all-features --workspace --no-fail-fast -j 8` — whose own line is, verbatim: |

> `test tests::heavyweight_pipeline::test_heavyweight_coinbase_rejects_wrong_reward ... ok`

**Quoted rather than cited by line number, and that is a correction to this row as first written.** It cited `baseline_sweep.log:64194` — a line number into `/tmp`, which a reboot emptied between the two runs, leaving the register's only *measured* closure resting on a file that no longer exists. The claim was true (it reproduced, the test passing again in the second run) but the evidence was stored where evidence does not survive. The positive direction is exercised by every passing heavyweight test, each of which builds a coinbase with the correct reward and is accepted — so the pair is a real positive/negative control, not one-sided |
| OBL-C3 | **SATISFIED** | `S_H = S_{H-1} + C_H` for the cumulative Pedersen commitment, and `TOTAL_SUPPLY_H = expected_cumulative_supply(H)` | `entrypoint/mod.rs` (cumulative chain writes); `blockchain.rs` (`expected_cumulative_supply`) | `verify_cumulative_supply.sh` (plaintext supply only, not the commitment); `SupplyChain.total_supply_theorem` and `cumulative_commit_theorem` are Lean theorems **conditional on the Pedersen assumptions** | C | **SATISFIED, and the schedule half is enforced *by induction* rather than by recomputation — which the row does not say and a reader must not assume. Read 2026-09-22.** Both halves are checked in `pow_reward_v1`. The commitment half is direct: `pr.old_cumulative_commit != old_cumulative` (`native_token/src/entrypoint/mod.rs:1105-1117`, against state) and `pr.new_cumulative_commit != new_cumulative` (`:1122-1129`) where `new_cumulative = old_cumulative + pr.output.value_commit` — so `S_H = S_{H-1} + C_H` with `C_H` the Pedersen commitment to the **full** base reward, not the reduced `header.total_reward` (`verify_uncle_split` uses that reduced quantity separately, `chain_state.rs:1071-1079`; they are different values and this row is about the former). The plaintext half is `current_supply + pr.input.value != pr.expected_cumulative_supply` (`:1054-1059`). **The precision: nothing recomputes the emission schedule at verification time.** `expected_cumulative_supply(H)` is a *builder-supplied field* (`bin/dwowd/src/registry/model.rs:202-205`), and the host's M10 re-derivation (`block_acceptor.rs:656-689`) checks only the *step* (`compute_next` = previous entry + this coinbase, `supply_chain.rs:292-303`) — never the schedule. What makes the equality hold anyway is the per-block schedule check in the same function, `pr.input.value != expected_reward(verifying_block_height)` (`:1029-1036`), against a **node-computed** pure function: every block advances the running total by exactly `R(h)`, so if the state starts at zero the accumulated total equals `Σ_{h≤H} R(h)` by induction, and the builder's field is a redundant cross-check rather than the authority. So the proposition holds, but through C4's equality and induction — not because anyone recomputes the sum. `verify_cumulative_supply.sh:52-64` is the only place that compares the stored total against the schedule, and it disclaims the commitment half itself (`:70-72`). **Gap: no negative control for either half here** — no test builds a `PoWRewardParamsV1` with a wrong `expected_cumulative_supply`, `new_cumulative_commit` or `old_cumulative_commit` and asserts rejection. The Lean theorems remain conditional on the Pedersen axioms, and `SupplyChain.lean:44-47` states plainly that the generators are unvalidated *parameters* |
| OBL-C4 | **SATISFIED** | The spendable coinbase note commits `effective_value` with `effective_value + total_pin == expected_reward(H)`, and `commitment == to_commitment(preimage)` | `entrypoint/mod.rs`; `src/contract/native_token/src/model/mod.rs` | Rust only | C | **SATISFIED — as *two* checks in *two* layers, not one equality in one place, and one of the two layers is the weaker one. Read 2026-09-22.** The premise's single equality is enforced transitively. **(i) The split** — `pr.effective_value.checked_add(pr.total_pin) != Some(pr.input.value)` — is checked **twice**: in the host's `validate_block_structure` (`src/linear/src/validation.rs:455-460`, reached from `block_acceptor.rs:105`) and again in the WASM `pow_reward_v1` (`native_token/src/entrypoint/mod.rs:1000-1006`). **(ii) The schedule** — `pr.input.value != expected_reward(verifying_block_height)` — is checked **only in WASM** (`:1029-1036`, the HAZOP F1 exact-equality fix); the host structural layer never compares the coinbase value against `expected_reward(H)`. So the premise holds, but a reader who looks only at the host's validator would find the split and not the schedule. `commitment == to_commitment(preimage)` is likewise checked on both sides (`validation.rs:450-454`, `entrypoint/mod.rs:996-999`), and `to_commitment` is the Poseidon preimage over domain, pubkey, `value`, asset id, spend hook, user data and blind (`native_token/src/model/mod.rs:232-253`) — recomputed rather than compared to a supplied digest, which is what makes it a binding. The preimage commits the **reduced** `effective_value` while `value_commit` commits the **full** value (`client/transfer/proof.rs:169-190`), so the two commitments in one coinbase legitimately disagree, and that is the design rather than a leak. Note `model/mod.rs` is struct definitions only — the row's citation of it is where the types live, not where a check runs. **Coverage: the schedule equality has a negative control; the split equalities do not.** Over-minting by one is rejected — `test_heavyweight_coinbase_rejects_wrong_reward` submits `expected_reward + 1` (quoted in `OBL-C2`'s closure above), which is caught by (ii). No test mutates `effective_value`, `total_pin` or `commitment_attrs` to a mismatched value and asserts rejection, and the positive coverage is real but narrow: `uncle_minting.rs:159` (`test_uncle_note_persisted_and_reversed`) drives a genuinely reduced coinbase (`effective = reward − uncle.pin_confirmed`) through `accept_block`, and the no-uncle path (`build_linear_coinbase`, `registry/model.rs:173-174`) passes `effective == value` |
| OBL-C5 | **PARTLY** | `expected_reward` is non-increasing and never below `TAIL_REWARD`; `R(0) = 0`, `R(1) = INITIAL_REWARD`; there is **no** supply cap | `src/sdk/src/blockchain.rs` (`expected_reward`, `fixed_pow_decay`) | **partly closed.** `DarkFi/Emission.lean` transcribes the schedule — constants, the exponentiation-by-squaring loop and `mul_fixed_point` — so `reward` is a **definition**, and it proves `reward 0 = 0`, `reward 1 = INITIAL_REWARD`, the **tail floor** for `h ≥ 1`, and the contraction `fpMul_le_left` (from `DECAY_FP < FP_ONE`). Still assumed: **non-increase**, but the analysis has narrowed to one case. Monotonicity follows from the single-step lemma `G (n+1) r b ≤ G n r b`, which splits by parity: the even case is **proved** (`Emission.fixedPowDecayGo_mono_acc`), and only the odd case is open — it needs a bound on the *factor* the extra step multiplies by, because the accumulator induction lands on the wrong side of the target. Three routes are ruled out with evidence and should not be retried (`Axioms.reward_monotone` lists them) | M |
| OBL-C6 | **SATISFIED** | Per `token_commit` group, `Σ input value_commits == Σ output value_commits` | PN `entrypoint/mod.rs` (`verify_value_conservation`); native `entrypoint/mod.rs` (transfer; SpendV1 1-in-1-out) | Rust only; the native and PN implementations are **duplicated, not shared** | C | **SATISFIED on every live path, and *unpinned everywhere* — no test asserts any of the three checks, and no negative control exists for any of them. Read 2026-09-22.** Three implementations, all exact equalities, none shared: the PN's `verify_value_conservation` (`promissory_note/src/entrypoint/mod.rs:516-557`, a named function, called from `transfer_v1` at `:822` and `otc_swap_v1` at `:1269`), the native `transfer_v1`'s inline copy of the *same algorithm* (`native_token/src/entrypoint/mod.rs:779-814`, logic-for-logic identical apart from the `msg!` prefix and the error enum), and the native `spend_v1`'s strict subset (`:861-867`, one input against one output, `!=` on the points, additionally gated to the native token at `:830-838`). All are reached from `process_instruction`'s dispatch, so none is a verifier without a caller. **The gap is coverage, and it is the `C10` class**: a repo-wide search finds `ValueMismatch` only at error-variant definitions and entrypoint return sites — never in an assertion. `native_token`'s test module and `tests/unit.rs` are structural/serialization only, `promissory_note/tests/integration.rs` is enum/serialization only, and the one test that asserts the *property* (`bin/dwowd/src/tests/wallet_integration.rs:472-490`) checks that the wallet *builds* a balanced transaction, which is a positive control on the builder rather than a rejection test on the entrypoint. So value inflation in one call is refused by code no test would notice the removal of. **And the duplication has already drifted**: a third copy at `bearer_bond/src/entrypoint/mod.rs:708-715` sums *all* inputs against *all* outputs globally, without the per-`token_commit` grouping the other two do — a weaker variant, recorded here because the row's premise ("duplicated, not shared") is the cause and this is its second consequence. **A negative control was attempted on 2026-09-23 and the attempt is what is recorded, because the obstacle is a design constraint rather than a missing test.** Every check in this row runs in **exec**, and no test in this tree has ever put a transfer in a block: `bin/dwowd/src/tests/wallet_transfer_integration.rs` builds a real `TransferV1` and stops, never wrapping it. The pieces to do so exist — `tests/harness.rs:136` `build_contract_tx`, `heavyweight_pipeline.rs:79` `build_witness`, and `bin/dww`'s `build_native_transfer` — but nothing combines them, and `block_acceptor.rs:257-292` refuses a non-coinbase transaction without a witness, so a hand-built one cannot reach exec either. *The perturbation that would distinguish this rule from `OBL-C1`, and why the obvious one does not:* `verify_proof_of_token_balance` runs at `block_acceptor.rs:184`, before WASM at `:475`, and it sums **per item** over native-tagged commitments, while this row checks **per `token_commit` group** and additionally rejects an output whose token type is absent from the inputs (`native_token/…/entrypoint/mod.rs:808-810`). Any regrouping that moves value shifts the native sums, so `C1` fires first and the test would pin `C1`. The one construction `C1` cannot see is an extra output of **zero** value carrying a foreign token type — a zero commitment is the identity, so the sums are untouched — and that is this row's rule alone. *Why even that is blocked:* it requires editing the transfer's serialized params, after which the transaction carries no valid proof for its own contents, and L2 refuses it before exec. **So the unlock for this row and for `OBL-C11`'s testable half is a test-side transfer prover that works** — `bin/dww`'s has a documented `ProvingKey::build` synthesis error — **or a documented test-only L2 bypass.** Recorded as the finding rather than left as "no test": the constraint is shared by every exec-phase rule in this contract, which is a larger fact than two rows |
| OBL-C7 | **CLOSED** | `canonical_reward + Σ uncle_rewards == base_reward` | `src/linear/src/supply_chain.rs` (`verify_uncle_split`); `contrib/model/chain_model.py` | Python model; Rust | C | **CLOSED 2026-09-22 — another row with no recorded status.** `CumulativeSupplyChain::verify_uncle_split` (`supply_chain.rs:313`) enforces the sum as an **exact equality**, erroring with the three values named rather than clamping or bounding. Two things make it an obligation rather than a function: it is **called in production** at `chain_state.rs:1075` — the comment at `:1068` records the ordering requirement that it run *before* the sled commit closure, so a violating block cannot be persisted — and it carries a negative control, `test_verify_uncle_split_rejects_imbalance`, beside the no-uncle, single and multi cases. So this is not the "verifier with no caller" shape the residue is full of |
| OBL-C8 | **SATISFIED** | A spend nullifier appears at most once chain-wide | `src/linear/src/chain_state.rs` (`spent_nullifiers`, the authoritative gate) + `db_contains_key` at each contract (defence-in-depth) | Rust + Python (`nullifier_lifecycle.py`); `Combinatorial/NullifierStorage.lean` mechanizes the *storage* half | C | **SATISFIED — the gate is consensus-side, typed, and restart-durable; one test gap named. Read 2026-09-22.** The authoritative check is in `connect_block` (`chain_state.rs:1203-1213`): for every transaction's nullifiers, `if self.has_nullifier(nf) |  | !block_nfs.insert(*nf)` → `BlockIsInvalid("duplicate spend nullifier (double-spend)")`, where the local `block_nfs` set catches a duplicate *within* one block and `has_nullifier` (`:658-664`) catches one across blocks. Three properties make it an obligation rather than a lookup. **(i) It is consensus-side**, not contract-side: `connect_block`'s own doc calls itself "the ONLY path for block insertion — used by genesis, sync, broadcast, miner RPC, stratum, and merge mining" (`:824-827`), and the contract-layer `db_contains_key` checks in `transfer_v1` (`:760`), `spend_v1` (`:847`), `burn_v1` (`:898`) and `fee_v3` (`:225`) are defence-in-depth rather than the gate. **(ii) It is keyed by the field element**, `BTreeSet<Nullifier>` with `Nullifier(pallas::Base)` (`sdk/src/crypto/nullifier.rs:41-42`, `Ord` derived from the element) — so the two-encodings-one-nullifier hazard that `OBL-C9` closed in the *byte*-keyed sled layer cannot arise in this set at all; the sled keys are `to_bytes()` and are re-parsed through `from_bytes`, which rejects zero and non-canonical input, when the set is rebuilt on restart (`chain_state.rs:413-426`). **(iii) Check and insert are separate mutex acquisitions but cannot interleave**, because both live inside `connect_block` under the outer `connect_lock` (`:845`), which `disconnect_block` also takes (`:1709`) and which removes the same nullifiers on reversal (`:1922-1926`). Mempool admission consults the same set (`crates/dwow-mempool/src/lib.rs:379-389`). Tests are real but **not end-to-end**: `test_nullifier_persistence_roundtrip` (`:2242`) and `test_nullifier_replay_detected` (`:2271`) pin `has_nullifier`/`track_nullifier` — including the distinction that a *claim* nullifier (coinbase, fee-collect) is not a spend — and `nullifier.rs`'s six type tests plus `native_token/tests/unit.rs:232-250` pin the encoding rules with a positive control. **No test feeds a block carrying a duplicate spend nullifier into `connect_block` and asserts that error string** — it appears only at `:1208` — so the *composition* of gate, block-local set and replay set is enforced but unpinned. **Closed 2026-09-22** (`c027d1de6e`): `test_connect_block_rejects_duplicate_spend_nullifier` drives **both halves** through `connect_block` itself — across blocks, and repeated within one block — asserting the gate's own error text and that the chain did not advance. Reaching it needed four things, none of them about the rule under test, and the test records all four: a merkle root over its transactions; the WASM batches `connect_block` demands of a non-genesis block even when the test executes no WASM; the target the chain itself expects, read from `get_next_work_required`; and a height above the tip, because a block at the tip's own height is diverted to the *competing* path — which is why no earlier in-crate test could reach this gate at all |
| OBL-C9 | **CLOSED** | A nullifier is non-zero and canonically encoded; decode rejects zero and non-canonical | `src/sdk/src/crypto/nullifier.rs` | Rust tests | H | **CLOSED 2026-09-22 — one of the eleven rows that had no recorded status.** Both halves hold: `Nullifier::from_bytes` matches on `pallas::Base::from_repr`, returning `Ok` only for `Some(v) if v != 0`, and erroring with a distinct message for a zero value and for `None` (a non-canonical encoding). Only the **zero** half was tested (`native_token/tests/unit.rs::test_nullifier_rejects_zero`), so the canonical half was enforced but unpinned — and it is the half that matters beyond tidiness: if a non-canonical encoding were accepted, one nullifier would have **two** byte representations and a spent-nullifier set keyed on raw bytes would see them as two nullifiers. `test_nullifier_rejects_non_canonical_encoding` closes it, with a control that a canonical value is still accepted so it cannot pass against a `from_bytes` that rejects everything. The test was placed in the **contract's** `tests/` directory on purpose: that crate's `SOURCE_MANIFEST` globs `find src -type f -name '*.rs'`, which does not reach `tests/`, so it invalidates no artifact — verified by re-running `check-artifact-freshness.sh` (32 fresh). A test in `src/sdk` would have invalidated all 32 |
| OBL-C10 | **CLOSED** | Coinbase output is spendable only after `COINBASE_MATURITY` (100) blocks | `src/linear/src/chain_state.rs` | Rust + Python | M | **CLOSED 2026-09-22 by reading, with its coverage gap named rather than glossed.** The check is `chain_state.rs:1119-1155`: for every non-coinbase transaction, each nullifier's *own* creation height (from `nullifier_set`, which holds kind-0 **claim** nullifiers — PoWRewardV1 and FeeCollectV1 — for exactly this purpose) is compared against `height - created_at < COINBASE_MATURITY`, erroring with both heights named. Three properties make it an obligation rather than a lookup: it runs **before** the sled commit closure (`:1157`), which is the `C-6` fix — an immature spend is no longer persisted before the error returns; it is the **only** copy, the acceptor's duplicate having been deleted under `P2-1`, so there is no second source of truth; and a commitment-set-based variant was considered and **rejected** in favour of per-nullifier heights (`:1127-1131`), which is the class RC5 names. Coinbase transactions are skipped via the shared `is_pow_reward_coinbase_tx` classifier rather than a re-derived predicate. **The gap: no test pins it.** A unit test cannot reach the check cheaply — `seed_canonical` inserts straight into the store and never enters `connect_block`, so exercising it means mining a fully valid block (RandomX, merkle, timestamps) inside a unit test, or extracting the maturity predicate from `connect_block` into a function a test can call with a seeded `nullifier_set`. Until one of those happens the rule is enforced but unpinned, and a future edit that broke it would be caught by nothing — the same class as `C61`'s residuals, and the reason this **The gap is now closed by the second of those routes, 2026-09-23.** The rule was extracted verbatim into `CChainState::check_coinbase_maturity` — a named predicate beside the `nullifier_height` helper it reads, with the two properties it was argued for carried in its doc comment (it is the only copy, and it reads a per-nullifier height rather than a commitment-set lookup, which `P2-9` Item 4 considered and rejected) — and `connect_block` keeps only the *ordering*, which is the real content of that call site: it must precede the sled commit closure. `test_coinbase_maturity_refuses_an_immature_spend` then calls the production predicate directly, with three cases because two of them are what make it evidence: an immature spend is refused naming both heights, a spend **exactly at** maturity is *admitted* (a predicate that rejected everything would pass the first case), and a nullifier the chain has never seen as a claim is left to the replay gate rather than refused here. Verified with the whole crate's lib suite rather than a filter — `cargo test --release -p dwow_chain --lib --all-features` → **270 passed, 0 failed, 8 ignored** — since the extraction touched `connect_block`'s body. The extraction route was chosen over the mining route because it makes the *deployed* rule testable rather than a copy of it row says *how* to close it rather than only that it is closed |
| OBL-C11 | **RESTATED** | No two outputs in one call carry the same commitment | `entrypoint/mod.rs` (mint, transfer) | Rust only | H | **RESTATED 2026-09-22 — the proposition is not what the code does, and the property holds by a different mechanism. Both halves checked before this was written.** The loop the row points at (`native_token/src/entrypoint/mod.rs:769-777`) tests each output only against the **pre-existing on-chain** `commitment_set` — `db_contains_key(commitment_set, &output.commitment.to_bytes())` — and never against the *other outputs of the same call*; there is no set, hash or nested comparison of sibling outputs anywhere in the entrypoint. So two byte-identical outputs in one `TransferV1` both pass that loop, and `apply_transfer` (`:1429-1433`) then `db_set`s the same key twice (second write idempotent) and pushes **two identical leaves** into the commitment Merkle tree. The two "mint" arms the row names are vacuous on this point — `pow_reward_v1` and `uncle_mint_v1` each carry exactly one output (`:1008-1012`, `:1172-1176`) — and `MintV1` is disabled outright (`:712-715`, `:1240-1243`). **What makes the duplicate harmless is not this check but the nullifier derivation plus `OBL-C8`**: `nf = poseidon_hash(DOMAIN_NULLIFIER, secret, coin_hash)` (`src/sdk/src/crypto/nullifier.rs:99-101`) — a function of the spending key and the commitment *only*, with no leaf index or per-output nonce — so two identical commitments are the same note, produce **one** nullifier, and the chain-wide spend gate rejects the second spend. Value conservation is unaffected because both copies are summed like any other output. What the implemented check *does* protect is a different thing: an output whose commitment already exists on chain, which nothing else would catch. **Consequence for the register**: the row was filed against the wrong mechanism (its second instance, after `OBL-C13`), the protection that matters is `OBL-C8`'s, and **neither** is pinned by a test — no test constructs a call with two identical outputs and asserts anything about it. **The half that *is* this row's own — the on-chain duplicate-commitment check in `transfer_v1` — cannot be pinned for the same reason `OBL-C6` cannot**, and that row carries the full record: the check runs in exec, no test in this tree has ever carried a transfer in a block, and the one construction that would reach it needs a proof the transaction no longer possesses after perturbation. One unlock serves both rows |
| OBL-C12 | **PARTLY** | Secret keys are not `Copy`, not `Debug`-printable, zeroized on drop; `derive_instance` is deterministic and scopes keys per `(wallet, contract, instance)` | `src/sdk/src/crypto/keypair.rs` | Rust (compile-time for `Copy`) + Python model | M | **PARTLY — three clauses hold, one is mis-worded, and one carries a real loophole. Each of the four checked separately, 2026-09-22.** **(a) Not `Copy` — holds.** `SecretKey` derives `Clone, PartialEq, Eq` only (`keypair.rs:144`), as does `Keypair` (`:47`); no `impl Copy` exists. (For contrast `PublicKey` and `Nullifier` *are* `Copy`.) **(b) "Not `Debug`-printable" — mis-worded: a `Debug` impl exists and cannot be removed; what holds is that it does not leak.** `keypair.rs:147-151` writes the literal string `<redacted>`, `Keypair`'s redacts its `secret` field (`:53-60`), and the one impl that *would* print the key, `Display`, is gated behind the opt-in feature `unsafe-display-secret` (`:297-306`). So the clause should read "not *printable through* `Debug`". **(c) Zeroized on drop — holds for the owned value, with a loophole.** `Drop` (`:153-167`) hand-rolls `core::ptr::write_bytes` over the raw limbs rather than using the `zeroize` crate, and says why (zeroizing the byte repr would zeroize a copy). But `SecretKey` is `Clone` and `pallas::Base` is `Copy`, so a cloned key, and any copy produced by dereferencing `inner()` — which the live call sites do, e.g. `PublicKey::from_secret` at `:353` — is a separate value whose drop does nothing. The clause is true of the object and false of every copy of it. **(d) `derive_instance` deterministic and scoped — holds, exactly as stated.** `keypair.rs:202-222` hashes `poseidon_hash([DRK_POSEIDON_DOMAIN_KEY_DERIVE, self.0, contract_id.inner(), instance_elem])`, so the triple is the domain-separated input by construction, and it is used on live client paths (`native_token/src/client/pow_reward.rs:87` and the game contracts' clients). The instance id is folded into a field element by copying `min(32, len)` bytes into a zeroed 32-byte buffer, and a non-canonical value returns an error rather than panicking. **Coverage: none of the four is pinned by a test.** `keypair.rs`'s test module (`:607-660`) covers address encoding only — no determinism or scoping assertion for `derive_instance`, no redaction test, no zeroization test, and no `compile_fail`/`static_assertions` negative test for `Copy` anywhere in the tree, so clause (a) rests on the derive line alone |
| OBL-C13 | **CLOSED** | Mint authority: PN `issue_v1` requires `poseidon_hash(backing_secret)` bound in-circuit, and rejects a stale registry root | `src/contract/promissory_note/src/entrypoint/mod.rs`; `proof/issue.zk` | zkas + `hooks/pre-commit` | C | **CLOSED 2026-09-22 — and its cited enforcement was the wrong mechanism.** The circuit does the work, in the *sound* form: `issue.zk:46-48` derives `derived_mint_public = poseidon_hash(DOMAIN_SIGNATURE_SECRET, backing_secret)`, constrains it equal to `mint_public`, and then **exposes `mint_public`** with `constrain_instance` — derive-and-expose, so the verifier sees the value the witness is pinned to and the prover must know a `backing_secret` hashing to it. Cross-checked against `scripts/check-pubkey-binding.sh` (`PASS: no vacuous bindings`), the detector built to flag the opposite shape. **`hooks/pre-commit` never enforced this**: the hook that existed until 2026-09-22 contained exactly one check — the pubkey-binding rule — and enforced it *backwards*, so it contributed nothing to mint authority and would have rejected the sound pattern had the file been edited. Anyone reading the row's citation as "the hook guards mint authority" should read this instead |
| OBL-C14 | **SATISFIED** | The number of distinct L1 operation combinations over C contracts is `∏(nᵢ + 1) − 1`, hence `≥ 2^C − 1` for `nᵢ ≥ 1` — exponential in the number of contracts, and a **product**, not a sum | `proofs/lean/src/DarkFi/Combinatorial/Combinations.lean` | **proved.** `two_pow_sub_one_le_combinationCount` (exponential floor), `increment_ge_value` (`f(C+1) ≥ 2·f(C)`: the increment is at least the accumulated value, so no constant can be its increment), `combinationsIncludingIdle_append` (appending multiplies by `n+1`), `combinationCount_gt_sum` (refutes the additive reading at every `C ≥ 2`), and the instance `615192791076863999999999` over 31 contracts / 166 circuits by `norm_num` | M |
| OBL-C15 | **SATISFIED** | The *size* of one composed capability is additive — barbs compose by union, so ` | ⋃_{c∈S} B c | ≤ Σ_{c∈S} | B c | ` | `Capability/Composition.compose`; `ocap.md` §5.1 containment | **proved.** `Combinations.card_biUnion_le_sum`. This is the additive law that bounds blast radius, and it is a *different quantity* from OBL-C14 | M |

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
combinatorial explosion" (`privacy.md` §6, `safety.md`'s L1 combinatorial bound,
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

| ID | Status | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-Z1 | **MECHANIZED** | For every circuit and every `constrain_instance(X)`: `X` is either a pure opcode expression over witness bindings, or bound by `constrain_equal_base(derived, X)` before the expose, or **redundant** with another exposed determination, or a **declared** free witness | every `.zk` under `src/contract/*/proof/`, `proofs/core/`, `bin/darkirc/proof/` | **mechanized** — `script/circuit_instance_derivation.py`, gated by `scripts/check-circuit-instance-derivation.sh`. As measured 2026-09-22, of **897** instances over **181** circuits: 365 derived, 201 bound, 54 derived-inline, 226 redundant, 36 declared free, **15 unclassified** — down from 33 after the 2026-09-20 triage, which added the entries with a readable host mechanism in `script/circuit_free_instances.txt` and left the ones that have none (OBL-Z16). The figures in this row are re-run rather than recalled; the gate is the authority and the counts move whenever a circuit does | C |
| OBL-Z2 | **FAILS** | Each circuit's public-input metadata matches its `constrain_instance` set — **position for position**, not merely in count | `scripts/check-circuit-metadata-alignment.sh` vs the entrypoint's `zk_inputs.push` | the script, now **three-way** (circuit / metadata / client `to_vec`) and covering **11** contracts. Two blind spots closed on 2026-09-20: entrypoints at `src/entrypoint.rs` and `src/entrypoint/mod.rs` are now found (oracle and bearer_bond were skipped), and the namespace is resolved by the circuit's identity string rather than its file name — which had been reporting `native_token/fee` and `set_transparency_level` as missing pushes that are present. It was **FAILING** on `bearer_bond/blind_output` (7 `constrain_instance`, 5 pushed) and `bearer_bond/redeem` (8 vs 6); both were repaired on 2026-09-20 (OBL-Z15), the declared-exception list `script/circuit_metadata_exceptions.txt` is empty, and the gate now passes on all **68** circuit/site pairs it walks | M | **FAILS — re-measured 2026-09-22: the gate is RED with 19 findings over 19 circuits.** The "passes on all 68 pairs" claim was true *of the scope the gate walked*, and that scope was the problem: the gate checked **11 of 32 contracts**, which is exactly the defect `OBL-C79` names, and at 20:16 today the other session fixed it (`1b749224a9`, "metadata gate: it checked 11 of 32 contracts and reported PASS"). Re-run after that change, `scripts/check-circuit-metadata-alignment.sh` exits 1: **drain_protection 8** (`execute`, `initialize`, `lock`, `propose`, `transfer`, `unlock`, `update_config`, `vote` — each "5 constrain_instance vs 2 pushed values"), **tender 5**, **insurance_market 4**, **lottery 1**, **subscription 1**. **These are newly visible, not newly introduced**, and that distinction was checked rather than assumed: the coverage change is what moved, `drain_protection` has not been touched since 09-21, and the failing shape ("5 vs 2 pushed values") is a pre-existing vector mismatch that no earlier run could see because those contracts were outside the walk. This makes the gate the **second** blocking red gate in `run-all-tests.sh` alongside `OBL-Z1`/`Z16`, so "the full gate is green" is two obligations away and the register said one. **`Z2` and `C78` are the same proposition from two surfaces** — the live rows for this class are the other session's `C78`/`C79`, and this row is corrected here rather than re-litigated |
| OBL-Z3 | **OPEN** | Every `poseidon_hash` call in a circuit is domain-separated, and by the *right* constant | `scripts/check-circuit-domain-separation.sh` | the script checks that *some* `DOMAIN_`/`witness_base` prefix is present, never that it is the correct one for the hash's purpose | M | **OPEN — re-verified by reading the gate itself, 2026-09-22.** The check is a substring test over a call's argument text: `re.search(r'DOMAIN_ | witness_base', call_args)` (`scripts/check-circuit-domain-separation.sh:56`), so a call is accepted if the token appears anywhere in its arguments and is never compared against the purpose the hash is being used for. The script's own success text enumerates the intended mapping (`:69-75`, `DOMAIN_NULLIFIER = witness_base(1)` … `DOMAIN_SIGNATURE_SECRET = witness_base(7)`), which is exactly the table a correctness check would need and does not consult. The row's named instance stands: `oracle/proof/push_value_commitment.zk:60-61` derives two different purposes from `witness_base(4)`. This is one of the three gates the register names as checking presence rather than correctness (`Z3`, `Z4`, `Z8`) |
| OBL-Z4 | **RESTATED** | A pubkey derived by `ec_mul_base` + `ec_get_x/y` is bound by `constrain_equal_base` before being exposed | `hooks/pre-commit` | the hook — line-anchored, single-assignment, staged files only, and it cannot see an inline `constrain_instance(ec_get_x(pk))` | C | **RESTATED 2026-09-22 — the proposition as written stated the anti-pattern, not the rule.** The row states the rule as *"a pubkey derived by `ec_mul_base` + `ec_get_x/y` **is bound by `constrain_equal_base`** before being exposed"* — and that is verbatim what the old hook enforced, which `hooks/pre-commit`'s own header now records as **wrong**: *"It required that a value derived by `ec_get_x` be bound by `constrain_equal_base` before being exposed."* The sound form is the opposite — derive-and-*expose*: `pub = ec_mul_base(secret, K); constrain_instance(ec_get_x(pub))` — because a `constrain_equal_base(A, B)` whose operands are both prover-chosen, neither a literal, a constant, nor exposed, is satisfied by *any* witness and tells the verifier nothing. So this row, followed literally, would have rejected correct code: the old hook would have failed a commit touching `proofs/core/{burn,opcodes}.zk`, and never fired only because those files were not edited while it existed. **Superseded by `OBL-Z18`, which states the same rule in the sound direction**; the mechanism the row cites is now `scripts/check-pubkey-binding.sh` (wired into the hook, **report-only** at 57 unadjudicated candidates, deliberately, since a fully private circuit with no host-side use has nothing to forge). The row's remaining true content is its *detector* limitation — an inline `constrain_instance(ec_get_x(pk))` is invisible to a line-anchored scan — which is a `Z18`-class finding about coverage, not a rule this register should keep in the inverted form |
| OBL-Z5 | **PARTLY** | The deliberately free witnesses are enumerated and each carries its host-side obligation | `script/circuit_free_instances.txt` | **partly. 36 entries**, each naming the Rust mechanism that constrains it (file + condition), all read rather than inferred. (This row said 43 until 2026-09-22; the "Stale prose" note below already corrected it to 36, and the count is now confirmed twice over — `grep -v '^#' script/circuit_free_instances.txt | grep -c '\S'` returns 36, and the Z1 gate's own tally line reports `declared-free:36`, so the file and its reader agree.) The entries added on 2026-09-20 came in three kinds and the file says which each is: **fourteen** id lookups, where the mechanism is a `db_get` → `NotFound` in the host; **two** values the host anchors elsewhere (`stablecoin`'s `outstanding`, re-derived from the config DB, and `collateral_ratio_bps`, pinned to the amounts the host checks); and **five** where *no mechanism exists* and the entry says so — `bearer_bond`'s coverage report and dex's fee, which are deliberately free rather than checked, and which is why this row reads `partly`. The `tx_nonce` case turned out not to need an entry at all — see "the redundant class" below | M |
| OBL-Z6 | **PARTLY** | The Orchard-tree hash is **Sinsemilla**: 10-bit altitude ‖ two 255-bit halves under `"z.cash:Orchard-MerkleCRH"`, depth 32, empty leaf **2** | `src/zk/vm.rs` (`MerkleRoot`) → `MerklePath`/`MerkleNode::combine`; `src/sdk/src/crypto/sinsemilla.rs` | **partly closed.** `HashOps.{merkleDepth, orchEmptyLeaf, sinsemillaCrh, computeMerkleRoot}` now carry the altitude in the CRH domain and use depth 32 / empty leaf 2, and `merkle_root_change_detection` is **proved** by induction rather than assumed. The remaining gap is the *primitive*: `sinsemillaCrh` substitutes the model's hash for Sinsemilla | M |
| OBL-Z7 | **SATISFIED** | The SMT root is rate-2 Poseidon with **no** domain prefix, depth 255, empty leaf **0** — sharing neither primitive nor constants with the Orchard tree | `src/zk/gadget/smt.rs`; `src/sdk/src/crypto/smt/` | **closed in model.** `HashOps.{smtCrh, smtDepth, smtEmptyLeaf}` state the raw-pair Poseidon and the distinct constants, and `smtCrh_injective` is **proved** from `poseidon_collision_resistance` — no substitution needed, because the SMT really does use Poseidon | M |
| OBL-Z8 | **OPEN** | ZK binaries are well-formed | `scripts/validate_zk_bins.sh` | the script — structural validity only, not that `.zk.bin` matches the current `.zk` source | M | **OPEN — re-verified by reading the gate itself, 2026-09-22.** Validation is `"$ZKAS" validate "$bin"` (`scripts/validate_zk_bins.sh:28`), i.e. the compiler's own structural check of the compiled artefact; nothing in the script compares the `.zk.bin` against its `.zk` source, and nothing hashes either. The `rebuild` mode reads the source only to *regenerate* a binary the validator already called corrupt (`:39-55`) — it is a repair path, not a comparison. So a `.zk.bin` that is structurally valid but was compiled from an older revision passes this gate, which is the class the register names alongside `Z3` and `Z4`: gates that check presence or shape rather than correctness. Contrast `scripts/check-artifact-freshness.sh`, which does hash contract sources against a recorded digest |
| OBL-Z12 | **OPEN** | A circuit that proves a value is the *integer quotient* of two others proves it about the integers those field elements stand for, not about their residues | `BaseDivGadget.qr_unique` and the five repaired quotient-remainder circuits | **the Lean layer states the hypothesis and nothing on chain establishes it; the syntactic half is now mechanized.** `qr_unique : 0 < d → q*d ≤ n → n < (q+1)*d → q = n/d` is a theorem about `Nat`; the circuits constrain the same inequalities over `ZMod p`. The bridge is the operand bound — every operand `range_check(64)`ed makes every product `< 2^129 < 2^253` — and `BaseDivGadget.qr_needs_bound` is the kernel-checked counterexample showing the bridge is *necessary* (with `n = 50000`, `d = 2`, `q = (p+49999)/2 ≈ 1.45e76`, both comparisons are satisfied modulo `p`). `script/circuit_instance_derivation.py` now also enforces the syntactic rule — every operand of a `less_than_*` must rest on range-checked witnesses. It first reported 15 operands in 10 circuits; five of those were the check's own artefact (it counted only range checks that *precede* the comparison, and a circuit's constraints are a conjunction, not a prefix), and the rest were repaired or removed on 2026-09-20 — `bin/darkirc`'s RLN rate limit was **live** (an unbounded `message_id` mints unbounded internal nullifiers, defeating the slash mechanism) and is bounded; `dao_escrow`'s premium-before-expiry check was the only enforcement of its property and is bounded; `bridge`'s anti-dust comparison is bounded but its floor is still prover-chosen (OBL-Z16); the escrow/otc_swap timelocks were redundant with the host's own read of chain state and were deleted rather than kept as a second, weaker claim. What remains is one declared exception, `proofs/core/lead.zk`, whose operands are full-width by design (`script/circuit_comparison_exceptions.txt`, which requires a register ID per entry). What is **not** proved is that the 253-bit chip plus `range_check` *implies* the integer reading: that composition is the obligation, and it is open | M |
| OBL-Z13 | **PARTLY** | A governance datum the reports are *about* is either derived from chain state or is recorded as the reporter's claim | `bearer_bond/proof/prove_coverage.zk`; `stablecoin/proof/governance_report.zk`; `dex/proof/execute_swap_fee.zk` | **partly.** The six circuits changed on 2026-09-20 expose the values they compute (`coverage_ratio_bps`, `collateral_ratio_bps`, `fee`) instead of leaving them as witnesses the host never checked — closing a hole in which the on-chain solvency ratio was instruction data proven by nothing. What remains is the *anchor*, and it differs per contract: stablecoin's amounts are compared against the config DB (`entrypoint.rs:1363-1368`), so its exposed ratio is a function of chain state; bearer_bond's reserve amounts are off-chain and nothing can anchor them, so its coverage report is an **issuer attestation** and `is_coverage_voided` is an issuer-controlled switch; dex's fee is proven and read by no one. Since 2026-09-20 the entrypoint checks `fee_bps <= 10000` (mirroring the circuit) and nothing else, and the reason is structural rather than an oversight: `ExecuteSwapFeeParams` carries no `fill_amount`, so the host cannot recompute the fee the circuit proves; no dex entrypoint validates a child *value* commitment, though promissory note exports `validate_child_value_commit` (`validation.rs:46`) and stablecoin already uses it; and no document names a fee recipient — `dex.md` calls the child function `otc_swap_v1 (0x05)` while the entrypoint requires `0x04` (`transfer_v1`). Binding the fee is therefore a design decision, not a patch. All three are in `script/circuit_free_instances.txt` with the mechanism — or the absence of one — stated | M |
| OBL-Z15 | **CLOSED** | A note commitment means one thing across the circuit, the client and the host | `bearer_bond/proof/{blind_output,redeem}.zk`; `commitment` handling throughout `bearer_bond` | **CLOSED 2026-09-20, and it was four objects with one name.** Found by reading: the circuit's `coin` is `poseidon_hash(4, coin_public, value, asset_id, spend_hook, user_data, commitment_blind)` — the same seven-argument hash promissory note's circuits and `CapCommitment` use; `CommitmentAttributes::to_commitment` was the same hash **plus `maturity_block`**, a local addition to the shared convention, so the client's commitment could never equal the circuit's `coin`; the host's metadata pushed `params.commitment.token_commit` (a third hash, over a different domain) where the circuit exposes the note commitment, and pushed `token_commit` twice while omitting the tx pair; neither the note commitment nor the *receipt* commitment was carried in the params at all, so the host had nothing to push; `redeem`'s client put `spend_hook` where the circuit wants the tx pair and the metadata a third order again; and the tx pair was a literal zero on one side against `poseidon_hash([3, 0, 0])` on the other. **Not exploitable — unusable**: no vector could satisfy verification, so nothing moved. Repaired by porting promissory note's shape: `to_commitment` drops the maturity (which is stored on the record and enforced against the chain height, not committed); `BondCommitment` carries the note commitment and `UnstakeParamsV1`/`EmergencyUnstakeParamsV1` carry the receipt's, because the host cannot recompute either (their preimages hold blinds the holder drew); the metadata pushes the circuits' orders with the right values; the clients compute the honest all-zero tx binding the rest of the tree uses; and `script/circuit_metadata_exceptions.txt` is empty again. The gate compares counts and would not have caught the values — `src/contract/test-harness/tests/bearer_bond_commitment_vectors.rs` pins the values | H |
| OBL-Z17 | **FAILS** | A capability check authorizes the holder it names, and only for the capability it was issued for | `identity/proof/verify_capability.zk`; `identity/src/entrypoint.rs:530-563`; its consumers — `dao_escrow` (`verify_member_capability_v1`), `labor_market` (`accept_job_with_capability`), `tender` (`submit_bid_with_capability`), `insurance_market` (both `with_capability` paths) | **FAILS, and it is the authorization primitive itself.** `doc/src/contract/identity.md:208-213` states the mechanism: *"a ZK proof that a capability exists and has not been revoked. The verifier … learns exactly one bit: 'authorised' or 'not authorised'."* Of that: (i) **the predicate is proved** — `less_than_or_equal(threshold, attribute_value)` pinned to `predicate_result`, both operands range-checked; (ii) **existence is checked by the host, not by the proof** — `db_get(capabilities_db, capability_id)` rejects an unknown id, but `capability_id` is a circuit *witness* that the proof never exposes and never binds to the credential (the circuit computes `computed_capability = poseidon_hash(DOMAIN_NULLIFIER, capability_secret, capability_id)` and then **uses it nowhere**), so the params' id and the proof's credential are unrelated; (iii) **revocation is checked nowhere** — the circuit exposes a nullifier and no host reads it, so a revoked credential verifies exactly like a live one; (iv) **`issuer_pub` and `predicate_result` are never exposed**, so the verifier cannot see which issuer or which predicate the proof was about; and (v) the exec path sets `verified: true` **unconditionally** (`entrypoint.rs:555-562`) — its own comment says *"Possession verified via Box::Take child call"*, and no such child call is validated. So every consumer's gate reduces to: *is there a capability with this id, and did the prover attach a proof about some credential?* — which any caller can satisfy. The consumers add only more declaration: `tender` compares the claimed id against its own config, `dao_escrow` and `labor_market` require a child call **to this same function**, and `insurance_market` checks only that the market *has* a required capability configured. `labor_market`'s routing is the shape the remedy takes — it is the only consumer whose call is even structurally a check. The remedy has four parts, in order: bind `capability_id` to the credential in-circuit (use or expose `computed_capability` and compare it against the registered capability's commitment); expose and check `issuer_pub` against the capability record; make the exposed nullifier a real revocation check (`db_contains_key` on the identity nullifiers tree, the way every other consumable is checked); and implement possession as the comment intends — a `Box::Take` child call, since the credential *is* a box. Until then no capability gate in this repository gates anything, and the `auditor_bond`, `institutional_inv`, `access_tier`, `qualified_provider` and `verified_contractor` gates the documentation lists as working are decorative. None of the affected contracts is in `GENESIS_CONTRACT_NAMES`. | M |

**Stage 1 landed 2026-09-21; this row now stands for what remains.** `verify_capability.zk` exposes the credential's `schema_hash`, `issuer_pub_x/y`, `threshold` and `predicate_result` beside its nullifier (8 public inputs), its dead `computed_capability` binding is deleted, and `process_verify_capability_instruction` — which loaded the capability record and discarded it (`let _cap_data = ...`) — now decodes it and rejects a proof whose schema is not the capability's, whose issuer is not the capability's trusted issuer, whose predicate did not hold, or whose threshold is below the capability's `min_threshold`; the nullifier is checked unspent in the identity nullifiers tree and written by the apply phase. `CapabilityProof` gained `threshold` — without it a caller could pass `threshold = 0` and satisfy any requirement — and `VerifyCapabilityUpdateV1` gained the nullifier, since a check nothing ever makes true is not a check. **Stage 2's binding landed the same day; one part remains.** `verify_capability.zk` now reconstructs the credential commitment from its preimage — issuer, holder, schema, both attributes, the attribute blind, the credential secret and the validity window — with the same two hashes and the same `constrain_equal_base` `issue_credential.zk:41-61` uses, and the predicate is evaluated over the *committed* `attribute_1` rather than over a free witness. `commitment` is exposed, and the host requires it to be the stored credential's, so the proof is about a credential an issuer actually signed; the host's own checks now run against the stored record rather than only the caller's claims. The former free witnesses (`capability_id`, `capability_secret`, `nullifier`, `attribute_value`) are gone — a witness nothing reads is a value the prover chooses.

**What still remains is *possession*, and it is a wiring campaign rather than a check.** The model is decided — single-use, spent on verify — and it is the credential's box that gets spent. But **there is no `children_indexes` anywhere in identity**: issuance puts no box (`apply_issue_credential_update`'s comment claims a `Box::Put` child call that does not exist) and verification requires no take, so the box-based possession model was documented and never built in either direction.

The check itself is small and compiles: require exactly one child call, whose function byte is `Box::Take` (0x02) and whose `contract_id` is the box contract from the info tree, then decode `TakeParams` and require `contents_commit == credential.commitment.inner()`. That last comparison is what makes it possession *of this credential* rather than of any box: `escrow`'s shape — the only in-tree precedent — checks the function byte and the contract id and stops there, so it admits a caller who takes an unrelated box.

**It is deliberately not landed**, and this is the reason: the identity heavyweight test passes today against a chain, and a verification that requires a real take needs a real box in the box tree with a real merkle path. No spec has one — `BoxHarness::take()` is a fixed fixture (`contents_commit = poseidon_hash([100])`, its own root), and box's contract rejects a take whose `expected_root` is not the store's. Landing the host check without that plumbing would turn a verified flow red, which is a worse state than an absent check with the reason written down. `attribute identity` is also only *partly* closed: the predicate is over `attribute_1` and the name is exposed and compared, but nothing maps `CredentialRequirement.attribute_name` to a *position* in the schema, so a capability and a credential must simply agree on which slot the name sits in. Both are the remaining work | C |
| OBL-Z16 | **FAILS** | A policy a circuit claims to enforce is one the prover cannot parametrise away | **15** instances across `oracle`, `insurance_market`, `labor_market`, `bridge`, `roulette`, `stablecoin`, `proofs/core` | **FAILS at 15 sites as measured 2026-09-21** (`scripts/check-circuit-instance-derivation.sh`: 181 circuits, 894 instances, 15 unclassified). This row previously read 19; the drift came from changes landed between this row being written and 2026-09-21 and is **not** attributed here — the number to trust is the gate's, run it. What *is* attributable: the OBL-Z9/Z10 oracle work added no new unclassified instance (`set_oracle_active.zk`'s `is_active` is declared free in `script/circuit_free_instances.txt`) and retired five `oracle_id` manifest entries that had become dead text. Two of the 15 are *live*: `insurance_market/proof/{purchase_coverage,underwrite}_with_capability.zk` expose `required_capability_id` that nothing verifies — the host only checks the market *has* a capability configured, never that the caller holds it, so a capability-gated market is open to anyone (the repository's one working pattern is `labor_market`'s `Identity::VerifyCapabilityV1` child call, `entrypoint.rs:1613-1644`). The rest are inert or prover-parametrised: `bridge/withdraw`'s anti-dust `token_minimum` is a field of the withdraw params, so the prover sets the floor it must clear; `roulette/settle_bet`'s `payout` and `stablecoin`'s `report_timestamp` are exposed and read by nobody; `oracle/aggregate`'s `min_result`/`max_result` are bounded only by each other (its `result`, formerly in this list, is now bound into the operation's nullifier by OBL-Z9/Z10 — that makes an aggregate one-shot per result value; it does not make the bounds anything other than mutual); `oracle/attest_value`'s `threshold` likewise; `oracle/attest_value`'s `attestation_id` is still stored without reference to the attestation contract — the nullifier now binds it to the operator, so an id cannot be reused, but nothing checks it against the attestation contract itself; `labor_market/create_job`'s `attestation_id` likewise; `labor_market/milestone_payment`'s circuit is registered by no dispatch path; `proofs/core/{lead,set_v1}.zk` have no host in this repository. **The gate stays red on exactly these**, which is its design: the exception list admits a mechanism, not an excuse | H |
| OBL-Z14 | **CLOSED** | A report that names a reporter is one that reporter filed | `stablecoin/src/entrypoint.rs:1428`; `stablecoin/proof/governance_report.zk:48-53` | **Audited 2026-09-23: CLOSED, not FAILS.** The circuit now *instances* the reporter pair (`governance_report.zk:61-62`) and the entrypoint rejects a mismatched reporter (`entrypoint.rs:1401`), so the mechanism described below is the state before `e65147af92`. The circuit derives `reporter_public` from `reporter_secret` and constrains it against `reporter_pub_x`/`reporter_pub_y` — two **witnesses the circuit never exposes**. So the equality is between two prover-chosen values and holds for any secret, and the entrypoint neither checks the exposed set (which does not contain them) nor compares `params.reporter_pub` against anything: it is written into the update and stored. Any account can file a governance report and attribute it to anyone. The *content* is now constrained — the amounts must match the config DB and the ratio is pinned to them (OBL-Z13) — so this is attribution rather than value, which is why it is M and not C. The remedy is OBL-Z9's: bind the registration key by commitment + nullifier, per "the constraint on any fix" above, and check it in `process_governance_report_instruction`. | M |
**CLOSED 2026-09-21 (`e65147af92`), by the point-exposed shape rather than commitment + nullifier.** The reporter pair is now `constrain_instance`d, so the circuit's equality is against the verifier's values instead of a witness the prover picks; `InitializeParams` carries `governance_pub_x/y` and `init_contract` stores them under `GOVERNANCE_PUBKEY_KEY` — which until then was defined, imported, and never written or read, so the remedy as first written had nothing to compare against; and `process_governance_report_instruction` rejects a report whose reporter is not that point. Exposing a point is not the reverted static-key repair: that repair put a *registration* key into every operation's instance vector and correlated a principal's whole history, whereas this is the one call that is *about* attribution, compared against the deployer's own authority. The spec fixture had to declare its reporter as the authority, which is the check working. | ~~M~~ C→closed |
| OBL-Z9 | **CLOSED** | A circuit whose purpose is to authorize an actor actually constrains the prover to that actor — by whatever the address model says identifies them, **not** by disclosing a static key | `oracle/proof/{push_value,attest_value,push_value_commitment,aggregate,set_oracle_active}.zk`; `multisig/proof/{sign,finalize}.zk` | **CLOSED for `oracle` 2026-09-21; `multisig` still FAILS.** The circuits constrained `constrain_equal_base(ec_get_x(ec_mul_base(secret, K)), pub_x)` where `pub_x` was a **witness the circuit never exposes** — an equality between two prover-chosen values, holding for *any* secret — and the entrypoint could not check either, because `PushValueParamsV1` carried no key. The oracle half now registers a hiding commitment `H(witness_base(4), oracle_secret, oracle_id)` in place of a public key, re-derives and exposes it on every operation, and the host compares it against the stored record; the four push/attest circuits additionally consume a per-operation nullifier. **Nothing static is disclosed**, which is the whole point of the remedy — see "The constraint on any fix" below. `multisig/proof/sign.zk` is untouched: its `params.signer_pub` is still instruction data the proof does not bind, so a non-member can still claim a member's key — see OBL-Z11 | C |
| OBL-Z10 | **CLOSED** | Every oracle state change is authorized by the registered operator | `oracle/src/entrypoint.rs` (`push_value_v1`, `attest_value_v1`, `aggregate_v1`); `oracle/proof/*.zk` | **CLOSED 2026-09-21.** Following OBL-Z9: `push_value_v1` looked the oracle up, checked `is_active`, and did `oracle.value = params.value` — nothing else — and `set_oracle_active_v1` had **no circuit at all** (*"Non-ZK function, no public inputs"*), its only check being `oracle.oracle_pub != params.oracle_pub`, on a prover-supplied copy of a **public** key, which anyone could pass to deactivate any feed. Now every state-changing arm calls `authorize_oracle`, which requires the exposed `oracle_commitment` to equal the stored record (`NotAuthorized`) and, for the four push/attest arms, the operation's nullifier to be unspent (`DuplicateNullifier`). `set_oracle_active` gained a circuit. Verified by two rejection endpoints in `tests::heavyweight_oracle` — the verbatim causes are in the "Done for `oracle`" note below | C |
| OBL-Z11 | **CLOSED** | A signer's nullifier is spendable only by that signer | `multisig/src/entrypoint/mod.rs`; `multisig/proof/sign.zk` | **CLOSED 2026-09-22**, and it was more severe than this row first recorded. It read "a non-member can claim any member's key, pass the check, and spend that member's nullifier for the message — blocking the member from signing it". Blocking was the *lesser* consequence: the attacker repeats over the group's other members, every claim passes because the check compares a caller-typed key against the stored list, and `FinalizeV1` counts the recorded nullifiers toward the threshold. **A non-member could forge threshold approval outright.** The group now stores a hiding commitment per member (`H(witness_base(4), member_secret)`), `sign.zk` derives and exposes it along with `nullifier = H(witness_base(1), member_secret, group_id, message_hash)`, and the host checks the commitment against the group's set and the nullifier unspent. `FinalizeV1` can no longer recompute a member's nullifier — it is derived from a secret the host does not hold — so it now takes the approvals from the caller and verifies each against a signature record for *this* group and message | C |
| OBL-Z18 | **OPEN** | Every `constrain_equal_base` has at least one operand the verifier can see — a literal, a named constant, or a value exposed as a public input | **57 sites across 12 contracts**, detected by `scripts/check-pubkey-binding.sh`; the largest concentrations are `stablecoin` 13, `subscription` 8, `dex` 7, `escrow` 6, `purse` 5, `native_token` 5, `otc_swap` 4 | **NEW 2026-09-22, and this row is the general form of Z9 and Z14 rather than a new class.** Both of those state the defect in prose — Z9: "an equality between two prover-chosen values, holding for *any* secret"; Z14 is a named instance. This is the same rule applied mechanically: if neither operand is a literal, a named constant, or exposed, the prover chooses both and the constraint is satisfied by any witness, so the verifier learns nothing. It was found by writing the rule down — the check that existed (`hooks/pre-commit`) enforced **the opposite** and would have rejected the derive-and-expose form that Z9's remedy adopted (see that hook's header for the worked example, `oracle/proof/push_value.zk:53`, which keeps the old form as a comment annotated `-- witness == witness`). **The 57 are not yet adjudicated**, and the row does not claim they are all defects: the detector treats a value as "seen" only when it is a `constrain_instance` target, so a circuit that is fully private by design — no host-side use of the value — has nothing to forge and its comparisons may be harmless. Each site needs its circuit *and* its host read together, which is why the detector is report-only in the hook. One is read and is live: `subscription/proof/verify_access.zk:33-34` compares `derived_pub_x/y` against the witnesses `subscriber_pub_x/y`, while `subscription/src/entrypoint.rs:169-170` consumes exactly those params-side fields — so the host trusts a value nothing authenticated. Until the sweep is adjudicated the hook **reports and does not block**, which is recorded rather than quiet: a gate that fails on unadjudicated findings is a gate whose authority cannot be defended yet | M | **The sweep is still open, and a first pass on 2026-09-22 produced the rule it needs plus one problem with this row's own worked example.** *The rule*, calibrated on the one contract the row already classifies as held-elsewhere, and stated so the next pass need not re-derive it: a flagged `constrain_equal_base` is **not** a defect when either (a) the same relationship is enforced by the host on public/params data, or (b) it is enforced by the circuit elsewhere through an exposed value, or (c) violating it can only harm the violator. *The calibration*: `native_token/proof/mint.zk:62-63` binds `pk_x/y` to the witnesses `public_x/y`, neither exposed — but the note's owner is the only party who can spend it, so a wrong pair yields an unspendable note and harms nobody else; and `mint.zk:118`'s `effective_plus_pin == value` is enforced for real by the *host* on the plaintext params (`validation.rs:455-460` plus `pow_reward_v1:1000-1006`, `OBL-C4`), so the circuit's copy is decorative rather than load-bearing. Both are correctly *not* defects, and the rule above recovers both. *The problem*: the row's single adjudicated-live site cites `subscription/src/entrypoint.rs:169-170`, and reading it, those lines are inside **`UpdateUsageV1`'s `derived_id` recomputation in `get_metadata`** — where the host *publishes* `derived_id` as an expected public input for the update circuit — not the access path at all. `VerifyAccessV1` has **no host state update whatsoever**: its `process_instruction` arm only logs (`:230-232`), and its `process_update` arm likewise (`:273-275`). So for `verify_access.zk` the "the host trusts a value nothing authenticated" argument does not hold as written, and the site needs re-adjudicating *with the credential lifecycle traced* rather than by its citation. The adjacent host path that does carry weight is `update_usage_v1` (`:605-630`), which recomputes an expected nullifier from the **params-supplied** `subscriber_secret` and compares it to the params-supplied `spent_nullifier` — a different question from the one this row asks, and the one to follow next. Recorded rather than resolved: I read four files here and the classification of even one site turns on `Subscription::compute_nullifier` and what the stored record binds, which is a tranche of its own, not an aside |

#### The constraint on any fix: no static key, in or out

An attempted repair — exposing the registered `oracle_pub` (and the multisig member key) as a public
input so the host could compare it — was **reverted**, and the reason belongs on the record because
it is the obvious repair to reach for and it is wrong here.

`doc/src/arch/verification-hazop.md` and the address model say what the first attempt ignored:
**addresses are cycled per transaction**, derived by `derive_instance(secret, contract_id, instance)`
(`src/sdk/src/crypto/keypair.rs:202-222`), and a **static address is the anti-pattern** — a privacy
break, not a default. Putting the registered key into every `push_value`/`attest`/`sign` instance
vector would disclose a static identity and correlate every operation that principal ever performs.
There is no "public key exposure" in this design; there are cycled addresses and nullifiers.

**The in-model pattern is already in the tree.** `native_token/proof/burn.zk` authorizes without
disclosing anything static:

    signature_secret = poseidon_hash(DOMAIN_SIGNATURE_SECRET, spend_secret, nullifier);
    signature_public = ec_mul_base(signature_secret, NULLIFIER_K);
    constrain_instance(signature_public_x);
    constrain_instance(signature_public_y);

A **per-instance derived key** — from the root secret *and* the instance — with authorization
resting on the nullifier being unspent plus Merkle inclusion, not on the host comparing a key to a
registered one.

**The remedy is chosen: commitment + nullifier, expressed as a capability type.** Decided
2026-09-20; the type-level half has landed, the circuit and entrypoint halves have not.

* the oracle registers a **hiding commitment** to its secret, `H(DOMAIN_ORACLE, oracle_secret,
  oracle_id)`, rather than a public key — nothing static is disclosed and a non-operator cannot
  open it;
* every push or attestation proves the opening in-circuit and consumes a **per-operation
  nullifier** the host checks unspent, exactly as `native_token/proof/burn.zk` does for a coin;
* the operation's **barbs** carry it in the type system, so the same `coversBarbs` obligation the
  other twelve capability types carry applies here.

The type-level half is `Capability.Composition.oracleOperatorType` — resource
`{commit, nullify, prove, dispatch}`, primitives `[commitment, nullifier, dleqProof, contractId]`,
with `coversBarbs := by decide` proved (kernel-checked, as with the other twelve). That the proof
closes is itself the useful check: it says the four barbs the operation exhibits are *exactly* the
four those primitives compose to, with nothing smuggled in.

**Done for `oracle`, 2026-09-21** (OBL-Z9's oracle half and OBL-Z10; `multisig`, OBL-Z11's half, is
still open). In the order they depended on each other:

1. `Oracle.oracle_pub: PublicKey` → `oracle_commitment: pallas::Base`
   (`oracle/src/model/mod.rs`), and the same swap in `RegisterOracleParamsV1`,
   `PushValueParamsV1`, `AttestValueParamsV1`, `PushValueCommitmentParamsV1`,
   `AggregateParamsV1` and `SetOracleActiveParamsV1`.
2. `ORACLE_CONTRACT_NULLIFIERS_TREE` added; `init_contract` creates it.
3. All five circuits re-derive `oracle_commitment = poseidon_hash(DOMAIN_OPERATOR_COMMITMENT,
   oracle_secret, oracle_id)` and expose it; the four push/attest circuits also derive and expose a
   per-operation nullifier bound to the operation's own payload (`value` for push, `attestation_id`
   for attest, `commitment` for commitment-push, `result` for aggregate). `set_oracle_active` got a
   **new circuit** (`proof/set_oracle_active.zk`) — it had none.
4. `authorize_oracle` in the entrypoint compares the exposed commitment against the stored record
   (`NotAuthorized`), checks the nullifier unspent (`DuplicateNullifier`, new error, custom 9), and
   `process_update` marks it spent.

Two design points worth recording, because both were decided rather than discovered:

* **`set_oracle_active` carries no nullifier.** A nullifier over `(oracle_id, is_active)` would
  permit one deactivation and then forbid ever reactivating — bricking a feed whose operator merely
  paused it. Authorization there rests on the commitment opening; `tx_binding` covers replay.
* **The operator commitment and the data commitment in `push_value_commitment.zk` both use
  `witness_base(4)`**, and both are `poseidon_hash(4, a, b)`. This is a domain overlap and is
  recorded rather than hidden. It is not exploitable: the two values are compared against different
  state (stored record vs. params) and are never interchangeable in any check, and a party who does
  not know `oracle_secret` cannot produce the operator commitment at all. The pre-existing
  `DOMAIN_COMMITMENT = witness_base(4)` for a *data* commitment is the half that is arguably
  mis-domained; changing it is a separate decision, not part of this remedy.

Also recorded: the circuit that used to be here compared against `oracle_pub_x`/`oracle_pub_y`, and
those lines were deleted rather than kept — a reader who finds the old note must not find the old
code.

**Evidence.** `tests::heavyweight_oracle` PASS, including two *rejection* endpoints added for this
purpose, because a happy-path-only suite cannot tell an authorization check from its absence — all
six endpoints passed before the fix, when the contract authorized nobody. The two rejections read
verbatim from the run's `DWOW_TEST_LOGS=1` output, each after `Successfully got metadata` (so the
proof itself verified, and the refusal is the host's, not the verifier's):

    [oracle] ERROR: Not authorized: commitment does not match the registered operator
    [DIAG] reconstructed error: Custom(3)
    Block 8 rejected at local height 7: canonical call failed at exec ... call_idx=0 fn_code=0x01

    [oracle] ERROR: Nullifier already spent
    [DIAG] reconstructed error: Custom(9)
    Block 8 rejected at local height 7: canonical call failed at exec ... call_idx=0 fn_code=0x01

`scripts/check-circuit-metadata-alignment.sh` PASS (70 pairs, three-way). The derivation gate
`scripts/check-circuit-instance-derivation.sh` stays red on its 15 pre-existing instances and added
none; the five dead `oracle_id` manifest entries were retired, since the id is now inside an exposed
determination and the checker reads the manifest only for bare-witness exposes.

**Done for `multisig`, 2026-09-22** (OBL-Z11). The same shape, with two differences worth stating.

1. `MultiSigGroup.pubkeys: Vec<PublicKey>` → `member_commitments: Vec<pallas::Base>`, and the same
   swap in `CreateGroupParamsV1`, `CreateGroupUpdateV1` and `SignParamsV1`. The record layout is
   unchanged in shape — N × 32 bytes — so the encode/decode offsets are the same fields.
2. `derive_group_id` hashes the first *commitment* rather than the first *key*:
   `H(commitment_0, threshold, total_keys)`. It has to: the commitment cannot depend on
   `group_id`, because `group_id` is derived from it. **The consequence is recorded rather than
   hidden** — a member's commitment is therefore identical in every group they join, so two groups
   with an overlapping member are linkable by that shared pseudonym. A member who must not be
   linked across groups should use a distinct secret per group. This is the one property the
   oracle's version of the remedy does not share, because there the commitment *is* bound to its
   id.
3. `sign.zk` derives and exposes `member_commitment = H(witness_base(4), member_secret)` and
   `nullifier = H(witness_base(1), member_secret, group_id, message_hash)`.
4. **`FinalizeV1` had to change shape.** It counted approvals by iterating the group's keys and
   recomputing each member's nullifier. That is impossible now — the nullifier comes from a secret
   the host does not hold — so the params carry the approvals and the host verifies each one
   against a signature record that exists, is for *this* group, and is for *this* message. An
   approval repeated in the list counts once. Nothing can be fabricated: a nullifier reaches the
   signatures tree only through a `SignV1` that passed the proof and membership checks.
5. `finalize.zk` exposed `approval_commit`, which also repaired a live inconsistency: the circuit
   computed `H(witness_base(4), group_id, message_hash)` while the entrypoint computed
   `H(group_id, message_hash)` with no domain prefix, and because the value was never exposed
   nothing compared them. The circuit's dead `threshold`/`signature_count` witnesses were removed —
   neither was used or exposed.

**Evidence.** `tests::heavyweight_multisig` PASS. Verbatim from the run's `DWOW_TEST_LOGS=1`
output, both after the proof's public inputs verified:

    [multisig::SignV1] Error: signer is not a member of the group
    [DIAG] reconstructed error: Custom(5)      -- NotAMember, the OBL-Z11 attack
    Block 5 rejected at local height 4: ... fn_code=0x01

    [DIAG] reconstructed error: Custom(7)      -- InsufficientSignatures, 2 approvals vs threshold 3
    Block 5 rejected at local height 4: ... fn_code=0x01

A third rejection, `Custom(2)` (`GroupAlreadyExists`), is the runner's own §3.6 replay check: it
re-submits `spec.endpoints[first_zk_index()]`, which for multisig is `CreateGroupV1`. Expected, and
listed here so the next reader does not read it as a symptom.

`scripts/check-circuit-metadata-alignment.sh` PASS (70 pairs). The derivation gate stays red on its
15 pre-existing instances and added none; the four dead `multisig` manifest entries were retired,
and one of them — `sign.zk : message_hash`, which described the old nullifier — had become *wrong*
rather than merely stale.

**Where the pieces are** (surveyed 2026-09-20, before any of the above was written):

* *There is no shared helper.* `promissory_note` and `bearer_bond` have a `validation.rs` and
  neither contains the nullifier check; the glue is ~4 lines per contract. The canonical shape is
  `native_token`: derive in-circuit (`burn.zk:67`, `poseidon_hash(DOMAIN_NULLIFIER, spend_secret,
  coin)` where the coin is itself derived from the secret, so the nullifier is bound to the secret
  rather than to a free witness), expose (`burn.zk:68`), check unspent (`entrypoint/mod.rs:899-902`,
  `db_contains_key` → `DuplicateNullifier`), mark spent in `apply_burn`
  (`entrypoint/mod.rs:1471-1480`). `purse` and `box` repeat it; the two SDK primitives are
  `Nullifier::new` (`src/sdk/src/crypto/nullifier.rs:99-101`, domain
  `DRK_POSEIDON_DOMAIN_NULLIFIER = 1`) and `db_mark_spent` (`src/sdk/src/wasm/db.rs:185-187`).
* *The oracle has no nullifier tree at all.* `init_contract` creates `info`, `oracles` and
  `attestations` only (`oracle/src/entrypoint.rs:76-104`), so one has to be created there and in
  `src/lib.rs:115-117`. `Oracle.oracle_pub: PublicKey` (`model/mod.rs:118-134`) is where the
  commitment goes, and `set_oracle_active_v1` has **no circuit** — its only check is an equality
  against a prover-supplied copy of a public key (`entrypoint.rs:475-478`).
* *Genesis impact:* oracle **and** multisig are in `GENESIS_CONTRACT_NAMES`
  (`bin/dwowd/src/tests/blockchain.rs:69-72`); stablecoin is not, so OBL-Z14's remedy moves no pin.
* *The warm-up:* OBL-Z14 closes with two lines in `process_governance_report_instruction` —
  compare `params.reporter_pub` against the stored `governance_pubkey`
  (`stablecoin/src/lib.rs:157`, set at init from `params.deployer_auth`) — plus
  `constrain_instance(reporter_pub_x/y)` in the circuit so the comparison is proof-bound. It is
  smaller than the oracle work and touches no genesis contract.

So OBL-Z9's "FAILS" stands, its **remedy is now specified**, and what must not happen is the
disclosure of a static key — that would trade an authorization failure for a correlation failure,
and the second is the one this project's model is built to prevent.

### OBL-Z9–Z11: the residue was not paperwork

The 33 unclassified instances were the checker saying "I cannot tell whether these are safe". That
is a signal, and following it into the entrypoints found three authorization failures — two of them
in **genesis contracts** (`oracle` and `multisig` are both in `GENESIS_CONTRACT_NAMES`).

**The residue, triaged (2026-09-20).** The 33 were walked one at a time, and they are not one
class: **14** turned out to have a readable host mechanism and are now entries in
`script/circuit_free_instances.txt` — mostly id lookups (`db_get` → `NotFound`) and membership
checks, plus two worth naming because they are stronger than the rest: `multisig/sign`'s
`message_hash` is bound into the nullifier the host deduplicates on (`entrypoint/mod.rs:319`), and
`labor_market/accept_job_with_capability`'s `capability_id` is the repository's **only** real
on-chain capability check, a routed `Identity::VerifyCapabilityV1` child call
(`entrypoint.rs:1613-1644`). Two are benign by design (the oracle operator's own `value` and
`threshold` — publishing them *is* the operation; what is missing there is authorization, OBL-Z10).
The remaining **19** are OBL-Z16, and two of them are live — or rather, they *were* the live end of something larger: following `insurance_market`'s two `with_capability` instances into `Identity::VerifyCapabilityV1` found that the capability check every one of these contracts relies on verifies nothing at all. That is OBL-Z17, and it is the reason the insurance holes were recorded rather than patched: the in-tree pattern they would have been patched *with* is the empty one.

Also found in that pass, and tracked separately: `insurance_market`'s three `with_capability`
metadata functions push three public inputs where their circuits expose six and five
(`entrypoint.rs:175,198`) — the same instance-vector misalignment as OBL-Z15, found by reading
rather than by any gate, because the metadata gate covers eleven contracts and this is not one of
them.

**Deliberately not repaired yet**, and the reason is worth a line of its own: making those vectors
agree is a small change, and it is the wrong one *now*. The params cannot supply the missing
`tx_binding`, `tx_nonce` or `required_capability_id` at all (`UnderwriteWithCapabilityParamsV1` has
none of them; its third field is `capability_secret`, which the metadata pushes where the circuit
wants `tx_binding`), so those two circuits cannot be proven today — and OBL-Z17 says the capability
check they gate on authorizes everyone. Repairing the vectors first would turn "cannot run" into
"runs and gates nothing". Repair the capability model, then these.

**The shape, which is one shape.** An oracle or signer circuit authorizes an actor like this:

    oracle_pub = ec_mul_base(oracle_secret, NULLIFIER_K);
    derived_pub_x = ec_get_x(oracle_pub);
    constrain_equal_base(derived_pub_x, oracle_pub_x);     -- witness == witness
    constrain_equal_base(derived_pub_y, oracle_pub_y);
    constrain_instance(oracle_id);
    constrain_instance(value);
    -- `oracle_pub_x`/`oracle_pub_y` are declared `witness` and are NEVER exposed

The equality is between the derived public key and the prover's own witness, so it holds for *any*
`oracle_secret`: the proof establishes "the prover knows some curve secret", not "the prover is the
registered oracle". The link that would make it an authorization — the witness equalling the
registered key — is absent, and cannot be supplied host-side either: `PushValueParamsV1` is
`{proof, oracle_id, value, tx_binding, tx_nonce}`, with no pubkey in it.

**`oracle`.** `push_value_v1`, `attest_value_v1`, `push_value_commitment_v1` and `aggregate_v1` all
look the oracle up by id, check `is_active`, and then write: `oracle.value = params.value`. Nothing
checks the caller. Anyone can push any value to any registered oracle, attest any predicate, or
aggregate to any result. Separately, `set_oracle_active_v1` has **no circuit at all** — the
entrypoint marks it *"Non-ZK function, no public inputs"* — and its only check is
`oracle.oracle_pub != params.oracle_pub`, comparing a prover-supplied key against stored state. A
public key is public data, so anyone can deactivate any oracle.

**`multisig`.** This one is subtler and more instructive, because the check that *is* there is
correct. `mod.rs:303` rejects a signer not in `group.pubkeys` — but `params.signer_pub` is
instruction data, and the proof does not bind it. The nullifier,
`poseidon_hash([group_id, msg_hash, pk_x, pk_y])`, is built from that same claimed key. So a
non-member copies a member's public key into `params.signer_pub`, supplies a proof over its own
secret, passes the membership check, and spends the member's nullifier for that message. The result
is not theft but denial: the real member can no longer sign it.

**What the checker did and did not see.** It flagged the exposed instances — `oracle_id`, `value`,
`predicate`, `threshold`, `attestation_id`, `result`, `min_result`, `max_result`, `group_id`,
`message_hash` — which is exactly the right set of circuits to look at. It did **not** flag the
pubkey witnesses, because they are not exposed, and the rule as mechanized only asks about exposed
values. So the residue pointed at the right place for a reason it could not state. That is worth
recording as a property of the instrument: `unclassified` means "a human must look", and the first
three genesis contracts a human looked at had authorization failures.

**What is still untriaged.** These three account for 8 of the 33 instances (`oracle_id`, `value`,
`predicate`, `threshold`, `attestation_id`, `result`, `min_result`, `max_result` in `oracle`;
`group_id`, `message_hash` in `multisig`). The remaining twenty-odd sit in `tender` (3 circuits),
`insurance_market` (2), `labor_market/accept_job_with_capability`, `bridge/withdraw`,
`baccarat/draw_cards`, `darktoshi_dice/reveal_roll`, `slot/reveal_spin`, `roulette/settle_bet` (whose
`payout` is recorded above as inert), `stablecoin/governance_report` (`report_timestamp` only),
`proofs/core/{lead,set_v1}` and `bin/darkirc/rlnv2-diff-signal`. They are not cleared — the gate
stays red at 33, and this section says what has been looked at rather than implying the rest is fine.

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

| ID | Status | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-T1 | **SATISFIED** | No barb distinction among the primitives is redundant: distinct names ⇒ distinct barb sets, and equal barb sets ⇒ equal names | `Types.lean` (`allPrimitiveTypes`, `typesDistinct`) | `Capability/Pareto.lean` — **proved**, and it was **false** until `↓shard` was added (see below) | C |
| OBL-T2 | **DEFINITIONAL** | Two types are distinct iff their barb sets differ (§2) | `Types.lean` (`typesDistinct`); 10 pair theorems in `Distinction.lean` | `decide` in the Lean; the criterion is *sufficient* for distinctness but does not by itself separate every pair — which is exactly how OBL-T1 failed | C | **DEFINITIONAL, not a theorem — read and recorded 2026-09-22, and the row cannot be closed by a proof because there is nothing to prove.** `typesDistinct` **is** `t1.barbs ≠ t2.barbs` (`Types.lean:268-269`), with `typesEquivalent` its negation (`:271-272`): the "iff" is the model's *definition* of type identity (the bisimulation reading of `type-system.md` §2), so it holds by construction and no edit to the code could falsify it. The content is entirely in the two directions the definition leaves open. *Sufficiency is proved* for the ten pairs the spec names (§8.4): `Distinction.lean:39`–`:124`, each a `theorem` closed by `decide` — the file's own header lists all ten, and `allUnifiablePairsProved` (`:136`) conjoins them. Those ten used to be `native_decide` and are `decide` now, which is why they cost no axioms beyond `Classical.choice`. *Completeness — that no two types which should be distinct share a barb set — is **not** proved, and is measured **FALSE**: `OBL-T9` (13 of 14 capability resources subsumed, and `purse_deposit`/`purse_withdrawal` with identical barb sets). So this row is not an obligation of its own: its sufficiency half is the ten theorems, and the obligation it gestures at is `T9`'s |
| OBL-T3 | **SATISFIED** | The five representations of the barb alphabet agree as intended (doc == Lean == core `BarbId`; sdk ⊆ doc; python == sdk) | `Types.lean`, `src/barb.rs`, `src/sdk/src/capability.rs`, `contrib/model/wallet_model.py`, §1.1 | `contrib/barb_alphabet_diff.sh` — now checks all five, including Python, which was previously asserted rather than checked | H | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-T4 | **SATISFIED** | A "spent" barb is faithfully encoded iff its witness is a distinguished non-empty element (§0.1) | `Combinatorial/NullifierStorage.lean` | **proved** — `markSpent_faithful`, `markSpent_sound`, `markEmpty_not_adds`, `faithful_iff_nonempty`, … | C |
| OBL-T5 | **SATISFIED** | Barb coverage: `requiredBarbs ⊆ compose(primitives)` for each capability type | `Capability/Composition.lean` | **proved** — all twelve by `decide` | H |
| OBL-T6 | **SATISFIED** | If a circuit is derivable for `(r, s)` then `CapabilityType r s` is inhabited | `Capability/Inversion.lean` | **proved**, and **one-directional** — the converse is false (anonymous credentials, blind signatures and MAC tokens authorize with no proof system) | H |
| OBL-T7 | **OPEN** | Every public input of the circuit for `(r, s)` is `constrain_instance`-derived | named as `Axioms.NoFreeInstances` | **nothing.** Uninterpreted, unconsumed — this is OBL-Z1 in type-system vocabulary | M |
| OBL-T8 | **SATISFIED** | Genesis is a pure function of its inputs — no ambient authority, stages are pure transitions, embedded contract bytes are a quoted argument | `doc/src/arch/genesis.md` §"Genesis Is A Pure Function"; `bin/dwowd` `init_genesis` (hard-error on hash mismatch) | the pinned hash in `bin/dwowd/genesis_hash.txt` — an *observable witness*, not a proof; the audit's B-series shows the pin was broken by build non-reproducibility | C |
| OBL-T9 | **FAILS** | The barb alphabet distinguishes the types it claims to: distinct capability types have distinct, non-subsumed barb sets, so a barb set identifies a capability | `Capability/Types.lean` (`allPrimitiveTypes`, 33 barbs); `Capability/Composition.lean` (13 `Resource`s) | **FALSE as it stands, and measured.** Of 13 capability resources, **12** have a barb set contained in the union of the others, so they contribute nothing to a composition the others do not; `purse_deposit` and `purse_withdrawal` have **identical** barb sets; of 16 primitives, **11** carry no barb of their own (`nullifier = {nullify}` ⊆ `intentNullifier = {gate, nullify}`). The consequence: **38** distinct barb-set unions over all `2^13 = 8192` subsets — the type count is *sublinear* in the resources, not exponential. The same defect that made `primitiveTypesAreParetoEfficient` false before `↓shard` was added | M | **FAILS, confirmed by re-measurement on 2026-09-22, with the figures refreshed and one of them not reproduced.** Method, stated so it can be re-run: extract every `requiredBarbs` set from `Capability/Composition.lean` and every `barbs` set from `Types.lean`, then compute subsumption, the identical-set groups, and the distinct unions over all subsets. *Resources — the row's counts are stale by one, and the conclusion holds.* **14** resources are defined (the row says 13; the register's own "stale prose" note already moved this to 14 with `oracleOperatorType`), **13 of the 14** have their barb set contained in the union of the others (row: 12 of 13), and **48** distinct barb-set unions arise over all `2^14 = 16384` subsets (row: 38 of `2^13`) — so the type count remains **sublinear** in the resources, which is the finding. *The identical pair is confirmed, and the model says so itself*: `purseWithdrawResource.requiredBarbs` (`Composition.lean:201`) and `purseDepositResource.requiredBarbs` (`:216`) are both `{spend, commit, nullify, dispatch, denominate}`, and the definition above the second one is annotated *"Purse Deposit (consumable via nullifier, **identical barbs to Withdraw**)"*. Under `OBL-T2`'s definition — `typesDistinct := barbs ≠ barbs` — **a deposit and a withdrawal are the same capability type**, which for two opposite operations on the same purse is the sharpest form of this row's defect. *Primitives — not reproduced, and recorded rather than smoothed.* The row says "of 16 primitives, **11** carry no barb of their own", with `nullifier = {nullify} ⊆ intentNullifier = {gate, nullify}` as its example; **17** primitives carry barbs (the row's 16 is stale by one — 20 `PrimitiveType` definitions exist, three of them — `rawBytes`, `rawFieldElement`, `rawCurvePoint` — are barbless *by construction* and the file says they are not valid DarkWow types), and under the row's own example's reading — one set contained in another's — I measure **6**, not 11: `nullifier` (which its example names, so the reading is the intended one), `contractId`, `funcId`, `merkleNode`, `ownedSecretKey`, `bridgeAddress`. No two primitive sets are equal. The gap between 6 and 11 is recorded, not resolved: the direction of the finding is unaffected, and the row should not be quoted for the count |
| OBL-T10 | **CLOSED** | No proof in the tree is checked by the *compiled code generator* rather than the kernel — i.e. no proof rests on `Lean.ofReduceBool` or `Lean.trustCompiler` | `proofs/lean/src/**/*.lean`; `script/check_lean_axioms.py` check 5 | **CLOSED (2026-09-20).** Every `native_decide` is gone: 15 in `Pareto.lean` and 10 in `Distinction.lean` are `decide` (kernel-checked), the four in `Combinatorial/{CeilingDerivation,ComplexityJump,Limits}.lean` are `decide` / `unfold …; decide`, and the four `-- DECLARED:` lines are removed with them. Measured: **zero** occurrences of `ofReduceBool`/`trustCompiler` in the budget table, over 237 theorems. This is the mechanism by which a budget could silently under-report, so its absence is worth an entry | H |

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

## Surface 1 continued — residues promoted from the remediated corpus (`OBL-C16`+)

The HAZOP findings documents were removed on 2026-09-22: a resolved finding is a lesson, and lessons
live as root causes in `dev/contracts/safety.md`. What could not go were the items those documents
left **open** — `hazop-completion` and `no-partial-hazop-completion` forbid deferring an item into
silence, so each one is promoted here, where a proposition has a home and a status. The rows use the
code surface's numbering because that is what they are; two ZK-surface items the corpus also left open
(`H-13`/`M-11`/`M-15`/`L-3`/`L-4`) are already `OBL-Z16`'s, and the red-team families `RC-A`–`RC-I`
are the root-cause map now stated once in `safety.md`.

**What each row's status means.** *verified 2026-09-22* means the code was read and the proposition is
false today. *carried* means the source document's own status is reproduced without re-verification —
the honest label, not a claim of currency. Where a source document was itself stale, the disparity is
recorded rather than resolved.

**The status vocabulary, stated once (added 2026-09-22, because there was none and its absence had
already cost three mis-counts).** A row's status is one of these words, in bold, and
`scripts/register-status.sh` counts them:

- **`CLOSED`** — was a defect; it is fixed in the code, and the marker cites the commit or the
  enforcement it now rests on.
- **`FIXED`** — the code changed, and where the verification is incomplete the marker says what is
  still owed rather than implying the row is finished (`OBL-C68`: *"FIXED 2026-09-22; the behavioural
  half is still owed"*). **This token has always been enforced by `scripts/register-status.sh` and was
  missing from this list until 2026-09-23** — the guard accepted a word the vocabulary did not
  document, which is the same class of gap as a marker no grep can find.
- **`SATISFIED`** — was an obligation; the code already met it and no change was needed.
- **`RESTATED`** — the proposition as written did not describe the code, so it was re-written rather
  than met. This is *not* a softer `CLOSED`: nothing was fixed, and whatever the row was reaching for
  must be looked for elsewhere. `OBL-Z4`, `OBL-C11`.
- **`ACCEPTED-WITH-REASON`** — the obligation will not be met, and the row says why, with the gap
  recorded rather than narrowed.
- **`OPEN`** — a defect, still present.
- **`PARTLY`** — some clauses hold, and the row names which.
- **`FAILS`** — measured false as written. This is a verdict on the **proposition**, not on the harm:
  a row may be `FAILS` and still have no reachable consequence.
- **`PROVED`** / **`MECHANIZED`** / **`DEFINITIONAL`** — about the model rather than the code:
  a theorem; a checker; or a statement true by construction that no proof can close.

**What the severity column means — stated 2026-09-23, because its absence has now cost an audit.**
The column is the register's oldest field and the only one whose meaning was never written down, and
an adversarial pass over all 107 rows found that it is read one way and used another. It carries two
different meanings depending on the row's *status*, and the reader cannot tell which:

- On a **closed** row (`CLOSED`, `SATISFIED`, `FIXED`, `PROVED`, `MECHANIZED`) the letter names the
  **class of property** the row guards — `C` because its absence would let value be minted, `H`
  because its absence would break a stated property. That is legitimate and useful: it is how a
  reader knows what a closure was worth.
- On an **open** row (`OPEN`, `PARTLY`, `FAILS` with work outstanding) the same letter reads as a
  **live risk**, and it is frequently not one. `OBL-C44` is the clearest case: severity **H** in the
  column while its own prose says *"That is a liveness difference from Bitcoin Core … rather than a
  safety one"* — which by the legend above is `M`.

**The rule from here, so the column can be trusted again.** An `OPEN`/`PARTLY`/`FAILS` row may carry
`C` or `H` only if the row states a **path from an untrusted input to the consequence**, and says
whether that path is demonstrated or merely asserted. Where the row cannot state one, the letter is
`M` and the harm it guards is described in the row's prose instead. `C` and `H` on a closed row need
no path — closed rows are graded by what they protect, not by what is reachable.

**The form is now guarded, and this is the second time the guard was needed before it existed.** Three
markers written on 2026-09-22 used words this note does not list — "STALE" and "CONFIRMED" — or carried
no status word at all, and each was caught by a human reading a diff rather than by anything
mechanical. `scripts/register-status.sh --check` now fails such a row: it examines the last cell of
every row, and if that cell opens with an ALL-CAPS word the word must be one of the tokens above.
Narrow on purpose — last cell, bold span, ALL-CAPS only — so it cannot cry wolf, and negative-controlled
before it was wired into `run-all-tests.sh`: a copy of this file with a coined marker appended to
`OBL-C23`'s last cell fails it, the file itself passes. It guards the *form* of a marker, not the
existence of one; the ten rows that still carry none are listed by the same script in report mode.

Two rules learned the hard way, both already broken once above. **A marker must be greppable**: write
`CLOSED 2026-09-22`, not "CLOSED by reading", because the second form is invisible to the grep that
counts closures (`OBL-C10`). **A fix recorded only in prose is not recorded**: if a paragraph below a
table says a row is fixed, the row itself must carry the marker (`OBL-C68`–`C71`).

| ID | Status | Proposition | Carried from | Sev |
|---|---|---|---|
| OBL-C16 | **PARTLY** | No `ContractId::ZERO` guard leaves a check disabled by default: an unconfigured contract ID is a hard `Err`, not a skipped validation | `H-11` (red-team, HIGH); `RC-F` (structural fix `SC-6`, PARTIAL) | M | **Audited 2026-09-23: PARTLY, not CLOSED** — the classifier tests `Err`-*adjacency*, not guard direction, and **4** sites of the shape `if cid != ContractId::ZERO { validate }` skip the check entirely (`dao_escrow:1762`, `labor_market:959`, `:1546`, `:1638`) while init writes `[0u8;32]` and nothing else ever writes those keys. The 72/20/69 arithmetic reproduces exactly. Originally closed by `dc20369a48` ("fix: HAZOP H-11 — ContractId::ZERO fail-closed in 57 sites across 20 contracts"), and re-measured rather than assumed on 2026-09-22. The premise this row was carried with — "every one of them fail-open: an unconfigured contract ID skips the validation instead of rejecting" — is no longer true of any of them.** Measured: `ContractId::ZERO` occurs **72** times in `src/contract/*/src/**` across **20** contracts (auction, baccarat, betting_stake, bridge, dao_escrow, darkbet_exchange, darktoshi_dice, dex, drain_protection, escrow, insurance_market, labor_market, lottery, otc_swap, pool_stake, relayer_endowment, roulette, slot, stablecoin, subscription) — the row's 72 and 20 are both right. Of those 72, **69 are comparisons that `return Err`** (`if promissory_note_cid == ContractId::ZERO { msg!(…"not configured"); return Err(…) }`, e.g. `bridge/src/entrypoint.rs:274-277`, `:594-596`; `subscription/src/entrypoint.rs:326-328`), **2 are deploy-path or struct defaults rather than guards** (`labor_market/src/entrypoint.rs:101`, `stablecoin/src/entrypoint.rs:144`), and **1 is a comment**. **Zero fail-open sites**, measured with a four-line window and re-checked with an eight-line window; the classifier is mechanical, so three sites were read by hand across three contracts and all three matched. Note the arithmetic the fix commit explains: 57 sites were fail-closed when `dc20369a48` landed, 69 are now — the class grew with the contracts and grew *into* the closed form. The row's own parenthetical also holds: no genesis contract appears in the list, so this needed no re-roll |
| OBL-C17 | **SATISFIED** | The WASM non-deterministic-feature scanner rejects threads and atomics (`0xFE`), with Rust-stdlib-aware filtering rather than by being disabled | `H-7` (red-team, HIGH); red-team `RC-I` | H | **SATISFIED — by a different mechanism than the row's wording asks for, and *unpinned*. Read 2026-09-22.** The scanner is live: `reject_nondeterministic_features` runs before module compilation (`src/runtime/vm_runtime.rs:354`, called on every instantiation). It rejects the first byte being `0xFD` **or** `0xFE` (`:310-322`) — i.e. SIMD as well as threads/atomics, which is *stricter* than the row requires. "Rust-stdlib-aware filtering" is realised differently than the phrase suggests, and the function's own doc says how (`:268-285`): the scanner reads **only the code section**, through wasmparser's `OperatorsReader`, so opcode-prefix bytes are distinguished from operand data — the earlier byte-by-byte scan's false positives (a `0xFE` inside a LEB128 immediate, named there as deployooor offset 2146) are what had disabled it, so the stdlib problem was solved by parsing rather than by filtering. Scalar float opcodes (`0x8A..=0xBF`) are deliberately **not** rejected, on the stated ground that they are IEEE-754 deterministic within a backend (`:266-267`, `:284-285`). **The gap: no test pins any of it.** A repo-wide search finds `NonDeterministicWasm` only in the error enum and in this function's own body — no test constructs a module with `0xFE` and asserts rejection, so the scanner **was** enforced but unpinned — **closed 2026-09-22** (`bbb33f2bc7`): four tests on hand-built modules assert `0xFE` and `0xFD` rejection by their own error text, an empty module accepted, and — the control without which the two rejections mean nothing — a scalar float opcode **accepted**, since the function's own doc says floats are deliberately allowed and a scanner that refused every module would otherwise pass both negatives. **And two comments in the file disagree about what is rejected**: the call site says "floating-point, bulk memory, SIMD" (`:351-353`), while the function says SIMD and threads/atomics only and explicitly permits floats; `0xFC` (bulk memory) is not checked at all. The function's doc block is the accurate one — the discrepancy is recorded here rather than fixed by editing the comment, because a comment-only change to a runtime file during a campaign that has just re-rolled the genesis pin is not worth the diff, and the call-site comment is the one that is wrong. (Note the campaign's freeze is `src/zk/vm.rs`; this is `src/runtime/vm_runtime.rs`, a different file, and it is *not* off-limits — the discrepancy is a choice, not a constraint) |
| OBL-C18 | **RESTATED** | Chain traversal in `get_next_work_required` is bounded, or the cache that makes it O(1) exists | `M-1` (red-team, MEDIUM) | M | **Audited 2026-09-23: RESTATED, not SATISFIED** — "bounds the traversal by `TIMESTAMP_WINDOW`, not by genesis" is true of the *cache-hit* path only. The full genesis walk survives at `src/linear/src/consensus.rs:423-448` on a cold cache or a missing/zero entry, so the row's disjunction is satisfied by its second clause and the evidence sentence needs the scope "on the cache-hit path". **Originally: SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below.** |
| OBL-C19 | **OPEN** | A call that exceeds its wall-clock budget is terminated, not merely warned about | `M-13` (red-team, MEDIUM) | M | **OPEN — confirmed, and the code states the gap in its own words rather than leaving it to be inferred. Read 2026-09-22.** `src/runtime/vm_runtime.rs:774-788`: the call is timed (`std::time::Instant::now()` before `entrypoint.call`, `:777-778`) and on return `MAX_WASM_CALL_TIME` (30 s) is compared — but the overrun branch is a `tracing::warn!` and nothing else, with the comment saying why: *"MAX_WASM_CALL_TIME is a soft limit — exceeded calls log a warning but don't fail. Hard enforcement would require cooperative yield in the WASM metering middleware."* So the row's "terminated, not merely warned about" is **false today**: it is warned about and the call completes. The exposure is the one the neighbouring comment names — *"Gas metering bounds instruction count but not wall-clock time. A contract with 400M expensive opcodes can consume unbounded CPU"* — which is a real bound because the cost function charges per *instruction* (`:356-366`, tiered at 1/2/4/8/256) while the expensive work is in the operations themselves; the bridge's BN254 pairings and the Keccak/SHA-256d helpers are named there as 10-100× undercharged. Closing it needs a mechanism, not a constant: cooperative yield in the metering middleware, or an out-of-band kill of the Wasm instance. Recorded as open with the obstruction named, since the row is achievable and only the mechanism is missing |
| OBL-C20 | **OPEN** | Public-input ordering is verified by parse-and-compare, not by count | `M-5` (red-team, MEDIUM); partially `OBL-Z2`, which now compares position for position across 68 pairs. **OPEN — this row was recorded as satisfied on 2026-09-22 and is not**: the gate it cited (`check-circuit-metadata-alignment.sh`) compared counts, over 11 of 32 contracts. The ordering comparison now exists and is advisory; see `OBL-C78`/`OBL-C79` | M |
| OBL-C21 | **OPEN** | Bridge verification is real cryptography at every chain, not a shape check: Monero DLEq, Ethereum MPT, Zcash Groth16, Aztec PLONK all verify rather than returning "not yet implemented" | `C-1`, `C-3`, `C-4` (red-team, CRITICAL, all PARTIAL); `RC-A`'s structural fix `SC-1` (`Verified<T>`) not implemented | C | **OPEN — the live half re-confirmed 2026-09-22, and it is narrower and sharper than "not yet implemented": the contract contains two different policies for its own disabled state, and the reachable one is the weak one.** *The live hole, read whole rather than grepped.* `entrypoint.rs:320-333`, the `#[cfg(not(feature = "bridge-verify"))]` arm of the deposit path: for `ExternalChain::Ethereum` it rejects only when `params.merkle_proof.is_empty()` and otherwise **falls through and accepts**, while the `else` rejects every other chain with "verification not compiled". (An initial grep-based read of this block attributed the rejection to the Ethereum branch and would have concluded the opposite; the branch nesting is what decides it.) *The inconsistency.* The dispatch the entrypoint does **not** call when the feature is off is fail-closed at every arm: `verify_chain_proof` (`bridge/src/verify/mod.rs:40-90`) returns `Err(InvalidDeposit("… not compiled"))` for Ethereum, Monero, Zcash, Aztec and Litecoin alike. So the contract rejects all five chains in one place and accepts one chain in another, and the accepting one is on the path a deposit actually takes. *The feature is enabled by nothing*: `bridge-verify = []` is declared (`bridge/Cargo.toml:51`) and no manifest, script or Makefile turns it on, so `mod litecoin`, `mod monero`, `mod zcash` and `mod aztec` are absent from every build (the row's "written and never compiled" is exactly right for those four), while `ethereum` is ungated and present but reachable only through the gated arm. *The prerequisite is unanswered*: the Zcash Groth16 and Aztec PLONK arms cannot compile at all — no pairing dependency exists in the tree (`vendor/` holds `halo2` and nothing of the `ark-*` family) — so the feasibility question the plan names (does a pairing crate build for `wasm32-unknown-unknown` and fit the declared block capacity?) must be answered *before* those verifiers are written, and a negative answer is a design finding rather than an obstacle to work around. *Order of work implied*: make the off-branch fail closed as the dispatch already does, then answer the pairing question, then write and compile the verifiers. **That order is wrong, and reading the consequences rather than the code is what shows it — corrected 2026-09-23 before the change was made.** The first step cannot precede the second, because *nothing can build `bridge-verify` today*: the feature gates `monero`, `zcash` and `aztec` alongside `litecoin`, and the two zk verifiers need a pairing dependency that does not exist in the tree, so turning the feature on fails to compile. Making the off-branch fail closed would therefore not "close a hole pending the verifiers" — it would make **Ethereum deposits impossible in every configuration that can be built**, because the Ethereum MPT verifier is ungated but reachable only through the gated arm. And something depends on the permissive path: `bin/dwowd/src/tests/specs/bridge_spec.rs:170` deposits on `ExternalChain::Ethereum` through the test harness, so the change would move a red heavyweight test's failure earlier and leave its subject unsatisfiable. **The two halves are one decision, not two steps**: either the bridge is deferred (status quo, with the permissive arm and the hole both named, which this row does) or the verifiers become buildable (which needs the pairing feasibility answered first, and can come out negative). Recorded as a finding so the next person does not start from "just make it fail closed" |
| OBL-C22 | **SATISFIED** | Reorg recursion (`perform_reorg`) has a depth cap | `M8` (sync-audit) | H | **SATISFIED, re-asked of the successor function as the "premises that moved" note requires. Read 2026-09-22.** `perform_reorg` no longer exists; the live path is `reorg_to_heavier_chain` (`bin/dwowd/src/task/consensus_linear.rs:137`), and it carries an explicit cap: `const MAX_REORG_DEPTH: u64 = 100` (`:128`), enforced at the top of the fork walk — `if walked > MAX_REORG_DEPTH { warn!(...); return ReorgOutcome::Failed }` (`:159-163`), so the walk cannot be driven past 100 blocks by a peer. The cap is not the only bound on that loop and the comment says so (`:148-151`): each fetched block's PoW is validated as the walk proceeds, so a peer cannot inflate chain work with unmined hard targets, and a local `get_block` gap aborts the walk rather than looping. The 100-block limit is also acknowledged on the consensus side, where the general walk is described as bounded by it (`chain_state.rs:1595`). **Gap: nothing tests the bound** — `MAX_REORG_DEPTH` appears only at its definition, its two uses and two comments; the `Failed` branch is unpinned, and `chain_state.rs`'s reorg tests (`:2497`–`:2666`) exercise `detect_reorg`'s signal, not this walk |
| OBL-C23 | **SATISFIED** | The client handshake read has a timeout | `M7.3` (sync-audit) | M | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C24 | **SATISFIED** | `disconnect_block` reverses every tree that `connect_block` writes — the classification predicates in the two paths compared explicitly, not inferred from the uncle-note path alone | genesis-consensus open item 4; `M7` (sync-audit) — the same defect recorded from two directions | C | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C25 | **SATISFIED** | The cumulative-supply overlay is re-derived by the host rather than passively mirrored | `M10` (sync-audit) | H | **SATISFIED — the host recomputes rather than trusting the WASM overlay, and says so at the site. Read 2026-09-22.** `read_cumulative_from_overlay` (`bin/dwowd/src/block_acceptor.rs:588`) carries the `M10` comment and does the work: *"do NOT trust the WASM overlay mirror blindly. Recompute `S_H = S_{H-1} + C_base` from the coinbase's value commitment and the previous supply entry, then cross-check against what WASM wrote. A divergence means the WASM overlay and the host disagree on supply accounting — reject the block"* (`:656-660`). The re-derivation reads the **previous** entry from `chain_state.supply_chain` (`:666-668`) and compares the recomputed value-commit, blind and total supply against the overlay's entry, rejecting on any of the three. So the overlay is a mirror the host checks, not a value it stores on trust — which is what the row asks for. **One precision, checked rather than assumed**: the check is deliberately conditional — it fires only when the coinbase's first call is `PoWRewardV1` whose params decode, and skips silently otherwise (`:661-664`, where a failed decode becomes `None` rather than an error). That is safe because a block whose coinbase params do not decode cannot reach here at all: `validate_block_structure` hard-errors on exactly that decode (`src/linear/src/validation.rs:432-433`), and it runs earlier in `accept_block`. Worth stating because the skip is invisible in this function and a future reordering of those two steps would turn it into a hole. **Gap: no negative control** — nothing constructs an overlay entry that disagrees with the recomputation and asserts the rejection, so the cross-check is enforced but unpinned |
| OBL-C26 | **OPEN** | `uncle_commitment_set` is reconstructible after a restart — persisted, or derived on demand. A restart cannot currently reconstruct it | genesis-consensus open item 3 | M | **OPEN — the premise is true, and the hazard is moot: the set is write-only. Read 2026-09-22.** Both halves verified. **(i) It is created empty on restart.** `chain_state.rs:379-382` constructs `Mutex::new(HashMap::new())` under a `TODO(Phase 2)`, and says why the old restoration was deleted rather than repaired: it read `store.uncles` — which holds JSON-serialized `UncleBlock` values, not `u64` heights — so its `v.len() == 8` filter discarded every entry and "this was always empty". So a restart really cannot reconstruct it. The recomputation it says is possible is indeed possible and deterministic: `r_i = blake3(uncle_hash ‖ u_i ‖ H)` then `C = u_i·G_v + r_i·G_r` is how each entry is built (`:1086-1114`), and `UncleBlock` (`block.rs:184-200`) persists the header, the transactions, `pin_accepted` and `pin_confirmed` — everything the formula needs — while carrying **no** commitment field of its own. Nothing derives it. **(ii) Nothing reads it, so the gap decides nothing.** The only uses of `uncle_commitment_set` in the tree are the insert after a successful sled commit (`:1479-1481`) and the retain-on-reverse in `disconnect_block` (`:1931-1932`); there is no `get`, no `contains_key`, no iteration, no public accessor reading it, and no other file in `src/` or `bin/` names it. `has_commitment()` (`:654-656`) reads `commitment_set`, a different structure. Uncle dedup — the need this set looks like it was built for — is met elsewhere and sled-backed, by `stored_uncle_hashes()` (`:743`, consumed as `existing_keys` by `check_uncles`). **So the row should not be read as a restart-divergence hazard: it is dead state** — memory and a mutex acquisition per block for a map no decision consults — and the fix is a choice between deleting it and implementing the read that would justify it, not a persistence patch. **One stale comment found while checking this**, recorded because it is the kind of thing that makes a reader believe the opposite: `:1084-1085` says the entries are "computed before the closure so uncle commitment batch is included in the atomic sled transaction" — they are not; the only sled batches for that height take blocks, uncles and *claim* commitments (`:1216-1286`), and `uncle_commitment_entries` has exactly one consumer, the in-memory insert |
| OBL-C27 | **OPEN** | `stored_uncle_hashes` compares keys without zero-padding ambiguity: correct only while every key is a 32-byte blake3 hash, which is a fragile invariant rather than an enforced one | genesis-consensus open item 5 | M | **OPEN (latent) — the invariant is real, unenforced, and currently unviolable. Read 2026-09-22.** The function (`chain_state.rs:743-754`) maps each sled key to a fixed `[u8; 32]` by copying `min(32, key.len())` bytes into a zeroed array, so a 30-byte key `K` and the 32-byte key `K‖00 00` become the **same** array — the zero-padding ambiguity the row describes, and it is silent: the copy is an iterator `zip`, not a length check. What bounds the exposure today is that `store.uncles` has exactly one insertion site, `uncles_batch.insert(uncle_hash.as_bytes(), …)` (`:1171`), where the argument is a `blake3::Hash` and therefore always 32 bytes — a *typed* guarantee at one call site rather than an enforced property of the reader. So the row's "fragile invariant rather than an enforced one" is exactly right, and it is worth keeping open rather than closing on the strength of the current caller. **The fix is small but is a semantic change, not a decoration**: reject or refuse a non-32-byte key instead of padding it, so the reader fails closed on a key shape it cannot compare unambiguously. Not done here — a drive-by change to a consensus-state accessor, for a case that cannot currently occur, is the kind of edit this register exists to prevent unless it is batched with a reason |
| OBL-C28 | **PARTLY** | `competing_seen` cannot grow unbounded from network input | genesis-consensus open item 6 | M | **Audited 2026-09-23: PARTLY, not CLOSED** — the `competing_blocks` bound holds, but `competing_seen` does not: both insert paths add the dedup key *before* the per-height cap test (`chain_state.rs:955` then `:964`), and take/prune remove keys only for blocks they actually hold, so an over-cap key has no removal path and the set grows from network input. Originally closed 2026-09-22, and it was two rows deep.** The premise was stale: `prune_competing` is *not* "called only when blocks are taken" — `connect_block` calls it on every block (`:1486`, behind the `pow` feature gate), and it drops every height below `MAX_UNCLE_DEPTH`, so the bound is `MAX_UNCLE_DEPTH × MAX_COMPETING_BLOCKS` and network input cannot exceed it. (A `never used` warning for this method appears in configurations *without* `pow`, where `connect_block` does not exist — that is what made the row look open, and it is a warning about a different build.) **But the bound was defeated in effect**: `prune_competing`'s removals keyed on the serialization hash while the inserts keyed on `hash_with_vm`, so before `OBL-C52` was fixed every prune removed nothing and the set really did grow unboundedly — twice over, since `take_competing_blocks`'s removals were also no-ops. So this row closes as a consequence of `OBL-C52`'s fix rather than by its own change: the structure was always right and the keys were not. Recorded because a reader who finds "C28 closed" and then sees the remove-key fix under C52 would otherwise have to re-derive the connection |
| OBL-C29 | **PARTLY** | The genesis filter modes (`Off`/`Relaxed`/`Strict`) and the Path-B tie-breaker are implemented in Rust, or the Python model is amended — they are specified and absent | genesis-consensus open item 1 | M | **PARTLY — and the two halves are in different places, which is what the row could not say.** *In the model the modes are real*: `apply_genesis_filter(our_genesis, peer_tips, mode)` with `mode: "Off" | "Relaxed" | "Strict"` (`contrib/model/chain_validation_model.py:3667-3720`) — Off accepts all peers without filtering, Relaxed falls back to all peers if the filter empties, Strict keeps an empty result empty — and each is exercised (`:3737`, `:3754`, `:3766`, `:3775`). *In the machine they do not exist*: the vocabulary appears nowhere in `src/`, `bin/` or `contrib/` outside that file, so there is no mode parameter, and the Rust implements **Path A only** — `sync_connection.rs:442-455`, where a peer presenting a **different** genesis hash is incompatible and a peer presenting **none** is admitted because it is a bootstrapping joiner whose genesis is validated against the pin downstream. That admission is the *hard-coded* equivalent of one mode, not a parameterised one. *Path B is deferred with its reason in the code*, exactly as the "premises that moved" note says: *"Path B (the plurality vote over peer tips at height 0) is a multi-peer decision and cannot be made on a single connection, so a node with no genesis still accepts and lets the filter run once tips are known"* (`:448-451`). So the row's disjunction is unmet on both arms — not implemented in Rust, not removed from the model — and the honest status is the split: Path A live, Path B deferred-with-reason, the mode parameterisation model-only |
| OBL-C30 | **SATISFIED** | `compute_reward`/`verify_uncle_split` signatures agree between the model and the Rust; the connect path's one-element slice is equivalent but reads oddly | genesis-consensus open item 2 | **SATISFIED — the signatures agree parameter for parameter, and the odd-looking slice is equivalent with its reason written at the site. Read 2026-09-22.** Rust: `verify_uncle_split(base_reward: BlockReward, canonical_reward: BlockReward, uncle_pin_confirmed: &[BlockReward])` (`src/linear/src/supply_chain.rs:313-317`). Model: `verify_uncle_split(base_reward: int, canonical_reward: int, uncle_pin_confirmed: List[int])` (`contrib/model/chain_model.py:217-220`) — same arity, same names, same order, and the docstring states the invariant in the same words (*"canonical_reward + sum(uncle_pin_confirmed) == base_reward"*) while naming the Rust function it mirrors; `compute_reward` likewise (`chain_model.py:200`). The body is the same two lines: sum the pins, then require the exact equality (`supply_chain.rs:318-319`; `chain_model.py:226-231`). **The one-element slice is equivalent and the code says why**: the connect path calls it with `&[total_pin]` where `total_pin = crate::block::total_accepted_pin(uncles)?` (`chain_state.rs:1071-1079`) — the *sum* wrapped in a slice — and since the callee sums its argument, a one-element slice of the sum and the per-uncle list produce the same total; the comment above the call records exactly that ("Same Σ-pin semantics as the host check and the builder (`total_accepted_pin`)"). The multi-element shape is not left untested either: `supply_chain.rs:437-450` covers single-pin and multi-pin calls against the same equality | M |
| OBL-C31 | **SATISFIED** | The generic-prover write path enforces the derived-rule DAG at parse time — a forward reference or a cycle is a parse error, not an unhandled case | l1-write-path `V1`–`V4` (the DAG extension); `V5`–`V7` already applied | M | **Audited 2026-09-23: SATISFIED, not PARTLY** — the derived-rule DAG *is* enforced at parse time, in `src/sdk/src/prover.rs:201-217`, pinned by `witness_map_rejects_derived_forward_reference` and its positive control `witness_map_allows_input_forward_reference`; added `4a6e5c5609` (2026-08-27), before this row was written. The row searched `bin/dww/src/prover_impl.rs`. Originally: PARTLY — the property holds in effect, and not where the row asks; there is no parse stage to hold it in. Read 2026-09-22.** The generic prover is `bin/dww/src/prover_impl.rs`, and it evaluates `DerivedRule`s in order against a slot vector. A forward reference is caught by `operand()` (`:485-496`), which reads an *already-bound* slot and otherwise returns a clear error — `"witness[{idx}]: derived operand slot {i} is unbound or out of range"` — so the case is handled rather than unhandled, exactly as the register's "premises that moved" note says. Two things the row's requirement does not get. **First, it is not a parse-time check, because there is no parse stage**: a search of the file for `parse`, `DAG`, `cycle` or `topolog` finds nothing — the rules are evaluated as they are read, and the only safety net is that the slot must already be bound. **Second, a cycle cannot be expressed in this representation**: a rule referencing a later slot *is* a forward reference, so cycles collapse into the case above rather than needing separate enforcement. So the obligation is satisfied in substance (no unhandled case, and no expressible cycle) and unsatisfied in form (no DAG validated at parse time). **Gap: the error path is unpinned** — the message occurs only at its definition, and none of the file's three tests exercises it |
| OBL-C32 | **OPEN** | `Message::BARBS` is enforced, or the net-layer quarantine is lifted with the reason recorded | sync-hazop `D3`, DEFERRED | M | **OPEN — neither half of the row's disjunction has happened: the constant exists and is threaded, but the one site that would read it is commented out. Read 2026-09-22.** `BARBS` is real: declared on the `Message` trait with an empty default (`src/net/message.rs:71`, `:109`) and overridable by the macros (`:85`, `:140`), and the sync layer sets it (`SYNC_BARBS`, `sync_types.rs:322`) and passes it into the boundary codec (`:357` → `dwow_core::impl_boundary_codec!`) for all four messages. **Enforcement, however, does not run**: the accessor that would publish a message's barbs to the channel-boundary rule is commented out — `message_publisher.rs:318` (`// fn barbs(&self) -> &'static [crate::barb::BarbId] { M::BARBS }`) — and the nearby note says when it returns: *"reinstated when quarantine boundary enforcement is activated (§10.4)"* (`:247`, `:319`). So this is a **third** state, not one of the row's two: deferred-by-design with the reason recorded in code and pointing at `type-system.md` §10.4 ("Bridging — Shared Channels with Typed Barbs"). Recording it as such, because "DEFERRED" in the carried-from column and a live constant in the tree is exactly the pair a reader would otherwise read as enforcement |
| OBL-C33 | **OPEN** | A per-peer failure score persists across sync passes, so a peer that fails is deprioritised rather than reset each tick | node-sync `F5`, PARTIALLY RESOLVED (`sync-protocol.md` §13.3) | M | **OPEN — searched for, not sampled: no per-peer score exists anywhere in the tree, while the spec requires one by name. Read 2026-09-22.** A repo-wide search of `src/`, `bin/` and `crates/` for `misbehav`, `ban_peer`, `peer_score`, `banned` and `disconnect_peer` returns nothing that scores or penalises a peer — the only hits are unrelated (a Tor-connection comment in `net/channel.rs:651`, a hostname fixture, a propagation-rate comment). The spec is explicit and the row quotes it correctly: *"Peer discipline SHALL be a single persistent score (Bitcoin Core `Misbehaving()`): a peer that serves an **invalid block** is disconnected; a peer that times out is simply skipped and the next peer tried"* (`doc/src/arch/sync-protocol.md:355-356`). What the code implements is the **second** clause only — the pull loop skips a failing peer within a pass and tries the next (§13.3 step 4, `bin/dwowd/src/task/consensus_linear.rs`) — and it persists nothing across the 30-second ticks, so a peer that lied in pass N is treated exactly like a fresh peer in pass N+1. Nor is there a disconnection path for an invalid-block server: the only `disconnect` in the sync task refers to *blocks* in a reorg (`:285`, `:293`), not peers. Worth noting the adjacent rule the code does record, so it is not mistaken for this one: `chain_state.rs:865` — *"§14.3 — a duplicate SHALL be skipped, never banned"* — is about duplicate *blocks* |
| OBL-C34 | **OPEN** | The fail-closed genesis handshake and its bootstrap path are re-read against the `P2-7` rework before this finding is called closed: the state machine folded `WaitingForGenesis` into `Behind` (`bin/dwowd/src/lib.rs:161-179`) and the peer's genesis is no longer compared in the sync handshake (`linear_genesis_hash` survives only on the merge-mining path), so the analysis describes a handshake that has since changed | daemon-sync `V1`–`V4` | H | **Audited 2026-09-23: REFUTED — this is OPEN.** The genesis pin is not enforced on the bootstrap path: `check_genesis_pin`'s only production call sites are `bin/dwowd/src/lib.rs:893` and `:910`, both inside `init_linear`'s `create_genesis == true` branch, while every miner and observer runs `CREATE_GENESIS=false`, dials with no genesis, is admitted, and has its received genesis checked only by a four-byte magic test (`bin/dwowd/src/task/consensus_linear.rs:382-388`). `apply_genesis_deployments` (`src/linear/src/execution.rs:902`) then deploys whatever WASM bytes the block carries — there is no pin comparison in that file. The comment the closure rested on (`src/linear/src/sync_connection.rs:446`) names a call that does not happen. **Originally: SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below.** |

### The consensus HAZID's residue (`OBL-C35`+)

The consensus HAZID report was a *hazard register*, not a findings list: 4 CATASTROPHIC, 12 HIGH and
18 MEDIUM, each named and each with a status. Its six root causes are `RC1`–`RC12` material now, and
three hazards it records as RESOLVED stay resolved — `H-C1` (fork selection reconnects through the
full pipeline), `H-C3` (the exponential reward schedule, `fixed_pow_decay`), `H-H1` (consensus
atomics are `SeqCst` or `Acquire`/`Release`); `H-H5` (`serde_json` block storage) is closed above.
Everything else it left open is below, one proposition per hazard, because collapsing 28 named
hazards into a summary is how a HAZOP ends up with items nobody can find.

| ID | Status | Proposition | HAZID | Sev |
|---|---|---|---|
| OBL-C35 | **CLOSED** | A stratum-mined block is broadcast on accept, as the merge-mining path now does — the stratum path commits locally and peers learn of it only on the 30-second sync poll | `H-C2`, CATASTROPHIC, residual | M | **Audited 2026-09-23: CLOSED, not OPEN** — `stratum_submit`'s `CanonicalExtension` arm now calls `broadcast_block` with the block's own uncles (`bin/dwowd/src/rpc/stratum.rs:711`), matching the merge-mining path it was told to match. Originally confirmed open, and now scoped exactly: stratum is the *only* accepting path in the tree that does not broadcast. Verified 2026-09-22.** Enumerating every `broadcast_block` call site in `bin/dwowd/src/` leaves one gap: the built-in miner broadcasts (`lib.rs:1861`), the RPC miner broadcasts **with its uncles** (`rpc/miner.rs:302`), merge mining broadcasts (`rpc/mm_rpc.rs:749`, with `vec![]`), the receive path relays onward (`lib.rs:1043`), and the tests call it directly — while `stratum_submit`'s `CanonicalExtension` arm (`rpc/stratum.rs:681-701`) marks the mined transactions from the mempool and pushes the next job, and **never broadcasts**. So a stratum-mined block reaches peers only when their 30-second poll happens to fetch it, which is the catastrophic-residual shape the row names. **The fix is a port, not a design**: `mm_rpc.rs:745-747` already carries the HAZID H-C2 comment and the call, and `stratum.rs`'s handler is `impl RequestHandler<StratumRpcHandler> for DwowNode`, so `self.p2p_handler.p2p` is in scope exactly as it is there. One refinement to take from the *other* correct site rather than from `mm_rpc`: `miner.rs:302` passes `prep.uncles.clone()`, while `mm_rpc` passes an empty vector — and the stratum path already has its uncles to hand, loaded at `:665-668` from the template, so the port should pass those rather than discard them **CLOSED 2026-09-22** (`e252e4f35c`): `stratum_submit`'s `CanonicalExtension` arm now calls `broadcast_block(&self.p2p_handler.p2p, block.clone(), uncles.clone())`, mirroring `miner.rs` rather than `mm_rpc` so the uncles travel with the block. Verified by `cargo test --release -p dwowd --all-features merge_mined` → 3 passed, 0 failed — the same run that carries the `OBL-C67` seating test, so the acceptance path and both of its new behaviours are exercised together. |
| OBL-C36 | **SATISFIED** | No serialization fallback substitutes a zero vector for a dedup hash: a `vec![0u8; 32]` on failure makes duplicate competing blocks acceptable and valid ones droppable | `H-C4` | C | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C37 | **PARTLY** | `saturating_sub` on block timestamps does not mask a decreasing timestamp as a zero interval | `H-H2` | M | **Audited 2026-09-23: PARTLY, not CLOSED** — the *decreasing*-timestamp route to a zero interval is closed, but the **equal**-timestamp route, which the median rule permits, reaches the same branch, and `avg_interval == 0 → ratio_scaled = SCALE * 9 / 10` (`consensus.rs:237-239`) makes the target ~11% *easier* while its own comment says "10% harder" (`BlockTarget::adjust`, `sdk/src/blockchain.rs:345-349`). Originally closed 2026-09-22. The zero interval had already been replaced by `checked_sub` plus a warning, so the row's wording was stale — but the *behaviour* the row objects to survived the change: the substitution kept `0` and only added a log line, and a zero interval drags `avg_interval` down, `ratio_scaled` up, and the target **easier**. So the anomaly was logged and then acted upon, in the miner's favour. A decreasing pair now **abandons the adjustment** — the target is returned exactly as it was, which is deterministic, conservative, and the only answer a miner cannot profit from engineering. Both paths (`adjust_target` and `compute_adjustment`) changed together, or the same window would produce different targets depending on which the caller used. **Reachable, not defensive**: `validation::check_block_timestamp` requires a timestamp above the *median* of the last 11, which does not exclude one below its own parent — a parent above the median with a child between the two passes. The test that covered this asserted only "a validly clamped target", which the defective behaviour also satisfied, so it was **strengthened** to assert the target is unchanged, with a positive control that a well-formed-but-fast window still hardens it, and **negative-controlled** — reverting the fix makes it fail with the row's own name |
| OBL-C38 | **SATISFIED** | A lock poisoned by one panic does not bring down the node — 50+ `.lock().unwrap()` sites have no recovery | `H-H3` | H | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C39 | **SATISFIED** | Every in-memory cache is restored from sled on restart: `uncle_coin_set` never is, so duplicate uncle inclusion is admitted | `H-H4` | H | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C40 | **SATISFIED** | Uncle merge detects duplicate-key conflicts instead of letting the second write silently overwrite the first | `H-H6` | H | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C41 | **SATISFIED** | Mempool transactions are re-inserted on mining error paths in every miner, not only the RPC miner | `H-H7` | H | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C42 | **PARTLY** | `take_competing_blocks()` follows the fallible work in all four call sites; it is destructive and currently precedes it in three | `H-H8` | M | **PARTLY — one of the three production sites follows the rule; the other two precede it and compensate rather than reorder, which is the downgrade the "premises that moved" note records. Re-verified 2026-09-22.** The three production call sites are `lib.rs:1354`, `rpc/mm_rpc.rs:263` and `rpc/stratum.rs:191`. **`prepare_block` is the one that is right, and says why**: the take is annotated as *"the LAST step. `take_competing_blocks` is DESTRUCTIVE. All fallible operations (coinbase build, uncle-mint, fee_collect_tx) MUST succeed before this"* (`lib.rs:1348-1354`). The other two take first — `mm_rpc` takes at `:263` before building the template, `stratum` takes at `:191` before generating one — so the row's "precedes the fallible work" still describes them. **What changed is the consequence**: both now re-put on the failure path (`mm_rpc.rs:297`, `stratum.rs:221`, and `lib.rs:1709`/`:1837` on the miner's error paths), so a failed template no longer forfeits the competing blocks — the take is still premature, but it is no longer lossy, which is why this is a reordering question rather than a data-loss one. **RESTATED 2026-09-23 — the ordering this row prescribes is worse than the one the code has, and the |
check that settled that is recorded because the change was about to be made.** The row says the take
must *follow* the fallible work, and `prepare_block` is its model. Reading the model closely, the
port would introduce a loss window that does not exist today: `prepare_block` peeks at `lib.rs:1222`
and takes at `:1354`, with the coinbase build, uncle-mint and fee-collect between them — all
potentially awaiting — and `take_competing_blocks` locks only `competing_blocks` + `competing_seen`,
which `chain_state.rs:43` states outright *"never `connect_lock`"*. So a competing block arriving
during those steps is consumed by the take **without ever being built into an uncle** — the exact
loss `HAZOP #5` fixed — and nothing compensates for it. The take-first shape's only exposure is its
*failure* path, and there the compensating `put_competing_blocks` restores precisely what was taken.
Applied to `mm_rpc.rs:263` and `stratum.rs:191`, the prescribed reorder would trade an exactly-closed
window on the failure path for an unclosed one on the success path. **The invariant that matters is
therefore not "take last" but "nothing is destroyed before the work that needed it succeeded, *and*
nothing is consumed without being used"** — which take-first plus the exact put already satisfies, and
which the reorder would break. No code change made, deliberately, and the two sites keep their
compensating puts. (Residual, stated rather than implied: the ordering is not the mechanism; if these
paths are ever to be tightened, the fix is a single operation that returns what it consumed *and*
what the uncles were built from, so the two cannot diverge.)ree |
| OBL-C43 | **OPEN** | Concurrent stratum logins cannot observe a mismatched template+config pair | `H-H9` | M | **OPEN — the pair is written and read under two separate locks, with nothing pairing them. Read 2026-09-22.** `stratum_login` stores the two values in sequence under distinct locks — `*current_linear_template.lock().await = Some(template.clone())` then `*linear_recipient_config.lock().await = Some(config)` (`bin/dwowd/src/rpc/stratum.rs:228`, `:230`) — where `config` is the **login's** own config (it is what `generate_linear_block_template` was called with at `:205`) and the template embeds it. Two concurrent logins can interleave those two writes, leaving one login's template beside the other's config. Nothing detects it: `stratum_submit` reads them separately too (`:465`, `:581`, `:665`, `:704`), there is no generation counter, no joint struct, and no cross-check that the stored template's recipient matches the stored config. The impact is bounded by the design rather than by a guard — there is exactly one stored template and one stored config for the node, so a second miner's login overwrites the first's values and *any* submit is already validated against the newest login — but a torn pair means a submitted block judged against a recipient config its template was not built from. Closing it means storing the template and its config as one lock-protected value, not adding a check |
| OBL-C44 | **OPEN** | A reorg re-admits the disconnected block's transactions to the mempool, as Bitcoin Core does | `H-H10` | M | **OPEN — confirmed by the absence of the operation rather than by reading a broken one. Read 2026-09-22.** The mempool is told about inclusion at five acceptance sites — `bin/dwowd/src/lib.rs:1809`, `rpc/miner.rs:271`, `rpc/mm_rpc.rs:742`, `rpc/stratum.rs:699` and `proto/linear_broadcast.rs:447`, each calling `mp.mark_mined(&tx_hashes)` — and `mark_mined` (`crates/dwow-mempool/src/lib.rs:562-580`) removes those hashes from the pool, persistently (`sled remove`). There is **no inverse**: no `mark_unmined`, no re-insertion path, and neither the reorg driver (`bin/dwowd/src/task/consensus_linear.rs` — which does not mention the mempool at all) nor `CChainState::disconnect_block` (`chain_state.rs`'s only mempool mention is a doc comment on the nullifier set) puts a displaced block's transactions back. So a reorged-away transaction is dropped from the pool and stays gone until its own submitter re-broadcasts it; nothing in the node recovers it. That is a liveness difference from Bitcoin Core — which the row explicitly benchmarks against — rather than a safety one: no double-spend follows, since the nullifier gate is untouched and the transaction remains valid to re-submit. The wire to close it already exists (`mark_mined`'s hashes are computed at each site from `block.transactions`), but the re-admission must happen where the reorg is decided, and that is the daemon's `reorg_to_heavier_chain`, which currently has no mempool handle |
| OBL-C45 | **OPEN** | The contracts tree and the supply-chain tree reconcile automatically; no `verify_cumulative_supply()` exists and the `get_cumulative_supply` RPC returns stored state unverified | `H-H11` | M | **OPEN as recorded, with one half of its premise narrowed by what is already enforced. Read 2026-09-22.** The RPC half is exact: `blockchain_get_cumulative_supply` (`bin/dwowd/src/rpc/blockchain.rs:357-380`) reads `chain.supply_chain.get_latest()` — the *stored* entry — and returns its `value_commit`, `blind` and `total_supply` with no recomputation, carrying its own `UNVERIFIED(F3-1): needs cargo test -p dwowd --lib && ./verify_cumulative_supply.sh against a devnet node` tag. No `verify_cumulative_supply()` function exists in the tree at all; `verify_cumulative_supply.sh` is a script nobody calls at runtime, and it compares the stored *plaintext* total against the schedule while disclaiming the commitment half itself (`:70-72`). **What is already enforced, and why the row's first clause is too strong as written**: the two trees are not reconciled globally, but the supply-chain entry is *re-derived and cross-checked at every block* before it is written — `read_cumulative_from_overlay` recomputes `S_H = S_{H-1} + C_base` and rejects the block on any disagreement with the WASM overlay (`block_acceptor.rs:656-689`, `OBL-C25`), and `connect_block` is the only insertion path. So the residual is narrower than "unverified": there is no *read-time* verification and no end-to-end check against the emission schedule outside a script, but every write is step-verified |
| OBL-C46 | **SATISFIED** | The non-ZK template fallback, which produces all-zero cryptographic material, is removed or gated rather than left as dead code in the consensus hot path | `H-H12` | H | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C47 | **SATISFIED** | `Clone` on `PoWConsensus` does not yield independent atomics, and `save_to_batch` does not read `target` outside its lock | `H-M1`, `H-M2` | M | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C48 | **SATISFIED** | Deployooor post-processing participates in duplicate-key detection during merge | `H-M3` | M | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C49 | **SATISFIED** | Malformed ZK metadata is not masked by `unwrap_or_default`, nor overlay failure by `aggregate().unwrap_or_default()` in `accept_block`, nor a WASM `total_supply` failure by `unwrap_or(0)` | `H-M4`, `H-M6`, `H-M14` | M | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C50 | **CLOSED** | Error variants are matched by type, not by string matching in `TxBackend::get_tx` | `H-M5` | M | **CLOSED 2026-09-23 — the fix was available in the type system rather than needing one. Read 2026-09-22, fixed the next day.** Two sites in `src/linear/src/execution.rs` decide "not found" by substring-matching a rendered error message: `get_tx` tests `e.to_string().contains("TransactionNotFound")` (`:1109-1110`) and `get_block_hash_by_height` tests `e.to_string().contains("BlockNotFound")` (`:1141-1142`), each mapping a hit to `Ok(None)` and everything else to `Err(Error::Custom(e.to_string()))`. The typed variants already exist and are already the ones thrown — `LinearError::BlockNotFound(BlockHeight)` (`error.rs:38`) and `LinearError::TransactionNotFound(String)` (`:41`), produced by the store at `store.rs:115` and `:122` via `.ok_or(...)` — so the match could be on the variant today. What the string form costs is silent: rewording either variant's `#[error(...)]` text, or wrapping it, changes behaviour with no compile error — a genuine not-found becomes a hard error (a rejected block or a failed WASM call) and vice versa. Same family as the `OBL-C49` metadata-decode work, which moved in the other direction by giving failures a typed `fail_stage` |
| OBL-C51 | **PARTLY** | Cumulative supply does not silently saturate at `u64::MAX` (`saturating_add`), and `compute_reward` does not hide pin-reward overflow (`saturating_sub`) | `H-M7`, `H-M13` | M | **Audited 2026-09-23: REFUTED on the `H-M7` half — PARTLY.** The conjunction is not satisfied: `saturating_add` is live on the cumulative supply in three places (`native_token/src/entrypoint/mod.rs:1054`, `src/linear/src/supply_chain.rs:301`, `src/sdk/src/blockchain.rs:1264`), and `git log -S checked_add -- src/linear/src/supply_chain.rs` is **empty** — a checked add was never introduced. The contract's own guard `new_supply != pr.expected_cumulative_supply` saturates on *both* sides, so saturation would pass it silently. The `H-M13` half (the pin-reward `saturating_sub`) is genuinely closed at `src/linear/src/block.rs:664`. `PATH: NONE` — reaching saturation needs ~1.3e10 blocks, so the class stays `M`. **Originally: SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below.** |
| OBL-C52 | **CLOSED** | Competing-block dedup hashes use the block hash, not a header serialization | `H-M8` | M | **CLOSED 2026-09-22.** The row understated itself: the two sides of the same set used *different* keys, not merely a different one from the row's preference. The three insert paths keyed on `hash_with_vm` (RandomX over the mining blob) while `take_competing_blocks`, `put_competing_blocks` and `prune_competing` keyed on `blake3(dwow_serialize(header))`, so a take removed an entry that had never been added, the insert's key stayed in the set forever, and a block legitimately re-submitted after being taken as an uncle was **silently dropped as a duplicate** — `CompetingStored` without anything stored, indistinguishable to the caller from success. Both sides now call one `competing_dedup_key`, which is the serialization form: it needs no RandomX VM (so the insert paths lose a hash and a VM lock, and `store_competing_block`'s `pow` gate is now spurious), the three removing paths cannot obtain a VM at all, and `uncles_by_height`'s storage key was already this exact expression — so it is the convention the struct had settled on everywhere except the one place it mattered. `chain_state::tests::competing_block_can_be_re_stored_after_being_taken` is the test, and it was **negative-controlled**: reintroducing the asymmetry at the insert site makes it fail with the row's own name |
| OBL-C53 | **CLOSED** | `consensus.load()` failure is not discarded — a node does not start on silently corrupted state | `H-M9` | M | **CLOSED 2026-09-22.** `PoWConsensus::load` read `target` only `if bytes.len() == 4` and otherwise *skipped it silently*, so a node whose `target` key was damaged started on the **initial target** — a consensus-critical value quietly wrong, with nothing downstream able to tell. The fix draws the distinction the doc comment had already stated and the code did not implement: a value that is **absent** keeps its default (a fresh node has no stored target), and a value that is present but the **wrong length** is corrupt and is now an `Err`. The same class sat in the sibling read — `bytes.chunks_exact(8)` discards a trailing partial chunk, dropping a timestamp without a word — so a `timestamps` value that is not a multiple of 8 is refused too. `consensus::tests::test_load_rejects_corrupt_state_but_allows_absence` covers all three cases: absent loads, each corrupt form errors, and well-formed loads — the third being the positive control that stops a `load` which always errored from satisfying the other two |
| OBL-C54 | **PARTLY** | Template read/write is not racy between stratum login and submit | `H-M10` | M | **PARTLY — the writes are serialised but nothing versions or pairs the template with the login that produced it; one compensating check bounds the damage. Read 2026-09-22, same evidence as `OBL-C43`.** The template is written under its own lock (`stratum.rs:228`) and read under that lock at each site (`:465`, `:581`, `:665`), so no individual read is torn — but the *sequence* is unversioned: `stratum_submit` takes a clone at `:665`, validates the submitted block against it, and a concurrent login may replace it in between, so a submit can be judged against a template that is no longer current. Two things bound this rather than a guard: the template height travels with the template and a stale submit is rejected by `accept_block`'s own height check (the compensation the `OBL-C68` note relies on), and the job cache is single-slot, so "current" is whatever the newest login left. What is absent is what the row asks for — a way to tell whether the template a submit is using is still the one it was issued from, which is the same missing pairing as `C43` (one lock-protected value holding template + config + a generation counter, rather than three independently-written fields) |
| OBL-C55 | **PARTLY** | `mm_jobs` eviction at capacity is FIFO, not clear-all | `H-M11` | M | **PARTLY — the bulk-clear defect is gone; FIFO is not implemented, and the code says so itself while its neighbouring comment says the opposite. Read 2026-09-22.** `mm_rpc.rs:330-343`: `MAX_MM_JOBS = 100`, and at capacity a **single** entry is removed (`if let Some(evicted) = mm_jobs.keys().next().cloned() { mm_jobs.remove(&evicted); }`) before the new job is inserted — so the bulk `clear()` this row was filed against is gone, which is the half that mattered. **FIFO is not what happens**: `mm_jobs` is a `HashMap`, so `keys().next()` yields an arbitrary entry, and the code's own note says exactly that — *"Note mm_jobs is a HashMap, so the evicted entry is arbitrary, not FIFO - acceptable for a job cache since jobs expire naturally (submitted/current-height tracking)"*. That justification is recorded here rather than treated as compliance: since the row's proposition is FIFO, the row is not satisfied, it is *deviated from with a reason*. **And the same block contradicts itself one line up**: the header comment says *"Jobs older than the latest MAX_MM_JOBS entries are evicted"*, which is false for a HashMap and is the third comment-vs-code mismatch found in this campaign (with `OBL-C26`'s sled-batch claim and `OBL-C17`'s float/bulk-memory claim). The sibling structure `mm_jobs_submitted` uses the identical pattern (`:751-760`, also single-entry, also from an unordered container) |
| OBL-C56 | **PARTLY** | A miner does not mine during a `SYNC_BEHIND` transition (no sync-state TOCTOU) | `H-M12` | M | **PARTLY — the gate is real and re-checked every attempt; the check and the act are not atomic, and the residual window is bounded by the accept path, not eliminated. Read 2026-09-22.** The gate exists and is not a one-shot: `bin/dwowd/src/lib.rs:1412-1431` re-reads `sync_state` at the top of **each** mining iteration — `if state != SyncState::CaughtUp { …wait 1s; continue; }` — with the comment naming the purpose ("Avoids mining at a stale height when peers have advanced"), and the state is logged on transition so an operator sees why mining stopped instead of a silent spin. `Behind` is set by the consensus task on a crash (`:1080`, described there as terminal until restart) and the loop is also gated at startup (`:1387` waits for `CaughtUp`). What the row calls TOCTOU is nonetheless present in the narrow sense: `sync_state` is an `AtomicU8` (`:855`), so the read at the top of the iteration and the mining attempt that follows it are not one atomic step — a transition landing between them admits one attempt at a height that is already stale. That is bounded rather than closed: the attempt builds from local chain state and the resulting block is judged by `accept_block`'s own height/extension checks, so the worst case is a rejected block and a wasted nonce search, not a fork. Closing it properly would mean holding the state as a lease across the attempt, which the register should ask for explicitly rather than treat as implied by the atomic load |
| OBL-C57 | **SATISFIED** | The identity point is not used as a "not initialized" sentinel, conflating that with a validly zero value | `H-M15` | M | **SATISFIED — every identity-point use in the tree is either an additive initial value or a validation target, and the one skip-site is defined in both build profiles. Read 2026-09-22.** Searched rather than sampled: outside test fixtures, `pallas::Point::identity()` appears as (i) the initialiser of Pedersen **sum accumulators** (`proof_of_token_balance.rs:95-98`, and the genesis entry at `supply_chain.rs:120`, whose accessor says *"Returns genesis (identity) state if no blocks have been committed"* at `:248`) — where identity is the additive identity, the correct `S_0`, not a sentinel reading as "absent"; (ii) **explicit validation targets** in key handling (`sdk/src/crypto/keypair.rs:366`, `:408`, which reject an identity point) and the placeholder `SchnorrSignature` (`schnorr.rs:47`), which cannot verify against a real key since `sG = R + eP` needs `R` to be a valid point for a non-zero `eP`; and (iii) exactly **one** site that skips on identity — the uncle-commitment entry at `chain_state.rs:1109`, `if bool::from(c_uncle.is_identity()) { continue; }`. That third one is the row's concern and it holds: the comment records that the *previous* form was the real defect — a debug-only assertion meant debug and release behaved differently, with release recording an identity commitment as an uncle entry — and skipping is now defined behaviour in both profiles. The conflation is also unreachable, because the entries are filtered to `pin_accepted && pin_confirmed > 0` before the commitment is built (`:1087`) and the blind is a hash, so a validly-zero pin never reaches it. The cumulative chain's identity *value* remains a legitimate zero, not an absence |
| OBL-C58 | **SATISFIED** | A Pedersen chain integrity check exists | `H-M16` | M | **SATISFIED — the chain is checked at every step, in the contract and again in the host; only the standalone verifier does not carry the commitment half. Read 2026-09-22, evidence shared with `OBL-C3` and `OBL-C25`.** The integrity check exists in two places rather than one. In the contract, `pow_reward_v1` verifies the chain link by link: the prover's `old_cumulative_commit` must equal the on-chain value (`native_token/…/entrypoint/mod.rs:1105-1117`, with the blind checked too except at genesis), and `new_cumulative_commit` must equal `old_cumulative + pr.output.value_commit` (`:1122-1129`) — so a block cannot advance the Pedersen accumulator by any amount other than its own coinbase commitment. In the host, the same step is re-derived independently of the WASM overlay and the block is rejected on any disagreement (`block_acceptor.rs:656-689`, `OBL-C25`), which is what makes the check a cross-check rather than a mirror. What does **not** carry it is the only standalone verifier: `verify_cumulative_supply.sh` compares the stored plaintext total against the schedule and says so itself (*"Full Pedersen commitment chain verification … is demonstrated by the Python model"*, `:70-72`), and the RPC returns the stored entry unverified (`OBL-C45`). So the row's proposition holds; the caveat is about the tooling, not the enforcement |
| OBL-C59 | **SATISFIED** | `prev_coin` bridging between the contracts tree and the supply-chain tree is verified | `H-M17` | M | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C60 | **SATISFIED** | `expected_reward` does not truncate u64→u32 (currently safe for ~16,000 years, and a truncation rather than a bound) | `H-M18` | M | **SATISFIED 2026-09-22 — the evidence is named in the "Verified 2026-09-22" section below; this marker makes that recorded verdict greppable rather than a fresh claim.** |
| OBL-C61 | **RESTATED** | Two controls the HAZID names as residual: an automated reorg regression test (diverge → heavier peer → converge, covering WASM state and cumulative supply across disconnect/reconnect), and a CI test comparing `expected_reward()` against the spec formula | `H-C1`/`H-C3` residuals | M | **Audited 2026-09-23: RESTATED, not PARTLY** — the clause "a search finds no Rust assertion against the spec formula" is false: `src/sdk/src/blockchain.rs:1080` carries `reward_formula_key_points` and `:1100` `reward_monotonic_decrease`. What survives is narrower and is the real content — no test cross-checks the Rust against an *independently computed* schedule. Originally: PARTLY — the first control exists and is better than asked for; the second exists against the *model* and nothing binds the Rust. Verified 2026-09-22.** *First half: satisfied, five times over.* `bin/dwowd/src/tests/daemon_sync_integration.rs` carries `test_reorg_state_consistency` (`:754`, two competing block-3s from different miners, accept one, reorg to the other) and `test_activate_best_chain_adopts_competing_block` (`:688`, whose doc comment states the scenario the row asks for — *"a divergent node reorgs onto the competing chain via `activate_best_chain` … after disconnect + cumulative-commit rollback + reconnect"*) plus `test_accept_block_reorgs_onto_heavier_chain` (`:859`), `test_multi_block_reorg_state_consistency` (`:910`) and `test_sync_path_reorg_to_heavier_chain` (`:968`). All five exercise the ZK/WASM path (`enable_deterministic_zk()` at each entry) and **all five passed** in the 2026-09-22 sweep. *Second half: partial, and the gap is specific.* A CI-run model of the schedule exists — `contrib/docker/darkwow-testnet/merge_mining_model.py:321` mirrors `expected_reward` (`src/sdk/src/blockchain.rs`) and is **gate 13** of `scripts/run-all-tests.sh`, and `Emission.lean` transcribes the schedule as a definition with the constants (`INITIAL_REWARD = 1_383_764_049`, `:42`, `:75`). But the model checks *hand-calculated* values from the Rust schedule (per its own README, which tabulates them), and no test executes the Rust and compares: a search finds no Rust assertion against the spec formula, and no reward fixture among `contrib/model/fixtures/` (which holds coinbase_scan, escrow_create, finality_fork_vectors, multi_contract, pn_transfer). So the schedule is checked twice against *transcriptions* of itself and never once against the binary — the same shape as `OBL-C45`'s missing reconciliation, and the fix is a fixture, not a formula |
| OBL-C62 | **OPEN** | A decoder accepts every length its own encoder produces | `multisig/src/model/mod.rs` (`SignParamsV1`) | **NEW 2026-09-22, introduced by `55a2de9c42` (OBL-Z11), and it is an encoder/decoder asymmetry — the shape `safety.md` RC5 names.** `SignParamsV1::decode` opens with `if data.len() < 200 { return Err("too short") }`, but the layout is `group_id(32) + message_hash(32) + member_commitment(32) + nullifier(32) + len(4) + proof + tx_binding(32) + tx_nonce(32)` = `132 + proof_len + 64`, so the true minimum is **196**. Any proof shorter than four bytes encodes to a length in `196..199` that the decoder then rejects as too short — it refuses its own output. Found by `test_multisig_encode_roundtrip`, whose values are arbitrary: the asymmetry is the defect, not the test data. `FinalizeParamsV1`'s guard is `168` against an exact minimum of 168, so it is correct, and the fix is one constant here. **Not fixed in place**: `multisig` is a genesis contract, so it moves a source hash and re-rolls the pin, and it belongs to the campaign's batched genesis stage rather than to a drive-by one-character edit | H |

**Closed on re-reading, recorded so they are not re-carried:** `C-6` (coinbase maturity after the sled
commit) — the check at `src/linear/src/chain_state.rs:1117` now precedes every `apply_batch` at
`:1340-1349`, all of them inside `connect_block`; `H-3` (`serde_json` for block storage) —
`chain_state.rs` no longer imports it, and the surviving `serde_json` in `src/linear/src/sync_connection.rs`
is the deliberate sync wire format with a documented size cap, decoded before hashing, so it is not a
determinism obligation; coinbase key-binding `V1`–`V4` — the miner fills the field (`src/linear/src/miner.rs:76`)
and the acceptor checks it (`bin/dwowd/src/block_acceptor.rs:341`, genesis exempt, `block.rs:682`'s
placeholder surviving only on the genesis and test paths); l1-capability-tests `V1`–`V5` — the
deterministic `cargo test --lib --no-run --message-format=json-render-diagnostics` selection replaced the
`find 'dwowd-*'` glob, and `pipeline_spec.py:641-651` models `phase_98` including the `--list` smoke
check.

### The finality widgets' residue (`OBL-C63`+)

DarkWow ships two finality layers — Caribina (Arweave-anchored) and the Monero anchoring gadget — and
asserts a 51% security property for each. Reading both end to end against the code that enforces them
produced this residue. The unifying shape is worth stating once, because it is what makes six rows out
of one design error: **finality is enforced from free header fields, while the verifying code that
would authenticate those fields exists and is never called.** `caribina::verify_anchor` is called only
from its own test file; `should_verify_anchor` and `should_verify_monero_anchor` have no production
caller anywhere; `verify_monero_anchor` is re-exported at `src/linear/src/lib.rs:103` and called by
nothing but its own tests; and `bin/dwowd/src/proto/linear_broadcast.rs` contains zero occurrences of
`anchor` or `finality`. The only live finality mechanism is the guard below, and it consults no proof.

Two rows — `C69` and `C70` — are not about finality at all despite being found here; they are the
merge-mining proof and the block-size bound, and they surfaced because reading this surface end to end
means reading `PowSource`. They are worth keeping adjacent to this residue rather than filed elsewhere:
`C69` is the merge-mining half of the same question ("what authenticates a block's claim about an
external chain?"), and it is the more severe of the two.

**A coverage gap found alongside them, now closed, recorded because it bears on how much these rows can
be trusted.** Merge mining's security argument is three receipts, and two of them had **no test in any
feature configuration**: `extract_aux_merkle_root` (Receipt 1) and `is_coinbase_valid_merkle_root` with
`check_coinbase_path` (Receipt 3) were reachable only from `validation.rs:82`, `block_acceptor.rs:194`
and `mm_rpc.rs:509`/`:561`. The one test that touched them, `test_monero_powdata_serde`, is
`#[cfg(feature = "async")]` and exercised them incidentally. Four tests were added on 2026-09-22:
Receipt 1 against the real merge-mined testnet block plus absent-tag and **two-tag ambiguity** controls
(the second matters because `mm_submit_solution` compares the submitted proof against the extracted root,
so a coinbase carrying two tags would let a submitter choose which to satisfy — the code refuses that,
and nothing had checked it); and Receipt 3 with a positive control on the real block plus two negatives
that attack its two halves separately. The positive control passing is itself a result: it establishes
that the coinbase-hash reconstruction — prefix keccak state, `tx_extra`, the null `RctSigBase` and a null
hash — is correct for a real Monero block, which nothing had verified.

| ID | Status | Proposition | Source | Sev |
|---|---|---|---|
| OBL-C63 | **CLOSED** | Finality is conferred by a *verified* anchor, not by a header field the relaying peer chooses. **CLOSED 2026-09-22 (`5c6bf01a7c`).** Both enforcement sites (`chain_state.rs:1011`, `:1546`) now require `caribina::verify_anchor_proof(&header)` — a pure local function, so consensus can call it — and share one predicate, so a block `connect_block` treats as un-replaceable cannot be one `detect_reorg` is willing to reorg away. The mirror direction the row insisted on holds: a block asserting an anchor it cannot prove confers **no** finality, and is deliberately not rejected either, because rejection would let any peer halt the chain with a malformed relay whereas ignoring the claim costs only that block's finality. That asymmetry is what removes the free chain-freeze. Proven by the positive control and the forged-key control in `chain_state::tests::test_finality_conferred_without_any_anchor_verification`, and the eight negatives in `caribina::verify::tests` | `chain_state.rs:1011`, `:1546`; `caribina/verify.rs` | C |
| OBL-C64 | **CLOSED** | The finality fields are authenticated by the proof-of-work that produced the block. **CLOSED for the field that decides finality, 2026-09-22 (`5c6bf01a7c`).** `anchor_owner` — a fresh per-block Ed25519 public key — is now **inside** the mining blob (bytes 260..292; blob 260 → 292), so a relaying peer cannot re-attribute an anchor to a block it did not mine without redoing the proof-of-work, and one PoW solution no longer admits headers differing in the authenticating field. The four older fields (`anchor_tx_id`, `anchor_monero_*`, `finality_flags`) remain outside the blob, because they are set after the nonce and must not invalidate the solution they annotate — and they are no longer consulted for finality at all, so their malleability buys an attacker nothing. Asserted by `block::tests::test_anchor_owner_is_pow_covered_but_post_mining_fields_are_not`, which is the flipped form of the characterization test this row was filed against | `block.rs` (`to_mining_blob`, `ANCHOR_OWNER_OFFSET`) | C |
| OBL-C65 | **CLOSED** | Enforcement never outruns verification. **CLOSED 2026-09-22 (`5c6bf01a7c`), by construction rather than by a corrected predicate.** `should_verify_anchor` and `should_verify_monero_anchor` had no production caller and returned `false` whenever their `*_enabled` flag was off while `should_enforce` ignored those flags, so a configuration existed that enforced anchors it had decided not to check — and `--finality-disable-caribina`, which `bin/dwowd/src/main.rs` sets, was exactly that configuration. Both methods are **removed** rather than fixed: a dead method encoding a wrong invariant is worse than no method, and "a verifier with no caller" is this residue's root cause. Enforcement and verification are now the same call at both sites, so they cannot disagree. `finality::tests::test_enforce_decision_depends_only_on_mode_and_flags` replaces the removed tests and asserts no `*_enabled` flag leaks into the enforcement decision | `finality.rs` | H |
| OBL-C66 | **ACCEPTED-WITH-REASON** | Caribina anchors a block to a *settled* Arweave block. **ACCEPTED-WITH-REASON 2026-09-22, premise restated.** Settlement is not checked, and **cannot be**: it lives on another chain and consensus must be a pure function of local data — the same reason `verify_monero_anchor` is uncallable from consensus. What the anchor establishes instead is **publication**: the miner committed a key inside the block's mined region, signed a DataItem binding that block, and published it — and the property the 51% argument needs is that the adversary in scope cannot delete a DataItem, not that Arweave has buried it. Settlement depth is Arweave's business and this chain does not read it. The gap between "settled" and "published" is therefore **recorded, not narrowed**, and `doc/src/arch/caribina.md` no longer advertises a settlement time. The `ardrive.net`/`arweave.net` discrepancy in the gateway constant was resolved in the same pass, and the gateway path (`verify_anchor`) remains the opt-in live-conformance arm rather than a consensus input | `caribina/verify.rs`; `caribina.md` | M |
| OBL-C67 | **OPEN** | The Monero anchor is derived from the Monero proof, not asserted beside it — **and this row was wrong about what is derivable, and understated its own severity. Both corrected 2026-09-22 by reading the types. OPEN.** *What is impossible*: the Monero `BlockHeader` carries only `major_version`, `minor_version`, `timestamp`, `prev_id` and `nonce` — **no height and no difficulty**. So `anchor_monero_height` cannot be derived from the proof (in the testnet fixture the height, 2912484, exists only as a comment), and a locally-checked PoW is impossible too: Monero's difficulty is not in the block, and a submitter-chosen target is vacuous. Recording that is the honest outcome; the row is not narrowed to whatever happens to be achievable. *What is derivable*: `anchor_monero_hash`, via `Keccak-256(VarInt(len) ‖ create_blockhashing_blob(header, merkle_root, tx_count))` — currently assembled inline at `bin/explorer/src/rpc.rs:215-230` with no reusable helper. *The severity correction*: the three receipts prove only that our aux hash sits in a coinbase of **some serialized** Monero block — `is_coinbase_valid_merkle_root` recomputes the coinbase hash from submitter-supplied fields and compares it against a root the submitter also supplied — so a fabricated five-field header with any transaction list and the merge-mining tag in the coinbase passes all three. Merge-mined blocks skip native PoW entirely, so **this is a block-minting hole, not only a finality one: a fabricated merge-mined block is accepted today.** `get_block_by_hash` is absent from `src/linear/src/monero/rpc.rs`, which is what blocks the route its own `TODO(HAZOP F3)` at `mm_rpc.rs:574` recommends. Decision taken: add that RPC method and a **node-local admission policy** — a node with `monerod_url` admits a merge-mined block only if monerod confirms the Monero block exists at the claimed height and is `monero_min_confirmations` deep; with `monerod_url` unset (its default) it accepts as today **with a logged warning**, so the residual gap is stated in the log rather than implied away. Node-local because it is network-bound, exactly as `OBL-C66` is. Work named: `monero_block_hash` helper (**done** 2026-09-22, `359b1d6f3f`), derive-and-validate `anchor_monero_hash` (**done** in the same commit, with its three-direction test), `get_block_by_hash`, the admission policy, and the `bin/dwowd/src/tests/merge_mining.rs` assertions — plus a negative control the row lacks: a fabricated Monero block must not be admitted when the policy is on. **One item is deliberately deferred**: `src/sdk/src/blockchain.rs:885-891`'s doc comment on `MoneroBlockHeight` still claims the field "gates merge-mining finality", which it does not and has not since enforcement moved to the verified anchor proof. Correcting it is an `src/sdk` edit, `src/sdk/**` is in every contract's `SOURCE_MANIFEST`, and the genesis pin has already moved once this campaign — so it waits for the batched re-roll rather than invalidating all 32 artifacts for a comment | `monero/mod.rs` (`MoneroPowData`), `monero/rpc.rs`, `mm_rpc.rs:574`, `:630-631`; `src/sdk/src/blockchain.rs` | C | **Audited 2026-09-23: OPEN, and the residual is a genuine `C`.** With `monerod_url` unset — the default — the block is admitted, and `bin/dwowd/src/block_acceptor.rs:232` states it in the log: "Its receipts prove coinbase inclusion in some serialized block, which a fabricated block can also satisfy." **The policy's premise is corrected again, 2026-09-22, before any code was written.** The paragraph above states the policy as "monerod confirms the Monero block exists **at the claimed height**". There is no claimed height on this path, and there cannot be: production writes *both* `anchor_monero_height` and `anchor_monero_hash` **zero** for a merge-mined block (`mm_rpc.rs:638-639`, deliberately — nothing reads them), and the height is not derivable from the proof at all, which is this same row's first half. What *is* derivable is the **hash**: `MoneroPowData::block_hash()` (`monero/mod.rs:259-267`), which the header field must already equal when non-zero (`validation.rs:98-103`). So the admission policy must be **hash-driven**: derive the hash from the block's own proof, ask monerod whether it knows *that* hash, and take the confirmation depth from the height monerod reports for it. That is the strictly stronger direction — the lookup key is derived from the proof rather than supplied by the submitter — and the weaker one (query by claimed height, then compare) is not expressible for the blocks this policy governs. Recorded as a premise correction of the same kind as this row's "the height is derivable" half, not as a narrowing to what happens to be achievable. **Was `CLOSED 2026-09-22`; reopened 2026-09-23 on the residual** — see the audit note at the head of this cell, and the policy's own log line at `bin/dwowd/src/block_acceptor.rs:232`. |
- **`get_block_by_hash` exists** (`monero/rpc.rs`), mirroring `get_block_by_height` — same envelope, same `JsonRpcResult<GetBlockResponse>`, `params: {"hash": …}` — and it is the RPC client's first production caller. Four tests: success (asserting the height comes from monerod's reply, not the caller), an unknown hash (monerod answers with an `error` object, so the answer is an error rather than a silent pass), an empty hash, and connection-refused.
- **The policy is `verify_monero_powdata`** (`monero/verify.rs`), beside the sibling it extends: **hash-driven** (derive from the block's own proof, look *that* up, take the depth from the height monerod reports), and **fail closed on every failure to confirm** — transport, timeout, unknown hash, insufficient depth — which is this row's own decision ("only if monerod confirms") rather than a softened version of it. `monerod_url = None` returns `Ok` **having checked nothing**; the caller says so in its log.
- **It is seated in the node-local acceptor**, the `PowSource::Monero` branch of `block_acceptor.rs`, reached by all five entry points and never by `check_pow_stage`/`check_block_header`/`connect_block` — the same purity argument this row and `OBL-C66` make for settlement. When no URL is configured it emits a warning naming exactly what was not checked.
- **The negative control the row lacked now exists**: `test_merge_mined_block_rejected_when_monerod_cannot_confirm_it` drives the *same* block the acceptance test admits through a chain whose policy points at a closed port, and asserts the refusal comes from the policy and that the chain did not advance. What makes it a seating test rather than a policy test is that it fails if the acceptance path ever stops consulting the verifier — the campaign's recurring "verifier with no caller" shape, which no reading of the verifier can catch. The policy's own semantics (unknown hash, wrong hash, shallow depth, transport failure) are covered beside it in `monero::verify`, against the crate's real testnet fixture and the stub monerod.
- **Still deferred, and named**: `src/sdk/src/blockchain.rs:885-891`'s doc comment, which claims `MoneroBlockHeight` "gates merge-mining finality" — it does not, and correcting it invalidates all 32 contract artifacts, so it waits for the batched re-roll. **Also still true and now worth stating**: `verify_monero_anchor` itself remains without a production caller, because a *claimed* height-and-hash pair is a shape nothing produces — the policy above derives its hash instead. It is the arm a future height-carrying anchor would use, and dead until then |
| OBL-C68 | **FIXED** | Anchor acquisition is off the consensus path. `stratum_submit` calls the blocking `anchor_block` directly inside the async handler **while `linear_submit_lock` is still held** (`stratum.rs:561`, guard taken at `:348`), so one slow ArDrive Turbo POST serialises every stratum submission — and the whole handler blocks the async executor thread while it waits. The built-in miner does this correctly via `smol::unblock` (`miner.rs:293-307`). `doc/src/arch/consensus/stratum.md:145` describes the anchor as "best-effort, non-blocking", which is false for this path. The delay is **bounded at 30 s by `ureq`'s `timeout_global`**, which is a wall-clock deadline and not the per-IO timeout an earlier version of this row called it: the crate documents it as "end-to-end, from DNS lookup to finishing reading the response body. Thus it covers all other timeouts" (`ureq-3.4.0/src/config.rs:663-669`), verified while fixing this row. So the hazard is 30 seconds of serialised submissions per stuck anchor, not an unbounded wait. **This is the one row in this residue with no characterization test**, deliberately: the lock scope is not observable without a stub Turbo server and a live handler, so the lock half is proven by the Stage 4 integration test — a second submission must complete while the first's anchor call is outstanding — and the deadline half needs no test, because it is the dependency's documented behaviour rather than ours | `stratum.rs:348`, `:561`; `caribina/anchor.rs:84-86` | H | **FIXED 2026-09-22; the behavioural half is still owed.** The lock half is verified by reading: `stratum.rs:631` `drop(submit_guard)` runs *before* the POST at `:633`, and the guard is re-taken at `:658` before `accept_block`; the comment there records both the 30-second bound and why the release is safe (an interleaved submission is rejected by `accept_block`'s own height check). The row's own test — a second submission completing while the first's anchor call is outstanding — remains the Stage 4 integration test, so this marker must not be read as behavioural verification |
| OBL-C69 | **FIXED** | A merge-mined block survives the **P2P broadcast** with the proof that makes it merge-mined. `BlockBroadcast`'s wire codec is `serde_json` in all three directions — `Encodable` (`linear_broadcast.rs:112`), `AsyncEncodable` (`:144`), `AsyncDecodable` (`:172`) — and `BlockHeader::pow_source` is declared `#[serde(default = "PowSource::native", skip)]` (`block.rs:131`), so `pow_source` is **never written and always decodes as `Native`**. A relayed merge-mined block therefore arrives reclassified as native, and `block_acceptor.rs:191` takes the *native* branch, verifying RandomX over a header xmrig never hashed — the exact check the Monero branch exists to replace, and one the block cannot satisfy. **Demonstrated** by `block_broadcast_wire_drops_pow_source` (`bin/dwowd/src/tests/wire_format.rs`): the encoded JSON carries no `pow_source` and the round-trip decodes to `Native`. The rejection itself is *inferred*, not demonstrated, and the register says which: `block_acceptor` is invoked with `BlockTarget::MAX` throughout the test harness, under which every hash passes, so the harness structurally cannot reproduce the failure it would cause on a chain with a real target. Following the mechanism through, a merge-mined block cannot propagate by broadcast at all; it reaches a peer only by the sync path, whose codec (`src/linear/src/serial_sync.rs:116-180`) encodes the discriminator correctly. **Three serializations of one consensus field now exist and only two preserve it**: the canonical one (correct), the storage path — `chain_state.rs:1135` uses `dwow_serialize`, and `chain_state.rs` contains **zero** occurrences of `serde_json` — and the wire, which does not. Note that the comment at `bin/dwowd/src/tests/merge_mining.rs:300-303` blames `serde_json` for storage and concludes the loss is a "pre-existing bug, tracked separately"; that comment is stale in its attribution (storage is fine) and was the only record of the real defect, which is on the wire | `linear_broadcast.rs:112`,`:144`,`:172`; `block.rs:131`; `block_acceptor.rs:191` | C | **FIXED 2026-09-22, and the characterization test is flipped.** `PowSource` gained a codec of its own (`serial_sync.rs:155`, `:168`) whose unknown-discriminator arm **errors** instead of returning `Native` (`:174-180`), plus `Serialize`/`Deserialize` impls that carry that codec's bytes (`:192`, `:199`); `block.rs`'s `#[serde(skip, default = "PowSource::native")]` is now `#[serde(default = "PowSource::native")]` (`block.rs:169-170`), so the wire and the canonical encoding cannot disagree. The demonstrating test was replaced by `block_broadcast_wire_carries_pow_source` (`wire_format.rs:123`), which round-trips a real `PowSource::Monero` through the actual JSON codec |
| OBL-C71 | **FIXED** | A `DataItem` decoded from untrusted bytes cannot panic its verifier. `DataItem::deserialize` (`caribina/data_item.rs:218`) accepts any buffer of at least `HEADER_LENGTH` (116) whose first two bytes are signature type 2, and validates nothing else. Two accessors then slice on `tag_bytes` — an unvalidated `u64` read straight from bytes 108..116 — and **which one panics depends on `tag_count`**: with `tag_count == 0`, `raw_tags` returns early without slicing and `raw_data` panics at `:149` on `bytes[116 + tag_bytes..]`; with `tag_count != 0`, `raw_tags` panics first at `:214` on `bytes[116..116 + tag_bytes]`. Both are "range end index out of range", both in release as well as debug, and both are reachable from `verify_signature` (`:193` builds its item list from `raw_tags` then `raw_data`). A `tag_bytes` near `u64::MAX` additionally overflows the `usize` addition in `data_start` (`:153`), panicking in debug and silently wrapping in release — in release `raw_data` then returns the wrong region, so the payload comparison reads attacker-chosen bytes. **Observed, not argued**: `test_data_item_hostile_tag_bytes_do_not_panic_the_verifier` exercises both branches and the panic is at `data_item.rs:149`; the test is written with `catch_unwind` so the effect is measured rather than described. **Currently reached only through the dead verifier, which is exactly why it must land before Stage 3 wires that verifier into consensus**: the plan's own design makes this path parse proof bytes carried in a block, at which point a single crafted block panics every node that accepts it. The six pre-existing tests in this file cannot see it: every one of them round-trips an item this code produced, so `tag_bytes` is always honest | `caribina/data_item.rs:149`, `:153`, `:193`, `:214`, `:218` | C | **FIXED 2026-09-22.** `DataItem::deserialize` now validates the declared tag area *before* constructing the item — `usize::try_from` on the `u64` (`:251`), then `checked_add` (`:254`) and a bound check against the buffer (`:257`) — so both previously-panicking accessor branches (`:149`, `:214`) and the near-`u64::MAX` `usize` overflow that silently wrapped in release are all unreachable. The test that asserted the panic is now the regression control that asserts rejection, `test_data_item_hostile_tag_bytes_are_rejected_not_panicked` (`:574`), and it covers all three hostile cases separately: `tag_count == 0`, `tag_count != 0`, and `tag_bytes == u64::MAX` |
| OBL-C70 | **FIXED** | `MAX_BLOCK_SIZE` bounds the size it claims to bound. `MAX_BLOCK_SIZE` is the block-wire cap ("single source of truth pinned across nodes, L1 barrier #7"), but both enforcement sites measure a `serde_json` encoding — `block_acceptor.rs:164` for acceptance and `linear_broadcast.rs:175` for the wire — and that encoding omits `pow_source` (`OBL-C69`). For a merge-mined block the measured length therefore excludes the entire `MoneroPowData`: the Monero block header, both merkle proofs, and a `Vec<u8>` `tx_extra`. This is **not** the concern the comment at `block_acceptor.rs:167-172` already addresses — that one is serde *version* drift, mitigated by a 1% margin; this is a systematic omission of a whole field, which a 1% margin does not cover. Severity is bounded by the same comment's own reasoning (`MAX_BLOCK_SIZE` is declared a DoS gate rather than a consensus rule), which is why this is M and not C | `block_acceptor.rs:164-181`; `linear_broadcast.rs:175`; `block.rs:131` | M | **FIXED 2026-09-22, as a consequence of `OBL-C69` rather than by its own change.** With `pow_source` no longer absent from serde, the acceptance measure — `serde_json::to_vec(block).len()` at `block_acceptor.rs:161-166` — now *includes* the whole `MoneroPowData`, which was the systematic omission this row objects to. The wire site measures the raw buffer (`linear_broadcast.rs:175`), which was always the true wire size. The serde-version margin the neighbouring comment describes is untouched and still the only remaining slack |

| OBL-C76 | **OPEN** | A test that does not run in the configuration used to claim verification is not verification. Three mechanisms hide tests in this tree and they compound: **feature gates** — `src/linear/src/block.rs:703` declares `#[cfg(all(test, feature = "pow"))]`, so its entire test module is absent from a bare `cargo test -p dwow_chain --lib`, and `monero/mod.rs`'s `test_monero_powdata_serde` is `#[cfg(feature = "async")]`; **`#[ignore]`** — all eight Caribina integration tests; and the **name filters** in `heavyweight.sh`. Measured 2026-09-22: `cargo test -p dwow_chain --lib` lists **172** tests, `--all-features` lists **263**, and the 91 that differ are `validation` (26), `block` (23), `chain_state` (20), `consensus` (15), `sync_boundary` (4), `sync_connection` (1) and two Monero tests — precisely the consensus surface this campaign works on. **The gates are not affected**: `make test` runs `--release --all-features --workspace`, so `scripts/run-all-tests.sh` always exercised all 263. The defect is in the *ad-hoc verification command*, which is what one reaches for to check a single crate quickly — and this campaign's Stage 0 and Stage 1 commits both quoted `162 passed` from such a run, understating what had been verified and, for `chain_state.rs`, `block.rs` and the Monero receipts, claiming verification of tests that were never compiled. All pass under `--all-features` (re-measured the same day), so the findings stand and only the *claims* were under-run — which is the part worth pinning, because an unqualified pass count is indistinguishable from a run that skipped the test. A verification command must name its feature set | `block.rs:703`; `monero/mod.rs:484`; `caribina/integration_tests.rs` | M | **OPEN — it describes a verification *practice*, so it cannot be closed by a code change, only by a convention that a gate can enforce. Re-measured 2026-09-22, and it has not moved.** The three hiding mechanisms are all still present: `block.rs`'s test module is still `#[cfg(all(test, feature = "pow"))]` (`:703`), `monero/mod.rs`'s `test_monero_powdata_serde` is still `#[cfg(feature = "async")]`, and the Caribina integration tests are still `#[ignore]`d. The measurement is reproducible in one command — `cargo test -p dwow_chain --lib` lists **172** tests, `--all-features` lists **263** — and the 91 that differ are the consensus surface this campaign works on. What the row asks for is not a fix but a **convention**: every verification claim names its command and its feature set, which is a rule a reader follows and no test can enforce. This session is the first to apply it throughout — every command quoted in today's closures carries `--all-features` — and the practice still fails whenever someone writes "tests pass" without one, which is why the row stays open rather than being marked satisfied by improved behaviour |

**Fixed on 2026-09-22, in the rows above.** `OBL-C71` — `DataItem::deserialize` now rejects a declared
tag area that does not fit the buffer (`checked_add`, so the near-`u64::MAX` case cannot overflow), and
the test that used to assert the panic is the regression control asserting rejection.
`OBL-C69` — `PowSource` gained a codec of its own (`src/linear/src/serial_sync.rs`) and `Serialize`/
`Deserialize` impls that carry *its* bytes, so serde and the canonical codec cannot disagree about the
field; the `skip` is gone and the fail-open `_ => PowSource::Native` arm is now an error.
`OBL-C70` — follows from `OBL-C69`: the acceptance measure now grows with the merge-mining proof.
`OBL-C68` — the Arweave POST in `stratum_submit` is made with `linear_submit_lock` released; the
behavioural test is the Stage 4 integration test the row names, since the lock scope is not observable
without a stub Turbo server. `OBL-C76` — recorded, not fixed: it describes a verification *practice*.

**Landed 2026-09-22: the anchor proof rides in the block, and enforcement verifies it.** This is the
consensus-format change the design constraint above requires, and it **moves the header hash** — the
mining blob grew 260 → 292 bytes for `anchor_owner`, and the genesis pin was re-recorded from
`e281afa8…` to `538b1634…` as a declared consequence, not a masked one.

- **`OBL-C63` — CLOSED.** Both enforcement sites (`chain_state.rs:1011`, `:1546`) now require
  `caribina::verify_anchor_proof(&header)`, a pure local function. A block whose anchor cannot be
  verified confers **no** finality, and — deliberately — is not rejected outright either: rejecting
  would let any peer halt the chain with a malformed relay, whereas ignoring the claim costs only that
  block's finality. That asymmetry is what removes the free chain-freeze. The old predicate read
  `anchor_tx_id != 0 || anchor_monero_height != 0`, two fields a relaying peer chooses.
- **`OBL-C64` — CLOSED for the field that matters.** `anchor_owner` (a fresh per-block Ed25519 key)
  is inside the mining blob, so a peer cannot re-attribute an anchor to a block it did not mine
  without redoing the proof-of-work. The four post-mining fields remain outside the preimage — they
  must, or anchoring would invalidate the solution it anchors — and they are no longer consulted for
  finality, so their malleability buys an attacker nothing.
- **`OBL-C65` — CLOSED by construction, and the wrong invariant deleted.** `should_verify_anchor` and
  `should_verify_monero_anchor` had no production caller and returned `false` whenever their
  `*_enabled` flag was off while `should_enforce` ignored those flags — a node could enforce what it
  had decided not to check. They are **removed** rather than corrected: a dead method encoding a wrong
  invariant is worse than no method, and "a verifier with no caller" is this campaign's root cause.
  Enforcement and verification are now literally the same call.
- **`OBL-C66` — recorded here as still open; superseded later the same day, and now
  ACCEPTED-WITH-REASON.** An anchor is *verified*; Arweave *settlement*
  is not checked at all. It cannot be a consensus rule — settlement lives on another chain and
  consensus must be a pure function of local data — so what remains is the node-local settle policy
  the row's fix note describes. Recorded as open rather than quietly implied to be done.
  **Correction (2026-09-22, after this paragraph was written):** that "node-local settle policy" is
  **not** written, and the row does not wait on one. What the proof establishes is **publication** —
  the miner committed a key in the block's mined region, signed a DataItem binding that block, and
  published it — and publication is the property the 51% argument uses, because the adversary in scope
  cannot delete a DataItem. Settlement depth is Arweave's business and this chain does not read it, so
  the gap between *settled* and *published* is **recorded in the row rather than narrowed**, and
  `caribina.md` no longer advertises a settlement time. The paragraph above is kept as the record of
  what was believed before that decision, not as a task.
- **`OBL-C67` — still open.** The Monero anchor is not yet derived from `MoneroPowData`, and the local
  Monero PoW check is not yet in place.
- **Anchoring is now real on both miner paths.** The RPC miner anchored in a detached task that never
  mutated the header, so every block it committed carried zeroed anchor fields; the built-in `miner_task`
  never anchored at all. Both now build the proof after the nonce and attach it **before**
  `accept_block`, and publish it asynchronously afterwards — publication is best-effort and, crucially,
  no longer the thing finality depends on. Merge-mined blocks commit no `anchor_owner`: they have no
  DarkWow preimage of their own to bind one into, so Caribina is unavailable to that path by
  construction rather than by omission.
- **Tests.** `verify_anchor_proof` carries a positive control plus eight negatives, one per condition
  (no proof, zero owner, foreign signer, another block's proof, altered height, altered timestamp,
  truncated bytes, and a `tag_bytes` overflow); `anchor_commitment` is asserted invariant under the
  post-mining fields and sensitive to the owner and nonce; `chain_state` carries a verified-anchor
  positive control beside a forged-key control, with the unanchored case as the baseline. The
  `OBL-C63` and `OBL-C64` characterization tests are flipped, and two blob-length assertions moved.

**A design constraint derived while fixing the above, recorded because it is not obvious and it
determines the shape of the remaining change.** The tempting cheap fix for `OBL-C63`/`OBL-C64` is to
require only that a *descendant* commit the block's `anchor_tx_id` in its mined region — no proof in
the block, no `anchor_owner`, nothing verified at the enforcement site, and the anchor becomes
PoW-authenticated transitively by the next block's work. **That fix is unsound, and not marginally.**
`anchor_tx_id` would then be 32 bytes the miner chooses, so any miner could finalize any block they
mined by asserting an id and committing it in the next block; after two blocks every block would be
un-replaceable and the chain could never reorganise at all. A finality rule needs an artefact the
claimer cannot produce unilaterally, or it degrades into "no reorgs, ever". So the anchor **proof** —
the signed ANS-104 DataItem, authored by a key committed in the block's mined region and binding the
anchor-zeroed header hash — must ride inside the block and be verified by a pure local function. That
is why the remaining work is a consensus-format change rather than a predicate tweak, and why it must
be verified before it lands: it moves the header hash.

**One further defect, recorded but not opened as a finality row.** `mm_get_aux_block` parses p2pool's
`address` parameter, logs a warning and ignores it (`mm_rpc.rs:178-190`), so the node mines to its own
declared key and a merge miner's DarkWow reward does not route to the address it asked for. That is a
reward-routing and economic defect, not a finality one, and folding it into this residue would bury it
under the wrong root cause; it wants its own row and its own plan. Until then it is pinned by a test so
it cannot drift silently.

**Process items, not obligations.** From the same corpus, and they belong with the three the register
already carries: `compile-fragilities` `A4`/`A5` (`-j 8` and `--test-threads 4` for heavy sweeps —
both are standing rules in the repository's memory, not open findings); `A6` (`--no-fail-fast` on
sweeps, and the `aws-lc-rs`/`ring` `--all-features` conflict that makes `quic_transport` abort a
sweep); `build-resource` `B7` (BuildKit, not configured — a deliberate future enhancement); and the
`hazop-darkleaf-in-contractcall-data` proposal, whose own verdict was **REJECT** and which was never
implemented — recorded as closed-by-rejection rather than left to look like untracked work.

### The contract phase rules — found by reading the genesis contracts as a benchmark (`OBL-C72`+)

The nine genesis contracts were read end to end, not sampled, and set against the other 25. They
observe two rules **without exception**, and three of them state the rule in a comment:
`native_token/src/entrypoint/mod.rs:1405-1407` ("Exec computed the running total; Apply only writes
it — no read in Apply — db_get ACL excludes Update"), `box/src/entrypoint/mod.rs:143-144`, and
`purse/src/entrypoint/mod.rs:202`. Both rules are normative in
[contract-wasm-type-system.md](../arch/contract-wasm-type-system.md) §A.4.7 and §B.2.2 and enforced
mechanically by the host ACL. **Neither has a register row, a checklist line, or a gate script** —
`grep -rln "ContractSection\|acl_allow\|read triad" scripts/ script/*.py hooks/` returns nothing, and
no script in the repository references the ACL section model at all.

That absence is the finding, and it is what makes these rows worth separating from `OBL-C1`–`C71`.
Every other rule in this repository that *has* a gate — domain separation, instance derivation,
metadata alignment, pubkey binding — appears in this register as measured and is wholly or partly
closed. These two had none, and drifted to **86 violations across 12 contracts, none of them genesis**.

The measurement is `scripts/check-phase-host-functions.sh`, which parses each contract's declared
`define_contract!` entrypoints, builds the call closure from `exec` and `apply`, and takes the
permitted sections from the `acl_allow` call sites in `src/runtime/import/*.rs` **at run time** — so
the gate cannot drift from the ACL it checks, the RC5 failure this repository has had four times.
**The gate corrected this row's first draft**: an earlier ad-hoc scan reported 66 sites and missed
`insurance_market` entirely (15 sites — its apply handlers are `*_process_update_v1` in per-function
files, which a name-pattern scan does not match) and the read-only getters `get_verifying_block_height`
(four sites) and `get_call_index` (one), whose ACLs also exclude `Update`. The 86 is the gate's
number; 66 was a pattern-matching artifact.

**Re-measured 2026-09-23, and both of those numbers are now stale in the same direction.** The gate
reports **7 violations, in one contract**, all one class:

```
$ bash scripts/check-phase-host-functions.sh ; echo EXIT=$?
FAIL: src/contract/drain_protection/src/entrypoint.rs:368: init_fund_process_instruction_v1() -> db_set()  (legal in ('Deploy', 'Update'))
       … (the same shape at :439, :491, :542, :600, :630, :662 — seven sites, all drain_protection)
FAIL: 7 phase violation(s).
EXIT=1
```

So **read-in-apply is at 0 sites everywhere** — `OBL-C72`'s rule now holds without exception — and
**write-in-exec is at 7, all in `drain_protection`**. Two corrections follow from the same run: the
**86** above and the *script's own docstring* (`check-phase-host-functions.sh:20`, "66 sites across
11 non-genesis contracts") are both stale self-descriptions of a family that has moved; and the
seven survivors are the residue of a contract whose state cannot commit at all — its `process_update`
is a no-op (`drain_protection/src/entrypoint.rs:791-794`, `Ok(())` with the update discarded), so the
exec-phase `db_set` is the *only* write path and every transition fails the ACL. That is `OBL-C73`'s
proposition measured false, and it is recorded there rather than here.

**And the gate that found this is not wired into `run-all-tests.sh`.** Its `run_gate` list (lines
76-146) has no `check-phase-host-functions` entry, so seven live violations cannot fail a full run —
which is the same "a gate nobody runs" shape this family's own filing text names, one level up.

| ID | Status | Proposition | Source | Sev |
|---|---|---|---|
| OBL-C72 | **SATISFIED** | The `apply` entrypoint performs blind writes only: no `db_get`, `db_contains_key`, `get_object_size` or `get_object_bytes` is reachable from it, and any value apply needs is computed in `exec` and carried through the update struct | §A.4.7, §B.2.2; `src/runtime/import/db.rs:638`,`:787`,`:1325`,`:1551`; `vm_runtime.rs:954` | M |
| OBL-C73 | **OPEN** | A state write (`db_set`, `db_del`, `merkle_add`, `sparse_merkle_insert_batch`) is not reachable from an `exec`-phase function; all mutation is in `apply` | §A.4.7, §B.2.2; `src/runtime/import/db.rs:358`,`:504`; `merkle.rs:50`; `smt.rs:142` | M |
| OBL-C74 | **PARTLY** | Every contract declares in its manifest the barbs each action requires, the capabilities it defines, its note schema and its capability primitives — the declaration `type-system.md` §13 and `ocap.md` §7 make the basis of `wallet_construct` | `manifest.md`; `ocap.md` §7; `contract-wasm-type-system.md` §A.2.2, §A.0.3, §13 | M |
| OBL-C75 | **OPEN** | The id a host uses to key state is the id its circuit derives and `get_metadata` publishes — the host does not independently recompute it from data the client cannot know | `safety.md` RC5 (one fact, two sources of truth); `contract-wasm-type-system.md` §A.1.6, §A.2 | M |
| OBL-C77 | **OPEN** | A metadata arm for a callable non-ZK function returns an **encoded** empty public-input vector, not a bare empty `Vec` | `execution.rs:423`; `native_token/src/entrypoint/mod.rs:922` (`plaintext_call_get_metadata`, the reference); contract-standards.md §3 | M |
| OBL-C78 | **FAILS** | A contract's `get_metadata` publishes, for each circuit, a public-input vector whose **count and order** agree with the circuit's `constrain_instance` list; the tx pair is `poseidon_hash([3, tx_commitment, tx_nonce])` and is never a literal zero | `pool_stake/src/client/create_pool.rs:84-90` (the worked template); `src/sdk/src/crypto/constants.rs:57`; `check-circuit-metadata-alignment.sh`; `privacy.md` §5.3 | M |
| OBL-C79 | **SATISFIED** | A checker enumerates the **whole class** it claims to check, reports the scope it did not examine, and reconciles the set it walked against the set it declared | `check-circuit-metadata-alignment.sh:71` (`GENESIS`, the counter-example); the four gates that already glob (`check-circuit-domain-separation.sh:27`, `check-pubkey-binding.sh:44`, `check-artifact-freshness.sh:67`, `check-phase-host-functions.sh:175`) | M |

**OBL-C72 — read-in-apply. 66 sites, 9 contracts, zero genesis.** §A.4.7 states the rule as a list
of four denied functions and §B.2.2 supplies the consequence verbatim: *"An `apply` function that
calls any read-triad function will fail at runtime with `CALLER_ACCESS_DENIED`."* The mechanism is
not partial: `vm_runtime.rs:954` runs apply as `ContractSection::Update`, and **no** read function
admits `Update` — not `db_get`, not its `_local` variant, not `db_contains_key`. The class is wider
than the four names §A.4.7 lists: every read-only getter carries the same `[Deploy, Metadata, Exec]`
ACL, so `get_verifying_block_height` (four sites) and `get_call_index` (one) are in it too. There is
no workaround idiom, so every one of these sites is a call that cannot succeed.

| contract | sites | first example |
|---|---|---|
| `labor_market` | 24 | `entrypoint.rs:920 create_job_apply_v1()` → `db_contains_key` |
| `insurance_market` | 15 | `entrypoint/close_market.rs:63 …_process_update_v1()` → `db_get` |
| `dao_escrow` | 12 | `entrypoint.rs:554 pay_premium_apply_v1()` → `db_get` |
| `pool_stake` | 9 | `entrypoint.rs:487 apply_join_pool_update()` → `db_get` |
| `subscription` | 2 | `entrypoint.rs:669 update_usage_apply_v1()` → `db_get` |
| `bearer_bond` | 1 | `entrypoint/mod.rs:1237 apply_prove_coverage()` → `db_get` |
| `bridge` | 1 | `entrypoint.rs:811 get_current_timestamp()` → `db_get` |
| `relayer_endowment` | 1 | `relayer.rs:379 apply_register_fee_schedule()` → `db_get` |
| `stablecoin` | 1 | `entrypoint.rs:1738 apply_redeem_stable_update()` → `db_contains_key` |

**Confirmed at runtime, not only by reading**: `pool_stake`'s heavyweight suite fails at apply with
`ContractError(CallerAccessDenied)`, and the first statement of `apply_join_pool_update` is a
`db_get` on the registry tree. This row exists because that failure was previously recorded as "a
fresh thread" while §B.2.2 had already named both the rule and the error — the row is placed here so
the next reader does not have to rediscover it.

**OBL-C73 — write-in-exec. 20 sites, 4 contracts, zero genesis.** The mirror rule. `db_set`/`db_del`
admit `[Deploy, Update]` only and `merkle_add` admits `[Update]`, so a write in exec fails with the
same `CALLER_ACCESS_DENIED`:

| contract | sites | first example |
|---|---|---|
| `tender` | 9 | `entrypoint.rs:484 create_tender_v1()` → `db_set` |
| `drain_protection` | 7 | `entrypoint.rs:368 init_fund_process_instruction_v1()` → `db_set` |
| `pool_stake` | 2 | `entrypoint.rs:532 process_leave_pool_instruction()` → `db_set` |
| `dex` | 2 | `entrypoint/set_transparency_level.rs:42` → `db_set` |

`pool_stake:532` is the already-recorded LeavePool write; `:1112`
(`process_rebalance_pool_shares_instruction`) is its sibling. Union with `OBL-C72`: **12 distinct
contracts, 86 sites, zero genesis.**

**OBL-C74 — the manifest capability block exists only in genesis.** `type-system.md` §13 makes
`wallet_construct` *"a pure function of primitives and required barbs, not contract names"*, and
§A.2.2 makes the barb declaration mandatory: *"Every contract SHALL declare in its manifest which
barbs each action requires."* Measured across all 32 manifests: `[[actions]]` is present in 9 (the 8
genesis contracts that have it, plus `dex`), `required_barbs` / `note_schema` / `primitives` in **8,
all genesis** — and in **none** of the 24 non-genesis contracts. For those contracts the declaration
§A.0.3 builds the fundamental invariant on does not exist, so the invariant cannot be stated and the
generic wallet has nothing to compose against. `identity` is the genesis exception, missing all six
of `[[capabilities]]`, `[[actions]]`, `required_barbs`, `note_schema`, `primitives` and `witness_map`
— consistent with it being the one genesis contract that documents a `Box::Take` child call it never
enforces (`entrypoint.rs:670-677`, `OBL-Z17`). `witness_map` is present in 3 of 32, and §A.6.1
already records that nothing validates it. **Not part of this row, and recorded so it is not read as
one:** `[[cost_profiles]]` is absent from **all 32** manifests, genesis included — a schema-wide gap
rather than a genesis/non-genesis one.

**OBL-C75 — a published public input the host does not use.** Found while fixing `pool_stake`'s
`OBL-C72`/`OBL-C73` sites, and stated separately because it is a different failure: the circuit
derives an id, `get_metadata` publishes it, and the host then recomputes a *different* id and keys
state by that. The published input is decorative — the proof binds a value nothing consumes — and
because the recomputation fed on `get_verifying_block_height()`, **the client could not predict the
id of the record it had just created**: the height is not known when a proof is built. Three live
instances, all in `pool_stake`: `CreatePoolV1` ignored `derived_pool_id` and keyed the pool by
`poseidon_hash([height])`, `JoinPoolV1` ignored `derived_member_id`, and `AllocateCoverageV1` ignored
`derived_allocation_id`. Two pools created in one block also collapsed onto one key. Fixed
2026-09-22 by using the proof-bound field in each case and deleting the two `derive_*` helpers, which
had no other caller.

**The class is bounded, and this is how.** A scan for host-side id derivations that depend on the
verifying block height — the sharp form of the defect, since a derivation over values the client
already knows is merely a second source of truth, not unknowable — now finds **one** remaining, and
it is **dead code**: `game_room/src/model/mod.rs:367 derive_room_id()` with zero call sites. The
register carries it here rather than in a row of its own because an uncalled function cannot key
anything; it should be deleted when `game_room` is next touched, not before, so the deletion is not
mistaken for a behaviour change.

**OBL-C77 — the bare-empty metadata arm, and why it is not a style nit.** `contract-standards.md` §3
makes an *empty* metadata buffer the documented rejection signal, and the host implements exactly
that: it decodes the metadata as `Vec<(String, Vec<pallas::Base>)>` (`execution.rs:423`) and, when the
buffer is zero bytes, reports *"contract signalled EMPTY metadata, which is the documented rejection
signal"*. So an arm written `F => vec![]` — a bare `Vec`, not an *encoded* empty one — is a function
that **cannot be called at all**: the host rejects every invocation before exec runs. The correct
form is what `native_token`'s `plaintext_call_get_metadata` (`entrypoint/mod.rs:922`) does for a
no-proofs call: encode both empty vectors, so the buffer is a valid 4-byte (or 8-byte) encoding
rather than nothing. `pool_stake`'s `LeavePoolV1` reached this at runtime
(`metadata-decode-zkp … contract signalled EMPTY metadata`, block 6) and is fixed.

**Measured scope, separated by reachability, because most of these arms are dead.** Across the tree
the bare-empty arm appears for:

- **Reachable** — real callable non-ZK functions: `insurance_market` 11 (`CreateMarketV1`,
  `RegisterRiskTypeV1`, `UnderwriteV1`, `FileClaimV1`, `ResolveClaimV1`, `WithdrawPremiumV1`,
  `UpdatePremiumV1`, `RetireRiskTypeV1`, `CloseMarketV1`, `DeactivateUnderwriterV1`,
  `ResolveClaimWithCapabilityV1`), `tender` 3 (`CancelTenderV1`, `RejectBidV1`,
  `CreateTenderWithCapabilityV1`), `pool_stake` 5 (fixed), and **`labor_market` 3**
  (`CancelV1`, `CreateJobWithCapabilityV1`, `CreateJobWithMilestonesAndCapabilityV1`,
  `entrypoint.rs:274-277`, `:341`, `:359`) — added 2026-09-22, having been absent from this
  scope in its first revision. `CancelV1` carries the comment *"CancelV1 has no ZK circuit"*: the
  intent is right and the encoding is wrong, which is why it is the arm that most looks
  deliberate and is still uncallable.
- **Unreachable by construction** — the `Initialize` arm in `box`, `purse`, `multisig`, `dex`,
  `otc_swap` and `escrow`. Initialization is the separate `__initialize` entrypoint, so no
  `ContractCall` ever dispatches to that arm. Recorded because it is the reason three *genesis*
  contracts (box, purse, multisig) carry the pattern without moving the pin — the arm is dead, not
  correct. It should still be fixed when each contract is next touched.
- **Catch-alls needing per-contract determination** — `baccarat` and `darktoshi_dice` write `_ =>
  vec![]`, so whether any function falls through depends on their enum coverage; that is a read per
  contract, not a global claim.

**Why these three and not more.** The other candidate classes found in the same pass were left out
deliberately, because the measurement that produced them is not trustworthy and a row that overstates
its evidence is worse than no row. The `set_return_data` selector-byte rule (`A.1.5`) was scored
wrongly three times — twice because the byte is pushed inside the `encode_*_update` helper rather
than at the call site — and its residual list (`bearer_bond` 8, `betting_stake` 1, `insurance_market`
1, `roulette` 1) is a **candidate list, not a finding**, needing each exec/apply pair read by hand.
The "version every state struct" rule was not measured at all: the scan counted `*PublicInputs` and
`*CallData` structs, which are not persisted state. The lesson is the one `safety.md` RC5 keeps
teaching — a scan that reads one side of a pair manufactures defects — and it is recorded here rather
than left in a scratch file.

**OBL-C78 — the host's public inputs and the circuit's instances must agree in count *and* order.**
The rule this row names is old; what is new is that it is measurable, and that measuring it shows the
class was never closed. `check-circuit-metadata-alignment.sh` has always been described as comparing
"position-for-position, three ways" — `circuit constrain_instance order == entrypoint metadata push
order == client to_vec order` — while the code compared **counts**, and only for the eleven contracts
in its allowlist. So `subscription/cancel` passed the gate while pushing `pallas::Base::zero()` at
**all four** of the positions its circuit constrains, and `stablecoin/init` passed while pushing
zeros where its circuit constrains `tx_binding` and `tx_nonce`. A vector of the right length whose
entries are wrong is what "counts agree" cannot express.

The convention is **not** an open design question, and the register previously deferred it as one
(*"it needs a decision about the convention, not a patch"*, on `dao_escrow`). It is implemented and
green in genesis: `tx_binding = poseidon_hash([3, tx_commitment, tx_nonce])`, the domain constant at
`src/sdk/src/crypto/constants.rs:57`, carried in params and published by
`box/src/entrypoint.rs:70-83` and `purse/src/entrypoint.rs:75-106`, with clients at
`promissory_note/src/client/issue.rs:181`, `register_type.rs:187`, `revoke.rs:293`. The worked
template is `pool_stake/src/client/create_pool.rs:73-144` — the contract repaired earlier in this
campaign — whose four parts are `compute_tx_binding()`, `compute_public_inputs()`, `to_witnesses()`
and `*_v1_proof()`, and whose comment names what it replaced: *"This was a literal `Base::zero()`
written as **both** the public input and the witness, while the circuit constrains the witness to
equal this hash — so the proof was unsatisfiable, not merely unbound."*

Seven contracts were known to be dead at their first ZK endpoint, in two shapes — **literal zero**
(`dao_escrow`, `drain_protection`, `pool_stake`, `subscription`: the client satisfies the circuit
with a value the circuit rejects, so the proof is unsatisfiable) and **omitted** (`insurance_market`,
`labor_market`, `tender`: the host publishes no tx pair at all while the circuits constrain both, so
the proof is unverifiable). `pool_stake` is fixed. The rest are measured per circuit in the plan
file, read from both sides of the pair: `labor_market` publishes three values where `create_job`'s
circuit constrains five, publishes `dao_escrow_bulla` where `dispute` requires `dispute_reason_hash`,
publishes `milestone_count` and `completed_payment` that `refund` does not constrain, and leaves
`milestone_payment`'s namespace pushed by nobody. This is why those suites are dead, and why every
other fix on those contracts lands unverified until it is closed.

**OBL-C79 — a gate must name what it did not check, and prove it checked the rest.**
`scripts/check-circuit-metadata-alignment.sh:71` defined an eleven-name list called `GENESIS` (holding
three non-genesis contracts, added one at a time as each was repaired) and iterated only that list.
The gate examined **11 of 32** contracts and printed `Passed: 70  Failed: 0` /
`PASS: All circuits have matching metadata push counts`, saying nothing about the 21 it skipped —
every broken-proof contract among them. `script/circuit_metadata_exceptions.txt` was empty, so this
was not a declared exception: the coverage was simply absent, and the scope was not reported. This is
the OBL-C78 class surviving inside the instrument built to detect it.

The other four gates do **not** share the shape, which bounds the row: `check-circuit-domain-
separation.sh:27`, `check-pubkey-binding.sh:44`, `check-artifact-freshness.sh:67`,
`check-phase-host-functions.sh:175` and `script/circuit_instance_derivation.py:520` all glob the whole
tree. This was the only allowlist.

Widening the coverage then exposed three latent parse bugs in the same gate, each of which would have
been filed as a defect against correct code — and did, before being caught by reading the file:
a path-qualified constant (`crate::DAO_ESCROW_ZKAS_INIT_NS_V2.to_string()`, which the pattern
`([A-Z0-9_]+)` cannot match because `crate` is lowercase) accounted for 34 phantom "no metadata push
carries" findings; the `let zk_public_inputs = vec![(NS.to_string(), vec![…])]` idiom, used by
`dao_escrow` and `game_room` with no `push` call at all, accounted for 19 more; and `game_room`'s
`get_metadata` living in `src/lib.rs:236` rather than an entrypoint file accounted for twelve. None
of the three was visible while the allowlist constrained which contracts were parsed.

The generalisation is the row. A checker's coverage is part of its claim, and a checker that reports
a verdict must report the scope it did not examine — which is why the script now prints its covered
and circuit-free sets, reconciles the walked set against the declared set so a future `continue`
cannot shrink coverage invisibly, and refuses a hand-picked single contract that the enumeration did
not itself find.

**And the register recorded this gate as satisfying an obligation it was not satisfying.** `OBL-C20`
— *"Public-input ordering is verified by parse-and-compare, not by count"* — was listed under
"Verified 2026-09-22: satisfied, with the evidence", citing this script as comparing
"position-for-position, three ways". It compared counts. **`OBL-C20` is not satisfied**, and the
verification pass that declared it satisfied is itself an instance of the failure the register
exists to catch: the evidence was the script's *description of itself* rather than its code.

## Verified 2026-09-22: what these rows actually are

Every row added or already present was re-read against the code before a remediation campaign was
planned against it. **Twenty are satisfied** and need no fix; five rest on premises that have
moved; one is worse than its row said; and **one was recorded as satisfied and is not** — `OBL-C20`,
corrected on 2026-09-22 when the gate it cited was read instead of trusted (`OBL-C79`). Recording
this is the point — a campaign that starts by
fixing things that are already fixed is the failure mode this register exists to prevent.

**A counting correction, appended 2026-09-22 because the two counts disagreed with each other and with
the list.** This section said "twenty-one" and the section below said "twenty-two"; the paragraph
immediately following enumerates **twenty** satisfied rows. A reader who greps the paragraph for IDs
finds **twenty-one**, because `OBL-C45` is named there only as a cross-reference — "the concern folds
into `OBL-C45`" — and `C45` is not satisfied; it carries that concern. The count is **twenty**, which is
the list; the two bare numerals were miscounts, and they are corrected in place rather than by rewriting
the paragraphs that carried them. This is the same defect class as `OBL-C10`'s ungreppable closure
marker: a register whose counts and statuses cannot be re-derived from its own rows stops being
auditable, which is the only thing it is for.

**A gap this correction found, and which was then closed rather than left recorded.** The twenty rows
enumerated below are satisfied *by that paragraph*; as of the correction above their own table rows
carried **no status marker** (`OBL-C18`, `C23`, `C24`, `C34`, `C36`, `C38`–`C41`, `C46`–`C49`, `C51`,
`C59`, `C60`, `Z15`, `T3`, `T6`, `T10`), so a reader grepping the tables for a closure marker missed
**eighteen of the twenty** — only `Z15` and `T10` carried one. Those markers were added on
2026-09-22 to the fourteen still missing one (`Z15`/`T10` already had theirs, and `T6` already read
**proved**), each phrased as what it is — *"the evidence is named in the 'Verified 2026-09-22'
section below"* — rather than as a fresh verdict, because the verification was that section's work
and this pass only made its result countable. The same was done for the four finality rows whose
fixes were recorded only in prose (`OBL-C68`–`C71`). With those, `scripts/register-status.sh` reports
the register's per-row status without a human counting by hand, which is the whole point.

**Satisfied, with the evidence.** `OBL-C18` — the cached `target[H-1]` fast path
(`src/linear/src/consensus.rs:359`, the "M-1 fix") bounds the traversal by `TIMESTAMP_WINDOW`, not by
genesis. `OBL-C23` — `HANDSHAKE_TIMEOUT = 15s` (`src/linear/src/sync_connection.rs:70`, carrying
the `M7.3` reference). `OBL-C24` — `disconnect_block` reverses all ten trees `connect_block` writes,
including `uncles`/`uncles_by_height` ("M7 — symmetric") and `contracts` via the per-block undo batch.
`OBL-C34` — the handshake now **admits** a peer presenting no genesis, validating it downstream
against the pin (`sync_connection.rs:441-455`); this is precisely the bootstrap path the daemon-sync
analysis said was missing, so that finding is closed. `OBL-C36` — no zero-vector fallback survives;
`OBL-C38` — `chain_state.rs` has **0** `.lock().unwrap()` and 66 poison-recovering sites (note: 7 bare
ones remain elsewhere in `bin/dwowd`, which is a *different* site set); `OBL-C39` — `uncle_coin_set`
is gone, uncle dedup is sled-backed via `stored_uncle_hashes()`; `OBL-C40` — `execution.rs:656-682`
rejects uncle-vs-uncle key collisions; `OBL-C41` — four `miner_task` error paths re-insert mempool
transactions; `OBL-C46` — the non-ZK fallback no longer exists, the template is plaintext by design;
`OBL-C47` — `Clone for PoWConsensus` builds fresh atomics and `save_to_batch` reads under `Acquire`;
`OBL-C48` — `execution.rs:717-748` detects Deployooor duplicate keys; `OBL-C49` (M-4, M-14) and
`OBL-C51` (M-13) — metadata decode failures now carry `fail_stage`, `total_supply` uses a typed
conversion, and `block.rs:612` uses `checked_sub` with a supply-invariant error; `OBL-C59` —
`prev_coin` no longer exists (the concern folds into `OBL-C45`); `OBL-C60` —
`expected_reward(height: BlockHeight)` is u64 throughout. On the other surfaces: `OBL-Z15`
(`circuit_metadata_exceptions.txt` is empty; `bearer_bond_commitment_vectors.rs` pins the values),
`OBL-T3` (all five barb representations agree), `OBL-T6` (`Capability/Inversion.lean:101`, proved and
one-directional), `OBL-T10` (zero code occurrences of `native_decide`/`ofReduceBool`/`trustCompiler`).

**Premises that moved, so the rows must not be read as written.** `OBL-C17` —
`reject_nondeterministic_features` is **not** a no-op; it is live at
`src/runtime/vm_runtime.rs:354`, so the question is now whether `0xFE` alone is covered.
`OBL-C22` — `perform_reorg` no longer exists; reorg is `reorg_to_heavier_chain`
(`bin/dwowd/src/task/consensus_linear.rs:137`), so the depth-cap question must be re-asked of it.
`OBL-C42` — **downgraded from "three of four call sites" to one of three**: `prepare_block` takes
last, and the other two add a compensating `put_competing_blocks` recovery, so data is no longer lost
even though the take still precedes the fallible work. `OBL-C29` — **better than "absent"**: Path A is
implemented, Path B is deferred *with its reason recorded in the Rust* (a plurality vote over peer
tips is a multi-peer decision a single connection cannot make), and what remains absent is the
`Off`/`Relaxed`/`Strict` mode parameterization and the Path-B vote itself.
`OBL-C31` — **downgraded to partial**: a forward-referenced derived operand is no longer unhandled
(`prover_impl.rs:487` errors on it), but nothing enforces the DAG *at parse time* as the row requires.

**One row is worse than recorded.** `OBL-C52` is not merely "dedup uses a header serialization" — the
two sides of the same set **disagree**. Insert uses `block.hash_with_vm()` (`chain_state.rs:890`,
`:966`); take, put and prune use `blake3::hash(&dwow_serialize(&b.header))` (`:700`, `:776`, `:794`).
So `take_competing_blocks` removes the wrong key, the block-hash entry survives, `put_competing_blocks`
inserts a second key, and a legitimately re-submitted block is dedup-rejected. That is a live
correctness bug in the competing-block path.

**Stale prose in three rows, corrected here.** `OBL-Z5` — `script/circuit_free_instances.txt` holds
**36** entries, not 43; the five whose mechanism is *absent* are confirmed as the dex fee and
bearer_bond's four coverage-report quantities. `OBL-Z6` — the cited
`src/sdk/src/crypto/sinsemilla.rs` **does not exist**; Sinsemilla lives at
`src/sdk/src/crypto/constants/sinsemilla.rs`, `merkle_node.rs` and `src/zk/vm.rs`. `OBL-Z16` — the
residue list now names `stablecoin`'s `report_timestamp` and `attest_value`'s `attestation_id`, both
since classified, so the live sites are the two `insurance_market` ones. `OBL-T5` and `OBL-T9` — the
counts moved with `oracleOperatorType`: **14** capability resources, not 12 or 13, and 17 primitives,
not 16.

## Audited 2026-09-23: every row, adversarially

All **107** rows were re-read against the code, not re-checked against their own prose. The pass had
one rule, and it is the rule this register has needed since `OBL-C20`:

> **A row earns `C` or `H` only if a path can be named from an untrusted input to the consequence.**
> A check that cannot fail, an absent gate, a doc/code drift, a detector's raw candidate, an unpinned
> test and a moved count are all **observations** — recorded, not ranked.

Closed rows are exempt: on a `CLOSED`/`SATISFIED`/`FIXED` row the letter names the class of property
the closure guards, which needs no path. That distinction is now stated in the status-vocabulary note
above, because its absence is what let this column be read two ways for as long as it has existed.

### The genuine findings — four, of 107 rows

| Row | Sev | What survived |
|---|---|---|
| `OBL-C21` | **C** | With `bridge-verify` off — and nothing in the tree enables it, the only mention being its declaration at `bridge/Cargo.toml:51` — an Ethereum deposit is accepted when `merkle_proof` is merely **non-empty** (`bridge/src/entrypoint.rs:320-333`; the `else` binds to the inner `if`, so a grep-based read concludes the opposite). The deposit requires a `promissory_note::issue_v1` child call (`:258-294`), so it **mints a wrapped note against no external backing**. Precondition: the bridge must be deployed — it is not genesis |
| `OBL-C67` | **C** | The merge-mining admission policy is opt-in on `monerod_url`; unset is the default, and `block_acceptor.rs:225-240` admits the block, its own warning stating that the receipts "prove coinbase inclusion in some serialized block, which a fabricated block can also satisfy". **A fabricated block is minted** |
| `OBL-C34` | **H** | **The genesis pin is not enforced on the bootstrap path.** `check_genesis_pin`'s only production call sites are `bin/dwowd/src/lib.rs:893`/`:910`, both inside `init_linear`'s `create_genesis == true` branch. Every miner and observer runs `CREATE_GENESIS=false`, dials with no genesis, is admitted, and the **only** gate on the received genesis is a 4-byte magic check (`consensus_linear.rs:382-388`) the code itself calls "defense-in-depth". `apply_genesis_deployments` then deploys whatever WASM the block carries — there is no pin comparison in that file. The comment the closure rested on (`sync_connection.rs:446`) names a call that does not happen |
| `OBL-Z16` | **H** | The two live sites are real, but they are `OBL-Z17` arriving at a consumer: `constrain_equal_base(capability_predicate_result, ONE)` pins a witness to a **constant**, so the circuit asserts a predicate it never computes, and the host checks only that the market *has* a capability configured |

`C21` and `C67` are the same shape — **verification implemented, not enabled by default** — which is
`RC9`, "safety that is opt-in", with two live value-bearing instances sitting in this register's own
rows and documented as accepted trade-offs rather than as one pattern.

`OBL-C34` is a different shape and worth naming separately: **a comment asserts the enforcement.** The
verification that a closure rests on was a description of a check, not the check — `OBL-C20`'s failure
mode, in the code rather than in a script.

### The refutations — this register's own failure mode, ten times

Ten rows were **refuted**: their claim is false of the code. Four of them failed the same way.

| Row | What was believed | What is true |
|---|---|---|
| `OBL-C31` | "nothing enforces the derived-rule DAG at parse time" | It is enforced at `src/sdk/src/prover.rs:201-217`, pinned by two tests — added `4a6e5c5609`, **before the row was written**. The row searched `bin/dww/src/prover_impl.rs` |
| `Z9` | "`multisig` still FAILS" | Fixed by `55a2de9c42`, the commit its own sibling `Z11` records. `signer_pub` survives only in comments and a stale README |
| `Z6` | cites `src/sdk/src/crypto/sinsemilla.rs` | **That file does not exist** — the constants live in `crypto/constants/sinsemilla.rs` |
| `C73` | the exec-write rule holds | It does not: `scripts/check-phase-host-functions.sh` reports **7** violations, all `drain_protection` |

**The generalisation: the register's method — search one file for a keyword, conclude "not enforced" —
is how four of these ten went wrong.** `OBL-C20` is the same error against a *script's* self-description
instead of a *file's* contents. A negative claim ("nothing enforces this") needs a whole-tree search
before it may be recorded, and this pass found four that did not have one.

The other six: `C12` (a clone *is* zeroized — `SecretKey` derives `Clone` **and** has `Drop`; the real
leak is raw `pallas::Base` copies through `inner()`), `C16` (the arithmetic 72/20/69 reproduces
exactly, but the classifier tests `Err`-**adjacency**, not guard direction — **4** sites of the shape
`if cid != ContractId::ZERO { validate }` skip the check entirely, and init writes `[0u8;32]` with no
other writer), `C28` (the dedup key is inserted *before* the per-height cap, so an over-cap key has no
removal path), `C51` (`saturating_add` is live on the cumulative supply in three places and a
`checked_add` was never introduced), `C37` (the decreasing-timestamp route is closed and the
**equal**-timestamp route to the same zero interval still reaches a branch that makes the target
~11% *easier* while its comment says "10% harder"), `Z18` (its one adjudicated-live site is
mis-cited; no host path consumes it).

`OBL-Z14` is the mirror image: recorded `FAILS`, and it is **closed** — the circuit instances the
reporter pair and the host rejects on the stored governance point.

### The status column, and why it is now enforced

Added to all seven tables immediately after `ID`. Before it, **40 of 107 rows were invisible to the
guard**: 31 carried their token somewhere other than the cell it read, and 9 carried none — while it
printed "107 rows walked". `scripts/register-status.sh --check` now reads a fixed cell and *requires*
a token there, negative-controlled (`REGISTER=<copy> … --check` fails on a blanked cell, exits 0 on
this file). Each row carries one word from the vocabulary above.

### What this pass did not do

- **It did not fix the defects.** Four genuine findings and ~24 severity demotions are recorded; the
  fixes are separate work. `OBL-C21` and `OBL-C67` are the two that should be decided first.
- **It did not adjudicate every candidate.** `OBL-Z18`'s remaining 56 sites and 13 of `OBL-Z16`'s 15
  are still unread — the bar forbids calling them defects, and this pass does not call them clean.
- **It did not build or test.** One runnable check was executed — `check-phase-host-functions.sh`, for
  the `C73` refutation — and `check-circuit-metadata-alignment.sh` for `C78`/`C20`. Where a claim's
  truth needed a build (a Lean count, a test's pass count), the row says so rather than implying
  coverage.

## What this register implies

Rewritten **2026-09-22**, after the docs clean-up promoted the remediated corpus's residue into this
register and every row was re-read against the code (see "Verified 2026-09-22" above — the **twenty**
rows enumerated there were already satisfied). Ordered by what unblocks the most, with severity
breaking ties:

1. ~~**OBL-C35, C52, C36-class: the consensus wrong-block and lost-block defects.**~~ **DONE, and
   this item was stale when it was written — corrected 2026-09-23.** Both named rows had already
   closed on 2026-09-22 and this list was not updated with them, which is the failure the register's
   own rule about prose exists to prevent ("a fix recorded only in prose is not recorded" — and its
   converse: a fix recorded in a row must not survive as an open item in prose).
   `OBL-C35` — closed `e252e4f35c`; `stratum_submit`'s `CanonicalExtension` arm now calls
   `broadcast_block` with the block's uncles (`bin/dwowd/src/rpc/stratum.rs:711`). `OBL-C52` —
   closed 2026-09-22; **all eight uses** of the dedup key now call one `competing_dedup_key`
   (`src/linear/src/chain_state.rs:763`), including the insert at `:834` and the take and prune at
   `:739`/`:852`, and `:949` carries the note recording that the key is that function's and not
   `hash_with_vm`'s. Verified on both sides of the set, 2026-09-23.
   **The live consensus residue is elsewhere**: see `OBL-C19` (a call that overruns its wall-clock
   budget is warned about, not terminated), `OBL-C44` (a reorg does not re-admit the disconnected
   transactions) and `OBL-C67` (a fabricated merge-mined block is admitted when `monerod_url` is
   unset, which is the default).
2. **OBL-C21 — the Bridge's cross-chain verification.** The largest item in the register. Two of five
   chains (Monero DLEq, Litecoin Merkle) are **already written and have never been compiled**, because
   no build enables `bridge-verify`; Ethereum MPT needs the traversal its RLP decoder is missing; Zcash
   Groth16 and Aztec PLONK need a pairing dependency that does not exist in this tree. The live
   vulnerability is narrower than the row's framing: with the feature off, an Ethereum deposit is
   accepted when its proof is merely *non-empty* (`entrypoint.rs:320-333`), which is RC1's shape.
3. **OBL-Z17 — the capability check's remaining half is *possession*.** Stage 1 and Stage 2 landed
   2026-09-21: the predicate is proved, and the proof is about a credential an issuer actually signed
   (schema, issuer, threshold and commitment are all checked against the stored record). What remains
   is that the credential is a *box* and nothing consumes it — issuance emits no `Box::Put`, and
   verification requires no `Box::Take`. It is plumbing before it is a check: no fixture has a real
   box in the tree with a real merkle path, so the host check that *is* possession turns a passing
   flow red today. Touches genesis `identity` and `box`.
4. **OBL-Z16 — 15 instances a prover can parametrise away**, two of them live (both
   `insurance_market`'s `with_capability` circuits, which expose a `required_capability_id` nothing
   verifies). The other thirteen are inert or prover-parametrised and the repair per site is small
   once read. `OBL-Z1` — the same rule as a checker — stands at **897 instances over 181 circuits,
   15 unclassified**.
5. **OBL-C16 — 72 `ContractId::ZERO` guards across 20 contracts**, every one of them fail-open: an
   unconfigured contract id skips the validation instead of rejecting. All non-genesis (verified:
   zero such sites in any genesis contract), so no re-roll.
6. **OBL-C5 and OBL-C3 are the supply chain**, and both are only partly proved: the Lean
   `SupplyChain` theorems are structural inductions that hold *for any* `reward`, conditional on the
   Pedersen assumptions. Making `reward` a definition transcribed from `blockchain.rs` and proving
   non-increase plus the tail floor is what turns them into statements about the real schedule.
   `reward_monotone` remains an assumption; the odd case is the obstruction, and `Emission.lean`
   states precisely why the current lemma does not reach it.
7. **OBL-Z6 and OBL-Z12 are Lean models, not code changes.** Z6 wants Sinsemilla formalized in place
   of the model's substitution (`HashOps.lean:189`) — the deployed `vm.rs` already uses the real
   primitive. Z12 wants the comparison chip's semantics modelled so "253-bit comparison plus
   `range_check`" implies the integer reading over `ZMod p`. Neither needs `src/zk/vm.rs` edited.
8. **OBL-T7 is still a name, not a proof.** `NoFreeInstances` is carried as a hypothesis of
   `capabilityType_of_circuitDerivable` but no theorem discharges it; interpreting it needs a Lean
   model of Halo2 constraint-system semantics, the PCS and Fiat–Shamir. Related and next: `OBL-T9` —
   the barb alphabet does not separate the types it claims to (13 of 14 resources subsumed, and
   `purse_deposit`/`purse_withdrawal` have identical barb sets) — which touches all five
   representations including `src/sdk/src/capability.rs` and so carries a re-roll.
9. **`OBL-Z3`, `OBL-Z4`, `OBL-Z8` are gates that check the wrong thing** — domain *presence* rather
   than correctness, a hook that cannot see an inline `constrain_instance(ec_get_x(pk))`, and a
   validator that checks structural validity rather than whether the committed `.zk.bin` matches the
   current `.zk` source. All three are shell and Python, with no build impact.

**And three things that are not obligations but process**, all learned on 2026-09-20:

* **the heavyweight pipeline was red on sixteen tests as of 2026-09-20**, and now visible:
  `cargo test --workspace` runs ~30 of them and `test_heavyweight_{auction, bridge, dao_escrow,
  drain_protection, escrow, insurance_market, labor_market, pool_stake, purse, relayer_endowment,
  subscription, tender, bearer_bond}` plus `relayer_lifecycle_heavyweight`,
  `recruitment_pipeline_call_data` and `test_pipeline` fail. **These could not be seen before**:
  `make test` stopped at the contract build, and the contract build stopped on artifacts whose
  recorded hashes did not match their sources. So "pre-existing" is *inferred* from each failure
  matching a recorded cause, not from a before-and-after run — the before was not obtainable. Treat
  the causes below as hypotheses to re-derive, not as measurements, and do not inherit the count.

  **A partial measurement now exists, and it is partial in a way that must not be quoted as the
  whole.** On 2026-09-22 a full sweep was run —
  `cargo test --release --all-features --workspace --no-fail-fast -j 8`, log preserved at
  `~/baseline_sweep_2026-09-22.log` — and was **stopped at the operator's direction** while still
  inside the `dwowd` test binary. Parsed with the repository's own harness
  (`contrib/test_verdicts.sh parse --causes`), the inventory is **1293 ok, 17 FAILED, 12 ignored**,
  saved as `~/baseline_sweep_2026-09-22.verdicts.tsv` — the first per-test verdict record this
  repository has had, which is what that script exists for. The three completed binaries' causes are
  recorded verbatim below; **the heavyweight failures have names only**, because panic detail prints
  in a binary's closing `failures:` section and that binary never finished — do not read a cause into
  them, and re-run rather than infer.
  **Two differences from the list above are already established and are the point of measuring.**
  `test_heavyweight_pool_stake` **passes** — the other session's phase-rule work fixed it, so it
  should come off the red list. And `test_heavyweight_slot` **fails**, which the list above does not
  mention at all: a red heavyweight test nobody had recorded. The heavyweight failures are `auction,
  bearer_bond, bridge, dao_escrow, drain_protection, escrow, insurance_market, labor_market, purse,
  relayer_endowment, slot, subscription, tender`, and `relayer_lifecycle_heavyweight`.

  **The three causes the completed binaries did print, verbatim:**
  - `thread 'test_multisig_encode_roundtrip' panicked at src/contract/test-harness/tests/encode_roundtrip.rs:191:5: decode failed for SignParamsV1: IoError("SignParamsV1: too short")` — `OBL-C62`'s encoder/decoder asymmetry arriving as its own test, exactly as that row predicts.
  - `thread 'test_confirm_milestone_params_encoding' panicked at src/contract/labor_market/tests/integration.rs:480:67: called \`Result::unwrap()\` on an \`Err\` value: Custom { kind: Other, error: "IO error: ConfirmMilestoneParamsV1: expected at least 208 bytes, got 211" }` — the regression `fecb3deb45` introduced by raising a decode guard to 240; **fixed the same day**, see that commit.
  - `thread 'governance_report_accepts_the_outstanding_ratio' panicked at src/contract/test-harness/tests/stablecoin_governance_report.rs:153:38: the honest ratio must prove and verify: "verification: InvalidProof"` — a governance-report circuit that cannot prove its own honest case. **This one is in no row**, and it is a genesis-adjacent contract; it needs its own finding.

  **`test_heavyweight_slot`, measured 2026-09-23** — the entry above never mentioned it, so its cause
  was captured rather than inherited: one targeted run on the already-built binary
  (`cargo test --release -p dwowd --all-features test_heavyweight_slot`, 6m26s wall / 56 core-minutes —
  longer than the estimate, and recorded because the next person will want the real figure), and it
  fails at **height 11** on `fn_code=0x02` — `RevealSpinV1` (`src/contract/slot/src/lib.rs:63-69`) —
  with the contract rejecting its own call:
  `Error: Custom("accept_block at height 11: canonical call failed at exec for tx 083f1cc2… call_idx=0 fn_code=0x02 (contract 21LYoife…): ContractError(Custom(20))")`.
  Heights 9 and 10 pass, so it is a specific late transition rather than a setup failure.
  **The reason is not known, and the register says why rather than guessing**: the run was made without
  `DWOW_TEST_LOGS=1`, so the contract's own `msg!` lines — the ones that would name which check returned
  exit code 20 — were discarded. The harness log ends at "submitting to accept_block" and the test
  output carries only `[DIAG] raw WASM exit code: 20 (0x14)`. **The next step is one re-run of the same
  single test with that variable set**, and it is the same trap this register already records for
  heavyweight tests: a contract's explanation is not kept unless it is asked for.

  **The re-run was made, 2026-09-23** (`DWOW_TEST_LOGS=1`, same single test, 5m08s / 28 core-minutes),
  and it changes the question rather than answering it — which is the result. With contract logs on,
  the timeline is: `commit_spin` commits a spin at bet 1000 and reports *"settle at block 11"*, then
  `reveal_spin` succeeds — *"Revealing spin …"*, *"revealed"*, *"Reveal confirmed"* — and only
  afterwards does an exec of the same function return the error. **The contract prints no `msg!` on the
  failing path**, so no log configuration can name the condition: the diagnosis has to come from the
  source, and that is what the run bought. From the source: `Custom(20)` is
  `SlotError::InvalidChildCall` (`slot/src/error.rs:110`), dispatch maps `0x02` to
  `reveal_spin_process_instruction_v1` (`entrypoint.rs:203`), and **that function has no
  `InvalidChildCall` site at all** — the variant's only three raise sites are `commit_spin`
  (`:307`/`:313`), `settle_spin` (`:552`/`:558`) and `cancel_spin` (`:673`/`:679`). So the failing exec
  returned an error its own code path cannot raise directly. **The reading was then done, and it
  eliminated two of the three explanations while exposing the fact that is missing.**

  *Eliminated:* `reveal_spin_process_instruction_v1` was read to its end — it can return only codes
  **1** (`SpinNotFound`), **3** (`InvalidSpinState`), **11** (`InvalidSignature`) and **18**
  (`HouseNotInitialized`), plus `?` on db/serde errors — so it cannot return 20, and its first
  statement is a `msg!` that **never printed in the failing block**, so the handler did not run at all.
  The same holds for the child contract: `promissory_note` has **no `Custom(20)` anywhere**, neither
  mapped in `error.rs` nor written inline, so the `IssueV1 = 0x02` child (children precede parents in
  the call list) cannot be the source either. And `fn_code` is genuinely that call's own selector — it
  is `job.call_data.first()` (`execution.rs:525`) — so the printed `0x02` is not a framing artefact.

  *What that leaves is a question the tree cannot answer by reading:* the error names the contract as
  the base58 id `21LYoifepcySKhyDA1vzxRDWGHyDizPQ8f11zSqhep7t`, and **no such id appears anywhere in
  the source** — it is derived at deploy time (there is no `SLOT_CONTRACT_ID` or equivalent constant to
  grep for), so "which contract is this?" has no static answer. That single fact decides between the
  remaining possibilities, and obtaining it is the next step rather than more reading: resolve the id
  to a contract (the harness knows the ids it deploys; a print, or a small derivation check, settles
  it), and the diagnosis closes from there. Recorded because the alternative — three plausible
  explanations and no way to choose — is how a red test sits for a month.

  *The derivation was then found, and it is one line of printing away.* Ids come from
  `derive_contract_id_from_name` (`tests/blockchain.rs:831`): a `hash = hash*31 + byte` fold over the
  *name*, then `pallas::Base::from(u64)` and `ContractId::from_base` — total by construction, and the
  comment there explains why the older `Result`-returning form was collapsed. So the instrument is a
  single `println!` of the ids the harness redeems against, or a check of that derivation — not
  another six-minute run. **A quick reproduction of the derivation did not match the failing id, and
  that is recorded rather than acted on**: either my hand-rolled base58 encoder is wrong (the target
  begins `2` while every name I encoded began with a letter), or the id a *deployed* contract carries
  is not the harness's name-derived one. The lesson is the same either way, and it is the one this
  session keeps re-learning: do not re-implement what the tree already has and will print for free.

  What is known is uneven, and 2026-09-22 narrowed it twice. `insurance_market::UnderwriteV1`'s halo2
  synthesis error is OBL-Z16 arriving as a runtime symptom. `purse::WithdrawV1` fails its own
  post-condition ("nullifier must exist after withdrawal"). **`bridge`'s recorded cause was wrong**:
  it is not blocked at `metadata-decode-zkp` — that gate was fixed by the `SerializedLen` work — and
  the current source cannot produce the `Custom(5)` the test reports on `WithdrawV1`, since no
  `InvalidMerkleProof` site exists on that path. The gitignored `.wasm` is timestamped three minutes
  *older* than the last source commit, so the first step there is a rebuild, not a diagnosis. **The
  "clients still build V1-shaped public inputs" cause for auction and dao_escrow is unverified**, and
  one candidate explanation is now ruled out: every live `_V2` namespace constant resolves to a
  circuit that exists (there are **124 dead *legacy* constants** across 24 contracts, which is
  misleading but not this). Three bridge test artifacts are stale for independent reasons — see
  Stage 3 of the remediation plan:

* nineteen of the repository's contract artifacts were stale for a day — committed `.source_hash`
  files that no longer matched committed sources — which made `make test` unrunnable from any clone
  until they were rebuilt. Whatever changes a contract's sources has to rebuild them in the same
  commit;
* **a params struct lives in three places**, and the third is easy to forget: the model
  (`src/contract/<name>/src/model/mod.rs`), the client, and the Python binding
  (`src/sdk/python/src/contract/<name>/`), which mirrors every field by hand. Dropping one field
  from the stablecoin governance report left `report_timestamp` referenced in the binding, which
  broke the whole workspace build — and, because every contract's source hash covers *all* of
  `src/sdk/**`, invalidated all 32 artifacts at once. A model change is a three-file change, and
  the rebuild follows it.

## Cross-references

* Root causes: `dev/contracts/safety.md` (`RC1`–`RC12`, and the alias table mapping every legacy
  finding-ID scheme into them). The 2026-07-31 audit corpus and the HAZOP findings documents were
  removed on 2026-09-22; their open items are the `OBL-C16`+ rows above, and their texts are in git
  history. Remaining: `arch/security-analysis.md`, `arch/audit/README.md`
* Lean HAZOP register: `proofs/lean/src/DarkFi/HAZOP.lean` + `HAZOP/{Critical,High,Elevated}.lean`
  (the circuit pass, and the assumption pass added alongside it)
* Assumption boundary: `proofs/lean/src/DarkFi/Axioms.lean`; checker `script/check_lean_axioms.py`
