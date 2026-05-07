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

//! Uncle Merkle consensus correctness tests for 5-node local testnet.
//!
//! These tests verify Phase 1 of Uncle Merkle: blocks with
//! uncle_merkle_root = [0; 32] (no uncles) are accepted by all 5 nodes
//! and all nodes converge on the same canonical chain.

use std::sync::Arc;

use darkfi::{
    blockchain::{BlockchainOverlay, Blockchain, BlockInfo},
    validator::{utils::deploy_native_contracts, ValidatorConfig},
    Result,
};
use darkfi_sdk::num_traits::One;
use num_bigint::BigUint;
use sled;
use smol::Executor;

use crate::tests::localnet::FiveNodeHarness;

async fn make_validator_config() -> Result<ValidatorConfig> {
    let pow_target = 120;
    let pow_fixed_difficulty = Some(BigUint::one());

    let mut genesis_block = BlockInfo::default();
    let producer_tx = genesis_block.txs.pop().unwrap();
    genesis_block.append_txs(vec![producer_tx]);

    let sled_db = sled::Config::new().temporary(true).open()?;
    let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;
    deploy_native_contracts(&overlay, pow_target).await?;
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&[])?;
    genesis_block.header.state_root =
        overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

    Ok(ValidatorConfig {
        confirmation_threshold: 3,
        max_forks: 8,
        pow_target,
        pow_fixed_difficulty,
        genesis_block: Some(genesis_block),
        verify_fees: false,
    })
}

async fn five_node_consensus_impl(ex: Arc<Executor<'static>>) -> Result<()> {
    let validator_config = make_validator_config().await?;
    let th = FiveNodeHarness::new(validator_config, &ex).await?;

    // Generate a fork on alice and produce 5 blocks
    let mut fork = th.alice.validator.read().await.consensus.forks[0].full_clone()?;

    let block1 = th.generate_next_block(&mut fork).await?;
    let block2 = th.generate_next_block(&mut fork).await?;
    let block3 = th.generate_next_block(&mut fork).await?;
    let block4 = th.generate_next_block(&mut fork).await?;
    let block5 = th.generate_next_block(&mut fork).await?;

    // Verify Phase 1: uncle_merkle_root must be zero in all generated blocks
    for block in &[&block1, &block2, &block3, &block4, &block5] {
        assert_eq!(
            block.header.uncle_merkle_root,
            [0u8; 32],
            "Phase 1: uncle_merkle_root must be zero (no uncles)"
        );
    }

    // Broadcast all blocks to all 5 nodes via alice's P2P
    th.add_blocks(&[block1, block2, block3, block4, block5]).await?;

    // After confirmation_threshold=3 blocks, 2 should be canonical + 2 in fork
    // (genesis + block1 + block2 = 3 canonical, then fork has block3..block5)
    th.verify_consensus(3).await?;
    th.verify_tip_agreement().await?;

    Ok(())
}

#[test]
fn five_node_consensus() -> Result<()> {
    // 5 nodes + P2P futures need a larger stack than the default 2MB
    let handler = std::thread::Builder::new()
        .stack_size(128 * 1024 * 1024)
        .spawn(|| -> Result<()> {
            let ex = Arc::new(Executor::new());
            let (signal, shutdown) = smol::channel::unbounded::<()>();

            easy_parallel::Parallel::new()
                .each(0..4, |_| smol::block_on(ex.run(shutdown.recv())))
                .finish(|| {
                    smol::block_on(async {
                        five_node_consensus_impl(ex.clone()).await.unwrap();
                        drop(signal);
                    })
                });

            Ok(())
        })?;

    handler.join().unwrap()
}
