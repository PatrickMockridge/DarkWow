/-
DarkWow Capability Type System — Non-Unifiable Pair Proofs

Proves every non-unifiable pair from type-system.md §8.4. Each pair SHALL
NOT be unified under any generic interface, trait bound, From impl, or
type alias. The proof is mechanical: the left type has barbs that the
right type does not, so they are distinguishable under bisimulation.

All 10 pairs from the specification:
  1. Nullifier ≠ [u8; 32]
  2. Commitment ≠ [u8; 32]
  3. SecretKey ≠ [u8; 32]
  4. ContractId ≠ [u8; 32]
  5. PublicKey ≠ pallas::Point
  6. SecretKey ≠ pallas::Base
  7. FuncId ≠ pallas::Base
  8. AssetId ≠ pallas::Base
  9. Nullifier ≠ IntentNullifier
  10. OwnedSecretKey ≠ SecretKey
-/

import DarkFi.Capability.Types
import DarkFi.Capability.Pareto
import DarkFi.AxiomBudget

-- No `-- DECLARED:` line: the ten pairwise distinctness proofs here used to be `native_decide`,
-- which reflects through the compiled code generator (`Lean.ofReduceBool`, `Lean.trustCompiler`)
-- rather than the kernel, and charged +2 each. They are `decide` now, like Pareto.lean's fifteen,
-- so each rests on `Classical.choice` and nothing else. The obligation register tracked this.

open DarkFi.Capability.Types

/- ==========================================================================
   Pair 1: Nullifier ≠ [u8; 32]
   Nullifier has {↓nullify}; raw bytes have ∅.
-/

@[axiom_budget 1]
theorem nullifierNotBytes : typesDistinct nullifier rawBytes := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 2: Commitment ≠ [u8; 32]
   Commitment has {↓commit}; raw bytes have ∅.
-/

@[axiom_budget 1]
theorem commitmentNotBytes : typesDistinct commitment rawBytes := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 3: SecretKey ≠ [u8; 32]
   SecretKey has {↓spend, ↓derive}; raw bytes have ∅.
-/

@[axiom_budget 1]
theorem secretKeyNotBytes : typesDistinct secretKey rawBytes := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 4: ContractId ≠ [u8; 32]
   ContractId has {↓dispatch}; raw bytes have ∅.
-/

@[axiom_budget 1]
theorem contractIdNotBytes : typesDistinct contractId rawBytes := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 5: PublicKey ≠ pallas::Point
   PublicKey has {↓verify, ↓encrypt}; raw point has ∅.
-/

@[axiom_budget 1]
theorem publicKeyNotPoint : typesDistinct publicKey rawCurvePoint := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 6: SecretKey ≠ pallas::Base
   SecretKey has {↓spend, ↓derive}; raw field element has ∅.
-/

@[axiom_budget 1]
theorem secretKeyNotFieldElement : typesDistinct secretKey rawFieldElement := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 7: FuncId ≠ pallas::Base
   FuncId has {↓gate}; raw field element has ∅.
-/

@[axiom_budget 1]
theorem funcIdNotFieldElement : typesDistinct funcId rawFieldElement := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 8: AssetId ≠ pallas::Base
   AssetId has {↓denominate}; raw field element has ∅.
-/

@[axiom_budget 1]
theorem assetIdNotFieldElement : typesDistinct assetId rawFieldElement := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 9: Nullifier ≠ IntentNullifier
   Nullifier has {↓nullify}; IntentNullifier has {↓nullify, ↓gate}.
   Different predicate languages — IntentNullifier gates on intent scope.
-/

@[axiom_budget 1]
theorem nullifierNotIntentNullifier : typesDistinct nullifier intentNullifier := by
  unfold typesDistinct; decide

/- ==========================================================================
   Pair 10: OwnedSecretKey ≠ SecretKey
   OwnedSecretKey has {↓spend} (only if declared);
   SecretKey has {↓spend, ↓derive} (unconditional).
   The ↓derive barb distinguishes them: OwnedSecretKey SHALL NOT derive
   sub-keys directly — derivation goes through AccountManager.
-/

@[axiom_budget 1]
theorem ownedSecretKeyNotSecretKey : typesDistinct ownedSecretKey secretKey := by
  unfold typesDistinct; decide

/- ==========================================================================
   Supplementary: Every pair in §8.4 is provably distinct
   ==========================================================================
   This is the summary theorem: for every non-unifiable pair listed in
   type-system.md §8.4, the types are distinct (their barb sets differ).
   The proof is the conjunction of the 10 theorems above.
-/

@[axiom_budget 1]
theorem allUnifiablePairsProved :
    typesDistinct nullifier rawBytes ∧
    typesDistinct commitment rawBytes ∧
    typesDistinct secretKey rawBytes ∧
    typesDistinct contractId rawBytes ∧
    typesDistinct publicKey rawCurvePoint ∧
    typesDistinct secretKey rawFieldElement ∧
    typesDistinct funcId rawFieldElement ∧
    typesDistinct assetId rawFieldElement ∧
    typesDistinct nullifier intentNullifier ∧
    typesDistinct ownedSecretKey secretKey := by
  exact ⟨
    nullifierNotBytes,
    commitmentNotBytes,
    secretKeyNotBytes,
    contractIdNotBytes,
    publicKeyNotPoint,
    secretKeyNotFieldElement,
    funcIdNotFieldElement,
    assetIdNotFieldElement,
    nullifierNotIntentNullifier,
    ownedSecretKeyNotSecretKey
  ⟩
