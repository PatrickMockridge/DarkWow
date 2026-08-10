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

//! Capability selection algorithms for wallet transaction building.
//!
//! Production patterns:
//!   - Single cap first (privacy-preserving, avoids linking)
//!   - Largest-first accumulation fallback (fewest inputs)
//!   - Dust threshold for change outputs
//!   - Fee-aware: ensure fee cap is distinct from transfer caps
//!
//! HAZOP T3/T4/T5 remediation (2026-07-01).

use crate::wallet_error::{Error, Result};
use crate::walletdb::CapRecord;
use crate::contract_imports::NATIVE_TOKEN_CONTRACT_ID;
use dwow_sdk::crypto::TokenId;

/// Minimum value for a change output. Outputs below this are dust
/// and are added to the fee instead of creating an uneconomical output.
/// 1 DRKW = 1_000_000_000 base units, so 10_000 = 0.00001 DRKW.
pub const DUST_THRESHOLD: u64 = 10_000;

/// Result of capability selection.
#[derive(Debug, Clone)]
pub struct CapSelection {
    /// Selected capabilities for the transfer (spendable inputs)
    pub inputs: Vec<CapRecord>,
    /// Total value of selected inputs
    pub total_input: u64,
    /// Change amount to return to sender (0 if exact match)
    pub change: u64,
    /// Fee capability (distinct from transfer inputs, may be None if transfer
    /// is not paying a fee or if fee is included in transfer inputs)
    pub fee_cap: Option<CapRecord>,
}

/// Select capabilities for a transfer with fee awareness.
///
/// Strategy:
///   1. Single cap ≥ target + fee — privacy-preserving, no linking
///   2. Multi-input accumulation — largest-first greedy, fewest inputs
///   3. Fee cap selection — distinct from transfer inputs when possible
///
/// Returns `CapSelection` or an error if insufficient funds.
pub fn select_caps(
    available: &[CapRecord],
    asset_id: &TokenId,
    transfer_amount: u64,
    fee_amount: u64,
) -> Result<CapSelection> {
    let target = transfer_amount + fee_amount;

    // Filter to matching asset
    let matching: Vec<&CapRecord> = available
        .iter()
        .filter(|c| &c.asset_id == asset_id && c.status.is_none() && c.contract_id == *NATIVE_TOKEN_CONTRACT_ID)
        .collect();

    if matching.is_empty() {
        return Err(Error::Custom(format!(
            "No retained capabilities for asset {}",
            bs58::encode(&asset_id.to_bytes()).into_string()
        )));
    }

    // Strategy 1: Single cap ≥ target
    if let Some(cap) = matching.iter().find(|c| c.value >= target) {
        let change = cap.value - target;
        let change = if change < DUST_THRESHOLD { 0 } else { change };
        // Single cap covers both transfer + fee
        return Ok(CapSelection {
            inputs: vec![(*cap).clone()],
            total_input: cap.value,
            change,
            fee_cap: None, // fee paid from same cap, change covers it
        });
    }

    // Strategy 2: Multi-input accumulation (largest-first greedy)
    let mut sorted: Vec<&CapRecord> = matching.iter().copied().collect();
    sorted.sort_by(|a, b| b.value.cmp(&a.value));

    let mut selected: Vec<CapRecord> = Vec::new();
    let mut accumulated: u64 = 0;

    for &cap in &sorted {
        selected.push(cap.clone());
        accumulated += cap.value;
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

    Ok(CapSelection {
        inputs: selected,
        total_input: accumulated,
        change,
        fee_cap: None, // fee paid from accumulated caps, change covers it
    })
}

/// Select a separate capability for fee payment.
///
/// Used when the transfer cap(s) should not also pay the fee
/// (e.g., transfer is in a non-DRKW asset, or privacy preference).
///
/// `fee_asset_id` is the asset ID used for fee payment
/// (typically the DRKW native token).
///
/// Returns the selected fee capability or an error.
pub fn select_fee_cap(
    available: &[CapRecord],
    exclude_cap_ids: &[String],
    fee_amount: u64,
    fee_asset_id: &TokenId,
) -> Result<CapRecord> {
    let fee_caps: Vec<&CapRecord> = available
        .iter()
        .filter(|c| {
            &c.asset_id == fee_asset_id
                && c.status.is_none()
                && c.contract_id == *NATIVE_TOKEN_CONTRACT_ID
                && !exclude_cap_ids.contains(&c.cap_id)
        })
        .collect();

    // Try single cap ≥ fee
    if let Some(cap) = fee_caps.iter().find(|c| c.value >= fee_amount) {
        return Ok((*cap).clone());
    }

    // Error: no suitable fee cap
    Err(Error::Custom(format!(
        "No DRKW capability available for fee payment (need {}). \
         Excluded capabilities: {:?}. Available DRKW capabilities: {}",
        fee_amount,
        exclude_cap_ids,
        fee_caps.len()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapStatus;
    use dwow_chain::CoinCommitment;
    use dwow_sdk::crypto::BaseBlind;
    use dwow_sdk::crypto::Blind;
    use dwow_sdk::pasta::pallas;

    /// Test helper: label → TokenId
    fn tid(label: &str) -> TokenId {
        let mut arr = [0u8; 32];
        let bytes = label.as_bytes();
        let len = bytes.len().min(32);
        arr[..len].copy_from_slice(&bytes[..len]);
        TokenId::from_bytes(arr).unwrap()
    }

    fn make_cap(cap_id: &str, asset_label: &str, value: u64) -> CapRecord {
        CapRecord {
            cap_id: cap_id.to_string(),
            value,
            asset_id: tid(asset_label),
            spend_hook: None,
            user_data: None,
            leaf_position: 0,
            commitment: CoinCommitment::from_bytes([0u8; 32]).unwrap(),
            contract_id: *NATIVE_TOKEN_CONTRACT_ID,
            func_id: None,
            cap_blind: BaseBlind::from_u64(0u64),
            value_blind: Blind(pallas::Scalar::from(0u64)),
            asset_blind: BaseBlind::from_u64(0u64),
            capability_discriminant: None,
            capability_name: None,
            resource: None,
            action: None,
            primitives: vec![],
            barbs: vec![],
            status: None,
            status_height: None,
            revoked: false,
            revoked_at_height: None,
            created_at_height: 0,
            key_coords: None,
        }
    }

    #[test]
    fn test_single_cap_covers_all() {
        let caps = vec![
            make_cap("a", "DRKW", 100_000_000),
            make_cap("b", "DRKW", 50_000_000),
        ];
        let sel = select_caps(&caps, &tid("DRKW"), 40_000_000, 1).unwrap();
        assert_eq!(sel.inputs.len(), 1);
        assert_eq!(sel.inputs[0].cap_id, "a");
        assert_eq!(sel.change, 100_000_000 - 82_000_000);
    }

    #[test]
    fn test_multi_cap_accumulation() {
        let caps = vec![
            make_cap("a", "DRKW", 30_000_000),
            make_cap("b", "DRKW", 30_000_000),
            make_cap("c", "DRKW", 30_000_000),
        ];
        let sel = select_caps(&caps, &tid("DRKW"), 70_000_000, 0).unwrap();
        assert_eq!(sel.inputs.len(), 3);
        assert_eq!(sel.total_input, 90_000_000);
    }

    #[test]
    fn test_insufficient_funds() {
        let caps = vec![
            make_cap("a", "DRKW", 10_000_000),
        ];
        let err = select_caps(&caps, &tid("DRKW"), 100_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("Insufficient funds"));
    }

    #[test]
    fn test_dust_threshold() {
        let caps = vec![
            make_cap("a", "DRKW", 82_001_000),
        ];
        // transfer + fee = 82_000_000, change = 1_000 which is < DUST_THRESHOLD
        let sel = select_caps(&caps, &tid("DRKW"), 40_000_000, 1).unwrap();
        assert_eq!(sel.change, 0); // dust suppressed
    }

    #[test]
    fn test_fee_cap_selection() {
        let caps = vec![
            make_cap("a", "DRKW", 50_000_000),
            make_cap("b", "DRKW", 100_000_000),
        ];
        let fee = select_fee_cap(&caps, &["a".into()], 1, &tid("DRKW")).unwrap();
        assert_eq!(fee.cap_id, "b"); // "a" excluded, "b" selected
    }

    #[test]
    fn test_revoked_caps_excluded() {
        let mut caps = vec![
            make_cap("a", "DRKW", 100_000_000),
        ];
        caps[0].status = Some(CapStatus::Processing);
        caps[0].revoked = true;
        let err = select_caps(&caps, &tid("DRKW"), 50_000_000, 0).unwrap_err();
        assert!(err.to_string().contains("No retained capabilities"));
    }
}
