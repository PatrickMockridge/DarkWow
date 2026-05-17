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

//! DarkbetExchange Test Harness
//!
//! Provides isolated testing for DarkbetExchange contract.

use dwow::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, PublicKey, SecretKey, schnorr::Signature},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_darkbet_exchange_contract::client::{
    add_liquidity_v1::{add_liquidity_v1_proof, AddLiquidityV1CallData, AddLiquidityV1PublicInputs},
    buy_position_v1::{buy_position_v1_proof, BuyPositionV1CallData, BuyPositionV1PublicInputs},
    claim_winnings_v1::{claim_winnings_v1_proof, ClaimWinningsV1CallData, ClaimWinningsV1PublicInputs},
    create_market_v1::{create_market_v1_proof, CreateMarketV1CallData, CreateMarketV1PublicInputs},
};
use dwow_darkbet_exchange_contract::model::{
    AddLiquidityParamsV1, BuyPositionParamsV1, ClaimWinningsParamsV1, CreateMarketParamsV1,
};

/// DarkbetExchange Harness for isolated testing
pub struct DarkbetExchangeHarness {
    /// CreateMarket_V1 ZkBinary
    create_market_zkbin: ZkBinary,
    /// CreateMarket_V1 ProvingKey
    create_market_pk: ProvingKey,
    /// BuyPosition_V1 ZkBinary
    buy_position_zkbin: ZkBinary,
    /// BuyPosition_V1 ProvingKey
    buy_position_pk: ProvingKey,
    /// ClaimWinnings_V1 ZkBinary
    claim_winnings_zkbin: ZkBinary,
    /// ClaimWinnings_V1 ProvingKey
    claim_winnings_pk: ProvingKey,
    /// AddLiquidity_V1 ZkBinary
    add_liquidity_zkbin: ZkBinary,
    /// AddLiquidity_V1 ProvingKey
    add_liquidity_pk: ProvingKey,
}

impl DarkbetExchangeHarness {
    /// Spawn a new DarkbetExchange harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_market_bin =
            include_bytes!("../../../darkbet_exchange/proof/create_market_v1.zk.bin");
        let buy_position_bin =
            include_bytes!("../../../darkbet_exchange/proof/buy_position_v1.zk.bin");
        let claim_winnings_bin =
            include_bytes!("../../../darkbet_exchange/proof/claim_winnings_v1.zk.bin");
        let add_liquidity_bin =
            include_bytes!("../../../darkbet_exchange/proof/add_liquidity_v1.zk.bin");

        let create_market_zkbin = ZkBinary::decode(create_market_bin, false).unwrap();
        let buy_position_zkbin = ZkBinary::decode(buy_position_bin, false).unwrap();
        let claim_winnings_zkbin = ZkBinary::decode(claim_winnings_bin, false).unwrap();
        let add_liquidity_zkbin = ZkBinary::decode(add_liquidity_bin, false).unwrap();

        let create_market_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&create_market_zkbin).unwrap(),
            &create_market_zkbin,
        );
        let buy_position_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&buy_position_zkbin).unwrap(),
            &buy_position_zkbin,
        );
        let claim_winnings_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&claim_winnings_zkbin).unwrap(),
            &claim_winnings_zkbin,
        );
        let add_liquidity_circuit = ZkCircuit::new(
            dwow::zk::empty_witnesses(&add_liquidity_zkbin).unwrap(),
            &add_liquidity_zkbin,
        );

        let create_market_pk = ProvingKey::build(create_market_zkbin.k, &create_market_circuit);
        let buy_position_pk = ProvingKey::build(buy_position_zkbin.k, &buy_position_circuit);
        let claim_winnings_pk =
            ProvingKey::build(claim_winnings_zkbin.k, &claim_winnings_circuit);
        let add_liquidity_pk = ProvingKey::build(add_liquidity_zkbin.k, &add_liquidity_circuit);

        Self {
            create_market_zkbin,
            create_market_pk,
            buy_position_zkbin,
            buy_position_pk,
            claim_winnings_zkbin,
            claim_winnings_pk,
            add_liquidity_zkbin,
            add_liquidity_pk,
        }
    }

    /// Create a new market (AMM mode)
    pub fn create_market(
        &self,
        creator_pub_x: pallas::Base,
        creator_pub_y: pallas::Base,
        close_block: u64,
        block_height: u64,
        nonce: u64,
    ) -> Result<CreateMarketResult> {
        let input = CreateMarketV1CallData {
            creator_pub_x,
            creator_pub_y,
            close_block,
            block_height,
            nonce,
        };
        let (proof, public_inputs) = create_market_v1_proof(&self.create_market_zkbin, &self.create_market_pk, &input)?;

        // Build CreateMarketParamsV1 for call_data
        let creator_secret = SecretKey::from(pallas::Base::from(creator_pub_x));
        let creator_pub = PublicKey::from_secret(creator_secret);
        let params = CreateMarketParamsV1 {
            description: "Test Market".to_string(),
            outcomes: vec!["YES".to_string(), "NO".to_string()],
            oracle_id: pallas::Base::zero(),
            commission_bp: 200,
            market_type: 1, // AMM pool
            protocol_fee: 0,
            lp_fee: 0,
            duration_blocks: 1000,
            creator_pub,
            signature: Signature::dummy(),
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(CreateMarketResult { call_data, public_inputs, proof })
    }

    /// Buy a position on a market
    pub fn buy_position(
        &self,
        market_id: pallas::Base,
        owner_pub_x: pallas::Base,
        owner_pub_y: pallas::Base,
        outcome: u8,
        amount: u64,
        block_height: u64,
        value_blind: pallas::Scalar,
    ) -> Result<BuyPositionResult> {
        let input = BuyPositionV1CallData {
            market_id,
            owner_pub_x,
            owner_pub_y,
            outcome,
            amount,
            block_height,
            value_blind,
        };
        let (proof, public_inputs) = buy_position_v1_proof(&self.buy_position_zkbin, &self.buy_position_pk, &input)?;

        // Build BuyPositionParamsV1 for call_data
        let owner_secret = SecretKey::from(pallas::Base::from(owner_pub_x));
        let owner = PublicKey::from_secret(owner_secret);
        let params = BuyPositionParamsV1 {
            market_id,
            outcome,
            amount,
            min_payout: amount,
            owner,
            value_commit: pallas::Point::identity(),
            signature: Signature::dummy(),
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(BuyPositionResult { call_data, public_inputs, proof })
    }

    /// Claim winnings from a winning position
    pub fn claim_winnings(
        &self,
        market_id: pallas::Base,
        position_id: pallas::Base,
        owner_pub_x: pallas::Base,
        owner_pub_y: pallas::Base,
        winning_outcome: u8,
        block_height: u64,
        nonce: u64,
    ) -> Result<ClaimWinningsResult> {
        let input = ClaimWinningsV1CallData {
            market_id,
            position_id,
            owner_pub_x,
            owner_pub_y,
            winning_outcome,
            block_height,
            nonce,
        };
        let (proof, public_inputs) = claim_winnings_v1_proof(&self.claim_winnings_zkbin, &self.claim_winnings_pk, &input)?;

        // Build ClaimWinningsParamsV1 for call_data
        let owner_secret = SecretKey::from(pallas::Base::from(owner_pub_x));
        let owner = PublicKey::from_secret(owner_secret);
        let params = ClaimWinningsParamsV1 {
            position_id,
            market_id,
            winning_outcome,
            owner,
            proof: vec![],
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(ClaimWinningsResult { call_data, public_inputs, proof })
    }

    /// Add liquidity to a market's AMM pool
    pub fn add_liquidity(
        &self,
        market_id: pallas::Base,
        provider_pub_x: pallas::Base,
        provider_pub_y: pallas::Base,
        amount: u64,
        block_height: u64,
        value_blind: pallas::Scalar,
    ) -> Result<AddLiquidityResult> {
        let input = AddLiquidityV1CallData {
            market_id,
            provider_pub_x,
            provider_pub_y,
            amount,
            block_height,
            value_blind,
        };
        let (proof, public_inputs) = add_liquidity_v1_proof(&self.add_liquidity_zkbin, &self.add_liquidity_pk, &input)?;

        // Build AddLiquidityParamsV1 for call_data
        let provider_secret = SecretKey::from(pallas::Base::from(provider_pub_x));
        let provider = PublicKey::from_secret(provider_secret);
        let params = AddLiquidityParamsV1 {
            market_id,
            amount,
            provider,
            value_commit: pallas::Point::identity(),
            signature: Signature::dummy(),
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(AddLiquidityResult { call_data, public_inputs, proof })
    }
}

impl super::ContractHarness for DarkbetExchangeHarness {
    fn name(&self) -> &str {
        "darkbet_exchange"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateMarket", "BuyPosition", "ClaimWinnings", "AddLiquidity"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateMarket" => Some(&self.create_market_zkbin),
            "BuyPosition" => Some(&self.buy_position_zkbin),
            "ClaimWinnings" => Some(&self.claim_winnings_zkbin),
            "AddLiquidity" => Some(&self.add_liquidity_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateMarket" => Some(&self.create_market_pk),
            "BuyPosition" => Some(&self.buy_position_pk),
            "ClaimWinnings" => Some(&self.claim_winnings_pk),
            "AddLiquidity" => Some(&self.add_liquidity_pk),
            _ => None,
        }
    }
}

/// Result of create_market
pub struct CreateMarketResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateMarketV1PublicInputs,
    pub proof: dwow::zk::Proof,
}

/// Result of buy_position
pub struct BuyPositionResult {
    pub call_data: Vec<u8>,
    pub public_inputs: BuyPositionV1PublicInputs,
    pub proof: dwow::zk::Proof,
}

/// Result of claim_winnings
pub struct ClaimWinningsResult {
    pub call_data: Vec<u8>,
    pub public_inputs: ClaimWinningsV1PublicInputs,
    pub proof: dwow::zk::Proof,
}

/// Result of add_liquidity
pub struct AddLiquidityResult {
    pub call_data: Vec<u8>,
    pub public_inputs: AddLiquidityV1PublicInputs,
    pub proof: dwow::zk::Proof,
}