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

//! Minimal sync test using the new sync module

use std::sync::Arc;

use darkfi::{
    blockchain::{BlockInfo, Header},
    tx::Transaction,
    util::time::Timestamp,
    validator::sync::{apply_block, verify_block},
    Result,
};
use darkfi_sdk::crypto::SecretKey;
use smol::Executor;

async fn test_sync_verify_block_impl(_ex: Arc<Executor<'static>>) -> Result<()> {
    // 1. Genesis block (default)
    let genesis = BlockInfo::default();

    // 2. Create new header (must have timestamp > genesis timestamp)
    let header = Header::new(
        genesis.hash(),
        genesis.header.height + 1,
        0,
        genesis.header.timestamp.checked_add(1.into()).unwrap(),
    );

    // 3. Create block with default transaction
    let tx = Transaction::default();
    let mut block = BlockInfo::new_empty(header);
    block.append_txs(vec![tx]);
    block.sign(&SecretKey::from_bytes([0u8; 32]).unwrap());

    // 4. Verify block
    verify_block(&block, &genesis, &[]).await?;

    // 5. Apply block
    apply_block(&block).await?;

    Ok(())
}

#[test]
fn test_sync_verify_block() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new()
        .each(0..1, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                test_sync_verify_block_impl(ex.clone()).await.unwrap();
                drop(signal);
            })
        });

    Ok(())
}