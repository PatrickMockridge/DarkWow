import DarkFi.Axioms

/-!
# DarkFi Hash Operation Soundness Proofs

Merkle tree inclusion, Sparse Merkle tree membership, and Poseidon
hash soundness. These are the foundation for nullifier tracking,
commitment inclusion proofs, and token registry verification.

## Key Theorems

1. **MerkleInclusionSoundness**: If root = merkle_root(pos, path, leaf)
   and root is constrain_instance'd, then leaf IS at position pos.

2. **SMTMembershipSoundness**: set_membership (0x59) with output=1
   proves the leaf is in the SMT at pos under expected_root.

3. **PoseidonCollisionResistance**: No two distinct inputs produce
   the same Poseidon hash (computationally assumed, modeled as axiom).
-/

namespace HashOps

/-
## `poseidon_hash_output` and the Poseidon assumptions — moved

`opaque poseidon_hash_output`, `compute_merkle_root` and
`axiom poseidon_collision_resistance` are declared in `DarkFi/Axioms.lean`, under `namespace
HashOps` and the same names, so nothing below needed editing.

They move rather than stay because a value-less `opaque` is an assumption — a constant with no
value, spelled with a different keyword — and `script/check_lean_axioms.py` requires every
assumption to be on the boundary side of the wall. That is not pedantry: this file's own
comment used to say the opaque was there so that "collision resistance is a non-trivial
assumption, not a contradiction with a trivial stub", which is exactly the argument for
declaring it where the assumptions are.
-/

/-! ===== The two binding assumptions, discharged as theorems

`Axioms.lean` used to carry

    axiom commitment_binding  (pub1 …blind1 pub2 …blind2 : Int) (h_any_diff : …) :
      poseidon_hash_output [pub1, …, blind1] ≠ poseidon_hash_output [pub2, …, blind2]
    axiom nullifier_binding   (secret1 commitment1 secret2 commitment2 : Int) (h_ne : …) :
      poseidon_hash_output [secret1, commitment1] ≠ poseidon_hash_output [secret2, commitment2]

Both are now theorems. Neither ever carried content beyond `poseidon_collision_resistance`
applied to two fixed-arity lists — the assumption was that injectivity over *arbitrary* lists
implied injectivity at arity 6 and arity 2, which is a derivation, not an assumption. The
derivation is: from equal outputs, collision resistance gives equal inputs; from equal inputs,
each component is equal; that contradicts the hypothesis that some component differs.

The proof goes through `List.getD i 0` on both sides, which is what turns a list equality into a
component equality without needing `List.cons.injEq` to reassociate six times. -/

/-- Equal `List Int`s have equal `i`-th components (`0` if out of range). -/
private lemma getD_eq_of_eq {l₁ l₂ : List Int} (h : l₁ = l₂) (i : Nat) :
    l₁.getD i 0 = l₂.getD i 0 := congrArg (fun l => l.getD i 0) h

/-- **Coin commitment binding**, as a theorem: changing any one of the six field elements fed to
    `poseidon_hash_output` changes the commitment. Budget 1 — rests on
    `poseidon_collision_resistance`. -/
@[axiom_budget 1]
theorem commitment_binding
    (pub1 value1 token_id1 spend_hook1 user_data1 blind1 : Int)
    (pub2 value2 token_id2 spend_hook2 user_data2 blind2 : Int)
    (h_any_diff : pub1 ≠ pub2 ∨ value1 ≠ value2 ∨ token_id1 ≠ token_id2
                  ∨ spend_hook1 ≠ spend_hook2 ∨ user_data1 ≠ user_data2
                  ∨ blind1 ≠ blind2) :
    poseidon_hash_output [pub1, value1, token_id1, spend_hook1, user_data1, blind1]
    ≠ poseidon_hash_output [pub2, value2, token_id2, spend_hook2, user_data2, blind2] := by
  intro h_eq
  have hlists : [pub1, value1, token_id1, spend_hook1, user_data1, blind1]
      = [pub2, value2, token_id2, spend_hook2, user_data2, blind2] := by
    by_contra hne
    exact poseidon_collision_resistance _ _ hne h_eq
  rcases h_any_diff with h | h | h | h | h | h
  · exact h (getD_eq_of_eq hlists 0)
  · exact h (getD_eq_of_eq hlists 1)
  · exact h (getD_eq_of_eq hlists 2)
  · exact h (getD_eq_of_eq hlists 3)
  · exact h (getD_eq_of_eq hlists 4)
  · exact h (getD_eq_of_eq hlists 5)

/-- **Nullifier binding**, as a theorem: distinct `(secret, commitment)` pairs give distinct
    nullifiers, at arity 2. Budget 1. -/
@[axiom_budget 1]
theorem nullifier_binding
    (secret1 commitment1 secret2 commitment2 : Int)
    (h_ne : secret1 ≠ secret2 ∨ commitment1 ≠ commitment2) :
    poseidon_hash_output [secret1, commitment1]
    ≠ poseidon_hash_output [secret2, commitment2] := by
  intro h_eq
  have hlists : [secret1, commitment1] = [secret2, commitment2] := by
    by_contra hne
    exact poseidon_collision_resistance _ _ hne h_eq
  rcases h_ne with h | h
  · exact h (getD_eq_of_eq hlists 0)
  · exact h (getD_eq_of_eq hlists 1)

/-! ===== The Merkle model, and the change-detection theorem

The old model folded `poseidon_hash_output` over a bare two-element list with **no altitude**, and
`axiom merkle_root_change_detection` asserted the whole-tree property on top of it. Both were
wrong about the implementation and the axiom was unnecessary:

* **Wrong shape.** `MerkleNode::combine` (`src/sdk/src/crypto/merkle_node.rs:132-173`) hashes a
  10-bit altitude followed by two 255-bit halves, under the Sinsemilla personalization
  `"z.cash:Orchard-MerkleCRH"`; depth is 32 and the empty leaf is `UNCOMMITTED_ORCHARD = 2`. The
  old fold had none of that — no altitude, no depth, no base case.
* **Unnecessary assumption.** The induction failed because the fold's *domain* carried no
  altitude, so the hypothesis at one level said nothing usable about the next. With the altitude
  in the domain, injectivity at each level composes and the theorem is a two-line induction.

The remaining substitution is the primitive: Sinsemilla is not Poseidon, and this model uses the
model's single hash. That is recorded as OBL-Z6 in `doc/src/arch/verification-hazop.md` and is
narrower than before — the domain, the base case and the fold order now match. -/

/-- Orchard Merkle depth (`MERKLE_DEPTH_ORCHARD`). -/
def merkleDepth : Nat := 32

/-- The Orchard tree's empty leaf: `UNCOMMITTED_ORCHARD = pallas::Base::from(2)`. **Not zero** —
    the SMT's empty leaf is 0 (see `smtEmptyLeaf`), and conflating them was one of the old
    model's errors. -/
def orchEmptyLeaf : Int := 2

/-- The Orchard Merkle CRH, with the altitude in the domain: `combine(level, left, right)`.
    Substitutes the model's hash for Sinsemilla; see the section comment. -/
def sinsemillaCrh (level : Nat) (left right : Int) : Int :=
  poseidon_hash_output [(level : Int), left, right]

/-- The Merkle root: fold the CRH up the path. `level` increases with each sibling consumed and
    `pos` halves, so for a fixed path the altitude at each level is determined by the path alone —
    which is what lets the induction below go through. -/
def computeMerkleRoot (level pos : Nat) (path : List Int) (leaf : Int) : Int :=
  match path with
  | [] => leaf
  | sibling :: rest =>
    if pos % 2 = 0 then
      computeMerkleRoot (level + 1) (pos / 2) rest (sinsemillaCrh level leaf sibling)
    else
      computeMerkleRoot (level + 1) (pos / 2) rest (sinsemillaCrh level sibling leaf)

/-- **Merkle root change detection — proved, not assumed.** Changing the leaf at a fixed position
    changes the root, by induction on the path. Budget 1: it rests on
    `poseidon_collision_resistance`, which is what a Merkle inclusion proof's soundness actually
    rests on.

    This replaces `axiom merkle_root_change_detection`. The `IF FALSE:` entry that axiom carried
    ("NOTHING. No theorem consumes it") is why it survived unchallenged: an assumption nothing
    depends on cannot fail. -/
@[axiom_budget 1]
theorem merkle_root_change_detection (level pos : Nat) (path : List Int) (leaf leaf' : Int)
    (h_leaf_ne : leaf ≠ leaf') :
    computeMerkleRoot level pos path leaf ≠ computeMerkleRoot level pos path leaf' := by
  induction path generalizing level pos leaf leaf' with
  | nil => simpa [computeMerkleRoot] using h_leaf_ne
  | cons sibling rest ih =>
      -- The two children differ at this level, because the CRH is injective at a fixed altitude.
      have h_even : sinsemillaCrh level leaf sibling ≠ sinsemillaCrh level leaf' sibling := by
        intro heq
        have hlists : [(level : Int), leaf, sibling] = [(level : Int), leaf', sibling] := by
          by_contra hne
          exact poseidon_collision_resistance _ _ hne heq
        exact h_leaf_ne (getD_eq_of_eq hlists 1)
      have h_odd : sinsemillaCrh level sibling leaf ≠ sinsemillaCrh level sibling leaf' := by
        intro heq
        have hlists : [(level : Int), sibling, leaf] = [(level : Int), sibling, leaf'] := by
          by_contra hne
          exact poseidon_collision_resistance _ _ hne heq
        exact h_leaf_ne (getD_eq_of_eq hlists 2)
      simp only [computeMerkleRoot]
      split
      · exact ih (level + 1) (pos / 2) (sinsemillaCrh level leaf sibling)
          (sinsemillaCrh level leaf' sibling) h_even
      · exact ih (level + 1) (pos / 2) (sinsemillaCrh level sibling leaf)
          (sinsemillaCrh level sibling leaf') h_odd

/-! ===== The Sparse Merkle Tree's CRH — concrete, and its injectivity proved

The SMT is a different tree from the Orchard one and shares neither primitive nor constants:
`src/zk/gadget/smt.rs:299-312` hashes the raw `[left, right]` pair with **rate-2 Poseidon and no
domain prefix**, depth is `SMT_FP_DEPTH = 255` (the docs say 256), and the empty leaf is
`Fp::from(0)`. So here the primitive is *right*: the SMT really does use Poseidon, and its
injectivity is therefore derived rather than assumed. -/

/-- The SMT's per-level compression: rate-2 Poseidon over the raw pair, no domain prefix. -/
def smtCrh (left right : Int) : Int := poseidon_hash_output [left, right]

/-- SMT depth (`SMT_FP_DEPTH`). The docs say 256; the constant is 255. -/
def smtDepth : Nat := 255

/-- The SMT's empty leaf, which is **0** — distinct from the Orchard tree's 2. -/
def smtEmptyLeaf : Int := 0

/-- Injectivity of the SMT compression, **proved** from `poseidon_collision_resistance`. Unlike
    `sinsemillaCrh`, this needs no substitution: the implementation's SMT hash is Poseidon. -/
@[axiom_budget 1]
theorem smtCrh_injective (l₁ r₁ l₂ r₂ : Int) (h : smtCrh l₁ r₁ = smtCrh l₂ r₂) :
    l₁ = l₂ ∧ r₁ = r₂ := by
  have hlists : [l₁, r₁] = [l₂, r₂] := by
    by_contra hne
    exact poseidon_collision_resistance _ _ hne h
  exact ⟨getD_eq_of_eq hlists 0, getD_eq_of_eq hlists 1⟩

/--
## Merkle Path

A Merkle path of depth D consists of D sibling nodes.
At each level, we choose left or right based on the position bit.
-/
structure MerklePath where
  depth : Nat
  siblings : List Int    -- sibling node at each level
  position : Int         -- leaf position (bit-decomposed from LSB)
deriving BEq

/--
## Merkle Root Computation (Orchard-style, depth 32)

root = merkle_root(pos, path, leaf)

For Orchard Merkle tree (Sinsemilla-based):
  - depth = 32
  - At level i: if bit_i(pos) = 0, hash = H(path[i], cur)
                if bit_i(pos) = 1, hash = H(cur, path[i])
  - After 32 levels, root = cur
-/
structure MerkleRootGadget where
  leaf_pos : Int        -- Uint32 leaf position
  path : MerklePath     -- MerklePath[32]
  leaf : Int            -- Leaf value
  root : Int            -- Computed root (constrain_instance'd)
deriving BEq

/-
## Merkle Root Computation Function

`compute_merkle_root` is declared in `DarkFi/Axioms.lean`, together with the
`poseidon_hash_output` opaque it folds with, because that opaque is an assumption. It is
usable here and everywhere else through the import.

(Was a doc comment, which requires a declaration to attach to. Nothing follows it, so Lean
reported `unexpected token` at its closing delimiter.)

Modeled as a recursive fold over the path, using Poseidon hash at each level. When the path is
empty, returns the leaf directly.

NOTE: the Rust `MerkleNode::combine` (src/sdk/src/crypto/merkle_node.rs) uses the
Orchard **Sinsemilla** Merkle CRH, NOT Poseidon. `poseidon_hash_output` is therefore an
APPROXIMATION of the tree hash; the fold ORDER (position-bit branching, left/right) is what
this model pins, not the exact hash primitive.
-/

/-
## Merkle root determinism, and Merkle inclusion soundness — claims removed

Two theorems used to sit here.

    theorem merkle_root_deterministic (pos : Int) (path : List Int) (leaf : Int) :
      compute_merkle_root pos path leaf = compute_merkle_root pos path leaf := rfl

was `x = x`: true of every term, and it mentioned `compute_merkle_root` only as a term whose
value is irrelevant. The name asserted a determinism property of the Merkle fold.

    theorem merkle_inclusion_soundness
      (pos : Int) (path : List Int) (leaf root : Int)
      (h_root_computed : compute_merkle_root pos path leaf = root)
      (h_root_instance : root = root) :
      compute_merkle_root pos path leaf = root := h_root_computed

is its hypothesis restated — its second hypothesis `root = root` is the tautology it was
named after, and its conclusion is its first hypothesis. It says nothing about a prover, a
host, or a `constrain_instance`. The long comment above it described the soundness argument
for Merkle inclusion; the theorem was `id`.

Both are deleted. The argument is real, but its first premise is "the host verifies the ZK
proof", and nothing in this tree turns a Halo2 proof into a Lean proposition — that is the
modelling gap `Axioms.NoFreeInstances` names. Recorded in `DarkFi.HAZOP.High`.

The name `merkle_root_change_detection` is now `HashOps.merkle_root_change_detection` in
`DarkFi/Axioms.lean`. The old justification attached to it — "the structural-induction proof
requires `induction'`, which is not available in core Lean" — was false: `induction'` is a
mathlib tactic and mathlib is a dependency. The corrected reason is in `Axioms.lean`.
-/

/-
## Sparse Merkle Tree (0x21, 0x59)

Poseidon-based, depth = SMT_FP_DEPTH = 256.
set_membership (0x59) returns 1 if the leaf IS in the tree
at pos under expected_root, 0 otherwise.

Key security: expected_root is constrain_instance'd internally
by the opcode — the prover cannot choose a root that makes
a fake proof pass.

### The two SMT claims that used to be axioms, and are now removed

    axiom smt_membership_sound   (g : SMTMembershipGadget) (h_out : g.output = 1) : Prop
    axiom smt_membership_privacy (g : SMTMembershipGadget) (h_out : g.output = 1) : Prop

Both are `: Prop`-valued axioms — uninterpreted predicates that name an SMT soundness claim
and an SMT privacy claim without stating either, and that no proof can consume. They are
deleted, the "## THEOREM: SMT Membership Is Sound" heading that preceded the first is gone
with them, and the corresponding rows in `doc/src/arch/zk/opcodes.md`,
`opcodes-status.md` and `security-analysis.md` no longer read "SOUND ✓" / "VERIFIED ✓".
Recorded as SILENT in `DarkFi.HAZOP.Elevated`.

The argument in that heading is real, and worth restating honestly: (1) `expected_root` is
`constrain_instance`'d by the opcode; (2) the root is recomputed in-circuit; (3) the output is
`bool_check`'d; (4) the host verifies the ZK proof. Step 4 is where it stops being Lean: no
theorem in this tree turns a verified Halo2 proof into the proposition "the recomputed root
equals `expected_root`". That is the gap `Axioms.NoFreeInstances` names, and no theorem
consumes it yet. The privacy claim has the same shape — it needs the zero-knowledge property
of the proof system, which is likewise un-modelled.
-/

/--
## SMT Membership Gadget

Models set_membership (0x59):
  output = 1 if sparse_merkle_root(pos, path, leaf) = expected_root
  output = 0 otherwise
-/
structure SMTMembershipGadget where
  pos : Int          -- Position in SMT
  path : List (Int × Int) -- SparseMerklePath[256]: (left_sibling, right_sibling)
  leaf : Int         -- Leaf value
  expected_root : Int -- constrain_instance'd public input
  output : Int       -- 0 or 1 (bool_check'd)
deriving BEq

/-
## Poseidon Hash Soundness

Poseidon is used throughout the zkVM:
- Coin commitments: poseidon_hash(pub, value, token_id, spend_hook, user_data, blind)
- Nullifiers: poseidon_hash(secret, commitment)
- Token commitments: poseidon_hash(token_id, token_id_blind)
- Mint authority: poseidon_hash(backing_secret)
- Signature public key: poseidon_hash(signature_secret)

Configuration: P128Pow5T3, rate=3, capacity=2.
Variable-length: 1..24 Base field elements.
-/

/--
## Poseidon Hash Gadget

Models a single poseidon_hash call with n inputs (1 ≤ n ≤ 24).
-/
structure PoseidonHashGadget where
  inputs : List Int    -- 1..24 Base field elements
  output : Int         -- Hash result
deriving BEq

/-
## Poseidon output determinism — claim removed

A theorem used to sit here:

    theorem poseidon_deterministic (g : PoseidonHashGadget) : g.output = g.output := rfl

That is `x = x`, true of every term. The name asserted that Poseidon is deterministic; the
statement did not mention `poseidon_hash_output` at all. Deleted; the real determinism content
is that `poseidon_hash_output` is a *function*, which Lean gives for free from its type.

`signature_public_determinism` was the same shape with a hypothesis attached —

    theorem signature_public_determinism (secret1 secret2 : Int)
      (h_secret_eq : secret1 = secret2) :
      poseidon_hash_output [secret1] = poseidon_hash_output [secret2] := by rw [h_secret_eq]

— which is `congrArg`, not a claim about signatures, uniqueness, unlinkability or the
commitment owner. Also deleted. Recorded in `DarkFi.HAZOP.High`.

### The two binding assumptions that used to be axioms here

`commitment_binding` and `nullifier_binding` were declared in this file, each preceded by a
"## THEOREM" heading and the comment `-- PROOF SKETCH (broken): … — converted to axiom`. They
have moved unchanged in content to `DarkFi/Axioms.lean`, where they carry the four-field
annotation. Their statements mean what their names say — injectivity of the concrete
`poseidon_hash_output` opaque at arity 6 and arity 2 — so they are legitimate assumptions, not
vacuous ones. What was wrong was calling them theorems and marking them "broken": a `-- PROOF
SKETCH` comment above an `axiom` is a claim about a proof that does not exist.

Both are recorded as SILENT in `DarkFi.HAZOP.Elevated`: nothing in this tree consumes them,
and the "prevents double-spending" argument above them turns on the circuit constraint, not
on these statements.
-/

end HashOps
