import Mathlib

/-!
# Derived-Rule DAG — intermediate-referencing rules (write-path invariant #5)

Formalizes the central root cause of the L1 write-path blockers (HAZOP V1/V2/V3):
the generic prover's `compute_derived` (`bin/dww/src/prover_impl.rs`) resolves
operands only from **bound witness slots**, so it cannot express a derived rule
whose operand is *another rule's output* — the zkas VM's notion of an
intermediate heap value. This is exactly the shape needed for

  * purse `new_nonce = state_nonce + 1` feeding `new_leaf = poseidon(5, pid, bal, new_nonce)`, and
  * PN RevokeV2 `signature_secret = poseidon(7, secret, nullifier)` where
    `nullifier = poseidon(1, secret, coin)` and `coin = poseidon(4, pub, …)`.

The fix is an **intermediate-referencing DAG**: a rule operand may be a witness
slot OR a prior rule's output, with a topological pre-order and no cycles
(`wallet.md:816` invariant #5).
-/

namespace DarkFi.Capability

/-! ===== Operand: a witness slot or a prior rule's output ===== -/

/-- An operand of a derived rule. `slot i` reads witness slot `i`; `derived i`
    reads the output of the `i`-th prior rule in the chain (an intermediate
    heap value). -/
inductive Operand where
  | slot (i : Nat)
  | derived (i : Nat)
deriving Repr, DecidableEq

/-- A derived rule is an (opaque) rule tag plus its operand list. -/
structure DerivedNode where
  rule : String
  operands : List Operand
deriving Repr, DecidableEq

/-- A node's operand is resolvable when every `derived i` reference points at a
    strictly-earlier node in the chain (index < the node's own index). -/
def resolvable (chain : List DerivedNode) (idx : Nat) : Prop :=
  ∀ op ∈ (chain.getD idx default).operands,
    match op with
    | Operand.slot _ => True
    | Operand.derived j => j < idx

/-- A chain is a DAG (topological, intermediate-referencing) iff every node's
    operands are resolvable against the prefix before it. -/
def derivedChainWellFormed (chain : List DerivedNode) : Prop :=
  ∀ idx, idx < chain.length → resolvable chain idx

/-- **DAG soundness**: in a well-formed chain, a `derived j` operand always
    references an already-emitted node — no forward reference, no cycle. -/
theorem derived_chain_topological
    (chain : List DerivedNode) (idx : Nat) (hidx : idx < chain.length)
    (h : derivedChainWellFormed chain) (op : Operand)
    (hop : op ∈ (chain.getD idx default).operands) :
    match op with
    | Operand.slot _ => True
    | Operand.derived j => j < idx := by
  exact h idx hidx op hop

/-- A chain with a self-referential `derived` operand (cycle) is not well-formed.
    This is the structural rejection of the manifest's `derived:signature_secret:0,0`
    pattern when the rule needs the nullifier (a *later* derived value), not the
    secret (a witness slot). -/
theorem derived_chain_rejects_cycle
    (chain : List DerivedNode) (idx : Nat) (hidx : idx < chain.length)
    (hcycle : Operand.derived idx ∈ (chain.getD idx default).operands) :
    ¬ derivedChainWellFormed chain := by
  intro h
  have htop := h idx hidx (Operand.derived idx) hcycle
  have : idx < idx := htop
  exact (Nat.lt_irrefl idx) this

/-! ===== Computability of a nested chain ===== -/

/-- The purse nonce-increment chain: `new_nonce = slot nonce + 1`, then
    `new_leaf` references that intermediate. This is the V2 fix expressed as a
    two-node intermediate-referencing DAG. -/
def purseNonceChain (nonceSlot leafIdSlot balanceSlot : Nat) : List DerivedNode :=
  [ ⟨"increment", [Operand.slot nonceSlot]⟩
  , ⟨"leaf", [Operand.slot leafIdSlot, Operand.slot balanceSlot, Operand.derived 0]⟩ ]

/-- The purse nonce chain is well-formed: the `leaf` node's `derived 0` operand
    references the prior `increment` node. -/
theorem purseNonceChain_wellFormed
    (nonceSlot leafIdSlot balanceSlot : Nat) :
    derivedChainWellFormed (purseNonceChain nonceSlot leafIdSlot balanceSlot) := by
  intro idx hidx op hop
  simp [purseNonceChain] at hop ⊢
  have hlen : (purseNonceChain nonceSlot leafIdSlot balanceSlot).length = 2 := rfl
  -- Only node index 1 (leaf) has a `derived` operand, referencing node 0.
  interval_cases idx <;> simp_all

/-- The PN RevokeV2 burn chain: `coin` (6 witness slots) → `nullifier` (secret +
    coin) → `signature_secret` (secret + nullifier). Each subsequent rule
    references the previous rule's output as an intermediate. -/
def revokeChain : List DerivedNode :=
  [ ⟨"coin", [Operand.slot 0, Operand.slot 1, Operand.slot 2, Operand.slot 3, Operand.slot 4, Operand.slot 5]⟩
  , ⟨"nullifier", [Operand.slot 0, Operand.derived 0]⟩
  , ⟨"signature_secret", [Operand.slot 0, Operand.derived 1]⟩ ]

/-- The revoke chain is well-formed: each rule's `derived` operand references a
    strictly-earlier rule. -/
theorem revokeChain_wellFormed : derivedChainWellFormed revokeChain := by
  intro idx hidx op hop
  simp [revokeChain] at hop ⊢
  interval_cases idx <;> simp_all

end DarkFi.Capability
