import DarkFi.Combinatorial.StateSpace
import DarkFi.AxiomBudget

/-!
# State Transition Combinatorics

Defines the set of valid state transitions for each L1 Box and Purse operation.
Each operation selects a target from the anonymity set, applies constraints,
and (for non-terminal operations) creates a new object.

The key combinatorial insight: in L1, each operation has `N` valid targets
(N = number of unspent objects). This is the anonymity set — the observer
cannot determine which object was targeted.
-/

open Combinatorial

namespace Combinatorial.Transitions

/-! ==========================================================================
   Part 1: Box Transitions
   ==========================================================================
   Box Put: consume old Box → create new Box (new Merkle leaf)
   Box Take: consume old Box → terminal (nullifier only, no new leaf)
-/

/--
Count of valid Box Put transitions given N concurrent objects and M possible
new contents commitments.

Each of the N objects can be consumed (target selection), and for each, M
different new contents commitments are possible.

Total valid transitions = N * M
-/
def boxPutTransitionCount (N M : Nat) : Nat := N * M

/--
Count of valid Box Take transitions given N concurrent objects.
Take is terminal — it consumes an object without creating a new one.

Total valid transitions = N (just pick which object to consume)
-/
def boxTakeTransitionCount (N : Nat) : Nat := N

/--
Total Box state transitions for N objects:
  Put: N * M (target × new contents)
  Take: N (target only)
  Total: N * (M + 1)
-/
def boxTotalTransitionCount (N M : Nat) : Nat :=
  boxPutTransitionCount N M + boxTakeTransitionCount N

/-! ==========================================================================
   Part 2: Purse Transitions
   ==========================================================================
   Purse Deposit: consume old Purse → create new Purse (value increase)
   Purse Withdraw: consume old Purse → create new Purse (value decrease, bounds)
   Purse Balance: read-only (no state change, no nullifier)
-/

/--
Count of valid Purse Deposit/Withdraw transitions given N concurrent objects
and A possible amount values.

Each of the N objects can be targeted, and for each, A different
deposit/withdraw amounts are possible (within balance bounds).

Total valid transitions = N * A
-/
def purseMutateTransitionCount (N A : Nat) : Nat := N * A

/--
Purse Balance is a read-only query — it does not consume or create state.
For N objects, there are exactly N valid balance queries (one per object).
The reply reveals only: "yes, object i has balance b." The observer learns
nothing except the existence of a valid inclusion proof.

Total valid queries = N
-/
def purseBalanceQueryCount (N : Nat) : Nat := N

/--
Total Purse state transitions for N objects:
  Deposit: N * A
  Withdraw: N * A
  Balance: N (read-only, no state change)
  Total: 2*N*A + N
-/
def purseTotalTransitionCount (N A : Nat) : Nat :=
  purseMutateTransitionCount N A + purseMutateTransitionCount N A + purseBalanceQueryCount N

/-! ==========================================================================
   Part 3: Sequential Transition Trajectories
   ==========================================================================
   For K sequential operations, the number of valid trajectories grows
   combinatorially in L1 but is exactly 1 in L2.
-/

/--
Count of valid K-step trajectories in L1 for N objects.
For the first operation: N choices. After consuming one, N-1 remain.
Each non-terminal operation replaces the consumed object with a new one,
keeping the active set at N (consume+create model).

For Box Put: N targets × M contents = N*M choices per step
For Box Take: N targets, reduces active set to N-1
For Purse Deposit/Withdraw: N targets × A amounts = N*A choices per step

This function computes the total for K mixed operations, simplified to
just multiply N choices per step for a lower bound.
-/
def l1TrajectoryCount (N K : Nat) : Nat := N ^ K

/-- In L2, there is exactly 1 valid trajectory for any sequence of operations because there is only
    1 object to operate on. No target selection, no anonymity set, no combinatorial branching.

    `K` is taken and ignored: that *is* the claim — the count does not depend on the number of
    operations. It is kept in the signature for arity symmetry with `l1TrajectoryCount N K`, and
    bound as `_K` so the signature does not read as though the value were used.

    Because the body is the literal `1`, `l2_singleton_trajectory` (`ComplexityJump.lean`) proves
    its content by `rfl` — the count *is* the definition. That is a modelling choice stated in
    prose here, not a derived fact, and the `l1_exceeds_l2` comparison against it is where the real
    arithmetic lives. -/
def l2TrajectoryCount (_K : Nat) : Nat := 1

/-! ==========================================================================
   Part 4: Trajectory Ratio — The Combinatorial Explosion
   ==========================================================================
   The ratio l1TrajectoryCount / l2TrajectoryCount quantifies the
   combinatorial jump from L2 to L1. For N=10, K=5:
     L1 ≥ 10^5 = 100,000
     L2 = 1
     Ratio = 100,000x
-/

/--
The L1/L2 trajectory ratio for N objects and K operations.
This is the combinatorial anonymity multiplier — how many times more
"paths through state space" L1 provides vs L2.
-/
def trajectoryRatio (N K : Nat) : Nat :=
  l1TrajectoryCount N K / l2TrajectoryCount K

/-! ==========================================================================
   Part 5: Invariant — Consume+Create Keeps the Active Set's Size

   The consume+create model: each non-terminal operation removes one object (via nullifier) and adds
   one object (via a new leaf), so the count of active objects is unchanged. Without it, each
   operation would ADD an object without removing the old one, and the state would grow without
   bound: N, N+1, N+2, …

   **Until 2026-09-24 this section's theorem was not about this model at all.** What stood here was
   `theorem consumeCreatePreservesCount (N K : Nat) : N + K - K = N`, closed by `omega` — true of
   every `N` and `K`, mentioning nothing in this file, and named for an invariant of a step function
   that did not exist. It survived the tautology check because that check compares the two sides
   *syntactically* and `N + K - K` is not written as `N`; the soft signal that fires on a statement
   mentioning no constant this project declares **did** fire on it, and this is the first of those
   16 it has been acted on.

   The restatement is about the model, and its hypothesis is its whole content: there has to be an
   object to consume. At `objects = []` the claim is **false** — `(([]).drop 1 ++ [w]).length` is 1
   against 0 — so the empty set is the boundary the old statement stepped over by being about `Nat`
   rather than about objects.
   ========================================================================== -/

/-- **One consume+create step preserves the number of active objects.** The object at the head of
    the set is consumed and one new object takes its place, so the count is unchanged — and it is
    unchanged *because* something was consumed, which is what `h` supplies. The hypothesis is
    load-bearing rather than defensive: see the section note for why the statement is false without
    it, and note that the old form of this theorem had no place to put that fact. -/
@[axiom_budget 0]
theorem consumeCreatePreservesCount (s : L1AnonymitySet) (w : WitnessState)
    (h : s.objects ≠ []) :
    ((s.objects.drop 1) ++ [w]).length = s.objects.length := by
  cases hobj : s.objects with
  | nil => exact absurd hobj h
  | cons o rest => simp [hobj]

end Combinatorial.Transitions
