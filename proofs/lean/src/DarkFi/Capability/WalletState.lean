/-
DarkFi/Capability/WalletState.lean — wallet.md §1 (wallet state as a pure function), §6.5 (provisional
state) and §6.4.0's commitment-tree obligation.

The declarations live in `namespace DarkFi.Capability` rather than in a namespace named after this file,
which is `Wire.lean`'s arrangement and not an accident: §6.5's central type *is* `WalletState`, so the
self-named spelling would be `DarkFi.Capability.WalletState.WalletState` and Lean's `dupNamespace`
linter warns on it (and this tree suppresses no linters). Everything here is therefore
`DarkFi.Capability.<name>`, with the two actions §6.5 names generically — dropping and confirming —
carrying domain-specific names (`dropTransaction`, `confirmBlock`) because the namespace is shared.

**What this models.** §1 says `WalletState = f(AccountManager, ChainBlocks)` and gives the seven steps
that make it pure; §6.5 refines it as `WalletState = ConfirmedState ⊕ ProvisionalState` with one
governing invariant — *"ProvisionalState SHALL NOT mutate ConfirmedState"* — and the spend-state
lifecycle each held capability carries. Neither had a model: §1's only ever artifact gesturing at it
was `walletConstruct_idempotent`, which was `x = x` and is deleted (`wallet.md:1009`), and the
provisional layer existed in Rust (`bin/dww/src/capability.rs`) and in prose only.

What is modelled of §1 is **steps 6 and 7** — sequential iteration by height, and `INSERT OR IGNORE` —
because those two are what make the scan a *fold*: step 6 is why the fold is over a list rather than a
set, and step 7's idempotence is what `rescanning_an_applied_block_adds_nothing` below is about. Steps
1–5 (key derivation, AEAD, commitment, nullifier, Merkle) produce the *facts* a block carries; they are
`KeyScope.lean`, `Net/Receive.lean`, `HashOps.lean` and `PerContractTree.lean`'s subjects, and this
module consumes their results rather than restating them.

**The `revoked` field, and why the repair is a theorem.** §6.5 derives `CapRecord.revoked` from the
capability's status (`revoked == (status IS Processing OR status IS Spent)`) and records that
`check_status_revoked_consistency()` exists in Rust to *repair* any divergence after a crash. Two
representations of one fact is exactly the shape that drifts, so the invariant is stated as a predicate
and the repair is proved to establish it — the theorem's content is that the repair is a repair, and
`an_inconsistent_record_exists` is there so that the predicate is not vacuously true of every record.

**What is not modelled.** The Merkle root and historical roots in the confirmed state: recomputing a
root from discovered commitments is the contract tree's business (`PerContractTree.lean`, which this
module extends with the subset-tree divergence §6.4.0's obligation turns on), and `applyBlock` leaves
both fields untouched rather than pretending otherwise. The transaction status lifecycle of §6.5
(Built → Broadcast → Pending → Mined → Confirmed, Dropped) is not modelled here: what this module
models is what a *capability's* status does, and the transaction-level machine is the one whose edges
the Rust drives from two observers (mempool and scan). Naming it as absent is the honest reading of
"one of the two machines is modelled".

**A correction this file owes, added 2026-09-24.** It landed claiming a clean build — "0 errors, 0
warnings" — and that claim was wrong: `scanFrom_append`'s cons case wrote `| cons b bs ih` and
`linter.unusedVariables` flagged `bs`, which the proof never mentions because the induction hypothesis
alone discharges the goal. The build that reported it was the next full `DarkFi` build, so between the
two the tree carried a warning in a file whose landing said it had none. The binder is `_` now and the
build is clean (measured, `scripts/lean-build.sh build DarkFi`, 0 errors / 0 warnings, and the log
shows `✔ [5222/5223] Built DarkFi`, so the module really was elaborated rather than left cached).

**Why the earlier claim was wrong is not known and is not guessed here.** Two candidates are obvious —
a warning line read past in a log that did contain it, or a claim about a module a given run had left
cached — and this note does not pick one, because neither was measured. What *is* general, and is the
reason this paragraph exists rather than being a silent fix: **"0 warnings" is a claim about a build
that ran, so it has to be read off that build's log, not asserted about the code.** The tree's bar is
zero warnings, which makes the difference between reading and asserting it a real one.
-/

import Mathlib
import DarkFi.AxiomBudget
import DarkFi.Capability.Selection
import DarkFi.Capability.PerContractTree
import DarkFi.Combinatorial.StateSpace

namespace DarkFi.Capability

open DarkFi.Capability.Selection

/- ==========================================================================
   §1 step 7 — `INSERT OR IGNORE`
   ==========================================================================
   The one operation that gives the scan its idempotence. Appending only what is absent makes the
   result a set *as a list*: order is preserved (step 6 needs that) and re-inserting adds nothing
   (step 7's `INSERT OR IGNORE`).
   ========================================================================== -/

/-- §1 step 7's `INSERT OR IGNORE`: append each of `ys` to `xs` unless it is already there. -/
def insertOrIgnore (xs ys : List Nat) : List Nat :=
  ys.foldl (fun acc y => if y ∈ acc then acc else acc ++ [y]) xs

/-- The fold only ever adds: everything the accumulator had is still in it. Needed because the head
    case of `mem_insertOrIgnore` has to know that an element it just inserted survives the rest of
    the fold. -/
@[axiom_budget 0]
theorem foldl_insertOrIgnore_grows (a : Nat) (xs ys : List Nat) (ha : a ∈ xs) :
    a ∈ ys.foldl (fun acc y => if y ∈ acc then acc else acc ++ [y]) xs := by
  induction ys generalizing xs with
  | nil => exact ha
  | cons z zs ih =>
      simp only [List.foldl_cons]
      exact ih _ (by by_cases hz : z ∈ xs <;> simp_all [List.mem_append])

/-- Everything inserted is present afterwards — the half of `INSERT OR IGNORE` that makes the second
    insertion a no-op. -/
@[axiom_budget 0]
theorem mem_insertOrIgnore (xs ys : List Nat) : ∀ y ∈ ys, y ∈ insertOrIgnore xs ys := by
  unfold insertOrIgnore
  induction ys generalizing xs with
  | nil => intro y hy; simp at hy
  | cons z zs ih =>
      intro y hy
      simp only [List.foldl_cons]
      -- `rfl` on `y = z` substitutes `z := y` (Lean eliminates the variable that is not the one
      -- being introduced), so this branch is written in terms of `y`.
      rcases List.mem_cons.mp hy with rfl | hy'
      · exact foldl_insertOrIgnore_grows y _
          zs (by by_cases h : y ∈ xs <;> simp_all [List.mem_append])
      · exact ih _ y hy'

/-- And a fold that inserts only what is absent leaves the accumulator alone once everything it
    inserts is already there — the other half. -/
@[axiom_budget 0]
theorem foldl_insertOrIgnore_absorbed (xs ys : List Nat) (h : ∀ y ∈ ys, y ∈ xs) :
    ys.foldl (fun acc y => if y ∈ acc then acc else acc ++ [y]) xs = xs := by
  induction ys generalizing xs with
  | nil => rfl
  | cons z zs ih =>
      simp only [List.foldl_cons]
      have hz : z ∈ xs := h z (by simp)
      rw [if_pos hz]
      exact ih xs (fun w hw => h w (by simp [hw]))

/-- **Idempotence, which is the whole of step 7.** Inserting `ys` into a list that already has them
    changes nothing — so a re-scan of the same facts is the identity. -/
@[axiom_budget 0]
theorem insertOrIgnore_idempotent (xs ys : List Nat) :
    insertOrIgnore (insertOrIgnore xs ys) ys = insertOrIgnore xs ys := by
  unfold insertOrIgnore
  exact foldl_insertOrIgnore_absorbed _ ys (fun y hy => mem_insertOrIgnore xs ys y hy)

/- ==========================================================================
   §1 steps 6 and 7 — the scan as a fold
   ========================================================================== -/

/-- §1's `ChainBlocks`, one block: what the scan reads out of it. Steps 1–5 are what *produce* these
    two lists (a block's nullifier observations are step 4's result, its discovered commitments step
    3's), so this is the fold's input rather than a second copy of those steps. -/
structure ChainBlock where
  height : Nat
  observedNullifiers : List Combinatorial.NullifierValue
  discoveredCommitments : List Combinatorial.LeafCommitment
  deriving DecidableEq, BEq, Repr

/-- §1's `ConfirmedState`: what the scan has established. The two tree fields are §1 step 5's, and
    `applyBlock` deliberately does not touch them — recomputing a root from discovered commitments is
    `PerContractTree.lean` and `HashOps.lean`'s subject, and leaving them untouched says so instead of
    pretending. `held` is the wallet's own capability set, which §6.2's selection reads.
    The fields are `Combinatorial.PublicState`'s plus `held`, inlined rather than nested so that a
    `ConfirmedState` equality is one structure equality and not two. -/
structure ConfirmedState where
  merkleRoot : Combinatorial.MerkleRoot
  spentNullifiers : List Combinatorial.NullifierValue
  historicalRoots : List Combinatorial.MerkleRoot
  recognizedCommitments : List Combinatorial.LeafCommitment
  held : List Combinatorial.LeafCommitment
  deriving DecidableEq, BEq, Repr

/-- The empty confirmed state: nothing scanned, no root, no capabilities. -/
def ConfirmedState.empty : ConfirmedState :=
  { merkleRoot := 0, spentNullifiers := [], historicalRoots := [0]
  , recognizedCommitments := [], held := [] }

/-- §1's scan of one block: publish its observed nullifiers, recognize its commitments, hold the
    capabilities it produced. Three `INSERT OR IGNORE`s (step 7) over a sequential iteration (step 6),
    and nothing else — which is what `applyBlock_idempotent` below turns into a law. -/
def applyBlock (s : ConfirmedState) (b : ChainBlock) : ConfirmedState :=
  { merkleRoot := s.merkleRoot
  , spentNullifiers := insertOrIgnore s.spentNullifiers b.observedNullifiers
  , historicalRoots := s.historicalRoots
  , recognizedCommitments := insertOrIgnore s.recognizedCommitments b.discoveredCommitments
  , held := insertOrIgnore s.held b.discoveredCommitments }

/-- **Applying a block twice is applying it once** — §1 step 7's `INSERT OR IGNORE`, field by field. -/
@[axiom_budget 0]
theorem applyBlock_idempotent (s : ConfirmedState) (b : ChainBlock) :
    applyBlock (applyBlock s b) b = applyBlock s b := by
  unfold applyBlock
  simp only [insertOrIgnore_idempotent]

/-- §1 step 6: the scan is sequential, an explicit fold rather than a set union. -/
def scanFrom (s : ConfirmedState) : List ChainBlock → ConfirmedState
  | [] => s
  | b :: bs => scanFrom (applyBlock s b) bs

/-- §1's `f(AccountManager, ChainBlocks)`, confirmed half: scan every synced block from empty. The
    `AccountManager` is what makes a block's facts *readable* (steps 1–4), so it is an input to the
    decrypt/derive layer rather than to this fold; this module starts where those facts exist. -/
def scan (bs : List ChainBlock) : ConfirmedState := scanFrom ConfirmedState.empty bs

/-- The fold commutes with concatenation, which is what lets a scan be extended rather than redone. -/
@[axiom_budget 0]
theorem scanFrom_append (s : ConfirmedState) (bs₁ bs₂ : List ChainBlock) :
    scanFrom s (bs₁ ++ bs₂) = scanFrom (scanFrom s bs₁) bs₂ := by
  induction bs₁ generalizing s with
  | nil => rfl
  -- `_` and not `bs`: the goal is discharged by the induction hypothesis alone, so the tail binder is
  -- never mentioned and `linter.unusedVariables` is right to flag it. It was `bs` when this module
  -- landed, and the build reported it a warning on 2026-09-24 — **after** this file's landing claimed
  -- a clean build, which is the correction recorded in the module note below.
  | cons b _ ih => exact ih (applyBlock s b)

/-- **The fold law §1 needs, and the reason the scan is stable under a re-scan**: a block that has
    already been applied adds nothing when it is seen again. This is `applyBlock_idempotent` lifted
    through the fold, and it is the statement `wallet.md` §1 step 7's `INSERT OR IGNORE` is there for —
    without it a wallet that re-scans a block (restart, re-sync, an overlapping range) would duplicate
    every fact it had already recorded. -/
@[axiom_budget 0]
theorem rescanning_an_applied_block_adds_nothing (s : ConfirmedState) (b : ChainBlock)
    (bs : List ChainBlock) :
    scanFrom (applyBlock s b) (b :: bs) = scanFrom (applyBlock s b) bs := by
  -- The two sides differ only in that the left one applies `b` a second time, which
  -- `applyBlock_idempotent` says is nothing: `scanFrom`'s own cons equation is what puts it there.
  calc scanFrom (applyBlock s b) (b :: bs)
      = scanFrom (applyBlock (applyBlock s b) b) bs := rfl
    _ = scanFrom (applyBlock s b) bs := by rw [applyBlock_idempotent]

/-- **And the order is load-bearing**, which is why step 6 is "sequential by height" rather than an
    unordered union: two blocks in the other order leave the recorded lists in the other order. A
    model that unioned its inputs would satisfy every theorem above and fail this one. -/
@[axiom_budget 0]
theorem scan_order_is_load_bearing :
    ∃ b₁ b₂ : ChainBlock, scan [b₁, b₂] ≠ scan [b₂, b₁] :=
  ⟨{ height := 1, observedNullifiers := [1], discoveredCommitments := [] }
  , { height := 2, observedNullifiers := [2], discoveredCommitments := [] }
  , by decide⟩

/- ==========================================================================
   §6.5 — the provisional layer, and the invariant that governs it
   ========================================================================== -/

/-- §6.5's `ProvisionalState`: transactions in flight, and the capabilities they have spoken for. -/
structure ProvisionalState where
  pendingTransactions : List Nat
  reservedCapabilities : List (Nat × Combinatorial.LeafCommitment)
  deriving BEq, Repr

/-- §1's `WalletState`, refined by §6.5: confirmed state (a pure function of the blocks) beside a
    provisional layer that holds no confirmed authority. -/
structure WalletState where
  confirmed : ConfirmedState
  provisional : ProvisionalState

/-- The only way a provisional fact changes: replace the provisional half. Named so that the governing
    invariant below has something to be about. -/
def applyProvisional (ws : WalletState) (ps : ProvisionalState) : WalletState :=
  { ws with provisional := ps }

/-- **§6.5's governing invariant, in the general form**: `ProvisionalState SHALL NOT mutate
    ConfirmedState`. Every named action below is an instance of this, and the statement's content is
    that the confirmed half is the *argument's* — no provisional input appears on the right. -/
@[axiom_budget 0]
theorem provisional_never_mutates_confirmed (ws : WalletState) (ps : ProvisionalState) :
    (applyProvisional ws ps).confirmed = ws.confirmed := rfl

/-- §6.5's broadcast: the transaction becomes pending and the capabilities it consumes are spoken for,
    *provisionally* — §6.2's exclusion follows from this, not from a confirmed fact. -/
def broadcast (ws : WalletState) (txId : Nat)
    (consumed : List Combinatorial.LeafCommitment) : WalletState :=
  { confirmed := ws.confirmed
  , provisional :=
      { pendingTransactions := ws.provisional.pendingTransactions ++ [txId]
      , reservedCapabilities :=
          ws.provisional.reservedCapabilities ++ consumed.map (fun c => (txId, c)) } }

/-- Broadcasting cannot touch confirmed state. -/
@[axiom_budget 0]
theorem broadcast_preserves_confirmed (ws : WalletState) (txId : Nat)
    (consumed : List Combinatorial.LeafCommitment) :
    (broadcast ws txId consumed).confirmed = ws.confirmed := rfl

/-- §6.5's terminal `Dropped`: the transaction is discarded and its reservations are released
    (`Reserved → Unspent`). Both halves are filters over the provisional layer — the dropping is
    total, not "forget one of the two". `!=` rather than `≠` because `List.filter` takes a `Bool`
    predicate, as the rest of this tree's uses of it do. -/
def dropTransaction (ws : WalletState) (txId : Nat) : WalletState :=
  { confirmed := ws.confirmed
  , provisional :=
      { pendingTransactions := ws.provisional.pendingTransactions.filter (fun t => t != txId)
      , reservedCapabilities :=
          ws.provisional.reservedCapabilities.filter (fun p => p.1 != txId) } }

/-- Dropping cannot touch confirmed state either. -/
@[axiom_budget 0]
theorem dropTransaction_preserves_confirmed (ws : WalletState) (txId : Nat) :
    (dropTransaction ws txId).confirmed = ws.confirmed := rfl

/-- **`Dropped` releases every reservation the transaction held** — §6.5's release rule, stated as
    release rather than as deletion: a capability that was spoken for by the dropped transaction is
    spoken for by nothing afterwards. Without this half a dropped transaction would leave its
    capabilities unselectable forever, which is the failure mode the rule exists to prevent. -/
@[axiom_budget 0]
theorem dropped_transaction_releases_its_reservations (ws : WalletState) (txId : Nat)
    (c : Combinatorial.LeafCommitment)
    (h : (txId, c) ∈ ws.provisional.reservedCapabilities) :
    (txId, c) ∉ (dropTransaction ws txId).provisional.reservedCapabilities := by
  intro hmem
  simp only [dropTransaction] at hmem
  simp_all

/-- §6.5's confirmation: a block arrives, and its facts are confirmed — *from the block*, never from
    the provisional layer. `settled` names the transactions whose nullifiers this block carried; their
    provisional entries are released, and nothing else about the provisional layer changes. -/
def confirmBlock (ws : WalletState) (blocks : List ChainBlock) (settled : List Nat) : WalletState :=
  { confirmed := scan blocks
  , provisional :=
      { pendingTransactions := ws.provisional.pendingTransactions.filter (fun t => !settled.contains t)
      , reservedCapabilities :=
          ws.provisional.reservedCapabilities.filter (fun p => !settled.contains p.1) } }

/-- **"Only scanning a block (§2) promotes a provisional fact to a confirmed fact"**, as a statement
    about where the confirmed half comes from: it is `scan blocks`, and no argument of this function is
    provisional. A confirmation path that merged the pending layer into the confirmed state would fail
    this, which is exactly what §6.5 forbids. -/
@[axiom_budget 0]
theorem confirmation_comes_from_blocks_alone (ws : WalletState) (blocks : List ChainBlock)
    (settled : List Nat) :
    (confirmBlock ws blocks settled).confirmed = scan blocks := rfl

/-- And confirmation releases the settled transactions' reservations, by the same filter as
    `dropTransaction`. -/
@[axiom_budget 0]
theorem confirmation_releases_settled_reservations (ws : WalletState) (blocks : List ChainBlock)
    (settled : List Nat) (txId : Nat) (c : Combinatorial.LeafCommitment)
    (h : (txId, c) ∈ ws.provisional.reservedCapabilities) (hs : txId ∈ settled) :
    (txId, c) ∉ (confirmBlock ws blocks settled).provisional.reservedCapabilities := by
  intro hmem
  simp only [confirmBlock] at hmem
  simp_all

/- ==========================================================================
   §6.5's spend-state lifecycle
   ==========================================================================
   `none` is `NULL` — no status recorded, which §6.2's `available` reads as selectable. The four edges
   are the diagram's, named for what causes them: broadcast, the nullifier appearing on-chain, the
   confirmation depth being reached, and the mempool window expiring without the nullifier.
   ========================================================================== -/

/-- §6.5's lifecycle as a transition relation over one capability's status. -/
inductive Step : Option CapStatus → Option CapStatus → Prop where
  /-- Broadcast to the mempool: `NULL → Pending` (`mark_tx_exercise`). -/
  | broadcast : Step none (some CapStatus.pending)
  /-- The nullifier appears on-chain: `Pending → Processing` (`mark_revoked`, in `match_nullifiers`). -/
  | nullifier_observed : Step (some CapStatus.pending) (some CapStatus.processing)
  /-- `CONFIRMATION_DEPTH` reached: `Processing → Spent` (`check_confirmations`). -/
  | confirmed : Step (some CapStatus.processing) (some CapStatus.spent)
  /-- `MEMPOOL_WINDOW` expired with no nullifier: `Pending → NULL` (`expire_pending_caps`). -/
  | expired : Step (some CapStatus.pending) none

/-- **No path reaches `Spent` except through `Processing`** — the state-machine-validity property
    §6.5's diagram encodes and the reason the spend state cannot be forged: `Spent` has exactly one
    incoming edge, and its source is the state the *scan* sets when it observes the nullifier on-chain.
    So "fully spent" is never reached from `NULL` or from `Pending` directly; a capability that has not
    had its nullifier observed is not spent, whatever the wallet's records say. -/
@[axiom_budget 0]
theorem spent_is_entered_only_from_processing (s : Option CapStatus)
    (h : Step s (some CapStatus.spent)) : s = some CapStatus.processing := by
  cases h
  rfl

/-- And `Processing` is entered only by the nullifier observation — the other half of the same claim,
    stated because `Processing` is what §6.5's `revoked` derivation turns on. -/
@[axiom_budget 0]
theorem processing_is_entered_only_by_a_nullifier_observation (s : Option CapStatus)
    (h : Step s (some CapStatus.processing)) : s = some CapStatus.pending := by
  cases h
  rfl

/-- **The two representations of one fact, and the repair §6.5 records.** `CapRecord.revoked` is
    derived from the status — `revoked == (status IS Processing OR status IS Spent)` — so a crash can
    leave the two disagreeing, which is why `check_status_revoked_consistency()` exists in Rust. -/
def derivedRevoked (status : Option CapStatus) : Bool :=
  decide (status = some CapStatus.processing ∨ status = some CapStatus.spent)

/-- A capability record as the deployment stores it: the §6.2 `Held` and the derived `revoked` flag. -/
structure CapRecord where
  held : Held
  revoked : Bool

/-- §6.5's consistency condition between the two representations. -/
def consistent (r : CapRecord) : Prop := r.revoked = derivedRevoked r.held.status

/-- `check_status_revoked_consistency()`: recompute the derived field from the status. -/
def repair (r : CapRecord) : CapRecord := { r with revoked := derivedRevoked r.held.status }

/-- **The repair establishes the invariant** — which is what makes it a repair rather than a rewrite. -/
@[axiom_budget 0]
theorem repair_restores_consistency (r : CapRecord) : consistent (repair r) := rfl

/-- And repairing an already-consistent record changes nothing, so the startup pass is safe to run on
    every start. -/
@[axiom_budget 0]
theorem repair_is_idempotent (r : CapRecord) : repair (repair r) = repair r := rfl

/-- **And the predicate is not vacuous**: a record whose flag disagrees with its status is exactly the
    state the repair exists for, and it is representable. Without this, `repair_restores_consistency`
    would be equally true of a `consistent` that was `True` for everything. -/
@[axiom_budget 0]
theorem an_inconsistent_record_exists : ∃ r : CapRecord, ¬ consistent r :=
  ⟨{ held := { primitives := [], status := some CapStatus.spent }, revoked := false },
   by simp [consistent, derivedRevoked]⟩

/- ==========================================================================
   §6.4.0's commitment-tree obligation, against `PerContractTree.lean`
   ==========================================================================
   §6.4.0: the wallet "SHALL build a commitment's Merkle proof against the **global** native-token
   commitment tree … never against a wallet-local subset tree". `PerContractTree.lean` proves the
   concrete instance of the divergence — a zero-seeded contract tree shifts every non-zero leaf by
   one — and the theorem below is the general one, which is what makes the obligation a class rather
   than an incident: *any* predecessor the local tree omits shifts every later leaf's position.
   ========================================================================== -/

/-- **A wallet-local subset tree puts every later leaf at the wrong position.** If `d` precedes `leaf`
    in the global tree and the local tree omits it, the two positions differ — so a proof built against
    the local tree is at the wrong position for the global root, which is `PerContractTree.lean`'s
    observed inconsistency in the general case. -/
@[axiom_budget 0]
theorem an_omitted_predecessor_shifts_the_position (d : Nat) (mid : List Nat) (leaf : Nat)
    (h : d ≠ leaf) :
    findPos leaf (d :: mid) ≠ findPos leaf mid := by
  simp only [findPos]
  rw [if_neg h]
  omega

end DarkFi.Capability
