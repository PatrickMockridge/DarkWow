/-
DarkWow.Net.Receive — Decrypt Soundness (↓discover)

Formalizes the wallet receive invariant from wallet.md §2.1 and
transfer-spec.md: a note decrypts to a capability ONLY when the trial key is
the note's recipient key. This is what makes a wrong-key wallet discover zero
capabilities — the soundness of the ↓discover barb — and it is the invariant
the Docker transfer-receive path must preserve.
-/

import Mathlib
import DarkFi.Capability.Types

namespace DarkFi.Net

open DarkFi.Capability.Types

/- A recipient key and a note. The note carries the key it was encrypted to. -/
structure Note where
  recipient : Nat
  payload : Nat
deriving Repr

/- A capability discovered from a decrypted note (opaque here). -/
structure Capability where
  value : Nat
deriving Repr

/- decrypt: trial decryption with a candidate key. Returns a capability only
   when the candidate key equals the note's recipient key. -/
def decrypt (k : Nat) (n : Note) : Option Capability :=
  if k = n.recipient then some { value := n.payload } else none

/- Theorem (decrypt_sound): a note decrypts to a capability only for the
   recipient key. A wrong key discovers nothing — the ↓discover barb is sound. -/
theorem decrypt_sound (k : Nat) (n : Note) :
    (∃ c, decrypt k n = some c) → k = n.recipient := by
  intro h
  rcases h with ⟨c, hc⟩
  unfold decrypt at hc
  split at hc
  · next k_eq => exact k_eq
  · next h_ne => contradiction

/- Corollary: a wrong key never produces a capability (the negative case). -/
theorem decrypt_wrong_key_none (k : Nat) (n : Note) :
    k ≠ n.recipient → decrypt k n = none := by
  intro h_ne
  unfold decrypt
  simp [h_ne]

end DarkFi.Net
