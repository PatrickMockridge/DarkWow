/-
DarkWow Capability Type System — Pareto-Efficiency Proof

Proves that the primitive type namespace (type-system.md §8.1) is
pareto-efficient: no type distinction can be removed without losing
behavioral information, and no unnecessary distinction exists.

The proof is exhaustive: all 12 primitive types have pairwise distinct
barb sets. Since Finset Barb has DecidableEq and the type list is finite,
this is decidable by computation.
-/

import DarkFi.Capability.Types
import DarkFi.Capability.Composition
import DarkFi.Axioms
import DarkFi.AxiomBudget

-- `allPairsDistinctProp` quantifies over the 17×17 `List.product`, and instantiating
-- `pairsDistinctCheck_eq_true` at a pair requires unfolding that expression. That exceeds the
-- default elaborator recursion depth. Raised for this file only; `decide` still produces a
-- kernel-checked proof.
set_option maxRecDepth 10000

open DarkFi.Capability.Types
open DarkFi.Capability.Composition

/- ==========================================================================
   Part 1: Exhaustive Pairwise Distinction

   The proposition below says: for every pair of primitives in
   `allPrimitiveTypes`, distinct names imply distinct barb sets. This is what
   "pareto-efficiency" means for the primitive namespace — no barb distinction
   is redundant.

   ## This was false until `↓shard` was added, and here is what it looked like

   `ContractId` and `ExternalChain` both had the barb set `{↓dispatch}`:

       def contractId    : PrimitiveType := { barbs := {Barb.dispatch}, … }
       def externalChain : PrimitiveType := { barbs := {Barb.dispatch}, … }

   Under the type system's own criterion — `typesDistinct t1 t2 := t1.barbs ≠
   t2.barbs`, §2 of `type-system.md` — those two "distinct" primitive types were
   the *same* behavioural type. An external-chain routing discriminant and a
   contract-dispatch identifier were indistinguishable to any observer of barbs.
   `doc/src/arch/type-system.md` §11.1 listed this theorem as PROVED throughout.

   The collision was invisible because the file did not parse: the binders were
   written `∀ (t1 ∈ …) (t2 ∈ …)`, which is not Lean 4 syntax, so the statement
   never elaborated and the `native_decide` under it never ran. `decide` on the
   repaired statement reported the proposition was *false*, which is how it
   surfaced.

   It is fixed by giving `ExternalChain` the `↓shard` barb, which is what an
   external chain actually is in this system: §9.5 writes the shards as
   ρ-processes (`S_1!(state_root_1, …) | S_2!(…) | CrossShardProof?(import_A_B)`)
   with the canonical chain as settlement layer and the topology emergent, so an
   external chain is a shard whose state lives off-network. `↓denominate` names
   the asset, `↓shard` names the name-space.

   Note what this makes visible rather than merely fixed: a bridge is *not* a
   primitive with a convergence barb. Convergence is what a composition exhibits
   — `externalChain ⊗ chainDepositProof ⊗ bridgeAddress ⊗ dleqProof` — which is
   the ρ-calculus reading of "bridges as namespace convergences". The barb set is
   where that shows up, and it only shows up if the shard barb exists.
-/

def allPairsDistinctProp : Prop :=
  ∀ p ∈ allPrimitiveTypes ×ˢ allPrimitiveTypes,
    p.1.name ≠ p.2.name → p.1.barbs ≠ p.2.barbs

/-- The same condition as a computation: `false` on exactly the pairs
    `allPairsDistinctProp` forbids. -/
def pairsDistinctCheck : Bool :=
  (allPrimitiveTypes ×ˢ allPrimitiveTypes).all fun p =>
    decide (p.1.name = p.2.name) || !decide (p.1.barbs = p.2.barbs)

/-- The check is `true`, by the kernel. Fifteen `native_decide` calls used to sit in this file;
    they are `decide` now, and this is where the claimed fact is actually established. -/
@[axiom_budget 1]
theorem pairsDistinctCheck_eq_true : pairsDistinctCheck = true := by decide

/-- The two that collided are now distinct. Stated explicitly because it is the pair that was
    wrong, and a reader who greps for `ExternalChain` should find the reason it has its own barb. -/
@[axiom_budget 1]
theorem contractId_externalChain_distinct : typesDistinct contractId externalChain := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem primitiveTypesAreParetoEfficient : allPairsDistinctProp := by
  intro p hp h_name
  have hall : (decide (p.1.name = p.2.name) || !decide (p.1.barbs = p.2.barbs)) = true :=
    List.all_eq_true.mp pairsDistinctCheck_eq_true p hp
  rw [decide_eq_false h_name, Bool.false_or] at hall
  intro h_barbs
  rw [decide_eq_true h_barbs, Bool.not_true] at hall
  exact Bool.noConfusion hall

/- ==========================================================================
   Part 2: Named Theorems for Individual Pairs (for spec cross-reference)
   ==========================================================================
   Each theorem below is a special case of the general pareto-efficiency
   proof. They exist so specification documents can reference them by name.
-/

@[axiom_budget 1]
theorem secretKey_distinct_from_publicKey : typesDistinct secretKey publicKey := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem secretKey_distinct_from_nullifier : typesDistinct secretKey nullifier := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem secretKey_distinct_from_commitment : typesDistinct secretKey commitment := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem nullifier_distinct_from_commitment : typesDistinct nullifier commitment := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem nullifier_distinct_from_contractId : typesDistinct nullifier contractId := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem commitment_distinct_from_contractId : typesDistinct commitment contractId := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem contractId_distinct_from_assetId : typesDistinct contractId assetId := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem assetId_distinct_from_funcId : typesDistinct assetId funcId := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem funcId_distinct_from_merkleNode : typesDistinct funcId merkleNode := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem secretKey_distinct_from_ownedSecretKey : typesDistinct secretKey ownedSecretKey := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem ownedSecretKey_distinct_from_miningRecipient : typesDistinct ownedSecretKey miningRecipient := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem nullifier_distinct_from_intentNullifier : typesDistinct nullifier intentNullifier := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem nullifier_distinct_from_bridgeCapNullifier : typesDistinct nullifier bridgeCapNullifier := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem intentNullifier_distinct_from_bridgeCapNullifier : typesDistinct intentNullifier bridgeCapNullifier := by
  unfold typesDistinct; decide

@[axiom_budget 1]
theorem miningRecipient_distinct_from_secretKey : typesDistinct miningRecipient secretKey := by
  unfold typesDistinct; decide

/- ==========================================================================
   Part 3: No Accidental Unification

   If two types in the list have identical barb sets, they must be the same type.
   This is the converse of pareto-efficiency, and it was false alongside it — the
   `ContractId`/`ExternalChain` pair was the counterexample in both directions.
   With `↓shard` on `ExternalChain` it holds.
-/

@[axiom_budget 1]
theorem barbEqualityImpliesTypeEquality (t1 t2 : PrimitiveType)
    (h1 : t1 ∈ allPrimitiveTypes) (h2 : t2 ∈ allPrimitiveTypes)
    (h_barbs : t1.barbs = t2.barbs) : t1.name = t2.name := by
  by_contra h_names
  exact primitiveTypesAreParetoEfficient (t1, t2)
    (by simpa using ⟨h1, h2⟩) h_names h_barbs

/- ==========================================================================
   Part 4: Compositional Pareto-Efficiency — REMOVED

   A theorem used to sit here:

       theorem compositionalDistinction (prims1 prims2 : List PrimitiveType)
           (h : compose prims1 ≠ compose prims2) : True := by trivial

   Its conclusion was `True` — it asserted nothing at all, while its name and the
   paragraph above it asserted that compositional distinctness follows from
   differing composed barb sets. `doc/src/arch/composition.md` already flags this
   ("Composition-level distinctness is unproved"), so the spec knew; the name did
   not say so.

   The real content here is definitional: `capTypesDistinct` in
   `Composition.lean` *is* `compose ct1.primitives ≠ compose ct2.primitives`, so
   there is nothing to prove beyond the definition, and the previous paragraph
   was describing a definition as though it were a result. Deleted rather than
   restated, because restating it would produce `h → h`. -/

