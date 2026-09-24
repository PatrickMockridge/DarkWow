/-
DarkFi.Capability.WritePath — wallet.md §6.1 (Exercise is a pure function), §6.3 (the construction
pipeline), §0.1.5 (the purity rules) and §7.8 (the write path's obligations).

**What this models.** §6.1 states the write path as one pure function:

    Transaction = f(SelectedCapabilities, Action, Params, Secrets, Seed)

and §7.8 lists the obligations that function carries: `construct_sound`, `construct_deterministic`,
`nullifier_completeness`. Two of those were **absent from the whole proof tree** until this module, and
`wallet.md` §7.8 named `Capability/Wallet.lean` as their home — the wrong home in both directions: that
file models *type construction* (`walletConstruct`, §7.1/§7.4), not the write path, and the one
obligation that was discharged lives in `Capability/Exercise.lean`, on `PublicState` rather than on a
transaction.

`construct` takes §6.1's tuple as its arguments, in §6.1's order. One component is deliberately absent:
`Secrets`. The reason is §6.4.0's own rule rather than convenience — the secret a nullifier is computed
from is the capability's **`spend_secret`**, which §6.4.0 says the wallet persists *with* the capability
and which `key_coords` (the account-derived names a `Secrets` argument would model) SHALL NOT be used to
recover. So the secret lives on `SelectedCapability`, where the spec puts it, and a `Secrets` argument
here would model exactly what §6.4.0 forbids.

**Why `params` is an argument no definition reads.** §7.8's `construct_deterministic` is stated over the
tuple *including* `Params`, so the argument has to exist for the statement to be §7.8's — and its content
is the opposite of reading it: `params_are_not_read` below is §0.1.5's purity rule in checkable form.
§6.1's own emphasis is the same shape ("the capability being exercised is NEVER in the params").

**What is not modelled, named rather than papered over.** The ZK premise: §7.8's `construct_sound` says
the transaction's *proofs* inhabit L_{r,s}, and what is modelled here is the half a `CapabilityType`
carries — barb coverage — by way of `walletConstruct_sound`, so the write path's soundness is *derived
from* the read path's constructor rather than restated beside it. (Both live at the root namespace, in
`Capability/Wallet.lean` — that file declares no namespace, which is why the names are unqualified.) The
proofs themselves, the `calls` structure of §6.3 step 7, the Merkle inclusion proofs §6.1 lists for each
selected capability, and the fee capability of §6.3 step 6 are all absent: none of §7.8's three
obligations reads them, and a field nothing reads is the defect `Exercise.outputs` was — a header
claiming a create side while `applyExercise` ignored the field.

**The hash.** `computeNullifier` is the SDK's `compute_nullifier(secret, commitment)`
(`src/sdk/src/primitives.rs:299`, `poseidon_hash([secret, commitment])`), modelled as a total `Nat`
function. Every statement below is about *which pair* it is applied to and whether the result is
published, never about the hash's algebra, so nothing here needs a collision-resistance budget and none
is spent.
-/

import Mathlib
import DarkFi.AxiomBudget
import DarkFi.Capability.Types
import DarkFi.Capability.Composition
import DarkFi.Capability.Selection
import DarkFi.Capability.Wallet
import DarkFi.Capability.Prover

namespace DarkFi.Capability.WritePath

open DarkFi.Capability.Types
open DarkFi.Capability.Composition
open DarkFi.Capability.Selection

/- ==========================================================================
   §6.1's inputs
   ========================================================================== -/

/-- §6.1's `Seed`: "the explicit randomness name". A name passed *in*, never ambient authority — which
    is why it is an argument rather than something a definition could reach for. -/
structure Seed where
  value : Nat
  deriving DecidableEq, Repr

/-- §6.1's SelectedCapabilities, one capability: the §6.2 `Held` the selection chose (the barbs it
    composes and its spend state), the commitment that identifies it as an instance, and — per §6.4.0 —
    the fresh `spend_secret` the wallet persisted with it.

    §6.1 also lists a Merkle inclusion proof for each. It is not a field: neither
    `nullifier_completeness` nor `construct_sound` reads it, and "recognized under a root" is
    `Capability/PerContractTree.lean` and `HashOps.lean`'s subject — a field here that no theorem read
    would be the `Exercise.outputs` defect at one remove. -/
structure SelectedCapability where
  held : Held
  commitment : Nat
  spendSecret : Nat

/- ==========================================================================
   §6.3 steps 4, 6 and 7 — the transaction
   ========================================================================== -/

/-- §6.3 step 7's transaction, as far as §7.8 reads it: the published nullifiers, and the two names of
    the transaction binding (T6: `poseidon(3, tx_commitment, tx_nonce)`, `Prover.lean`). `calls` and
    `proofs` are not modelled; see the module note. -/
structure Transaction where
  nullifiers : List Nat
  txCommitment : Nat
  txNonce : Nat
  deriving DecidableEq, Repr

/-- The SDK's `compute_nullifier(secret, commitment)` (`src/sdk/src/primitives.rs:299`). Uninterpreted
    in its algebra on purpose: no statement here asks it to be injective, which is why it costs no
    budget. -/
def computeNullifier (secret commitment : Nat) : Nat := secret + commitment

/-- §6.3 step 4's nullifier for one consumed capability, from that capability's own spend secret
    (§6.4.0) and commitment.

    It is deliberately **not** a function of the `Seed`, and that is a property rather than an omission:
    a nullifier that moved with the randomness would defeat the purpose §6.3 step 4 states it for, since
    the mempool recognizes a double spend by seeing the same nullifier twice (`mempool.md`). -/
def nullifierOf (s : SelectedCapability) : Nat := computeNullifier s.spendSecret s.commitment

/-- The transaction binding's commitment name, derived from the `Seed` — §0.1.5 rule 1 ("No function
    below the shell SHALL draw from ambient randomness"), which is also why it is an argument here. The
    deployment computes it out of the seed; the model keeps the derivation a function of `Seed` alone
    and claims nothing about its shape beyond the one invariant #4 needs (`binding_is_real` below). -/
def bindingCommitment (seed : Seed) : Nat := seed.value

/-- The second name of the binding. Deliberately not also `seed.value`: `bindsRealTxBinding` is a
    disjunction, so a pair that could be `(0, 0)` would make invariant #4 unstatable — which is exactly
    the shape the SDK's defaults have (see `zero_binding_is_not_real`). -/
def bindingNonce (seed : Seed) : Nat := seed.value + 1

/-- §6.3 step 4's published set for a selection, named separately because two theorems are about this
    set alone (`nullifier_completeness`) and one about the transaction it is carried in. -/
def publishedNullifiers (selected : List SelectedCapability) : List Nat := selected.map nullifierOf

/-- §6.3 steps 6 and 7 for a given selection and seed: the transaction `f` returns. Split out from
    `construct` because four theorems below are about the *assembly* (what the transaction contains),
    while `construct` is about the *gate* (whether a transaction is returned at all). -/
def assemble (selected : List SelectedCapability) (seed : Seed) : Transaction :=
  { nullifiers := publishedNullifiers selected
  , txCommitment := bindingCommitment seed
  , txNonce := bindingNonce seed }

/-- §6.1's `f`. The manifest (step 1) and the selection (step 2) are the caller's, as the spec has them
    — selection is §6.2's predicate — and the gate here is the read path's own constructor:
    `walletConstruct` returns `some` exactly when the selection's composed barbs cover the action's
    required barbs, so the write path *uses* the read path's construction rather than a second copy of
    the same test. `none` is §6.3 step 4's "construction error", reached whenever the selection does not
    cover.

    `_params` is unused by design and named with the underscore for it: see `params_are_not_read`, which
    is §0.1.5's rule about it, and the module note on why the argument exists at all. -/
def construct (selected : List SelectedCapability) (resource : Resource) (action : Action)
    (_params : List Nat) (seed : Seed) : Option Transaction :=
  match walletConstruct (selected.bind (fun s => s.held.primitives)) resource action with
  | some _ => some (assemble selected seed)
  | none => none

/- ==========================================================================
   §7.8's obligations, by the names §7.8 gives them
   ========================================================================== -/

/-- **§7.8's `construct_sound`.** If `f` returns a transaction then the selection covers the action's
    required barbs — §7.8's "the proofs it carries inhabit L_{r,s}", in the half a `CapabilityType`
    carries — and the transaction is the assembly of that selection and seed.

    The coverage is obtained **from `walletConstruct_sound`**, not from `construct`'s own branch: that is
    the difference between this theorem and `walletConstruct_sound` itself, which `Wallet.lean` records
    as returning the branch's own hypothesis. Here the write path's soundness is a *consequence* of the
    read path's constructor, so a change to `walletConstruct` breaks this rather than silently leaving
    two definitions to agree.

    What it does not give: L_{r,s}'s ZK premise. `CapabilityType`'s field is barb coverage, and the
    deployment's proving additionally requires a proof inhabiting L_{r,s}; that gap is named where this
    tree names it (`Axioms.lean`'s ZK boundary) and is not closed here. -/
@[axiom_budget 0]
theorem construct_sound (selected : List SelectedCapability) (resource : Resource) (action : Action)
    (params : List Nat) (seed : Seed) (tx : Transaction)
    (h : construct selected resource action params seed = some tx) :
    tx = assemble selected seed ∧
      ∃ ct : CapabilityType resource action,
        ct.primitives = selected.bind (fun s => s.held.primitives) := by
  unfold construct at h
  cases hw : walletConstruct (selected.bind (fun s => s.held.primitives)) resource action with
  | none => simp [hw] at h
  | some ct =>
      have hcov : resource.requiredBarbs ⊆ compose (selected.bind (fun s => s.held.primitives)) :=
        walletConstruct_sound (selected.bind (fun s => s.held.primitives)) resource action ct hw
      rw [hw] at h
      have htx : assemble selected seed = tx := Option.some.inj h
      exact ⟨htx.symm,
        ⟨{ primitives := selected.bind (fun s => s.held.primitives), coversBarbs := hcov }, rfl⟩⟩

/-- **§7.8's `construct_deterministic`**, with the content §7.4's trivial form lacks. Two calls that
    agree on the *selection* and the *`Seed`* agree on the transaction regardless of the resource,
    action and params they were given — so `f` is a function of those two, not of its tuple's shape, and
    the randomness §0.1.5 rule 1 talks about reaches the transaction only through `Seed`. Stated over two
    calls rather than as `f x = f x`; `params_are_not_read` below is the same fact one component at a
    time. -/
@[axiom_budget 0]
theorem construct_deterministic (selected : List SelectedCapability) (seed : Seed)
    (resource₁ resource₂ : Resource) (action₁ action₂ : Action) (params₁ params₂ : List Nat)
    (t₁ t₂ : Transaction)
    (h₁ : construct selected resource₁ action₁ params₁ seed = some t₁)
    (h₂ : construct selected resource₂ action₂ params₂ seed = some t₂) :
    t₁ = t₂ := by
  have a₁ : t₁ = assemble selected seed := (construct_sound selected resource₁ action₁ params₁ seed
    t₁ h₁).1
  have a₂ : t₂ = assemble selected seed := (construct_sound selected resource₂ action₂ params₂ seed
    t₂ h₂).1
  rw [a₁, a₂]

/-- **§7.8's `nullifier_completeness`, on the transaction** — the form §6.3 step 4 needs and
    `Exercise.lean`'s is not: every consumed capability's nullifier is published in `Transaction.nullifiers`,
    which is what the mempool reads to detect a double spend. -/
@[axiom_budget 0]
theorem nullifier_completeness (selected : List SelectedCapability) (resource : Resource)
    (action : Action) (params : List Nat) (seed : Seed) (tx : Transaction)
    (h : construct selected resource action params seed = some tx)
    (s : SelectedCapability) (hs : s ∈ selected) :
    nullifierOf s ∈ tx.nullifiers := by
  have htx : tx = assemble selected seed := (construct_sound selected resource action params seed
    tx h).1
  rw [htx]
  simp only [assemble, publishedNullifiers, List.mem_map]
  exact ⟨s, hs, rfl⟩

/- ==========================================================================
   §0.1.5's purity rules, in the two forms the model can carry
   ========================================================================== -/

/-- **§0.1.5 rule 1 in checkable form.** Nothing but the selection and the `Seed` reaches the
    transaction: changing the params cannot change the result, which is also §6.1's "the capability
    being exercised is NEVER in the params" read as a statement about `f`. The two sides are the same
    term because `construct`'s body does not mention the argument. -/
@[axiom_budget 0]
theorem params_are_not_read (selected : List SelectedCapability) (resource : Resource)
    (action : Action) (params₁ params₂ : List Nat) (seed : Seed) :
    construct selected resource action params₁ seed
      = construct selected resource action params₂ seed :=
  rfl

/-- **And the `Seed` is load-bearing**, so "derive the randomness from `Seed`" is not satisfied by a
    constant: two seeds whose derived binding nonce differs. Without this, `construct_deterministic`
    would be equally true of an `f` that ignored its seed. -/
@[axiom_budget 0]
theorem seed_changes_the_binding :
    ∃ s₁ s₂ : Seed, bindingNonce s₁ ≠ bindingNonce s₂ :=
  ⟨{ value := 0 }, { value := 1 }, by simp [bindingNonce]⟩

/- ==========================================================================
   `bindsRealTxBinding` (invariant #4) gets its consumers — F9 of the wallet plan
   ==========================================================================
   `Prover.bindsRealTxBinding` stood as a bare `def` with **no reference outside its own line**
   (measured: `grep` over `.lean`/`.rs`/`.md`/`.py`), which is the species the register records for
   `capTypesDistinct` (`OBL-T2`). wallet.md §6.4's invariant 4 — "the proof binds the real
   `tx_commitment`/`tx_nonce`, never zero" — was therefore stated and connected to nothing. Both
   directions are given here: a seed-derived binding satisfies the predicate, and the shape the SDK's
   defaults produce violates it.
   ========================================================================== -/

/-- **Invariant #4, positively.** The binding this write path derives satisfies `bindsRealTxBinding`: at
    least one of the two names is non-zero, for every seed. -/
@[axiom_budget 0]
theorem binding_is_real (seed : Seed) :
    bindsRealTxBinding (bindingCommitment seed) (bindingNonce seed) :=
  Or.inr (Nat.succ_ne_zero seed.value)

/-- **And the violation, in the shape the code has it.** `CapabilityProvider::tx_commitment`/`tx_nonce`
    default to `pallas::Base::zero()` (`src/sdk/src/prover.rs:354-360`, "zero if unbound") and
    `bind_slot` binds whatever they return with no failure path, so the predicate above is false of the
    pair those defaults produce — which is what makes it a predicate rather than a tautology. The
    wallet's own provider overrides both with real fields, so no live defect is claimed here; the claim
    is that the trait's default is the shape invariant #4 forbids, and that this predicate is the
    statement it violates. -/
@[axiom_budget 0]
theorem zero_binding_is_not_real : ¬ bindsRealTxBinding 0 0 := by
  simp [bindsRealTxBinding]

/- ==========================================================================
   Non-vacuity — both branches of the gate, and the step-4 error
   ========================================================================== -/

/-- The assembly with §6.3 step 4 skipped: the nullifier set left empty. Not a model of anything the
    spec permits — it exists so `nullifier_completeness` has a refuting instance rather than being true
    of every `Transaction` anyone could write. -/
def omitNullifiers (selected : List SelectedCapability) (seed : Seed) : Transaction :=
  { assemble selected seed with nullifiers := [] }

/-- A capability whose commitment and spend secret are fixed, available, and composing nothing — the
    data the witnesses below share. Named rather than repeated so the three of them are plainly about
    the same capability. -/
def witnessCapability : SelectedCapability :=
  { held := { primitives := [], status := none }, commitment := 1, spendSecret := 2 }

/-- **§6.3 step 4's error, as a witness.** A selection whose nullifiers were not published refutes
    `nullifier_completeness` — so that obligation is not vacuously true of the empty list. -/
@[axiom_budget 0]
theorem omitted_nullifier_set_is_a_construction_error :
    ∃ (selected : List SelectedCapability) (s : SelectedCapability),
      s ∈ selected ∧ nullifierOf s ∉ (omitNullifiers selected { value := 0 }).nullifiers :=
  ⟨[witnessCapability], witnessCapability, by simp, by simp [omitNullifiers]⟩

/-- **And the gate can refuse.** A selection holding only `assetId` does not cover
    `nativeTokenResource` — `↓denominate` is not `↓spend`/`↓nullify`/`↓commit`/`↓dispatch`/`↓gate` — so
    `f` returns `none`. This is the branch the other theorems are conditional on, and a gate that could
    never refuse would make `construct_sound`'s hypothesis unreachable rather than meaningful. -/
@[axiom_budget 0]
theorem construct_rejects_a_non_covering_selection :
    construct [{ held := { primitives := [assetId], status := none }, commitment := 1, spendSecret := 2 }]
      nativeTokenResource transferAction [] { value := 0 } = none := by
  decide

end DarkFi.Capability.WritePath
