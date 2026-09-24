/-
DarkWow.Net.Receive — Decrypt Soundness (↓discover)

Formalizes the wallet receive invariant from wallet.md §2.1 and transfer-spec.md: a note
decrypts to a capability ONLY when the trial key is the note's recipient key. This is what
makes a wrong-key wallet discover zero capabilities — the soundness of the ↓discover barb —
and it is the invariant the Docker transfer-receive path must preserve.

## What this file was, and what it is now

Until 2026-09-24 `decrypt` **was** the invariant rather than a function obeying it:

    def decrypt (k : Nat) (n : Note) : Option Capability :=
      if k = n.recipient then some { value := n.payload } else none

so `decrypt_sound` proved that a branch whose condition is `k = n.recipient` returns `some`
only when `k = n.recipient`. That is true of *every* possible notion of decryption and says
nothing about a ciphertext, yet `doc/src/arch/wallet.md` §2.1 cited it as "Decrypt soundness
(↓discover) … a note decrypts to a capability only when the trial key is the note's recipient
key" — i.e. as evidence about the deployed receive path. It was not: the key comparison was the
definition, so the theorem was true of the definition.

`decrypt` now **opens an opaque ciphertext** (`Axioms.aead_open`) and never compares keys. The
key equality `decrypt_sound` concludes comes from `Axioms.aead_key_committing` — the AEAD
key-committing property — which is *assumed*, and which therefore appears in the theorem's
`@[axiom_budget 1]` instead of being built into a branch.

## What the model still does not have

Bytes; the Sapling DH agreement (`sapling_ka_agree`); the KDF (`kdf_sapling`); the nonce
derivation (`AeadEncryptedNote::derive_nonce`, the M7 fix); and the plaintext *decoding* —
`aead_open` returns an `Int` where the deployment returns `encode(note)` and then runs
`D::decode`. Those are the gaps `Axioms.aead_key_committing` names, not gaps this file hides.

`recipient` is deliberately **not** something the ciphertext carries. In the deployment the key
is `kdf_sapling(sapling_ka_agree(recipient_secret, ephem_public), ephem_public)`
(`src/sdk/src/crypto/note.rs:113-135`): the note carries only `ephem_public`, and only a holder
of the matching secret can derive the key that authenticates it. It is a field here so that the
sealing fact can be a hypothesis rather than something the model asserts away.
-/

import Mathlib
import DarkFi.Axioms
import DarkFi.AxiomBudget

namespace DarkFi.Net

open DarkFi.Capability.Types

/- A note as the receive path sees it: an opaque ciphertext, the AEAD key it was sealed to, and
   the plaintext that key recovers. -/
structure Note where
  ciphertext : Int
  recipient : Int
  payload : Int
deriving Repr

/- A capability discovered from a decrypted note (opaque here). -/
structure Capability where
  value : Int
deriving Repr

/- decrypt: trial opening of the note's ciphertext under `k` — the AEAD key derived from a
   candidate `SecretKey`. Returns a capability carrying the recovered plaintext, or `none` when
   the ciphertext does not authenticate under `k`. Note what is absent: no comparison of `k`
   against `n.recipient` anywhere in this definition. -/
def decrypt (k : Int) (n : Note) : Option Capability :=
  match aead_open n.ciphertext k with
  | some v => some { value := v }
  | none   => none

/- Theorem (decrypt_sound): a note decrypts to a capability only under the key it was sealed to.
   A wrong key discovers nothing — the ↓discover barb is sound.

   **The content is `Axioms.aead_key_committing`**, not this proof: the branch structure gives
   the *recovered plaintext*, and the key equality comes from the assumption. Budget 1 is that
   dependency, and it is the honest reading — before this change the same theorem was budget 0
   because it needed nothing, having assumed everything in its definition. -/
@[axiom_budget 1]
theorem decrypt_sound (k : Int) (n : Note)
    (h_sealed : aead_open n.ciphertext n.recipient = some n.payload) :
    (∃ c, decrypt k n = some c) → k = n.recipient := by
  rintro ⟨c, hc⟩
  unfold decrypt at hc
  cases h_ao : aead_open n.ciphertext k with
  | none => rw [h_ao] at hc; exact absurd hc (by simp)
  | some v => exact aead_key_committing n.ciphertext k n.recipient v n.payload h_ao h_sealed

/- Corollary: a wrong key never produces a capability (the negative case) — the form in which
   the invariant is stated about the wallet's trial-decryption loop. -/
@[axiom_budget 1]
theorem decrypt_wrong_key_none (k : Int) (n : Note)
    (h_sealed : aead_open n.ciphertext n.recipient = some n.payload)
    (h_ne : k ≠ n.recipient) : decrypt k n = none := by
  cases h_ao : aead_open n.ciphertext k with
  | none => simp [decrypt, h_ao]
  | some v =>
      exact absurd (aead_key_committing n.ciphertext k n.recipient v n.payload h_ao h_sealed) h_ne

/- ==========================================================================
   Non-vacuity: the assumption's shape is falsifiable
   ==========================================================================
   Two questions a reader should ask of an assumption, and what this file can answer:

   1. **Is the shape a tautology?** No — `key_committing_is_false_for_leaky_open` below exhibits
      an opening function for which the *same statement* is false, so
      `Axioms.aead_key_committing` is load-bearing: dropping it from `decrypt_sound` would leave
      that theorem unprovable rather than trivially true.

   2. **Is it satisfiable?** A tag-based AEAD of the deployment's shape has this property by
      construction — authentication is a function of the key, so two keys authenticating one
      ciphertext would be a tag collision. **No such model is built here**, so nothing in this
      tree exhibits a satisfiable instance of `aead_open`. That is the residual, and it is why
      the property sits in the budget rather than in a proof: this file shows the assumption is
      not vacuous in shape, and `Axioms.aead_key_committing`'s own entry records what would
      discharge it. The unproved half is the *authentication*, not the plaintext recovery —
      `AeadEncryptedNote::decrypt`'s `ChaCha20Poly1305::decrypt_in_place` returning `Err` under a
      wrong key is the deployment's form of exactly this statement.
   ========================================================================== -/

/-- An opening that ignores the key: every key "authenticates" every ciphertext and the plaintext
    is the ciphertext shifted by the key. This is what an AEAD without a tag check looks like. -/
def leaky_open (ciphertext key : Int) : Option Int := some (ciphertext + key)

/-- The shape of `Axioms.aead_key_committing` is **not** a tautology: for `leaky_open` two
    distinct keys open the same ciphertext, so the statement is false. The assumption is what
    rules this out, and this refutation is the falsification the budget does not show. -/
@[axiom_budget 0]
theorem key_committing_is_false_for_leaky_open :
    ¬ (∀ (c k₁ k₂ p₁ p₂ : Int),
        leaky_open c k₁ = some p₁ → leaky_open c k₂ = some p₂ → k₁ = k₂) := by
  intro h
  have h01 : (0 : Int) = 1 := h 0 0 1 0 1 (by simp [leaky_open]) (by simp [leaky_open])
  norm_num at h01

end DarkFi.Net
