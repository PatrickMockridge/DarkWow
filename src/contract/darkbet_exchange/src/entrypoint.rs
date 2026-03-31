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

//! DarkBet Exchange Contract Entrypoint

use darkfi_sdk::{
    crypto::{ContractId, PublicKey, SecretKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta::pallas, wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize};

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
    DARKBET_EXCHANGE_MIN_ORDER_SIZE, DEFAULT_PROTOCOL_FEE as SDK_PROTOCOL_FEE,
    DEFAULT_LP_FEE as SDK_LP_FEE,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize database trees
    wasm::db::db_init(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    wasm::db::db_init(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    wasm::db::db_init(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;
    wasm::db::db_init(cid, DARKBET_EXCHANGE_MATCHES_TREE)?;
    wasm::db::db_init(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;
    wasm::db::db_init(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    wasm::db::db_init(cid, DARKBET_EXCHANGE_NULLIFIERS_TREE)?;

    Ok(())
}

/// Get metadata for verification
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
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
            let update: CreateMarketUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_create_market_process_update_v1(cid, update)
        }
        DarkbetFunction::PlaceBackV1 => {
            let update: PlaceBackUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_place_back_process_update_v1(cid, update)
        }
        DarkbetFunction::PlaceLayV1 => {
            let update: PlaceLayUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_place_lay_process_update_v1(cid, update)
        }
        DarkbetFunction::MatchOrdersV1 => {
            let update: MatchOrdersUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_match_orders_process_update_v1(cid, update)
        }
        DarkbetFunction::BuyPositionV1 => {
            let update: BuyPositionUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_buy_position_process_update_v1(cid, update)
        }
        DarkbetFunction::AddLiquidityV1 => {
            let update: AddLiquidityUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_add_liquidity_process_update_v1(cid, update)
        }
        DarkbetFunction::RemoveLiquidityV1 => {
            let update: RemoveLiquidityUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_remove_liquidity_process_update_v1(cid, update)
        }
        DarkbetFunction::ClaimWinningsV1 => {
            let update: ClaimWinningsUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_claim_winnings_process_update_v1(cid, update)
        }
        DarkbetFunction::ResolveMarketV1 => {
            let update: ResolveMarketUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_resolve_market_process_update_v1(cid, update)
        }
        DarkbetFunction::SettleMarketV1 => {
            let update: SettleMarketUpdateV1 = deserialize(&update_data[1..])?;
            darkbet_settle_market_process_update_v1(cid, update)
        }
        DarkbetFunction::CancelOrderV1 => {
            let update: CancelOrderUpdateV1 = deserialize(&update_data[1..])?;
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
    let params: CreateMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::create_market] Creating market: {}", params.description);

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()? as u64;

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
    let _protocol_fee = if params.protocol_fee == 0 {
        SDK_PROTOCOL_FEE
    } else {
        if params.protocol_fee > crate::MAX_PROTOCOL_FEE {
            return Err(DarkbetError::InvalidFee.into())
        }
        params.protocol_fee
    };

    let _lp_fee = if params.lp_fee == 0 { SDK_LP_FEE } else { params.lp_fee };

    let close_block = current_block + params.duration_blocks;

    let update = CreateMarketUpdateV1 {
        market_id: pallas::Base::zero(), // Will be computed properly
        market_type,
        close_block,
    };

    msg!(
        "[darkbet::create_market] Market type: {:?}, closes at block {}",
        market_type,
        close_block
    );
    Ok(serialize(&update))
}

fn darkbet_create_market_process_update_v1(
    cid: ContractId,
    _update: CreateMarketUpdateV1,
) -> ContractResult {
    let _markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    msg!("[darkbet::create_market::update] Market created successfully");

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
    let params: PlaceBackParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::place_back] Placing back order on market {:?}", params.market_id);

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?;
    if market_data.is_none() {
        return Err(DarkbetError::MarketNotFound.into())
    }

    let _current_block = wasm::util::get_verifying_block_height()? as u64;

    // Validate order
    if params.stake < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }
    if params.odds < 10000 {
        return Err(DarkbetError::InvalidOdds.into())
    }

    let update = PlaceBackUpdateV1 {
        order_id: pallas::Base::zero(),
        market_id: params.market_id,
        outcome_index: params.outcome_index,
        odds: params.odds,
        stake: params.stake,
        nullifier: pallas::Base::zero(),
    };

    msg!("[darkbet::place_back] Back order placed: {} @ {}bps", params.stake, params.odds);
    Ok(serialize(&update))
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
        order_id: update.order_id,
        market_id: update.market_id,
        order_type: OrderType::Back,
        outcome_index: update.outcome_index,
        odds: update.odds,
        stake: update.stake,
        liability: 0,
        user_pub: PublicKey::from_secret(SecretKey::from(pallas::Base::zero())),
        state: OrderState::Open,
        created_at: wasm::util::get_verifying_block_height()? as u64,
        nullifier: update.nullifier,
    };

    wasm::db::db_set(back_orders_db, &serialize(&update.order_id), &serialize(&order))?;

    // Record nullifier
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

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
    let params: PlaceLayParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::place_lay] Placing lay order on market {:?}", params.market_id);

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?;
    if market_data.is_none() {
        return Err(DarkbetError::MarketNotFound.into())
    }

    let _current_block = wasm::util::get_verifying_block_height()? as u64;

    // Validate order
    if params.stake < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }
    if params.odds < 10000 {
        return Err(DarkbetError::InvalidOdds.into())
    }

    // Liability = stake * (odds - 1)
    let liability = (params.stake * ((params.odds - 10000) as u64)) / 10000;

    let update = PlaceLayUpdateV1 {
        order_id: pallas::Base::zero(),
        market_id: params.market_id,
        outcome_index: params.outcome_index,
        odds: params.odds,
        stake: params.stake,
        liability,
        nullifier: pallas::Base::zero(),
    };

    msg!("[darkbet::place_lay] Lay order placed: {} liability @ {}bps", liability, params.odds);
    Ok(serialize(&update))
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
        order_id: update.order_id,
        market_id: update.market_id,
        order_type: OrderType::Lay,
        outcome_index: update.outcome_index,
        odds: update.odds,
        stake: update.stake,
        liability: update.liability,
        user_pub: PublicKey::from_secret(SecretKey::from(pallas::Base::zero())),
        state: OrderState::Open,
        created_at: wasm::util::get_verifying_block_height()? as u64,
        nullifier: update.nullifier,
    };

    wasm::db::db_set(lay_orders_db, &serialize(&update.order_id), &serialize(&order))?;

    // Record nullifier
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier), &[])?;

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
    let params: MatchOrdersParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::match_orders] Matching back {:?} with lay {:?}", params.back_order_id, params.lay_order_id);

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?;
    if market_data.is_none() {
        return Err(DarkbetError::MarketNotFound.into())
    }

    // Get orders
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let back_order_data = wasm::db::db_get(back_orders_db, &serialize(&params.back_order_id))?;
    if back_order_data.is_none() {
        return Err(DarkbetError::OrderNotFound.into())
    }

    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;
    let lay_order_data = wasm::db::db_get(lay_orders_db, &serialize(&params.lay_order_id))?;
    if lay_order_data.is_none() {
        return Err(DarkbetError::OrderNotFound.into())
    }

    // Calculate commission
    let back_payout = (params.odds as u64 * 100) / 10000;
    let commission = (back_payout * DARKBET_EXCHANGE_COMMISSION_BP as u64) / 10000;

    let update = MatchOrdersUpdateV1 {
        match_id: pallas::Base::zero(),
        market_id: params.market_id,
        back_order_id: params.back_order_id,
        lay_order_id: params.lay_order_id,
        odds: params.odds,
        back_stake: 100,
        lay_liability: 100,
        commission,
    };

    msg!("[darkbet::match_orders] Matched at {} odds, commission {}", params.odds, commission);
    Ok(serialize(&update))
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
    let mut back_order: Order =
        deserialize(&wasm::db::db_get(back_orders_db, &serialize(&update.back_order_id))?.unwrap())?;
    back_order.state = OrderState::Matched;
    wasm::db::db_set(back_orders_db, &serialize(&update.back_order_id), &serialize(&back_order))?;

    // Update lay order state
    let mut lay_order: Order =
        deserialize(&wasm::db::db_get(lay_orders_db, &serialize(&update.lay_order_id))?.unwrap())?;
    lay_order.state = OrderState::Matched;
    wasm::db::db_set(lay_orders_db, &serialize(&update.lay_order_id), &serialize(&lay_order))?;

    // Store match
    let m = Match {
        match_id: update.match_id,
        market_id: update.market_id,
        outcome_index: 0,
        odds: update.odds,
        back_stake: update.back_stake,
        lay_liability: update.lay_liability,
        back_user: PublicKey::from_secret(SecretKey::from(pallas::Base::zero())),
        lay_user: PublicKey::from_secret(SecretKey::from(pallas::Base::zero())),
        commission: update.commission,
        state: MatchState::Pending,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };
    wasm::db::db_set(matches_db, &serialize(&update.match_id), &serialize(&m))?;

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
    let self_ = &calls[call_idx].data;
    let params: BuyPositionParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[darkbet::buy_position] Buying position on market {:?}, outcome {}",
        params.market_id,
        params.outcome
    );

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?;
    let market: Market = if let Some(data) = market_data {
        deserialize(&data)?
    } else {
        return Err(DarkbetError::MarketNotFound.into())
    };

    // Verify market is AMM type
    if market.market_type != MarketType::AmmPool {
        return Err(DarkbetError::InvalidMarketType.into())
    }

    // Validate amount
    if params.amount < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Calculate payout using AMM formula
    let payout = market
        .calculate_position_price(params.outcome, params.amount)
        .map_err(|_| DarkbetError::ArithmeticOverflow)?;

    // Check slippage
    if payout < params.min_payout {
        return Err(DarkbetError::SlippageExceeded.into())
    }

    let update = BuyPositionUpdateV1 {
        position_id: pallas::Base::zero(),
        market_id: params.market_id,
        owner: params.owner,
        outcome: params.outcome,
        amount: params.amount,
        payout,
        created_at: current_block,
    };

    msg!(
        "[darkbet::buy_position] Position bought: {} tokens, payout {}",
        params.amount,
        payout
    );
    Ok(serialize(&update))
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
    );

    wasm::db::db_set(positions_db, &serialize(&position.position_id), &serialize(&position))?;

    // Update market pool
    let market_data = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: Market = deserialize(&market_data)?;
    market.total_pool += update.amount;
    market.outcome_pools[update.outcome as usize] += update.amount;
    wasm::db::db_set(markets_db, &serialize(&market.market_id), &serialize(&market))?;

    // Record nullifier
    wasm::db::db_set(nullifiers_db, &serialize(&position.position_id), &[])?;

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
    let self_ = &calls[call_idx].data;
    let params: AddLiquidityParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[darkbet::add_liquidity] Adding {} liquidity to market {:?}",
        params.amount,
        params.market_id
    );

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?;
    if market_data.is_none() {
        return Err(DarkbetError::MarketNotFound.into())
    }

    // Validate amount
    if params.amount < DARKBET_EXCHANGE_MIN_ORDER_SIZE {
        return Err(DarkbetError::InsufficientStake.into())
    }

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    let update = AddLiquidityUpdateV1 {
        lp_share_id: pallas::Base::zero(),
        market_id: params.market_id,
        provider: params.provider,
        shares_minted: 0, // Calculated in update
        fees_earned: 0,
        created_at: current_block,
    };

    msg!("[darkbet::add_liquidity] Adding liquidity: {}", params.amount);
    Ok(serialize(&update))
}

fn darkbet_add_liquidity_process_update_v1(
    cid: ContractId,
    update: AddLiquidityUpdateV1,
) -> ContractResult {
    let lp_shares_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Get market to calculate shares
    let market_data = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: Market = deserialize(&market_data)?;

    // Calculate shares to mint
    let shares_minted = market.calculate_lp_shares(update.shares_minted);

    // Store LP share
    let lp_share = LpShare::new(update.market_id, update.provider, shares_minted, update.created_at);

    wasm::db::db_set(lp_shares_db, &serialize(&lp_share.lp_share_id), &serialize(&lp_share))?;

    // Update market
    market.total_pool += update.shares_minted;
    market.total_lp_shares += shares_minted;
    wasm::db::db_set(markets_db, &serialize(&market.market_id), &serialize(&market))?;

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
    let self_ = &calls[call_idx].data;
    let params: RemoveLiquidityParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[darkbet::remove_liquidity] Removing LP share {:?} from market {:?}",
        params.lp_share_id,
        params.market_id
    );

    // Get LP share
    let lp_shares_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    let lp_share_data = wasm::db::db_get(lp_shares_db, &serialize(&params.lp_share_id))?;
    if lp_share_data.is_none() {
        return Err(DarkbetError::LpShareNotFound.into())
    }

    let lp_share: LpShare = deserialize(&lp_share_data.unwrap())?;

    // Verify ownership
    if lp_share.provider != params.provider {
        return Err(DarkbetError::UnauthorizedCaller.into())
    }

    // Get market for payout calculation
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: Market = deserialize(&market_data)?;

    // Calculate payout
    let payout = market.calculate_liquidity_payout(lp_share.shares);
    let fees_withdrawn = lp_share.earned_fees;

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
    Ok(serialize(&update))
}

fn darkbet_remove_liquidity_process_update_v1(
    cid: ContractId,
    update: RemoveLiquidityUpdateV1,
) -> ContractResult {
    let lp_shares_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LP_SHARES_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    // Update LP share state
    let lp_share_data = wasm::db::db_get(lp_shares_db, &serialize(&update.lp_share_id))?.unwrap();
    let mut lp_share: LpShare = deserialize(&lp_share_data)?;
    lp_share.state = LpShareState::Removed;
    wasm::db::db_set(lp_shares_db, &serialize(&lp_share.lp_share_id), &serialize(&lp_share))?;

    // Update market
    let market_data = wasm::db::db_get(markets_db, &serialize(&update.market_id))?.unwrap();
    let mut market: Market = deserialize(&market_data)?;
    market.total_pool = market.total_pool.saturating_sub(update.payout);
    market.total_lp_shares = market.total_lp_shares.saturating_sub(update.shares_burned);
    wasm::db::db_set(markets_db, &serialize(&market.market_id), &serialize(&market))?;

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
    let self_ = &calls[call_idx].data;
    let params: ClaimWinningsParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::claim_winnings] Claiming winnings for position {:?}", params.position_id);

    // Get position
    let positions_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;
    let position_data = wasm::db::db_get(positions_db, &serialize(&params.position_id))?;
    if position_data.is_none() {
        return Err(DarkbetError::PositionNotFound.into())
    }

    let position: Position = deserialize(&position_data.unwrap())?;

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
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?.unwrap();
    let market: Market = deserialize(&market_data)?;

    if market.state != MarketState::Resolved {
        return Err(DarkbetError::MarketNotResolved.into())
    }

    // Verify this position's outcome won
    let winning_outcome = market.winning_outcome.ok_or(DarkbetError::MarketNotResolved)?;
    if position.outcome != winning_outcome {
        // Position didn't win - no winnings
        return Err(DarkbetError::InvalidOutcome.into())
    }

    let update = ClaimWinningsUpdateV1 {
        position_id: params.position_id,
        payout: position.potential_payout,
        claimed: true,
    };

    msg!("[darkbet::claim_winnings] Claiming payout: {}", update.payout);
    Ok(serialize(&update))
}

fn darkbet_claim_winnings_process_update_v1(
    cid: ContractId,
    update: ClaimWinningsUpdateV1,
) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_POSITIONS_TREE)?;

    // Update position state
    let position_data = wasm::db::db_get(positions_db, &serialize(&update.position_id))?.unwrap();
    let mut position: Position = deserialize(&position_data)?;
    position.state = PositionState::Claimed;
    wasm::db::db_set(positions_db, &serialize(&position.position_id), &serialize(&position))?;

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
    let params: ResolveMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::resolve_market] Resolving market {:?}", params.market_id);

    // Get market
    let markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;
    let market_data = wasm::db::db_get(markets_db, &serialize(&params.market_id))?;
    if market_data.is_none() {
        return Err(DarkbetError::MarketNotFound.into())
    }

    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // Validate winning outcome
    if params.winning_outcome > 10 {
        return Err(DarkbetError::InvalidOutcome.into())
    }

    let update = ResolveMarketUpdateV1 {
        market_id: params.market_id,
        winning_outcome: params.winning_outcome,
        resolved_at_block: current_block,
    };

    msg!("[darkbet::resolve_market] Market resolved at block {}", current_block);
    Ok(serialize(&update))
}

fn darkbet_resolve_market_process_update_v1(
    cid: ContractId,
    _update: ResolveMarketUpdateV1,
) -> ContractResult {
    let _markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    msg!("[darkbet::resolve_market::update] Market state updated to Resolved");

    Ok(())
}

// ============================================================================
// SETTLE MARKET
// ============================================================================

fn darkbet_settle_market_process_instruction_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params: SettleMarketParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[darkbet::settle_market] Settling {} matches for market {:?}",
        params.match_ids.len(),
        params.market_id
    );

    let update = SettleMarketUpdateV1 {
        market_id: params.market_id,
        settled_count: params.match_ids.len() as u64,
        total_payout: 0,
        total_commission: 0,
    };

    msg!("[darkbet::settle_market] Settling {} matches", params.match_ids.len());
    Ok(serialize(&update))
}

fn darkbet_settle_market_process_update_v1(
    cid: ContractId,
    _update: SettleMarketUpdateV1,
) -> ContractResult {
    let _matches_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MATCHES_TREE)?;
    let _markets_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_MARKETS_TREE)?;

    msg!("[darkbet::settle_market::update] Market settled");

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
    let self_ = &calls[call_idx].data;
    let params: CancelOrderParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[darkbet::cancel_order] Cancelling order {:?}", params.order_id);

    // Check order exists and is open
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;

    let back_order_data = wasm::db::db_get(back_orders_db, &serialize(&params.order_id))?;
    let lay_order_data = wasm::db::db_get(lay_orders_db, &serialize(&params.order_id))?;

    let order_data = if let Some(d) = back_order_data {
        d
    } else {
        lay_order_data.ok_or(DarkbetError::OrderNotFound)?
    };

    let order: Order = deserialize(&order_data)?;
    if order.state != OrderState::Open {
        return Err(DarkbetError::OrderAlreadyMatched.into())
    }

    let update = CancelOrderUpdateV1 { order_id: params.order_id, refund_amount: order.stake };

    msg!("[darkbet::cancel_order] Order cancelled, refunding {}", order.stake);
    Ok(serialize(&update))
}

fn darkbet_cancel_order_process_update_v1(
    cid: ContractId,
    update: CancelOrderUpdateV1,
) -> ContractResult {
    let back_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_BACK_ORDERS_TREE)?;
    let lay_orders_db = wasm::db::db_lookup(cid, DARKBET_EXCHANGE_LAY_ORDERS_TREE)?;

    // Update order state (try back first, then lay)
    if let Some(order_data) = wasm::db::db_get(back_orders_db, &serialize(&update.order_id))? {
        let mut order: Order = deserialize(&order_data)?;
        order.state = OrderState::Cancelled;
        wasm::db::db_set(back_orders_db, &serialize(&update.order_id), &serialize(&order))?;
    } else if let Some(order_data) = wasm::db::db_get(lay_orders_db, &serialize(&update.order_id))? {
        let mut order: Order = deserialize(&order_data)?;
        order.state = OrderState::Cancelled;
        wasm::db::db_set(lay_orders_db, &serialize(&update.order_id), &serialize(&order))?;
    }

    msg!("[darkbet::cancel_order::update] Order cancelled successfully");

    Ok(())
}