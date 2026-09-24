/-!
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.
-/
/-!
# HAZOP HIGH Tier — Targeted Vulnerability Proofs (Risk 40-59)

## HIGH-1/2: burn_v2.zk zero_cond Merkle Bypass (Risk 42/100)
When commitment_value = 0, zero_cond returns 0 instead of the commitment hash. The Merkle proof
verifies against the tree's zero leaf, not the actual commitment.

## HIGH-3: labor_market/refund_v2.zk Refund Check Skip (Risk 42/100)
For non-milestone jobs, the refund amount constraint is vacuous — the guard
passes trivially for any refund_amount.

## HIGH-4: labor_market Nullifier Collision (Risk 40/100)
confirm_delivery, milestone_payment, and refund all use H(job_id, employer_secret).
Any one action consumes the nullifier for all three.

## HIGH-5: governance_report less_than_strict on zero values (Risk 42/100)
-/

namespace HAZOP.High

-- ===========================================================================
-- HIGH-1/2: burn_v2.zk — zero_cond Merkle Bypass
-- ===========================================================================

/-
Models the zero_cond gate used in burn_v2.zk (both PN and NT variants).

zero_cond(a, b):
  If a = 0: returns 0 (the value of `a`)
  If a ≠ 0: returns b

In burn circuits: commitment_incl = zero_cond(commitment_value, commitment)
  If commitment_value = 0: commitment_incl = 0 (the ZERO LEAF of the Merkle tree)
  If commitment_value ≠ 0: commitment_incl = commitment (the actual commitment hash)

The Merkle proof then verifies: merkle_root(pos, path, commitment_incl) = root

ATTACK: Mallory sets commitment_value = 0. Then commitment_incl = 0.
The Merkle proof now verifies that the ZERO LEAF is in the tree at position pos.
The zero leaf exists at EVERY empty position in the tree.
Mallory proves "inclusion" of a leaf that is trivially present everywhere.
-/

/-
THEOREM (HIGH-1): When commitment_value = 0, the Merkle proof is vacuous.

The circuit proves: merkle_root(pos, path, 0) = root

Since the zero leaf (value 0) is the default value for all empty positions
in a Merkle tree, this proof is trivially satisfiable for ANY position.
Mallory does NOT need a real commitment — any position in the tree with a zero
leaf satisfies the proof.

Combined with a nullifier = poseidon_hash(secret, 0x00), Mallory creates
a valid burn proof for a non-existent commitment.
-/
structure BurnV1ZeroCondAttack where
  commitment_value : Int    -- Mallory sets this to 0
  commitment_hash : Int     -- Any value; zero_cond ignores it when commitment_value = 0
  commitment_incl : Int     -- = zero_cond(commitment_value, commitment_hash) = 0
  leaf_pos : Int      -- Any position with a zero leaf
  path : List (Int × Int) -- Any path to a zero leaf position

/-
THEOREM (HIGH-1): The attack works because zero_cond returns 0 when value = 0.

Mallory's attack:
  1. Set commitment_value = 0 (free witness)
  2. Set commitment_hash to any value (free witness — zero_cond ignores it)
  3. commitment_incl = zero_cond(0, any_commitment) = 0
  4. Choose any leaf_pos where the Merkle tree has a zero leaf (all empty positions)
  5. Provide path to that position
  6. merkle_root(pos, path, 0) = actual_tree_root (always true for empty positions)
  7. nullifier = poseidon_hash(secret, 0x00) — creates a nullifier for a commitment that doesn't exist

The entrypoint then marks the nullifier as spent. The "burned" commitment never existed.
-/
def burn_v1_zero_cond_bypass : String :=
  "HIGH-1/2: zero_cond(0,commitment)=0; Merkle proof against zero leaf; non-existent commitment burn"

/-
THEOREM (HIGH-1 fix): Require commitment_value > 0.

Add to circuit:
  less_than_strict(ZERO, commitment_value)  -- or equivalently range_check with a non-zero check

This ensures commitment_value ≥ 1, so zero_cond always returns the actual commitment hash.
The Merkle proof then verifies a real commitment, not the zero leaf.

Alternative fix: Remove zero_cond entirely and enforce commitment_value > 0.
The zero_cond gate exists to handle dummy inputs in batch operations.
If the entrypoint enforces at least one real input, zero_cond is unnecessary.
-/
def burn_v1_zero_cond_fix : String := "less_than_strict(ZERO, commitment_value) before zero_cond"

/-
THEOREM (HIGH-2): Same attack applies to native_token/burn_v2.zk.

The NT burn circuit has identical structure: zero_cond(commitment_value, commitment) at line 70.
The attack and fix are identical.
-/

-- ===========================================================================
-- HIGH-3: labor_market/refund_v2.zk — Refund Amount Check Skipped
-- ===========================================================================

/-
Models the refund_v2.zk milestone logic.

The circuit computes:
  has_milestones = is_not_equal(milestone_count, ZERO)   -- boolean
  payment_remaining = base_sub(total_payment, completed_payment)  -- field sub, wraps around
  expected_refund = cond_select(payment_remaining, total_payment, has_milestones)
  refund_match = is_equal_base(expected_refund, refund_amount)    -- 1 if match, 0 otherwise
  milestone_refund_valid = base_mul(has_milestones, refund_match) -- has_milestones * refund_match
  refund_check_ok = is_equal_base(milestone_refund_valid, has_milestones)
  constrain_equal_base(refund_check_ok, ONE)

BUG: When has_milestones = 0 (no milestones):
  expected_refund = cond_select(payment_remaining, total_payment, 0) = total_payment
  refund_match = is_equal_base(total_payment, refund_amount)
  milestone_refund_valid = 0 * refund_match = 0
  refund_check_ok = is_equal_base(0, 0) = 1
  constrain_equal_base(1, 1) → PASSES

The guard `milestone_refund_valid == has_milestones` is 0 == 0 → TRUE.
The refund_amount check is COMPLETELY BYPASSED.
-/

/-
THEOREM (HIGH-3): For non-milestone jobs, refund_amount is unconstrained.

Mallory sets milestone_count = 0 (no milestones). Then:
  has_milestones = 0
  milestone_refund_valid = 0 * refund_match = 0
  refund_check_ok = is_equal_base(0, 0) = 1

The constraint passes regardless of refund_amount. Mallory can claim
ANY refund amount — even exceeding the total payment.
-/
def test_refund_bypass (milestone_count expected_refund refund_amount : Int) : Bool :=
  let has_milestones := if milestone_count = 0 then 0 else 1
  let refund_match := if expected_refund = refund_amount then 1 else 0
  let milestone_refund_valid := has_milestones * refund_match
  -- When has_milestones = 0: milestone_refund_valid = 0, check passes trivially
  milestone_refund_valid = has_milestones

/-
THEOREM (HIGH-3 fix): For non-milestone jobs, directly constrain the refund amount.

Replace the milestone-based guard with a direct equality:
  constrain_equal_base(refund_amount, total_payment)

Or add a branch:
  if has_milestones == 0:
    constrain_equal_base(refund_amount, total_payment)
  else:
    constrain_equal_base(refund_amount, base_sub(total_payment, completed_payment))
-/

-- ===========================================================================
-- HIGH-4: labor_market — Nullifier Collision Across Circuits
-- ===========================================================================

/-
Three labor_market circuits use the same nullifier derivation:
  confirm_delivery_v1:  nullifier = H(job_id, employer_secret)
  milestone_payment_v1: nullifier = H(job_id, employer_secret)
  refund_v1:             nullifier = H(job_id, employer_secret)

All three produce identical nullifiers for the same (job_id, employer_secret).
The first action to be processed consumes the nullifier. The other two actions
can never be performed because the nullifier is already marked spent.

This is a LIVENESS bug: performing one action blocks the others.
-/

/-
Simulated Poseidon hash for nullifier derivation testing.
-/
def sim_nullifier (job_id secret : Int) : Int := job_id * 12345 + secret * 67890 + 777

/-
THEOREM (HIGH-4): All three circuits produce identical nullifiers.

For the same (job_id, employer_secret), the nullifiers are byte-for-byte identical.
-/
def labor_nullifier_collision : String :=
  "HIGH-4: confirm/milestone/refund all use H(job_id,secret); consuming one nullifier blocks all three"

/-
THEOREM (HIGH-4 fix): Add a domain separator to each nullifier derivation.

  confirm_delivery:  nullifier = H(b"confirm", job_id, employer_secret)
  milestone_payment: nullifier = H(b"milestone", job_id, employer_secret)
  refund:             nullifier = H(b"refund", job_id, employer_secret)

This ensures each action has a unique nullifier even for the same (job, secret).
-/
def labor_nullifier_fix : String :=
  "nullifier = poseidon_hash(action_domain_separator, job_id, employer_secret)"

/-
THEOREM (HIGH-4): The collision is a real operational bug.

If an employer confirms delivery, they can NEVER make a milestone payment.
If an employer requests a refund, they can NEVER confirm delivery.
The job lifecycle becomes broken after any single action is taken.

Since labor_market is designed for iterative work (multiple milestones),
this bug makes the milestone feature unusable.
-/

-- ===========================================================================
-- HIGH-5: governance_report — less_than_strict on zero-division values
-- ===========================================================================

/-
When total_debt = 0 (free witness), collateral_ratio_bps = 0 (division by zero).
Then less_than_strict(15000, 0) compares 15000 < 0 in the field.
In field arithmetic: 0 ≡ p (mod p), so 15000 < p ≈ 2^254, which is TRUE.

Wait — LESS_THAN in the field: is 15000 < 0 in F_p?
0 in the field is the element 0. 15000 is the element 15000.
In the field: 15000 < 0 is FALSE (15000 is greater than 0).
But in field arithmetic with range_check interpretation, the comparison
is done on the integer representation modulo p.

The actual behavior depends on how `less_than_strict` interprets field elements.
If it treats them as integers in [0, p), then:
  15000 < 0 → 15000 < p → TRUE (since 0 ≡ p in the field)

This means: when total_debt = 0, the "ratio > 150%" check PASSES.
A governance report with zero debt and the minimum ratio threshold
would be accepted.
-/
def governance_report_less_than_strict_zero : String :=
  "HIGH-5: total_debt=0 → ratio=0 → less_than_strict(15000,0); division-by-zero produces undefined check"

-- ===========================================================================
-- HAZOP RISK VERIFICATION
-- ===========================================================================

def highFindings : List (String × Nat × String) := [
  ("HIGH-1: PN burn_v1 zero_cond bypass", 42,
   "zero_cond(0, commitment) returns 0; Merkle proof against zero leaf"),
  ("HIGH-2: NT burn_v1 zero_cond bypass", 42,
   "Same zero_cond pattern as PN burn_v1"),
  ("HIGH-3: labor refund amount check skipped", 42,
   "Non-milestone jobs: guard 0*refund_match == 0 passes trivially"),
  ("HIGH-4: labor nullifier collision", 40,
   "confirm/milestone/refund all use H(job_id, secret); one blocks others"),
  ("HIGH-5: governance_report zero-division threshold", 42,
   "less_than_strict on ratio=0 from total_debt=0")
]

-- ===========================================================================
-- ASSUMPTIONS AND REMOVED CLAIMS WHERE SOMETHING IS AT STAKE (HIGH-6 onwards)
-- ===========================================================================
--
-- Same guideword as `DarkFi.HAZOP.Elevated`: if this is false, does anything fail loudly or
-- silently? These are the ones where the answer is not a flat "silently" — either because a
-- theorem would break, or because the claim was carrying weight in the docs and is gone.

/-- HIGH-6: `pedersen_additive_homomorphism`. This is the one assumption whose falsity has a
    named victim: `SupplyChain.supply_chain_invariant`, `no_hidden_inflation` and
    `cumulative_auditable` equate a running total with a chain of commitments, and without the
    homomorphism the chain is no longer a chain. So it is LOUD in intent.
    It is silent *today*, which is the interesting part: the current proofs are `rfl`/`simp`
    level and never invoke the assumption, so making it false in a model would not make any
    existing proof term fail — the theorems would simply stop meaning what they are read as
    meaning. That gap between "the proof still type-checks" and "the theorem still says the
    right thing" is what an axiom budget is for, and why this one's consumers should carry a
    non-zero budget the moment the curve model lands. -/
def pedersenHomomorphismStatus : String :=
  "HIGH-6: LOUD in intent, SILENT today. consumers: supply_chain_invariant, no_hidden_inflation, cumulative_auditable"

/-- HIGH-7: `CrossCutting.zero_cond_prevents_smuggling`. REMOVED. Its conclusion restated its
    own hypothesis and `zero_cond` did not appear in the statement. The attack/defence prose
    above it describes the `.zk` gate accurately; it was never a Lean proof. -/
def zeroCondSmugglingClaimStatus : String :=
  "HIGH-7: REMOVED. statement was `h → h`; zero_cond absent from it"

/-- HIGH-8: `HashOps.merkle_inclusion_soundness` and `merkle_root_deterministic`. REMOVED.
    The first was its hypothesis restated, with a second hypothesis `root = root`; the second
    was `x = x`. The soundness argument for Merkle inclusion is real, but its first premise is
    "the host verifies the ZK proof", which is the gap `Axioms.NoFreeInstances` names. -/
def merkleInclusionClaimStatus : String :=
  "HIGH-8: REMOVED. `h → h` with a `root = root` hypothesis, and `x = x`"

/-- HIGH-9: `HashOps.poseidon_deterministic` and `signature_public_determinism`. REMOVED.
    The first said `g.output = g.output`; the second said `congrArg poseidon_hash_output` with
    a hypothesis attached, and was named for uniqueness, unlinkability and commitment-owner
    binding that its statement never mentioned. -/
def poseidonDeterminismClaimStatus : String :=
  "HIGH-9: REMOVED. `g.output = g.output`, and a congrArg named for signature unlinkability"

/-- HIGH-10: `ECOps.pedersen_commitment_binding`. REMOVED. Its conclusion was
    `(v1 = v2 ∧ r1 = r2) ∨ (v1 ≠ v2 ∨ r1 ≠ r2)` — a tautology by `em` — and its two `Bool`
    hypotheses were unused. Its name asserted Pedersen binding; a `Bool` argument is not a
    commitment and `true` is not a constraint, so it could not have carried that content even
    with a non-trivial conclusion. Pedersen binding is not modelled in Lean. -/
def pedersenBindingClaimStatus : String :=
  "HIGH-10: REMOVED. tautology `P ∨ ¬P`; binding content lived in unused Bool hypotheses"

/-- HIGH-11: `Circuits.Token.burn_v1_no_free_instances`. REMOVED. Its statement was
    `x = x ∧ y = y ∧ True`, proved `⟨rfl, rfl, trivial⟩`, under a heading claiming the
    Orchard-class guarantee. `proofs/lean/README.md` cited it as that guarantee. -/
def burnV1NoFreeInstancesClaimStatus : String :=
  "HIGH-11: REMOVED. `x = x ∧ y = y ∧ True`; README cited it as the Orchard-class guarantee"

/-- HIGH-12: `Soundness.less_than_strict_sound`. REMOVED. `a < b → a < b`, proved by
    `intro h; exact h` — the identity function, under this file's own header claim
    "LessThanStrict is SOUND — verified, no counterexamples". The real result is
    `Comparison.less_than_strict_sound`, which takes the gadget's actual parameters. -/
def lessThanStrictClaimStatus : String :=
  "HIGH-12: REMOVED. `a < b → a < b` was `id`; real result is Comparison.less_than_strict_sound"

def highAxiomFindings : List (String × Nat × String) := [
  ("HIGH-6: pedersen_additive_homomorphism", 45,
   "DISCHARGED. Was an assumption about an opaque operation; now a theorem about the real Pallas " ++
   "curve (Pedersen.lean), resting on `pallasPrime` alone"),
  ("HIGH-7: zero_cond_prevents_smuggling", 42, "REMOVED; conclusion restated its hypothesis"),
  ("HIGH-8: merkle_inclusion_soundness / merkle_root_deterministic", 42,
   "REMOVED; `h → h` with a `root = root` hypothesis, and `x = x`"),
  ("HIGH-9: poseidon_deterministic / signature_public_determinism", 40,
   "REMOVED; `x = x`, and a congrArg named for signature unlinkability"),
  ("HIGH-10: pedersen_commitment_binding", 42,
   "REMOVED; tautology, binding content in unused Bool hypotheses"),
  ("HIGH-11: burn_v1_no_free_instances", 42,
   "REMOVED; `x = x ∧ y = y ∧ True`, cited by README as the Orchard-class guarantee"),
  ("HIGH-12: less_than_strict_sound", 40, "REMOVED; `a < b → a < b` was `id`"),
  ("HIGH-13: poseidon_collision_resistance", 78,
   "PRESENT, CONSISTENT, AND FALSE OF POSEIDON. The axiom states injectivity " ++
   "(`x ≠ y → h x ≠ h y`), which its docstring called equivalent to collision-resistance. It is " ++
   "strictly stronger, and injectivity is false of the real sponge by pigeonhole: `src/zk/vm.rs:1123` " ++
   "dispatches `poseidon_hash` for 1–24 field elements, so the domain exceeds the 254-bit range and " ++
   "collisions exist necessarily. The model is satisfiable — `List Int` and `Int` are both countable, " ++
   "so an injection between them exists — which is why nothing is derivable from it and why this is " ++
   "not the CRIT-6 failure. But `commitment_binding`, `nullifier_binding`, `smtCrh_injective` and " ++
   "`merkle_root_change_detection` each derive a hash inequality from an input inequality, i.e. the " ++
   "injectivity direction, and so hold of an injective `h` rather than of Poseidon. This entry did not " ++
   "exist until now, though the axiom's own docstring has cited `DarkFi.HAZOP.High` for it throughout. " ++
   "SPLIT, so that the two are distinguishable at the call site: `hash_ne_of_component_ne`, " ++
   "`{commitment,nullifier}_binding_of_injective`, `smtCrh_injective_of_injective` and " ++
   "`foldMerkleRoot_change_detection` are the *content* and carry budget 0 with no assumption at all; " ++
   "the four `poseidon_hash_output` corollaries are the *instantiation*, and their budgets are " ++
   "exactly the assumption. The Merkle induction in particular is now unconditional — only the " ++
   "claim that `sinsemillaCrh` is injective is not"),
  ("HIGH-15: aead_key_committing", 45,
   "PRESENT, CONSISTENT, AND THE UNCONDITIONAL FORM OF A COMPUTATIONAL PROPERTY. The axiom " ++
   "states that a ciphertext authenticates under at most one key, which is what makes a " ++
   "wrong-key wallet discover nothing. The deployment is Sapling DH -> kdf_sapling -> " ++
   "ChaCha20Poly1305 with the nonce derived from `ephem_public` " ++
   "(`src/sdk/src/crypto/note.rs:113-135`), whose guarantee is that a second authenticating key " ++
   "is found with probability about 2^-128 per attempt — a statement about adversaries. The axiom " ++
   "asserts no such pair exists, false of the real construction by counting, and satisfiable only " ++
   "because `aead_open` is opaque and `Int` is countable. Same species as HIGH-13, and recorded " ++
   "here rather than left to the reader because of what it replaced: `Net/Receive.lean`'s " ++
   "`decrypt` *was* `if k = n.recipient then some … else none`, so its `decrypt_sound` was true " ++
   "of the branch and said nothing about a ciphertext, while `wallet.md` §2.1 cited it as the " ++
   "receive path's soundness. The assumption is now visible in a budget (1) where before it was " ++
   "invisible in a definition (0). The shape is falsifiable — " ++
   "`Net.key_committing_is_false_for_leaky_open` exhibits an opening function for which it is " ++
   "false — and the *satisfiable* half is not built: no model in this tree exhibits a tag-based " ++
   "`aead_open`, so the authentication is the residual, not the plaintext recovery"),
  ("HIGH-14: the HAZOP citation itself", 30,
   "The `HIGH-13` citation above was stale: `poseidon_collision_resistance`'s docstring said " ++
   "`Recorded as LOUD in DarkFi.HAZOP.High` and no such entry existed. `check_lean_axioms.py` check 3 " ++
   "validates that an `IF FALSE:` field names a declaration that exists; nothing validates that a " ++
   "`Recorded in HAZOP X` claim names an entry that exists. Same failure class as the other stale " ++
   "citations this pass corrected — found by grepping for the thing being cited")
]

end HAZOP.High
