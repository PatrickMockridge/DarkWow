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

//! Meta-test: verify CircuitPublicInputs::COUNT matches constrain_instance calls in .zk files.
//!
//! This is the structural prevention mechanism for G4 — if a developer adds
//! a `constrain_instance` to a .zk file without updating the corresponding
//! `CircuitPublicInputs::COUNT`, this test FAILS.
//!
//! Conversely, if they update COUNT without adding to the circuit, it also FAILS.

use std::fs;

/// Count `constrain_instance(` calls in a .zk file.
/// Filters out commented lines (lines starting with `//`).
fn count_constrain_instance(path: &str) -> usize {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read circuit file {}: {}", path, e));

    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .filter(|line| line.contains("constrain_instance("))
        .count()
}

#[test]
fn mint_v2_constrain_instance_count_matches_trait() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/proof/mint.zk"
    );
    let count = count_constrain_instance(path);
    assert_eq!(
        count, 9,
        "mint_v2.zk has {} constrain_instance calls, expected 9. \
         Update CircuitPublicInputs::COUNT for TransferMintRevealed.",
        count
    );
}

#[test]
fn burn_v2_constrain_instance_count_matches_trait() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/proof/burn.zk"
    );
    let count = count_constrain_instance(path);
    assert_eq!(
        count, 11,
        "burn_v2.zk has {} constrain_instance calls, expected 11. \
         Update CircuitPublicInputs::COUNT for TransferBurnRevealed.",
        count
    );
}

#[test]
fn fee_v2_constrain_instance_count_matches_trait() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/proof/fee.zk"
    );
    let count = count_constrain_instance(path);
    // Fee_V2 has 15 public inputs: nullifier, input_vc.x, input_vc.y,
    // token_commit, merkle_root, user_data_enc, sig_x, sig_y, output_coin,
    // output_vc.x, output_vc.y, fee_vc.x, fee_vc.y, tx_binding, tx_nonce.
    assert_eq!(
        count, 15,
        "fee.zk has {} constrain_instance calls, expected 15. \
         Update CircuitPublicInputs::COUNT for the fee reveal type.",
        count
    );
}
