import Lake
open Lake DSL

package darkfi_gadgets where
  srcDir := "src"

-- Mathlib: the Field.lean proofs (cross_mul_lt, wraparound_safe, etc.) use
-- Mathlib lemmas (mul_comm, Int.div_lt_iff_lt_mul, pow_le_pow_right). Pinned
-- to the Lean 4.12.0-compatible tag (matches lean-toolchain).
require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @ "v4.12.0"

-- The DarkFi capability type system and ZK gadget specifications.
-- See doc/src/arch/type-system.md for the formal specification this
-- formalizes. Cryptographic properties (collision resistance, binding,
-- zero-knowledge) are assumed as axioms — these correspond to the
-- same assumptions the ZK circuits make. The theorems proven here
-- verify that the type system, gadget algebra, and supply chain
-- invariants hold GIVEN those cryptographic assumptions.

lean_lib DarkFi where
  roots := #[`DarkFi]