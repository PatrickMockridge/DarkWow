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

use darkfi::Result;
use darkfi_contract_test_harness::{
    contract_graph::Contract, init_logger, Holder, TestHarness,
};
use tracing::info;

#[test]
fn token_mint_burn() -> Result<()> {
    smol::block_on(async {
        init_logger();

        use Holder::{Alice, Bob};

        const BOB_SUPPLY: u64 = 2000000000; // 10 BOB

        let block_height = 0;

        let mut th = TestHarness::new(&[Alice, Bob], false, &[Contract::Money, Contract::MoneyV2]).await?;

        // Mint BOB token
        info!("Minting BOB token");
        let bob_token = th.token_mint_to_all(BOB_SUPPLY, &Bob, &Bob, block_height).await?;

        assert_eq!(th.coins(&Bob).len(), 1);
        assert_eq!(th.balance(&Bob, bob_token), BOB_SUPPLY);

        // Freeze BOB token authority
        info!("Freezing BOB token authority");
        th.token_freeze_to_all(&Bob, block_height).await?;

        // Burn the BOB tokens (single coin supply)
        info!("Burning BOB token");
        let bob_coins = th.coins(&Bob).to_vec();
        th.burn_to_all(&Bob, &bob_coins, block_height).await?;
        assert!(th.coins(&Bob).is_empty());

        // Thanks for reading
        Ok(())
    })
}
