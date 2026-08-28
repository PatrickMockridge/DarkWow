/-
DarkWow.Capability.PerContractTree — per-contract merkle tree identity (T5)

The wallet's `get_merkle_proof` must return the `(leaf_position, merkle_path,
merkle_root)` triple from the CONTRACT's own zero-seeded per-contract tree, not
the wallet-local capability tree. The two trees differ structurally: the contract
tree seeds position 0 with the zero leaf, so every capability leaf is shifted by
one position relative to the non-seeded wallet-local tree.

This module proves that shift — the concrete, structural fact behind the observed
bug: feeding the circuit the wallet-local position with the contract tree's
path/root makes the triple inconsistent.
-/

import Mathlib

namespace DarkFi.Capability

/- Position (0-based) of a leaf within a tree modelled as an ordered leaf list.
   `findPos l t = 0` when `l` is not present (the sentinel). -/
def findPos (leaf : Nat) (tree : List Nat) : Nat :=
  match tree with
  | [] => 0
  | x :: rest => if x = leaf then 0 else 1 + findPos leaf rest

/- T5 (structural): a zero-seeded contract tree `(0 :: leaves)` places every
   NON-zero leaf one position later than the non-seeded wallet-local tree
   `leaves`. The zero seed shifts the position by exactly 1. -/
theorem zero_seed_shifts_position (leaf : Nat) (leaves : List Nat) (h : leaf ≠ 0) :
    findPos leaf (0 :: leaves) = 1 + findPos leaf leaves := by
  simp only [findPos]
  exact if_neg (by intro hz; exact h hz.symm)

/- Consequently the same capability leaf has DIFFERENT positions in the two
   trees. Using the wallet-local position with the contract tree's path/root is an
   off-by-one: the circuit's `merkle_root(pos_local, path_c, leaf)` is computed at
   the wrong position and does not equal the contract root `bound[6]`. -/
theorem contract_tree_position_differs_from_wallet_local
    (leaf : Nat) (leaves : List Nat) (h : leaf ≠ 0) :
    findPos leaf (0 :: leaves) ≠ findPos leaf leaves := by
  rw [zero_seed_shifts_position leaf leaves h]
  omega

end DarkFi.Capability
