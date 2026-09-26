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

//! UnderwriteWithCapabilityV1 Implementation
//!
//! Allows underwriting with an O-Cap capability token instead of direct authorization.

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    error::ContractError,
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::deserialize;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};

use crate::error::InsuranceMarketError;
use crate::InsuranceMarketFunction;
use crate::model::{
    calculate_max_coverage,
    derive_underwriter_id,
    UnderwriteWithCapabilityParamsV1,
    UnderwriteWithCapabilityUpdateV1,
};
use crate::{
    INSURANCE_CONTRACT_IDENTITY_CONTRACT_ID, INSURANCE_CONTRACT_INFO_TREE,
    INSURANCE_CONTRACT_MARKETS_TREE, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    INSURANCE_CONTRACT_RISK_TYPES_TREE, INSURANCE_CONTRACT_UNDERWRITERS_TREE,
};

/// Process instruction for UnderwriteWithCapabilityV1
pub fn insurance_market_underwrite_with_capability_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    // Validate child calls: (1) PN::TransferV1 to pay the bond, (2) Identity::VerifyCapabilityV1 to
    // prove the caller holds the capability the market requires.
    //
    // Both were absent, and their absence was two separate holes rather than one. The non-capability
    // path (`underwrite.rs`) requires child 0 and this one required neither, so an underwriter could
    // name any `bond_amount`, transfer nothing, and still be credited `coverage_sold` and a
    // `max_coverage` computed from the unpaid figure. And the only capability check was
    // `market.required_underwriter_capability.is_none()` — *does the market require one* — while the
    // value the circuit publishes as `required_capability_id` came from `params.capability_secret`,
    // the caller's own input, compared to nothing. A capability-gated market was open to anyone
    // (register OBL-Z16).
    //
    // The shapes are ports, not new mechanisms: child 0 from `underwrite.rs:60-83`, child 1 from
    // `labor_market/src/entrypoint.rs`'s `accept_job_with_capability_v1`, which OBL-Z16 names as the
    // repository's only working capability pattern.
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 2 {
        msg!("[insurance_market::underwrite_with_cap] Error: Expected 2 child calls (PN::transfer_v1 + Identity::VerifyCapabilityV1), got {}", this_call.children_indexes.len());
        return Err(InsuranceMarketError::InvalidChildrenIndexes.into())
    }

    // Child 0: PN transfer — moves the bond
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[insurance_market::underwrite_with_cap] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }
    let info_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Child 1: Identity::VerifyCapabilityV1 — the capability check itself, delegated to Identity's
    // own proof. The parent's obligations are the child call's *shape* (present, the right function,
    // addressed to the configured Identity contract) and, at the end of this function, that the
    // capability the child verified is the one this market requires.
    //
    // The second half is not optional here and is not done anywhere else in the tree. Identity reads
    // `capability_id` from its **own** params (`identity/src/entrypoint.rs:583`) and checks the
    // caller's credential against *that* capability's requirement — so a shape-only check admits any
    // valid, unrelated capability. `labor_market` stops at the shape: it reads
    // `job.required_capability_id` (`labor_market/src/entrypoint.rs:1682`) and uses it only in a log
    // line (`:1696`), which is why this file goes further than the pattern it is otherwise ported
    // from (register OBL-Z16).
    let identity_idx = this_call.children_indexes[1];
    let identity_call = &calls[identity_idx].data;
    if identity_call.data[0] != 0x06 {
        msg!("[insurance_market::underwrite_with_cap] Error: Expected Identity::VerifyCapabilityV1 (0x06), got 0x{:02x}", identity_call.data[0]);
        return Err(InsuranceMarketError::InvalidChildCall.into())
    }
    let identity_bytes = wasm::db::db_get(info_db, INSURANCE_CONTRACT_IDENTITY_CONTRACT_ID)?
        .ok_or(InsuranceMarketError::InvalidChildCall)?;
    let identity_cid: ContractId = deserialize(&identity_bytes)?;
    if identity_cid == ContractId::ZERO {
        return Err(ContractError::IoError("identity contract ID not configured".into()));
    }
    validate_child_contract_id(&identity_call.contract_id, &identity_cid)?;

    let self_ = &calls[call_idx].data;
    let params = UnderwriteWithCapabilityParamsV1::decode(&self_.data[1..])?;

    msg!("[insurance_market::underwrite_with_cap] Registering as underwriter with capability");
    msg!("  market_id: {:?}", params.market_id);
    msg!("  bond_amount: {}", params.bond_amount);

    // Look up the market
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;
    let market_bytes = wasm::db::db_get(markets_db, &params.market_id.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let market = crate::model::InsuranceMarket::decode(&market_bytes)?;

    if !market.active {
        return Err(InsuranceMarketError::MarketNotActive.into())
    }

    // Verify market requires a capability for underwriting
    if market.required_underwriter_capability.is_none() {
        return Err(InsuranceMarketError::CapabilityNotMet.into())
    }

    #[expect(clippy::unwrap_used, reason = "guarded by is_none() check above")]
    let required_capability_id = market.required_underwriter_capability.unwrap();

    // The child must have verified *this* capability, not merely some capability.
    //
    // Identity's handler takes `capability_id` from the child's own params, looks that capability up
    // and checks the caller's credential against it. It never learns what this market requires, so
    // without the comparison below a caller holding any valid, unrelated capability is accepted and
    // the market's `required_underwriter_capability` is consulted only for its `is_none()`.
    //
    // `CapabilityId::to_bytes()` is `pallas::Base::to_repr()` and the market stores a `[u8; 32]`
    // (`model/mod.rs`), so this is a byte-for-byte comparison of the same encoding — not a re-hash.
    let identity_params = dwow_identity_contract::model::VerifyCapabilityParams::decode(
        &identity_call.data[1..],
    )?;
    if identity_params.capability_proof.capability_id.to_bytes() != required_capability_id {
        msg!("[insurance_market::underwrite_with_cap] Error: child verified capability {:?}, market requires {:?}",
             identity_params.capability_proof.capability_id.to_bytes(), required_capability_id);
        return Err(InsuranceMarketError::CapabilityNotMet.into())
    }

    // ZK proof verified by host via get_metadata
    // (namespace: INSURANCE_MARKET_ZKAS_UNDERWRITE_WITH_CAPABILITY_NS_V1)

    // Look up risk type to get min bond rate
    let risk_types_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_RISK_TYPES_TREE)?;
    let risk_type_bytes =
        wasm::db::db_get(risk_types_db, &market.risk_type.to_repr())?.ok_or(ContractError::DbGetEmpty)?;
    let risk_type = crate::model::RiskType::decode(&risk_type_bytes)?;

    // Validate bond amount meets minimum
    let min_bond = (params.coverage_limit * risk_type.min_bond_rate as u64) / 10000;
    if params.bond_amount < min_bond {
        return Err(InsuranceMarketError::InsufficientBond.into())
    }

    // Calculate max coverage this bond can support (10x leverage default)
    let coverage_leverage = 10u32;
    let max_coverage = calculate_max_coverage(params.bond_amount, coverage_leverage)?;

    if params.coverage_limit > max_coverage {
        return Err(InsuranceMarketError::BondTooSmall.into())
    }

    // Check if coverage_limit would exceed market's remaining coverage
    let remaining_coverage = market.total_coverage - market.coverage_sold;
    if params.coverage_limit > remaining_coverage {
        return Err(InsuranceMarketError::InsufficientCoverage.into())
    }

    // Derive underwriter ID
    let underwriter_id =
        derive_underwriter_id(params.market_id, &params.underwriter, params.bond_amount);

    // The child call must move *this* bond, not merely exist. `underwrite.rs:166-170` is the source
    // of both the blind and the comparison, and this is the half the shape guard above cannot do:
    // without it a caller attaches a transfer of any amount — one unit — while declaring
    // `bond_amount` arbitrarily large, and the coverage credited below is still computed from the
    // declared figure. `validate_child_value_commit` recomputes
    // `pedersen(bond_amount, fp_mod_fv(value_blind))` and compares it to the child's commitment.
    let value_blind = poseidon_hash([
        pallas::Base::from(params.bond_amount),
        underwriter_id,
    ]);
    validate_child_value_commit(&child_call.data, params.bond_amount, value_blind)?;

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Build both records here. apply used to read the underwriter (or construct it) and read the
    // market back after the block was accepted — reads the ACL denies in `Update` (register
    // OBL-C72).
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let underwriter = if wasm::db::db_contains_key(underwriters_db, &underwriter_id.to_repr())? {
        let bytes = wasm::db::db_get(underwriters_db, &underwriter_id.to_repr())?
            .ok_or(ContractError::DbGetEmpty)?;
        let mut existing = crate::model::Underwriter::decode(&bytes)?;
        existing.bond_amount += params.bond_amount;
        existing.coverage_provided += params.coverage_limit;
        existing
    } else {
        crate::model::Underwriter {
            version: 1,
            id: underwriter_id,
            owner: params.underwriter,
            market_id: params.market_id,
            bond_amount: params.bond_amount,
            coverage_provided: params.coverage_limit,
            coverage_sold: 0,
            earned_premiums: 0,
            claims_paid: 0,
            slash_count: 0,
            performance_score: 10000, // Start at perfect score
            active: true,
            created_at: current_block,
        }
    };

    let mut market = market;
    market.coverage_sold += params.coverage_limit;

    // Create the update
    let update = UnderwriteWithCapabilityUpdateV1 {
        underwriter_id,
        market_id: params.market_id,
        owner: params.underwriter,
        bond_amount: params.bond_amount,
        coverage_provided: params.coverage_limit,
        required_capability_id,
        created_at: current_block,
        underwriter_bytes: underwriter.encode(),
        market_bytes: market.encode(),
    };

    msg!(
        "[insurance_market::underwrite_with_cap] Underwriter registered with capability: {:?}",
        underwriter_id
    );
    Ok([&[InsuranceMarketFunction::UnderwriteWithCapabilityV1 as u8],
        &update.encode()?[..]].concat())
}

/// Process update for UnderwriteWithCapabilityV1
pub fn insurance_market_underwrite_with_capability_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: UnderwriteWithCapabilityUpdateV1,
) -> Result<(), ContractError> {
    let underwriters_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_UNDERWRITERS_TREE)?;
    let markets_db = wasm::db::db_lookup(cid, INSURANCE_CONTRACT_MARKETS_TREE)?;

    // Blind writes — exec built the underwriter (new or updated) and advanced the market's
    // coverage_sold, and carried both (register OBL-C72).
    wasm::db::db_set(
        underwriters_db,
        &update.underwriter_id.to_repr(),
        &update.underwriter_bytes,
    )?;

    wasm::db::db_set(
        markets_db,
        &update.market_id.to_repr(),
        &update.market_bytes,
    )?;

    msg!(
        "[insurance_market::underwrite_with_cap::update] Underwriter: {:?}, Coverage: {}, Required Cap: {:?}",
        update.underwriter_id,
        update.coverage_provided,
        update.required_capability_id
    );
    Ok(())
}