/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License for
 * more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Genesis Builder Module
//!
//! Provides a shared function for building genesis ValidatorConfig
//! used by both the daemon (darkfid) and testing pipeline.
//!
//! This ensures identical genesis initialization across all environments.

use darkfi::blockchain::{BlockInfo, Blockchain, BlockchainOverlay};
use darkfi::validator::{utils::deploy_native_contracts, ValidatorConfig};
use darkfi::Result;
use num_bigint::BigUint;
use sled;

/// Build a genesis ValidatorConfig with fresh state.
/// Uses the same pattern as the testing pipeline for consistency.
///
/// This function:
/// 1. Creates a temporary in-memory sled database
/// 2. Deploys native contracts (Deployooor + Native Token)
/// 3. Computes the genesis state_root
/// 4. Returns a ValidatorConfig ready for use
pub async fn build_genesis_config(
    pow_target: u32,
    pow_fixed_difficulty: Option<BigUint>,
    confirmation_threshold: usize,
    max_forks: usize,
    verify_fees: bool,
) -> Result<ValidatorConfig> {
    // Generate default genesis block
    let mut genesis_block = BlockInfo::default();

    // Retrieve genesis producer transaction
    let producer_tx = genesis_block.txs.pop().unwrap();

    // Append it again so its added to the merkle tree
    genesis_block.append_txs(vec![producer_tx]);

    // Compute genesis contracts states monotree root
    let sled_db = sled::Config::new().temporary(true).open()?;
    let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;
    deploy_native_contracts(&overlay, pow_target).await?;
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&[])?;
    genesis_block.header.state_root =
        overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

    Ok(ValidatorConfig {
        confirmation_threshold,
        max_forks,
        pow_target,
        pow_fixed_difficulty,
        genesis_block: Some(genesis_block),
        verify_fees,
    })
}