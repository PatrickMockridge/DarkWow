/-!
MANUAL AUDIT DOCUMENTATION — NOT FORMAL PROOFS
This file contains structured vulnerability findings / circuit audit
results. It contains ZERO Lean theorems with non-trivial proofs.
All defs return String or List values for programmatic consumption.

`proofs/lean/README.md` used to list `exchange_circuits_orchard_safe` among eleven
"Circuit Audit Axioms". This file declares nothing: it is comment-only, and the name appears
only inside prose. The `-- NOT DECLARED IN LEAN` line below is a comment. The audit it
describes is manual.
-/
/-!
# Exchange/DEX Circuit Instance-Derivation Proofs

Dex (6), OtcSwap (4), DarkBet (4) — 14 circuits total.

All exchange circuits use commitment/nullifier patterns for trade privacy.
-/

namespace Circuits

/-
## DEX: ExecuteSwapV1 (k=11)

Child OtcSwapV1 calls. Swap state commitments + nullifiers.
All instances derived in-circuit.

## DEX: CancelSwapV1 (k=11)

Swap nullifier verification. Public inputs: computed_nullifier, swap_id.
Both derived in-circuit (nullifier from secret, swap_id from swap state).

## OtcSwap: ExecuteSwapV1 (k=11)

Atomic swap execution. Uses PN::OtcSwapV1.
Value conservation enforced by PN, not by otc_swap contract.

## DarkBet: CreateMarketV1 (k=11)

Market creation with collateral lock.
Commitment = poseidon_hash(secret, amount, market_id).
-/

/-
THEOREM: All exchange circuits are Orchard-class safe.

No unconstrained constrain_instance calls. All public inputs
are derived from witnesses in-circuit.
-/
-- NOT DECLARED IN LEAN (comment, not a declaration):exchange_circuits_orchard_safe : Prop

end Circuits
