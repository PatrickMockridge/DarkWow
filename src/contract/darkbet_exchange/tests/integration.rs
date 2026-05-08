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

//! Darkbet Exchange contract integration tests
//!
//! These tests verify the darkbet exchange contract's:
//! - Function enum parsing
//! - Data structure encoding/decoding
//! - Model type invariants
//! - Order matching logic

use dwow_darkbet_exchange_contract::{
    model::{
        CancelOrderParamsV1, CreateMarketParamsV1,
        LpShare, LpShareState, Market, MarketState, MarketType, Match, MatchOrdersParamsV1,
        MatchState, Order, OrderState, OrderType, Outcome, PlaceBackParamsV1,
        PlaceLayParamsV1, Position, PositionState,
        ResolveMarketParamsV1,
    },
    DarkbetFunction, DARKBET_EXCHANGE_COMMISSION_BP, DARKBET_EXCHANGE_MAX_MARKET_LIFETIME,
    DARKBET_EXCHANGE_MIN_ORDER_SIZE,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{crypto::pasta_prelude::PrimeField, crypto::PublicKey, pasta::pallas};

/// Helper to create a pallas::Base from bytes
fn make_base(bytes: [u8; 32]) -> pallas::Base {
    pallas::Base::from_repr(bytes).unwrap()
}

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    use dwow_sdk::crypto::SecretKey;
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_darkbet_function_enum_valid() {
    // Test that all function IDs are valid
    assert!(DarkbetFunction::try_from(0x00).is_ok()); // CreateMarketV1
    assert!(DarkbetFunction::try_from(0x01).is_ok()); // PlaceBackV1
    assert!(DarkbetFunction::try_from(0x02).is_ok()); // PlaceLayV1
    assert!(DarkbetFunction::try_from(0x03).is_ok()); // MatchOrdersV1
    assert!(DarkbetFunction::try_from(0x04).is_ok()); // ResolveMarketV1
    assert!(DarkbetFunction::try_from(0x05).is_ok()); // SettleMarketV1
    assert!(DarkbetFunction::try_from(0x06).is_ok()); // CancelOrderV1
    assert!(DarkbetFunction::try_from(0x07).is_ok()); // BuyPositionV1
    assert!(DarkbetFunction::try_from(0x08).is_ok()); // AddLiquidityV1
    assert!(DarkbetFunction::try_from(0x09).is_ok()); // RemoveLiquidityV1
    assert!(DarkbetFunction::try_from(0x0A).is_ok()); // ClaimWinningsV1
}

#[test]
fn test_darkbet_function_enum_invalid() {
    // Test that invalid function IDs return errors
    assert!(DarkbetFunction::try_from(0xFF).is_err());
    assert!(DarkbetFunction::try_from(0x10).is_err());
    assert!(DarkbetFunction::try_from(0x1B).is_err());
}

#[test]
fn test_market_state_enum() {
    assert_eq!(MarketState::Open as u8, 0);
    assert_eq!(MarketState::Closed as u8, 1);
    assert_eq!(MarketState::Resolved as u8, 2);
    assert_eq!(MarketState::Settled as u8, 3);
    assert_eq!(MarketState::Cancelled as u8, 4);
}

#[test]
fn test_market_type_enum() {
    assert_eq!(MarketType::OrderBook as u8, 0);
    assert_eq!(MarketType::AmmPool as u8, 1);
}

#[test]
fn test_order_type_enum() {
    assert_eq!(OrderType::Back as u8, 0);
    assert_eq!(OrderType::Lay as u8, 1);
}

#[test]
fn test_outcome_enum() {
    assert_eq!(Outcome::No as u8, 0);
    assert_eq!(Outcome::Yes as u8, 1);
    assert_eq!(Outcome::Void as u8, 2);
}

#[test]
fn test_order_state_enum() {
    assert_eq!(OrderState::Open as u8, 0);
    assert_eq!(OrderState::Matched as u8, 1);
    assert_eq!(OrderState::Cancelled as u8, 2);
    assert_eq!(OrderState::Expired as u8, 3);
}

#[test]
fn test_match_state_enum() {
    assert_eq!(MatchState::Pending as u8, 0);
    assert_eq!(MatchState::Settled as u8, 1);
    assert_eq!(MatchState::Cancelled as u8, 2);
}

#[test]
fn test_position_state_enum() {
    assert_eq!(PositionState::Active as u8, 0);
    assert_eq!(PositionState::Claimed as u8, 1);
    assert_eq!(PositionState::Refunded as u8, 2);
}

#[test]
fn test_lp_share_state_enum() {
    assert_eq!(LpShareState::Active as u8, 0);
    assert_eq!(LpShareState::Removed as u8, 1);
}

#[test]
fn test_market_order_book_creation() {
    let creator = make_pubkey(1);
    let oracle_id = make_base([2u8; 32]);
    let close_block = 1000u64;
    let current_block = 100u64;

    let market = Market::new_order_book(
        creator,
        "Team A vs Team B".to_string(),
        vec!["Team_A_Wins".to_string(), "Team_B_Wins".to_string(), "Draw".to_string()],
        oracle_id,
        DARKBET_EXCHANGE_COMMISSION_BP,
        close_block,
        current_block,
    );

    assert_eq!(market.market_type, MarketType::OrderBook);
    assert_eq!(market.state, MarketState::Open);
    assert_eq!(market.back_volume, 0);
    assert_eq!(market.lay_volume, 0);
    assert_eq!(market.matched_volume, 0);
    assert!(market.can_accept_order(current_block).is_ok());
}

#[test]
fn test_market_amm_pool_creation() {
    let creator = make_pubkey(1);
    let oracle_id = make_base([2u8; 32]);
    let close_block = 1000u64;
    let current_block = 100u64;

    let market = Market::new_amm_pool(
        creator,
        "Team A vs Team B".to_string(),
        vec!["Team_A_Wins".to_string(), "Team_B_Wins".to_string()],
        oracle_id,
        100,  // protocol_fee
        200,  // lp_fee
        close_block,
        current_block,
    );

    assert_eq!(market.market_type, MarketType::AmmPool);
    assert_eq!(market.state, MarketState::Open);
    assert_eq!(market.outcome_pools.len(), 2);
    assert_eq!(market.outcome_pools, vec![0, 0]);
    assert!(market.can_accept_order(current_block).is_ok());
}

#[test]
fn test_market_cannot_accept_order_when_closed() {
    let creator = make_pubkey(1);
    let oracle_id = make_base([2u8; 32]);

    let market = Market::new_order_book(
        creator,
        "Team A vs Team B".to_string(),
        vec!["Team_A_Wins".to_string()],
        oracle_id,
        DARKBET_EXCHANGE_COMMISSION_BP,
        1000,  // close_block
        100,   // current_block
    );

    // Market can accept orders before close_block
    assert!(market.can_accept_order(500).is_ok());
    // Market cannot accept orders at or after close_block
    assert!(market.can_accept_order(1000).is_err());
    assert!(market.can_accept_order(1500).is_err());
}

#[test]
fn test_market_calculate_commission() {
    let creator = make_pubkey(1);
    let market = Market::new_order_book(
        creator,
        "Test".to_string(),
        vec!["Yes".to_string()],
        make_base([0u8; 32]),
        200,  // 2% commission
        1000,
        100,
    );

    // 10000 bp = 1.0, so 200 bp = 2%
    // Commission on 1000 = 1000 * 200 / 10000 = 20
    assert_eq!(market.calculate_commission(1000), 20);
    assert_eq!(market.calculate_commission(10000), 200);
}

#[test]
fn test_market_calculate_position_price() {
    let creator = make_pubkey(1);
    let mut market = Market::new_amm_pool(
        creator,
        "Test".to_string(),
        vec!["Yes".to_string(), "No".to_string()],
        make_base([0u8; 32]),
        100,
        200,
        1000,
        100,
    );

    // Set up some pool liquidity
    market.outcome_pools = vec![10000, 10000];
    market.total_pool = 20000;

    // With pre-seeded pools, price uses constant-product formula
    // price = (other_pools * amount) / (pool_for_outcome + amount)
    // price = (10000 * 1000) / (10000 + 1000) = 909
    // Note: calculate_position_price is read-only, doesn't update pool
    let price1 = market.calculate_position_price(0, 1000).unwrap();
    assert_eq!(price1, 909);

    // Same price since pool state wasn't modified
    let price2 = market.calculate_position_price(0, 1000).unwrap();
    assert_eq!(price2, 909);
}

#[test]
fn test_order_back_creation() {
    let user = make_pubkey(1);
    let market_id = make_base([2u8; 32]);
    let current_block = 100u64;

    let order = Order::new_back(
        market_id,
        0,      // outcome_index
        25000,  // odds = 2.5
        1000,   // stake
        user,
        current_block,
    );

    assert_eq!(order.order_type, OrderType::Back);
    assert_eq!(order.stake, 1000);
    assert_eq!(order.odds, 25000);
    assert_eq!(order.liability, 0); // Back has no liability
    assert_eq!(order.state, OrderState::Open);
}

#[test]
fn test_order_lay_creation() {
    let user = make_pubkey(1);
    let market_id = make_base([2u8; 32]);
    let current_block = 100u64;

    let order = Order::new_lay(
        market_id,
        0,      // outcome_index
        20000,  // odds = 2.0
        1000,   // stake
        user,
        current_block,
    );

    assert_eq!(order.order_type, OrderType::Lay);
    assert_eq!(order.stake, 1000);
    assert_eq!(order.odds, 20000);
    // Liability = stake * (odds - 10000) / 10000 = 1000 * (20000 - 10000) / 10000 = 1000
    assert_eq!(order.liability, 1000);
    assert_eq!(order.state, OrderState::Open);
}

#[test]
fn test_order_back_payout() {
    let user = make_pubkey(1);
    let order = Order::new_back(
        make_base([1u8; 32]),
        0,
        25000,  // 2.5:1 odds
        1000,
        user,
        100,
    );

    // Payout = stake * odds / 10000 = 1000 * 25000 / 10000 = 2500
    assert_eq!(order.back_payout(), 2500);
}

#[test]
fn test_order_matching_compatibility() {
    let user1 = make_pubkey(1);
    let user2 = make_pubkey(2);
    let market_id = make_base([5u8; 32]);

    let back_order = Order::new_back(market_id, 0, 20000, 1000, user1, 100);
    let lay_order = Order::new_lay(market_id, 0, 20000, 1000, user2, 100);

    // Same odds should match
    assert!(back_order.matches(&lay_order));

    // Different outcomes should not match
    let lay_order_wrong_outcome = Order::new_lay(market_id, 1, 20000, 1000, user2, 100);
    assert!(!back_order.matches(&lay_order_wrong_outcome));

    // Same order type should not match
    let back_order2 = Order::new_back(market_id, 0, 20000, 1000, user2, 100);
    assert!(!back_order.matches(&back_order2));
}

#[test]
fn test_match_creation() {
    let user1 = make_pubkey(1);
    let user2 = make_pubkey(2);
    let market_id = make_base([5u8; 32]);

    let back_order = Order::new_back(market_id, 0, 20000, 1000, user1, 100);
    let lay_order = Order::new_lay(market_id, 0, 20000, 1000, user2, 100);

    let commission = 20u64;
    let matched_at = 150u64;

    let match_obj = Match::new(
        make_base([6u8; 32]), // match_id
        market_id,
        0,
        20000,
        &back_order,
        &lay_order,
        commission,
        matched_at,
    );

    assert_eq!(match_obj.back_stake, 1000);
    assert_eq!(match_obj.lay_liability, 1000);
    assert_eq!(match_obj.commission, 20);
    assert_eq!(match_obj.state, MatchState::Pending);
}

#[test]
fn test_match_back_winnings() {
    let user1 = make_pubkey(1);
    let user2 = make_pubkey(2);

    let back_order = Order::new_back(make_base([5u8; 32]), 0, 20000, 1000, user1, 100);
    let lay_order = Order::new_lay(make_base([5u8; 32]), 0, 20000, 1000, user2, 100);

    let match_obj = Match::new(
        make_base([7u8; 32]), // match_id
        make_base([5u8; 32]),
        0,
        20000,
        &back_order,
        &lay_order,
        0,  // no commission for winnings calc test
        150,
    );

    // Back winnings = back_stake * odds / 10000 = 1000 * 20000 / 10000 = 2000
    assert_eq!(match_obj.back_winnings(), 2000);
}

#[test]
fn test_position_creation() {
    let owner = make_pubkey(1);
    let market_id = make_base([2u8; 32]);
    let current_block = 100u64;

    let position = Position::new(
        market_id,
        owner,
        0,      // outcome
        1000,   // amount
        2500,   // potential_payout
        current_block,
    );

    assert_eq!(position.market_id, market_id);
    assert_eq!(position.owner, owner);
    assert_eq!(position.outcome, 0);
    assert_eq!(position.amount, 1000);
    assert_eq!(position.potential_payout, 2500);
    assert_eq!(position.state, PositionState::Active);
}

#[test]
fn test_lp_share_creation() {
    let provider = make_pubkey(1);
    let market_id = make_base([2u8; 32]);
    let current_block = 100u64;

    let lp_share = LpShare::new(
        market_id,
        provider,
        1000,  // shares
        current_block,
    );

    assert_eq!(lp_share.market_id, market_id);
    assert_eq!(lp_share.provider, provider);
    assert_eq!(lp_share.shares, 1000);
    assert_eq!(lp_share.earned_fees, 0);
    assert_eq!(lp_share.state, LpShareState::Active);
}

#[test]
fn test_create_market_params_encoding() {
    use dwow_sdk::crypto::schnorr::Signature;

    let params = CreateMarketParamsV1 {
        description: "Team A vs Team B".to_string(),
        outcomes: vec!["Team_A_Wins".to_string(), "Team_B_Wins".to_string()],
        oracle_id: make_base([1u8; 32]),
        commission_bp: 200,
        market_type: 0,
        protocol_fee: 100,
        lp_fee: 200,
        duration_blocks: 1000,
        creator_pub: make_pubkey(2),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: CreateMarketParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.description, params.description);
    assert_eq!(decoded.outcomes.len(), 2);
    assert_eq!(decoded.commission_bp, 200);
    assert_eq!(decoded.market_type, 0);
}

#[test]
fn test_place_back_params_encoding() {
    use dwow_sdk::crypto::schnorr::Signature;

    let params = PlaceBackParamsV1 {
        market_id: make_base([1u8; 32]),
        outcome_index: 0,
        odds: 25000,
        stake: 1000,
        user_pub: make_pubkey(2),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: PlaceBackParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.market_id, params.market_id);
    assert_eq!(decoded.odds, 25000);
    assert_eq!(decoded.stake, 1000);
}

#[test]
fn test_place_lay_params_encoding() {
    use dwow_sdk::crypto::schnorr::Signature;

    let params = PlaceLayParamsV1 {
        market_id: make_base([1u8; 32]),
        outcome_index: 0,
        odds: 20000,
        stake: 1000,
        user_pub: make_pubkey(2),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: PlaceLayParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.market_id, params.market_id);
    assert_eq!(decoded.odds, 20000);
    assert_eq!(decoded.stake, 1000);
}

#[test]
fn test_match_orders_params_encoding() {
    use dwow_sdk::crypto::schnorr::Signature;

    let params = MatchOrdersParamsV1 {
        market_id: make_base([1u8; 32]),
        back_order_id: make_base([2u8; 32]),
        lay_order_id: make_base([3u8; 32]),
        odds: 20000,
        user_pub: make_pubkey(4),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: MatchOrdersParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.market_id, params.market_id);
    assert_eq!(decoded.odds, 20000);
}

#[test]
fn test_cancel_order_params_encoding() {
    use dwow_sdk::crypto::schnorr::Signature;

    let params = CancelOrderParamsV1 {
        order_id: make_base([1u8; 32]),
        user_pub: make_pubkey(2),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: CancelOrderParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.order_id, params.order_id);
}

#[test]
fn test_resolve_market_params_encoding() {
    use dwow_sdk::crypto::schnorr::Signature;

    let params = ResolveMarketParamsV1 {
        market_id: make_base([1u8; 32]),
        winning_outcome: 0,
        oracle_pub: make_pubkey(2),
        oracle_signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: ResolveMarketParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.market_id, params.market_id);
    assert_eq!(decoded.winning_outcome, 0);
}

#[test]
fn test_constants() {
    // Verify commission rate is 2%
    assert_eq!(DARKBET_EXCHANGE_COMMISSION_BP, 200);

    // Verify minimum order size
    assert_eq!(DARKBET_EXCHANGE_MIN_ORDER_SIZE, 10);

    // Verify max market lifetime is ~1 week (5min blocks)
    assert_eq!(DARKBET_EXCHANGE_MAX_MARKET_LIFETIME, 2016);
}