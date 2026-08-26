/-
DarkWow.Fee.Window — Fee-Window Boundary Emission

Formalizes the fee-window boundary invariant from fee-spec.md §12: the miner
signals `fee_window_flags` only at boundary blocks (height ≡ 0 mod WINDOW), so
a node at height < WINDOW has NOT yet emitted a boundary log. That pre-boundary
state is a legal, non-error state the multi-node consensus check (L3-FW-2)
SHALL treat as "not yet reached", never as an abort.
-/

import Mathlib

namespace DarkFi.Fee

/- The fee window boundary period (fee-spec.md §12.8.2: every 20 blocks). -/
def WINDOW : Nat := 20

/- A node emits the fee-window boundary log exactly at boundary blocks
   (height > 0 and height ≡ 0 mod WINDOW). -/
def emitsAtBoundary (height : Nat) : Prop :=
  height > 0 ∧ height % WINDOW = 0

/- Theorem (pre-boundary no emission): a node strictly below the first window
   boundary has no boundary log — the "not yet reached" state. -/
theorem pre_boundary_no_emission (height : Nat) :
    height > 0 → height < WINDOW → ¬ emitsAtBoundary height := by
  intro h_pos h_lt h_emit
  unfold emitsAtBoundary at h_emit
  rcases h_emit with ⟨_, h_mod_zero⟩
  have h_mod_eq : height % WINDOW = height := Nat.mod_eq_of_lt h_lt
  rw [h_mod_eq] at h_mod_zero
  exact (Nat.ne_of_gt h_pos) h_mod_zero

/- Theorem: the first boundary is exactly WINDOW (height 20). -/
theorem first_boundary_at_window : emitsAtBoundary WINDOW := by
  unfold emitsAtBoundary
  constructor
  · omega
  · exact Nat.mod_self WINDOW

end DarkFi.Fee
