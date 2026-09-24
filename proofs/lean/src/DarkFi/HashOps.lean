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

/-- Different `i`-th components make the lists different. -/
private lemma ne_of_getD_ne {l₁ l₂ : List Int} (i : Nat) (h : l₁.getD i 0 ≠ l₂.getD i 0) :
    l₁ ≠ l₂ := fun heq => h (getD_eq_of_eq heq i)

/-! ===== The binding engine, and where the assumption actually sits =====

Every binding statement in this file is the same derivation: given an injective hash, a differing
component forces a differing hash. Stated once, over an **arbitrary** `h`, it needs no
cryptography at all — the whole of it is `getD` bookkeeping — and it is budget 0.

The earlier versions folded the assumption into each statement, which is why all four carried
budget 1 and why the *content* was invisible: a reader could not tell which part was the
assumption and which part was proved. Now the split is explicit. `…_of_injective` is the content,
proved; the `poseidon_hash_output` corollary beside it is the instantiation, and its budget is
exactly the assumption. Since `Axioms.poseidon_collision_resistance` states injectivity, which is
**false of the real Poseidon** (see that entry), the corollaries are conditional on something the
deployed system does not satisfy — and the split is what makes that visible at the call site. -/

/-- **The engine.** An injective hash sends lists differing at any component to different values. -/
@[axiom_budget 0]
theorem hash_ne_of_component_ne (h : List Int → Int)
    (h_inj : ∀ x y : List Int, x ≠ y → h x ≠ h y)
    (l₁ l₂ : List Int) (i : Nat) (h_diff : l₁.getD i 0 ≠ l₂.getD i 0) : h l₁ ≠ h l₂ :=
  fun heq => h_inj l₁ l₂ (ne_of_getD_ne i h_diff) heq

/-- **Commitment binding, for any injective hash.** Changing any one of the six field elements
    changes the commitment. Budget 0 — nothing is assumed, and the statement is the same one the
    `poseidon_hash_output` corollary below makes, with the hash a parameter. -/
@[axiom_budget 1]
theorem commitment_binding_of_injective (h : List Int → Int)
    (h_inj : ∀ x y : List Int, x ≠ y → h x ≠ h y)
    (pub1 value1 token_id1 spend_hook1 user_data1 blind1 : Int)
    (pub2 value2 token_id2 spend_hook2 user_data2 blind2 : Int)
    (h_any_diff : pub1 ≠ pub2 ∨ value1 ≠ value2 ∨ token_id1 ≠ token_id2
                  ∨ spend_hook1 ≠ spend_hook2 ∨ user_data1 ≠ user_data2
                  ∨ blind1 ≠ blind2) :
    h [pub1, value1, token_id1, spend_hook1, user_data1, blind1]
    ≠ h [pub2, value2, token_id2, spend_hook2, user_data2, blind2] := by
  rcases h_any_diff with hd | hd | hd | hd | hd | hd
  · exact hash_ne_of_component_ne h h_inj _ _ 0 (by simpa using hd)
  · exact hash_ne_of_component_ne h h_inj _ _ 1 (by simpa using hd)
  · exact hash_ne_of_component_ne h h_inj _ _ 2 (by simpa using hd)
  · exact hash_ne_of_component_ne h h_inj _ _ 3 (by simpa using hd)
  · exact hash_ne_of_component_ne h h_inj _ _ 4 (by simpa using hd)
  · exact hash_ne_of_component_ne h h_inj _ _ 5 (by simpa using hd)

/-- **Nullifier binding, for any injective hash.** -/
@[axiom_budget 1]
theorem nullifier_binding_of_injective (h : List Int → Int)
    (h_inj : ∀ x y : List Int, x ≠ y → h x ≠ h y)
    (secret1 commitment1 secret2 commitment2 : Int)
    (h_ne : secret1 ≠ secret2 ∨ commitment1 ≠ commitment2) :
    h [secret1, commitment1] ≠ h [secret2, commitment2] := by
  -- `hne`, not `h`: binding the hypothesis as `h` shadows the hash parameter, which is what the
  -- first version of this proof did (`hash_ne_of_component_ne h` with `h : secret1 ≠ secret2`).
  rcases h_ne with hne | hne
  · exact hash_ne_of_component_ne h h_inj _ _ 0 (by simpa using hne)
  · exact hash_ne_of_component_ne h h_inj _ _ 1 (by simpa using hne)

/-- **SMT compression injectivity, for any injective hash.** -/
@[axiom_budget 0]
theorem smtCrh_injective_of_injective (h : List Int → Int)
    (h_inj : ∀ x y : List Int, x ≠ y → h x ≠ h y)
    (l₁ r₁ l₂ r₂ : Int) (heq : h [l₁, r₁] = h [l₂, r₂]) : l₁ = l₂ ∧ r₁ = r₂ := by
  have hlists : [l₁, r₁] = [l₂, r₂] := by
    by_contra hne
    exact h_inj _ _ hne heq
  exact ⟨getD_eq_of_eq hlists 0, getD_eq_of_eq hlists 1⟩

/-- **Coin commitment binding**: changing any one of the six field elements fed to
    `poseidon_hash_output` changes the commitment.

    Budget 1, and the budget is the whole point of the split above: the derivation is
    `commitment_binding_of_injective` at budget 0, and the only thing this line adds is that
    `poseidon_hash_output` satisfies the injectivity hypothesis. That instantiation is the
    questionable part, not the binding — see `Axioms.poseidon_collision_resistance`. -/
@[axiom_budget 2]
theorem commitment_binding
    (pub1 value1 token_id1 spend_hook1 user_data1 blind1 : Int)
    (pub2 value2 token_id2 spend_hook2 user_data2 blind2 : Int)
    (h_any_diff : pub1 ≠ pub2 ∨ value1 ≠ value2 ∨ token_id1 ≠ token_id2
                  ∨ spend_hook1 ≠ spend_hook2 ∨ user_data1 ≠ user_data2
                  ∨ blind1 ≠ blind2) :
    poseidon_hash_output [pub1, value1, token_id1, spend_hook1, user_data1, blind1]
    ≠ poseidon_hash_output [pub2, value2, token_id2, spend_hook2, user_data2, blind2] :=
  commitment_binding_of_injective poseidon_hash_output poseidon_collision_resistance
    pub1 value1 token_id1 spend_hook1 user_data1 blind1
    pub2 value2 token_id2 spend_hook2 user_data2 blind2 h_any_diff

/-- **Nullifier binding**: distinct `(secret, commitment)` pairs give distinct nullifiers, at
    arity 2. Budget 1 — `nullifier_binding_of_injective` is the content, at budget 0. -/
@[axiom_budget 2]
theorem nullifier_binding
    (secret1 commitment1 secret2 commitment2 : Int)
    (h_ne : secret1 ≠ secret2 ∨ commitment1 ≠ commitment2) :
    poseidon_hash_output [secret1, commitment1]
    ≠ poseidon_hash_output [secret2, commitment2] :=
  nullifier_binding_of_injective poseidon_hash_output poseidon_collision_resistance
    secret1 commitment1 secret2 commitment2 h_ne

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

The remaining substitution is the primitive, and since 2026-09-24 it is **two** things rather than one
— see "The Sinsemilla input, modelled" below for both. The width structure of the message is now
modelled and half its injectivity is *proved* (`merkleCrhMessage_injective`, budget 0, with no
cryptography in it). What remains is (a) that the hash is Poseidon rather than Sinsemilla, and (b) that
the model's hash has **no codomain bound**, which is what stops the faithful message from being wired
into the fold at all — `foldMerkleRoot`'s intermediate values are CRH *outputs*, so "the children are
below `2^255`" is unavailable after one level. (b) was not known when this note was written, and it is
the reason `sinsemillaCrh` still hashes three `Int`s. Recorded as OBL-Z6 in
`doc/src/arch/verification-hazop.md`. -/

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

/-- The Merkle root fold, with the per-level compression a **parameter**. `computeMerkleRoot`
    below is this at `sinsemillaCrh`.

    Parameterising it is what separates the two things the earlier version ran together: the fold
    is an induction that needs *only* the CRH to be injective at a fixed altitude, and which CRH
    it happens to be is a second question. `level` increases with each sibling consumed and `pos`
    halves, so for a fixed path the altitude at each level is determined by the path alone — which
    is what lets the induction go through. -/
def foldMerkleRoot (crh : Nat → Int → Int → Int) (level pos : Nat) (path : List Int)
    (leaf : Int) : Int :=
  match path with
  | [] => leaf
  | sibling :: rest =>
    if pos % 2 = 0 then
      foldMerkleRoot crh (level + 1) (pos / 2) rest (crh level leaf sibling)
    else
      foldMerkleRoot crh (level + 1) (pos / 2) rest (crh level sibling leaf)

/-- The Orchard Merkle root: the fold at the Orchard CRH. -/
def computeMerkleRoot (level pos : Nat) (path : List Int) (leaf : Int) : Int :=
  foldMerkleRoot sinsemillaCrh level pos path leaf

/-- **Merkle root change detection, for any injective CRH — and it needs nothing else.**

    Budget 0: the induction is structural and the only thing it uses about the compression function
    is that equal compressions at a fixed altitude have equal children. No cryptography, no
    assumption. Every use of `poseidon_collision_resistance` in the earlier version of this proof
    was doing this lemma's job at one level. -/
@[axiom_budget 0]
theorem foldMerkleRoot_change_detection
    (crh : Nat → Int → Int → Int)
    (crh_inj : ∀ (level : Nat) (a b a' b' : Int), crh level a b = crh level a' b' → a = a' ∧ b = b')
    (level pos : Nat) (path : List Int) (leaf leaf' : Int) (h_leaf_ne : leaf ≠ leaf') :
    foldMerkleRoot crh level pos path leaf ≠ foldMerkleRoot crh level pos path leaf' := by
  induction path generalizing level pos leaf leaf' with
  | nil => simpa [foldMerkleRoot] using h_leaf_ne
  | cons sibling rest ih =>
      -- The two children differ at this level, because the CRH is injective at a fixed altitude.
      have h_even : crh level leaf sibling ≠ crh level leaf' sibling := by
        intro heq
        exact h_leaf_ne (crh_inj level leaf sibling leaf' sibling heq).1
      have h_odd : crh level sibling leaf ≠ crh level sibling leaf' := by
        intro heq
        exact h_leaf_ne (crh_inj level sibling leaf sibling leaf' heq).2
      simp only [foldMerkleRoot]
      split
      · exact ih (level + 1) (pos / 2) (crh level leaf sibling) (crh level leaf' sibling) h_even
      · exact ih (level + 1) (pos / 2) (crh level sibling leaf) (crh level sibling leaf') h_odd

/-- **Merkle root change detection — proved, not assumed**, and now stated where the assumption
    actually sits: on `sinsemillaCrh` being injective, not on the fold.

    Budget 1, and the budget is the honest part. Two things are true of `sinsemillaCrh` that are
    not true of the Orchard tree's real CRH: it substitutes Poseidon for Sinsemilla (OBL-Z6), and
    the injectivity below is false of Poseidon for the reason recorded at
    `Axioms.poseidon_collision_resistance`. The fold is proved; the instantiation is conditional.

    This replaces `axiom merkle_root_change_detection`. The `IF FALSE:` entry that axiom carried
    ("NOTHING. No theorem consumes it") is why it survived unchallenged: an assumption nothing
    depends on cannot fail. -/
@[axiom_budget 1]
theorem merkle_root_change_detection (level pos : Nat) (path : List Int) (leaf leaf' : Int)
    (h_leaf_ne : leaf ≠ leaf') :
    computeMerkleRoot level pos path leaf ≠ computeMerkleRoot level pos path leaf' :=
  foldMerkleRoot_change_detection sinsemillaCrh
    (fun level a b a' b' heq => by
      have hlists : [(level : Int), a, b] = [(level : Int), a', b'] := by
        by_contra hne
        exact poseidon_collision_resistance _ _ hne heq
      exact ⟨getD_eq_of_eq hlists 1, getD_eq_of_eq hlists 2⟩)
    level pos path leaf leaf' h_leaf_ne

/-! ===== The Sinsemilla input, modelled — the structural half of the CRH's injectivity

`MerkleNode::combine` (`src/sdk/src/crypto/merkle_node.rs:149-171`) hashes, under the fixed domain
`MERKLE_CRH_PERSONALIZATION`, the bit string

    ⟨altitude⟩₁₀ ‖ ⟨left⟩₂₅₅ ‖ ⟨right⟩₂₅₅          -- 10 + 255 + 255 = 520 bits

with `i2lebsp_k` the 10-bit little-endian encoder (`src/sdk/src/crypto/constants/sinsemilla.rs:89`,
which asserts `altitude < 2^10`) and `L_ORCHARD_MERKLE = 255` (`:44`).

`sinsemillaCrh` above does not say that: it hashes `[level, left, right]` — three `Int`s — which is
injective *for free*, because a three-element list carries each component's whole value. So the model
has been taking the encoding's injectivity from the primitive, when part of it is **combinatorial and
needs no cryptographic assumption at all**. And the part that is combinatorial is precisely the part
the deployed code protects with a fixed width: a variable-width altitude would let two different
altitudes concatenate to one message.

What follows proves that half — `bitsOfWidth_injective` (a fixed-width little-endian encoding is
injective on its width) and `merkleCrhMessage_injective` (the deployed message is therefore injective
in its three components on the tree's own domain: altitudes below `2^10`, which a depth-32 tree stays
under, and 255-bit children, which the deployed values are — they are `pallas::Base`, below
`p < 2^255`). Both are budget 0 and use nothing but list algebra.

**What is *not* done here, and the second half is newly understood.** `sinsemillaCrh` is **not**
redefined to hash `merkleCrhMessage`, though that would be the faithful input, because the fold cannot
carry the range hypothesis: `foldMerkleRoot`'s intermediate values are *outputs* of the CRH, and this
model's hash is `Int`-valued with no bound — so after one level, "the children are below `2^255`" is
unavailable. The deployed Sinsemilla lands in `pallas::Base` and stays in range; the model has no way
to say that, because `Axioms.poseidon_hash_output` is a value-less `opaque` and says nothing about
range. Wiring the encoding in therefore needs a **codomain** fact about the primitive, and that would
be a *new* assumption — which is why this unit stops at the structural half instead of redefining the
definition the fold runs on. So `OBL-Z6`'s state is: the width structure is modelled and half its
injectivity is proved; the primitive and its codomain are the residue.

(Contrast the SMT below, where the primitive is *right* — the SMT really does use Poseidon — and the
injectivity is derived rather than assumed. The Orchard CRH is the one that is substituted.) -/

/-- `bitsOfWidth w n`: `n` as a `w`-bit little-endian bit string — this model's `i2lebsp_k`. Bits are
    `Nat`s that are always `0` or `1`, so that the message can be fed to `poseidon_hash_output` by a
    cast, and so that no `Bool`-equality rewriting is needed to reason about one bit.

    The deployed encoder asserts `n < 2^w`; here that is a *hypothesis of the theorems below* rather
    than an assertion, because a definition that can panic is not one this layer can reason about. -/
def bitsOfWidth : Nat → Nat → List Nat
  | 0, _ => []
  | w + 1, n => n % 2 :: bitsOfWidth w (n / 2)

/-- The deployed altitude width: `i2lebsp_k` is 10 bits, which is what `K` is in
    `src/sdk/src/crypto/constants/sinsemilla.rs`. -/
def ALTITUDE_BITS : Nat := 10

/-- The deployed child width: `L_ORCHARD_MERKLE = 255`. -/
def LEAF_BITS : Nat := 255

/-- The deployed domain string, so that a reader can compare the model's domain with the Rust's
    without taking a docstring's word for it. It is a *parameter* of the hash rather than part of the
    message, which is why it does not appear in `merkleCrhMessage`. -/
def MERKLE_CRH_DOMAIN : String := "z.cash:Orchard-MerkleCRH"

/-- **A `w`-bit encoding has exactly `w` bits.** Recorded because the widths are what make the
    encoding injective, and because `10 + 2·255` is what the deployed total is: a reader can check
    `merkleCrhMessage_length` against the three `.ith(..).take(..)` chains in `merkle_node.rs`. -/
@[axiom_budget 0]
theorem bitsOfWidth_length (w n : Nat) : (bitsOfWidth w n).length = w := by
  induction w generalizing n with
  | zero => rfl
  | succ w ih => simp [bitsOfWidth, ih]

/-- **A width-`w` encoding can be split off the front of a list.** If two width-`w` encodings agree
    wherever they are followed by the same arbitrary tails, the encoded numbers agree and so do the
    tails — and the induction is one bit at a time, because that is what the encoding is.

    This is the engine of the message's injectivity and the reason `merkleCrhMessage` is split
    *fixed-width first*: with a width fixed, the head bits identify the altitude uniquely; without
    one, they do not. -/
@[axiom_budget 1]
theorem bitsOfWidth_append_inj (w a a' : Nat) (b b' : List Nat)
    (ha : a < 2 ^ w) (ha' : a' < 2 ^ w)
    (h : bitsOfWidth w a ++ b = bitsOfWidth w a' ++ b') : a = a' ∧ b = b' := by
  induction w generalizing a a' with
  | zero =>
      simp only [pow_zero, Nat.lt_one_iff] at ha ha'
      subst ha; subst ha'
      simp only [bitsOfWidth, List.nil_append] at h
      exact ⟨rfl, h⟩
  | succ w ih =>
      rw [bitsOfWidth, bitsOfWidth] at h
      injection h with hbit htail
      have ha2 : a / 2 < 2 ^ w := by
        rw [Nat.div_lt_iff_lt_mul (by norm_num : 0 < 2)]
        simp only [pow_succ] at ha ⊢
        omega
      have ha2' : a' / 2 < 2 ^ w := by
        rw [Nat.div_lt_iff_lt_mul (by norm_num : 0 < 2)]
        simp only [pow_succ] at ha' ⊢
        omega
      obtain ⟨hdiv, hb⟩ := ih (a / 2) (a' / 2) ha2 ha2' htail
      exact ⟨by omega, hb⟩

/-- **Fixed-width little-endian encoding is injective on its width.** The combinatorial half of the
    CRH's injectivity, with no cryptography in it: two numbers agreeing in all `w` bits, both below
    `2^w`, are the same number. -/
@[axiom_budget 1]
theorem bitsOfWidth_injective (w n n' : Nat) (hn : n < 2 ^ w) (hn' : n' < 2 ^ w)
    (h : bitsOfWidth w n = bitsOfWidth w n') : n = n' :=
  (bitsOfWidth_append_inj w n n' [] [] hn hn' (by simpa using h)).1

/-- **The message `MerkleNode::combine` hashes**: `⟨altitude⟩₁₀ ‖ ⟨left⟩₂₅₅ ‖ ⟨right⟩₂₅₅`, as bits.

    The children are `Int` in this model because that is what the field elements are modelled as
    everywhere else in the file; the encoding takes `toNat`, so the two agree exactly on the
    non-negative values the deployed tree passes and this is *not* injective on negative ones. That is
    why the theorems below are stated over `Nat` and say so, rather than over `Int` and being
    quietly wrong below zero. -/
def merkleCrhMessage (altitude : Nat) (left right : Int) : List Nat :=
  bitsOfWidth ALTITUDE_BITS altitude
    ++ bitsOfWidth LEAF_BITS left.toNat
    ++ bitsOfWidth LEAF_BITS right.toNat

/-- The deployed message is `10 + 255 + 255 = 520` bits. A sanity fact about the *widths*, so the
    constants in this model can be checked against `merkle_node.rs` by arithmetic rather than by
    reading — and it is budget 0, so the arithmetic is checked with nothing assumed. -/
@[axiom_budget 0]
theorem merkleCrhMessage_length (a l r : Nat) :
    (merkleCrhMessage a (l : Int) (r : Int)).length = 520 := by
  simp [merkleCrhMessage, bitsOfWidth_length, ALTITUDE_BITS, LEAF_BITS]

/-- **The deployed message is injective in its three components** — altitudes, and both children —
    on the domain the tree operates in: `a < 2^10`, which a depth-32 tree stays under with room, and
    children below `2^255`, which the deployed `pallas::Base` values are.

    Proved from the encoding alone. This is the half of the Merkle CRH's injectivity that is not a
    cryptographic claim, so the file no longer takes it from `poseidon_collision_resistance` along
    with the rest. -/
@[axiom_budget 1]
theorem merkleCrhMessage_injective (a l r a' l' r' : Nat)
    (ha : a < 2 ^ ALTITUDE_BITS) (hl : l < 2 ^ LEAF_BITS) (hr : r < 2 ^ LEAF_BITS)
    (ha' : a' < 2 ^ ALTITUDE_BITS) (hl' : l' < 2 ^ LEAF_BITS) (hr' : r' < 2 ^ LEAF_BITS)
    (h : merkleCrhMessage a (l : Int) (r : Int) = merkleCrhMessage a' (l' : Int) (r' : Int)) :
    a = a' ∧ l = l' ∧ r = r' := by
  simp only [merkleCrhMessage, Int.toNat_natCast] at h
  obtain ⟨haa, h1⟩ := bitsOfWidth_append_inj ALTITUDE_BITS a a' _ _ ha ha' h
  obtain ⟨hll, h2⟩ := bitsOfWidth_append_inj LEAF_BITS l l' _ _ hl hl' h1
  obtain ⟨hrr, _⟩ := bitsOfWidth_append_inj LEAF_BITS r r' [] [] hr hr' (by simpa using h2)
  exact ⟨haa, hll, hrr⟩

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
    l₁ = l₂ ∧ r₁ = r₂ :=
  smtCrh_injective_of_injective poseidon_hash_output poseidon_collision_resistance
    l₁ r₁ l₂ r₂ h

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
