import Lake
open Lake DSL

package darkfi_gadgets where
  srcDir := "src"

-- The DarkFi capability type system and ZK gadget specifications.
-- See doc/src/arch/type-system.md for the formal specification this
-- formalizes. Cryptographic properties (collision resistance, binding,
-- zero-knowledge) are assumed as axioms — these correspond to the
-- same assumptions the ZK circuits make. The theorems proven here
-- verify that the type system, gadget algebra, and supply chain
-- invariants hold GIVEN those cryptographic assumptions.

lean_lib DarkFi where
  roots := #[`DarkFi]