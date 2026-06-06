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

//! NativeToken Test Harness
//!
//! Provides isolated testing for NativeToken contract (consensus token).

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{MerkleNode, PublicKey, SecretKey},
    crypto::pasta_prelude::Group,
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_native_token_contract::{
    client::{
        pow_reward_v1::PoWRewardCallBuilder,
        burn_v1::BurnCallBuilder,
        fee_v1::{FeeCallBuilder, FeeCallInput, FeeCallOutput},
    },
    model::{FeeParamsV1, Output},
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
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&mint_zkbin).unwrap(), &mint_zkbin);
        let burn_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);
        let fee_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&fee_zkbin).unwrap(), &fee_zkbin);

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

    /// Build a PoW reward call (mint native tokens to miner)
    pub fn mint_pow_reward(
        &self,
        secret: SecretKey,
        ephemeral_signature_secret: SecretKey,
        block_height: u32,
        fees: u64,
        recipient: Option<PublicKey>,
    ) -> Result<PoWRewardResult, Box<dyn std::error::Error>> {
        let mint_zkbin = self.mint_zkbin.clone();
        let mint_pk = self.mint_pk.clone();

        let debris = PoWRewardCallBuilder {
            secret,
            ephemeral_signature_secret,
            block_height,
            fees,
            recipient,
            spend_hook: None,
            user_data: None,
            expected_cumulative_supply: 0,
            old_cumulative_commit: pallas::Point::identity(),
            old_cumulative_blind: pallas::Scalar::zero(),
            mint_zkbin,
            mint_pk,
        }
        .build()?;

        let mut call_data = vec![0x05];
        debris.params.encode(&mut call_data)?;

        Ok(PoWRewardResult {
            call_data,
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

    /// Build a fee call (pay network fees)
    pub fn fee(
        &self,
        input_value: u64,
        token_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
        secret: SecretKey,
        ephemeral_signature_secret: SecretKey,
        recipient: PublicKey,
        output_spend_hook: pallas::Base,
        output_user_data: pallas::Base,
        fee_amount: u64,
    ) -> Result<FeeResult, Box<dyn std::error::Error>> {
        let builder = FeeCallBuilder {
            input: FeeCallInput {
                value: input_value,
                token_id,
                spend_hook,
                user_data,
                coin_blind,
                leaf_position,
                merkle_path,
                secret,
                ephemeral_signature_secret,
            },
            output: FeeCallOutput {
                recipient,
                value: input_value - fee_amount,
                spend_hook: output_spend_hook,
                user_data: output_user_data,
                coin_blind,
            },
            fee_zkbin: self.fee_zkbin.clone(),
            fee_pk: self.fee_pk.clone(),
            fee: fee_amount,
        };

        let debris = builder.build()?;

        let mut call_data = vec![0x00];
        debris.params.encode(&mut call_data)?;

        Ok(FeeResult { call_data, params: debris.params, proofs: debris.proofs })
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
pub use dwow_native_token_contract::client::burn_v1::BurnCallInput;

/// Result of PoW reward minting
pub struct PoWRewardResult {
    pub call_data: Vec<u8>,
    pub output: Output,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of burn
pub struct BurnResult {
    pub inputs: Vec<dwow_native_token_contract::model::Input>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of fee
pub struct FeeResult {
    pub call_data: Vec<u8>,
    pub params: FeeParamsV1,
    pub proofs: Vec<dwow_core::zk::Proof>,
}