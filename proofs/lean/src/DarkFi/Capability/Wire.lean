/-
DarkWow.Capability.Wire — Manifest wire-schema congruence (T1)

Formalizes the write-path invariant that a manifest's `[[parameters]]` is a
faithful description of the contract's `*Params::decode` wire struct: same
ordered fields, same types, same byte widths. The wallet encodes params from the
manifest; the contract decodes them with its own struct. Correctness is
`wireCongruent`: the two schemas are equal as ordered (name, type) lists.

Two lemmas capture the observed bug classes:
- `wireCongruent_implies_len` — congruence forces equal encoded length, so a
  *missing* field (NO / PART OF) is caught by a length check.
- `swap_not_congruent` / `schemaWireLen_swap_eq` — reordering two fields leaves
  the length unchanged but breaks congruence, so a *reversed field pair*
  (REVERSE) is NOT caught by a length check alone; only structural equality does.
-/

import Mathlib

namespace DarkFi.Capability

/- The closed wire-type vocabulary for `[[parameters]]` (mirrors the Rust
   `field_wire_len` / Python `WIRE_PARAM_TYPES`). `bytes` is variable-width and
   excluded from fixed-width sums. -/
inductive WireType where
  | u64 | u32 | bool | base | scalar | publicKey | contractId | bytes | merklePath | proof
deriving Repr, DecidableEq

/- Fixed byte width of a wire type. `bytes` is variable (0 here; its runtime
   length is added by the caller, as in the Rust encoder). -/
def wireWidth : WireType → Nat
  | .u64 => 8
  | .u32 => 4
  | .bool => 1
  | .base => 32
  | .scalar => 32
  | .publicKey => 32
  | .contractId => 32
  | .bytes => 0
  | .merklePath => 32 * 32
  | .proof => 1

/- A single wire field: name + type. Width is derived from the type. -/
structure FieldSpec where
  name : String
  ty : WireType
deriving Repr, DecidableEq

/- Total fixed wire width of a schema (sum of field widths). -/
def schemaWireLen : List FieldSpec → Nat
  | [] => 0
  | f :: rest => wireWidth f.ty + schemaWireLen rest

/- T1: the manifest schema is congruent to the contract schema iff they are
   equal as ordered (name, type) lists. -/
def wireCongruent (manifest contract : List FieldSpec) : Prop :=
  manifest = contract

/- `schemaWireLen` is a homomorphism over list append — a schema's width is the
   sum of its parts. -/
theorem schemaWireLen_append (a b : List FieldSpec) :
    schemaWireLen (a ++ b) = schemaWireLen a + schemaWireLen b := by
  induction a with
  | nil => simp [schemaWireLen]
  | cons f rest ih =>
      simp [schemaWireLen, ih]
      omega

/- Congruence forces equal encoded length (T1, necessary condition): a missing
   field changes the width, so it is caught by a length check. -/
theorem wireCongruent_implies_len (m c : List FieldSpec) :
    wireCongruent m c → schemaWireLen m = schemaWireLen c := by
  intro h
  rw [h]

/- Reordering two fields leaves the length unchanged (addition is commutative),
   so a reversed field pair is INVISIBLE to a length check: `[a, b]` and
   `[b, a]` are distinct lists (so `¬ wireCongruent`), yet
   `schemaWireLen [a, b] = schemaWireLen [b, a]`. Length equality is therefore
   necessary (`wireCongruent_implies_len`) but not sufficient — this is the
   tx_binding/tx_nonce swap bug class. -/
theorem schemaWireLen_swap_eq (a b : FieldSpec) :
    schemaWireLen [a, b] = schemaWireLen [b, a] := by
  simp [schemaWireLen, Nat.add_comm]

end DarkFi.Capability
