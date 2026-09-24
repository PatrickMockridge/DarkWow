import DarkFi.Axioms
import DarkFi.Field
import DarkFi.Gadgets
import DarkFi.Arithmetic
import DarkFi.BaseDiv
import DarkFi.BaseDivGadget
import DarkFi.Comparison
import DarkFi.Soundness
import DarkFi.ECOps
import DarkFi.HashOps
import DarkFi.CrossCutting
import DarkFi.SupplyChain
import DarkFi.HAZOP
import DarkFi.Capability.Types
import DarkFi.Capability.Pareto
import DarkFi.Capability.Distinction
import DarkFi.Capability.Composition
import DarkFi.Capability.Concurrency
import DarkFi.Capability.Gossip
import DarkFi.Capability.Inversion
import DarkFi.Capability.Wallet
import DarkFi.Capability.Prover
import DarkFi.Capability.Wire
import DarkFi.Capability.PublicInputs
import DarkFi.Capability.PerContractTree
import DarkFi.Capability.Purse
import DarkFi.Capability.DerivedChain
import DarkFi.Capability.PromissoryNote
import DarkFi.Capability.MultiProof
import DarkFi.Net.Framing
import DarkFi.Net.Receive
import DarkFi.Fee.Window
import DarkFi.Capability.Exercise
import DarkFi.Capability.Value
import DarkFi.Capability.NativeToken
import DarkFi.Capability.KeyScope
import DarkFi.Capability.Selection
import DarkFi.Capability.WritePath
import DarkFi.Circuits.Token
import DarkFi.Circuits.Bridge
import DarkFi.Circuits.Exchange
import DarkFi.Circuits.All
import DarkFi.Circuits.InstanceDerivation
-- `Transcribed` is deliberately NOT imported here, and re-adding this import would put the tree's
-- most expensive elaboration back on the default build path. It is a whole library of `decide` proofs
-- over thousands of statements — 181 theorems over 2747 until two `proofs/core` circuits were deleted
-- on 2026-09-24, and the count tracks `proofs/core`, so it moves whenever that does. While it rode on
-- this module's import graph, a `LEAN_NUM_THREADS=4` build of `DarkFi` exhausted this host's memory
-- and froze the machine (2026-09-24). It is now its own library — `lean_lib Transcribed`, built by the
-- gate as `lake build DarkFi Transcribed` — and `CheckAxioms.lean` imports it directly, so the axiom
-- walk still covers every theorem in it. See scripts/lean-build.sh and proofs/lean/README.md.
import DarkFi.HAZOP.Critical
import DarkFi.HAZOP.High
import DarkFi.HAZOP.Elevated
import DarkFi.Combinatorial.StateSpace
import DarkFi.Combinatorial.NullifierStorage
import DarkFi.Combinatorial.Transitions
import DarkFi.Combinatorial.ComplexityJump
import DarkFi.Combinatorial.CompositionBounds
import DarkFi.Combinatorial.Limits
import DarkFi.Combinatorial.CeilingDerivation
import DarkFi.Combinatorial.GeneralTheorem
import DarkFi.Combinatorial.Combinations
import DarkFi.Consensus.MassBalance
import DarkFi.Consensus.NullifierLifecycle
import DarkFi.Consensus.CommitmentSet
import DarkFi.Consensus.BlockTimestamp
import DarkFi.Consensus.CoinbaseSplit
import DarkFi.Consensus.UncleRules
import DarkFi.Consensus.BlockHeader
import DarkFi.Consensus.FeeCollect
import DarkFi.Consensus.CoinbaseStructure
import DarkFi.Consensus.SupplyReconciliation
import DarkFi.Genesis.Ceremony
import DarkFi.Semantics.Proc
import DarkFi.Semantics.Congruence
import DarkFi.Semantics.Substitution
import DarkFi.Semantics.LTS
import DarkFi.Semantics.Ledger

/-!
# DarkFi — ZK Circuit & Capability Type System Formal Verification

Root module for the DarkFi Lean4 library. Imports all submodules
so Lake can build the full project.
-/
