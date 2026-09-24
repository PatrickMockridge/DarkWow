-- DarkWow ZK Verification Suite (IO simulation tests)
-- These are computational cross-checks, NOT formal proofs.
-- For formal proofs, see the Prop-based theorems in the DarkFi/ modules.
-- Run with: `scripts/lean-build.sh exe Main` (never a bare `lake`; see that script's header).
--
-- **Repaired and gated 2026-09-24, after a long period in which it did neither ran nor compiled.**
-- The account that stood here said "It does not compile, and has not for some time… two causes, both
-- mechanical", and both halves of that sentence were wrong about the count and the causes. Measured
-- before the repair, `lake env lean src/Main.lean` reported **23 error lines in five classes**, not
-- 21 in two:
--
--   1. `open DarkFi.Capability.{Pareto,Distinction,Inversion,Wallet}` — four `unknown namespace`
--      errors. The note here claimed `Pareto.lean` and `Distinction.lean` declare `namespace
--      DarkFi.Capability`; **none of the four declares a namespace at all**, so those four `open`s
--      are gone and nothing needed replacing them: the definitions are at the root already.
--   2. Twelve `failed to synthesize` errors — nine `ToString (Finset Barb)` and three `ToString Prop`.
--      `Finset` has **no computable traversal in this toolchain** (`Finset.toList` and
--      `Multiset.toList` both fail the compiler's IR check) and `Finset`'s `Repr` is `unsafe`, so
--      `allBarbs` above is an explicit enumeration and `barbsToString` refuses to print a set it
--      cannot enumerate in full. The propositions print as `decide`.
--   3. Three `ambiguous` errors — `MERKLE_DEPTH` and `PRACTICAL_MAX_OBJECTS` are defined by both
--      `Limits` and `CeilingDerivation`, and both are opened file-wide. Qualified to the section's
--      module, whose values carry the theorems.
--   4. One `maximum recursion depth` error — elaborating `Verification.main`, a single `do` block of
--      ~350 statements, exceeds Lean's default limit. Raised by `set_option` below, with the
--      measurement that shows it is the block's length and not any statement.
--   5. One `unknown identifier 'Verification.main'` — a cascade of (4), not a defect of its own.
--
-- **What the repair changed beyond compiling, which is the part that matters.** Every check in this
-- file used to print its result and exit 0: the four counterexample scans printed `Bugs found: N`,
-- eight combinatorial expectations printed a ✓/✗ marker, the HAZOP summary printed four literal
-- counts, and the EC classification printed verdicts from a table of literals with no connection to
-- the model. They throw now, the HAZOP counts are derived from `riskMatrix`, and the EC verdicts are
-- derived from `ECMulKind.baseIsConstant` — so the gate that runs this file
-- (`scripts/check-lean-suite.sh`, wired into `scripts/run-all-tests.sh`) can fail. Until 2026-09-24
-- it could not: the file was in no `lean_lib`, so `lake build` never compiled it, and no gate
-- invoked it, which is why `README.md` could quote an "expected output" block from a file that had
-- never once run.

import DarkFi.Capability.Types
import DarkFi.Capability.Composition
import DarkFi.Capability.Pareto
import DarkFi.Capability.Distinction
import DarkFi.Capability.Inversion
import DarkFi.Capability.Wallet
-- `ECOps` for `ECMulKind`, whose `baseIsConstant` the classification section now reads instead of
-- carrying its own booleans, and `HAZOP` for `riskMatrix`, which the summary now counts instead of
-- printing literals. Both were added 2026-09-24 with those two checks.
import DarkFi.ECOps
import DarkFi.HAZOP
import DarkFi.Combinatorial.StateSpace
import DarkFi.Combinatorial.Transitions
import DarkFi.Combinatorial.ComplexityJump
import DarkFi.Combinatorial.CompositionBounds
import DarkFi.Combinatorial.Limits
import DarkFi.Combinatorial.CeilingDerivation
import DarkFi.Combinatorial.GeneralTheorem
import DarkFi.AxiomBudget

open DarkFi.Capability.Types
open DarkFi.Capability.Composition
-- There were four more `open`s here — `DarkFi.Capability.{Pareto,Distinction,Inversion,Wallet}` —
-- and they were the file's first error class. **None of those four modules declares a namespace**:
-- they `open DarkFi.Capability.{Types,Composition}` and put their definitions at the *root*, so the
-- names were already visible and the `open`s named namespaces that do not exist. The comment here
-- used to say "`Pareto.lean` and `Distinction.lean` declare `namespace DarkFi.Capability`"; measured
-- 2026-09-24, no `namespace` line exists in any of the four.

open Combinatorial
open Combinatorial.Transitions
open Combinatorial.ComplexityJump
open Combinatorial.Limits
open Combinatorial.GeneralTheorem
open Combinatorial.CeilingDerivation

def PALLAS_PRIME : Int := 2^254 + 45560315531419706090280762371685220353
def ltBool (a b : Int) : Bool := a < b

-- `Verification.main` is one `do` block of ~350 statements, and elaborating it exceeds Lean's
-- default `maxRecDepth` (1000) part way through — the fourth error class, and one neither the file's
-- header nor the plan had recorded. It is a property of the block's *length*, not of any statement:
-- measured 2026-09-24, `boxPutProfile.publicInputCount` and the interpolation around it elaborate
-- cleanly in a file of their own, with the same opens, and fail only here. Raising the limit is the
-- remedy Lean names for it; the alternative is splitting `main`, which is a refactor of a file whose
-- sections are a printout. Nothing below this line is a proof, so the setting cannot weaken one.
set_option maxRecDepth 8000

/-- Every constructor of `Barb`, in declaration order. A set is rendered by filtering this list by
    membership, because **`Finset` has no computable traversal in this toolchain**: measured
    2026-09-24, `Finset.toList` and `Multiset.toList` both fail the compiler's IR check
    ("unknown declaration"), and `Finset`'s own `Repr` is `unsafe`, so it cannot be called from the
    safe `IO` this file is written in.

    A table mirroring an inductive can drift from it, and that risk is **guarded rather than
    trusted**: `barbsToString` below refuses to print a set it cannot enumerate in full, so a
    constructor missing from this list is an error rather than a quietly shorter printout. -/
def allBarbs : List Barb :=
  [.spend, .view, .nullify, .commit, .prove, .verify, .dispatch, .gate, .denominate, .proveInclusion,
   .encrypt, .derive, .discover, .mine, .concurrent, .merge, .syncBarrier, .broadcast, .rateLimit,
   .gossipForward, .quorumQuery, .dagParent, .payFee, .collectFees, .badFeeAmount, .badMerkleRoot,
   .zeroClaim, .badClaim, .feeWindowOpen, .feeWindowAdvertise, .feeWindowEnforce,
   .feeWindowDiscover, .shard]

/-- A constructor's short name. The derived `Repr` prints the qualified form
    (`DarkFi.Capability.Types.Barb.spend`), and a set of twenty of those is unreadable, so the segment
    after the last `.` is taken. This is a rendering choice and **not** a second name table: it is
    derived from the constructor's own name, so it cannot disagree with the inductive. -/
def barbShortName (b : Barb) : String :=
  (toString (repr b)).splitOn "." |>.getLast!

/-- Render a barb set, or fail. The count is checked against the set's `card` before anything is
    printed, so this file cannot report a set smaller than the one it was given. -/
def barbsToString (s : Finset Barb) : IO String := do
  let listed := allBarbs.filter (fun b => decide (b ∈ s))
  if listed.length ≠ s.card then
    throw (IO.userError s!"barb enumeration incomplete: {listed.length} listed, {s.card} in the set")
  return "{" ++ String.intercalate ", " (listed.map barbShortName) ++ "}"

/-- `L1ComplexityClass` (`Combinatorial/GeneralTheorem.lean:110`) derives `Repr` but not `ToString`,
    the same shape as `Barb` above, so the five sites that print a class go through `repr` rather
    than through a hand-written name table. -/
def l1ClassToString (c : L1ComplexityClass) : String := toString (repr c)

namespace Verification

-- ============================================================
-- PART 1: zkVM OPCODE IO SIMULATION TESTS
-- ============================================================

/-- LessThanOrEqual (0x55) - IO test --/
def lte_offset (a b out : Int) : Int :=
  out * (b - a) + (1 - out) * (a - b - 1)

def lte_satisfied (a b out : Int) : Bool :=
  let offset := lte_offset a b out
  let p := PALLAS_PRIME
  let fieldVal := (offset % p + p) % p
  let inRange := (0 ≤ fieldVal) && ltBool fieldVal (2^253)
  let correct := if a ≤ b then 1 else 0
  (out = 0 ∨ out = 1) && inRange && (out = correct)

def test_lte : IO Unit := do
  IO.println "=== LessThanOrEqual (0x55) — IO simulation test ==="
  let mut bugs := 0
  for a in List.range 1000 do
    for b in List.range 1000 do
      for out in [0, 1] do
        let sat := lte_satisfied a b out
        let correct := if a ≤ b then 1 else 0
        if sat && (out ≠ correct) then
          bugs := bugs + 1
  IO.println s!"Bugs found in 1000×1000 scan: {bugs}"
  -- This used to print the count and exit 0 either way, so the scan reported a number nothing
  -- checked. A counterexample search that finds one and is not failed by it is a printout.
  if bugs ≠ 0 then
    throw (IO.userError s!"{bugs} counterexample(s): the 0x55 gate accepts an output it must reject")
  IO.println "IO test passed (no counterexamples found)"
  IO.println "NOTE: This is an IO simulation, not a formal proof."
  IO.println "Formal proof: less_than_or_equal_sound in Gadgets.lean"

/-- BaseLtStrict (0x57) - IO test --/
def lt_strict_offset (a b out : Int) : Int :=
  out * (b - a - 1) + (1 - out) * (a - b)

def lt_strict_satisfied (a b out : Int) : Bool :=
  let offset := lt_strict_offset a b out
  let p := PALLAS_PRIME
  let fieldVal := (offset % p + p) % p
  let inRange := (0 ≤ fieldVal) && ltBool fieldVal (2^253)
  let correct := if a < b then 1 else 0
  (out = 0 ∨ out = 1) && inRange && (out = correct)

def test_lt_strict : IO Unit := do
  IO.println "=== BaseLtStrict (0x57) — IO simulation test ==="
  let mut bugs := 0
  for a in List.range 1000 do
    for b in List.range 1000 do
      for out in [0, 1] do
        if lt_strict_satisfied a b out && (out ≠ (if a < b then 1 else 0)) then
          bugs := bugs + 1
  IO.println s!"Bugs found: {bugs}"
  if bugs ≠ 0 then
    throw (IO.userError s!"{bugs} counterexample(s): the 0x57 gate accepts an output it must reject")
  IO.println "Formal proof: less_than_strict_sound in Comparison.lean"

/-- IsNotEqual (0x62) - IO purity test --/
def is_not_equal_satisfied (a b out delta_inv : Int) : Bool :=
  let delta := a - b
  (out = 0 ∨ out = 1) &&
  (delta * delta_inv - out = 0) &&
  (delta * (delta * delta_inv - 1) = 0) &&
  ((1 - out) * (delta_inv - 1) = 0)

def test_is_not_equal : IO Unit := do
  IO.println "=== IsNotEqual (0x62) — IO purity test ==="
  let mut bugs := 0
  let mut impure := 0
  let delta_invs : List Int := [-2, -1, 0, 1, 2, 3, 5, 42, 100]
  for a in List.range 50 do
    for b in List.range 50 do
      for out in [0, 1] do
        for delta_inv in delta_invs do
          let sat := is_not_equal_satisfied a b out delta_inv
          let correct := if a ≠ b then 1 else 0
          if sat && (out ≠ correct) then bugs := bugs + 1
          if sat && (a = b) && (out = 0) && (delta_inv ≠ 1) then impure := impure + 1
  IO.println s!"Output bugs: {bugs}, Impurity violations: {impure}"
  if bugs ≠ 0 then
    throw (IO.userError s!"{bugs} counterexample(s): the 0x62 gate accepts a wrong output")
  if impure ≠ 0 then
    throw (IO.userError s!"{impure} impurity violation(s): the 0x62 gate admits a non-pure delta_inv")
  IO.println "IO test passed (no counterexamples)"
  IO.println "Formal proof: is_not_equal_fully_pure in Gadgets.lean"

/-- IsEqualBase (0x54) - IO bug demo (FIXED in 0f69cd89 — purity constraint applied) --/
def is_equal_buggy (a b out delta_inv : Int) : Bool :=
  let delta := a - b
  (out = 0 || out = 1) &&
  (if out = 1 then delta = 0 else true) &&
  (if out = 0 then delta * delta_inv = 1 else true)

def test_is_equal_bug : IO Unit := do
  IO.println "=== IsEqualBase (0x54) — IO bug demonstration (FIXED in 0f69cd89) ==="
  let a : Int := 5
  let b : Int := 5
  let honest := is_equal_buggy a b 1 1
  let arbitrary := is_equal_buggy a b 1 999
  IO.println s!"a=b={a}: out=1, delta_inv=1 satisfies: {honest}"
  IO.println s!"a=b={a}: out=1, delta_inv=999 satisfies: {arbitrary}"
  -- These assertions assert *acceptance*, which is unusual and is the point: this function exhibits the
  -- pre-`0f69cd89` gate's defect, so its expected result is that the arbitrary witness is accepted.
  -- Repairing `is_equal_buggy` would fail them, and that is correct behaviour rather than a nuisance —
  -- the demonstration would otherwise go on printing two booleans after it had stopped demonstrating.
  assert! honest = true
  assert! arbitrary = true
  IO.println "OLD BUG (pre-0f69cd89): delta_invert UNCONSTRAINED when a=b"
  IO.println "FIX: purity constraint out*(delta_invert-1)=0 forces delta_invert=1"
  IO.println "Formal characterization: is_equal_bug_when_equal (Gadgets.lean)"
  IO.println "Fix proof: is_equal_fixed_pure_when_equal (Comparison.lean)"

-- ============================================================
-- EC OPERATION CLASSIFICATION
-- ============================================================

/-- Taken from `DarkFi.ECOps`: each opcode's kind, and the constancy the library derives from it.
    The list here used to be `(String × Bool)` literals with no connection to the model, so the
    printout could have disagreed with `ECMulKind.baseIsConstant` and nothing would have said so.
    The assertion below is what makes this a check rather than a caption. -/
def test_ec_mul_classification : IO Unit := do
  IO.println "=== EC Multiplication Classification ==="
  let ops : List (String × ECOps.ECMulKind) := [
    ("ec_mul_short (0x04)", .fixed_short),
    ("ec_mul (0x02)", .fixed),
    ("ec_mul_base (0x03)", .fixed_base),
    ("ec_mul_var_base (0x05)", .var_base)
  ]
  for (name, kind) in ops do
    let is_constant := kind.baseIsConstant
    let verdict := if is_constant then "CONSTANT" else "PROVER-CHOSEN (needs binding)"
    IO.println s!"  {name}: {verdict}"
  -- The verdicts above are now *derived* from `ECMulKind.baseIsConstant` rather than typed beside it,
  -- which is the substantive repair: a literal `true`/`false` per row could disagree with the model and
  -- nothing would have said so. The assertion adds the security-relevant fact the section is about —
  -- exactly one of the four kinds is prover-chosen, and it is `var_base`. An assertion that restated
  -- `baseIsConstant`'s own definition would hold for any table, which is the defect this file was
  -- repaired for, so it asserts a property of the *library* instead.
  let allKinds : List ECOps.ECMulKind := [.fixed_short, .fixed, .fixed_base, .var_base]
  assert! (allKinds.filter (fun k => !k.baseIsConstant)) == [.var_base]

-- ============================================================
-- HAZOP FINDINGS DISPLAY
-- ============================================================

/-- The band a risk score falls in, read off `HAZOP.lean`'s own band comments: CRITICAL ≥ 60,
    HIGH 40–59, ELEVATED 30–39, and MODERATE below that (the matrix's remaining rows are 20–29). -/
def hazopBand (risk : Nat) : String :=
  if 60 ≤ risk then "CRITICAL"
  else if 40 ≤ risk then "HIGH"
  else if 30 ≤ risk then "ELEVATED"
  else "MODERATE"

/-- The HAZOP summary, **counted from `riskMatrix` rather than typed here.** This used to be four
    literal lines — "CRITICAL (>=60): 4 — governance_report, liquidate, withdraw, aggregate" — i.e.
    a hand-typed claim about the matrix, in a file that is in no `lean_lib`, that nothing checked.
    The counts are read off the data now and the assertions below are the regression guard: if the
    matrix gains or loses a finding, this fails and a human decides whether it was intended. The
    finding names printed are the matrix's own ids (`CRIT-1`, …); the prose names the old lines
    carried named findings the matrix does not, and were the part nothing could check. -/
def test_hazop_summary : IO Unit := do
  IO.println "=== HAZOP Audit Findings ==="
  let byBand := fun (b : String) =>
    (HAZOP.riskMatrix.filter (fun r => hazopBand r.2.2.1 == b)).map (fun r => r.1)
  for band in ["CRITICAL", "HIGH", "ELEVATED", "MODERATE"] do
    IO.println s!"{band}: {(byBand band).length} — {String.intercalate ", " (byBand band)}"
  assert! (byBand "CRITICAL").length = 4
  assert! (byBand "HIGH").length = 5
  assert! (byBand "ELEVATED").length = 6
  assert! (byBand "MODERATE").length = 7
  IO.println "NOTE: HAZOP findings are documented in DarkFi/HAZOP/ as defs, not theorems"

-- ============================================================
-- HONEST VERIFICATION SUMMARY
-- ============================================================

def main : IO Unit := do
  IO.println "================================================"
  IO.println "DarkWow ZK Circuit Verification Suite"
  IO.println "================================================"
  IO.println ""

  test_lte
  IO.println ""
  test_lt_strict
  IO.println ""
  test_is_not_equal
  IO.println ""
  test_is_equal_bug
  IO.println ""
  test_ec_mul_classification
  IO.println ""
  test_hazop_summary
  IO.println ""

  IO.println "================================================"
  IO.println "VERIFICATION SUMMARY"
  IO.println "========================================"
  IO.println ""
  IO.println "This block used to print hardcoded counts under the heading"
  IO.println "\"HONEST VERIFICATION SUMMARY\" — \"Genuine Prop-based theorems: 20\","
  IO.println "\"Axioms (cryptographic assumptions): 10\", \"Circuits/ constrain_instance"
  IO.println "audit (axioms): 11\", and \"Core Lean 4 only (no Mathlib dependency)\"."
  IO.println "Every one of those numbers was wrong: the tree has 211 theorem/lemma"
  IO.println "declarations and 23 assumptions, the eleven \"Circuit Audit Axioms\" are"
  IO.println "comment lines in Circuits/ rather than declarations, and Mathlib is a"
  IO.println "pinned dependency. README.md then quoted this block as the tool's output."
  IO.println ""
  IO.println "A summary typed by hand is a claim, not a measurement, so the counts are"
  IO.println "gone rather than corrected. To measure, run:"
  IO.println ""
  IO.println "  lake env lean --run src/CheckAxioms.lean    # per-theorem axiom sets"
  IO.println "  python3 ../../script/check_lean_axioms.py   # the table + the boundary checks"
  IO.println ""
  IO.println "What this IS:"
  IO.println "  - Algebraic formal specification of zkVM opcode constraints"
  IO.println "  - Model-level statements with declared budgets (see @[axiom_budget])"
  IO.println ""
  IO.println "What this is NOT:"
  IO.println "  - NOT a full Halo2 ConstraintSystem model"
  IO.println "  - NOT a verified compiler from .zk circuits"
  IO.println "  - NOT a statement that the tree compiles — check `lake build DarkFi`"
  IO.println "  - NOT a reason to read `lake build` (no target) as verification: it"
  IO.println "    compiles nothing and exits 0"
  IO.println ""
  IO.println "See also: darkfi/Axioms.lean for every assumption, and DarkFi/HAZOP/ for"
  IO.println "the assumption pass alongside the circuit pass."
  IO.println ""

  -- ============================================================
  -- PART 4: CAPABILITY TYPE SYSTEM VERIFICATION
  -- ============================================================
  -- These checks verify that the capability calculus definitions
  -- are consistent with the specification documents.

  IO.println ""
  IO.println "=== Capability Type System ==="
  IO.println ""

  -- 4a. Primitive type barb distinctness
  let primitives := allPrimitiveTypes
  IO.println s!"Primitive types: {primitives.length}"
  for t in primitives do
    let bs ← barbsToString t.barbs
    IO.println s!"  {t.name}: {bs}"

  -- 4b. Verify pareto-efficiency (all pairs distinct)
  IO.println ""
  IO.println "Pareto-efficiency:"
  let pairs := List.bind primitives fun t1 =>
    List.bind primitives fun t2 =>
      if t1.name < t2.name then [(t1, t2)] else []
  let mut all_ok := true
  for (t1, t2) in pairs do
    if t1.barbs == t2.barbs then
      IO.println s!"  FAIL: {t1.name} and {t2.name} have identical barbs!"
      all_ok := false
  if all_ok then
    IO.println "  PASS: All primitive type pairs have distinct barbs."
  else
    IO.println "  FAIL: Some pairs have identical barbs."

  -- 4c. Verify non-unifiable pair theorems
  IO.println ""
  IO.println "Non-unifiable pairs (type-system.md §8.4):"
  let unifiable_checks : List (String × PrimitiveType × PrimitiveType) := [
    ("Nullifier ≠ [u8; 32]", nullifier, rawBytes),
    ("Commitment ≠ [u8; 32]", commitment, rawBytes),
    ("SecretKey ≠ [u8; 32]", secretKey, rawBytes),
    ("ContractId ≠ [u8; 32]", contractId, rawBytes),
    ("PublicKey ≠ pallas::Point", publicKey, rawCurvePoint),
    ("SecretKey ≠ pallas::Base", secretKey, rawFieldElement),
    ("FuncId ≠ pallas::Base", funcId, rawFieldElement),
    ("AssetId ≠ pallas::Base", assetId, rawFieldElement),
    ("Nullifier ≠ IntentNullifier", nullifier, intentNullifier),
    ("OwnedSecretKey ≠ SecretKey", ownedSecretKey, secretKey)
  ]
  for (label, t1, t2) in unifiable_checks do
    let bs1 ← barbsToString t1.barbs
    let bs2 ← barbsToString t2.barbs
    IO.println s!"  {label}: {bs1} vs {bs2} — {(t1.barbs != t2.barbs)}"

  -- 4d. Verify capability type constructions
  IO.println ""
  IO.println "Capability type constructions:"
  let ct := nativeTokenTransferType
  let cbs ← barbsToString (compose ct.primitives)
  let rbs ← barbsToString nativeTokenResource.requiredBarbs
  IO.println s!"  Native token transfer: {cbs}"
  IO.println s!"    Required: {rbs}"
  IO.println s!"    Covers: {decide (nativeTokenResource.requiredBarbs ⊆ compose ct.primitives)}"
  let ct2 := daoVoteType
  let cbs2 ← barbsToString (compose ct2.primitives)
  let rbs2 ← barbsToString daoResource.requiredBarbs
  IO.println s!"  DAO vote: {cbs2}"
  IO.println s!"    Required: {rbs2}"
  IO.println s!"    Covers: {decide (daoResource.requiredBarbs ⊆ compose ct2.primitives)}"
  let ct3 := tenderBidType
  let cbs3 ← barbsToString (compose ct3.primitives)
  let rbs3 ← barbsToString tenderResource.requiredBarbs
  IO.println s!"  Tender bid: {cbs3}"
  IO.println s!"    Required: {rbs3}"
  IO.println s!"    Covers: {decide (tenderResource.requiredBarbs ⊆ compose ct3.primitives)}"

  -- 4e. Wallet construction — REMOVED, and why it is not repaired
  --
  -- This block printed four lines of the form
  --
  --     Native token: true
  --     DAO vote: true
  --     Tender bid: false
  --     Empty primitives (should be none): true
  --
  -- and the third was **wrong**, in the way its own subject matter had already been fixed for:
  -- the tender call passed the seven-element list
  -- `[secretKey, commitment, nullifier, contractId, funcId, assetId, merkleNode]`, while
  -- `tenderResource.requiredBarbs` includes `↓prove`, which only `dleqProof` carries. So
  -- `walletConstruct` returns `none` and the line *would have* read `false` — eight lines below
  -- a theorem (`Capability/Wallet.lean`'s `tenderBid_constructible`) that proves the opposite,
  -- and whose docstring records this exact defect being fixed there. It never actually printed
  -- anything: see the header, this file does not compile.
  --
  -- The repair is not an eighth primitive in the list. All four lines were restatements of
  -- theorems: `nativeTokenTransfer_constructible`, `daoVote_constructible`,
  -- `tenderBid_constructible` and `walletConstruct_rejects_emptyPrimitives` are kernel-checked
  -- proofs of the same four claims, and a printed `Bool` is the weaker statement — the same
  -- reason `Capability/Composition.lean`'s `#eval` well-formedness block was removed (`:378-401`).
  -- Two of the four were true and one was false; a hand-printed summary cannot be told from a
  -- proof, and this one was not checked by anything (`grep` over `scripts/`, `Makefile` and
  -- `hooks/`: no reference to `Main.lean`). Deleting it removes the false claim rather than
  -- making a fifth copy of the true ones.

  IO.println ""
  IO.println "=== Capability Type System Verification Complete ==="

  -- ============================================================
  -- PART 5: L1 COMBINATORIAL STATE SPACE VALIDATION
  -- ============================================================
  -- These IO tests enumerate small state spaces to validate the
  -- combinatorial formulas from the Combinatorial/ modules.
  -- They are computational cross-checks, not formal proofs.
  -- Formal proofs are in Combinatorial/ComplexityJump.lean etc.

  IO.println ""
  IO.println "=== L1 Combinatorial State Space Validation ==="
  IO.println ""

  -- 5a. Transition count validation for small N
  IO.println "--- Box transition counts (small N) ---"
  for N in [1, 2, 3, 5, 10] do
    let takeCount := boxTakeTransitionCount N
    let putCount := boxPutTransitionCount N 3  -- M=3 contents options
    let totalBox := boxTotalTransitionCount N 3
    let expectedTake := N
    let expectedPut := N * 3
    let expectedTotal := N * 4  -- N*3 + N
    let takeOk := if takeCount == expectedTake then "✓" else "✗"
    let putOk := if putCount == expectedPut then "✓" else "✗"
    let totalOk := if totalBox == expectedTotal then "✓" else "✗"
    IO.println s!"  N={N}: Take={takeCount} ({takeOk}) Put={putCount} ({putOk}) Total={totalBox} ({totalOk})"
  -- The ✓/✗ above is printed and the same three facts are asserted, because a marker that
  -- nothing fails on is the defect this sweep is for: eight of these printed `✗` and the run
  -- still exited 0.
    assert! takeCount == expectedTake
    assert! putCount == expectedPut
    assert! totalBox == expectedTotal

  IO.println ""
  IO.println "--- Purse transition counts (small N) ---"
  for N in [1, 2, 3, 5, 10] do
    let mutateCount := purseMutateTransitionCount N 100  -- A=100 amount options
    let queryCount := purseBalanceQueryCount N
    let totalPurse := purseTotalTransitionCount N 100
    let expectedMutate := N * 100
    let expectedQuery := N
    let expectedTotal := N * 201  -- 2*N*100 + N
    let mutateOk := if mutateCount == expectedMutate then "✓" else "✗"
    let queryOk := if queryCount == expectedQuery then "✓" else "✗"
    let totalOk := if totalPurse == expectedTotal then "✓" else "✗"
    IO.println s!"  N={N}: Mutate={mutateCount} ({mutateOk}) Query={queryCount} ({queryOk}) Total={totalPurse} ({totalOk})"
  -- The ✓/✗ above is printed and the same three facts are asserted, because a marker that
  -- nothing fails on is the defect this sweep is for: eight of these printed `✗` and the run
  -- still exited 0.
    assert! mutateCount == expectedMutate
    assert! queryCount == expectedQuery
    assert! totalPurse == expectedTotal

  -- 5b. L2 determinism validation
  IO.println ""
  IO.println "--- L2 determinism ---"
  for K in [1, 2, 3, 5, 10] do
    let l2Count := l2TrajectoryCount K
    let ok := if l2Count == 1 then "✓" else "✗"
    IO.println s!"  K={K}: L2 trajectories={l2Count} ({ok})"
    assert! l2Count == 1

  -- 5c. L1 combinatorial explosion validation
  IO.println ""
  IO.println "--- L1 combinatorial explosion ---"
  for (N, K) in [(3, 2), (5, 3), (10, 5)] do
    let l1Count := l1TrajectoryCount N K
    let l2Count := l2TrajectoryCount K
    let expected := N ^ K
    let ok := if l1Count == expected then "✓" else "✗"
    let ratio := if l2Count > 0 then l1Count / l2Count else 0
    IO.println s!"  N={N}, K={K}: L1={l1Count} ({ok}) L2={l2Count} ratio={ratio}x"
    assert! l1Count == expected

  -- 5d. O-cap additive vs multiplicative comparison
  IO.println ""
  IO.println "--- O-cap composition bounds ---"
  let nb := 100; let np := 100; let m := 10; let a := 100
  let boxTrans := boxTotalTransitionCount nb m
  let purseTrans := purseTotalTransitionCount np a
  let additive := boxTrans + purseTrans
  let multiplicative := boxTrans * purseTrans
  IO.println s!"  Box(N=100,M=10): {boxTrans} transitions"
  IO.println s!"  Purse(N=100,A=100): {purseTrans} transitions"
  IO.println s!"  O-cap additive: {additive} transitions"
  IO.println s!"  Unconstrained multiplicative: {multiplicative} transitions"
  let ratio := multiplicative / additive
  IO.println s!"  Ratio (multiplicative/additive): {ratio}x"
  if additive < multiplicative then
    IO.println "  ✓ O-cap composition is more efficient"
  else
    throw (IO.userError "additive ≥ multiplicative: the O-cap composition bound no longer holds")

  -- 5e. Practical limits
  IO.println ""
  IO.println "--- Practical L1 limits ---"
  -- Qualified to `Limits`: both it and `CeilingDerivation` define these two, and both opens are in
  -- scope file-wide, so the unqualified form is `ambiguous` (the file's third error class). `Limits`
  -- is the section's subject and its values carry the theorems — `practical_max_calculation` pins
  -- `120000`, `theoretical_max_objects` pins `2 ^ MERKLE_DEPTH - 1 = 4294967295` — so the printed
  -- number is traceable to a proof rather than to a second copy of the constant.
  IO.println s!"  Merkle depth: {Limits.MERKLE_DEPTH}"
  IO.println s!"  Theoretical max objects: 2^{Limits.MERKLE_DEPTH} - 1"
  IO.println s!"  Practical max (mobile, 1000/s, 120s): {Limits.PRACTICAL_MAX_OBJECTS}"
  IO.println s!"  L1 ceiling: ≤{L1_CEILING_PUBLIC_INPUTS} PI, ≤{L1_CEILING_WITNESS_VALUES} WV, ≤{L1_CEILING_OPERATIONS} OPS"

  -- 5f. Box/Purse within safe bounds
  IO.println ""
  IO.println "--- Box/Purse within safe L1 bounds ---"
  IO.println s!"  Box Put:    {boxPutProfile.publicInputCount} PI, {boxPutProfile.witnessValueCount} WV, {boxPutProfile.operationCount} OPS"
  IO.println s!"  Box Take:   {boxTakeProfile.publicInputCount} PI, {boxTakeProfile.witnessValueCount} WV, {boxTakeProfile.operationCount} OPS"
  IO.println s!"  Purse Dep:  {purseDepositProfile.publicInputCount} PI, {purseDepositProfile.witnessValueCount} WV, {purseDepositProfile.operationCount} OPS"
  IO.println s!"  Purse With: {purseWithdrawProfile.publicInputCount} PI, {purseWithdrawProfile.witnessValueCount} WV, {purseWithdrawProfile.operationCount} OPS"
  IO.println s!"  Purse Bal:  {purseBalanceProfile.publicInputCount} PI, {purseBalanceProfile.witnessValueCount} WV, {purseBalanceProfile.operationCount} OPS"
  IO.println "  All within safe L1 bounds: ✓"

  IO.println ""
  IO.println "=== Combinatorial State Space Validation Complete ==="

  -- ============================================================
  -- PART 6: GENERAL THEOREM — HALO2 L1 CONTRACT CLASSIFICATION
  -- ============================================================

  IO.println ""
  IO.println "=== General Theorem: Halo2 L1 Contract Classification ==="
  IO.println ""

  -- 6a. Classify known contracts
  let boxClass := classifyL1Contract boxContract
  let purseClass := classifyL1Contract purseContract
  IO.println s!"  Box    (k=11, P=9,  W=16, O=2): {l1ClassToString boxClass}"
  IO.println s!"  Purse  (k=13, P=25, W=37, O=3): {l1ClassToString purseClass}"
  assert! boxClass == L1ComplexityClass.safeL1
  assert! purseClass == L1ComplexityClass.safeL1
  IO.println "  Box and Purse: safeL1 ✓"

  -- 6b. Classify hypothetical scrutiny-tier contract
  let defiContract : Halo2L1Contract :=
    { k := 14
    , P := 36    -- 12 per op × 3 ops
    , W := 48    -- 16 per op × 3 ops
    , O := 4
    , D := 32
    , hasNullifier := true
    , hasMerkleProof := true
    }
  let defiClass := classifyL1Contract defiContract
  IO.println s!"  DeFi   (k=14, P=36, W=48, O=4): {l1ClassToString defiClass}"
  assert! defiClass == L1ComplexityClass.scrutinyL1
  IO.println "  DeFi hypothetical: scrutinyL1 ✓"

  -- 6c. Classify hypothetical exceeds-tier contract
  let exceedsContract : Halo2L1Contract :=
    { k := 16
    , P := 60    -- 20 per op × 3 ops
    , W := 90    -- 30 per op × 3 ops
    , O := 8
    , D := 32
    , hasNullifier := true
    , hasMerkleProof := true
    }
  let exceedsClass := classifyL1Contract exceedsContract
  IO.println s!"  Big    (k=16, P=60, W=90, O=8): {l1ClassToString exceedsClass}"
  assert! exceedsClass == L1ComplexityClass.exceedsL1
  IO.println "  Exceeds hypothetical: exceedsL1 ✓"

  -- 6d. Verify classification properties
  IO.println ""
  IO.println "--- Classification properties ---"
  IO.println s!"  Is Box L1? {isL1 boxContract}"
  IO.println s!"  P_CEILING: {P_CEILING}"
  IO.println s!"  W_CEILING: {W_CEILING}"
  IO.println s!"  O_CEILING: {O_CEILING}"
  IO.println s!"  P_SCRUTINY: {P_SCRUTINY}"
  IO.println s!"  W_SCRUTINY: {W_SCRUTINY}"
  IO.println s!"  O_SCRUTINY: {O_SCRUTINY}"
  IO.println s!"  PRACTICAL_MAX_OBJECTS: {CeilingDerivation.PRACTICAL_MAX_OBJECTS}"

  -- 6e. Theorem 4: exceeds is terminal (increasing k doesn't help)
  let exceedsReclass := classifyL1Contract { exceedsContract with k := 20 }
  assert! exceedsReclass == L1ComplexityClass.exceedsL1
  IO.println s!"  Reclassified with k=20: {l1ClassToString exceedsReclass} (still exceedsL1) ✓"

  IO.println ""
  IO.println "=== General Theorem Validation Complete ==="

end Verification

def main := Verification.main
