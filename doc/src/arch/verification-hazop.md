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
fixed_base_mul_uses_constant
NoFreeInstances
pallasPrime
poseidon_collision_resistance
poseidon_hash_output
reward_monotone
variable_base_mul_is_prover_chosen
```

Nine, down from 34. Each carries its four fields in `Axioms.lean`; the classes are:

| assumption | why it is not proved | disposition |
|---|---|---|
| `poseidon_hash_output` / `poseidon_collision_resistance` | the sponge is not formalised | the two cryptographic assumptions; four binding theorems are proved *from* them |
| `pallasPrime` | `Nat.Prime` of a 254-bit modulus needs a Pratt certificate | **the** arithmetic assumption — replaced seven Pedersen postulates |
| `fixed_base_mul_uses_constant`, `variable_base_mul_is_prover_chosen` | the zkas VM's opcode dispatch is not modelled | model-to-implementation correspondence |
| `base_div_mul_cancel` | same `pallasPrime` fact, stated over `Int` | candidate for discharge once `pallasPrime` lands |
| `coinbase_blind` | the real blind is `f(prev_commitment, H)`; `f` is an implementation detail | free parameter |
| `reward_monotone` | needs monotonicity of `fixedPowDecay`'s bit-loop in `exp` | falsifiable claim about a computable function |
| `NoFreeInstances` | Halo2 semantics are not modelled | names the ZK obligation; consumed by nothing |

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

### The one that was silently false

**OBL-C1 was inert until recently.** The token-commit filter in
`verify_proof_of_token_balance` used `poseidon_hash([0,0])`, which matches no real DRKW call, so the
mass-balance anti-inflation check **passed vacuously** — every block satisfied it because nothing
was ever included. That is the failure mode this register exists to make visible: the check ran,
reported success, and verified nothing. The filter was fixed; the obligation remains worth stating
because the *silence* is what a regression would reproduce.

---

## Surface 2 — ZK circuits (`OBL-Z`)

**Inventory: 180 circuits.** `src/contract/*/proof/` holds **166** across **31** contracts (34
contract directories, 3 without a `proof/`), plus 12 in `proofs/core/` and 2 in
`bin/darkirc/proof/`. The figure "120 across 26 contracts" in `arch/zk/opcodes.md`,
`opcodes-status.md` and `security-analysis.md` is wrong, and the docs' own per-contract table sums
to 139. All 180 contain at least one `constrain_instance`; 122 use `constrain_equal_base`.

| ID | Proposition | Enforced at | Checked today by | Sev |
|---|---|---|---|---|
| OBL-Z1 | For every circuit and every `constrain_instance(X)`: `X` is either a pure opcode expression over witness bindings, or bound by `constrain_equal_base(derived, X)` before the expose, or a **declared** free witness | every `.zk` under `src/contract/*/proof/`, `proofs/core/`, `bin/darkirc/proof/` | **nothing.** The three existing gates are structural (see below); no mechanical check of this rule exists anywhere in the repo | C |
| OBL-Z2 | Each circuit's public-input metadata matches its `constrain_instance` set — **position for position**, not merely in count | `scripts/check-circuit-metadata-alignment.sh` vs the entrypoint's `zk_inputs.push` | the script, which compares counts only, covers 8 of 31 contracts, and currently **FAILS** on `native_token/fee` (15 `constrain_instance`, no `ZKAS_FEE_NS` push) | C |
| OBL-Z3 | Every `poseidon_hash` call in a circuit is domain-separated, and by the *right* constant | `scripts/check-circuit-domain-separation.sh` | the script checks that *some* `DOMAIN_`/`witness_base` prefix is present, never that it is the correct one for the hash's purpose | H |
| OBL-Z4 | A pubkey derived by `ec_mul_base` + `ec_get_x/y` is bound by `constrain_equal_base` before being exposed | `hooks/pre-commit` | the hook — line-anchored, single-assignment, staged files only, and it cannot see an inline `constrain_instance(ec_get_x(pk))` | C |
| OBL-Z5 | The deliberately free witnesses are enumerated and each carries its host-side obligation | `native_token/proof/mint.zk` (`total_pin`, host-bound to the header's `total_reward`; `tx_nonce`, paired with `tx_binding`) | nothing — they are prose | H |
| OBL-Z6 | The Orchard-tree hash is **Sinsemilla**: 10-bit altitude ‖ two 255-bit halves under `"z.cash:Orchard-MerkleCRH"`, depth 32, empty leaf **2** | `src/zk/vm.rs` (`MerkleRoot`) → `MerklePath`/`MerkleNode::combine`; `src/sdk/src/crypto/sinsemilla.rs` | **partly closed.** `HashOps.{merkleDepth, orchEmptyLeaf, sinsemillaCrh, computeMerkleRoot}` now carry the altitude in the CRH domain and use depth 32 / empty leaf 2, and `merkle_root_change_detection` is **proved** by induction rather than assumed. The remaining gap is the *primitive*: `sinsemillaCrh` substitutes the model's hash for Sinsemilla | H |
| OBL-Z7 | The SMT root is rate-2 Poseidon with **no** domain prefix, depth 255, empty leaf **0** — sharing neither primitive nor constants with the Orchard tree | `src/zk/gadget/smt.rs`; `src/sdk/src/crypto/smt/` | **closed in model.** `HashOps.{smtCrh, smtDepth, smtEmptyLeaf}` state the raw-pair Poseidon and the distinct constants, and `smtCrh_injective` is **proved** from `poseidon_collision_resistance` — no substitution needed, because the SMT really does use Poseidon | H |
| OBL-Z8 | ZK binaries are well-formed | `scripts/validate_zk_bins.sh` | the script — structural validity only, not that `.zk.bin` matches the current `.zk` source | H |

`set_membership` (0x59) is implemented and forces `expected_root` into the instance column, but
**no deployed `.zk` calls it** — only a comment in `oracle/proof/push_value_commitment.zk` warns
against it. `opcodes-status.md` lists it as "SOUND ✓" regardless.

### What the existing gates do and do not cover

Three scripts exist and are worth keeping as cheap structural checks. **None of them implements
OBL-Z1**, and the gap between them and it is where the Orchard bug lives:

* `check-circuit-metadata-alignment.sh` — **count parity**. It compares the number of
  `constrain_instance` calls against the number of values pushed for that circuit's namespace. Its
  own tail text says the real invariant is positional; it never checks position or names.
* `check-circuit-domain-separation.sh` — **prefix presence**. `witness_base` anywhere in the
  argument list satisfies it.
* `hooks/pre-commit` — **one binding pattern**, on staged files.

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
