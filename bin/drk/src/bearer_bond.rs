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

//! Bearer Bond wallet operations and transaction builders.
//!
//! Follows the same pattern as the Promissory Note module.

use dwow_core::Result;
use dwow_sdk::{
    crypto::{Keypair, SecretKey},
    pasta::{group::ff::PrimeField, pallas},
};
use rand::rngs::OsRng;

use crate::walletdb::BondCoinRecord;

/// Sled key for the bearer bond Merkle tree.
pub const SLED_MERKLE_TREES_BEARER_BOND: &[u8] = b"bearer_bond_merkle_trees";

// ============================================================================
// KEY GENERATION
// ============================================================================

/// Generate a new keypair for bearer bond operations.
///
/// Bearer bond uses the same keypair mechanism as Promissory Note:
/// `poseidon_hash([secret.inner()]) == signature_public` for ownership.
pub async fn keygen(
    wallet: &crate::walletdb::WalletDb,
    output: &mut Vec<String>,
) -> Result<Keypair> {
    // Generate new keypair
    let keypair = Keypair::random(&mut OsRng);

    // Encode to base58
    let public_str = bs58::encode(keypair.public.to_bytes()).into_string();
    let secret_bytes: [u8; 32] = keypair.secret.inner().to_repr();
    let secret_str = bs58::encode(secret_bytes).into_string();

    // Store in wallet
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    wallet.insert_address(&public_str, &secret_str, false, now)
        .map_err(|e| dwow_core::Error::Custom(format!("{:?}", e)))?;
    wallet.insert_secret(&secret_str, "bb_keygen")
        .map_err(|e| dwow_core::Error::Custom(format!("{:?}", e)))?;

    output.push(format!(
        "Bearer bond keypair generated:\n  public:  {}\n  secret:  {}",
        public_str, secret_str
    ));

    Ok(keypair)
}

// ============================================================================
// BOND COIN QUERIES
// ============================================================================

/// Get bond coins from the wallet database.
pub fn get_bond_coins(
    wallet: &crate::walletdb::WalletDb,
    spent: bool,
) -> crate::error::WalletDbResult<Vec<BondCoinRecord>> {
    use rusqlite::params;

    let conn = wallet.conn.lock().map_err(|_| crate::error::WalletDbError::FailedToAquireLock)?;
    let mut stmt = conn.prepare(
        "SELECT coin_id, value_commit_x, value_commit_y, token_commit,
                spend_hook, user_data, leaf_position, secret,
                coin_blind, value_blind, token_blind,
                last_claim_block, maturity_block, issuer_contract,
                interest_rate_bps, spent, spent_at_height, created_at_height
         FROM bond_coins WHERE spent = ?1",
    )?;

    let spent_int: i64 = if spent { 1 } else { 0 };
    let rows = stmt.query_map(params![spent_int], |row| {
        Ok(BondCoinRecord {
            coin_id: row.get(0)?,
            value_commit_x: row.get(1)?,
            value_commit_y: row.get(2)?,
            token_commit: row.get(3)?,
            spend_hook: row.get(4)?,
            user_data: row.get(5)?,
            leaf_position: row.get::<_, i64>(6)? as u64,
            secret: row.get(7)?,
            coin_blind: row.get(8)?,
            value_blind: row.get(9)?,
            token_blind: row.get(10)?,
            last_claim_block: row.get::<_, i64>(11)? as u64,
            maturity_block: row.get::<_, i64>(12)? as u64,
            issuer_contract: row.get(13)?,
            interest_rate_bps: row.get::<_, i64>(14)? as u64,
            spent: row.get::<_, i64>(15)? != 0,
            spent_at_height: row.get::<_, Option<i64>>(16)?.map(|h| h as u32),
            created_at_height: row.get::<_, i64>(17)? as u32,
        })
    })?;

    let mut coins = vec![];
    for row in rows {
        coins.push(row?);
    }
    Ok(coins)
}

// ============================================================================
// TRANSACTION BUILDERS (stubs — full impl wires up client ZK proof builders)
// ============================================================================

/// Build an IssueStakeV1 transaction.
pub async fn build_issue_stake(
    _secret: &SecretKey,
    _token_id: pallas::Base,
    _principal: u64,
    _min_claim: u64,
    _interest_rate_bps: u64,
    _maturity_block: u64,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("IssueStakeV1 transaction builder — stub (BlindOutput_V1 proof)".to_string());
    Ok(vec![])
}

/// Build a TransferStakeV1 transaction.
pub async fn build_transfer_stake(
    _secret: &SecretKey,
    _coin: &BondCoinRecord,
    _recipient: pallas::Base,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("TransferStakeV1 transaction builder — stub (Burn_V1 + BlindOutput_V1)".to_string());
    Ok(vec![])
}

/// Build a RequestInterestV1 transaction.
pub async fn build_request_interest(
    _secret: &SecretKey,
    _coin: &BondCoinRecord,
    _claim_block: u64,
    _min_claim: u64,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("RequestInterestV1 transaction builder — stub (Burn_V1 proof)".to_string());
    Ok(vec![])
}

/// Build an UnstakeV1 transaction.
pub async fn build_unstake(
    _secret: &SecretKey,
    _coin: &BondCoinRecord,
    _current_block: u64,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("UnstakeV1 transaction builder — stub (Burn_V1 + Redeem_V1)".to_string());
    Ok(vec![])
}

/// Build an EmergencyUnstakeV1 transaction.
pub async fn build_emergency_unstake(
    _secret: &SecretKey,
    _coin: &BondCoinRecord,
    _coverage_report: &dwow_bearer_bond_contract::model::CoverageReport,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("EmergencyUnstakeV1 transaction builder — stub".to_string());
    Ok(vec![])
}

/// Build a PayInterestV1 transaction (issuer-side).
pub async fn build_pay_interest(
    _bond_token_commit: pallas::Base,
    _claim_block: u64,
    _interest_amount: u64,
    _payment_key: pallas::Base,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("PayInterestV1 transaction builder — stub (BlindOutput_V1)".to_string());
    Ok(vec![])
}

/// Build a ProveCoverageV1 transaction.
pub async fn build_prove_coverage(
    _series_token_id: pallas::Base,
    _total_outstanding: u64,
    _total_interest_obligation: u64,
    _reserve_amount: u64,
    _report_block: u64,
    output: &mut Vec<String>,
) -> Result<Vec<u8>> {
    output.push("ProveCoverageV1 transaction builder — stub (ProveCoverage_V1)".to_string());
    Ok(vec![])
}
