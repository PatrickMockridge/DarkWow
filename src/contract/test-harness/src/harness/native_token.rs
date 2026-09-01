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
    blockchain::{FeeAmount, FeeTier},
    crypto::{MerkleNode, PublicKey, SecretKey, poseidon_hash},
    crypto::pasta_prelude::Group,
    pasta::pallas,
};
use dwow_native_token_contract::{
    client::{
        pow_reward::PoWRewardCallBuilder,
        burn::BurnCallBuilder,
        fee::{FeeV2CallBuilder, FeeV2CallInput, FeeV2CallOutput},
    },
    model::{FeeParamsV3, Output},
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
}

impl NativeTokenHarness {
    /// Spawn a new NativeToken harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let mint_bin = include_bytes!("../../../native_token/proof/mint.zk.bin");
        let burn_bin = include_bytes!("../../../native_token/proof/burn.zk.bin");
        let fee_bin = include_bytes!("../../../native_token/proof/fee.zk.bin");

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

        let mint_pk = ProvingKey::build(mint_zkbin.k, &mint_circuit).expect("ProvingKey::build failed");
        let burn_pk = ProvingKey::build(burn_zkbin.k, &burn_circuit).expect("ProvingKey::build failed");
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit).expect("ProvingKey::build failed");

        Self {
            mint_zkbin, mint_pk,
            burn_zkbin, burn_pk,
            fee_zkbin, fee_pk,
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

        // Deterministic commitment_blind — same formula as PoWRewardCallBuilder
        // (pow_reward_v1.rs:164-167, DOMAIN_COIN_BLIND=3). Exposed so tests
        // can build fee/burn call_data referencing the minted commitment.
        let commitment_blind = poseidon_hash([
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
            commitment_blind,
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

    /// Build a FeeV3 call (plaintext fee payment).
    /// Produces [0x08][FeeParamsV3] call data with the Fee_V2 mass-balance proof.
    pub fn fee_v2(
        &self,
        input_value: u64,
        asset_id: pallas::Base,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
        commitment_blind: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
        merkle_root: MerkleNode,
        secret: SecretKey,
        ephemeral_signature_secret: SecretKey,
        recipient: PublicKey,
        output_spend_hook: pallas::Base,
        output_user_data: pallas::Base,
        fee_amount: u64,
    ) -> Result<FeeV2Result, Box<dyn std::error::Error>> {
        let builder = FeeV2CallBuilder {
            input: FeeV2CallInput {
                value: input_value,
                asset_id,
                spend_hook,
                user_data,
                commitment_blind,
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
                commitment_blind,
            },
            fee_amount: FeeAmount::new(fee_amount),
            tier: FeeTier::LOW,
            fee_zkbin: self.fee_zkbin.clone(),
            fee_pk: self.fee_pk.clone(),
        };

        let result = builder.build()
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        // FeeV3 call data: [0x08][FeeParamsV3 encoded] — plaintext fee
        let mut call_data = vec![0x08u8];
        call_data.extend_from_slice(&result.params.encode());

        Ok(FeeV2Result { call_data, params: result.params, proofs: result.proofs })
    }

    /// Build a transfer call (function code 0x03, ZK).
    /// Wraps the existing TransferCallBuilder with deterministic test inputs.
    pub fn transfer(
        &self,
        value: u64,
        asset_id: pallas::Base,
        secret: SecretKey,
        commitment_blind: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
        recipient_pub: PublicKey,
    ) -> Result<TransferResult, Box<dyn std::error::Error>> {
        use dwow_native_token_contract::client::transfer::TransferCallBuilder;
        use dwow_native_token_contract::model::{CommitmentAttributes, InputWitness};
        use rand::{rngs::OsRng, SeedableRng};

        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();

        use dwow_sdk::crypto::{Blind, AssetId, FuncId};
        let cb = Blind(commitment_blind);
        let cb2 = cb.clone();
        let input = InputWitness {
            value, asset_id, user_data,
            commitment_blind: cb,
            leaf_position, merkle_path,
        };
        // `public_key` is the note-encryption recipient (the address the note is
        // sealed to). The commitment's own public key is derived inside the mint proof
        // from a fresh per-output spend_secret (TransferCallBuilder::build), so a
        // transfer can target any recipient — not just the spender.
        let output = CommitmentAttributes {
            version: 0,
            public_key: recipient_pub,
            value,
            asset_id: AssetId::from_base(asset_id),
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
        // DZ-4: deterministic RNG in test mode for PI-7 replay.
        let debris = if dwow_native_token_contract::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            builder.build(&mut rng)?
        } else {
            let mut rng = OsRng;
            builder.build(&mut rng)?
        };
        let mut call_data = vec![0x03u8]; // TransferV1
        call_data.extend_from_slice(&debris.params.encode());
        Ok(TransferResult { call_data, proofs: debris.proofs, nullifier: debris.params.inputs[0].nullifier })
    }

    /// Build a spend call (function code 0x04, ZK).
    /// Single input burn + single output mint, same pattern as TransferV1.
    pub fn spend(
        &self,
        value: u64,
        asset_id: pallas::Base,
        secret: SecretKey,
        commitment_blind: pallas::Base,
        leaf_position: u64,
        merkle_path: Vec<MerkleNode>,
        recipient_pub: PublicKey,
    ) -> Result<SpendResult, Box<dyn std::error::Error>> {
        use dwow_native_token_contract::client::transfer::TransferCallBuilder;
        use dwow_native_token_contract::model::{CommitmentAttributes, InputWitness, SpendParamsV1};
        use dwow_sdk::crypto::{AssetId, FuncId};
        use rand::{rngs::OsRng, SeedableRng};

        let cb = dwow_sdk::crypto::Blind(commitment_blind);
        let cb2 = cb.clone();
        let user_data = pallas::Base::zero();

        let input = InputWitness {
            value, asset_id, user_data,
            commitment_blind: cb,
            leaf_position, merkle_path,
        };
        // `public_key` is the note-encryption recipient; the commitment's public key is
        // derived from a fresh per-output spend_secret inside the mint proof.
        let output = CommitmentAttributes {
            version: 0, public_key: recipient_pub, value,
            asset_id: AssetId::from_base(asset_id),
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
        // DZ-4: deterministic RNG in test mode for PI-7 replay.
        let debris = if dwow_native_token_contract::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            builder.build(&mut rng)?
        } else {
            let mut rng = OsRng;
            builder.build(&mut rng)?
        };

        // Wrap TransferCallDebris as SpendParamsV1 for function code 0x04
        let output_value = debris.params.output_values.into_iter().next().unwrap_or(0);
        let params = SpendParamsV1 {
            input: debris.params.inputs.into_iter().next()
                .unwrap_or_else(|| panic!("expected 1 input")),
            output: debris.params.outputs.into_iter().next()
                .unwrap_or_else(|| panic!("expected 1 output")),
            output_value,
            tx_binding: debris.params.tx_binding,
            tx_nonce: debris.params.tx_nonce,
        };
        let mut call_data = vec![0x04u8]; // SpendV1
        call_data.extend_from_slice(&params.encode());
        Ok(SpendResult { call_data, proofs: debris.proofs, nullifier: params.input.nullifier })
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
    pub commitment_blind: pallas::Base,
}

/// Result of burn
pub struct BurnResult {
    pub call_data: Vec<u8>,
    pub inputs: Vec<dwow_native_token_contract::model::Input>,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

/// Result of fee (FeeV3)
pub struct FeeV2Result {
    pub call_data: Vec<u8>,
    pub params: FeeParamsV3,
    pub proofs: Vec<dwow_core::zk::Proof>,
}

pub struct TransferResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub nullifier: dwow_sdk::crypto::Nullifier,
}

pub struct SpendResult {
    pub call_data: Vec<u8>,
    pub proofs: Vec<dwow_core::zk::Proof>,
    pub nullifier: dwow_sdk::crypto::Nullifier,
}