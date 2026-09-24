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
import DarkFi.AxiomBudget

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
@[axiom_budget 0]
theorem zero_seed_shifts_position (leaf : Nat) (leaves : List Nat) (h : leaf ≠ 0) :
    findPos leaf (0 :: leaves) = 1 + findPos leaf leaves := by
  simp only [findPos]
  exact if_neg (by intro hz; exact h hz.symm)

/- Consequently the same capability leaf has DIFFERENT positions in the two
   trees. Using the wallet-local position with the contract tree's path/root is an
   off-by-one: the circuit's `merkle_root(pos_local, path_c, leaf)` is computed at
   the wrong position and does not equal the contract root `bound[6]`. -/
@[axiom_budget 0]
theorem contract_tree_position_differs_from_wallet_local
    (leaf : Nat) (leaves : List Nat) (h : leaf ≠ 0) :
    findPos leaf (0 :: leaves) ≠ findPos leaf leaves := by
  rw [zero_seed_shifts_position leaf leaves h]
  omega

/- ==========================================================================
   The invariant's stronger half: the mixed triple does not hash to the root
   ==========================================================================
   The two theorems above prove the off-by-one in the *position*. §6.4.1's invariant 8 (T5) claims
   more, and this file stated it as prose in both places — the module header ("feeding the circuit the
   wallet-local position with the contract tree's path/root makes the triple inconsistent") and the
   comment above ("does not equal the contract root `bound[6]`"). That is a claim about the *root*, and
   it is the one that matters: the position on its own is a fact about two trees, while the failure is
   a fact about a proof.

   What the root is a function of is the triple the circuit feeds the hash, so that is what is
   modelled: `RootPreimage` is that triple, and the two facts below are the invariant —
   structurally, and then through the hash.

   **The hash's collision resistance is a hypothesis here rather than an axiom**, which is the shape
   this layer uses for a premise the model does not own (`Circuits/InstanceDerivation.lean` does the
   same with the circuit's statement list): the general theorem takes `Function.Injective hash` as a
   binder, so what it proves is "given the CCR the circuit already assumes, the mixing changes the
   root" — and the concrete instance below needs no assumption at all.
   ========================================================================== -/

/-- The triple the circuit hashes: the leaf, its position in the tree, and the path of siblings.
    Modelled as data rather than as a `Nat` because the *inconsistency* the invariant is about is a
    difference between two of these — and a model that hashed them away would have to assume its way
    back to the fact. -/
structure RootPreimage where
  leaf : Nat
  pos : Nat
  path : List Nat
deriving DecidableEq, Repr

/-- The honest triple for a leaf in a declared tree: the position `findPos` gives, with the path the
    caller supplied. This is `get_merkle_proof`'s contract-side half, and it is the value the invariant
    says must come from *one* tree. -/
def contractTriple (path : List Nat) (leaf : Nat) (contractTree : List Nat) : RootPreimage :=
  { leaf := leaf, pos := findPos leaf contractTree, path := path }

/-- The mixed triple the invariant forbids: the same leaf and path, but the **wallet-local** tree's
    position — which is what substituting `cap.leaf_position` for the reconstructed contract-tree
    position produces. -/
def mixedTriple (path : List Nat) (leaf : Nat) (walletLocalTree : List Nat) : RootPreimage :=
  { leaf := leaf, pos := findPos leaf walletLocalTree, path := path }

/-- **The two triples differ**, for every non-zero leaf — the invariant's structural half, and it
    needs no assumption about the hash: the preimages are unequal because their positions are. -/
@[axiom_budget 0]
theorem mixed_triple_differs (path : List Nat) (leaf : Nat) (leaves : List Nat) (h : leaf ≠ 0) :
    contractTriple path leaf (0 :: leaves) ≠ mixedTriple path leaf leaves := by
  intro heq
  have hpos : findPos leaf (0 :: leaves) = findPos leaf leaves := by
    simpa [contractTriple, mixedTriple] using congrArg RootPreimage.pos heq
  exact contract_tree_position_differs_from_wallet_local leaf leaves h hpos

/-- **And so the roots differ, given the hash is injective on them** — which is the claim the prose
    above was making. Stated over an arbitrary `hash : RootPreimage → Nat` so the theorem is about the
    *argument* rather than about a particular Merkle construction, and with collision resistance as a
    hypothesis rather than an axiom, so a reader sees exactly what it rests on.

    **The proof does not mention `hash`**, and the assumption gate's unused-binder arm reports it for
    that reason (the same arm reports `WritePath.construct_sound` and `HashOps.hash_ne_of_component_ne`).
    That is this arm's documented legitimate class rather than an oversight: what the proof needs is
    `hinj`, and `hash`'s identity is carried by it — a binder whose content another binder already has. -/
@[axiom_budget 0]
theorem mixing_the_trees_changes_the_root
    (hash : RootPreimage → Nat) (hinj : Function.Injective hash)
    (path : List Nat) (leaf : Nat) (leaves : List Nat) (h : leaf ≠ 0) :
    hash (contractTriple path leaf (0 :: leaves)) ≠ hash (mixedTriple path leaf leaves) :=
  fun heq => mixed_triple_differs path leaf leaves h (hinj heq)

/-- **The falsifier, at concrete values and with no assumption.** A three-leaf contract tree whose
    position 0 is the zero seed, leaf `42` at position 2 with a one-element path; the honest and mixed
    triples are computed and are different *values*. This is what stops the two theorems above from
    being true of every pair of triples: they are not — only of pairs whose positions differ. -/
@[axiom_budget 0]
theorem a_concrete_mixed_triple_differs :
    contractTriple [7] 42 [0, 11, 42] ≠ mixedTriple [7] 42 [11, 42] := by
  decide

end DarkFi.Capability
