import DarkFi.AxiomBudget

-- No `-- DECLARED:` line: the ceiling comparisons here used to be `native_decide`, which reflects
-- through the compiled code generator (`Lean.ofReduceBool`, `Lean.trustCompiler`) rather than the
-- kernel and charged +2 each. They are `decide` now — kernel-checked, resting on `Classical.choice`
-- alone — because every comparison is between closed numerals. The obligation register tracked it.

/-!
# Derivation of the L1 Complexity Ceiling

The ceiling constants (P_max, W_max, O_max) used by the general theorem
(GeneralTheorem.lean) are NOT empirical — they are structural consequences
of three constraints:

1. Halo2 circuit architecture: k rows, advice/instance column layout
2. Merkle tree structure: depth D, Sinsemilla MerkleCRH, proof size
3. Wallet scan economics: objects/sec × block interval

This module documents the derivation. The constants are computed here
so the general theorem can reference them without hard-coded numbers.

References:
  - GeneralTheorem.lean (classifier, theorems)
  - safety.md Lesson 23 (hardening log book)
  - src/sdk/src/crypto/merkle_node.rs (MERKLE_DEPTH = 32)
  - doc/src/arch/privacy.md §6 (L1 privacy budget)
-/

namespace Combinatorial.CeilingDerivation

/-! ==========================================================================
   Part 1: Halo2 Circuit Structure Constraints
   ==========================================================================
   A Halo2 circuit of size k has 2^k rows. Each row has:
   - Advice columns: witness values, intermediate computations
   - Instance columns: public inputs via constrain_instance
   - Fixed columns: selector gates, constants
   - Permutation argument: copy constraints across columns

   For practical WASM deployment:
   - k ≤ 15 (2^15 = 32,768 rows): proving key fits in WASM linear memory
   - k ≤ 13 is the typical sweet spot (8,192 rows, ~100ms proving)
   - k ≤ 11 for lightweight contracts (2,048 rows, ~25ms proving)

   The proportion of instance columns to total columns determines the
   maximum practical P. With 3 advice columns, 1 instance column, and
   3 fixed columns (typical configuration):
   - Instance column = 1/7 ≈ 14% of circuit cells
   - But not all rows produce public inputs (most are intermediate)
   - Conservative: ~1% of rows produce constrain_instance calls
   - For k=13 (8,192 rows): ~82 constrain_instance cells max
   - Distributed across O operations: ~27 per operation (O=3)
   - Single operation ceiling: 9 (accounts for lookups + copy constraints)
-/

/-- Circuit size exponent (max practical for WASM) --/
def MAX_K_WASM : Nat := 15

/-- Typical k for L1 contracts --/
def TYPICAL_K : Nat := 13

/-- Approximate proportion of rows that can produce constrain_instance calls --/
def INSTANCE_PROPORTION : Nat := 1  -- 1% of total rows

/-- Maximum constrain_instance cells for typical k=13 circuit --/
def MAX_INSTANCE_CELLS : Nat := (2 ^ TYPICAL_K) * INSTANCE_PROPORTION / 100

/-! ==========================================================================
   Part 2: Merkle Tree Constraints
   ==========================================================================
   Merkle tree depth D = 32 (Orchard standard). Each inclusion proof:
   - 32 sibling nodes × 32 bytes = 1024 bytes (merkle_path)
   - 1 leaf position (u32, 4 bytes)
   - 1 leaf commitment (32 bytes)
   - Total per proof: ~1060 bytes

   The Merkle proof is a witness value — it doesn't appear as a public
   input. But the merkle_root opcode in the circuit consumes:
   - D + 1 witness values (leaf position + D siblings)
   - 1 constrain_instance (expected_root)
   - ~D gates (one per level, Sinsemilla hash)

   So each Merkle inclusion proof contributes:
   - 1 public input (expected_root)
   - D+2 witness values (leaf_pos, path bytes, leaf_value)
   - ~D gates (MerkleCRH per level)
-/

/-- Merkle tree depth (Orchard standard) --/
def MERKLE_DEPTH : Nat := 32

/-- Witness values consumed by a Merkle inclusion proof --/
def MERKLE_WITNESS_COST : Nat := MERKLE_DEPTH + 2  -- 34 witness values

/-- Public inputs produced by a Merkle inclusion proof --/
def MERKLE_INSTANCE_COST : Nat := 1  -- expected_root only

/-! ==========================================================================
   Part 3: Wallet Scan Economics
   ==========================================================================
   For a wallet to maintain privacy, it must:
   1. Download all new Merkle leaves since last scan
   2. Attempt trial decryption (AEAD) on each leaf
   3. Match decrypted values against known object IDs
   4. Complete all scans within the block interval

   The scan rate is bounded by the slowest supported client (mobile):
   - Mobile: ~1,000 objects/sec (trial decryption dominates)
   - Desktop: ~10,000 objects/sec
   - Light client: ~100 objects/sec (bandwidth-constrained)

   With 120s block intervals:
   - Mobile: ~120,000 objects max
   - Desktop: ~1,200,000 objects max
   - Light: ~12,000 objects max

   The PRACTICAL ceiling uses the mobile bound — if mobile users can't
   scan the anonymity set, privacy collapses to desktop-only.
-/

def MOBILE_SCAN_RATE : Nat := 1000     -- objects/second
def DESKTOP_SCAN_RATE : Nat := 10000   -- objects/second
def BLOCK_INTERVAL : Nat := 120        -- seconds
def PRACTICAL_MAX_OBJECTS : Nat := MOBILE_SCAN_RATE * BLOCK_INTERVAL  -- 120,000

/-! ==========================================================================
   Part 4: The Ceiling Constants
   ==========================================================================
   Derived from the three constraints above.

   P_CEILING = 9 per operation:
     - Each operation needs: 1 nullifier + 1 merkle_root + 1 tx_binding + 1 tx_nonce = 4 minimum
     - Plus optional: 1 new_leaf (non-terminal) = 5
     - Plus optional: 4 Pedersen coords (Purse) = 9
     - 9 is the Purse Deposit ceiling — no existing contract exceeds this

   W_CEILING = 13 per operation:
     - Each operation needs: object_id + state_nonce + owner_secret + leaf_pos = 4 minimum
     - Plus: merkle_path (counted as 1 witness, the array is collapsed) = 5
     - Plus optional: 2 contents commits (old+new Box) = 7
     - Plus optional: 3 balances + 3 blinds (Purse) = 13
     - 13 is the Purse Deposit ceiling

   O_CEILING = 3 per contract:
     - Minimum viable L1 contract: 1 consume + 1 create = 2 operations
     - Plus optional: 1 read-only query = 3
     - 3 is the Purse ceiling (Deposit, Withdraw, Balance)
     - A contract with 4+ operations has too many state transitions
       for a single Merkle tree — the scan complexity per block exceeds
       wallet capacity
-/

def P_CEILING : Nat := 9     -- max public inputs per operation
def W_CEILING : Nat := 13    -- max witness values per operation
def O_CEILING : Nat := 3     -- max operations per contract

-- Scrutiny tier: up to ~1.67× the safe ceilings
-- Contracts here need explicit combinatorial bounds proof
def P_SCRUTINY : Nat := 15   -- 9 * 5/3
def W_SCRUTINY : Nat := 20   -- 13 * 3/2 ≈ 20
def O_SCRUTINY : Nat := 6    -- 3 * 2

/-! ==========================================================================
   Part 5: Derivation Soundness
   ==========================================================================
   Verify that the derived constants are internally consistent:
   - P_CEILING ≥ minimum required for an L1 operation (4)
   - W_CEILING ≥ minimum required for an L1 operation (5)
   - O_CEILING ≥ minimum viable contract (2)
   - Scrutiny constants > safe constants
-/

@[axiom_budget 0]
theorem p_ceiling_ge_minimum : P_CEILING ≥ 4 := by decide
@[axiom_budget 0]
theorem w_ceiling_ge_minimum : W_CEILING ≥ 5 := by decide
@[axiom_budget 0]
theorem o_ceiling_ge_minimum : O_CEILING ≥ 2 := by decide
@[axiom_budget 0]
theorem scrutiny_gt_safe : P_SCRUTINY > P_CEILING ∧ W_SCRUTINY > W_CEILING ∧ O_SCRUTINY > O_CEILING := by
  decide

/-! ==========================================================================
   Part 6: Contract-Specific Ceiling Check
   ==========================================================================
   **Recounted 2026-09-26, and the accounting here is measured rather than recalled.** Each row is
   one circuit: (name, `constrain_instance` sites, *private* witness slots), where the private count
   is the circuit's witness declarations minus its instances. That definition is not a convenience
   chosen to fit — it is what the four figures this section already carried meant: `Box Put 9` was
   14 declarations − 5 instances before the owner-binding fix, `Purse Deposit 13` is 22 − 9,
   `Purse Balance 11` is 18 − 7, and `Box Take 7` is 11 − 4.

   Two things changed with the recount, and the second is why this section is rewritten rather than
   corrected. (1) **The owner binding added a witness to five circuits**, so Box's rows moved 9 → 10
   and 7 → 8; the sums this section used to check were therefore stale as well as unverifiable.
   (2) **The shape was wrong**: `(9 : Nat) / 2 ≤ P_CEILING` is a *sum over operations* divided by the
   operation count, and `Nat` division truncates, so it cannot fail per operation —
   `a_sum_over_operations_cannot_bound_one_operation` below is the refutation, and PromissoryNote is
   the instance that matters, because `RevokeV2` carries ten public inputs while the sum-shaped check
   over the same contract passes at `(42) / 5 = 8 ≤ 9`.

   The two theorems this replaces (`box_within_ceilings`, `purse_within_ceilings`) were referenced
   nowhere else; their claim is subsumed by the per-operation statement, which is strictly stronger.
-/

/-- Every L1 circuit's public inputs and private witnesses, counted from the circuit sources on
    2026-09-26: `constrain_instance` sites, and witness declarations minus those instances. -/
def l1Operations : List (String × Nat × Nat) := [
  ("promissory_note/RegisterTypeV2", 8, 5),
  ("promissory_note/IssueV2", 9, 5),
  ("promissory_note/RedeemV2", 8, 3),
  ("promissory_note/RevokeV2", 10, 5),
  ("promissory_note/TransferV2", 7, 4),
  ("box/Put", 5, 10),
  ("box/Take", 4, 8),
  ("purse/Deposit", 9, 13),
  ("purse/Withdraw", 9, 13),
  ("purse/Balance", 7, 11),
]

/-- The operations each L1 contract carries, against `O_CEILING`. -/
def l1Contracts : List (String × Nat) :=
  [("promissory_note", 5), ("box", 2), ("purse", 3)]

/-- **The per-operation statement the sum shape could not make.** Every measured L1 operation is
    inside `P_CEILING` in the `W` axis and, for all but one, in the `P` axis too — and the exception
    is the point: `RevokeV2` carries ten public inputs, one above `P_CEILING`, and a contract-level
    sum cannot say so. -/
@[axiom_budget 0]
theorem every_measured_l1_operation_is_within_the_w_ceiling :
    l1Operations.all (fun op => decide (op.2.2 ≤ W_CEILING)) = true := by
  decide

/-- The operations above `P_CEILING`, by name: the scrutiny tier, computed rather than listed. -/
@[axiom_budget 0]
theorem the_operations_above_p_ceiling :
    l1Operations.filterMap (fun op => if P_CEILING < op.2.1 then some op.1 else none) =
      ["promissory_note/RevokeV2"] := by
  decide

/-- The contracts above `O_CEILING`, by name. **PromissoryNote is in the operations scrutiny tier at
    five circuits**, which the earlier section did not say because it looked at Box and Purse only —
    the external report's finding 15, as a computation. -/
@[axiom_budget 0]
theorem the_contracts_above_o_ceiling :
    l1Contracts.filterMap (fun c => if O_CEILING < c.2 then some c.1 else none) =
      ["promissory_note"] := by
  decide

/-- **Why the previous shape could not be the claim.** `(a + b) / 2 ≤ c` is satisfiable with `a` twice
    the ceiling: take `b = 0`. Averages of a sum over its operation count cannot bound one operation,
    so a conformance check written that way is green for exactly the contracts a per-operation
    ceiling exists to catch. -/
@[axiom_budget 0]
theorem a_sum_over_operations_cannot_bound_one_operation :
    ∃ a b c : Nat, (a + b) / 2 ≤ c ∧ ¬ (a ≤ c) :=
  ⟨2 * P_CEILING, 0, P_CEILING, by decide, by decide⟩

end Combinatorial.CeilingDerivation
