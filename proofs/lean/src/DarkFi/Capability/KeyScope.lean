/-
# Key scope — `derive_instance` and type-system.md §7.3

`type-system.md` §7 lists seven "compiler-enforced invariants", and the third is:

> **Scope restriction.** No restricted name shall cross its declared scope boundary. A `SecretKey`
> derived for contract instance `A` SHALL NOT be usable in contract instance `B`.

Until 2026-09-24 the whole Lean tree contained **no model of it**: the only occurrence of
`derive_instance` anywhere in `proofs/lean/` was a comment in `Capability/Composition.lean`. This
module is that model, and the invariant is `scopeRestriction` below.

## What is modelled

`SecretKey::derive_instance` (`src/sdk/src/crypto/keypair.rs:202-222`) is

    poseidon_hash([DRK_POSEIDON_DOMAIN_KEY_DERIVE, secret, contract_id, instance_elem])

and `DRK_POSEIDON_DOMAIN_KEY_DERIVE` is `from_raw([8, 0, 0, 0])`
(`src/sdk/src/crypto/constants.rs:62`), so the four-element input is
`[8, secret, contract_id, instance_elem]`. `deriveInstance` is that, over the tree's opaque
`poseidon_hash_output`.

The invariant follows from `Axioms.poseidon_collision_resistance` — the same assumption
`Capability/Purse.lean` consumes at budget 1 — because injectivity makes the *input list*
recoverable from the output, and the list carries the contract and the instance in fixed
positions. So a name derived for one scope cannot be a name derived for another, which is what
makes cycled addresses possible rather than merely preferable: `privacy-model.md:59-60`'s "a
stable/master identity SHALL NOT appear in any block or transaction" is a rule the derivation
mechanism enforces, not a convention callers are asked to keep.

## What is not modelled, stated rather than implied

* **The instance-element encoding.** The Rust zero-pads or truncates `instance_id` to 32 bytes
  (`:209-210`) and then rejects a non-canonical field element (`pallas::Base::from_repr`)
  (`:211-218`). Here the instance is an `Int` already. So the model says nothing about which byte
  strings are admissible instances — only that distinct admissible instances give distinct keys.
* **The primitive.** `poseidon_hash_output` is opaque, so this is the injectivity assumption
  applied to a four-element list, and it inherits that assumption's honest scope (see
  `Axioms.lean` and `README.md`): it is not a statement about the deployed sponge.
* **Curve-point-ness.** The Rust's result is a `pallas::Base` that the caller treats as a secret
  scalar; nothing here models the field or the group, so "the derived value is a valid scalar" is
  not claimed.

That this module did not exist until 2026-09-24 — an invariant `type-system.md` §7 calls
compiler-enforced, carrying the privacy model's central mechanism, with no model anywhere in this
tree — is recorded as `HIGH-17` in `DarkFi.HAZOP.High`.
-/

import Mathlib
import DarkFi.Axioms
import DarkFi.AxiomBudget

namespace DarkFi.Capability

open HashOps

/-- `DRK_POSEIDON_DOMAIN_KEY_DERIVE` (`src/sdk/src/crypto/constants.rs:62`, `from_raw([8,0,0,0])`).
    The domain that separates key derivation from every other Poseidon use in the tree. -/
def KEY_DERIVE_DOMAIN : Int := 8

/-- Key derivation scoped to a contract and an instance —
    `poseidon_hash([KEY_DERIVE_DOMAIN, secret, contractId, instanceElem])`, mirroring
    `SecretKey::derive_instance` (`src/sdk/src/crypto/keypair.rs:220`).

    This is the mechanism behind address cycling: a per-block or per-transaction key is
    `deriveInstance secret contractId <height-or-instance>`, so no single value is reused across
    scopes. -/
def deriveInstance (secret contractId instanceElem : Int) : Int :=
  poseidon_hash_output [KEY_DERIVE_DOMAIN, secret, contractId, instanceElem]

/-- **type-system.md §7.3 — scope restriction.** Two derivations from the same secret are equal
    only if they were made for the *same* contract and the *same* instance. So no name derived for
    one scope equals a name derived for another; the scope boundary cannot be crossed without
    finding a Poseidon collision over four-element lists.

    Budget 1 is `Axioms.poseidon_collision_resistance`: injectivity recovers the input list from
    the output, and the list's second and third positions are the contract and the instance. -/
@[axiom_budget 1]
theorem scopeRestriction (s c₁ i₁ c₂ i₂ : Int)
    (h : deriveInstance s c₁ i₁ = deriveInstance s c₂ i₂) : c₁ = c₂ ∧ i₁ = i₂ := by
  unfold deriveInstance at h
  have hlist : ([KEY_DERIVE_DOMAIN, s, c₁, i₁] : List Int) = [KEY_DERIVE_DOMAIN, s, c₂, i₂] := by
    by_contra hne
    exact absurd h (poseidon_collision_resistance _ _ hne)
  simpa using hlist

/-- The contrapositive the wallet actually uses, contract side: a key derived for contract `A`
    under a secret is not the key derived for contract `B` under the same secret. This is what
    makes a per-contract key materially different from the master secret rather than a convention
    that happens to be followed. -/
@[axiom_budget 1]
theorem derivedKeysDifferAcrossContracts (s c₁ c₂ i : Int) (h : c₁ ≠ c₂) :
    deriveInstance s c₁ i ≠ deriveInstance s c₂ i := by
  intro heq
  exact h (scopeRestriction s c₁ i c₂ i heq).1

/-- And the instance side, which is the per-block case: `MiningRecipient::from_account` derives
    `sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H)`
    (`crates/dwow-accounts/src/lib.rs:1265`), one key per block height `H`. Two heights give two
    keys, so a wallet's per-block secret is not a longer-lived name in disguise. -/
@[axiom_budget 1]
theorem derivedKeysDifferAcrossInstances (s c i₁ i₂ : Int) (h : i₁ ≠ i₂) :
    deriveInstance s c i₁ ≠ deriveInstance s c i₂ := by
  intro heq
  exact h (scopeRestriction s c i₁ c i₂ heq).2

/-- The invariant is **instantiable**, on both sides, at scopes concretely chosen. This is not a
    consequence of the `∀` above in any cheap sense: `poseidon_hash_output` is opaque, so with the
    assumption removed these two disequalities are not provable at all — which is what makes
    `poseidon_collision_resistance` load-bearing here rather than decorative. -/
@[axiom_budget 1]
theorem derivedKeys_differ_at_concrete_scopes :
    deriveInstance 0 1 0 ≠ deriveInstance 0 2 0 ∧
      deriveInstance 0 1 0 ≠ deriveInstance 0 1 2 := by
  exact ⟨derivedKeysDifferAcrossContracts 0 1 2 0 (by norm_num),
    derivedKeysDifferAcrossInstances 0 1 0 2 (by norm_num)⟩

/-! ===== Non-vacuity: the invariant is falsifiable =====

    §7.3 is a claim about *this* derivation, not about derivation in general, and the difference
    is what the refutation below measures: a derivation that ignores its scope satisfies every
    other clause of "a derived key" and breaks exactly this one. So the theorem above is not true
    of every function of the same shape, and the assumption it rests on is load-bearing. -/

/-- A derivation that ignores the scope and returns the master secret unchanged — what using the
    master identity directly is. -/
def unscopedDerive (secret _contractId _instanceElem : Int) : Int := secret

/-- `scopeRestriction`'s shape is **not** a tautology: `unscopedDerive` makes it false, with two
    distinct contracts deriving the same key from one secret. `privacy-model.md:59-60` forbids
    exactly this shape, and this is that prohibition stated as a refutation rather than as
    advice. -/
@[axiom_budget 0]
theorem scopeRestriction_is_false_for_unscopedDerive :
    ¬ (∀ (s c₁ c₂ i₁ i₂ : Int),
        unscopedDerive s c₁ i₁ = unscopedDerive s c₂ i₂ → c₁ = c₂ ∧ i₁ = i₂) := by
  intro h
  exact absurd (h 0 1 2 0 0 rfl).1 (by norm_num)

end DarkFi.Capability
