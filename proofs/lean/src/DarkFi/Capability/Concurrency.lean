/-
DarkWow Concurrency — what remains of the parallel-composition layer

This file used to define parallel composition, process bisimulation over concurrency barbs, and a
"fundamental theorem" that parallel execution with disjoint key sets is weak-bisimilar to sequential
execution. All of that is deleted, and the section below is the record: `stronglyBisimilar` was
equality of a `Finset` of tags, so `parallel_commutative` reduced to `Finset.union_comm` — a true
theorem about set unions, and therefore not a theorem about parallel execution — and `KeyDisjoint`'s
`writeSetDisjoint : Bool` had neither an invariant nor a consumer. The real transition system,
structural congruence and bisimulation are in `DarkFi/Semantics/{Proc,Congruence,LTS}.lean`, and the
replacement for each deleted declaration is the table below.

What remains here is the tag vocabulary that `Gossip.lean` and the audit layers still read: the
`exhibits_*` predicates (Part 6) and `isBlockchainBarb` / `isEventGraphBarb` / `bridgeSafe` (Part 7).
Those are tag sets, not barbs in `type-system.md` §1.1's sense — a barb is now a consequence of the
transition relation and cannot be declared — so Part 6 stands as the *recorded obligation* to replace
them, not as the replacement itself.

Theorems follow type-system.md §9 (Concurrent Execution Model) and §10 (P2P Network as Replicated
Process Nets).
-/

import DarkFi.Capability.Types
import DarkFi.AxiomBudget

open DarkFi.Capability.Types

/-! ## Namespace

Declared into `DarkFi.Capability.Concurrency`, which `Gossip.lean` opens. The `open Types` this
replaces was written against a namespace that did not exist; nothing was ever imported by it. -/

namespace DarkFi.Capability.Concurrency

/- ==========================================================================
   Parts 1–4 deleted: `parallelCompose`, the bisimulation definitions, their
   two theorems, and `KeyDisjoint`
   ==========================================================================

What was here, and why it is gone:

    def parallelCompose (P Q : ConcurrentProcess) : ConcurrentProcess :=
      { name := s!"({P.name}|{Q.name})"
      , authorizationBarbs := P.authorizationBarbs ∪ Q.authorizationBarbs
      , concurrencyBarbs := P.concurrencyBarbs ∪ Q.concurrencyBarbs
      , canConcurrent := P.canConcurrent && Q.canConcurrent
      , canMerge := P.canMerge && Q.canMerge
      }

    def barbedEquivalent (P Q : ConcurrentProcess) : Prop :=
      concurrentProcessBarbs P = concurrentProcessBarbs Q
    def authorizationBisimilar (P Q : ConcurrentProcess) : Prop :=
      P.authorizationBarbs = Q.authorizationBarbs
    def concurrencyBisimilar (P Q : ConcurrentProcess) : Prop :=
      P.concurrencyBarbs = Q.concurrencyBarbs
    def stronglyBisimilar (P Q : ConcurrentProcess) : Prop := barbedEquivalent P Q

    @[axiom_budget 1] theorem parallel_commutative (P Q : ConcurrentProcess) :
      stronglyBisimilar (parallelCompose P Q) (parallelCompose Q P)
    @[axiom_budget 1] theorem parallel_associative (P Q R : ConcurrentProcess) : ...
    @[axiom_budget 0] theorem authorization_preserved ...

    structure KeyDisjoint (P Q : ConcurrentProcess) where writeSetDisjoint : Bool

`concurrencyBisimilar` and `authorizationBisimilar` are quoted because the record is meant to be
literal, not because they mattered. `concurrencyBisimilar` is in fact the same defect as `KeyDisjoint`
— a definition nothing read, recording a claim rather than making one — and `authorizationBisimilar`
was the hypothesis of the deleted `authorization_preserved` and appears nowhere else. Measured after
the deletion: `grep -rn "concurrencyBisimilar\|authorizationBisimilar" proofs/lean/src` matches
nothing, so neither has a replacement to name here.

**`stronglyBisimilar` was not a bisimulation.** It was equality of a `Finset` of tags, so
`parallel_commutative` reduced to `Finset.union_comm` — a true theorem about set union, and therefore
not a theorem about parallel execution. The consequence is measurable rather than rhetorical:
`barbedEquivalent` equates any two processes with the same barb set, so it equates two processes
whose transitions carry *different payloads* — which no bisimulation may do (the witness is the second
obligation recorded in `Semantics/LTS.lean`'s scope note, and `stronglyBisimilar` could not even
state it as a falsehood).

`KeyDisjoint`'s `writeSetDisjoint : Bool` had no invariant and no consumer: nothing read it, nothing
constrained it, so the field recorded a claim rather than making one. The real write-set notion
turned out to live in the Rust (`SledKey`, `ExecutionSchedule::build`, and the overlay diff), and its
replacement is `writeSet : Key → Prop` with the disjointness lemma *proved* rather than assumed — the
subject of `Semantics/Ledger.lean`, which **is not written yet**. Until it is, this deletion has no
replacement in the tree, and that absence is written here rather than papered over.

Replaced by, and this file's Part 5 note below is retained as the record:

| deleted | replacement |
|---|---|
| `parallelCompose` | `Proc.par` (`Semantics/Proc.lean`), with no separate combinator |
| `stronglyBisimilar` | `StrongBisim` (`Semantics/LTS.lean`) — the coinductive relation |
| `parallel_commutative` | `DarkFi.Semantics.parallel_commutative`, with `SCong` as its witness |
| `parallel_associative` | `DarkFi.Semantics.parallel_associative`, likewise |
| `authorization_preserved` | subsumed: `StrongBisim` is a congruence, so it is preserved by `par` |
| `KeyDisjoint` | `Disjoint δ.dom ε.dom` over the real key type, in `Semantics/Ledger.lean` — **owed; the module is not written yet** |

**Deleted and not renamed, deliberately.** A bridge lemma between the two notions would have left the
tree with two incompatible meanings of "bisimilar" — the situation this replacement exists to end. The
zero blast radius was measured before deleting: `grep -rn "stronglyBisimilar\|barbedEquivalent\|
parallelCompose\|KeyDisjoint" proofs/lean/src` matched nothing outside this file.

The barb-set `exhibits_*` definitions in Part 6 below are **not** part of this deletion: they are
tag-set membership tests too, but their replacement is the barb *realization* obligation (a predicate
over the transition system plus a check against the Rust path), which is a separate stage. They are
left in place with this note rather than deleted here, so that the deletion that lands is the one
whose replacement is complete.
-/

/- ==========================================================================
   Part 2: Bisimulation with Concurrency Barbs — DELETED, see above
   ========================================================================== -/

/- ==========================================================================
   Part 3: Fundamental Theorems — DELETED, see above
   ========================================================================== -/

/- ==========================================================================
   Part 4: Parallel Merge Correctness — DELETED, see above
   ========================================================================== -/

/- ==========================================================================
   Part 5: Concurrency Safety — No Deadlock
   ==========================================================================
   A process net has a deadlock if there exists a cycle of processes each
   waiting on a sync-barrier held by the next. The sync_barrier_acyclic
   condition prevents this.
-/

/-
## `has_deadlock` — deleted

    def has_deadlock (processes : List ConcurrentProcess) : Bool :=
      -- placeholder: deadlock detection via wait-for graph cycle
      false

A `Bool`-valued predicate named for deadlock detection that returned `false` for every input and
never inspected its argument. Nothing in the tree referenced it and no document cites it, so it was
not making any claim false — it was making a *name* available. Anything that had later proved "no
deadlock" by `has_deadlock p = false` would have proved nothing, and the placeholder is exactly what
makes that mistake available.

The condition the comment refers to, `sync_barrier_acyclic`, is named nowhere else either — it has
no definition in this tree. Deadlock freedom for the process net is **not modelled**, which is what
the Part 5 header above already says ("Currently assumed as a design invariant — the Rust
implementation enforces key disjointness via sled tree isolation"). That sentence is the record; a
`def` returning `false` was not.
-/

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
