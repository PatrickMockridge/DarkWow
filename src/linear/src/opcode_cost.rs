/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Per-opcode gas from ZK first principles: the number of Halo2 advice rows an
//! opcode's gadget consumes.
//!
//! Gas is the circuit's total row count. A circuit's `k` (domain size `2^k`) is
//! *derived* from its total rows (`k = ceil(log2(rows))`), and the verifier's
//! dominant cost is the multi-scalar multiplication over `2^k` points — which is
//! linear in the total row count. One gas unit = one advice row.
//!
//! # Row counts (fee-spec.md §12.4.2)
//!
//! | Category | Rows | Derivation |
//! |----------|------|------------|
//! | BaseAdd/Sub/Mul, WitnessBase | 1 | `arithmetic.rs` — 1 gate |
//! | ConstrainEqualBase/Instance, IsEqual/IsNotEqual, BoolCheck, CondSelect, ZeroCondSelect | 1 | 1 gate/copy |
//! | ConstrainEqualPoint | 2 | x + y copy constraints |
//! | NotBase | 2 | bool check + arithmetic sub |
//! | BaseDiv | 331 | 254 squarings + 76 conditional + 1 final (p−2: 255 bits, 77 set) |
//! | RangeCheck(bits) | ceil(bits/10) + (bits%10 ? 2 : 0) | running-sum window W=10 |
//! | LessThan* (253) | 57 | 1 gate + 2 × RangeCheck(253) |
//! | PoseidonHash(N) | ceil(N/2) × 36 | P128Pow5T3: R_F=8 + R_P/2=28, RATE=2 |
//! | EcAdd | 6 | incomplete addition (10 cols) |
//! | EcMul/EcMulVarBase | 510 | 255-bit double-and-add (2 rows/bit) |
//! | EcMulBase/EcMulShort | 85 | fixed-base windowed |
//! | EcGetX/EcGetY | 0 | coordinate extraction |
//! | MerkleRoot | 1632 | 32 levels × 51 Sinsemilla rows (2×255 bits / K=10) |
//! | SparseMerkleRoot/SetMembership | 9180 | 255 levels × 36 Poseidon rows |
//! | Noop/DebugPrint | 0 | no constraint |
//!
//! Python reference: contrib/model/fee_window_model.py (`OPCODE_ROWS`).

use dwow_core::zkas::Opcode;

/// Maximum Halo2 k-value (domain size 2^16 = 65536 rows).
/// [1:1] dwow_core::zkas::constants::MAX_K — hardcoded because the constant is crate-private.
pub const MAX_K: u32 = 16;

/// Range-check running-sum window size (bits per row) = `sinsemilla::K`.
pub const RANGE_CHECK_WINDOW: u64 = 10;

/// Poseidon P128Pow5T3: rows per permutation = R_F + R_P/2 = 8 + 28.
pub const POSEIDON_ROWS_PER_PERMUTATION: u64 = 36;

/// Poseidon P128Pow5T3 rate (state words absorbed per permutation).
pub const POSEIDON_RATE: u64 = 2;

/// Per-opcode computational cost from ZK first principles: the number of Halo2
/// advice rows the opcode's gadget occupies.
///
/// Consensus-critical — all miners SHALL agree on these values.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpcodeCost(pub u32);

impl OpcodeCost {
    /// Zero-cost opcode (Noop, DebugPrint, EcGetX, EcGetY).
    pub const ZERO: Self = Self(0);
}

/// RangeCheck gas = running-sum decomposition rows for a `bits`-bit range check.
/// `rows(bits) = ceil(bits/10) + (bits%10 ? 2 : 0)`. 253-bit → 28, 64-bit → 9.
pub fn range_check_rows(bits: u64) -> u64 {
    let windows = bits.div_ceil(RANGE_CHECK_WINDOW);
    let short = if bits % RANGE_CHECK_WINDOW != 0 { 2 } else { 0 };
    windows + short
}

/// PoseidonHash gas for `n` input field elements = `ceil(n/2) × 36`.
pub fn poseidon_rows(n: u64) -> u64 {
    n.div_ceil(POSEIDON_RATE) * POSEIDON_ROWS_PER_PERMUTATION
}

/// Return the fixed computational cost (advice rows) for a given opcode.
///
/// `operand_count` is the number of operands in the opcode's argument list; it is
/// only used for variable-length opcodes (`PoseidonHash` input count). `RangeCheck`
/// defaults to the 253-bit cost — its exact bit width is carried as a literal, not
/// in the operand list, and is resolved by the caller when available.
///
/// Consensus-critical — identical across all nodes.
/// [1:1] Python: contrib/model/fee_window_model.py (`OPCODE_ROWS`).
pub fn opcode_cost(op: Opcode, operand_count: usize) -> OpcodeCost {
    match op {
        // ── Arithmetic (arithmetic.rs: 1 gate = 1 row) ─────────────
        Opcode::BaseAdd => OpcodeCost(1),
        Opcode::BaseSub => OpcodeCost(1),
        Opcode::BaseMul => OpcodeCost(1),
        Opcode::WitnessBase => OpcodeCost(1),

        // ── Heavy arithmetic ───────────────────────────────────────
        // BaseDiv: square-and-multiply of p−2 (255 bits, 77 set bits):
        //   254 squarings + 76 conditional multiplies + 1 final = 331 rows.
        Opcode::BaseDiv => OpcodeCost(331),

        // ── Range / comparison ─────────────────────────────────────
        // Default 253-bit (bit width is a literal, resolved by caller when available).
        Opcode::RangeCheck => OpcodeCost(range_check_rows(253) as u32),
        // 1 compare gate + 2 × RangeCheck(253) = 57.
        Opcode::LessThanStrict => OpcodeCost(57),
        Opcode::LessThanLoose => OpcodeCost(57),
        Opcode::LessThanOrEqual => OpcodeCost(57),
        Opcode::BaseLtStrict => OpcodeCost(57),

        // ── Comparison / selection (1 gate = 1 row, 4 cols) ────────
        Opcode::IsEqualBase => OpcodeCost(1),
        Opcode::IsNotEqualBase => OpcodeCost(1),
        Opcode::BoolCheck => OpcodeCost(1),
        Opcode::NotBase => OpcodeCost(2), // bool check + arithmetic sub
        Opcode::CondSelect => OpcodeCost(1),
        Opcode::ZeroCondSelect => OpcodeCost(1),

        // ── Constrain (copy constraints) ───────────────────────────
        Opcode::ConstrainEqualBase => OpcodeCost(1),
        Opcode::ConstrainEqualPoint => OpcodeCost(2), // x + y
        Opcode::ConstrainInstance => OpcodeCost(1),

        // ── Poseidon (variable length) ─────────────────────────────
        Opcode::PoseidonHash => OpcodeCost(poseidon_rows(operand_count as u64) as u32),

        // ── ECC (halo2 ecc chip) ───────────────────────────────────
        Opcode::EcAdd => OpcodeCost(6),      // incomplete addition (10 cols)
        Opcode::EcMul => OpcodeCost(510),    // 255-bit double-and-add (2 rows/bit)
        Opcode::EcMulVarBase => OpcodeCost(510),
        Opcode::EcMulBase => OpcodeCost(85), // fixed-base windowed
        Opcode::EcMulShort => OpcodeCost(85),
        Opcode::EcGetX => OpcodeCost(0),     // coordinate extraction, no new gate
        Opcode::EcGetY => OpcodeCost(0),

        // ── Sinsemilla / Merkle ────────────────────────────────────
        // 32 levels × 51 Sinsemilla rows (2×255 bits / K=10).
        Opcode::MerkleRoot => OpcodeCost(1632),
        // 255 levels × 36 Poseidon rows.
        Opcode::SparseMerkleRoot => OpcodeCost(9180),
        Opcode::SetMembership => OpcodeCost(9180),

        // ── Zero cost ──────────────────────────────────────────────
        Opcode::Noop => OpcodeCost(0),
        Opcode::DebugPrint => OpcodeCost(0),
    }
}

/// Compute a circuit's total gas from its opcode list: the sum of per-opcode
/// advice rows.
///
/// The circuit's `k` is *derived* from the total row count (`k = ceil(log2(rows))`),
/// so there is no separate `2^(k − K_REF)` multiplier — that scaling was a
/// redundant proxy for the row count and is removed (fee-spec.md §12.11).
///
/// Takes a slice of opcodes with their heap type annotations (the format stored
/// in `ZkBinary.opcodes`). Variable-length opcodes (`PoseidonHash`) use the
/// operand count.
/// [1:1] Python: contrib/model/fee_window_model.py.
pub fn circuit_difficulty(
    opcodes: &[(Opcode, Vec<(dwow_core::zkas::types::HeapType, usize)>)],
) -> u64 {
    opcodes.iter().map(|(op, operands)| opcode_cost(*op, operands.len()).0 as u64).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_cost_noop_is_zero() {
        assert_eq!(opcode_cost(Opcode::Noop, 0), OpcodeCost(0));
    }

    #[test]
    fn test_opcode_cost_constrain_is_cheap() {
        assert_eq!(opcode_cost(Opcode::ConstrainInstance, 1), OpcodeCost(1));
        assert_eq!(opcode_cost(Opcode::ConstrainEqualBase, 2), OpcodeCost(1));
    }

    #[test]
    fn test_opcode_cost_ecc_is_expensive() {
        assert_eq!(opcode_cost(Opcode::EcAdd, 2), OpcodeCost(6));
        assert_eq!(opcode_cost(Opcode::EcMul, 2), OpcodeCost(510));
    }

    #[test]
    fn test_opcode_cost_ordering() {
        // SMT > Merkle > ECC > BaseDiv > LessThan > RangeCheck > arithmetic > constrain
        assert!(opcode_cost(Opcode::SparseMerkleRoot, 4) > opcode_cost(Opcode::MerkleRoot, 3));
        assert!(opcode_cost(Opcode::MerkleRoot, 3) > opcode_cost(Opcode::EcMul, 2));
        assert!(opcode_cost(Opcode::EcMul, 2) > opcode_cost(Opcode::BaseDiv, 2));
        assert!(opcode_cost(Opcode::BaseDiv, 2) > opcode_cost(Opcode::LessThanStrict, 2));
        assert!(opcode_cost(Opcode::LessThanStrict, 2) > opcode_cost(Opcode::RangeCheck, 2));
        assert!(opcode_cost(Opcode::RangeCheck, 2) > opcode_cost(Opcode::BaseMul, 2));
        assert!(opcode_cost(Opcode::BaseMul, 2) > opcode_cost(Opcode::ConstrainInstance, 1));
    }

    #[test]
    fn test_range_check_rows() {
        assert_eq!(range_check_rows(64), 9);   // ceil(64/10)=7 + 2 short
        assert_eq!(range_check_rows(253), 28); // ceil(253/10)=26 + 2 short
        assert_eq!(range_check_rows(10), 10);  // exact window, no short
    }

    #[test]
    fn test_poseidon_rows() {
        assert_eq!(poseidon_rows(1), 36);  // 1 element → 1 permutation
        assert_eq!(poseidon_rows(2), 36);  // 2 elements → 1 permutation (RATE=2)
        assert_eq!(poseidon_rows(3), 72);  // 3 elements → 2 permutations
        assert_eq!(poseidon_rows(4), 72);
    }

    #[test]
    fn test_circuit_difficulty_is_row_sum() {
        // 5 simple ops (1 row each) = 5
        let ops = vec![
            (Opcode::WitnessBase, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainInstance, vec![]),
            (Opcode::ConstrainInstance, vec![]),
        ];
        assert_eq!(circuit_difficulty(&ops), 5);
    }

    #[test]
    fn test_circuit_difficulty_empty() {
        assert_eq!(circuit_difficulty(&[]), 0);
    }

    #[test]
    fn test_circuit_difficulty_poseidon_variable_length() {
        // PoseidonHash with 3 operands → 2 permutations × 36 = 72
        let ops = vec![(
            Opcode::PoseidonHash,
            vec![
                (dwow_core::zkas::types::HeapType::Var, 0),
                (dwow_core::zkas::types::HeapType::Var, 1),
                (dwow_core::zkas::types::HeapType::Var, 2),
            ],
        )];
        assert_eq!(circuit_difficulty(&ops), 72);
    }

    #[test]
    fn test_circuit_difficulty_base_div_heavy() {
        let ops = vec![
            (Opcode::BaseDiv, vec![]), // 331
            (Opcode::BaseDiv, vec![]), // 331
            (Opcode::EcMul, vec![]),   // 510
        ];
        assert_eq!(circuit_difficulty(&ops), 331 + 331 + 510);
    }
}
