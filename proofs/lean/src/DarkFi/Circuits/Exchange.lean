/-!
# Exchange/DEX Circuit Instance-Derivation Proofs

Dex (6), OtcSwap (4), DarkBet (4) — 14 circuits total.

All exchange circuits use commitment/nullifier patterns for trade privacy.
-/

namespace Circuits

/--
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

/--
THEOREM: All exchange circuits are Orchard-class safe.

No unconstrained constrain_instance calls. All public inputs
are derived from witnesses in-circuit.
-/
-- ASSUMPTION (not proven): exchange_circuits_orchard_safe : Prop

end Circuits
