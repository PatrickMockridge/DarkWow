import DarkFi.Combinatorial.StateSpace

/-!
# Nullifier Storage Faithfulness — the Representation Faithfulness Law

Formal statement of the invariant from type-system.md §0.1 and
contract-wasm-standards-best-practices.md §9.4:

> A decidable, monotone observation (a barb) is faithfully encoded into a
> store iff its witness is a **distinguished element** — a value disjoint from
> the canonical "absent" element of the representation's carrier.

This module models the contract KV store (src/sdk/src/wasm/db.rs):

  db_mark_spent(key)  := db_set(key, &[1])   -- non-empty marker (faithful)
  db_set(key, &[])                            -- empty marker (the defect)
  db_contains_key treats an empty value as ABSENT (empty-value-as-absent).

The law, mechanized: the mark operation must be a *section* of the recover
(read) map — writing a marker value v makes key n "spent" iff v is non-empty.

This is the concrete data model for `PublicState.spentNullifiers`. It uses
core Lean only (List/Option/Nat/Prop/Bool) — no Mathlib, no `Finset` — so the
"spent set" is modeled as the predicate `NullifierValue → Prop`.
-/

namespace Combinatorial

/-! ==========================================================================
   The store model
   ========================================================================== -/

/-- A stored value: a byte string (List of byte Nat values). -/
abbrev Value := List Nat

/-- The empty value ε — the "absent" sentinel. -/
def ε : Value := []

/-- A value is "present" (non-empty). -/
def Value.present (v : Value) : Prop := v ≠ ε

/-- The nullifier KV store: a partial map nullifier ↦ optional value. -/
abbrev Store := NullifierValue → Option Value

/-- `recover`: read back the set of "spent" nullifiers
    = { n | store holds n with a non-empty value }, as a predicate. -/
def recover (s : Store) : NullifierValue → Prop :=
  fun n => ∃ v : Value, s n = some v ∧ Value.present v

/-- `containsKey`: the boolean read `db_contains_key`; empty value = absent. -/
def containsKey (s : Store) (n : NullifierValue) : Bool :=
  match s n with
  | some v => if v = ε then false else true
  | none   => false

/-- `mark`: write value v at key n (overwrite). -/
def mark (s : Store) (n : NullifierValue) (v : Value) : Store :=
  fun m => if m = n then some v else s m

/-- `markSpent`: write the faithful `[1]` marker (`db_mark_spent`). -/
def markSpent (s : Store) (n : NullifierValue) : Store := mark s n [1]

/-- `markEmpty`: write the empty `[]` marker (the defect). -/
def markEmpty (s : Store) (n : NullifierValue) : Store := mark s n ε

/-- Set combinators at the predicate level (no Mathlib Set/Finset). -/
def spentUnion (A B : NullifierValue → Prop) : NullifierValue → Prop :=
  fun n => A n ∨ B n

def spentSingleton (n : NullifierValue) : NullifierValue → Prop :=
  fun m => m = n

/-! ==========================================================================
   Basic lemmas
   ========================================================================== -/

theorem mark_self (s : Store) (n : NullifierValue) (v : Value) : mark s n v n = some v := by
  unfold mark; simp

theorem mark_other (s : Store) (n m : NullifierValue) (v : Value) (h : m ≠ n) :
    mark s n v m = s m := by
  unfold mark; rw [if_neg h]

/-- Marking key n with v makes n "spent" iff v is non-empty. -/
theorem recover_mark_self (s : Store) (n : NullifierValue) (v : Value) :
    recover (mark s n v) n ↔ v ≠ ε := by
  unfold recover mark
  constructor
  · intro h
    rcases h with ⟨w, hw, hwne⟩
    have hvw : v = w := by simpa using hw
    subst w
    exact hwne
  · intro hvne
    exact ⟨v, by simp, hvne⟩

/-- Marking key n does not affect other keys m ≠ n. -/
theorem recover_mark_other (s : Store) (n m : NullifierValue) (v : Value) (h : m ≠ n) :
    recover (mark s n v) m ↔ recover s m := by
  unfold recover mark
  rw [if_neg h]

/-- The honest overwrite law: after marking with v, key m is present iff
    (m = n and v non-empty) or (m ≠ n and it was already present). -/
theorem recover_mark (s : Store) (n m : NullifierValue) (v : Value) :
    recover (mark s n v) m ↔ ((m = n ∧ v ≠ ε) ∨ (m ≠ n ∧ recover s m)) := by
  by_cases h : m = n
  · subst m
    rw [recover_mark_self]
    constructor
    · intro hvne; exact Or.inl ⟨rfl, hvne⟩
    · intro hrec
      rcases hrec with ⟨_, hvne⟩ | ⟨hnn, _⟩
      · exact hvne
      · exact (hnn rfl).elim
  · rw [recover_mark_other s n m v h]
    constructor
    · intro hrec; exact Or.inr ⟨h, hrec⟩
    · intro hrec
      rcases hrec with ⟨hnn, _⟩ | ⟨_, hrec⟩
      · exact (h hnn).elim
      · exact hrec

/-! ==========================================================================
   T1. Faithful marking
   ========================================================================== -/

theorem markSpent_faithful (s : Store) (n m : NullifierValue) :
    recover (markSpent s n) m ↔ (recover s m ∨ m = n) := by
  unfold recover markSpent
  by_cases h : m = n
  · subst m
    constructor
    · intro _; exact Or.inr rfl
    · intro _; exact ⟨[1], by simp [mark], by simp [Value.present, ε]⟩
  · have hm : mark s n [1] m = s m := by simp [mark, h]
    rw [hm]
    constructor
    · intro hrec; exact Or.inl hrec
    · intro hrec
      rcases hrec with hl | hr
      · exact hl
      · exact (h hr).elim

/-- T1 in extensional set-form (uses funext + propext). -/
theorem markSpent_faithful_set (s : Store) (n : NullifierValue) :
    recover (markSpent s n) = spentUnion (recover s) (spentSingleton n) := by
  funext m
  exact propext (markSpent_faithful s n m)

/-! ==========================================================================
   T2. Empty marker (the defect)
   ========================================================================== -/

/-- The empty marker NEVER makes n "spent": replay protection silently bypassed. -/
theorem markEmpty_not_spent (s : Store) (n : NullifierValue) :
    ¬ recover (markEmpty s n) n := by
  intro h
  exact ((recover_mark_self s n ε).mp h) rfl

/-- The empty marker never ADDS to the spent set (it can only leave a key absent
    or erase a previously-spent key). -/
theorem markEmpty_never_adds (s : Store) (n m : NullifierValue) :
    recover (markEmpty s n) m → recover s m := by
  unfold markEmpty
  by_cases h : m = n
  · subst m
    intro hrec
    have h' : ε ≠ ε := (recover_mark_self s n ε).mp hrec
    exact (h' rfl).elim
  · rw [recover_mark_other s n m ε h]
    intro hrec; exact hrec

/-- The literal `recover (markEmpty s n) = recover s` is FALSE: the empty marker
    erases a previously-spent key. Concrete witness: a store where n is spent. -/
def spentAt (n : NullifierValue) : Store := markSpent (fun _ => none) n

theorem markEmpty_not_identity (n : NullifierValue) :
    recover (markEmpty (spentAt n) n) ≠ recover (spentAt n) := by
  intro h
  have hn : recover (markEmpty (spentAt n) n) n = recover (spentAt n) n := congrFun h n
  have hspent : recover (spentAt n) n := by
    unfold spentAt markSpent mark recover Value.present ε
    exact ⟨[1], by simp, by simp⟩
  have hnot : ¬ recover (markEmpty (spentAt n) n) n := markEmpty_not_spent (spentAt n) n
  have hleft : recover (markEmpty (spentAt n) n) n := by rw [hn]; exact hspent
  exact hnot hleft

/-! ==========================================================================
   T3. Soundness / decidability
   ========================================================================== -/

theorem markSpent_sound (s : Store) (n : NullifierValue) :
    recover (markSpent s n) n :=
  (markSpent_faithful s n n).mpr (Or.inr rfl)

/-- Boolean reflection of T3: `db_contains_key (db_mark_spent s n) n = true`. -/
theorem markSpent_containsKey (s : Store) (n : NullifierValue) :
    containsKey (markSpent s n) n = true := by
  unfold containsKey markSpent
  simp [mark, ε]

/-! ==========================================================================
   T4. Monotonicity
   ========================================================================== -/

theorem markSpent_monotone (s : Store) (n m : NullifierValue) :
    recover s m → recover (markSpent s n) m := by
  intro h
  exact (markSpent_faithful s n m).mpr (Or.inl h)

/-! ==========================================================================
   T5. Idempotence / replay rejection
   ========================================================================== -/

theorem markSpent_idempotent (s : Store) (n m : NullifierValue) (h : recover s n) :
    recover (markSpent s n) m ↔ recover s m := by
  rw [markSpent_faithful s n m]
  constructor
  · intro hm
    rcases hm with hl | hr
    · exact hl
    · subst m; exact h
  · intro hl; exact Or.inl hl

/-! ==========================================================================
   T6. General faithfulness iff the marker is non-empty
   ========================================================================== -/

/-- `Faithful v`: marking with v always yields `recover s ∪ {n}` (pointwise). -/
def Faithful (v : Value) : Prop :=
  ∀ (s : Store) (n m : NullifierValue),
    recover (mark s n v) m ↔ (recover s m ∨ m = n)

/-- The marker v faithfully encodes "spent" iff v is non-empty. -/
theorem faithful_iff_nonempty (v : Value) : Faithful v ↔ v ≠ ε := by
  unfold Faithful
  constructor
  · intro h
    intro hv
    have hmpr := (h (fun _ => none) 0 0).mpr (Or.inr rfl)
    exact (recover_mark_self (fun _ => none) 0 v).mp hmpr hv
  · intro hv
    intro s n m
    rw [recover_mark s n m v]
    by_cases hmn : m = n
    · subst m
      constructor
      · intro _; exact Or.inr rfl
      · intro _; exact Or.inl ⟨rfl, hv⟩
    · constructor
      · intro hrec
        rcases hrec with ⟨hmn_eq, _⟩ | ⟨_, hrec⟩
        · exact (hmn hmn_eq).elim
        · exact Or.inl hrec
      · intro hrec
        rcases hrec with hrec | hm_eq
        · exact Or.inr ⟨hmn, hrec⟩
        · exact (hmn hm_eq).elim

end Combinatorial
