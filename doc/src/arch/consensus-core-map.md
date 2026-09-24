# The Consensus State Core — a map, agreed before it is filled

**Status: a plan, with six of its six mechanisms filled, and every one of them amended by the unit that
filled it.** Nothing
here is a claim about how the code behaves;
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
| 4 | block and transaction validity | `src/linear/src/validation.rs` (`check_block_header`, `validate_block_structure`, `check_block_timestamp`) | `contrib/model/chain_validation_model.py` (3871) | `Consensus/BlockTimestamp.lean` (the timestamp rule only) | `OBL-C110` |
| 5 | cumulative supply chain | `src/linear/src/supply_chain.rs` (`compute_next`) | `contrib/model/supply_chain_model.py` (1622) | `SupplyChain.lean` (the schedule half) + `Consensus/SupplyReconciliation.lean` (the two trees, reconciled) | `OBL-C45`, `OBL-C5` |
| 6 | the coinbase split | `bin/dwowd/src/block_acceptor.rs` (the four value checks); `src/linear/src/validation.rs` (the coinbase's own consistency); `src/sdk/src/blockchain.rs` (`split_for_uncle`) | `contrib/model/chain_model.py` (`connect_block`, the split *and* the note-level rule) | `Consensus/CoinbaseSplit.lean` | `OBL-C4`, `OBL-C7`, `OBL-C85` |

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

   Then **validity** — **partly landed 2026-09-24** as `Consensus/BlockTimestamp.lean`, and it is the
   *smallest* thing on the list rather than the largest, which is a correction to this map in two ways.

   **The map's register citation for this row was wrong.** It named `OBL-C78` and `OBL-Z2`, which are
   both about a contract's `get_metadata` publishing a public-input vector that agrees with its circuit —
   ZK metadata rows, not validity rows. A reader following the map would have found two rows about
   something else. The correct row is `OBL-C110`, minted by the unit.

   **And what landed is one rule, not the surface.** `BlockTimestamp.lean` models
   `check_block_timestamp` — the median-of-11 rule — because it is the piece of validity that is a
   *rule* rather than plumbing, and because the unit's real content was three divergences in the
   specification of it (now `OBL-C110`), which needed the rule stated precisely to be about. The model
   is four interface laws; the security claim the rule exists for — that an adversary controlling at most
   `len / 2` of the window cannot lower the floor — is *stated in the module note and not proved*, and
   nothing here checks the rule on a concrete window, because neither of Mathlib's sorts reduces in the
   kernel. Both absences are recorded there rather than papered over.

   Then the **uncle rules** — the other end of the same reward path — **landed 2026-09-24** as
   `Consensus/UncleRules.lean` (15 theorems, 11 at budget 0), and this is the second unit to *add* to the
   map rather than fill it: `check_uncles` was named in this row's "what remains" list, and it turned out
   to be a mechanism rather than a remainder — a depth window with both ends bounded, an alignment guard
   over two parallel slices, and a pin **re-derived** because the merkle root commits only to the uncle's
   header. So the row's list was two-thirds right and the third it named was a design pass of its own.

   **What that unit found is a boundary statement about the map's own ordering.** The two modules that
   model the block's reward — this one and `CoinbaseSplit.lean` — are the two halves of one check, and
   neither bounds the sum alone: `three_uncles_at_one_depth_satisfy_every_rule` exhibits three uncles
   that satisfy the count bound and the depth window, so the uncle rules are necessary and not
   sufficient, and only `effective + Σ pin = base` bounds the payout. The map listed the split and the
   uncle rules as separate mechanisms (6 and a part of 4), and the honest relation between them is
   *composition* — the same relation this map got wrong for `Ledger.lean` and had to correct.

   Then the **header rules** — **landed 2026-09-24** as `Consensus/BlockHeader.lean` (10 theorems, all at
   budget 0), and it produced the map's *third* correction of the same shape. The validity row's "what
   remains" list is now down to header continuity, the fee-collect decision table and
   `validate_block_structure`'s remaining structural conditions — but the two rules that landed were not
   what the list implied. The **two-stage proof-of-work** turned out to be a *redundancy*: stage 1 is an
   up-set in the target the header declares, so it is vacuous in isolation, and stage 1-for-some-target
   plus stage 2 is exactly the consensus comparison — an existential, and the same statement with the
   declared target free is **false**. And the ordering comments in `check_block_header`, which read as
   rules ("fork detection MUST come before Stage 2 target"), measure as **diagnostics**: acceptance is
   invariant under any permutation of the checks, because the module is pure and the checks are
   conjunctive. So this row's framing of validity as a surface of *rules* needs one qualifier: it is a
   surface of rules and diagnostics, and telling them apart is a part of the work rather than a
   preliminary to it. `OBL-C115` and `OBL-C116` are the two rows minted for the pair, because neither
   proposition had a row.

   Then the **fee-collect rule** — **landed 2026-09-24** as `Consensus/FeeCollect.lean` (5 theorems,
   `OBL-C117` minted for it), and it is the fourth unit running to correct this row's framing. The code's
   four-arm `match` over a three-component tuple is a **presentation** of a two-clause rule: the table and
   the clauses are equivalent (`codeVerdict_is_the_rule`), the arms' order matters only for which message
   a refusal carries, and the component that reads as ignored is inert *because* the guard above has
   already excluded every value but 0 and 1 — a fact with a refutation behind it
   (`call_count_guard_is_load_bearing`). So this row's "what remains" list has now been wrong in the same
   way three times: it names leftovers, and each one turns out to be a mechanism with a structure worth
   stating. The list is down to header continuity and `validate_block_structure`'s remaining structural
   conditions.

   Then the **coinbase's structural rules** — **landed 2026-09-24** as `Consensus/CoinbaseStructure.lean`
   (13 theorems, all at budget 0, `OBL-C118` minted for it), and this closes the row. What it found is the
   strongest result of the four validity units: the four opening checks of `validate_block_structure`
   collapse to a *shape* — the block **is** `[powReward] :: rest` — and the payoff is that the
   mass-balance rule's blind spot has size exactly one, and it is the pow reward's own call. The code's
   comment states the reason and reads its own dependency backwards ("structural fix makes call-level skip
   fixes defense-in-depth"); the refutation shows the structural rule is what gives the whole-transaction
   skips their soundness, not a duplicate of them.

   **So `validation.rs` is modelled, with one deliberate omission**: header continuity — `height ==
   current + 1` and `previous == prev` — is two equalities with nothing to state, and the map records the
   omission with its reason rather than leaving a gap that reads as an oversight. That is the distinction
   this map has been making all along between what is *unmodelled* and what is *not worth modelling*, and
   it is the first time the second category has been reached on purpose.

   This map's guess that validity was "the largest surface with the least existing structure" is the part
   of this row that still stands, and the qualifier is the part that does not.
4. **The coinbase split** — **landed 2026-09-24** as `Consensus/CoinbaseSplit.lean` (16 theorems, 14 at
   budget 0), and it is **a mechanism this map did not have**. It was not in the five when the map was
   agreed, and it was found by taking the validity row's own advice: the map said the rest of validity
   "is a design pass of its own", the pass was started in `validation.rs`, and the surface it opened was
   the block's *reward* rather than its header. So the map was incomplete in the direction it predicted
   it might be, and this is the first unit that *added* a mechanism rather than filling one.

   **What it is**: five quantities — the header's reward, the coinbase's declared pin, the note's
   effective value, the coinbase call's input value, and the uncle-mint notes' sum — constrained by five
   equations spread across **three** enforcement sites (the host's accept path, `validation.rs`, and the
   WASM `pow_reward_v1` guard). The model is the conjunction, which is not a function in the tree, and
   that is the whole reason it is worth having: no single file can ask which conjunct is load-bearing.

   **And one of the two preceding units' findings is reversed here.** `OBL-C109` and `OBL-C110` were each
   about a specification that misdescribed the code. Here the specification had the *load-bearing
   analysis* — `chain_model.py` says in prose that "the value-level `verify_uncle_split` is NOT
   sufficient: it checks the header and the declared pins, not the SPENDABLE NOTE" — and this unit's
   headline theorem, `note_sum_is_load_bearing`, is that sentence mechanized as a refutation with its
   witness. So the map's own framing of the specification as unreliable is itself incomplete: the
   specification here was *right*, and earlier than the model.

   **Two corrections to this map's neighbouring row.** The validity row says the rest of `validation.rs`
   is "the largest surface with the least existing structure", and that clause still stands for the
   header rules. But the coinbase's own consistency check in `validation.rs` turned out to belong to a
   mechanism with working structure elsewhere — so "the rest of validity" is not one surface but at least
   two, and the second is the reward path.
5. **The supply chain** is already modelled. What remains is `OBL-C5`'s non-increase, which this
   campaign measured and left as a kernel-checked range plus a scan — see that row for why the obvious
   rescue lemmas are false.

   **Amended 2026-09-24: this row named one remaining piece where there were two, and the second is a
   composition rather than a schedule property.** `OBL-C45`'s first clause is that the *contracts* tree
   and the *supply-chain* tree reconcile, and that clause was not on this row's list at all — because
   the row reads `SupplyChain.lean` as the mechanism's whole and it is the mechanism's *schedule* half.
   `Consensus/SupplyReconciliation.lean` now states the other half: a block's net creation of commitment
   value is the schedule's value at that height, and summed over a chain it is `Σ expected_reward(H)`.
   The row's three fields are therefore the composition's three legs — `MassBalance.lean`'s `Balanced`,
   `CoinbaseSplit.lean`'s `accepts`, `FeeCollect.lean`'s `rule` — and the map's habit of listing
   mechanisms separately is what hid it: **this is the fourth time a mechanism this map lists has turned
   out to *compose* with a neighbour rather than stand beside it** (Steps 8, 12, 13 were the first
   three), and the first where the composition was the *missing* half rather than a correction to a
   stated one. What the new module does **not** do is the reconciliation itself: nothing in Rust and
   nothing in a script reads the two trees against each other, so `OBL-C45` stays `PARTLY`.

## What this map does not do

* It does not model `blake3`, RandomX, sled, or the WASM runtime. `blake3` is native and SIMD, and
  RandomX is C++ over FFI.
* It does not claim the Rust is correct. Two of these mechanisms are ones where this campaign has already
  found the code disagreeing with its own safety argument — `OBL-C100` is the write-set one — and the
  register is where such findings live, not this map.
* It does not promise that every row closes. A row whose subject needs a *new* assumption stays open with
  its reason; that is what happened to `OBL-Z6` (the hash's codomain) and `OBL-C5` (the emission proof).
* It does not touch `src/zk/vm.rs`, and it does not propose consensus-path Rust changes.
