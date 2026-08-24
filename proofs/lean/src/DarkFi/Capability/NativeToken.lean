import DarkFi.Combinatorial.StateSpace

/-!
# The Native Token — the ONE consensus special case

Per contract-wasm-type-system.md §A.0.5, NativeToken is a **consensus** contract:
it SHALL provide coinbase, fee payment, and transfers only. Its "coin" is a
value-denominated capability minted ex nihilo by `PoWRewardV1` and held back by
`COINBASE_MATURITY`. This is the *only* place a "coin" exists — every other
value/capability movement is the generic exercise of `Capability.Exercise`.

The emission schedule (`S_H = S_{H-1} + C_H`) is formalized in `SupplyChain`;
this module states the maturity gate — the special-case distinction between a
coinbase claim (recorded for maturity) and a spend nullifier (recorded for
double-spend).
-/

namespace Capability

open Combinatorial

/-- A coinbase claim: the native token's coinbase publishes a nullifier that is
    recorded for **maturity**, not double-spend. The claim nullifier IS the
    future spend nullifier (same poseidon hash), so the contract must NOT mark
    it spent at mint; the host records its creation height instead. -/
structure CoinbaseClaim where
  nullifier : NullifierValue
  createdAt : Nat
deriving BEq, Repr

/-- The coinbase maturity distance, matching `src/linear/src/lib.rs:65`. -/
def COINBASE_MATURITY : Nat := 100

/-- A coinbase claim is mature at `current` iff it has aged at least
    COINBASE_MATURITY blocks. -/
def coinbaseMature (current : Nat) (c : CoinbaseClaim) : Prop :=
  current - c.createdAt ≥ COINBASE_MATURITY

/-- The maturity gate: a coinbase coin may be spent only once mature. -/
def maturityGate (current : Nat) (c : CoinbaseClaim) : Prop :=
  coinbaseMature current c

/-- An immature coinbase is rejected by the maturity gate. -/
theorem immature_coinbase_rejected
    (current : Nat) (c : CoinbaseClaim) (h_young : current - c.createdAt < COINBASE_MATURITY) :
    ¬ maturityGate current c := by
  unfold maturityGate coinbaseMature
  omega

/-- A mature coinbase passes the maturity gate. -/
theorem mature_coinbase_accepted
    (current : Nat) (c : CoinbaseClaim) (h_old : current - c.createdAt ≥ COINBASE_MATURITY) :
    maturityGate current c := by
  unfold maturityGate coinbaseMature
  exact h_old

end Capability
