# The Consensus State Core — a map, agreed before it is filled

**Status: a plan, not a specification.** Nothing here is a claim about how the code behaves; it is the
agreed *shape* of the Lean models that the register's remaining consensus rows need, written down once
so each one can be built against something reviewable rather than invented per unit. Where this document
and a model disagree, the model is the artefact and this document is the stale one.

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
| 3 | commitment set | `src/linear/src/chain_state.rs` (`commitment_set : Mutex<BTreeMap<Commitment, BlockHeight>>`) | `contrib/model/chain_model.py`, `contrib/model/fee_model.py` | none | — |
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
mechanism 2 gets below.

## The order, and why

1. **Mass balance** (`OBL-C1`). It is the only mechanism with all four ingredients already: a Rust
   function, a Python spec, a Lean fragment (`CrossCutting`), and a register row. And it composes with
   `Semantics/Ledger.lean` rather than duplicating it — `exec_perm` is the order-independence half, and
   what is missing is what a block's *contents* must satisfy. Proposed shape: a block as a list of
   per-call Pedersen sums, the balance predicate as an equality over the field, and two laws — that the
   predicate survives reordering a block's calls when their write sets are disjoint, and that no call the
   predicate admits changes the total. Non-vacuity: a two-call block that balances, and a one-unit
   inflation that does not — the shapes the Rust's own negative controls use
   (`test_one_unit_inflation_rejected`).
2. **The nullifier lifecycle** (`OBL-C8`, `OBL-T4`). The most tractable, because only the connection is
   missing: the store, the single-use rule, the maturity gate and the consensus replay gate are one
   mechanism across four files today. Proposed shape: a single model in which they are the same
   mechanism, with the replay gate's *refusal* of a double spend proved rather than assumed.
   Non-vacuity: a coinbase claimed, matured, spent, and re-spent — the fourth step must fail.
3. **The commitment set**, then **validity**. After the first two, because the coupling with maturity is
   what makes the set interesting, and validity's pure functions are the largest surface with the least
   existing structure to build on.
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
