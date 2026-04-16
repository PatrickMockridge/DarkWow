/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or version 3
 * or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! NativeToken Test Harness
//!
//! Provides isolated testing for NativeToken contract (consensus token).

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::*, Keypair},
    pasta::pallas,
};

use darkfi_native_token_contract::{
    client::{pow_reward_v1::PoWRewardCallBuilder, burn_v1::BurnCallBuilder},
    model::Output,
};

/// NativeToken Harness for isolated testing
pub struct NativeTokenHarness {
    /// Mint_V1 ZkBinary
    mint_zkbin: ZkBinary,
    /// Mint_V1 ProvingKey
    mint_pk: ProvingKey,
    /// Burn_V1 ZkBinary
    burn_zkbin: ZkBinary,
    /// Burn_V1 ProvingKey
    burn_pk: ProvingKey,
    /// Fee_V1 ZkBinary
    fee_zkbin: ZkBinary,
    /// Fee_V1 ProvingKey
    fee_pk: ProvingKey,
}

impl NativeTokenHarness {
    /// Spawn a new NativeToken harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let mint_bin = include_bytes!("../../../native_token/proof/mint_v1.zk.bin");
        let burn_bin = include_bytes!("../../../native_token/proof/burn_v1.zk.bin");
        let fee_bin = include_bytes!("../../../native_token/proof/fee_v1.zk.bin");

        let mint_zkbin = ZkBinary::decode(mint_bin, false).unwrap();
        let burn_zkbin = ZkBinary::decode(burn_bin, false).unwrap();
        let fee_zkbin = ZkBinary::decode(fee_bin, false).unwrap();

        // Build proving keys
        let mint_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&mint_zkbin).unwrap(), &mint_zkbin);
        let burn_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);
        let fee_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&fee_zkbin).unwrap(), &fee_zkbin);

        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit);
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        Self {
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
            fee_zkbin,
            fee_pk,
        }
    }

    /// Get circuit namespaces
    pub fn circuits(&self) -> Vec<&'static str> {
        vec!["Mint_V1", "Burn_V1", "Fee_V1"]
    }

    /// Build a PoW reward call (mint native tokens to miner)
    pub fn mint_pow_reward(
        &self,
        signature_keypair: Keypair,
        block_height: u32,
        fees: u64,
        recipient: Option<pallas::Base>,
    ) -> Result<PoWRewardResult, Box<dyn std::error::Error>> {
        let mint_zkbin = self.mint_zkbin.clone();
        let mint_pk = self.mint_pk.clone();

        let debris = PoWRewardCallBuilder {
            signature_keypair,
            block_height,
            fees,
            recipient: recipient.map(|r| {
                // Convert pallas::Base to PublicKey - use x coordinate
                use darkfi_sdk::crypto::PublicKey;
                // Create a public key from the base field (as x coordinate)
                // This is a simplification - in real usage, the recipient would be a proper PublicKey
                PublicKey::from_bytes(r.to_repr()).unwrap()
            }),
            spend_hook: None,
            user_data: None,
            mint_zkbin,
            mint_pk,
        }
        .build()?;

        Ok(PoWRewardResult {
            output: debris.params.output,
            proofs: debris.proofs,
        })
    }

    /// Build a burn call (destroy native tokens)
    pub fn burn(
        &self,
        inputs: Vec<BurnCallInput>,
    ) -> Result<BurnResult, Box<dyn std::error::Error>> {
        let burn_zkbin = self.burn_zkbin.clone();
        let burn_pk = self.burn_pk.clone();

        let debris = BurnCallBuilder { inputs, burn_zkbin, burn_pk }.build()?;

        Ok(BurnResult {
            inputs: debris.params.inputs,
            proofs: debris.proofs,
        })
    }
}

impl super::ContractHarness for NativeTokenHarness {
    fn name(&self) -> &str {
        "native_token"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["MintV1", "BurnV1", "FeeV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "MintV1" => Some(&self.mint_zkbin),
            "BurnV1" => Some(&self.burn_zkbin),
            "FeeV1" => Some(&self.fee_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "MintV1" => Some(&self.mint_pk),
            "BurnV1" => Some(&self.burn_pk),
            "FeeV1" => Some(&self.fee_pk),
            _ => None,
        }
    }
}

/// Input for burn call (re-exported from native_token contract)
pub use darkfi_native_token_contract::client::burn_v1::BurnCallInput;

/// Result of PoW reward minting
pub struct PoWRewardResult {
    pub output: Output,
    pub proofs: Vec<darkfi::zk::Proof>,
}

/// Result of burn
pub struct BurnResult {
    pub inputs: Vec<darkfi_native_token_contract::model::Input>,
    pub proofs: Vec<darkfi::zk::Proof>,
}