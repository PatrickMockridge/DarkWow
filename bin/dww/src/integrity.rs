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

//! Startup integrity checks for the wallet database.
//!
//! Runs after wallet initialization and daemon startup. Verifies that the
//! SQLite database is internally consistent: tables exist, capabilities
//! have matching proofs, height markers are coherent, and no critical
//! columns contain nulls.

use crate::error::WalletDbResult;
use crate::walletdb::WalletDb;

/// Severity of an integrity check failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegritySeverity {
    /// Informational — not a problem, just a note.
    Info,
    /// Warning — unusual but not corrupt. Operator should review.
    Warn,
    /// Error — corruption detected. Manual recovery recommended.
    Error,
    /// Fatal — wallet cannot function. Abort startup.
    Fatal,
}

/// Result of a single integrity check.
#[derive(Debug, Clone)]
pub struct IntegrityCheckResult {
    /// Human-readable check name (e.g. "table existence").
    pub check_name: &'static str,
    /// Whether the check passed.
    pub passed: bool,
    /// Severity of the failure (only meaningful if !passed).
    pub severity: IntegritySeverity,
    /// Description of what was found.
    pub message: String,
    /// Suggested recovery action, if any.
    pub recovery: Option<String>,
}

/// Tables expected to exist in the wallet database.
const EXPECTED_TABLES: &[&str] = &[
    "scanned_blocks",
    "chain_blocks",
    "held_capabilities",
    "capability_proofs",
    "merkle_trees",
    "key_lifecycle",
    "contract_metadata",
];

/// Tables that are critical — the wallet cannot function without them.
/// Print integrity check results to stderr. FATAL/ERROR/WARN produce output;
/// INFO results are silent unless they failed.
pub fn print_integrity_results(results: &[IntegrityCheckResult]) {
    for r in results {
        if r.passed {
            continue; // only print failures/warnings
        }
        let prefix = match r.severity {
            IntegritySeverity::Fatal => "FATAL",
            IntegritySeverity::Error => "ERROR",
            IntegritySeverity::Warn  => "WARN",
            IntegritySeverity::Info  => "INFO",
        };
        eprintln!("[integrity] {prefix}: {} — {}", r.check_name, r.message);
        if let Some(ref recovery) = r.recovery {
            eprintln!("[integrity]   Recovery: {}", recovery);
        }
    }
}

const CRITICAL_TABLES: &[&str] = &[
    "held_capabilities",
    "capability_proofs",
    "scanned_blocks",
    "chain_blocks",
];

impl WalletDb {
    /// Run all startup integrity checks. Returns a list of results.
    /// Does NOT abort on failure — the caller decides based on severity.
    pub fn integrity_check(&self) -> WalletDbResult<Vec<IntegrityCheckResult>> {
        let mut results = Vec::new();

        results.extend(self.check_table_existence());
        results.extend(self.check_orphaned_caps());
        results.extend(self.check_height_consistency());
        results.extend(self.check_critical_column_nulls());
        // Block hash validity requires chain_blocks to be populated —
        // skipped silently if no blocks exist yet.
        if let Ok(h) = self.chain_height() {
            if h > dwow_sdk::blockchain::BlockHeight::new(0) {
                results.extend(self.check_block_hash_validity());
            }
        }

        Ok(results)
    }

    fn check_table_existence(&self) -> Vec<IntegrityCheckResult> {
        let mut results = Vec::new();
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                results.push(IntegrityCheckResult {
                    check_name: "table existence",
                    passed: false,
                    severity: IntegritySeverity::Fatal,
                    message: "Cannot lock database connection".into(),
                    recovery: Some("Restart the wallet. If the problem persists, the database file may be locked by another process.".into()),
                });
                return results;
            }
        };

        let mut existing = std::collections::HashSet::new();
        let mut stmt = match conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
            Ok(s) => s,
            Err(e) => {
                results.push(IntegrityCheckResult {
                    check_name: "table existence",
                    passed: false,
                    severity: IntegritySeverity::Fatal,
                    message: format!("Cannot query sqlite_master: {e}"),
                    recovery: Some("The database file may be corrupt. Restore from backup or delete and re-sync.".into()),
                });
                return results;
            }
        };
        let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(r) => r,
            Err(e) => {
                results.push(IntegrityCheckResult {
                    check_name: "table existence",
                    passed: false,
                    severity: IntegritySeverity::Fatal,
                    message: format!("Cannot read table list: {e}"),
                    recovery: Some("The database file may be corrupt.".into()),
                });
                return results;
            }
        };
        for row in rows.flatten() {
            existing.insert(row);
        }

        let mut missing_critical = false;
        for table in EXPECTED_TABLES {
            if !existing.contains(*table) {
                let is_critical = CRITICAL_TABLES.contains(table);
                let severity = if is_critical {
                    missing_critical = true;
                    IntegritySeverity::Fatal
                } else {
                    IntegritySeverity::Warn
                };
                results.push(IntegrityCheckResult {
                    check_name: "table existence",
                    passed: false,
                    severity,
                    message: format!("Table '{table}' is missing"),
                    recovery: Some(
                        "Run 'wallet initialize' to recreate missing tables. \
                         If critical tables are missing, the wallet may need to rescan.".into(),
                    ),
                });
            }
        }

        if !missing_critical && results.is_empty() {
            results.push(IntegrityCheckResult {
                check_name: "table existence",
                passed: true,
                severity: IntegritySeverity::Info,
                message: format!("All {} expected tables present", EXPECTED_TABLES.len()),
                recovery: None,
            });
        }

        results
    }

    fn check_orphaned_caps(&self) -> Vec<IntegrityCheckResult> {
        let mut results = Vec::new();
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return results,
        };

        // Check: held_capabilities without matching proof
        let orphaned_caps: i64 = conn.query_row(
            "SELECT COUNT(*) FROM held_capabilities \
             WHERE cap_id NOT IN (SELECT cap_id FROM capability_proofs)",
            [],
            |row| row.get(0),
        ).unwrap_or(-1);

        if orphaned_caps < 0 {
            results.push(IntegrityCheckResult {
                check_name: "orphaned capabilities",
                passed: false,
                severity: IntegritySeverity::Error,
                message: "Could not query orphaned capabilities".into(),
                recovery: Some("The database may be corrupt.".into()),
            });
        } else if orphaned_caps > 0 {
            results.push(IntegrityCheckResult {
                check_name: "orphaned capabilities",
                passed: false,
                severity: IntegritySeverity::Error,
                message: format!("{orphaned_caps} held_capabilities row(s) have no matching capability_proofs row"),
                recovery: Some(
                    "These coins cannot be spent (proofs missing). Run 'wallet reset' to \
                     clear state, then rescan the chain. The coins will be re-discovered \
                     with fresh proofs.".into(),
                ),
            });
        } else {
            results.push(IntegrityCheckResult {
                check_name: "orphaned capabilities",
                passed: true,
                severity: IntegritySeverity::Info,
                message: "All held_capabilities have matching proofs".into(),
                recovery: None,
            });
        }

        // Check reverse: proofs without matching capability
        let orphaned_proofs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM capability_proofs \
             WHERE cap_id NOT IN (SELECT cap_id FROM held_capabilities)",
            [],
            |row| row.get(0),
        ).unwrap_or(-1);

        if orphaned_proofs > 0 {
            results.push(IntegrityCheckResult {
                check_name: "orphaned proofs",
                passed: false,
                severity: IntegritySeverity::Warn,
                message: format!("{orphaned_proofs} capability_proofs row(s) have no matching held_capability — wasted space"),
                recovery: Some("Run 'wallet reset' to clear state, then rescan. Harmless but indicates a past crash during scan.".into()),
            });
        }

        results
    }

    fn check_height_consistency(&self) -> Vec<IntegrityCheckResult> {
        let mut results = Vec::new();
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return results,
        };

        let max_scanned: Option<i64> = conn.query_row(
            "SELECT MAX(height) FROM scanned_blocks", [], |row| row.get(0),
        ).ok().flatten();

        let max_cap_height: Option<i64> = conn.query_row(
            "SELECT MAX(created_at_height) FROM held_capabilities", [], |row| row.get(0),
        ).ok().flatten();

        match (max_scanned, max_cap_height) {
            (None, None) => {
                // Empty wallet — no blocks scanned, no caps
                results.push(IntegrityCheckResult {
                    check_name: "height consistency",
                    passed: true,
                    severity: IntegritySeverity::Info,
                    message: "Empty wallet (no scanned blocks, no capabilities)".into(),
                    recovery: None,
                });
            }
            (Some(scanned), Some(cap)) => {
                if cap > scanned + 1 {
                    results.push(IntegrityCheckResult {
                        check_name: "height consistency",
                        passed: false,
                        severity: IntegritySeverity::Error,
                        message: format!(
                            "Capability created at height {cap} but max scanned height is {scanned} — \
                             impossible without corruption (cap cannot be created before its block is scanned)"
                        ),
                        recovery: Some(
                            "Run 'wallet reset' to clear state, then rescan the chain.".into(),
                        ),
                    });
                } else if scanned > cap + 10 {
                    results.push(IntegrityCheckResult {
                        check_name: "height consistency",
                        passed: true, // not an error — just unusual
                        severity: IntegritySeverity::Warn,
                        message: format!(
                            "Scanned {scanned} blocks but highest capability is at height {cap}. \
                             Large gap may indicate the wallet key does not match any on-chain coins."
                        ),
                        recovery: Some(
                            "Verify the wallet key in keys.toml matches the expected identity.".into(),
                        ),
                    });
                } else {
                    results.push(IntegrityCheckResult {
                        check_name: "height consistency",
                        passed: true,
                        severity: IntegritySeverity::Info,
                        message: format!("Scanned through height {scanned}, highest cap at {cap}"),
                        recovery: None,
                    });
                }
            }
            (Some(scanned), None) => {
                results.push(IntegrityCheckResult {
                    check_name: "height consistency",
                    passed: true,
                    severity: IntegritySeverity::Warn,
                    message: format!("Scanned {scanned} blocks but found zero capabilities"),
                    recovery: Some(
                        "Verify the wallet key in keys.toml matches the expected identity.".into(),
                    ),
                });
            }
            (None, Some(_)) => {
                results.push(IntegrityCheckResult {
                    check_name: "height consistency",
                    passed: false,
                    severity: IntegritySeverity::Error,
                    message: "Capabilities exist but no blocks have been scanned — database corruption".into(),
                    recovery: Some("Run 'wallet reset' to clear state, then rescan.".into()),
                });
            }
        }

        results
    }

    fn check_critical_column_nulls(&self) -> Vec<IntegrityCheckResult> {
        let mut results = Vec::new();
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return results,
        };

        let null_caps: i64 = conn.query_row(
            "SELECT COUNT(*) FROM held_capabilities \
             WHERE cap_id IS NULL OR asset_id IS NULL \
                OR cap_blind IS NULL OR value_blind IS NULL OR asset_blind IS NULL",
            [],
            |row| row.get(0),
        ).unwrap_or(-1);

        if null_caps < 0 {
            return results; // query failed, skip
        }
        if null_caps > 0 {
            results.push(IntegrityCheckResult {
                check_name: "critical column nulls",
                passed: false,
                severity: IntegritySeverity::Error,
                message: format!("{null_caps} held_capabilities row(s) have NULL in a critical column"),
                recovery: Some(
                    "These capabilities are corrupt and cannot be spent. Run 'wallet reset' \
                     to clear state, then rescan the chain.".into(),
                ),
            });
        }

        let null_proofs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM capability_proofs \
             WHERE cap_id IS NULL OR merkle_proof IS NULL OR merkle_root IS NULL",
            [],
            |row| row.get(0),
        ).unwrap_or(-1);

        if null_proofs > 0 {
            results.push(IntegrityCheckResult {
                check_name: "critical column nulls",
                passed: false,
                severity: IntegritySeverity::Error,
                message: format!("{null_proofs} capability_proofs row(s) have NULL in a critical column"),
                recovery: Some("Run 'wallet reset' to clear state, then rescan.".into()),
            });
        }

        if null_caps == 0 && null_proofs == 0 {
            results.push(IntegrityCheckResult {
                check_name: "critical column nulls",
                passed: true,
                severity: IntegritySeverity::Info,
                message: "No NULL values in critical columns".into(),
                recovery: None,
            });
        }

        results
    }

    fn check_block_hash_validity(&self) -> Vec<IntegrityCheckResult> {
        let mut results = Vec::new();
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return results,
        };

        // Verify chain_blocks JSON is valid by attempting deserialization
        // on the highest block (quick spot-check)
        let result: Result<String, _> = conn.query_row(
            "SELECT block_json FROM chain_blocks ORDER BY height DESC LIMIT 1",
            [],
            |row| row.get(0),
        );

        match result {
            Ok(json) => {
                match serde_json::from_str::<serde_json::Value>(&json) {
                    Ok(v) => {
                        // Verify it has the expected block structure
                        if v.get("header").and_then(|h| h.get("height")).is_some() {
                            results.push(IntegrityCheckResult {
                                check_name: "block hash validity",
                                passed: true,
                                severity: IntegritySeverity::Info,
                                message: "Latest chain block JSON is valid".into(),
                                recovery: None,
                            });
                        } else {
                            results.push(IntegrityCheckResult {
                                check_name: "block hash validity",
                                passed: false,
                                severity: IntegritySeverity::Error,
                                message: "Latest chain block JSON is valid JSON but missing 'header.height' field".into(),
                                recovery: Some("The block data may be corrupt. Delete and re-sync the chain.".into()),
                            });
                        }
                    }
                    Err(e) => {
                        results.push(IntegrityCheckResult {
                            check_name: "block hash validity",
                            passed: false,
                            severity: IntegritySeverity::Error,
                            message: format!("Latest chain block JSON failed to parse: {e}"),
                            recovery: Some("Delete the chain_blocks row and re-sync from peers.".into()),
                        });
                    }
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // No blocks — fine, skip
            }
            Err(_) => {
                results.push(IntegrityCheckResult {
                    check_name: "block hash validity",
                    passed: false,
                    severity: IntegritySeverity::Error,
                    message: "Could not query chain_blocks table".into(),
                    recovery: Some("The database may be corrupt.".into()),
                });
            }
        }

        // Spot-check: verify every 100th scanned block hash is valid bs58
        let hash_check = conn.prepare(
            "SELECT height, hash FROM scanned_blocks WHERE height % 100 = 0 ORDER BY height"
        );
        if let Ok(mut stmt) = hash_check {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            }) {
                let mut invalid = 0u32;
                for row in rows.flatten() {
                    let (_height, hash) = row;
                    if hash == "-" { continue; } // genesis sentinel
                    if bs58::decode(&hash).into_vec().map(|v| v.len() == 32).unwrap_or(false) {
                        continue;
                    }
                    invalid += 1;
                }
                if invalid > 0 {
                    results.push(IntegrityCheckResult {
                        check_name: "block hash validity",
                        passed: false,
                        severity: IntegritySeverity::Error,
                        message: format!("{invalid} scanned_blocks row(s) have invalid (non-bs58-32-byte) hash values"),
                        recovery: Some("Delete the affected scanned_blocks rows and re-scan from those heights.".into()),
                    });
                }
            }
        }

        if results.is_empty() {
            results.push(IntegrityCheckResult {
                check_name: "block hash validity",
                passed: true,
                severity: IntegritySeverity::Info,
                message: "Block data passes spot-checks".into(),
                recovery: None,
            });
        }

        results
    }
}
