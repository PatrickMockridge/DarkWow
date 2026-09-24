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

/- ==========================================================================
   §6.4.0's spending-key persistence — a measured *non*-theorem, and why
   ==========================================================================
   `wallet.md`:745-748 states it in two halves:

   > A received `TransferV1`/`SpendV1` output carries a **fresh** `spend_secret` that is not derivable
   > from any account. The wallet SHALL persist that `spend_secret` with the capability so the spend path
   > can recover the commitment's spending key. `key_coords` (master/per-instance re-derivation) SHALL
   > NOT be used to recover a fresh `spend_secret`.

   The verification plan for this layer expected a theorem here — "prove that the scope-derived key is
   not the `spend_secret`, an inequality the `unscopedDerive` falsifier already establishes" — and
   **that theorem does not exist**, for a reason worth writing down rather than discovering twice.

   * **The inequality is not available from this layer's assumptions.** `deriveInstance s c i ≠ s` says
     no secret is a fixed point of the scoped derivation, and collision resistance does not give it: a
     hash *may* have fixed points, and `s = H([8, s, c, i])` is not a collision between two distinct
     inputs. Proving it would need an assumption this tree does not carry and should not add.
   * **Without that, the obligation is its own premise.** Taking the specification's freshness clause as
     a hypothesis and concluding "so the coordinate route cannot return it" is the hypothesis repackaged
     through a `resolve` definition — the definitional-restatement class this register removes by name
     (`wallet_construct_sound`, `walletConstruct_idempotent`). A theorem that unfolds a two-branch `def`
     and hands back its own binder is not evidence, whatever it is called.
   * **What the obligation actually constrains is a *call*, not a value** — which route the spend path
     takes — and that is a property of the code, checkable by reading it, not of the arithmetic.

   **So it was measured instead, 2026-09-24.** `bin/dww` persists the secret: `spend_secret` is a
   nullable column on held capabilities (`walletdb.rs:105-109`, written at `:963`). And the one
   `key_coords` fallback in the spend path is scoped by construction —
   `fee_builder.rs:147-166` resolves via `account_mgr.resolve_key(cap.key_coords)` only where its own
   comment says the value is *self-issued* and "derivable via `resolve_key`", i.e. for the wallet's own
   coinbase and fee capabilities rather than for a received output. **The obligation is met, and met by
   the distinction the specification draws rather than by the arithmetic this module models.** That is
   recorded as `OBL-T21`; nothing is proved here, and this note is the reason a reader will not look for
   a theorem that cannot be stated honestly.
   ========================================================================== -/

end DarkFi.Capability
