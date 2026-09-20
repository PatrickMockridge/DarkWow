/-
DarkWow Concurrency — ρ-Calculus Parallel Composition Theorems

Extends the capability type system (Types.lean) with concurrent execution
semantics. Defines parallel composition, process bisimulation extended to
concurrency barbs, and the fundamental theorem: parallel execution with
disjoint key sets is weak-bisimilar to sequential execution.

Theorems follow type-system.md §9 (Concurrent Execution Model) and §10
(P2P Network as Replicated Process Nets).
-/

import DarkFi.Capability.Types
import DarkFi.AxiomBudget

open DarkFi.Capability.Types

/-! ## Namespace

Declared into `DarkFi.Capability.Concurrency`, which `Gossip.lean` opens. The `open Types` this
replaces was written against a namespace that did not exist; nothing was ever imported by it. -/

namespace DarkFi.Capability.Concurrency

/- ==========================================================================
   Part 1: Parallel Composition
   ==========================================================================
   P | Q: execute P and Q concurrently, synchronize on shared names.
   Associative and commutative up to strong bisimulation.
-/

def parallelCompose (P Q : ConcurrentProcess) : ConcurrentProcess :=
  { name := s!"({P.name}|{Q.name})"
  , authorizationBarbs := P.authorizationBarbs ∪ Q.authorizationBarbs
  , concurrencyBarbs := P.concurrencyBarbs ∪ Q.concurrencyBarbs
  , canConcurrent := P.canConcurrent && Q.canConcurrent
  , canMerge := P.canMerge && Q.canMerge
  }

/- ==========================================================================
   Part 2: Bisimulation with Concurrency Barbs
   ==========================================================================
   Two processes are strongly bisimilar (P ~ Q) iff an observer cannot
   distinguish them through interaction — including concurrency observations.
-/

def barbedEquivalent (P Q : ConcurrentProcess) : Prop :=
  concurrentProcessBarbs P = concurrentProcessBarbs Q

def authorizationBisimilar (P Q : ConcurrentProcess) : Prop :=
  P.authorizationBarbs = Q.authorizationBarbs

def concurrencyBisimilar (P Q : ConcurrentProcess) : Prop :=
  P.concurrencyBarbs = Q.concurrencyBarbs

/- Strong bisimulation: all barbs (authorization + concurrency) must match -/
def stronglyBisimilar (P Q : ConcurrentProcess) : Prop :=
  barbedEquivalent P Q

/- ==========================================================================
   Part 3: Fundamental Theorems
   ========================================================================== -/

-- Theorem 1: Parallel composition is commutative
--
-- `simp [Set.union_comm]` reported "made no progress": `parallelCompose` unions *`Finset`s*, not
-- `Set`s, so the set-level commutativity lemma never matched. The two are equal because
-- `Finset` union is commutative and associative as propositional membership — `ext b` turns the
-- `Finset` equality into `b ∈ _ ↔ b ∈ _` and `tauto` discharges it.
@[axiom_budget 1]
theorem parallel_commutative (P Q : ConcurrentProcess) :
  stronglyBisimilar (parallelCompose P Q) (parallelCompose Q P) := by
  simp only [stronglyBisimilar, barbedEquivalent, concurrentProcessBarbs, parallelCompose]
  ext b
  simp only [Finset.mem_union]
  tauto

-- Theorem 2: Parallel composition is associative
@[axiom_budget 1]
theorem parallel_associative (P Q R : ConcurrentProcess) :
  stronglyBisimilar
    (parallelCompose (parallelCompose P Q) R)
    (parallelCompose P (parallelCompose Q R)) := by
  simp only [stronglyBisimilar, barbedEquivalent, concurrentProcessBarbs, parallelCompose]
  ext b
  simp only [Finset.mem_union]
  tauto

-- Theorem 3: Authorization bisimulation is preserved under parallel composition
@[axiom_budget 0]
theorem authorization_preserved (P Q R : ConcurrentProcess)
    (h : authorizationBisimilar P Q) :
    authorizationBisimilar (parallelCompose P R) (parallelCompose Q R) := by
  unfold authorizationBisimilar parallelCompose at *
  simp [h]

/- ==========================================================================
   Part 4: Parallel Merge Correctness (Proof Sketch)
   ==========================================================================
   The fundamental theorem: if two contract calls write to disjoint key sets,
   executing them in parallel is weak-bisimilar to executing them sequentially.

   Full formalization requires:
   1. A model of sled tree overlay state (key-value store)
   2. A definition of "disjoint key sets" for contract calls
   3. A model of WASM execution inside the zkVM (Halo2 prover)

   This is stated as an axiom (PROOF SKETCH) pending full Halo2 formalization.
-/

/- Two calls are key-disjoint if no key written by P₁ appears in P₂'s write set -/
structure KeyDisjoint (P Q : ConcurrentProcess) where
  writeSetDisjoint : Bool
  deriving Repr

/- Assumption: parallel execution ≈ sequential execution when keys are disjoint.
   This is the formal justification for parallel contract execution (type-system.md §1.2).
   Proving this requires: Halo2 prover model, sled overlay formalization, WASM execution model.
   Currently assumed as a design invariant — the Rust implementation enforces key disjointness
   via sled tree isolation. -/

/- ==========================================================================
   Part 5: Concurrency Safety — No Deadlock
   ==========================================================================
   A process net has a deadlock if there exists a cycle of processes each
   waiting on a sync-barrier held by the next. The sync_barrier_acyclic
   condition prevents this.
-/

def has_deadlock (processes : List ConcurrentProcess) : Bool :=
  -- placeholder: deadlock detection via wait-for graph cycle
  false

/- ==========================================================================
   Part 6: Concurrency Barb Predicates
   ========================================================================== -/

def exhibits_concurrent (P : ConcurrentProcess) : Bool :=
  Barb.concurrent ∈ P.concurrencyBarbs

def exhibits_merge (P : ConcurrentProcess) : Bool :=
  Barb.merge ∈ P.concurrencyBarbs

def exhibits_broadcast (P : ConcurrentProcess) : Bool :=
  Barb.broadcast ∈ P.concurrencyBarbs

def exhibits_sync_barrier (P : ConcurrentProcess) : Bool :=
  Barb.syncBarrier ∈ P.concurrencyBarbs

def exhibits_gossip_forward (P : ConcurrentProcess) : Bool :=
  Barb.gossipForward ∈ P.concurrencyBarbs

def exhibits_quorum_query (P : ConcurrentProcess) : Bool :=
  Barb.quorumQuery ∈ P.concurrencyBarbs

def exhibits_dag_parent (P : ConcurrentProcess) : Bool :=
  Barb.dagParent ∈ P.concurrencyBarbs

/- ==========================================================================
   Part 7: Quarantine Boundary (type-system.md §10.4)
   ==========================================================================
   The event graph sled tree MUST NOT touch blockchain execution sled trees.
   This is enforced at the type level: processes in the blockchain scope
   do not hold a reference to eventgraph_sled, and vice versa.
-/

def isBlockchainBarb (b : Barb) : Bool :=
  match b with
  | Barb.spend => true
  | Barb.nullify => true
  | Barb.commit => true
  | Barb.verify => true
  | Barb.mine => true
  | _ => false

def isEventGraphBarb (b : Barb) : Bool :=
  match b with
  | Barb.dagParent => true
  | Barb.quorumQuery => true
  | Barb.rateLimit => true
  | _ => false

/- The bridging check: a message crossing paths carries only allowed barbs.

   `Finset.all` does not exist in this mathlib, and this file could not compile for that reason
   alone. "No barb in the set satisfies `isBlockchainBarb`" is stated as the cardinality of the
   filtered set being zero, which needs only `Finset.filter` and `Finset.card`.

   NOTE: both branches below are the *same* predicate, which is almost certainly a copy-paste
   error rather than the intent — the comments describe opposite directions ("only
   dagParent/quorumQuery/rateLimit allowed" versus "blockchain barbs blocked") but the code
   checks the same thing in both. Recorded rather than changed: the fix is a semantics decision
   about which barbs each direction permits, not a compile error. -/
def bridgeSafe (P : ConcurrentProcess) (target : String) : Bool :=
  if target = "blockchain" then
    -- event-graph → blockchain: only dagParent, quorumQuery, rateLimit allowed
    (P.authorizationBarbs.filter (fun b => isBlockchainBarb b = true)).card == 0
  else if target = "event-graph" then
    -- blockchain → event-graph: blockchain barbs blocked
    (P.authorizationBarbs.filter (fun b => isBlockchainBarb b = true)).card == 0
  else
    false

end DarkFi.Capability.Concurrency
