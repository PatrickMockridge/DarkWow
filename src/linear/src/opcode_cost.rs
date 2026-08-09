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

//! Per-opcode computational cost from ZK first principles.
//!
//! Each opcode type has a fixed cost derived from its ZK constraint system
//! complexity — gate count, advice columns, and lookup table requirements.
//! A circuit's difficulty is the sum of its opcodes' costs.
//!
//! # First Principles
//!
//! | Category | Gates/op | Advice cols | Lookup | Cost |
//! |----------|----------|-------------|--------|------|
//! | ECC (EcAdd, EcMul, ...) | ~10 | 10 | none | 1000 |
//! | Sinsemilla/Merkle | ~1000 | 5+5 | gen table | 800 |
//! | PoseidonHash | ~100 | 4 | none | 500 |
//! | BaseDiv | ~255 | 4 | none | 250 |
//! | RangeCheck, LessThan* | ~N | 2 | K-table | 100 |
//! | BaseMul | 1 | 4 | none | 50 |
//! | BaseAdd, BaseSub, WitnessBase | 1 | 4 | none | 20 |
//! | Comparison (IsEqual, BoolCheck, ...) | ~5 | 4 | none | 30 |
//! | Selection (CondSelect, ZeroCondSelect) | ~4 | 4 | none | 40 |
//! | Constrain (ConstrainEqual*, ConstrainInstance) | 1 | 1 | none | 5 |
//! | Noop, DebugPrint | 0 | 0 | none | 0 |
//!
//! Calibrated so an average circuit (~20 mixed opcodes) = ~1000 total → 1.0x reference.
//! Python reference: contrib/model/fee_model.py.

use dwow_core::zkas::Opcode;

/// Maximum Halo2 k-value (domain size 2^16 = 65536 rows).
/// [1:1] dwow_core::zkas::constants::MAX_K — hardcoded because the constant is crate-private.
pub const MAX_K: u32 = 16;

/// Reference Halo2 k-value for circuit difficulty scaling.
///
/// K_REF = 11 is the k-value of FeeThreshold_V1, the simplest ZK circuit.
/// Circuits with k > K_REF pay proportionally more because verification
/// runs MSMs over 2^k points — each +1 in k doubles the verifier work.
/// [1:1] Python: contrib/model/fee_window_model.py.
pub const K_REF: u32 = 11;

/// Per-opcode computational cost from ZK first principles.
/// Consensus-critical — all miners SHALL agree on these values.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OpcodeCost(pub u32);

impl OpcodeCost {
    /// Zero-cost opcode (Noop, DebugPrint).
    pub const ZERO: Self = Self(0);
}

/// Return the fixed computational cost for a given opcode.
///
/// Costs are derived from ZK constraint system complexity:
/// gate count per invocation, advice column count, and lookup table
/// requirements. ECC ops are most expensive (10 advice columns),
/// Sinsemilla/Merkle next (generator table + 5+5 columns), arithmetic
/// and constrain ops are cheapest.
///
/// Consensus-critical — identical across all nodes.
/// [1:1] Python: contrib/model/fee_model.py.
pub fn opcode_cost(op: Opcode) -> OpcodeCost {
    match op {
        // ── ECC ops ──────────────────────────────────────────────────
        // 10 advice columns, complete addition formula per invocation.
        Opcode::EcAdd => OpcodeCost(1000),
        Opcode::EcMul => OpcodeCost(1000),
        Opcode::EcMulBase => OpcodeCost(1000),
        Opcode::EcMulShort => OpcodeCost(1000),
        Opcode::EcMulVarBase => OpcodeCost(1000),
        Opcode::EcGetX => OpcodeCost(1000),
        Opcode::EcGetY => OpcodeCost(1000),

        // ── Sinsemilla / Merkle ops ──────────────────────────────────
        // Generator table load + 5 advice columns + 5 for ECC.
        Opcode::MerkleRoot => OpcodeCost(800),
        Opcode::SparseMerkleRoot => OpcodeCost(800),
        Opcode::SetMembership => OpcodeCost(800),

        // ── Poseidon ─────────────────────────────────────────────────
        // ~12 partial + ~5 full rounds of the permutation.
        Opcode::PoseidonHash => OpcodeCost(500),

        // ── Heavy arithmetic ─────────────────────────────────────────
        Opcode::BaseDiv => OpcodeCost(250),        // ~255 gates (Fermat inversion)
        Opcode::RangeCheck => OpcodeCost(100),     // K-table lookup
        Opcode::LessThanStrict => OpcodeCost(100),
        Opcode::LessThanLoose => OpcodeCost(100),
        Opcode::LessThanOrEqual => OpcodeCost(100),
        Opcode::BaseLtStrict => OpcodeCost(100),

        // ── Light arithmetic ─────────────────────────────────────────
        Opcode::BaseAdd => OpcodeCost(20),         // 1 gate
        Opcode::BaseSub => OpcodeCost(20),         // 1 gate
        Opcode::BaseMul => OpcodeCost(50),         // 1 gate but wider column config
        Opcode::WitnessBase => OpcodeCost(20),     // 1 gate (instance witness)

        // ── Comparison ───────────────────────────────────────────────
        Opcode::IsEqualBase => OpcodeCost(30),
        Opcode::IsNotEqualBase => OpcodeCost(30),
        Opcode::BoolCheck => OpcodeCost(30),
        Opcode::NotBase => OpcodeCost(30),

        // ── Selection ────────────────────────────────────────────────
        Opcode::CondSelect => OpcodeCost(40),
        Opcode::ZeroCondSelect => OpcodeCost(40),

        // ── Constrain ops — very cheap ───────────────────────────────
        Opcode::ConstrainEqualBase => OpcodeCost(5),
        Opcode::ConstrainEqualPoint => OpcodeCost(5),
        Opcode::ConstrainInstance => OpcodeCost(5),

        // ── Zero cost ────────────────────────────────────────────────
        Opcode::Noop => OpcodeCost(0),
        Opcode::DebugPrint => OpcodeCost(0),
    }
}

/// Compute a circuit's total difficulty from its opcode list and k-value.
///
/// The circuit IS its opcodes — raw difficulty is the sum of per-opcode costs.
///
/// # k-Value Scaling
///
/// A circuit's `k` parameter determines the Halo2 domain size (2^k rows).
/// Verification runs MSMs over 2^k points — each +1 in k doubles the verifier's
/// work. The k-scaling formula reflects this:
///
/// ```text
/// circuit_difficulty(opcodes, k) = base_cost(opcodes) × 2^(k - K_REF)
/// ```
///
/// Where `K_REF = 11` (FeeThreshold_V1). Circuits with k < K_REF use K_REF
/// (no fractional scaling). Values above MAX_K (16) are capped.
///
/// Takes a slice of opcodes with their heap type annotations (the format
/// stored in `ZkBinary.opcodes`) and the circuit's `k` parameter.
/// [1:1] Python: contrib/model/fee_window_model.py.
pub fn circuit_difficulty(
    opcodes: &[(Opcode, Vec<(dwow_core::zkas::types::HeapType, usize)>)],
    k: u32,
) -> u64 {
    let base: u64 = opcodes.iter().map(|(op, _)| opcode_cost(*op).0 as u64).sum();
    // k < K_REF: no fractional scaling (K_REF is the minimum reference)
    // k > MAX_K: capped at MAX_K
    let effective_k = k.clamp(K_REF, MAX_K);
    let scale_shift = effective_k - K_REF;
    base * (1u64 << scale_shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_cost_noop_is_zero() {
        assert_eq!(opcode_cost(Opcode::Noop), OpcodeCost(0));
    }

    #[test]
    fn test_opcode_cost_constrain_is_cheap() {
        assert_eq!(opcode_cost(Opcode::ConstrainInstance), OpcodeCost(5));
        assert_eq!(opcode_cost(Opcode::ConstrainEqualBase), OpcodeCost(5));
    }

    #[test]
    fn test_opcode_cost_ecc_is_expensive() {
        assert_eq!(opcode_cost(Opcode::EcAdd), OpcodeCost(1000));
        assert_eq!(opcode_cost(Opcode::EcMul), OpcodeCost(1000));
    }

    #[test]
    fn test_opcode_cost_ordering() {
        // ECC > Sinsemilla > Poseidon > BaseDiv > RangeCheck > BaseMul > CondSelect > BaseAdd > Constrain
        assert!(opcode_cost(Opcode::EcAdd) > opcode_cost(Opcode::MerkleRoot));
        assert!(opcode_cost(Opcode::MerkleRoot) > opcode_cost(Opcode::PoseidonHash));
        assert!(opcode_cost(Opcode::PoseidonHash) > opcode_cost(Opcode::BaseDiv));
        assert!(opcode_cost(Opcode::BaseDiv) > opcode_cost(Opcode::RangeCheck));
        assert!(opcode_cost(Opcode::RangeCheck) > opcode_cost(Opcode::BaseMul));
        assert!(opcode_cost(Opcode::BaseMul) > opcode_cost(Opcode::CondSelect));
        assert!(opcode_cost(Opcode::CondSelect) > opcode_cost(Opcode::BaseAdd));
        assert!(opcode_cost(Opcode::BaseAdd) > opcode_cost(Opcode::ConstrainInstance));
    }

    #[test]
    fn test_circuit_difficulty_average_is_about_1000() {
        // Simulate an average circuit: ~20 mixed ops
        let ops = vec![
            (Opcode::WitnessBase, vec![]),
            (Opcode::BaseAdd, vec![]),
            (Opcode::BaseMul, vec![]),
            (Opcode::BaseAdd, vec![]),
            (Opcode::BaseMul, vec![]),
            (Opcode::PoseidonHash, vec![]),
            (Opcode::BaseAdd, vec![]),
            (Opcode::BaseMul, vec![]),
            (Opcode::RangeCheck, vec![]),
            (Opcode::BaseAdd, vec![]),
            (Opcode::BoolCheck, vec![]),
            (Opcode::BaseMul, vec![]),
            (Opcode::CondSelect, vec![]),
            (Opcode::BaseAdd, vec![]),
            (Opcode::BaseAdd, vec![]),
            (Opcode::BaseMul, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainInstance, vec![]),
            (Opcode::ConstrainInstance, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
        ];
        let diff = circuit_difficulty(&ops, K_REF);
        // Expected: 5×20 + 4×50 + 1×500 + 1×100 + 1×30 + 1×40 + 2×5 = 20+200+500+100+30+40+10 = 900
        // Close enough to 1000 for the average circuit calibration.
        assert!(diff > 500 && diff < 2000,
            "average circuit difficulty should be ~1000, got {}", diff);
    }

    #[test]
    fn test_circuit_difficulty_empty() {
        assert_eq!(circuit_difficulty(&[], K_REF), 0);
    }

    #[test]
    fn test_circuit_difficulty_base_div_heavy() {
        // Simulate a BaseDiv-heavy circuit
        let ops = vec![
            (Opcode::WitnessBase, vec![]),
            (Opcode::WitnessBase, vec![]),
            (Opcode::BaseDiv, vec![]),   // 250
            (Opcode::BaseDiv, vec![]),   // 250
            (Opcode::BaseDiv, vec![]),   // 250
            (Opcode::BaseDiv, vec![]),   // 250
            (Opcode::EcMul, vec![]),     // 1000
            (Opcode::EcAdd, vec![]),     // 1000
            (Opcode::EcAdd, vec![]),     // 1000
            (Opcode::EcMul, vec![]),     // 1000
            (Opcode::PoseidonHash, vec![]), // 500
            (Opcode::BaseMul, vec![]),    // 50
            (Opcode::BaseMul, vec![]),    // 50
            (Opcode::RangeCheck, vec![]), // 100
            (Opcode::ConstrainEqualBase, vec![]), // 5
            (Opcode::ConstrainInstance, vec![]),  // 5
        ];
        let diff = circuit_difficulty(&ops, K_REF);
        // Expected: ~6600 — a complex circuit ~6.6x average
        assert!(diff > 4000 && diff < 12000,
            "complex circuit difficulty should be 4000-12000, got {}", diff);
    }

    #[test]
    fn test_circuit_difficulty_simple() {
        // Simulate a very simple circuit (like FeeThreshold_V1)
        let ops = vec![
            (Opcode::WitnessBase, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainInstance, vec![]),
            (Opcode::ConstrainInstance, vec![]),
        ];
        let diff = circuit_difficulty(&ops, K_REF);
        // Expected: 20 + 5 + 5 + 5 + 5 = 40 — very simple
        assert!(diff < 500,
            "simple circuit difficulty should be <500, got {}", diff);
    }

    // ── k-scaling tests ──────────────────────────────────────────────

    #[test]
    fn test_k_scaling_at_reference_is_identity() {
        // At K_REF, scaling factor = 2^0 = 1 — no change
        let ops = vec![
            (Opcode::EcAdd, vec![]),      // 1000
            (Opcode::BaseMul, vec![]),    // 50
        ];
        assert_eq!(circuit_difficulty(&ops, 11), 1050);
    }

    #[test]
    fn test_k_scaling_doubles_per_increment() {
        // k=12: 2^1 = 2x scaling
        let ops = vec![(Opcode::BaseAdd, vec![])]; // 20
        assert_eq!(circuit_difficulty(&ops, 11), 20);
        assert_eq!(circuit_difficulty(&ops, 12), 40);
        assert_eq!(circuit_difficulty(&ops, 13), 80);
    }

    #[test]
    fn test_k_below_reference_no_fractional() {
        // k < K_REF: clamped to K_REF (no fractional reduction)
        let ops = vec![(Opcode::BaseAdd, vec![])]; // 20
        assert_eq!(circuit_difficulty(&ops, 5), 20);
        assert_eq!(circuit_difficulty(&ops, 10), 20);
    }

    #[test]
    fn test_k_above_max_capped() {
        // k > MAX_K (16): capped at MAX_K
        let ops = vec![(Opcode::BaseAdd, vec![])]; // 20
        let at_max = circuit_difficulty(&ops, 16); // 20 * 2^5 = 640
        assert_eq!(circuit_difficulty(&ops, 17), at_max);
        assert_eq!(circuit_difficulty(&ops, 20), at_max);
    }

    #[test]
    fn test_k_scaling_fee_threshold_v1() {
        // FeeThreshold_V1: k=11, simple constraints
        let ops = vec![
            (Opcode::WitnessBase, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainEqualBase, vec![]),
            (Opcode::ConstrainInstance, vec![]),
            (Opcode::ConstrainInstance, vec![]),
        ];
        let diff = circuit_difficulty(&ops, 11);
        // 20 + 5 + 5 + 5 + 5 = 40, k=11 → scale 2^0 = 1 → 40
        assert_eq!(diff, 40);
    }
}
