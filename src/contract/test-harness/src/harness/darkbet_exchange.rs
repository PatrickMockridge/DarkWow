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

use dwow_core::{
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
    add_liquidity::{add_liquidity_v1_proof, AddLiquidityV1CallData, AddLiquidityV1PublicInputs},
    auth_proof::{create_auth_proof, AuthCallData},
    buy_position::{buy_position_v1_proof, BuyPositionV1CallData, BuyPositionV1PublicInputs},
    claim_winnings::{claim_winnings_v1_proof, ClaimWinningsV1CallData, ClaimWinningsV1PublicInputs},
    create_market::{create_market_v1_proof, CreateMarketV1CallData, CreateMarketV1PublicInputs},
};
use dwow_darkbet_exchange_contract::model::{
    AddLiquidityParamsV1, BuyPositionParamsV1, CancelOrderParamsV1, ClaimWinningsParamsV1,
    CreateMarketParamsV1, MatchOrdersParamsV1, PlaceBackParamsV1, PlaceLayParamsV1,
    RemoveLiquidityParamsV1, ResolveMarketParamsV1, SettleMarketParamsV1,
};

/// DarkbetExchange Harness for isolated testing
pub struct DarkbetExchangeHarness {
    /// This contract's id — needed to frame a single-call transaction's commitment (`OBL-C198`).
    contract_id: dwow_sdk::crypto::ContractId,
    /// CreateMarketV2 ZkBinary
    create_market_zkbin: ZkBinary,
    /// CreateMarketV2 ProvingKey
    create_market_pk: ProvingKey,
    /// BuyPositionV2 ZkBinary
    buy_position_zkbin: ZkBinary,
    /// BuyPositionV2 ProvingKey
    buy_position_pk: ProvingKey,
    /// ClaimWinningsV2 ZkBinary
    claim_winnings_zkbin: ZkBinary,
    /// ClaimWinningsV2 ProvingKey
    claim_winnings_pk: ProvingKey,
    /// AddLiquidityV2 ZkBinary
    add_liquidity_zkbin: ZkBinary,
    /// AddLiquidityV2 ProvingKey
    add_liquidity_pk: ProvingKey,
    /// CancelOrderV2 ZkBinary
    cancel_order_zkbin: ZkBinary,
    /// CancelOrderV2 ProvingKey
    cancel_order_pk: ProvingKey,
    /// MatchOrdersV2 ZkBinary
    match_orders_zkbin: ZkBinary,
    /// MatchOrdersV2 ProvingKey
    match_orders_pk: ProvingKey,
    /// PlaceBackV2 ZkBinary
    place_back_zkbin: ZkBinary,
    /// PlaceBackV2 ProvingKey
    place_back_pk: ProvingKey,
    /// PlaceLayV2 ZkBinary
    place_lay_zkbin: ZkBinary,
    /// PlaceLayV2 ProvingKey
    place_lay_pk: ProvingKey,
    /// RemoveLiquidityV2 ZkBinary
    remove_liquidity_zkbin: ZkBinary,
    /// RemoveLiquidityV2 ProvingKey
    remove_liquidity_pk: ProvingKey,
    /// ResolveMarketV2 ZkBinary
    resolve_market_zkbin: ZkBinary,
    /// ResolveMarketV2 ProvingKey
    resolve_market_pk: ProvingKey,
}

impl DarkbetExchangeHarness {
    /// Spawn a new DarkbetExchange harness with pre-loaded circuits.
    ///
    /// Takes the contract's id because a one-step call must frame the transaction commitment over
    /// its own `ContractCall`, and that needs the id (`OBL-C198`). `slot`'s harness takes it the
    /// same way.
    pub fn spawn(contract_id: dwow_sdk::crypto::ContractId) -> Self {
        dwow_darkbet_exchange_contract::enable_deterministic_zk();
        let create_market_bin =
            include_bytes!("../../../darkbet_exchange/proof/create_market.zk.bin");
        let buy_position_bin =
            include_bytes!("../../../darkbet_exchange/proof/buy_position.zk.bin");
        let claim_winnings_bin =
            include_bytes!("../../../darkbet_exchange/proof/claim_winnings.zk.bin");
        let add_liquidity_bin =
            include_bytes!("../../../darkbet_exchange/proof/add_liquidity.zk.bin");
        let cancel_order_bin =
            include_bytes!("../../../darkbet_exchange/proof/cancel_order.zk.bin");
        let match_orders_bin =
            include_bytes!("../../../darkbet_exchange/proof/match_orders.zk.bin");
        let place_back_bin =
            include_bytes!("../../../darkbet_exchange/proof/place_back.zk.bin");
        let place_lay_bin =
            include_bytes!("../../../darkbet_exchange/proof/place_lay.zk.bin");
        let remove_liquidity_bin =
            include_bytes!("../../../darkbet_exchange/proof/remove_liquidity.zk.bin");
        let resolve_market_bin =
            include_bytes!("../../../darkbet_exchange/proof/resolve_market.zk.bin");

        let create_market_zkbin = ZkBinary::decode(create_market_bin, false).unwrap();
        let buy_position_zkbin = ZkBinary::decode(buy_position_bin, false).unwrap();
        let claim_winnings_zkbin = ZkBinary::decode(claim_winnings_bin, false).unwrap();
        let add_liquidity_zkbin = ZkBinary::decode(add_liquidity_bin, false).unwrap();
        let cancel_order_zkbin = ZkBinary::decode(cancel_order_bin, false).unwrap();
        let match_orders_zkbin = ZkBinary::decode(match_orders_bin, false).unwrap();
        let place_back_zkbin = ZkBinary::decode(place_back_bin, false).unwrap();
        let place_lay_zkbin = ZkBinary::decode(place_lay_bin, false).unwrap();
        let remove_liquidity_zkbin = ZkBinary::decode(remove_liquidity_bin, false).unwrap();
        let resolve_market_zkbin = ZkBinary::decode(resolve_market_bin, false).unwrap();

        let create_market_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_market_zkbin).unwrap(),
            &create_market_zkbin,
        );
        let buy_position_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&buy_position_zkbin).unwrap(),
            &buy_position_zkbin,
        );
        let claim_winnings_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&claim_winnings_zkbin).unwrap(),
            &claim_winnings_zkbin,
        );
        let add_liquidity_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&add_liquidity_zkbin).unwrap(),
            &add_liquidity_zkbin,
        );
        let cancel_order_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&cancel_order_zkbin).unwrap(),
            &cancel_order_zkbin,
        );
        let match_orders_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&match_orders_zkbin).unwrap(),
            &match_orders_zkbin,
        );
        let place_back_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&place_back_zkbin).unwrap(),
            &place_back_zkbin,
        );
        let place_lay_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&place_lay_zkbin).unwrap(),
            &place_lay_zkbin,
        );
        let remove_liquidity_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&remove_liquidity_zkbin).unwrap(),
            &remove_liquidity_zkbin,
        );
        let resolve_market_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&resolve_market_zkbin).unwrap(),
            &resolve_market_zkbin,
        );

        let create_market_pk = ProvingKey::build(create_market_zkbin.k, &create_market_circuit).expect("ProvingKey::build failed");
        let buy_position_pk = ProvingKey::build(buy_position_zkbin.k, &buy_position_circuit).expect("ProvingKey::build failed");
        let claim_winnings_pk =
            ProvingKey::build(claim_winnings_zkbin.k, &claim_winnings_circuit).expect("ProvingKey::build failed");
        let add_liquidity_pk = ProvingKey::build(add_liquidity_zkbin.k, &add_liquidity_circuit).expect("ProvingKey::build failed");
        let cancel_order_pk = ProvingKey::build(cancel_order_zkbin.k, &cancel_order_circuit).expect("ProvingKey::build failed");
        let match_orders_pk = ProvingKey::build(match_orders_zkbin.k, &match_orders_circuit).expect("ProvingKey::build failed");
        let place_back_pk = ProvingKey::build(place_back_zkbin.k, &place_back_circuit).expect("ProvingKey::build failed");
        let place_lay_pk = ProvingKey::build(place_lay_zkbin.k, &place_lay_circuit).expect("ProvingKey::build failed");
        let remove_liquidity_pk =
            ProvingKey::build(remove_liquidity_zkbin.k, &remove_liquidity_circuit).expect("ProvingKey::build failed");
        let resolve_market_pk =
            ProvingKey::build(resolve_market_zkbin.k, &resolve_market_circuit).expect("ProvingKey::build failed");

        Self {
            contract_id,
            create_market_zkbin,
            create_market_pk,
            buy_position_zkbin,
            buy_position_pk,
            claim_winnings_zkbin,
            claim_winnings_pk,
            add_liquidity_zkbin,
            add_liquidity_pk,
            cancel_order_zkbin,
            cancel_order_pk,
            match_orders_zkbin,
            match_orders_pk,
            place_back_zkbin,
            place_back_pk,
            place_lay_zkbin,
            place_lay_pk,
            remove_liquidity_zkbin,
            remove_liquidity_pk,
            resolve_market_zkbin,
            resolve_market_pk,
        }
    }

    /// The frame a **single-call** transaction commits to (`OBL-C198`). A one-step caller has no
    /// siblings to include, so this is exactly the value the node derives and the arm reads — which
    /// is what makes the one-step form bind something real rather than a zero.
    fn own_frame(&self, call_data: &[u8]) -> pallas::Base {
        dwow_sdk::crypto::util::tx_commitment([&dwow_sdk::tx::ContractCall {
            contract_id: self.contract_id,
            data: call_data.to_vec(),
        }])
    }

    /// Create a new market
    pub fn create_market_prepare(
        &self,
        creator_secret: pallas::Base,
        oracle_id: pallas::Base,
        nonce: u64,
        duration_blocks: u64,
        market_type: u8,
    ) -> Result<CreateMarketPlan> {
        let creator_public = PublicKey::from_secret(SecretKey::from_base(creator_secret));
        let close_block = nonce + duration_blocks;
        let input = CreateMarketV1CallData::new(creator_public, creator_secret, close_block, nonce);

        let params = CreateMarketParamsV1 {
            description: "Test Market".to_string(),
            outcomes: vec!["YES".to_string(), "NO".to_string()],
            oracle_id,
            commission_bp: 200,
            market_type,
            protocol_fee: 0,
            lp_fee: 0,
            duration_blocks,
            creator_pub: creator_public,
            signature: Signature::dummy(),
            instance_seed: [0u8; 32],
            nonce,
            // The client's own derivation, read rather than re-derived (`OBL-C198`).
            nullifier: input.compute_public_inputs().computed_nullifier,
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        Ok(CreateMarketPlan { call_data, input, zkbin: self.create_market_zkbin.clone(), pk: self.create_market_pk.clone() })
    }

    /// Create a new market and prove it in one step, framing this call alone. A caller whose call is
    /// a *parent* with children uses [`Self::create_market_prepare`] and frames the whole set.
    pub fn create_market(
        &self,
        creator_secret: pallas::Base,
        oracle_id: pallas::Base,
        nonce: u64,
        duration_blocks: u64,
        market_type: u8,
    ) -> Result<CreateMarketResult> {
        let plan = self.create_market_prepare(creator_secret, oracle_id, nonce, duration_blocks, market_type)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let (proof, public_inputs) = plan.prove(commitment, pallas::Base::zero())?;
        Ok(CreateMarketResult { call_data, public_inputs, proof })
    }

    /// Buy a position on a market
    pub fn buy_position_prepare(
        &self,
        market_id: pallas::Base,
        owner_secret: pallas::Base,
        outcome: u8,
        amount: u64,
        nonce: u64,
    ) -> Result<BuyPositionPlan> {
        let owner_public = PublicKey::from_secret(SecretKey::from_base(owner_secret));
        let input = BuyPositionV1CallData::new(market_id, owner_public, owner_secret, outcome, amount, nonce);

        let params = BuyPositionParamsV1 {
            market_id,
            outcome,
            amount,
            min_payout: amount,
            owner: owner_public,
            value_commit: pallas::Point::identity(),
            signature: Signature::dummy(),
            instance_seed: [0u8; 32],
            nonce,
            nullifier: input.compute_public_inputs().computed_nullifier,
        };

        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());

        Ok(BuyPositionPlan { call_data, input, zkbin: self.buy_position_zkbin.clone(), pk: self.buy_position_pk.clone() })
    }

    /// Buy a position and prove it in one step, framing this call alone.
    pub fn buy_position(
        &self,
        market_id: pallas::Base,
        owner_secret: pallas::Base,
        outcome: u8,
        amount: u64,
        nonce: u64,
    ) -> Result<BuyPositionResult> {
        let plan = self.buy_position_prepare(market_id, owner_secret, outcome, amount, nonce)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let (proof, public_inputs) = plan.prove(commitment, pallas::Base::zero())?;
        Ok(BuyPositionResult { call_data, public_inputs, proof })
    }

    /// Claim winnings from a winning position
    pub fn claim_winnings_prepare(
        &self,
        market_id: pallas::Base,
        position_id: pallas::Base,
        owner_secret: pallas::Base,
        winning_outcome: u8,
        amount: u64,
    ) -> Result<ClaimWinningsPlan> {
        let owner_public = PublicKey::from_secret(SecretKey::from_base(owner_secret));
        let input = ClaimWinningsV1CallData::new(market_id, owner_public, owner_secret, winning_outcome, amount, 0u64);

        let params = ClaimWinningsParamsV1 {
            position_id,
            market_id,
            winning_outcome,
            owner: owner_public,
            amount,
            proof: vec![],
        };

        let mut call_data = vec![0x0A];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        Ok(ClaimWinningsPlan { call_data, input, zkbin: self.claim_winnings_zkbin.clone(), pk: self.claim_winnings_pk.clone() })
    }

    /// Claim winnings and prove it in one step, framing this call alone.
    pub fn claim_winnings(
        &self,
        market_id: pallas::Base,
        position_id: pallas::Base,
        owner_secret: pallas::Base,
        winning_outcome: u8,
        amount: u64,
    ) -> Result<ClaimWinningsResult> {
        let plan = self.claim_winnings_prepare(market_id, position_id, owner_secret, winning_outcome, amount)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let (proof, public_inputs) = plan.prove(commitment, pallas::Base::zero())?;
        Ok(ClaimWinningsResult { call_data, public_inputs, proof })
    }

    /// Add liquidity to a market's AMM pool
    pub fn add_liquidity_prepare(
        &self,
        market_id: pallas::Base,
        provider_secret: pallas::Base,
        amount: u64,
        nonce: u64,
    ) -> Result<AddLiquidityPlan> {
        let provider_public = PublicKey::from_secret(SecretKey::from_base(provider_secret));
        let input = AddLiquidityV1CallData::new(market_id, provider_public, provider_secret, amount, nonce);

        let params = AddLiquidityParamsV1 {
            market_id,
            amount,
            provider: provider_public,
            value_commit: pallas::Point::identity(),
            signature: Signature::dummy(),
            instance_seed: [0u8; 32],
            nonce,
            nullifier: input.compute_public_inputs().computed_nullifier,
        };

        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&params.encode());

        Ok(AddLiquidityPlan { call_data, input, zkbin: self.add_liquidity_zkbin.clone(), pk: self.add_liquidity_pk.clone() })
    }

    /// Add liquidity and prove it in one step, framing this call alone.
    pub fn add_liquidity(
        &self,
        market_id: pallas::Base,
        provider_secret: pallas::Base,
        amount: u64,
        nonce: u64,
    ) -> Result<AddLiquidityResult> {
        let plan = self.add_liquidity_prepare(market_id, provider_secret, amount, nonce)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let (proof, public_inputs) = plan.prove(commitment, pallas::Base::zero())?;
        Ok(AddLiquidityResult { call_data, public_inputs, proof })
    }

    /// Place a back bet (function code 0x01) — the call data, and no proof yet (`OBL-C198`).
    pub fn place_back_prepare(
        &self,
        market_id: pallas::Base,
        user_secret: pallas::Base,
        outcome_index: u8,
        odds: u32,
        stake: u64,
    ) -> Result<PlaceBackPlan> {
        let user_pub = PublicKey::from_secret(SecretKey::from_base(user_secret));
        let auth = AuthCallData::new(market_id, user_secret);

        let params = PlaceBackParamsV1 {
            market_id,
            outcome_index,
            odds,
            stake,
            user_pub,
            signature: Signature::dummy(),
            instance_seed: [0u8; 32],
            nullifier: auth.nullifier.clone(),
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());

        Ok(PlaceBackPlan { call_data, input: auth, zkbin: self.place_back_zkbin.clone(), pk: self.place_back_pk.clone() })
    }

    /// Place a back bet and prove it in one step, framing this call alone.
    pub fn place_back(
        &self,
        market_id: pallas::Base,
        user_secret: pallas::Base,
        outcome_index: u8,
        odds: u32,
        stake: u64,
    ) -> Result<PlaceBackResult> {
        let plan = self.place_back_prepare(market_id, user_secret, outcome_index, odds, stake)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(PlaceBackResult { call_data, proof })
    }

    /// Place a lay bet (function code 0x02) — the call data, and no proof yet.
    pub fn place_lay_prepare(
        &self,
        market_id: pallas::Base,
        user_secret: pallas::Base,
        outcome_index: u8,
        odds: u32,
        stake: u64,
    ) -> Result<PlaceLayPlan> {
        let user_pub = PublicKey::from_secret(SecretKey::from_base(user_secret));
        let auth = AuthCallData::new(market_id, user_secret);

        let params = PlaceLayParamsV1 {
            market_id,
            outcome_index,
            odds,
            stake,
            user_pub,
            signature: Signature::dummy(),
            instance_seed: [0u8; 32],
            nullifier: auth.nullifier.clone(),
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        Ok(PlaceLayPlan { call_data, input: auth, zkbin: self.place_lay_zkbin.clone(), pk: self.place_lay_pk.clone() })
    }

    /// Place a lay bet and prove it in one step, framing this call alone.
    pub fn place_lay(
        &self,
        market_id: pallas::Base,
        user_secret: pallas::Base,
        outcome_index: u8,
        odds: u32,
        stake: u64,
    ) -> Result<PlaceLayResult> {
        let plan = self.place_lay_prepare(market_id, user_secret, outcome_index, odds, stake)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(PlaceLayResult { call_data, proof })
    }

    /// Match orders (function code 0x03) — the call data, and no proof yet.
    pub fn match_orders_prepare(
        &self,
        market_id: pallas::Base,
        matcher_secret: pallas::Base,
        back_order_id: pallas::Base,
        lay_order_id: pallas::Base,
        odds: u32,
    ) -> Result<MatchOrdersPlan> {
        let user_pub = PublicKey::from_secret(SecretKey::from_base(matcher_secret));
        let auth = AuthCallData::new(market_id, matcher_secret);

        let params = MatchOrdersParamsV1 {
            market_id,
            back_order_id,
            lay_order_id,
            odds,
            user_pub,
            signature: Signature::dummy(),
            nullifier: auth.nullifier.clone(),
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(MatchOrdersPlan { call_data, input: auth, zkbin: self.match_orders_zkbin.clone(), pk: self.match_orders_pk.clone() })
    }

    /// Match orders and prove it in one step, framing this call alone.
    pub fn match_orders(
        &self,
        market_id: pallas::Base,
        matcher_secret: pallas::Base,
        back_order_id: pallas::Base,
        lay_order_id: pallas::Base,
        odds: u32,
    ) -> Result<MatchOrdersResult> {
        let plan = self.match_orders_prepare(market_id, matcher_secret, back_order_id, lay_order_id, odds)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(MatchOrdersResult { call_data, proof })
    }

    /// Resolve a market (function code 0x04) — the call data, and no proof yet.
    ///
    /// `oracle_signature` is `Signature::dummy()`: the placeholder `OBL-C211` names, standing for a
    /// verification the contract does not make. It is untouched here — `OBL-C198` is a different
    /// repair, and deleting or verifying this field is that row's decision, not this one's.
    pub fn resolve_market_prepare(
        &self,
        market_id: pallas::Base,
        oracle_secret: pallas::Base,
        winning_outcome: u8,
    ) -> Result<ResolveMarketPlan> {
        let oracle_pub = PublicKey::from_secret(SecretKey::from_base(oracle_secret));
        let auth = AuthCallData::new(market_id, oracle_secret);

        let params = ResolveMarketParamsV1 {
            market_id,
            winning_outcome,
            oracle_pub,
            oracle_signature: Signature::dummy(),
            nullifier: auth.nullifier.clone(),
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        Ok(ResolveMarketPlan { call_data, input: auth, zkbin: self.resolve_market_zkbin.clone(), pk: self.resolve_market_pk.clone() })
    }

    /// Resolve a market and prove it in one step, framing this call alone.
    pub fn resolve_market(
        &self,
        market_id: pallas::Base,
        oracle_secret: pallas::Base,
        winning_outcome: u8,
    ) -> Result<ResolveMarketResult> {
        let plan = self.resolve_market_prepare(market_id, oracle_secret, winning_outcome)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(ResolveMarketResult { call_data, proof })
    }

    /// Settle a market (function code 0x05, non-ZK)
    pub fn settle_market(
        &self,
        market_id: pallas::Base,
        match_ids: Vec<pallas::Base>,
    ) -> Result<SettleMarketResult> {
        let params = SettleMarketParamsV1 { market_id, match_ids };

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        Ok(SettleMarketResult { call_data })
    }

    /// Cancel an order (function code 0x06) — the call data, and no proof yet.
    pub fn cancel_order_prepare(
        &self,
        order_id: pallas::Base,
        user_secret: pallas::Base,
    ) -> Result<CancelOrderPlan> {
        let user_pub = PublicKey::from_secret(SecretKey::from_base(user_secret));
        let auth = AuthCallData::new(order_id, user_secret);

        let params = CancelOrderParamsV1 {
            order_id,
            user_pub,
            signature: Signature::dummy(),
            nullifier: auth.nullifier.clone(),
        };

        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode());

        Ok(CancelOrderPlan { call_data, input: auth, zkbin: self.cancel_order_zkbin.clone(), pk: self.cancel_order_pk.clone() })
    }

    /// Cancel an order and prove it in one step, framing this call alone.
    pub fn cancel_order(
        &self,
        order_id: pallas::Base,
        user_secret: pallas::Base,
    ) -> Result<CancelOrderResult> {
        let plan = self.cancel_order_prepare(order_id, user_secret)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(CancelOrderResult { call_data, proof })
    }

    /// Remove liquidity (function code 0x09) — the call data, and no proof yet.
    pub fn remove_liquidity_prepare(
        &self,
        market_id: pallas::Base,
        lp_share_id: pallas::Base,
        provider_secret: pallas::Base,
    ) -> Result<RemoveLiquidityPlan> {
        let provider_pub = PublicKey::from_secret(SecretKey::from_base(provider_secret));
        let auth = AuthCallData::new(market_id, provider_secret);

        let params = RemoveLiquidityParamsV1 {
            market_id,
            lp_share_id,
            provider: provider_pub,
            signature: Signature::dummy(),
            nullifier: auth.nullifier.clone(),
        };

        let mut call_data = vec![0x09];
        call_data.extend_from_slice(&params.encode());

        Ok(RemoveLiquidityPlan { call_data, input: auth, zkbin: self.remove_liquidity_zkbin.clone(), pk: self.remove_liquidity_pk.clone() })
    }

    /// Remove liquidity and prove it in one step, framing this call alone.
    pub fn remove_liquidity(
        &self,
        market_id: pallas::Base,
        lp_share_id: pallas::Base,
        provider_secret: pallas::Base,
    ) -> Result<RemoveLiquidityResult> {
        let plan = self.remove_liquidity_prepare(market_id, lp_share_id, provider_secret)?;
        let call_data = plan.call_data.clone();
        let commitment = self.own_frame(&call_data);
        let proof = plan.prove(commitment, pallas::Base::zero())?;
        Ok(RemoveLiquidityResult { call_data, proof })
    }
}

impl super::ContractHarness for DarkbetExchangeHarness {
    fn name(&self) -> &str {
        "darkbet_exchange"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateMarketV2",
            "BuyPositionV2",
            "ClaimWinningsV2",
            "AddLiquidityV2",
            "CancelOrderV2",
            "MatchOrdersV2",
            "PlaceBackV2",
            "PlaceLayV2",
            "RemoveLiquidityV2",
            "ResolveMarketV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateMarketV2" => Some(&self.create_market_zkbin),
            "BuyPositionV2" => Some(&self.buy_position_zkbin),
            "ClaimWinningsV2" => Some(&self.claim_winnings_zkbin),
            "AddLiquidityV2" => Some(&self.add_liquidity_zkbin),
            "CancelOrderV2" => Some(&self.cancel_order_zkbin),
            "MatchOrdersV2" => Some(&self.match_orders_zkbin),
            "PlaceBackV2" => Some(&self.place_back_zkbin),
            "PlaceLayV2" => Some(&self.place_lay_zkbin),
            "RemoveLiquidityV2" => Some(&self.remove_liquidity_zkbin),
            "ResolveMarketV2" => Some(&self.resolve_market_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateMarketV2" => Some(&self.create_market_pk),
            "BuyPositionV2" => Some(&self.buy_position_pk),
            "ClaimWinningsV2" => Some(&self.claim_winnings_pk),
            "AddLiquidityV2" => Some(&self.add_liquidity_pk),
            "CancelOrderV2" => Some(&self.cancel_order_pk),
            "MatchOrdersV2" => Some(&self.match_orders_pk),
            "PlaceBackV2" => Some(&self.place_back_pk),
            "PlaceLayV2" => Some(&self.place_lay_pk),
            "RemoveLiquidityV2" => Some(&self.remove_liquidity_pk),
            "ResolveMarketV2" => Some(&self.resolve_market_pk),
            _ => None,
        }
    }
}

// ============================================================================
// Prepared calls (`OBL-C198`)
// ============================================================================
//
// Each `*_prepare` builds the call data and stops; `prove(commitment, nonce)` binds it to a
// transaction commitment the caller supplies, so a parent's proof and its children's bind the same
// value. The one-step fns above frame their own single call through `own_frame`.
//
// `settle_market` has no plan: it is non-ZK and carries no proof.

/// A prepared CreateMarket: the call data exists, the proof does not.
pub struct CreateMarketPlan {
    pub call_data: Vec<u8>,
    input: CreateMarketV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl CreateMarketPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<(dwow_core::zk::Proof, CreateMarketV1PublicInputs)> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        create_market_v1_proof(&self.zkbin, &self.pk, &self.input)
    }
}

/// A prepared BuyPosition. See [`CreateMarketPlan`].
pub struct BuyPositionPlan {
    pub call_data: Vec<u8>,
    input: BuyPositionV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl BuyPositionPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<(dwow_core::zk::Proof, BuyPositionV1PublicInputs)> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        buy_position_v1_proof(&self.zkbin, &self.pk, &self.input)
    }
}

/// A prepared ClaimWinnings. See [`CreateMarketPlan`].
pub struct ClaimWinningsPlan {
    pub call_data: Vec<u8>,
    input: ClaimWinningsV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl ClaimWinningsPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<(dwow_core::zk::Proof, ClaimWinningsV1PublicInputs)> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        claim_winnings_v1_proof(&self.zkbin, &self.pk, &self.input)
    }
}

/// A prepared AddLiquidity. See [`CreateMarketPlan`].
pub struct AddLiquidityPlan {
    pub call_data: Vec<u8>,
    input: AddLiquidityV1CallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl AddLiquidityPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<(dwow_core::zk::Proof, AddLiquidityV1PublicInputs)> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        add_liquidity_v1_proof(&self.zkbin, &self.pk, &self.input)
    }
}

/// A prepared PlaceBack. The auth proof's revealed values are not carried — no caller reads them.
pub struct PlaceBackPlan {
    pub call_data: Vec<u8>,
    input: AuthCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl PlaceBackPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<dwow_core::zk::Proof> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (proof, _pi) = create_auth_proof(&self.zkbin, &self.pk, &self.input)?;
        Ok(proof)
    }
}

/// A prepared PlaceLay. See [`PlaceBackPlan`].
pub struct PlaceLayPlan {
    pub call_data: Vec<u8>,
    input: AuthCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl PlaceLayPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<dwow_core::zk::Proof> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (proof, _pi) = create_auth_proof(&self.zkbin, &self.pk, &self.input)?;
        Ok(proof)
    }
}

/// A prepared MatchOrders. See [`PlaceBackPlan`].
pub struct MatchOrdersPlan {
    pub call_data: Vec<u8>,
    input: AuthCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl MatchOrdersPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<dwow_core::zk::Proof> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (proof, _pi) = create_auth_proof(&self.zkbin, &self.pk, &self.input)?;
        Ok(proof)
    }
}

/// A prepared ResolveMarket. See [`PlaceBackPlan`].
pub struct ResolveMarketPlan {
    pub call_data: Vec<u8>,
    input: AuthCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl ResolveMarketPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<dwow_core::zk::Proof> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (proof, _pi) = create_auth_proof(&self.zkbin, &self.pk, &self.input)?;
        Ok(proof)
    }
}

/// A prepared CancelOrder. See [`PlaceBackPlan`].
pub struct CancelOrderPlan {
    pub call_data: Vec<u8>,
    input: AuthCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl CancelOrderPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<dwow_core::zk::Proof> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (proof, _pi) = create_auth_proof(&self.zkbin, &self.pk, &self.input)?;
        Ok(proof)
    }
}

/// A prepared RemoveLiquidity. See [`PlaceBackPlan`].
pub struct RemoveLiquidityPlan {
    pub call_data: Vec<u8>,
    input: AuthCallData,
    zkbin: ZkBinary,
    pk: ProvingKey,
}

impl RemoveLiquidityPlan {
    pub fn prove(mut self, tx_commitment: pallas::Base, tx_nonce: pallas::Base) -> Result<dwow_core::zk::Proof> {
        self.input.tx_commitment = tx_commitment;
        self.input.tx_nonce = tx_nonce;
        let (proof, _pi) = create_auth_proof(&self.zkbin, &self.pk, &self.input)?;
        Ok(proof)
    }
}

/// Result of create_market
pub struct CreateMarketResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateMarketV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of buy_position
pub struct BuyPositionResult {
    pub call_data: Vec<u8>,
    pub public_inputs: BuyPositionV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of claim_winnings
pub struct ClaimWinningsResult {
    pub call_data: Vec<u8>,
    pub public_inputs: ClaimWinningsV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of add_liquidity
pub struct AddLiquidityResult {
    pub call_data: Vec<u8>,
    pub public_inputs: AddLiquidityV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

pub struct PlaceBackResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct PlaceLayResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct MatchOrdersResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct ResolveMarketResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct CancelOrderResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct RemoveLiquidityResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
pub struct SettleMarketResult { pub call_data: Vec<u8> }