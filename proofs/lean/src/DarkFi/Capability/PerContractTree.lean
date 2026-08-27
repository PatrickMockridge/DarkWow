/-
DarkWow.Capability.PerContractTree — per-contract merkle tree identity (T5)

Formalizes the invariant that a capability's Merkle proof must be against the
CONTRACT's own zero-seeded per-contract tree (box_roots / purse_roots /
commitment_roots), not the wallet-local capability tree. The two trees differ
structurally: the contract tree seeds position 0 with the zero leaf and holds
only that contract's leaves in on-chain order; the wallet-local tree has no zero
seed and mixes all contracts' leaves.

`merkle_root_change_detection` (HashOps) states that a different leaf at the same
position yields a different root. Hence a proof against the wallet-local leaf
(with the capability's commitment at position 0) cannot produce the same root as
the contract tree (zero seed at position 0) — the wallet's expected_root would
not be a contract-roots key and the gate rejects it. This is the structural
reason `get_merkle_proof` must replay the contract tree (Rust
`reconstruct_contract_tree`).
-/

import Mathlib
import DarkFi.HashOps

namespace DarkFi.Capability

/- The contract tree is zero-seeded: position 0 holds the zero leaf. The
   wallet-local tree places a capability commitment at position 0 instead. -/
theorem zero_seed_vs_capability_leaf_differ (pos : Int) (path : List Int) (leaf : Int)
    (h : 0 ≠ leaf) :
    HashOps.compute_merkle_root pos path 0 ≠ HashOps.compute_merkle_root pos path leaf := by
  exact HashOps.merkle_root_change_detection pos path 0 leaf h

/- T5 (structural): the expected_root the wallet proves against is determined by
   which leaf it replays. A wallet that proves against its own local leaf
   (≠ zero) cannot produce the contract tree's root (zero seed), so its proof is
   rejected. Replaying the contract tree is therefore necessary, not incidental.
-/
theorem tree_identity_matters (pos : Int) (path : List Int) (contractLeaf walletLeaf : Int)
    (h : contractLeaf ≠ walletLeaf) :
    HashOps.compute_merkle_root pos path contractLeaf
      ≠ HashOps.compute_merkle_root pos path walletLeaf := by
  exact HashOps.merkle_root_change_detection pos path contractLeaf walletLeaf h

end DarkFi.Capability
