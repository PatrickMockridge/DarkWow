/* This file is part of DarkWow
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

//! Coin selection algorithms for wallet transaction building.
//!
//! Production patterns (Bitcoin Core, Geth):
//!   - Single coin first (privacy-preserving, avoids linking)
//!   - Largest-first accumulation fallback (fewest inputs)
//!   - Dust threshold for change outputs
//!   - Fee-aware: ensure fee coin is distinct from transfer coins
//!
//! HAZOP T3/T4/T5 remediation (2026-07-01).

use crate::wallet_error::{Error, Result};
use crate::walletdb::CapRecord;
use dwow_chain::CoinCommitment;
use dwow_sdk::crypto::{BaseBlind, Blind, ContractId, TokenId};
use dwow_sdk::pasta::pallas;

/// Minimum value for a change output. Outputs below this are dust
/// and are added to the fee instead of creating an uneconomical output.
/// 1 DRKW = 1_000_000_000 base units, so 10_000 = 0.00001 DRKW.
pub const DUST_THRESHOLD: u64 = 10_000;

/// Result of coin selection.
#[derive(Debug, Clone)]
pub struct CoinSelection {
    /// Selected coins for the transfer (spendable inputs)
    pub inputs: Vec<CapRecord>,
    /// Total value of selected inputs
    pub total_input: u64,
    /// Change amount to return to sender (0 if exact match)
    pub change: u64,
    /// Fee coin (distinct from transfer inputs, may be None if transfer
    /// is not paying a fee or if fee is included in transfer inputs)
    pub fee_coin: Option<CapRecord>,
}

/// Select coins for a transfer with fee awareness.
///
/// Strategy:
///   1. Single coin ≥ target + fee — privacy-preserving, no linking
///   2. Multi-input accumulation — largest-first greedy, fewest inputs
///   3. Fee coin selection — distinct from transfer inputs when possible
///
/// Returns `CoinSelection` or an error if insufficient funds.
pub fn select_coins(
    available: &[CapRecord],
    token_id: &TokenId,
    transfer_amount: u64,
    fee_amount: u64,
) -> Result<CoinSelection> {
    let target = transfer_amount + fee_amount;

    // Filter to matching token
    let matching: Vec<&CapRecord> = available
        .iter()
        .filter(|c| &c.token_id == token_id && !c.revoked)
        .collect();

    if matching.is_empty() {
        return Err(Error::Custom(format!(
            "No retained capabilities for resource {}",
            bs58::encode(&token_id.to_bytes()).into_string()
        )));
    }

    // Strategy 1: Single coin ≥ target
    if let Some(coin) = matching.iter().find(|c| c.value >= target) {
        let change = coin.value - target;
        let change = if change < DUST_THRESHOLD { 0 } else { change };
        // Single coin covers both transfer + fee
        return Ok(CoinSelection {
            inputs: vec![(*coin).clone()],
            total_input: coin.value,
            change,
            fee_coin: None, // fee paid from same coin, change covers it
        });
    }

    // Strategy 2: Multi-input accumulation (largest-first greedy)
    let mut sorted: Vec<&CapRecord> = matching.iter().copied().collect();
    sorted.sort_by(|a, b| b.value.cmp(&a.value));

    let mut selected: Vec<CapRecord> = Vec::new();
    let mut accumulated: u64 = 0;

    for &coin in &sorted {
        selected.push(coin.clone());
        accumulated += coin.value;
        if accumulated >= target {
            break;
        }
    }

    if accumulated < target {
        return Err(Error::Custom(format!(
            "Insufficient funds: need {} but only have {} across {} capabilities",
            target, accumulated, selected.len()
        )));
    }

    let change = accumulated - target;
    let change = if change < DUST_THRESHOLD { 0 } else { change };

    Ok(CoinSelection {
        inputs: selected,
        total_input: accumulated,
        change,
        fee_coin: None, // fee paid from accumulated coins, change covers it
    })
}

/// Select a separate coin for fee payment.
///
/// Used when the transfer coin(s) should not also pay the fee
/// (e.g., transfer is in a non-DRKW token, or privacy preference).
///
/// `fee_token_id` is the bs58-encoded token ID used for fee payment
/// (typically the DRKW native token).
///
/// Returns the selected fee coin or an error.
pub fn select_fee_coin(
    available: &[CapRecord],
    exclude_cap_ids: &[String],
    fee_amount: u64,
    fee_token_id: &TokenId,
) -> Result<CapRecord> {
    let fee_coins: Vec<&CapRecord> = available
        .iter()
        .filter(|c| {
            &c.token_id == fee_token_id
                && !c.revoked
                && !exclude_cap_ids.contains(&c.cap_id)
        })
        .collect();

    // Try single coin ≥ fee
    if let Some(coin) = fee_coins.iter().find(|c| c.value >= fee_amount) {
        return Ok((*coin).clone());
    }

    // Error: no suitable fee coin
    Err(Error::Custom(format!(
        "No DRKW capability available for fee payment (need {}). \
         Excluded capabilities: {:?}. Available DRKW capabilities: {}",
        fee_amount,
        exclude_cap_ids,
        fee_coins.len()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: label → TokenId
    fn tid(label: &str) -> TokenId {
        let mut arr = [0u8; 32];
        let bytes = label.as_bytes();
        let len = bytes.len().min(32);
        arr[..len].copy_from_slice(&bytes[..len]);
        TokenId::from_bytes(arr).unwrap()
    }

    fn make_cap(cap_id: &str, token_label: &str, value: u64) -> CapRecord {
        CapRecord {
            cap_id: cap_id.to_string(),
            value,
            token_id: tid(token_label),
            spend_hook: None,
            user_data: None,
            leaf_position: 0,
            commitment: CoinCommitment::from_bytes([0u8; 32]).unwrap(),
            contract_id: ContractId::ZERO,
            func_id: None,
            cap_blind: BaseBlind::from(0u64),
            value_blind: Blind(pallas::Scalar::from(0u64)),
            token_blind: BaseBlind::from(0u64),
            capability_discriminant: None,
            revoked: false,
            revoked_at_height: None,
            created_at_height: 0,
        }
    }

    #[test]
    fn test_single_coin_covers_all() {
        let caps = vec![
            make_cap("a", "DRKW", 100_000_000),
            make_cap("b", "DRKW", 50_000_000),
        ];
        let sel = select_coins(&caps, &tid("DRKW"), 40_000_000, 42_000_000).unwrap();
        assert_eq!(sel.inputs.len(), 1);
        assert_eq!(sel.inputs[0].cap_id, "a");
        assert_eq!(sel.change, 100_000_000 - 82_000_000);
    }

    #[test]
    fn test_multi_input_accumulation() {
        let caps = vec![
            make_cap("a", "DRKW", 30_000_000),
            make_cap("b", "DRKW", 30_000_000),
            make_cap("c", "DRKW", 30_000_000),
        ];
        let sel = select_coins(&caps, &tid("DRKW"), 70_000_000, 0).unwrap();
        assert_eq!(sel.inputs.len(), 3);
        assert_eq!(sel.total_input, 90_000_000);
    }

    #[test]
    fn test_insufficient_funds() {
        let caps = vec![
            make_cap("a", "DRKW", 10_000_000),
        ];
        let err = select_coins(&caps, &tid("DRKW"), 100_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("Insufficient funds"));
    }

    #[test]
    fn test_dust_threshold() {
        let caps = vec![
            make_cap("a", "DRKW", 82_001_000),
        ];
        // transfer + fee = 82_000_000, change = 1_000 which is < DUST_THRESHOLD
        let sel = select_coins(&caps, &tid("DRKW"), 40_000_000, 42_000_000).unwrap();
        assert_eq!(sel.change, 0); // dust suppressed
    }

    #[test]
    fn test_fee_coin_selection() {
        let caps = vec![
            make_cap("a", "DRKW", 50_000_000),
            make_cap("b", "DRKW", 100_000_000),
        ];
        let fee = select_fee_coin(&caps, &["a".into()], 42_000_000, &tid("DRKW")).unwrap();
        assert_eq!(fee.cap_id, "b"); // "a" excluded, "b" selected
    }

    #[test]
    fn test_revoked_coins_excluded() {
        let mut caps = vec![
            make_cap("a", "DRKW", 100_000_000),
        ];
        caps[0].revoked = true;
        let err = select_coins(&caps, &tid("DRKW"), 50_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("No retained capabilities"));
    }
}
