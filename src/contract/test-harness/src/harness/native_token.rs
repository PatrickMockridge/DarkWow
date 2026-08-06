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
    crypto::{MerkleNode, PublicKey, SecretKey, poseidon_hash},
    crypto::pasta_prelude::Group,
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_native_token_contract::{
    client::{
        pow_reward::PoWRewardCallBuilder,
        burn::BurnCallBuilder,
        fee::{FeeV2CallBuilder, FeeV2CallInput, FeeV2CallOutput},
    },
    model::{FeeParamsV2, Output},
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
    /// Fee_V2 ZkBinary — used by both FeeV1 (deprecated) and FeeV2
    fee_zkbin: ZkBinary,
    /// Fee_V2 ProvingKey
    fee_pk: ProvingKey,
    /// FeeThreshold_V1 ZkBinary (FeeV2 — threshold proof)
    threshold_zkbin: ZkBinary,
    /// FeeThreshold_V1 ProvingKey
    threshold_pk: ProvingKey,
}

impl NativeTokenHarness {
    /// Spawn a new NativeToken harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let mint_bin = include_bytes!("../../../native_token/proof/mint.zk.bin");
        let burn_bin = include_bytes!("../../../native_token/proof/burn.zk.bin");
        let fee_bin = include_bytes!("../../../native_token/proof/fee.zk.bin");
        let threshold_bin = include_bytes!("../../../native_token/proof/fee_threshold_v1.zk.bin");

        let mint_zkbin = ZkBinary::decode(mint_bin, false).unwrap();
        let burn_zkbin = ZkBinary::decode(burn_bin, false).unwrap();
        let fee_zkbin = ZkBinary::decode(fee_bin, false).unwrap();
        let threshold_zkbin = ZkBinary::decode(threshold_bin, false).unwrap();

        // Build proving keys
        let mint_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&mint_zkbin).unwrap(), &mint_zkbin);
        let burn_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&burn_zkbin).unwrap(), &burn_zkbin);
        let fee_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&fee_zkbin).unwrap(), &fee_zkbin);
        let threshold_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&threshold_zkbin).unwrap(), &threshold_zkbin);

        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit).expect("ProvingKey::build failed");
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit).expect("ProvingKey::build failed");
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit).expect("ProvingKey::build failed");
        let threshold_pk = ProvingKey::build(threshold_zkbin.k, &threshold_circuit).expect("ProvingKey::build failed");

        Self {
            mint_zkbin, mint_pk,
            burn_zkbin, burn_pk,
            fee_zkbin, fee_pk,
            threshold_zkbin, threshold_pk,
        }
    }

    /// Build a PoW reward call (mint native tokens to miner)
    pub fn mint_pow_reward(
        &self,
        secret: SecretKey,
        ephemeral_signature_secret: SecretKey,
        block_height: dwow_sdk::blockchain::BlockHeight,
        fees: u64,
        recipient: Option<PublicKey>,
    ) -> Result<PoWRewardResult, Box<dyn std::error::Error>> {
        let mint_zkbin = self.mint_zkbin.clone();
        let mint_pk = self.mint_pk.clone();

        let debris = PoWRewardCallBuilder {
            secret: secret.clone(),
            ephemeral_signature_secret,
            block_height,
            fees,
            recipient,
            spend_hook: None,
            user_data: None,
            expected_cumulative_supply: 0,
            old_cumulative_commit: pallas::Point::identity(),
            old_cumulative_blind: pallas::Scalar::zero(),
            old_total_supply: 0,
            mint_zkbin,
            mint_pk,
            tx_nonce: pallas::Base::zero(),
            tx_commitment: pallas::Base::zero(),
        }
        .build()?;

        // Deterministic coin_blind — same formula as PoWRewardCallBuilder
        // (pow_reward_v1.rs:164-167, DOMAIN_COIN_BLIND=3). Exposed so tests
        // can build fee/burn call_data referencing the minted coin.
        let coin_blind = poseidon_hash([
            *secret.inner(),
            pallas::Base::from(block_height.get()),
            pallas::Base::from(3u64),
        ]);

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&debris.params.encode());

        Ok(PoWRewardResult {
            call_data,
            output: debris.params.output,
            proofs: debris.proofs,
            coin_blind,
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

        let mut call_data = vec![0x02u8]; // BurnV1
        call_data.extend_from_slice(&debris.params.encode());

        Ok(BurnResult {
            call_data,
            inputs: debris.params.inputs,
            proofs: debris.proofs,
        })
    }

    /// Build a FeeV2 call (privacy-preserving fee payment).
    /// Produces [0x08][FeeParamsV2] call data with dual ZK proofs
    /// (Fee_V2 + FeeThreshold_V1). The fee amount is hidden behind a
    /// Pedersen commitment.
    pub fn fee_v2(
        &self,
        input_value: u64,
        token_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        coin_blind: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
        merkle_root: MerkleNode,
        secret: SecretKey,
        ephemeral_signature_secret: SecretKey,
        recipient: PublicKey,
        output_spend_hook: pallas::Base,
        output_user_data: pallas::Base,
        fee_amount: u64,
        threshold: u64,
    ) -> Result<FeeV2Result, Box<dyn std::error::Error>> {
        let builder = FeeV2CallBuilder {
            input: FeeV2CallInput {
                value: input_value,
                token_id,
                spend_hook,
                user_data,
                coin_blind,
                leaf_position,
                merkle_path,
                merkle_root,
                secret,
                ephemeral_signature_secret,
                tx_nonce: pallas::Base::zero(),
                tx_commitment: pallas::Base::zero(),
            },
            output: FeeV2CallOutput {
                recipient,
                value: input_value - fee_amount,
                spend_hook: output_spend_hook,
                user_data: output_user_data,
                coin_blind,
            },
            fee_amount,
            threshold,
            fee_zkbin: self.fee_zkbin.clone(),
            fee_pk: self.fee_pk.clone(),
            threshold_zkbin: self.threshold_zkbin.clone(),
            threshold_pk: self.threshold_pk.clone(),
        };

        let result = builder.build()
            .map_err(|e| format!("FeeV2 build failed: {:?}", e))?;

        // FeeV2 call data: [0x08][FeeParamsV2 encoded] — NO clear-text fee bytes
        let mut call_data = vec![0x08u8];
        call_data.extend_from_slice(&result.params.encode());

        Ok(FeeV2Result { call_data, params: result.params, proofs: result.proofs })
    }

    /// Build a transfer call (function code 0x03, ZK).
    /// Wraps the existing TransferCallBuilder with deterministic test inputs.
    pub fn transfer(
        &self,
        value: u64,
        token_id: pallas::Base,
        secret: SecretKey,
        coin_blind: pallas::Base,
        recipient_pub: PublicKey,
    ) -> Result<TransferResult, Box<dyn std::error::Error>> {
        use dwow_native_token_contract::client::transfer::TransferCallBuilder;
        use dwow_native_token_contract::model::{CoinAttributes, InputWitness};
        use dwow_sdk::crypto::MerkleNode;
        use rand::rngs::OsRng;

        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();
        let merkle_path = vec![MerkleNode::new(pallas::Base::from(0u64)); 32];
        let leaf_position = 0u64;

        use dwow_sdk::crypto::{Blind, TokenId, FuncId};
        let cb = Blind(coin_blind);
        let cb2 = cb.clone();
        let input = InputWitness {
            value, token_id, user_data,
            coin_blind: cb,
            leaf_position, merkle_path,
        };
        let output = CoinAttributes {
            version: 0,
            public_key: recipient_pub,
            value,
            token_id: TokenId::from_base(token_id),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: cb2,
        };

        let builder = TransferCallBuilder {
            inputs: vec![(input, secret.clone(), pallas::Base::zero())],
            outputs: vec![output],
            burn_zkbin: self.burn_zkbin.clone(),
            burn_pk: self.burn_pk.clone(),
            mint_zkbin: self.mint_zkbin.clone(),
            mint_pk: self.mint_pk.clone(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let debris = builder.build(&mut OsRng)?;
        let mut call_data = vec![0x03u8]; // TransferV1
        call_data.extend_from_slice(&debris.params.encode());
        Ok(TransferResult { call_data, proofs: debris.proofs })
    }

    /// Build a spend call (function code 0x04, ZK).
    /// Single input burn + single output mint, same pattern as TransferV1.
    pub fn spend(
        &self,
        value: u64,
        token_id: pallas::Base,
        secret: SecretKey,
        coin_blind: pallas::Base,
        recipient_pub: PublicKey,
    ) -> Result<SpendResult, Box<dyn std::error::Error>> {
        use dwow_native_token_contract::client::transfer::TransferCallBuilder;
        use dwow_native_token_contract::model::{CoinAttributes, InputWitness, SpendParamsV1};
        use dwow_sdk::crypto::{Blind, TokenId, FuncId, MerkleNode};
        use rand::rngs::OsRng;

        let cb = dwow_sdk::crypto::Blind(coin_blind);
        let cb2 = cb.clone();
        let user_data = pallas::Base::zero();
        let merkle_path = vec![MerkleNode::new(pallas::Base::from(0u64)); 32];

        let input = InputWitness {
            value, token_id, user_data,
            coin_blind: cb,
            leaf_position: 0u64, merkle_path,
        };
        let output = CoinAttributes {
            version: 0, public_key: recipient_pub, value,
            token_id: TokenId::from_base(token_id),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: cb2,
        };

        let builder = TransferCallBuilder {
            inputs: vec![(input, secret.clone(), pallas::Base::zero())],
            outputs: vec![output],
            burn_zkbin: self.burn_zkbin.clone(), burn_pk: self.burn_pk.clone(),
            mint_zkbin: self.mint_zkbin.clone(), mint_pk: self.mint_pk.clone(),
            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
        };
        let debris = builder.build(&mut OsRng)?;

        // Wrap TransferCallDebris as SpendParamsV1 for function code 0x04
        let params = SpendParamsV1 {
            input: debris.params.inputs.into_iter().next()
                .unwrap_or_else(|| panic!("expected 1 input")),
            output: debris.params.outputs.into_iter().next()
                .unwrap_or_else(|| panic!("expected 1 output")),
            tx_binding: debris.params.tx_binding,
            tx_nonce: debris.params.tx_nonce,
        };
        let mut call_data = vec![0x04u8]; // SpendV1
        call_data.extend_from_slice(&params.encode());
        Ok(SpendResult { call_data, proofs: debris.proofs })
    }
}

impl super::ContractHarness for NativeTokenHarness {
    fn name(&self) -> &str {
        "native_token"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["MintV2", "BurnV2", "FeeV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "MintV2" => Some(&self.mint_zkbin),
            "BurnV2" => Some(&self.burn_zkbin),
            "FeeV2" => Some(&self.fee_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "MintV2" => Some(&self.mint_pk),
            "BurnV2" => Some(&self.burn_pk),
            "FeeV2" => Some(&self.fee_pk),
            _ => None,
        }
    }
}

/// Input for burn call (re-exported from native_token contract)
pub use dwow_native_token_contract::client::burn::BurnCallInput;

/// Result of PoW reward minting
pub struct PoWRewardResult {
    pub call_data: Vec<u8>,
    pub output: Output,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub coin_blind: pallas::Base,
}

/// Result of burn
pub struct BurnResult {
    pub call_data: Vec<u8>,
    pub inputs: Vec<dwow_native_token_contract::model::Input>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of fee (FeeV2)
pub struct FeeV2Result {
    pub call_data: Vec<u8>,
    pub params: FeeParamsV2,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

pub struct TransferResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

pub struct SpendResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}