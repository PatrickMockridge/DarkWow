import Mathlib
import DarkFi.CrossCutting

/-!
# Multi-proof composition — PN transfer/redeem (HAZOP V4)

PN `transfer` and `redeem` are **two proofs**, not one: a burn (`RevokeV2`) per
input and a mint (`TransferV2` / `RedeemV2`) per output. The generic prover's
`build` currently emits one proof per `proof_circuit`; this module states the
composition invariant the multi-proof extension must uphold: value conservation
across the burn+mint pair.

  * transfer: Σ input value_commit == Σ output value_commit  (per token_commit)
  * redeem:   the output is a zero-value receipt (deliberate value destruction)
-/

namespace DarkFi.Capability

open CrossCutting

/-! ===== Burn + mint composition ===== -/

/-- A burn+mint transfer: the burned inputs and the minted outputs, each a
    Pedersen value commitment (the plaintext value is never public). -/
structure TransferProof where
  burnInputs : List PedersenCommitment
  mintOutputs : List PedersenCommitment
deriving BEq, Repr

/-- Transfer value conservation (the `↓conserve` seam): the sum of burned value
    commitments equals the sum of minted value commitments. -/
def transferConserves (t : TransferProof) : Prop :=
  sum_pedersen t.burnInputs = sum_pedersen t.mintOutputs

/-- **Theorem (burn+mint value conservation)**: if the commitment sums agree,
    the plaintext value sums agree — the anti-inflation gate holds across the
    two proofs. -/
theorem transfer_burn_mint_value_conservation
    (t : TransferProof)
    (h : transferConserves t) :
    (sum_pedersen t.burnInputs).value = (sum_pedersen t.mintOutputs).value :=
  CrossCutting.pedersen_value_conservation t.burnInputs t.mintOutputs h

/-- No wraparound for the summed burn values (each 64-bit, at most 16 per tx). -/
theorem transfer_no_wraparound
    (t : TransferProof)
    (h_range : ∀ c ∈ t.burnInputs, 0 ≤ c.value ∧ c.value < 2^64)
    (h_count : t.burnInputs.length ≤ 16) :
    (t.burnInputs.map (fun c => c.value)).sum < 2^68 :=
  CrossCutting.value_conservation_no_wraparound (t.burnInputs.map (fun c => c.value)) h_range h_count

/-! ===== Redeem: zero-value receipt ===== -/

/-- A redeem is a burn + a single zero-value receipt (value destruction, not
    conservation — `RedeemV2` constrains `value = 0`). -/
structure RedeemProof where
  burnInput : PedersenCommitment
  receipt : PedersenCommitment
deriving BEq, Repr

/-- The receipt is zero-valued: redeem deliberately breaks value conservation. -/
def redeemZeroReceipt (r : RedeemProof) : Prop :=
  r.receipt.value = 0

/-- **Theorem (redeem receipt)**: if the receipt is zero-valued, then the burn
    value is not conserved — value is destroyed (this is the intended semantics,
    not an inflation/underflow bug). -/
theorem redeem_zero_value_receipt_destroys_value
    (r : RedeemProof)
    (hzero : redeemZeroReceipt r)
    (hburn : r.burnInput.value > 0) :
    r.burnInput.value ≠ r.receipt.value := by
  intro h
  rw [hzero] at h
  omega

end DarkFi.Capability
