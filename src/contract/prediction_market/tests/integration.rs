/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Prediction Market contract integration tests

use darkfi_prediction_market_contract::{
    model::{
        calculate_liquidity_payout, calculate_lp_shares, calculate_payout, calculate_position_price,
        derive_lp_share_id, derive_market_id, derive_position_id, validate_amount,
        validate_num_outcomes, validate_protocol_fee, AddLiquidityParamsV1, AddLiquidityUpdateV1,
        CancelMarketParamsV1, CancelMarketUpdateV1, ClaimWinningsParamsV1, ClaimWinningsUpdateV1,
        CreateMarketParamsV1, CreateMarketUpdateV1, CreatePositionParamsV1, CreatePositionUpdateV1,
        LpShare, Market, MarketState, Position, RemoveLiquidityParamsV1, RemoveLiquidityUpdateV1,
        ResolveMarketParamsV1, ResolveMarketUpdateV1, WithdrawFeesParamsV1, WithdrawFeesUpdateV1,
    },
    PredictionMarketFunction,
    // Constants
    PREDICTION_CONTRACT_MARKETS_TREE, PREDICTION_CONTRACT_POSITIONS_TREE,
    PREDICTION_CONTRACT_LIQUIDITY_TREE, PREDICTION_CONTRACT_INFO_TREE,
    PREDICTION_CONTRACT_RESOLUTIONS_TREE, PREDICTION_CONTRACT_PENDING_TREE,
    PREDICTION_CONTRACT_CLAIMS_TREE,
    DEFAULT_PROTOCOL_FEE, MIN_PROTOCOL_FEE, MAX_PROTOCOL_FEE,
    DEFAULT_LP_FEE, DEFAULT_RESOLUTION_TIMEOUT, MAX_OUTCOMES,
};

#[test]
fn test_prediction_market_function_enum_valid() {
    assert!(PredictionMarketFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(PredictionMarketFunction::try_from(0x01).is_ok()); // CreateMarketV1
    assert!(PredictionMarketFunction::try_from(0x02).is_ok()); // CreatePositionV1
    assert!(PredictionMarketFunction::try_from(0x03).is_ok()); // AddLiquidityV1
    assert!(PredictionMarketFunction::try_from(0x04).is_ok()); // RemoveLiquidityV1
    assert!(PredictionMarketFunction::try_from(0x05).is_ok()); // ResolveMarketV1
    assert!(PredictionMarketFunction::try_from(0x06).is_ok()); // CancelMarketV1
    assert!(PredictionMarketFunction::try_from(0x07).is_ok()); // ClaimWinningsV1
    assert!(PredictionMarketFunction::try_from(0x08).is_ok()); // WithdrawFeesV1
}

#[test]
fn test_prediction_market_function_enum_invalid() {
    assert!(PredictionMarketFunction::try_from(0xFF).is_err());
    assert!(PredictionMarketFunction::try_from(0x09).is_err());
    assert!(PredictionMarketFunction::try_from(0x10).is_err());
}

#[test]
fn test_market_state_from_u8() {
    assert_eq!(MarketState::try_from(0), Ok(MarketState::Active));
    assert_eq!(MarketState::try_from(1), Ok(MarketState::Frozen));
    assert_eq!(MarketState::try_from(2), Ok(MarketState::Resolved));
    assert_eq!(MarketState::try_from(3), Ok(MarketState::Cancelled));
    assert_eq!(MarketState::try_from(4), Ok(MarketState::Disputed));
    assert!(MarketState::try_from(5).is_err());
    assert!(MarketState::try_from(255).is_err());
}

#[test]
fn test_validate_num_outcomes() {
    assert!(validate_num_outcomes(1).is_ok()); // Binary (YES/NO)
    assert!(validate_num_outcomes(2).is_ok());
    assert!(validate_num_outcomes(20).is_ok()); // Max
    assert!(validate_num_outcomes(0).is_err()); // Invalid
    assert!(validate_num_outcomes(21).is_err()); // Too many
}

#[test]
fn test_validate_protocol_fee() {
    assert!(validate_protocol_fee(0).is_ok()); // Use default
    assert!(validate_protocol_fee(10).is_ok()); // Min
    assert!(validate_protocol_fee(1000).is_ok()); // Max
    assert!(validate_protocol_fee(5).is_err()); // Below min
    assert!(validate_protocol_fee(1001).is_err()); // Above max
}

#[test]
fn test_validate_amount() {
    assert!(validate_amount(1).is_ok());
    assert!(validate_amount(1000).is_ok());
    assert!(validate_amount(0).is_err());
}

#[test]
fn test_derive_market_id() {
    let creator = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let oracle_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let question = b"Will BTC reach $100k?";
    let resolve_time: u64 = 1700000000;
    let token_id = darkfi_sdk::pasta::pallas::Base::ONE;

    let id = derive_market_id(&creator, question, resolve_time, token_id, &oracle_pubkey);

    // Should be deterministic
    let id2 = derive_market_id(&creator, question, resolve_time, token_id, &oracle_pubkey);
    assert_eq!(id, id2);
}

#[test]
fn test_derive_position_id() {
    let market_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let owner = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let outcome: u8 = 1;
    let amount: u64 = 1000;
    let secret_nonce = darkfi_sdk::pasta::pallas::Base::from(42);

    let id = derive_position_id(market_id, &owner, outcome, amount, secret_nonce);

    // Should be deterministic
    let id2 = derive_position_id(market_id, &owner, outcome, amount, secret_nonce);
    assert_eq!(id, id2);
}

#[test]
fn test_derive_lp_share_id() {
    let market_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let provider = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let shares: u64 = 100;
    let secret_nonce = darkfi_sdk::pasta::pallas::Base::from(42);

    let id = derive_lp_share_id(market_id, &provider, shares, secret_nonce);

    // Should be deterministic
    let id2 = derive_lp_share_id(market_id, &provider, shares, secret_nonce);
    assert_eq!(id, id2);
}

#[test]
fn test_calculate_position_price() {
    // First bet: even odds
    let pools = vec![0, 0];
    let price = calculate_position_price(&pools, 0, 1000).unwrap();
    assert_eq!(price, 1000); // No pool, returns amount

    // After some bets
    let pools = vec![5000, 5000];
    let price = calculate_position_price(&pools, 0, 1000).unwrap();
    // price = (5000 * 1000) / (5000 + 1000) = 5000000 / 6000 = 833
    assert_eq!(price, 833);
}

#[test]
fn test_calculate_payout() {
    // Winner takes all from winning pool
    let payout = calculate_payout(1000, 5000, 10000, 100, 200).unwrap();
    // total_fees = 100 + 200 = 300
    // fee_factor = 10000 - 300 = 9700
    // share = 1000 * 10000 / 5000 = 2000
    // payout = 2000 * 9700 / 10000 = 1940
    assert_eq!(payout, 1940);
}

#[test]
fn test_calculate_lp_shares() {
    // First LP
    let shares = calculate_lp_shares(1000, 0, 0).unwrap();
    assert_eq!(shares, 1000);

    // Subsequent LPs
    let shares = calculate_lp_shares(1000, 1000, 1000).unwrap();
    assert_eq!(shares, 1000); // 1000 * 1000 / 1000 = 1000
}

#[test]
fn test_calculate_liquidity_payout() {
    let payout = calculate_liquidity_payout(100, 10000, 1000).unwrap();
    // 100 * 10000 / 1000 = 1000
    assert_eq!(payout, 1000);

    // Zero shares = zero payout
    let payout = calculate_liquidity_payout(0, 10000, 1000).unwrap();
    assert_eq!(payout, 0);
}

#[test]
fn test_market_encoding() {
    let market = Market {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        creator: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        question: b"Will BTC reach $100k?".to_vec(),
        resolve_time: 1700000000,
        betting_closes: 1699999900,
        num_outcomes: 2,
        total_pool: 10000,
        total_lp_shares: 1000,
        outcome_pools: vec![5000, 5000],
        state: MarketState::Active,
        resolved_outcome: None,
        protocol_fee: 100,
        lp_fee: 200,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        oracle_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        created_at: 50000,
        resolved_at: 0,
    };

    let encoded = market.encode().unwrap();
    let decoded = Market::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, market.id);
    assert_eq!(decoded.total_pool, market.total_pool);
    assert_eq!(decoded.state, market.state);
}

#[test]
fn test_position_encoding() {
    let position = Position {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(2),
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        outcome: 1,
        amount: 1000,
        potential_payout: 1940,
        claimed: false,
        created_at: 50000,
    };

    let encoded = position.encode().unwrap();
    let decoded = Position::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, position.id);
    assert_eq!(decoded.outcome, position.outcome);
    assert_eq!(decoded.amount, position.amount);
}

#[test]
fn test_lp_share_encoding() {
    let lp_share = LpShare {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(2),
        provider: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        shares: 1000,
        earned_fees: 50,
        created_at: 50000,
    };

    let encoded = lp_share.encode().unwrap();
    let decoded = LpShare::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, lp_share.id);
    assert_eq!(decoded.shares, lp_share.shares);
    assert_eq!(decoded.earned_fees, lp_share.earned_fees);
}

#[test]
fn test_create_market_params_encoding() {
    let params = CreateMarketParamsV1 {
        question: b"Will BTC reach $100k?".to_vec(),
        resolve_time: 1700000000,
        betting_closes: 1699999900,
        num_outcomes: 2,
        protocol_fee: 100,
        lp_fee: 200,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        oracle_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        oracle_signature: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = CreateMarketParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.question, params.question);
    assert_eq!(decoded.num_outcomes, params.num_outcomes);
}

#[test]
fn test_create_position_params_encoding() {
    let params = CreatePositionParamsV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        outcome: 1,
        amount: 1000,
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        signature: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = CreatePositionParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.outcome, params.outcome);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_add_liquidity_params_encoding() {
    let params = AddLiquidityParamsV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        amount: 1000,
        provider: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        signature: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = AddLiquidityParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_resolve_market_params_encoding() {
    let params = ResolveMarketParamsV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        outcome: 1,
        attestation: vec![1, 2, 3],
        oracle_signature: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = ResolveMarketParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.outcome, params.outcome);
}

#[test]
fn test_constants() {
    assert_eq!(PREDICTION_CONTRACT_MARKETS_TREE, "markets");
    assert_eq!(PREDICTION_CONTRACT_POSITIONS_TREE, "positions");
    assert_eq!(PREDICTION_CONTRACT_LIQUIDITY_TREE, "liquidity");
    assert_eq!(PREDICTION_CONTRACT_INFO_TREE, "info");
    assert_eq!(PREDICTION_CONTRACT_RESOLUTIONS_TREE, "resolutions");
    assert_eq!(PREDICTION_CONTRACT_PENDING_TREE, "pending");
    assert_eq!(PREDICTION_CONTRACT_CLAIMS_TREE, "claims");

    assert_eq!(DEFAULT_PROTOCOL_FEE, 100);
    assert_eq!(MIN_PROTOCOL_FEE, 10);
    assert_eq!(MAX_PROTOCOL_FEE, 1000);
    assert_eq!(DEFAULT_LP_FEE, 200);
    assert_eq!(DEFAULT_RESOLUTION_TIMEOUT, 1000);
    assert_eq!(MAX_OUTCOMES, 20);
}