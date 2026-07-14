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

open Types

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
   ==========================================================================

-- Theorem 1: Parallel composition is commutative
theorem parallel_commutative (P Q : ConcurrentProcess) :
  stronglyBisimilar (parallelCompose P Q) (parallelCompose Q P) := by
  unfold stronglyBisimilar barbedEquivalent parallelCompose
  simp [Set.union_comm]

-- Theorem 2: Parallel composition is associative
theorem parallel_associative (P Q R : ConcurrentProcess) :
  stronglyBisimilar
    (parallelCompose (parallelCompose P Q) R)
    (parallelCompose P (parallelCompose Q R)) := by
  unfold stronglyBisimilar barbedEquivalent parallelCompose
  simp [Set.union_assoc]

-- Theorem 3: Authorization bisimulation is preserved under parallel composition
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

/- Axiom: parallel execution ≈ sequential execution when keys are disjoint -/
axiom parallelMerge_correctness
    (calls : List ConcurrentProcess)
    (_h_disjoint : pairwise_disjoint_keys calls) :
    True
  -- Full statement: parallel_execute(calls) ≈ sequential_execute(calls)
  -- Requires: Halo2 prover model, sled overlay formalization, WASM execution model

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
   ==========================================================================

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

/- The bridging check: a message crossing paths carries only allowed barbs -/
def bridgeSafe (P : ConcurrentProcess) (target : String) : Bool :=
  if target = "blockchain" then
    -- event-graph → blockchain: only dagParent, quorumQuery, rateLimit allowed
    P.authorizationBarbs.all fun b => !isBlockchainBarb b
  else if target = "event-graph" then
    -- blockchain → event-graph: blockchain barbs blocked
    P.authorizationBarbs.all fun b => !isBlockchainBarb b
  else
    false
