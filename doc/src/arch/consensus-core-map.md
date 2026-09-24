# The Consensus State Core — a map, agreed before it is filled

**Status: a plan, with its first two units filled.** Nothing here is a claim about how the code behaves;
it is the agreed *shape* of the Lean models that the register's remaining consensus rows need, written
down once so each one can be built against something reviewable rather than invented per unit. Where
this document and a model disagree, the model is the artefact and this document is the stale one — so a
unit that departs from the shape below amends it in the same commit, and mechanisms 1 and 2 both did.

Measured 2026-09-24: every file below was opened, and Rust is cited by **function** rather than by line
— line numbers drift, as the register has already had to record more than once.

## Why this is a map, and not a list of theorems

The Lean layer's largest remaining gap is not a proof. It is a **subject**. The consensus state core —
the nullifier replay gate, the commitment set, the block-level mass-balance rule, the validity
predicates — is specified in Python under `contrib/model/`, enforced in Rust under `src/linear/`, and has
no Lean model at all. What exists is fragments, each deliberate and each stopping short:

* `Combinatorial/NullifierStorage.lean` mechanizes the *storage* half of one mechanism — a key→value
  store whose `markSpent` is faithful — and stops there.
* `Capability/NativeToken.lean` and `Capability/Exercise.lean` each model *one rule* about a coinbase:
  its maturity, and single-use consumption.
* `SupplyChain.lean` proves the cumulative-supply induction, for **any** schedule, which is why the
  register records the schedule's own non-increase as *checked over a range, not proved* (`OBL-C5`).
* `Semantics/Ledger.lean` proves that pairwise-disjoint calls commute, while saying nothing about what a
  call's write set *is* — and `OBL-C100` is the row that finding created.

So each mechanism below needs four things named before it can be built: the Rust that enforces it, the
Python that specifies it, the proposed Lean shape, and the **witness** that keeps the model from being
vacuous. The last is not decoration — this layer has deleted more than one statement that was true of
nothing, and its gate cannot see vacuity inside a `∀ … →`.

The repository's own rule governs readiness: **the Python model is the specification** (register,
`OBL-C29`). A mechanism whose Python does not exist is not ready to be modelled, and the table says which
those are.

## The mechanisms

| # | mechanism | Rust | Python spec (lines) | Lean today | register |
|---|---|---|---|---|---|
| 1 | block mass balance (Pedersen sum) | `src/linear/src/proof_of_token_balance.rs` (`verify_proof_of_token_balance`) | `contrib/model/proof_of_token_balance.py` (428) | `CrossCutting.value_conservation_no_wraparound`, and `Semantics/Ledger.lean`'s `exec_perm` | `OBL-C1` |
| 2 | nullifier replay gate, and maturity | `src/linear/src/chain_state.rs` (`connect_block`'s duplicate check; `check_coinbase_maturity`); `src/linear/src/lib.rs` (`COINBASE_MATURITY`) | `contrib/model/nullifier_lifecycle.py` (590) | the three fragments above | `OBL-C8`, `OBL-T4` |
| 3 | commitment set | `src/linear/src/chain_state.rs` (`commitment_set : Mutex<BTreeMap<Commitment, BlockHeight>>`) | `contrib/model/chain_model.py`, `contrib/model/fee_model.py` | `Consensus/CommitmentSet.lean` | `OBL-C109` |
| 4 | block and transaction validity | `src/linear/src/validation.rs` (`check_block_header`, `validate_block_structure`) | `contrib/model/chain_validation_model.py` (3871) | none | `OBL-C78`, `OBL-Z2` |
| 5 | cumulative supply chain | `src/linear/src/supply_chain.rs` (`compute_next`) | `contrib/model/supply_chain_model.py` (1622) | `SupplyChain.lean` | `OBL-C45`, `OBL-C5` |

## Two questions this map settles rather than assumes

**Does the commitment set get modelled, and in what shape?** Yes — and the first draft of this plan was
wrong about its Python, which it listed as "—". Two models address it. `contrib/model/chain_model.py`
carries `commitment_set: dict  # commitment → creation_height` and a `check_coinbase_maturity` that
*reads* it; `contrib/model/fee_model.py` carries `commitment_set: set[int]` and the rule "P8: output
commitment not already in commitment_set". So the Python already **couples** the set to maturity, which
the Lean does not.

The shape follows from the Rust rather than from a preference: the chain-level set is a **flat key set**
(`Mutex<BTreeMap<Commitment, BlockHeight>>`), not a Merkle tree and not an SMT. So the honest Lean model
is `NullifierStorage.lean`'s shape — a predicate over keys with a monotonicity law — and the new content
is the coupling: a creation height per commitment, and the maturity read that consults it. Modelling it
as a tree would be modelling something the chain does not have.

**Is `Capability/NativeToken.lean`'s `maturityGate` the maturity rule's model, or a neighbour of it?**
It **is** the model, and it stands alone. It names the same constant the Rust does
(`COINBASE_MATURITY := 100`, matching `src/linear/src/lib.rs`), and its two theorems are the rule's two
directions — `immature_coinbase_rejected` and `mature_coinbase_accepted`, both at budget 0. But its
`CoinbaseClaim.createdAt` is *supplied* rather than derived from the commitment set, and nothing connects
it to the nullifier lifecycle. So what is missing is a **connection**, not a component — the same verdict
mechanism 2 gets below. **And that verdict was acted on the same day**: `NullifierLifecycle.matureAt`
constructs the `CoinbaseClaim` the gate takes, from the height the claim store records, so `createdAt`
is now derived rather than supplied. The gate itself was not touched — which is what "a connection, not
a component" was supposed to mean, and it was right.

## The order, and why

1. **Mass balance** (`OBL-C1`) — **landed 2026-09-24** as `Consensus/MassBalance.lean`. It was the only
   mechanism with all four ingredients already (a Rust function, a Python spec, a Lean fragment, a
   register row), and it landed. **Three departures from this map's first draft, recorded because this
   document is meant to be the specification and the departures are the finding:**

   * the block is a **record of the specification's own categories** — `feeInputs`, `feeOutputs`,
     `burnInputs`, the transfer/spend lists, `mintOutputs`, `feeAmounts` — rather than "a list of
     per-call sums", because those are the parameters `verify_proof_of_token_balance` takes, and a model
     of a specification should have the specification's shape;
   * the predicate is a **pair of component equalities**, values *and* blinds, rather than "an equality
     over the field" — because the spec models commitments as `(value, blind)` integer pairs added
     componentwise, and says why (`pedersen_commit()` "breaks the simple additive property for test
     construction"). That choice is what makes **every theorem in the module budget 0**;
   * the second law is the **vacuity of the burn term**, not "no call the predicate admits changes the
     total": the spec's burn list sits on *both* sides of its equation, so `burn_cancels` is the fact
     that it constrains nothing. That was found by reading the spec, not by proving, which is why this
     map did not predict it.

   What landed as predicted: both sides of the predicate are inhabited, with the spec's own shapes — a
   legal transfer for the positive, and `test_illegal_hidden_mint`'s 100-in-against-1,000,000-out for the
   negative.

   **And this map's own sentence about `Ledger.lean` was wrong**, which is worth recording because the
   kind of claim it was is the kind a reader would act on. It said the mechanism "composes with
   `Semantics/Ledger.lean` rather than duplicating it — `exec_perm` is the order-independence half". It
   does not compose that way. The block level's order-independence is **unconditional** — sums commute,
   and `values_append` with `List.Perm.sum_eq` is all it takes — while `exec_perm`'s is **conditional on
   disjointness**, because it is about *stores*. Two different reasons for the same-sounding sentence, so
   the honest relation is a contrast and not a composition: a balance surviving a reordering is no
   evidence that the state transitions do. The model's module note says so.
2. **The nullifier lifecycle** (`OBL-C8`, `OBL-T4`) — **landed 2026-09-24** as
   `Consensus/NullifierLifecycle.lean`, seven theorems, all at budget 0. The connection this map
   predicted is the one that landed: a claim is recorded with its creation height, the existing
   `Capability.maturityGate` is **fed** by that height (closing the gap this map recorded — the gate's
   `createdAt` had been *supplied*), a spend is allowed only when the claim is mature and unspent, and a
   second spend is refused. The lifecycle witness is the four steps the map asked for: claimed at height
   0, refused at 50, allowed at 100, spent, refused again.

   **One correction, and it is about the word "store" in the row above.** This map said those pieces
   "are one mechanism" and implied one state. The *rules* are one lifecycle, but the *state* is **two
   maps** — a claim store (`nullifier → height`) and a spend set — and that is forced rather than
   chosen: `spend_preserves_claims` proves a spend leaves the height intact, which a single
   `nullifier → value` store could not, because the spend marker would overwrite the age the maturity
   rule reads. The Rust is the same shape for the same reason: two collections in memory
   (`nullifier_set` as a `BTreeMap<_, BlockHeight>`, `spent_nullifiers` as a `BTreeSet<_>`) unified on
   disk by **tagging the kind** — `src/linear/src/store.rs` records the value as
   `[kind] ++ height.to_le_bytes()`, kind `0` for a claim and `1` for a spend. So the tag is what buys
   back the distinction, and a model of this mechanism has to choose which of the Rust's two shapes it
   mirrors. This one mirrors the in-memory one.

   **Still not connected, deliberately**: `Capability/Exercise.lean`'s `validExercise` /
   `consume_is_single_use` is the *same rule* over a `List` on the contract side, and bridging a `List`
   to a predicate is a unit nothing consumes yet.
3. **The commitment set** — **landed 2026-09-24** as `Consensus/CommitmentSet.lean` (7 theorems, all at
   budget 0). **And this map's framing of it was wrong in a way worth recording**: it said "the new
   content is the coupling: a creation height per commitment, and the maturity read that consults it".
   Measured, that coupling is precisely what the *code* deliberately does not do and what the
   *specification* wrongly does. `check_coinbase_maturity` keys by **nullifier**, and its own docstring
   records the commitment-set route as considered and rejected — a second source of truth. Meanwhile
   `chain_model.py`'s `is_commitment_mature` keys by commitment *and* claims to mirror
   `chain_state.rs:is_commitment_mature()`, a function that does not exist. Both are now register row
   `OBL-C109`.

   So what the model is, is the **prune**. The set is written at the coinbase and fee-collect paths,
   pruned at the maturity window, restored from sled on restart, and read in production by **nobody** —
   its only accessor is called from `daemon_sync_integration.rs`'s reorg assertions.
   `prune_preserves_refusal` proves the prune neither creates nor destroys a refusal, so the conflation
   it introduces (a pruned commitment reads as `none`, exactly like one never recorded) costs the
   maturity rule nothing while costing the *set* its ability to answer at all. A model of a uniqueness
   rule — which is what "a set of commitments" suggests — would have been a model of nothing.

   Then **validity**: the largest surface, the least existing structure, and the last mechanism on the
   list.
4. **The supply chain** is already modelled. What remains is `OBL-C5`'s non-increase, which this
   campaign measured and left as a kernel-checked range plus a scan — see that row for why the obvious
   rescue lemmas are false.

## What this map does not do

* It does not model `blake3`, RandomX, sled, or the WASM runtime. `blake3` is native and SIMD, and
  RandomX is C++ over FFI.
* It does not claim the Rust is correct. Two of these mechanisms are ones where this campaign has already
  found the code disagreeing with its own safety argument — `OBL-C100` is the write-set one — and the
  register is where such findings live, not this map.
* It does not promise that every row closes. A row whose subject needs a *new* assumption stays open with
  its reason; that is what happened to `OBL-Z6` (the hash's codomain) and `OBL-C5` (the emission proof).
* It does not touch `src/zk/vm.rs`, and it does not propose consensus-path Rust changes.
