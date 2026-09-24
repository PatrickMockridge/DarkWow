import DarkFi.CrossCutting
import DarkFi.Combinatorial.StateSpace
import DarkFi.AxiomBudget

/-!
# Value Conservation — a property of value-denominated capabilities ONLY

Some capabilities are **value-denominated**: their commitment carries a hidden
value (a Pedersen commitment) and their asset denomination (`AssetId`). For
these, exercise MUST conserve value per asset: `Σ input value_commit == Σ output
value_commit` (Pedersen homomorphism). This is the anti-inflation gate.

State capabilities (Box contents, Purse balance) carry **no** value — their
transition is enforced in-circuit, not by value sums. There is no "value
conservation" for a Box or a Purse. This module states the invariant and binds
it to the Pedersen machinery already proved in `CrossCutting`.
-/

namespace Capability

open CrossCutting

/-- A value-denominated capability: its commitment, its hidden value (as a
    Pedersen commitment — the value itself is never public), and its asset. -/
structure Valued where
  commitment : Combinatorial.LeafCommitment
  valueCommit : PedersenCommitment
  asset : Nat
deriving BEq, Repr

/-- Per-asset value conservation: for each asset, the sum of input values
    equals the sum of output values. This is the abstract invariant; the
    on-chain check compares the Pedersen commitment sums instead of the
    plaintext values (see `value_conservation` below). -/
def valueConservedPerAsset (inputs outputs : List Valued) : Prop :=
  ∀ asset : Nat,
    ((inputs.filter (fun v => v.asset = asset)).map (fun v => v.valueCommit.value)).sum
    = ((outputs.filter (fun v => v.asset = asset)).map (fun v => v.valueCommit.value)).sum

/-- Value conservation via Pedersen homomorphism: if the commitment sums are
    equal, the value sums are equal. This is the mechanism the entrypoint uses
    (`verify_value_conservation`) — it never sees a plaintext value. -/
@[axiom_budget 0]
theorem value_conservation
    (inputs outputs : List PedersenCommitment)
    (h_sum : sum_pedersen inputs = sum_pedersen outputs) :
    (sum_pedersen inputs).value = (sum_pedersen outputs).value :=
  CrossCutting.pedersen_sum_equality_implies_value_equality inputs outputs h_sum

/-- No modular wraparound: with values range-checked to 64 bits and at most
    16 coins per transaction, the value sum stays below 2^68 < p — integer
    equality and field equality coincide (the entrypoint check is both
    necessary and sufficient).

    Budget 1, and it is worth saying why a *delegation* is not budget 0: the proof term is
    `CrossCutting.value_conservation_no_wraparound`, whose own budget is 1, and a proof's axiom set is
    that of the term, not of the statement. This declaration carried no annotation until
    2026-09-24, and passed check 4 anyway — by short-name collision with the `CrossCutting` theorem
    it delegates to, which is the hole `annotation_for` in `script/check_lean_axioms.py` now closes. -/
@[axiom_budget 1]
theorem value_conservation_no_wraparound
    (values : List Int)
    (h_range : ∀ v ∈ values, 0 ≤ v ∧ v < 2^64)
    (h_count : values.length ≤ 16) :
    List.sum values < 2^68 :=
  CrossCutting.value_conservation_no_wraparound values h_range h_count

end Capability
