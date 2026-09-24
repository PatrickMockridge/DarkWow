/-
# The chain-level commitment set — what it is for, and what the prune costs

`chain_state.rs` keeps `commitment_set : Mutex<BTreeMap<Commitment, BlockHeight>>`: a flat key→height
map, **not** a tree and not an SMT. This module models it as what the Rust does with it, and the first
finding of the unit is that this is *not* what the specification says it is for.

## Two things the specification gets wrong, both measured

**The Python claims to mirror a function that does not exist.** `contrib/model/chain_model.py`'s
`is_commitment_mature` carries the docstring "Mirrors
`src/linear/src/chain_state.rs:is_commitment_mature()`" — and there is no such function. The Rust's
maturity rule is `check_coinbase_maturity`, and it keys by **nullifier**
(`nullifier_set : BTreeMap<Nullifier, BlockHeight>`), where the Python keys by **commitment**. That is
not a naming slip: the Rust's own docstring records the other choice as
"*considered and rejected* it", on the grounds that a commitment-set lookup would be "a second source of
truth for maturity (hazid `RC5` class)". So the specification models the design the code rejected, and
cites a function to justify it that was never written. Register row `OBL-C109`.

**And the chain-level set has no production reader.** It is written at the coinbase and fee-collect
paths, pruned by maturity, and restored from sled on restart; `has_commitment` — its only accessor — is
called from `bin/dwowd/src/tests/daemon_sync_integration.rs`, whose assertions are about *reorg
reversal*. So the set is maintained, pruned and persisted, and what reads it is a test. A model of it
should therefore be a model of the **prune**, not of a uniqueness rule: nothing enforces uniqueness on
this set. (Uniqueness *is* enforced, but by the contracts' own trees — `db_contains_key(commitment_set,
…)` at each exec — which are different trees, one per contract, and not this one.)

## What the prune costs, and why it costs nothing to the rule that runs

`commitment_set` and `nullifier_set` are pruned by the same line: entries older than `COINBASE_MATURITY`
are dropped (`retain(|_, h| *h >= prune_h)`). That makes a **mature commitment indistinguishable from one
that was never recorded** — both read as `none`, which is why the Rust's docstring calls the set a
dangerous source: an ordinary reader cannot tell "pruned because old" from "not a coinbase commitment".

For the maturity rule that conflation is harmless, and the two theorems below are why: a refusal needs an
entry *inside* the window, the prune keeps everything inside the window, and it invents nothing. So
`prune_preserves_refusal` says the prune neither creates nor destroys a refusal — the rule's verdict is
the same before and after pruning — even though the *set* loses information. `none` already meant "not
this rule's business", and a dropped entry meant "no longer refusable", which is the same verdict.

## The rule is stated without subtraction

`refused` is `current < h + K` rather than the Rust's `current.saturating_sub(h) < K`. The two agree on
`Nat`, and the addition form is the one this layer can reason about: `omega` decides it directly, where
the truncated subtraction needed a case split at every step. That is not a modelling choice about the
rule — it is the same rule, stated in the shape the proofs can see. -/

import DarkFi.Combinatorial.StateSpace
import DarkFi.AxiomBudget

namespace Consensus.CommitmentSet

open Combinatorial

/-- The chain-level commitment set: a flat key→height map, as `chain_state.rs` keeps it. A predicate over
    commitments, not a Merkle tree — the chain-level set is not one, and modelling it as one would model
    something the code does not have. -/
abbrev CommitmentSet := LeafCommitment → Option Nat

/-- Record a commitment at the height it was created. -/
def record (s : CommitmentSet) (c : LeafCommitment) (h : Nat) : CommitmentSet :=
  fun c' => if c' = c then some h else s c'

/-- **The prune**, as `chain_state.rs` performs it: at `current`, drop every entry older than `K` blocks.
    `K` is `COINBASE_MATURITY` at the call site; it is a parameter here so the rule is stated once. -/
def prune (s : CommitmentSet) (current K : Nat) : CommitmentSet :=
  fun c => match s c with
    | some h => if current - K ≤ h then some h else none
    | none => none

/-- **What the maturity rule refuses**: a commitment recorded at a height inside the window. The Rust's
    `current.saturating_sub(h) < K`, restated as `current < h + K` — equivalent on `Nat`, and the form
    `omega` decides without a case split. See the module note. -/
def refused (s : CommitmentSet) (c : LeafCommitment) (current K : Nat) : Prop :=
  ∃ h, s c = some h ∧ current < h + K

/-- **The prune destroys no refusal.** A refused entry is inside the window and the prune keeps
    everything inside the window. -/
@[axiom_budget 0]
theorem prune_refuses_of_refuses (s : CommitmentSet) (c : LeafCommitment) (current K : Nat) :
    refused s c current K → refused (prune s current K) c current K := by
  rintro ⟨h, hs, hlt⟩
  have hle : current - K ≤ h := by rw [Nat.sub_le_iff_le_add]; omega
  exact ⟨h, by simp only [prune, hs, if_pos hle], hlt⟩

/-- **And it creates none.** The prune returns an entry only from the store it prunes, so a refusal it
    exhibits was already there. -/
@[axiom_budget 0]
theorem refuses_of_prune_refuses (s : CommitmentSet) (c : LeafCommitment) (current K : Nat) :
    refused (prune s current K) c current K → refused s c current K := by
  rintro ⟨h, hq, hlt⟩
  cases hs : s c with
  | none => simp only [prune, hs] at hq; exact absurd hq (by simp)
  | some h' =>
    by_cases hle : current - K ≤ h'
    · simp only [prune, hs, if_pos hle, Option.some.injEq] at hq
      exact ⟨h', by simp only [hs], by omega⟩
    · simp only [prune, hs, if_neg hle] at hq
      exact absurd hq (by simp)

/-- **The prune is verdict-preserving for the maturity rule**, which is the whole of what it costs that
    rule: the same commitments are refused before and after. The set still loses information — a pruned
    commitment reads as `none`, exactly like one never recorded — and `OBL-C109` is the row about the
    specification treating that set as an authority it is not. -/
@[axiom_budget 0]
theorem prune_preserves_refusal (s : CommitmentSet) (c : LeafCommitment) (current K : Nat) :
    refused (prune s current K) c current K ↔ refused s c current K :=
  ⟨refuses_of_prune_refuses s c current K, prune_refuses_of_refuses s c current K⟩

/-! ===== Non-vacuity: the boundary, at concrete heights =====

`K = 100` is `COINBASE_MATURITY`. A commitment recorded at height 0 is refused at 50, admitted at 100,
and admitted at 100 *after* the prune has run — which is the theorem above, exercised rather than
asserted, because a `↔` about a rule that refuses nothing would hold trivially. -/

/-- A commitment recorded at `h` is refused while it is young. -/
@[axiom_budget 0]
theorem record_refuses_while_young (s : CommitmentSet) (c : LeafCommitment) (h current K : Nat)
    (hyoung : current < h + K) : refused (record s c h) c current K :=
  ⟨h, by simp [record], hyoung⟩

/-- And admitted once it is old. -/
@[axiom_budget 0]
theorem record_admits_once_old (s : CommitmentSet) (c : LeafCommitment) (h current K : Nat)
    (hold : h + K ≤ current) : ¬ refused (record s c h) c current K := by
  rintro ⟨h', hs, hlt⟩
  simp only [record, if_true, Option.some.injEq] at hs
  subst hs
  omega

@[axiom_budget 0]
theorem witness :
    (refused (record (fun _ => none) 0 0) 0 50 100) ∧
    (¬ refused (record (fun _ => none) 0 0) 0 100 100) ∧
    (¬ refused (prune (record (fun _ => none) 0 0) 50 100) 0 100 100) :=
  ⟨record_refuses_while_young _ _ 0 50 100 (by omega),
   record_admits_once_old _ _ 0 100 100 (by omega),
   fun hr => record_admits_once_old _ _ 0 100 100 (by omega)
     (refuses_of_prune_refuses _ _ _ _ hr)⟩

/-- And the prune *does* eventually drop the entry — at a height past the window, which is what makes the
    verdict-preservation above a statement about something: the rule keeps its answer while the set
    stops holding the record that produced it. Stated at `200` rather than at `100` because the prune
    keeps `h ≥ current - K`, so at exactly `current = K` it keeps everything and drops nothing. -/
@[axiom_budget 0]
theorem prune_drops_the_old_entry :
    prune (record (fun _ => none) 0 0) 200 100 0 = none := by
  simp only [prune, record, if_true, if_neg (by omega : ¬ (200 - 100 ≤ 0))]

end Consensus.CommitmentSet
