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

//! DarkBet Exchange Contract Entrypoint

use dwow_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField, schnorr::SchnorrPublic, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta::pallas, wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};
use pasta_curves::group::Curve;
use pasta_curves::arithmetic::CurveAffine;

use crate::error::DarkbetError;
use crate::model::{
    CancelOrderParamsV1, CancelOrderUpdateV1, CreateMarketParamsV1, CreateMarketUpdateV1,
    LpShare, LpShareState, Market, MarketState, MarketType, Match, MatchOrdersParamsV1,
    MatchOrdersUpdateV1, MatchState, Order, OrderState, OrderType, PlaceBackParamsV1,
    PlaceBackUpdateV1, PlaceLayParamsV1, PlaceLayUpdateV1, Position, PositionState,
    BuyPositionParamsV1, BuyPositionUpdateV1, AddLiquidityParamsV1, AddLiquidityUpdateV1,
    RemoveLiquidityParamsV1, RemoveLiquidityUpdateV1, ResolveMarketParamsV1,
    ResolveMarketUpdateV1, SettleMarketParamsV1, SettleMarketUpdateV1,
    ClaimWinningsParamsV1, ClaimWinningsUpdateV1,
};
use crate::{
    DarkbetFunction, DARKBET_EXCHANGE_COMMISSION_BP,
    DARKBET_EXCHANGE_MARKETS_TREE, DARKBET_EXCHANGE_BACK_ORDERS_TREE,
    DARKBET_EXCHANGE_LAY_ORDERS_TREE, DARKBET_EXCHANGE_MATCHES_TREE,
    DARKBET_EXCHANGE_NULLIFIERS_TREE, DARKBET_EXCHANGE_POSITIONS_TREE,
    DARKBET_EXCHANGE_LP_SHARES_TREE, DARKBET_EXCHANGE_MAX_MARKET_LIFETIME,
    DARKBET_EXCHANGE_MIN_ORDER_SIZE, DARKBET_EXCHANGE_INFO_TREE,
    DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID,
    DEFAULT_PROTOCOL_FEE as SDK_PROTOCOL_FEE,
    DEFAULT_LP_FEE as SDK_LP_FEE,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize INFO_TREE with redeployment guard
    let info_db = match wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, DARKBET_EXCHANGE_INFO_TREE)?,
    };

    // Store default promissory_note contract ID for cross-contract validation
    wasm::db::db_set(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    // Initialize database trees with redeployment guards
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    }
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MATCHES_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_MATCHES_TREE)?;
    }
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_POSITIONS_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;
    }
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    }
    if wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;
    }


    // V2 circuits (HAZOP RC3: domain separation)
    let add_liquidity_v2_bincode = include_bytes!("../proof/add_liquidity.zk.bin");
    wasm::db::zkas_db_set(&add_liquidity_v2_bincode[..])?;
    let buy_position_v2_bincode = include_bytes!("../proof/buy_position.zk.bin");
    wasm::db::zkas_db_set(&buy_position_v2_bincode[..])?;
    let cancel_order_v2_bincode = include_bytes!("../proof/cancel_order.zk.bin");
    wasm::db::zkas_db_set(&cancel_order_v2_bincode[..])?;
    let claim_winnings_v2_bincode = include_bytes!("../proof/claim_winnings.zk.bin");
    wasm::db::zkas_db_set(&claim_winnings_v2_bincode[..])?;
    let create_market_v2_bincode = include_bytes!("../proof/create_market.zk.bin");
    wasm::db::zkas_db_set(&create_market_v2_bincode[..])?;
    let match_orders_v2_bincode = include_bytes!("../proof/match_orders.zk.bin");
    wasm::db::zkas_db_set(&match_orders_v2_bincode[..])?;
    let place_back_v2_bincode = include_bytes!("../proof/place_back.zk.bin");
    wasm::db::zkas_db_set(&place_back_v2_bincode[..])?;
    let place_lay_v2_bincode = include_bytes!("../proof/place_lay.zk.bin");
    wasm::db::zkas_db_set(&place_lay_v2_bincode[..])?;
    let remove_liquidity_v2_bincode = include_bytes!("../proof/remove_liquidity.zk.bin");
    wasm::db::zkas_db_set(&remove_liquidity_v2_bincode[..])?;
    let resolve_market_v2_bincode = include_bytes!("../proof/resolve_market.zk.bin");
    wasm::db::zkas_db_set(&resolve_market_v2_bincode[..])?;

    Ok(())
}

/// Get metadata for verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DarkbetFunction::try_from(self_.data[0])?;
    let current_block = wasm::util::get_verifying_block_height()?.get();

    let metadata = match func {
        DarkbetFunction::CreateMarketV1 => {
            let params= CreateMarketParamsV1::decode(&self_.data[1..])?;
            let cx = params.creator_pub.x().expect("pk not identity");
            let cy = params.creator_pub.y().expect("pk not identity");
            let close_block = current_block + params.duration_blocks;
            let market_id = poseidon_hash([
                cx, cy,
                pallas::Base::from(close_block),
                pallas::Base::from(current_block),
            ]);
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DARKBET_EXCHANGE_ZKAS_CREATE_MARKET_NS_V2.to_string(),
                vec![market_id, pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        DarkbetFunction::AddLiquidityV1 => {
            let params= AddLiquidityParamsV1::decode(&self_.data[1..])?;
            let px = params.provider.x().expect("pk not identity");
            let py = params.provider.y().expect("pk not identity");
            let lp_share_id = poseidon_hash([
                params.market_id,
                px, py,
                pallas::Base::from(params.amount),
                pallas::Base::from(current_block),
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DARKBET_EXCHANGE_ZKAS_ADD_LIQUIDITY_NS_V2.to_string(),
                vec![lp_share_id, *vc_coords.x(), *vc_coords.y(), pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
        DarkbetFunction::BuyPositionV1 => {
            let params= BuyPositionParamsV1::decode(&self_.data[1..])?;
            let ox = params.owner.x().expect("pk not identity");
            let oy = params.owner.y().expect("pk not identity");
            let position_id = poseidon_hash([
                params.market_id,
                ox, oy,
                pallas::Base::from(params.outcome as u64),
                pallas::Base::from(params.amount),
                pallas::Base::from(current_block),
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DARKBET_EXCHANGE_ZKAS_BUY_POSITION_NS_V2.to_string(),
                vec![position_id, *vc_coords.x(), *vc_coords.y(), pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
        DarkbetFunction::ClaimWinningsV1 => {
            let params= ClaimWinningsParamsV1::decode(&self_.data[1..])?;
            let ox = params.owner.x().expect("pk not identity");
            let oy = params.owner.y().expect("pk not identity");
            let claim_id = poseidon_hash([
                params.market_id,
                params.position_id,
                ox, oy,
                pallas::Base::from(params.winning_outcome as u64),
                pallas::Base::from(current_block),
            ]);
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DARKBET_EXCHANGE_ZKAS_CLAIM_WINNINGS_NS_V2.to_string(),
                vec![claim_id, pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DarkbetFunction::try_from(self_.data[0])?;

    let update_data = match func {
        DarkbetFunction::CreateMarketV1 => {
            darkbet_create_market_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::PlaceBackV1 => darkbet_place_back_process_instruction_v1(cid, call_idx, calls)?,
        DarkbetFunction::PlaceLayV1 => darkbet_place_lay_process_instruction_v1(cid, call_idx, calls)?,
        DarkbetFunction::MatchOrdersV1 => {
            darkbet_match_orders_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::BuyPositionV1 => {
            darkbet_buy_position_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::AddLiquidityV1 => {
            darkbet_add_liquidity_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::RemoveLiquidityV1 => {
            darkbet_remove_liquidity_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::ClaimWinningsV1 => {
            darkbet_claim_winnings_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::ResolveMarketV1 => {
            darkbet_resolve_market_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::SettleMarketV1 => {
            darkbet_settle_market_process_instruction_v1(cid, call_idx, calls)?
        }
        DarkbetFunction::CancelOrderV1 => {
            darkbet_cancel_order_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DarkbetFunction::try_from(update_data[0])? {
        DarkbetFunction::CreateMarketV1 => {
            let update = CreateMarketUpdateV1::decode(&update_data[1..])?;
            darkbet_create_market_process_update_v1(cid, update)
        }
        DarkbetFunction::PlaceBackV1 => {
            let update = PlaceBackUpdateV1::decode(&update_data[1..])?;
            darkbet_place_back_process_update_v1(cid, update)
        }
        DarkbetFunction::PlaceLayV1 => {
            let update = PlaceLayUpdateV1::decode(&update_data[1..])?;
            darkbet_place_lay_process_update_v1(cid, update)
        }
        DarkbetFunction::MatchOrdersV1 => {
            let update = MatchOrdersUpdateV1::decode(&update_data[1..])?;
            darkbet_match_orders_process_update_v1(cid, update)
        }
        DarkbetFunction::BuyPositionV1 => {
            let update = BuyPositionUpdateV1::decode(&update_data[1..])?;
            darkbet_buy_position_process_update_v1(cid, update)
        }
        DarkbetFunction::AddLiquidityV1 => {
            let update = AddLiquidityUpdateV1::decode(&update_data[1..])?;
            darkbet_add_liquidity_process_update_v1(cid, update)
        }
        DarkbetFunction::RemoveLiquidityV1 => {
            let update = RemoveLiquidityUpdateV1::decode(&update_data[1..])?;
            darkbet_remove_liquidity_process_update_v1(cid, update)
        }
        DarkbetFunction::ClaimWinningsV1 => {
            let update = ClaimWinningsUpdateV1::decode(&update_data[1..])?;
            darkbet_claim_winnings_process_update_v1(cid, update)
        }
        DarkbetFunction::ResolveMarketV1 => {
            let update = ResolveMarketUpdateV1::decode(&update_data[1..])?;
            darkbet_resolve_market_process_update_v1(cid, update)
        }
        DarkbetFunction::SettleMarketV1 => {
            let update = SettleMarketUpdateV1::decode(&update_data[1..])?;
            darkbet_settle_market_process_update_v1(cid, update)
        }
        DarkbetFunction::CancelOrderV1 => {
            let update = CancelOrderUpdateV1::decode(&update_data[1..])?;
            darkbet_cancel_order_process_update_v1(cid, update)
        }
    }
}

// ============================================================================
// CREATE MARKET
// ============================================================================

fn darkbet_create_market_process_instruction_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= CreateMarketParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::create_market] Creating market: {}", params.description);

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Validate params
    if params.outcomes.is_empty() {
        return Err(DarkbetError::InvalidOutcome.into())
    }
    if params.outcomes.len() > 20 {
        return Err(DarkbetError::InvalidOutcome.into())
    }
    if params.duration_blocks == 0 || params.duration_blocks > DARKBET_EXCHANGE_MAX_MARKET_LIFETIME {
        return Err(DarkbetError::InvalidDuration.into())
    }

    // Determine market type
    let market_type = match params.market_type {
        0 => MarketType::OrderBook,
        1 => MarketType::AmmPool,
        _ => return Err(DarkbetError::InvalidMarketType.into()),
    };

    // Validate fees for AMM mode
    let protocol_fee = if params.protocol_fee == 0 {
        SDK_PROTOCOL_FEE
    } else {
        if params.protocol_fee > crate::MAX_PROTOCOL_FEE {
            return Err(DarkbetError::InvalidFee.into())
        }
        params.protocol_fee
    };

    let lp_fee = if params.lp_fee == 0 { SDK_LP_FEE } else { params.lp_fee };

    let close_block = current_block + params.duration_blocks;

    // Create message for signature verification using poseidon_hash
    // This avoids serialization issues with String/Vec<String>
    let signature_msg = serialize(&poseidon_hash([
        params.oracle_id,
        params.creator_pub.x().expect("pk not identity"),
        params.creator_pub.y().expect("pk not identity"),
        pallas::Base::from(params.market_type as u64),
        pallas::Base::from(params.commission_bp as u64),
        pallas::Base::from(params.protocol_fee as u64),
        pallas::Base::from(params.lp_fee as u64),
        pallas::Base::from(params.duration_blocks),
        pallas::Base::from(close_block),
    ]));

    // Verify signature from creator
    if !params.creator_pub.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::create_market] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    // Derive market ID from params using poseidon_hash
    let market_id = poseidon_hash([
        params.oracle_id,
        pallas::Base::from(params.market_type as u64),
        pallas::Base::from(close_block),
        pallas::Base::from(current_block),
    ]);

    let update = CreateMarketUpdateV1 {
        market_id,
        creator: params.creator_pub,
        description: params.description,
        outcomes: params.outcomes,
        oracle_id: params.oracle_id,
        commission_bp: params.commission_bp,
        market_type,
        protocol_fee,
        lp_fee,
        close_block,
        instance_seed: params.instance_seed,
    };

    msg!(
        "[darkbet::create_market] Market type: {:?}, closes at block {}, id: {:?}",
        market_type,
        close_block,
        market_id
    );
    Ok(update.encode())
}

fn darkbet_create_market_process_update_v1(
    cid: ContractId,
    update: CreateMarketUpdateV1,
) -> ContractResult {
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Create market struct using the update fields
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let num_outcomes = update.outcomes.len();

    let market = Market {
        version: 1,
        market_id: update.market_id,
        creator: update.creator,
        description: update.description.clone(),
        outcomes: update.outcomes.clone(),
        oracle_id: update.oracle_id,
        commission_bp: update.commission_bp,
        market_type: update.market_type,
        state: MarketState::Open,
        back_volume: 0,
        lay_volume: 0,
        matched_volume: 0,
        total_pool: 0,
        total_lp_shares: 0,
        outcome_pools: if update.market_type == MarketType::AmmPool {
            vec![0; num_outcomes]
        } else {
            vec![]
        },
        protocol_fee: update.protocol_fee,
        lp_fee: update.lp_fee,
        close_block: update.close_block,
        resolved_at: None,
        winning_outcome: None,
        created_at: current_block,
        instance_seed: update.instance_seed,
    };

    wasm::db::db_set(markets_db, &update.market_id.to_repr(), &market.encode())?;

    msg!("[darkbet::create_market::update] Market stored successfully with {} outcomes", num_outcomes);

    Ok(())
}

// ============================================================================
// PLACE BACK (Order-book mode)
// ============================================================================

fn darkbet_place_back_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= PlaceBackParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::place_back] Placing back order on market {:?}", params.market_id);

    // Validate child call is promissory_note::transfer_v1 (0x04) for stake
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[place_back] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[place_back] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Get and validate market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    // Verify market is open and accepting orders
    if market.state != MarketState::Open {
        return Err(DarkbetError::MarketNotOpen.into())
    }

    // Validate outcome index is within bounds
    if params.outcome_index as usize >= market.outcomes.len() {
        return Err(DarkbetError::InvalidOutcome.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Validate order params
    if params.stake < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }
    if params.odds < 10000 {
        return Err(DarkbetError::InvalidOdds.into())
    }

    // Create message for signature verification
    let signature_msg = serialize(&(
        params.market_id,
        params.outcome_index,
        params.odds,
        params.stake,
        current_block,
    ));

    // Verify signature from user
    if !params.user_pub.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::place_back] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    // Derive order_id and nullifier
    let order_id = poseidon_hash([
        params.market_id,
        pallas::Base::from(params.outcome_index as u64),
        pallas::Base::from(current_block),
        pallas::Base::from(params.stake),
    ]);

    let value_blind = poseidon_hash([
        pallas::Base::from(params.stake),
        order_id,
    ]);
    validate_child_value_commit(&child_call.data, params.stake, value_blind)?;

    let nullifier = poseidon_hash([order_id, pallas::Base::from(current_block)]);

    // Check nullifier hasn't been used (replay protection)
    let nullifiers_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &nullifier.to_repr())? {
        return Err(DarkbetError::DuplicateNullifier.into())
    }

    let update = PlaceBackUpdateV1 {
        order_id,
        market_id: params.market_id,
        outcome_index: params.outcome_index,
        odds: params.odds,
        stake: params.stake,
        user_pub: params.user_pub,
        nullifier,
        instance_seed: params.instance_seed,
    };

    msg!("[darkbet::place_back] Back order placed: {} @ {}bps", params.stake, params.odds);
    Ok(update.encode())
}

fn darkbet_place_back_process_update_v1(
    cid: ContractId,
    update: PlaceBackUpdateV1,
) -> ContractResult {
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let _markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;

    // Store order
    let order = Order {
        version: 1,
        order_id: update.order_id,
        market_id: update.market_id,
        order_type: OrderType::Back,
        outcome_index: update.outcome_index,
        odds: update.odds,
        stake: update.stake,
        liability: 0,
        user_pub: update.user_pub,
        state: OrderState::Open,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        nullifier: update.nullifier,
        instance_seed: update.instance_seed,
    };

    wasm::db::db_set(back_orders_db, &update.order_id.to_repr(), &order.encode())?;

    // Record nullifier
    wasm::db::db_mark_spent(nullifiers_db, &update.nullifier.to_repr())?;

    msg!("[darkbet::place_back::update] Back order stored");

    Ok(())
}

// ============================================================================
// PLACE LAY (Order-book mode)
// ============================================================================

fn darkbet_place_lay_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= PlaceLayParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::place_lay] Placing lay order on market {:?}", params.market_id);

    // Validate child call is promissory_note::transfer_v1 (0x04) for stake
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[place_lay] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[place_lay] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Get and validate market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    // Verify market is open and accepting orders
    if market.state != MarketState::Open {
        return Err(DarkbetError::MarketNotOpen.into())
    }

    // Validate outcome index is within bounds
    if params.outcome_index as usize >= market.outcomes.len() {
        return Err(DarkbetError::InvalidOutcome.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Validate order params
    if params.stake < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }
    if params.odds < 10000 {
        return Err(DarkbetError::InvalidOdds.into())
    }

    // Create message for signature verification
    let signature_msg = serialize(&(
        params.market_id,
        params.outcome_index,
        params.odds,
        params.stake,
        current_block,
    ));

    // Verify signature from user
    if !params.user_pub.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::place_lay] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    // Liability = stake * (odds - 1) / 10000
    let liability = params
        .stake
        .checked_mul((params.odds - 10000) as u64)
        .ok_or(DarkbetError::ArithmeticOverflow)?
        / 10000;

    // Derive order_id and nullifier
    let order_id = poseidon_hash([
        params.market_id,
        pallas::Base::from(params.outcome_index as u64),
        pallas::Base::from(current_block),
        pallas::Base::from(params.stake),
        pallas::Base::one(), // Lay indicator
    ]);

    let value_blind = poseidon_hash([
        pallas::Base::from(params.stake),
        order_id,
    ]);
    validate_child_value_commit(&child_call.data, params.stake, value_blind)?;

    let nullifier = poseidon_hash([order_id, pallas::Base::from(current_block)]);

    // Check nullifier hasn't been used (replay protection)
    let nullifiers_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &nullifier.to_repr())? {
        return Err(DarkbetError::DuplicateNullifier.into())
    }

    let update = PlaceLayUpdateV1 {
        order_id,
        market_id: params.market_id,
        outcome_index: params.outcome_index,
        odds: params.odds,
        stake: params.stake,
        liability,
        user_pub: params.user_pub,
        nullifier,
        instance_seed: params.instance_seed,
    };

    msg!("[darkbet::place_lay] Lay order placed: {} liability @ {}bps", liability, params.odds);
    Ok(update.encode())
}

fn darkbet_place_lay_process_update_v1(
    cid: ContractId,
    update: PlaceLayUpdateV1,
) -> ContractResult {
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;
    let _markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;

    // Store order
    let order = Order {
        version: 1,
        order_id: update.order_id,
        market_id: update.market_id,
        order_type: OrderType::Lay,
        outcome_index: update.outcome_index,
        odds: update.odds,
        stake: update.stake,
        liability: update.liability,
        user_pub: update.user_pub,
        state: OrderState::Open,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        nullifier: update.nullifier,
        instance_seed: update.instance_seed,
    };

    wasm::db::db_set(lay_orders_db, &update.order_id.to_repr(), &order.encode())?;

    // Record nullifier
    wasm::db::db_mark_spent(nullifiers_db, &update.nullifier.to_repr())?;

    msg!("[darkbet::place_lay::update] Lay order stored");

    Ok(())
}

// ============================================================================
// MATCH ORDERS (Order-book mode)
// ============================================================================

fn darkbet_match_orders_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= MatchOrdersParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::match_orders] Matching back {:?} with lay {:?}", params.back_order_id, params.lay_order_id);

    // Get and validate market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    if market.state != MarketState::Open {
        return Err(DarkbetError::MarketNotOpen.into())
    }

    // Get back order
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let back_order: Order = match wasm::db::db_get(back_orders_db, &params.back_order_id.to_repr())? {
        Some(data) => Order::decode(&data)?,
        None => return Err(DarkbetError::OrderNotFound.into()),
    };

    // Verify back order is valid
    if back_order.state != OrderState::Open {
        return Err(DarkbetError::OrderAlreadyMatched.into())
    }
    if back_order.market_id != params.market_id {
        return Err(DarkbetError::OrderNotFound.into())
    }

    // Get lay order
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;
    let lay_order: Order = match wasm::db::db_get(lay_orders_db, &params.lay_order_id.to_repr())? {
        Some(data) => Order::decode(&data)?,
        None => return Err(DarkbetError::OrderNotFound.into()),
    };

    // Verify lay order is valid
    if lay_order.state != OrderState::Open {
        return Err(DarkbetError::OrderAlreadyMatched.into())
    }
    if lay_order.market_id != params.market_id {
        return Err(DarkbetError::OrderNotFound.into())
    }

    if back_order.outcome_index != lay_order.outcome_index {
        return Err(DarkbetError::OddsMismatch.into())
    }

    if lay_order.odds < back_order.odds {
        return Err(DarkbetError::OddsMismatch.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create message for signature verification
    let signature_msg = serialize(&(
        params.market_id,
        params.back_order_id,
        params.lay_order_id,
        params.odds,
        current_block,
    ));

    // Verify signature from matcher
    if !params.user_pub.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::match_orders] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    let match_id = poseidon_hash([
        params.market_id,
        params.back_order_id,
        params.lay_order_id,
        pallas::Base::from(back_order.odds as u64),
        pallas::Base::from(current_block),
    ]);

    // Calculate commission based on actual stake
    let back_payout = back_order
        .stake
        .checked_mul(back_order.odds as u64)
        .ok_or(DarkbetError::ArithmeticOverflow)?
        / 10000;
    let commission = back_payout
        .checked_mul(DARKBET_EXCHANGE_COMMISSION_BP as u64)
        .ok_or(DarkbetError::ArithmeticOverflow)?
        / 10000;

    let update = MatchOrdersUpdateV1 {
        match_id,
        market_id: params.market_id,
        back_order_id: params.back_order_id,
        lay_order_id: params.lay_order_id,
        odds: back_order.odds,
        back_stake: back_order.stake,
        lay_liability: lay_order.liability,
        commission,
    };

    msg!("[darkbet::match_orders] Matched at {} odds, commission {}", params.odds, commission);
    Ok(update.encode())
}

fn darkbet_match_orders_process_update_v1(
    cid: ContractId,
    update: MatchOrdersUpdateV1,
) -> ContractResult {
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;
    let matches_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MATCHES_TREE)?;
    let _markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Update back order state
    let mut back_order: Order = match wasm::db::db_get(back_orders_db, &update.back_order_id.to_repr())? {
        Some(data) => Order::decode(&data)?,
        None => return Err(DarkbetError::OrderNotFound.into()),
    };
    back_order.state = OrderState::Matched;
    wasm::db::db_set(back_orders_db, &update.back_order_id.to_repr(), &back_order.encode())?;

    // Update lay order state
    let mut lay_order: Order = match wasm::db::db_get(lay_orders_db, &update.lay_order_id.to_repr())? {
        Some(data) => Order::decode(&data)?,
        None => return Err(DarkbetError::OrderNotFound.into()),
    };
    lay_order.state = OrderState::Matched;
    wasm::db::db_set(lay_orders_db, &update.lay_order_id.to_repr(), &lay_order.encode())?;

    // Store match - outcome_index comes from back_order
    let m = Match {
        version: 1,
        match_id: update.match_id,
        market_id: update.market_id,
        outcome_index: back_order.outcome_index,
        odds: update.odds,
        back_stake: update.back_stake,
        lay_liability: update.lay_liability,
        back_user: back_order.user_pub,
        lay_user: lay_order.user_pub,
        commission: update.commission,
        state: MatchState::Pending,
        created_at: wasm::util::get_verifying_block_height()?.get(),
    };
    wasm::db::db_set(matches_db, &update.match_id.to_repr(), &m.encode())?;

    msg!("[darkbet::match_orders::update] Match stored, orders updated");

    Ok(())
}

// ============================================================================
// BUY POSITION (AMM mode)
// ============================================================================

fn darkbet_buy_position_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[darkbet::BuyPositionV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[darkbet::BuyPositionV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= BuyPositionParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[darkbet::buy_position] Buying position on market {:?}, outcome {}",
        params.market_id,
        params.outcome
    );

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    // Verify market is AMM type
    if market.market_type != MarketType::AmmPool {
        return Err(DarkbetError::InvalidMarketType.into())
    }

    if market.state != MarketState::Open {
        return Err(DarkbetError::MarketNotOpen.into())
    }

    if params.outcome as usize >= market.outcomes.len() {
        return Err(DarkbetError::InvalidOutcome.into())
    }

    // Validate amount
    if params.amount < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create message for signature verification
    let signature_msg = serialize(&(
        params.market_id,
        params.outcome,
        params.amount,
        params.min_payout,
        current_block,
    ));

    // Verify signature from owner
    if !params.owner.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::buy_position] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    // Calculate payout using AMM formula
    let payout = market
        .calculate_position_price(params.outcome, params.amount)
        .map_err(|_| DarkbetError::ArithmeticOverflow)?;

    // Check slippage
    if payout < params.min_payout {
        return Err(DarkbetError::SlippageExceeded.into())
    }

    let position_id = poseidon_hash([
        params.market_id,
        params.owner.x().expect("pk not identity"),
        params.owner.y().expect("pk not identity"),
        pallas::Base::from(params.outcome as u64),
        pallas::Base::from(params.amount),
        pallas::Base::from(current_block),
    ]);

    // Check nullifier hasn't been used (replay protection)
    let nullifiers_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &position_id.to_repr())? {
        return Err(DarkbetError::DuplicateNullifier.into())
    }

    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        position_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    let update = BuyPositionUpdateV1 {
        position_id,
        market_id: params.market_id,
        owner: params.owner,
        outcome: params.outcome,
        amount: params.amount,
        payout,
        created_at: current_block,
        instance_seed: params.instance_seed,
    };

    msg!(
        "[darkbet::buy_position] Position bought: {} tokens, payout {}",
        params.amount,
        payout
    );
    Ok(update.encode())
}

fn darkbet_buy_position_process_update_v1(
    cid: ContractId,
    update: BuyPositionUpdateV1,
) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;

    // Store position
    let position = Position::new(
        update.market_id,
        update.owner,
        update.outcome,
        update.amount,
        update.payout,
        update.created_at,
        update.instance_seed,
    );

    wasm::db::db_set(positions_db, &position.position_id.to_repr(), &position.encode())?;

    // Update market pool
    let market_data = wasm::db::db_get(markets_db, &update.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut market: Market = Market::decode(&market_data)?;
    market.total_pool += update.amount;
    market.outcome_pools[update.outcome as usize] += update.amount;
    wasm::db::db_set(markets_db, &market.market_id.to_repr(), &market.encode())?;

    // Record nullifier
    wasm::db::db_mark_spent(nullifiers_db, &position.position_id.to_repr())?;

    msg!("[darkbet::buy_position::update] Position stored: {:?}", position.position_id);

    Ok(())
}

// ============================================================================
// ADD LIQUIDITY (AMM mode)
// ============================================================================

fn darkbet_add_liquidity_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[darkbet::AddLiquidityV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[darkbet::AddLiquidityV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= AddLiquidityParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[darkbet::add_liquidity] Adding {} liquidity to market {:?}",
        params.amount,
        params.market_id
    );

    // Get and validate market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    if market.market_type != MarketType::AmmPool {
        return Err(DarkbetError::InvalidMarketType.into())
    }

    if market.state != MarketState::Open {
        return Err(DarkbetError::MarketNotOpen.into())
    }

    // Validate amount
    if params.amount < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create message for signature verification
    let signature_msg = serialize(&(params.market_id, params.amount, current_block));

    // Verify signature from provider
    if !params.provider.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::add_liquidity] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    let shares_minted = market
        .calculate_lp_shares(params.amount)
        .ok_or(DarkbetError::ArithmeticOverflow)?;

    let lp_share_id = poseidon_hash([
        params.market_id,
        params.provider.x().expect("pk not identity"),
        params.provider.y().expect("pk not identity"),
        pallas::Base::from(shares_minted),
        pallas::Base::from(current_block),
    ]);

    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        lp_share_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    let update = AddLiquidityUpdateV1 {
        lp_share_id,
        market_id: params.market_id,
        provider: params.provider,
        amount: params.amount,
        shares_minted,
        fees_earned: 0,
        created_at: current_block,
        instance_seed: params.instance_seed,
    };

    msg!("[darkbet::add_liquidity] Adding liquidity: {}", params.amount);
    Ok(update.encode())
}

fn darkbet_add_liquidity_process_update_v1(
    cid: ContractId,
    update: AddLiquidityUpdateV1,
) -> ContractResult {
    let lp_shares_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Get market to calculate shares
    let market_data = wasm::db::db_get(markets_db, &update.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let mut market: Market = Market::decode(&market_data)?;

    // Store LP share (shares_minted already calculated in instruction)
    let lp_share = LpShare::new(update.market_id, update.provider, update.shares_minted, update.created_at, update.instance_seed);

    wasm::db::db_set(lp_shares_db, &lp_share.lp_share_id.to_repr(), &lp_share.encode())?;

    market.total_pool += update.amount;  // Add actual token amount to pool
    market.total_lp_shares += update.shares_minted;  // Add LP shares
    wasm::db::db_set(markets_db, &market.market_id.to_repr(), &market.encode())?;

    msg!("[darkbet::add_liquidity::update] LP shares minted: {:?}", lp_share.lp_share_id);

    Ok(())
}

// ============================================================================
// REMOVE LIQUIDITY (AMM mode)
// ============================================================================

fn darkbet_remove_liquidity_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token transfer
    if this_call.children_indexes.len() != 1 {
        msg!("[darkbet::RemoveLiquidityV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[darkbet::RemoveLiquidityV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= RemoveLiquidityParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[darkbet::remove_liquidity] Removing LP share {:?} from market {:?}",
        params.lp_share_id,
        params.market_id
    );

    // Get LP share
    let lp_shares_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    let lp_share: LpShare = match wasm::db::db_get(lp_shares_db, &params.lp_share_id.to_repr())? {
        Some(data) => LpShare::decode(&data)?,
        None => return Err(DarkbetError::LpShareNotFound.into()),
    };

    if lp_share.market_id != params.market_id {
        return Err(DarkbetError::LpShareNotFound.into())
    }

    // Verify ownership
    if lp_share.provider != params.provider {
        return Err(DarkbetError::UnauthorizedCaller.into())
    }

    // Verify LP share is active
    if lp_share.state != LpShareState::Active {
        return Err(DarkbetError::LpShareNotFound.into())
    }

    // Create message for signature verification
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let signature_msg = serialize(&(params.market_id, params.lp_share_id, current_block));

    // Verify signature from provider
    if !params.provider.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::remove_liquidity] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    // Get market for payout calculation
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    // Calculate payout
    let payout = market
        .calculate_liquidity_payout(lp_share.shares)
        .ok_or(DarkbetError::ArithmeticOverflow)?;
    let fees_withdrawn = lp_share.earned_fees;

    let value_blind = poseidon_hash([
        pallas::Base::from(payout),
        params.lp_share_id,
    ]);
    validate_child_value_commit(&child_call.data, payout, value_blind)?;

    let update = RemoveLiquidityUpdateV1 {
        market_id: params.market_id,
        lp_share_id: params.lp_share_id,
        provider: params.provider,
        shares_burned: lp_share.shares,
        payout,
        fees_withdrawn,
    };

    msg!(
        "[darkbet::remove_liquidity] Payout: {}, fees: {}",
        payout,
        fees_withdrawn
    );
    Ok(update.encode())
}

fn darkbet_remove_liquidity_process_update_v1(
    cid: ContractId,
    update: RemoveLiquidityUpdateV1,
) -> ContractResult {
    let lp_shares_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Update LP share state
    let mut lp_share: LpShare = match wasm::db::db_get(lp_shares_db, &update.lp_share_id.to_repr())? {
        Some(data) => LpShare::decode(&data)?,
        None => return Err(DarkbetError::LpShareNotFound.into()),
    };
    lp_share.state = LpShareState::Removed;
    wasm::db::db_set(lp_shares_db, &lp_share.lp_share_id.to_repr(), &lp_share.encode())?;

    // Update market
    let mut market: Market = match wasm::db::db_get(markets_db, &update.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };
    market.total_pool = market
        .total_pool
        .checked_sub(update.payout)
        .ok_or(DarkbetError::ArithmeticOverflow)?;
    market.total_lp_shares = market
        .total_lp_shares
        .checked_sub(update.shares_burned)
        .ok_or(DarkbetError::ArithmeticOverflow)?;
    wasm::db::db_set(markets_db, &market.market_id.to_repr(), &market.encode())?;

    msg!("[darkbet::remove_liquidity::update] LP shares removed");

    Ok(())
}

// ============================================================================
// CLAIM WINNINGS (AMM mode)
// ============================================================================

fn darkbet_claim_winnings_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token payout
    if this_call.children_indexes.len() != 1 {
        msg!("[darkbet::ClaimWinningsV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[darkbet::ClaimWinningsV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= ClaimWinningsParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::claim_winnings] Claiming winnings for position {:?}", params.position_id);

    // Get position
    let positions_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;
    let position: Position = match wasm::db::db_get(positions_db, &params.position_id.to_repr())? {
        Some(data) => Position::decode(&data)?,
        None => return Err(DarkbetError::PositionNotFound.into()),
    };

    if position.market_id != params.market_id {
        return Err(DarkbetError::PositionNotFound.into())
    }

    // Verify ownership
    if position.owner != params.owner {
        return Err(DarkbetError::UnauthorizedCaller.into())
    }

    // Check not already claimed
    if position.state == PositionState::Claimed {
        return Err(DarkbetError::PositionAlreadyClaimed.into())
    }

    // Get market to verify resolved
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    if market.state != MarketState::Resolved {
        return Err(DarkbetError::MarketNotResolved.into())
    }

    // Verify this position's outcome won
    let winning_outcome = market.winning_outcome.ok_or(DarkbetError::MarketNotResolved)?;
    if position.outcome != winning_outcome {
        // Position didn't win - no winnings
        return Err(DarkbetError::InvalidOutcome.into())
    }

    let value_blind = poseidon_hash([
        pallas::Base::from(position.potential_payout),
        params.position_id,
    ]);
    validate_child_value_commit(&child_call.data, position.potential_payout, value_blind)?;

    let update = ClaimWinningsUpdateV1 {
        position_id: params.position_id,
        payout: position.potential_payout,
        claimed: true,
    };

    msg!("[darkbet::claim_winnings] Claiming payout: {}", update.payout);
    Ok(update.encode())
}

fn darkbet_claim_winnings_process_update_v1(
    cid: ContractId,
    update: ClaimWinningsUpdateV1,
) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;

    // Update position state
    let mut position: Position = match wasm::db::db_get(positions_db, &update.position_id.to_repr())? {
        Some(data) => Position::decode(&data)?,
        None => return Err(DarkbetError::PositionNotFound.into()),
    };
    position.state = PositionState::Claimed;
    wasm::db::db_set(positions_db, &position.position_id.to_repr(), &position.encode())?;

    msg!("[darkbet::claim_winnings::update] Winnings claimed for position {:?}", update.position_id);

    Ok(())
}

// ============================================================================
// RESOLVE MARKET
// ============================================================================

fn darkbet_resolve_market_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= ResolveMarketParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::resolve_market] Resolving market {:?}", params.market_id);

    // Get market and verify it exists
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    // Verify market is in correct state (must be Open or Closed, not already resolved)
    match market.state {
        MarketState::Open | MarketState::Closed => {}
        MarketState::Resolved => return Err(DarkbetError::MarketAlreadyResolved.into()),
        MarketState::Settled => return Err(DarkbetError::MarketAlreadySettled.into()),
        MarketState::Cancelled => return Err(DarkbetError::MarketNotOpen.into()),
    }

    // Validate winning outcome index is within bounds
    if params.winning_outcome as usize >= market.outcomes.len() {
        return Err(DarkbetError::InvalidOutcome.into())
    }

    // Verify the oracle public key x-coordinate matches the market's oracle_id
    // This ensures only the designated oracle can resolve this market
    if params.oracle_pub.x().expect("pk not identity") != market.oracle_id {
        msg!("[darkbet::resolve_market] ERROR: Oracle ID mismatch");
        return Err(DarkbetError::InvalidOracleSignature.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create message for oracle signature verification
    let signature_msg = serialize(&(
        params.market_id,
        params.winning_outcome,
        current_block,
    ));

    // Verify oracle signature
    if !params.oracle_pub.verify(&signature_msg, &params.oracle_signature) {
        msg!("[darkbet::resolve_market] ERROR: Invalid oracle signature");
        return Err(DarkbetError::InvalidOracleSignature.into())
    }

    let update = ResolveMarketUpdateV1 {
        market_id: params.market_id,
        winning_outcome: params.winning_outcome,
        resolved_at_block: current_block,
    };

    msg!("[darkbet::resolve_market] Market resolved at block {}", current_block);
    Ok(update.encode())
}

fn darkbet_resolve_market_process_update_v1(
    cid: ContractId,
    update: ResolveMarketUpdateV1,
) -> ContractResult {
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Get and update market
    let mut market: Market = match wasm::db::db_get(markets_db, &update.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    market.state = MarketState::Resolved;
    market.winning_outcome = Some(update.winning_outcome);
    market.resolved_at = Some(update.resolved_at_block);

    wasm::db::db_set(markets_db, &update.market_id.to_repr(), &market.encode())?;

    msg!("[darkbet::resolve_market::update] Market state updated to Resolved");

    Ok(())
}

// ============================================================================
// SETTLE MARKET
// ============================================================================

fn darkbet_settle_market_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token payouts to winners
    if this_call.children_indexes.len() != 1 {
        msg!("[darkbet::SettleMarketV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[darkbet::SettleMarketV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= SettleMarketParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[darkbet::settle_market] Settling {} matches for market {:?}",
        params.match_ids.len(),
        params.market_id
    );

    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market: Market = match wasm::db::db_get(markets_db, &params.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    if market.state != MarketState::Resolved {
        return Err(DarkbetError::MarketNotResolved.into())
    }

    // Verify winning_outcome is set
    let winning_outcome = market.winning_outcome
        .ok_or(DarkbetError::MarketNotResolved)?;

    // Calculate actual payouts based on match_ids and winning_outcome
    let matches_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MATCHES_TREE)?;
    let mut total_payout: u64 = 0;
    let mut total_commission: u64 = 0;

    if params.match_ids.len() > crate::DARKBET_EXCHANGE_MAX_SETTLE_MATCHES {
        msg!("[darkbet_exchange::settle_market] Too many match IDs: {} (max {})",
            params.match_ids.len(), crate::DARKBET_EXCHANGE_MAX_SETTLE_MATCHES);
        return Err(DarkbetError::InternalError("Too many match IDs".to_string()).into());
    }
    for &match_id in &params.match_ids {
        let match_data = wasm::db::db_get(matches_db, &match_id.to_repr())?
            .ok_or(DarkbetError::MatchNotFound)?;

        let m: Match = Match::decode(&match_data)?;

        // Verify match belongs to this market
        if m.market_id != params.market_id {
            return Err(DarkbetError::MatchNotFound.into())
        }

        // Verify match is in Pending state
        if m.state != MatchState::Pending {
            return Err(DarkbetError::OrderAlreadyMatched.into())
        }

        // Calculate payout if back user won
        if m.outcome_index == winning_outcome {
            // Payout = back_stake * odds / 10000 (decimal odds conversion from basis points)
            let payout = m.back_stake
                .checked_mul(m.odds as u64)
                .ok_or(DarkbetError::ArithmeticOverflow)?
                / 10000;
            total_payout = total_payout.saturating_add(payout);
        }

        // Sum up commission (already calculated at match time)
        total_commission = total_commission.saturating_add(m.commission);
    }

    let value_blind = poseidon_hash([
        pallas::Base::from(total_payout),
        params.market_id,
    ]);
    validate_child_value_commit(&child_call.data, total_payout, value_blind)?;

    let update = SettleMarketUpdateV1 {
        market_id: params.market_id,
        match_ids: params.match_ids.clone(),
        settled_count: params.match_ids.len() as u64,
        total_payout,
        total_commission,
    };

    msg!("[darkbet::settle_market] Settling {} matches", params.match_ids.len());
    Ok(update.encode())
}

fn darkbet_settle_market_process_update_v1(
    cid: ContractId,
    update: SettleMarketUpdateV1,
) -> ContractResult {
    let matches_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MATCHES_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Get and update market
    let mut market: Market = match wasm::db::db_get(markets_db, &update.market_id.to_repr())? {
        Some(data) => Market::decode(&data)?,
        None => return Err(DarkbetError::MarketNotFound.into()),
    };

    // Update market state to Settled
    market.state = MarketState::Settled;

    wasm::db::db_set(markets_db, &update.market_id.to_repr(), &market.encode())?;

    // Process individual matches - update their state to Settled
    for match_id in &update.match_ids {
        let match_data = wasm::db::db_get(matches_db, &match_id.to_repr())?
            .ok_or(DarkbetError::MatchNotFound)?;

        let mut m: Match = Match::decode(&match_data)?;
        m.state = MatchState::Settled;

        wasm::db::db_set(matches_db, &match_id.to_repr(), &m.encode())?;
    }

    msg!(
        "[darkbet::settle_market::update] Settled {} matches, total_payout: {}, total_commission: {}",
        update.settled_count,
        update.total_payout,
        update.total_commission
    );

    Ok(())
}

// ============================================================================
// CANCEL ORDER (Order-book mode)
// ============================================================================

fn darkbet_cancel_order_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token refund
    if this_call.children_indexes.len() != 1 {
        msg!("[darkbet::CancelOrderV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(DarkbetError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[darkbet::CancelOrderV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(DarkbetError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, DARKBET_EXCHANGE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(DarkbetError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= CancelOrderParamsV1::decode(&self_.data[1..])?;

    msg!("[darkbet::cancel_order] Cancelling order {:?}", params.order_id);

    // Check order exists and is open
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;

    let back_order_data = wasm::db::db_get(back_orders_db, &params.order_id.to_repr())?;
    let lay_order_data = wasm::db::db_get(lay_orders_db, &params.order_id.to_repr())?;

    let order_data = if let Some(d) = back_order_data {
        d
    } else {
        lay_order_data.ok_or(DarkbetError::OrderNotFound)?
    };

    let order: Order = Order::decode(&order_data)?;
    if order.state != OrderState::Open {
        return Err(DarkbetError::OrderAlreadyMatched.into())
    }

    // Verify the order belongs to the user making the cancellation request
    if order.user_pub != params.user_pub {
        return Err(DarkbetError::UnauthorizedCaller.into())
    }

    // Create message for signature verification
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let signature_msg = serialize(&(params.order_id, current_block));

    // Verify signature from user
    if !params.user_pub.verify(&signature_msg, &params.signature) {
        msg!("[darkbet::cancel_order] ERROR: Invalid signature");
        return Err(DarkbetError::InvalidSignature.into())
    }

    let value_blind = poseidon_hash([
        pallas::Base::from(order.stake),
        params.order_id,
    ]);
    validate_child_value_commit(&child_call.data, order.stake, value_blind)?;

    let update = CancelOrderUpdateV1 { order_id: params.order_id, refund_amount: order.stake };

    msg!("[darkbet::cancel_order] Order cancelled, refunding {}", order.stake);
    Ok(update.encode())
}

fn darkbet_cancel_order_process_update_v1(
    cid: ContractId,
    update: CancelOrderUpdateV1,
) -> ContractResult {
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;

    // Update order state (try back first, then lay)
    if let Some(order_data) = wasm::db::db_get(back_orders_db, &update.order_id.to_repr())? {
        let mut order: Order = Order::decode(&order_data)?;
        order.state = OrderState::Cancelled;
        wasm::db::db_set(back_orders_db, &update.order_id.to_repr(), &order.encode())?;
    } else if let Some(order_data) = wasm::db::db_get(lay_orders_db, &update.order_id.to_repr())? {
        let mut order: Order = Order::decode(&order_data)?;
        order.state = OrderState::Cancelled;
        wasm::db::db_set(lay_orders_db, &update.order_id.to_repr(), &order.encode())?;
    }

    msg!("[darkbet::cancel_order::update] Order cancelled successfully");

    Ok(())
}