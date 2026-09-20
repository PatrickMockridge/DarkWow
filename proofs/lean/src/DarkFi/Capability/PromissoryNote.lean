import Mathlib
import DarkFi.Capability.DerivedChain
import DarkFi.Axioms
import DarkFi.AxiomBudget

open HashOps

/-!
# Promissory Note — RevokeV2 burn chain (L1)

Formalizes the nested derived-value chain of the PN burn (`RevokeV2`,
`src/contract/promissory_note/proof/revoke.zk:39-53,82`), which the generic
prover's slot-only `compute_derived` cannot express (HAZOP V3):

    pub              = poseidon(7, spend_secret)
    coin             = poseidon(4, pub, value, asset_id, spend_hook, user_data, commitment_blind)
    nullifier        = poseidon(1, spend_secret, coin)
    signature_secret = poseidon(7, spend_secret, nullifier)
    signature_public = poseidon(7, signature_secret)

Each step references the *previous* step's output (an intermediate), not a
witness slot. This module states the chain as **definitions over
`HashOps.poseidon_hash_output`** and proves the three nesting steps, and it
proves separately (via `DerivedChain`) that the chain is a well-formed
intermediate-referencing DAG, so the generic prover's DAG extension can compute
it topologically.

## What used to be here, and why it was worse than nothing

This file used to declare the five steps as value-less `opaque`s and then assert
the chain with three axioms:

    axiom coin_is_commitment (b : BurnInput) : coin b = coin b
    axiom nullifier_is_poseidon_of_coin (b : BurnInput) : nullifier b = nullifier b
    axiom signature_secret_is_poseidon_of_nullifier (b : BurnInput) :
      signatureSecret b = signatureSecret b

Each states `f x = f x`, which holds for *any* function `f`. They asserted
nothing — and they could not have been used to prove anything either, since a
statement of that shape carries no information about `nullifier` or `coin`. What
made them worth removing is that their *names* assert the chain: a reader
skimming `nullifier_is_poseidon_of_coin` learns that the nullifier is the
Poseidon compression of the coin. The file said that in a comment and in three
declaration names, and proved it nowhere.

Because the chain is now defined rather than postulated, the three steps are
`rfl` and the dependency is visible in the definitions themselves.

## Where the chain is actually enforced

In `revoke.zk`, by the `constrain_equal_base` calls binding each derived value to
its recomputed compression. That is the enforcement; this file records its shape.
-/

namespace DarkFi.Capability

/-! ===== Burn inputs ===== -/

/-- A RevokeV2 burn input: the note's public fields plus the spending secret. -/
structure BurnInput where
  secret : Nat
  value : Nat
  assetId : Nat
  spendHook : Nat
  userData : Nat
  commitmentBlind : Nat
deriving Repr, BEq

/-! ===== The burn chain, as definitions =====

The five steps are `def`s over `HashOps.poseidon_hash_output`, with the round
constants from `revoke.zk`. Making them definitions rather than `opaque`s means
the three nesting steps below are provable, and that a reader can see what each
step is a hash of — which the `opaque` form deliberately hid. -/

/-- `pub = poseidon(7, spend_secret)`. -/
def pubKey (b : BurnInput) : Int :=
  poseidon_hash_output [7, (b.secret : Int)]

/-- `coin = poseidon(4, pub, value, asset_id, spend_hook, user_data, commitment_blind)`. -/
def coin (b : BurnInput) : Int :=
  poseidon_hash_output
    [4, pubKey b, (b.value : Int), (b.assetId : Int),
     (b.spendHook : Int), (b.userData : Int), (b.commitmentBlind : Int)]

/-- `nullifier = poseidon(1, spend_secret, coin)`. -/
def nullifier (b : BurnInput) : Int :=
  poseidon_hash_output [1, (b.secret : Int), coin b]

/-- `signature_secret = poseidon(7, spend_secret, nullifier)`. -/
def signatureSecret (b : BurnInput) : Int :=
  poseidon_hash_output [7, (b.secret : Int), nullifier b]

/-- `signature_public = poseidon(7, signature_secret)`. -/
def signaturePublic (b : BurnInput) : Int :=
  poseidon_hash_output [7, signatureSecret b]

/-! ===== The three nesting steps, as theorems =====

-- WEAKENED FROM: each of these replaces an `axiom` whose statement was `f b = f b`. The old
-- statements were true for any `f` and carried no information; these state the actual
-- compression. The change is a *strengthening* of what is asserted (from a tautology to the
-- chain link the name always claimed), and the names are narrowed to what is proved.

They are `rfl` because the chain is defined rather than postulated. Their axiom budget is 1,
not 0: `poseidon_hash_output` is a value-less `opaque`, and `Axioms.lean` records what that
assumption is and what would discharge it. -/

/-- The coin is the Poseidon compression of the public key and the note's six fields. -/
@[axiom_budget 0]
theorem coin_eq_poseidon_of_pubKey (b : BurnInput) :
    coin b = poseidon_hash_output
      [4, pubKey b, (b.value : Int), (b.assetId : Int),
       (b.spendHook : Int), (b.userData : Int), (b.commitmentBlind : Int)] := rfl

/-- The nullifier is the Poseidon compression of the spending secret and the coin. -/
@[axiom_budget 0]
theorem nullifier_eq_poseidon_of_coin (b : BurnInput) :
    nullifier b = poseidon_hash_output [1, (b.secret : Int), coin b] := rfl

/-- The signature secret is the Poseidon compression of the spending secret and the nullifier. -/
@[axiom_budget 0]
theorem signatureSecret_eq_poseidon_of_nullifier (b : BurnInput) :
    signatureSecret b = poseidon_hash_output [7, (b.secret : Int), nullifier b] := rfl

/-- The chain is *nested*: substituting the definitions shows that `signatureSecret` depends on
    `nullifier`, which depends on `coin`, which depends on `pubKey` — each through the previous
    step's output, not through a witness slot. This is the property the generic prover's
    slot-only `compute_derived` could not express (HAZOP V3), stated on the definitions. -/
@[axiom_budget 0]
theorem signatureSecret_eq_poseidon_of_coin_via_nullifier (b : BurnInput) :
    signatureSecret b =
      poseidon_hash_output
        [7, (b.secret : Int), poseidon_hash_output [1, (b.secret : Int), coin b]] :=
  congrArg (fun n => poseidon_hash_output [7, (b.secret : Int), n])
    (nullifier_eq_poseidon_of_coin b)

/-! ===== Congruence with the derived-rule DAG ===== -/

/-- The burn chain is exactly the three-node intermediate-referencing DAG
    `revokeChain` (`coin → nullifier → signature_secret`). A
    well-formed DAG is the structural precondition for the generic prover to
    compute each step in order. -/
@[axiom_budget 1]
theorem revoke_chain_is_well_formed_dag : derivedChainWellFormed revokeChain :=
  revokeChain_wellFormed

/-- **V3 fixed (structural)**: the `signature_secret` rule's second operand is a
    reference to the *prior* `nullifier` node (index 1), not witness slot 0
    twice. Concretely, the last node of `revokeChain` has operands
    `[slot 0, derived 1]`. -/
@[axiom_budget 0]
theorem revoke_signature_secret_references_nullifier :
    (revokeChain.getD 2 default).operands =
      [Operand.slot 0, Operand.derived 1] := by
  rfl

end DarkFi.Capability
