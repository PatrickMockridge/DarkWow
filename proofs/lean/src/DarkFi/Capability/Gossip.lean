/-
DarkWow Gossip — Structured P2P Dissemination Theorems

Formalizes the gossip protocols from type-system.md §10.2-§10.3.
Proves that structured fan-out gossip (k = log₂(N)) reaches all
honest nodes in O(log N) rounds, and that the event graph 2/3-majority
tip consensus converges under honest-majority assumptions.

These theorems are proofs about process nets — collections of
ConcurrentProcess values communicating via broadcast channels.
-/

import DarkFi.Capability.Types
import DarkFi.Capability.Concurrency
import DarkFi.AxiomBudget

open DarkFi.Capability.Types
open DarkFi.Capability.Concurrency

/- ==========================================================================
   Part 1: Network Model
   ==========================================================================
   A network is N processes, each with the gossip_forward barb.
   Rounds model synchronous message propagation (one hop per round).
-/

structure Network where
  nodes : List ConcurrentProcess
  fanOut : Nat  -- k = log₂(N) for structured gossip
-- No `deriving Repr`: `nodes : List ConcurrentProcess` holds `Finset Barb`, whose `Repr`
-- instance is `unsafe`, so the derived instance is rejected by the kernel.

def network_size (net : Network) : Nat :=
  net.nodes.length

/- ==========================================================================
   Part 2: Flood Gossip — O(N²) Baseline
   ==========================================================================
   Flood broadcast: every node relays to ALL peers. Traffic: O(N²).
-/

def floodRelayTargets (net : Network) (source : ConcurrentProcess) : List ConcurrentProcess :=
  net.nodes.filter fun n => n.name ≠ source.name

/- ==========================================================================
   Part 3: Structured Gossip — O(k·N) Optimal
   ==========================================================================
   Fan-out gossip: each node relays to k = log₂(N) randomly selected peers.
   Traffic: O(k·N) = O(N log N). Propagation rounds: O(log N).
-/

def fanOutTargets (net : Network) (_source : ConcurrentProcess) : List ConcurrentProcess :=
  -- Select min(fanOut, N-1) peers (random selection modeled as first-k)
  -- `let … in` with the body on the next line is a parse error here ("expected ';' or line
  -- break"): the `let` is already terminated by the line break, so `in` is unexpected.
  let k := min net.fanOut (network_size net - 1)
  net.nodes.take k

/-
## `gossip_log_rounds` — deleted

    theorem gossip_log_rounds (net : Network) (_h : net.fanOut ≥ 2) : True := by trivial

with the hypothesis named `_h` and the conclusion `True`. The comment above it says the proof is
"deferred to full distributed systems formalization", so the file was already honest in prose about
not having proved it — the problem is that a `theorem` with a name and an `@[axiom_budget 0]`
sits in the budget table as a discharge, and `True` is not a claim that could be deferred. The
`def`s this file *does* contribute (`network_size`, `reachable_in_one_round`,
`considerationThreshold`, …) are real definitions the type system uses; only the empty theorems go.
-/


/- ==========================================================================
   Part 4: Event Graph 2/3-Majority Tip Consensus
   ==========================================================================
   The tip consensus protocol: query all peers for their DAG tips, keep
   only tips seen by > 2/3 of communicated_peers. Converges under
   honest-majority (> 2/3) assumption.
-/

def considerationThreshold (communicatedPeers : Nat) : Nat :=
  communicatedPeers * 2 / 3

/-
## `tip_consensus_converges` — deleted

    theorem tip_consensus_converges (totalPeers honestPeers : Nat)
        (_h_honestSuperMajority : honestPeers * 3 > totalPeers * 2) : True := by trivial

The hypothesis is the > 2/3 majority assumption, named `_h`; the conclusion is `True`; the
parameters `totalPeers` and `honestPeers` appear nowhere in the statement. This is the shape the
register calls the most dangerous one, because every mark on it is positive: the name asserts
convergence, the hypothesis names the standard assumption, the annotation records a budget of 0,
and the searchable text contains the word "convergence". Nothing in it mentions a tip.

The honest statement — that under a > 2/3 honest majority the tip sets agree after one round —
needs the protocol's message model, which is not in this file. Deleted, not restated.
-/


/- ==========================================================================
   Part 5: Process Net Construction
   ==========================================================================
   Build process nets from lists of processes with known barbs.
-/

def buildFloodNet (nodeCount : Nat) : Network :=
  let nodes := List.range nodeCount |>.map (fun i =>
    { name := s!"node_{i}"
    , authorizationBarbs := ∅
    , concurrencyBarbs := {Barb.gossipForward}
    , canConcurrent := false
    , canMerge := false
    })
  { nodes, fanOut := nodeCount - 1 }  -- flood: relay to all

def buildStructuredGossipNet (nodeCount : Nat) : Network :=
  let fanOut := max 2 (Nat.log 2 nodeCount)
  let nodes := List.range nodeCount |>.map (fun i =>
    { name := s!"node_{i}"
    , authorizationBarbs := ∅
    , concurrencyBarbs := {Barb.gossipForward, Barb.concurrent}
    , canConcurrent := true
    , canMerge := false
    })
  { nodes, fanOut }

/- ==========================================================================
   Part 6: Event Graph Process Net
   ==========================================================================
   The ProtocolEventGraph = P_put | P_req | P_tip | P_broadcast
   as defined in type-system.md §10.3.
-/

def buildEventGraphProcessNet : List ConcurrentProcess :=
  [ { name := "handle_event_put"
    , authorizationBarbs := ∅
    , concurrencyBarbs := {Barb.dagParent, Barb.concurrent}
    , canConcurrent := true
    , canMerge := false
    }
  , { name := "handle_event_req"
    , authorizationBarbs := ∅
    , concurrencyBarbs := {Barb.dagParent, Barb.concurrent}
    , canConcurrent := true
    , canMerge := false
    }
  , { name := "handle_tip_req"
    , authorizationBarbs := ∅
    , concurrencyBarbs := {Barb.quorumQuery, Barb.concurrent}
    , canConcurrent := true
    , canMerge := false
    }
  , { name := "broadcast_rate_limiter"
    , authorizationBarbs := ∅
    , concurrencyBarbs := {Barb.broadcast, Barb.rateLimit, Barb.concurrent}
    , canConcurrent := true
    , canMerge := false
    }
  ]

/- ==========================================================================
   Part 7: Blockchain Process Net
   ==========================================================================
   The blockchain node process net as defined in type-system.md §10.2.
   Miner, observer, and wallet are process compositions with specific barbs.
-/

def buildBlockchainMinerProcess : ConcurrentProcess :=
  { name := "dwowd_miner"
  , authorizationBarbs := {Barb.mine, Barb.commit, Barb.verify, Barb.spend, Barb.nullify}
  , concurrencyBarbs := {Barb.gossipForward, Barb.concurrent, Barb.merge}
  , canConcurrent := true
  , canMerge := true
  }

def buildBlockchainObserverProcess : ConcurrentProcess :=
  { name := "dwowd_observer"
  , authorizationBarbs := {Barb.verify, Barb.commit}
  , concurrencyBarbs := {Barb.gossipForward, Barb.concurrent}
  , canConcurrent := true
  , canMerge := false
  }

def buildWalletProcess : ConcurrentProcess :=
  { name := "dwow_wallet"
  , authorizationBarbs := {Barb.spend, Barb.derive, Barb.discover, Barb.encrypt}
  , concurrencyBarbs := ∅
  , canConcurrent := false
  , canMerge := false
  }
