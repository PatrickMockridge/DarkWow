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

//! Block Height Prediction contract integration tests

use darkfi_block_height_prediction_contract::{
    model::{
        derive_market_id, derive_position_id, position_wins, calculate_payout,
        validate_confirmation_depth, validate_tolerance, validate_amount,
        Market, MarketState, Position, PositionType, PositionOutcome,
        CreateMarketParamsV1, CreateMarketUpdateV1, CreatePositionParamsV1,
        CreatePositionUpdateV1, ResolveMarketParamsV1, ResolveMarketUpdateV1,
        ClaimWinningsParamsV1, ClaimWinningsUpdateV1, CancelMarketParamsV1,
        CancelMarketUpdateV1,
    },
    BlockHeightPredictionFunction,
    // Constants
    BLOCK_HEIGHT_PREDICTION_MARKETS_TREE, BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE,
    BLOCK_HEIGHT_PREDICTION_INFO_TREE, BLOCK_HEIGHT_PREDICTION_CLAIMS_TREE,
    DEFAULT_PROTOCOL_FEE, MIN_PROTOCOL_FEE, MAX_PROTOCOL_FEE,
    DEFAULT_RESOLUTION_TIMEOUT, DEFAULT_CONFIRMATION_DEPTH, MAX_CONFIRMATION_DEPTH,
    MAX_TOLERANCE, EXPECTED_BLOCK_TIME,
};

#[test]
fn test_block_height_prediction_function_enum_valid() {
    assert!(BlockHeightPredictionFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(BlockHeightPredictionFunction::try_from(0x01).is_ok()); // CreateMarketV1
    assert!(BlockHeightPredictionFunction::try_from(0x02).is_ok()); // CreatePositionV1
    assert!(BlockHeightPredictionFunction::try_from(0x03).is_ok()); // ResolveMarketV1
    assert!(BlockHeightPredictionFunction::try_from(0x04).is_ok()); // ClaimWinningsV1
    assert!(BlockHeightPredictionFunction::try_from(0x05).is_ok()); // CancelMarketV1
}

#[test]
fn test_block_height_prediction_function_enum_invalid() {
    assert!(BlockHeightPredictionFunction::try_from(0xFF).is_err());
    assert!(BlockHeightPredictionFunction::try_from(0x06).is_err());
    assert!(BlockHeightPredictionFunction::try_from(0x10).is_err());
}

#[test]
fn test_market_state_from_u8() {
    assert_eq!(MarketState::try_from(0), Ok(MarketState::Active));
    assert_eq!(MarketState::try_from(1), Ok(MarketState::Resolved));
    assert_eq!(MarketState::try_from(2), Ok(MarketState::Cancelled));
    assert!(MarketState::try_from(3).is_err());
    assert!(MarketState::try_from(255).is_err());
}

#[test]
fn test_position_type_from_u8() {
    assert_eq!(PositionType::try_from(0), Ok(PositionType::Below));
    assert_eq!(PositionType::try_from(1), Ok(PositionType::Exact));
    assert_eq!(PositionType::try_from(2), Ok(PositionType::Above));
    assert!(PositionType::try_from(3).is_err());
    assert!(PositionType::try_from(255).is_err());
}

#[test]
fn test_derive_market_id() {
    let creator = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let target_time: u64 = 1700000000;
    let token_id = darkfi_sdk::pasta::pallas::Base::ONE;
    let confirmation_depth: u8 = 6;

    let id = derive_market_id(&creator, target_time, token_id, confirmation_depth);

    // Should be deterministic
    let id2 = derive_market_id(&creator, target_time, token_id, confirmation_depth);
    assert_eq!(id, id2);

    // Different input should produce different ID
    let id_different = derive_market_id(&creator, target_time + 1, token_id, confirmation_depth);
    assert_ne!(id, id_different);
}

#[test]
fn test_derive_position_id() {
    let market_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let owner = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let predicted_height: u64 = 100000;
    let position_type = PositionType::Below;
    let amount: u64 = 500;
    let secret_nonce = darkfi_sdk::pasta::pallas::Base::from(42);

    let id = derive_position_id(
        market_id, &owner, predicted_height, position_type, amount, secret_nonce,
    );

    // Should be deterministic
    let id2 = derive_position_id(
        market_id, &owner, predicted_height, position_type, amount, secret_nonce,
    );
    assert_eq!(id, id2);

    // Different input should produce different ID
    let id_different = derive_position_id(
        market_id, &owner, predicted_height + 1, position_type, amount, secret_nonce,
    );
    assert_ne!(id, id_different);
}

#[test]
fn test_validate_confirmation_depth() {
    // Valid depths
    assert!(validate_confirmation_depth(1).is_ok());
    assert!(validate_confirmation_depth(6).is_ok());
    assert!(validate_confirmation_depth(10).is_ok());

    // Invalid depths
    assert!(validate_confirmation_depth(0).is_err());
    assert!(validate_confirmation_depth(11).is_err());
}

#[test]
fn test_validate_tolerance() {
    // Valid tolerances
    assert!(validate_tolerance(0).is_ok());
    assert!(validate_tolerance(50).is_ok());

    // Invalid tolerance
    assert!(validate_tolerance(51).is_err());
}

#[test]
fn test_validate_amount() {
    assert!(validate_amount(1).is_ok());
    assert!(validate_amount(1000).is_ok());
    assert!(validate_amount(0).is_err());
}

#[test]
fn test_position_wins_below() {
    let position = Position {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        predicted_height: 100000,
        tolerance: 5,
        position_type: PositionType::Below,
        amount: 100,
        potential_payout: 100,
        claimed: false,
        created_at: 50000,
    };

    // Resolved below prediction = Won
    assert_eq!(position_wins(&position, 99990), PositionOutcome::Won);

    // Resolved above prediction = Lost
    assert_eq!(position_wins(&position, 100010), PositionOutcome::Lost);

    // Resolved at prediction = Lost (exact is not Below)
    assert_eq!(position_wins(&position, 100000), PositionOutcome::Lost);
}

#[test]
fn test_position_wins_exact() {
    let position = Position {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        predicted_height: 100000,
        tolerance: 5,
        position_type: PositionType::Exact,
        amount: 100,
        potential_payout: 100,
        claimed: false,
        created_at: 50000,
    };

    // Exact prediction = Exact jackpot
    assert_eq!(position_wins(&position, 100000), PositionOutcome::Exact);

    // Within tolerance = Close
    assert_eq!(position_wins(&position, 99998), PositionOutcome::Close);
    assert_eq!(position_wins(&position, 100003), PositionOutcome::Close);

    // Outside tolerance = Lost
    assert_eq!(position_wins(&position, 99990), PositionOutcome::Lost);
    assert_eq!(position_wins(&position, 100010), PositionOutcome::Lost);
}

#[test]
fn test_position_wins_above() {
    let position = Position {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        predicted_height: 100000,
        tolerance: 5,
        position_type: PositionType::Above,
        amount: 100,
        potential_payout: 100,
        claimed: false,
        created_at: 50000,
    };

    // Resolved above prediction = Won
    assert_eq!(position_wins(&position, 100010), PositionOutcome::Won);

    // Resolved below prediction = Lost
    assert_eq!(position_wins(&position, 99990), PositionOutcome::Lost);
}

#[test]
fn test_calculate_payout() {
    let protocol_fee: u32 = 100; // 1%

    // Winning position with 1000 in winning pool and 2000 total pool
    let payout = calculate_payout(100, 1000, 2000, protocol_fee).unwrap();
    // share = 100 * 2000 / 1000 = 200
    // fee_factor = 10000 - 100 = 9900
    // payout = 200 * 9900 / 10000 = 198
    assert_eq!(payout, 198);

    // Zero winning pool = no payout
    let payout = calculate_payout(100, 0, 2000, protocol_fee).unwrap();
    assert_eq!(payout, 0);
}

#[test]
fn test_market_encoding() {
    let market = Market {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        creator: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        target_time: 1700000000,
        base_block_height: 50000,
        created_at: 49000,
        total_pool: 10000,
        below_pool: 4000,
        above_pool: 4000,
        exact_pool: 2000,
        state: MarketState::Active,
        resolved_height: None,
        resolution_block: 0,
        confirmation_depth: 6,
        protocol_fee: 100,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        position_count: 10,
    };

    let encoded = market.encode().unwrap();
    let decoded = Market::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, market.id);
    assert_eq!(decoded.target_time, market.target_time);
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
        predicted_height: 100000,
        tolerance: 5,
        position_type: PositionType::Below,
        amount: 500,
        potential_payout: 495,
        claimed: false,
        created_at: 50000,
    };

    let encoded = position.encode().unwrap();
    let decoded = Position::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, position.id);
    assert_eq!(decoded.predicted_height, position.predicted_height);
    assert_eq!(decoded.position_type, position.position_type);
    assert_eq!(decoded.amount, position.amount);
}

#[test]
fn test_create_market_params_encoding() {
    let params = CreateMarketParamsV1 {
        creator: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        target_time: 1700000000,
        initial_prediction: 100000,
        confirmation_depth: 6,
        protocol_fee: 100,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
    };

    let encoded = params.encode().unwrap();
    let decoded = CreateMarketParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.target_time, params.target_time);
    assert_eq!(decoded.initial_prediction, params.initial_prediction);
    assert_eq!(decoded.confirmation_depth, params.confirmation_depth);
}

#[test]
fn test_create_market_update_encoding() {
    let update = CreateMarketUpdateV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        creator: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        target_time: 1700000000,
        base_block_height: 50000,
        confirmation_depth: 6,
        protocol_fee: 100,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        created_at: 49000,
    };

    let encoded = update.encode().unwrap();
    let decoded = CreateMarketUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.market_id, update.market_id);
    assert_eq!(decoded.target_time, update.target_time);
}

#[test]
fn test_create_position_params_encoding() {
    let params = CreatePositionParamsV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        predicted_height: 100000,
        tolerance: 5,
        position_type: 0,
        amount: 500,
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        signature: darkfi_sdk::pasta::pallas::Base::ZERO,
    };

    let encoded = params.encode().unwrap();
    let decoded = CreatePositionParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.predicted_height, params.predicted_height);
    assert_eq!(decoded.tolerance, params.tolerance);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_create_position_update_encoding() {
    let update = CreatePositionUpdateV1 {
        position_id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(2),
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        predicted_height: 100000,
        tolerance: 5,
        position_type: PositionType::Below,
        amount: 500,
        created_at: 50000,
    };

    let encoded = update.encode().unwrap();
    let decoded = CreatePositionUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.position_id, update.position_id);
    assert_eq!(decoded.predicted_height, update.predicted_height);
    assert_eq!(decoded.position_type, update.position_type);
}

#[test]
fn test_resolve_market_params_encoding() {
    let params = ResolveMarketParamsV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        observed_height: 100005,
        proof: vec![1, 2, 3],
    };

    let encoded = params.encode().unwrap();
    let decoded = ResolveMarketParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.market_id, params.market_id);
    assert_eq!(decoded.observed_height, params.observed_height);
}

#[test]
fn test_resolve_market_update_encoding() {
    let update = ResolveMarketUpdateV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        resolved_height: 100005,
        resolution_block: 51000,
        state: MarketState::Resolved,
    };

    let encoded = update.encode().unwrap();
    let decoded = ResolveMarketUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.resolved_height, update.resolved_height);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_claim_winnings_params_encoding() {
    let params = ClaimWinningsParamsV1 {
        position_id: darkfi_sdk::pasta::pallas::Base::from(1),
        market_id: darkfi_sdk::pasta::pallas::Base::from(2),
        owner: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        proof: vec![1, 2, 3],
    };

    let encoded = params.encode().unwrap();
    let decoded = ClaimWinningsParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.position_id, params.position_id);
    assert_eq!(decoded.market_id, params.market_id);
}

#[test]
fn test_claim_winnings_update_encoding() {
    let update = ClaimWinningsUpdateV1 {
        position_id: darkfi_sdk::pasta::pallas::Base::from(1),
        payout: 495,
        claimed: true,
    };

    let encoded = update.encode().unwrap();
    let decoded = ClaimWinningsUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.payout, update.payout);
    assert_eq!(decoded.claimed, update.claimed);
}

#[test]
fn test_cancel_market_params_encoding() {
    let params = CancelMarketParamsV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        canceller: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
    };

    let encoded = params.encode().unwrap();
    let decoded = CancelMarketParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.market_id, params.market_id);
}

#[test]
fn test_cancel_market_update_encoding() {
    let update = CancelMarketUpdateV1 {
        market_id: darkfi_sdk::pasta::pallas::Base::from(1),
        state: MarketState::Cancelled,
        refund_amounts: vec![
            (darkfi_sdk::pasta::pallas::Base::from(1), 500),
            (darkfi_sdk::pasta::pallas::Base::from(2), 300),
        ],
    };

    let encoded = update.encode().unwrap();
    let decoded = CancelMarketUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.market_id, update.market_id);
    assert_eq!(decoded.state, update.state);
    assert_eq!(decoded.refund_amounts.len(), 2);
}

#[test]
fn test_constants() {
    assert_eq!(BLOCK_HEIGHT_PREDICTION_MARKETS_TREE, "markets");
    assert_eq!(BLOCK_HEIGHT_PREDICTION_POSITIONS_TREE, "positions");
    assert_eq!(BLOCK_HEIGHT_PREDICTION_INFO_TREE, "info");
    assert_eq!(BLOCK_HEIGHT_PREDICTION_CLAIMS_TREE, "claims");

    assert_eq!(DEFAULT_PROTOCOL_FEE, 100);
    assert_eq!(MIN_PROTOCOL_FEE, 10);
    assert_eq!(MAX_PROTOCOL_FEE, 1000);
    assert_eq!(DEFAULT_RESOLUTION_TIMEOUT, 15);
    assert_eq!(DEFAULT_CONFIRMATION_DEPTH, 6);
    assert_eq!(MAX_CONFIRMATION_DEPTH, 10);
    assert_eq!(MAX_TOLERANCE, 50);
    assert_eq!(EXPECTED_BLOCK_TIME, 120);
}