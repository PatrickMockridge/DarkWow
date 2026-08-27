import DarkFi.Field
import DarkFi.Gadgets
import DarkFi.Arithmetic
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
import DarkFi.Net.Framing
import DarkFi.Net.Receive
import DarkFi.Fee.Window
import DarkFi.Capability.Exercise
import DarkFi.Capability.Value
import DarkFi.Capability.NativeToken
import DarkFi.Circuits.Token
import DarkFi.Circuits.Bridge
import DarkFi.Circuits.Exchange
import DarkFi.Circuits.All
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

/-!
# DarkFi — ZK Circuit & Capability Type System Formal Verification

Root module for the DarkFi Lean4 library. Imports all submodules
so Lake can build the full project.
-/
