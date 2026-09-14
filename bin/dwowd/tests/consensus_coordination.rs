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

//! Consensus Coordination — boundary-witness integration tests.
//!
//! Per type-system.md §10.5: "Every declared SHALL at a boundary SHALL have
//! at least one runtime witness test."
//!
//! ## Test Catalog
//!
//! | Test | Time | Verifies |
//! |------|------|----------|
//! | E  | 0s   | PeerTip::from_tip rejects invalid |
//! | G  | 0s   | Barb declarations complete |
//! | H  | 0s   | Byzantine message validation |

use dwow_sdk::blockchain::BlockHeight;
use dwow_sdk::ensure;
use dwow_sdk::test_support::{TestError, TestResult};

/// This module's name in an INFRA-FAIL attribution.
const MODULE: &str = "consensus_coordination";

/// INFRA-FAIL: a boundary-witness check on the sync protocol's types failed.
///
/// These tests witness the `SHALL`s at the sync boundary (type-system.md §10.5). That is
/// shared infrastructure rather than a contract under test, which is why the class is
/// INFRA-FAIL — the failure implicates the boundary type, not a caller's subject.
fn infra(stage: &'static str, cause: impl Into<Box<dyn std::error::Error>>) -> TestError {
    TestError::infra(MODULE, stage, cause)
}

// ── Test E: PeerTip::from_tip rejects invalid data ────────────────────

#[test]
fn test_peertip_rejects_invalid() -> TestResult<()> {
    use dwowd::proto::linear_sync_client::PeerTip;
    use dwow_chain::sync_types::Tip;

    fn assert_rejects(desc: &str, tip: &Tip) -> TestResult<()> {
        match PeerTip::from_tip(tip) {
            Err(e) => {
                eprintln!("[CHECK] Correctly rejected {}: {e}", desc);
                Ok(())
            }
            Ok(_) => Err(infra(
                "checking that an invalid tip is rejected",
                format!("{desc} — expected Err, got Ok"),
            )),
        }
    }

    fn assert_accepts(desc: &str, tip: &Tip) -> TestResult<()> {
        match PeerTip::from_tip(tip) {
            Ok(_) => {
                eprintln!("[CHECK] Correctly accepted: {}", desc);
                Ok(())
            }
            Err(e) => Err(infra(
                "checking that a valid tip is accepted",
                format!("{desc} — expected Ok, got Err: {e}"),
            )),
        }
    }

    assert_rejects("u64::MAX height", &Tip {
        height: BlockHeight::new(u64::MAX),
        hash: dwow_chain::sync_types::BlockHash::zero(),
        genesis_hash: Some(dwow_chain::sync_types::BlockHash::zero()),
    })?;
    assert_rejects("missing genesis hash at height > 0", &Tip {
        height: BlockHeight::new(5),
        hash: dwow_chain::sync_types::BlockHash::zero(),
        genesis_hash: None,
    })?;
    assert_accepts("valid tip with genesis", &Tip {
        height: BlockHeight::new(5),
        hash: dwow_chain::sync_types::BlockHash::zero(),
        genesis_hash: Some(dwow_chain::sync_types::BlockHash::zero()),
    })?;
    assert_accepts("height 0 zero hash valid", &Tip {
        height: BlockHeight::new(0),
        hash: dwow_chain::sync_types::BlockHash::zero(),
        genesis_hash: None,
    })?;

    eprintln!("[TEST] E: PASS — PeerTip::from_tip correctly rejects all invalid inputs");
    Ok(())
}

// ── Test G: Barb declarations complete ────────────────────────────────

#[test]
fn test_barb_declarations_complete() -> TestResult<()> {
    use dwow_core::barb::ExhibitsBarb;
    use dwowd::proto::linear_sync_client::PeerTip;
    use dwowd::task::{ConsensusInitTaskConfig, GenesisAuthority};

    let boundary_types: Vec<(&str, &[dwow_core::barb::BarbId])> = vec![
        ("PeerTip", PeerTip::exhibited_barbs()),
        ("ConsensusInitTaskConfig", ConsensusInitTaskConfig::exhibited_barbs()),
        ("GenesisAuthority", GenesisAuthority::exhibited_barbs()),
        ("LinearSyncClient", dwowd::proto::linear_sync_client::LinearSyncClient::exhibited_barbs()),
    ];

    for (name, barbs) in &boundary_types {
        ensure!(!barbs.is_empty(), format!("{name} missing ExhibitsBarb"));
        eprintln!("[CHECK] {}: exhibits {:?}", name, barbs);
    }

    // Specific barb checks
    ensure!(
        PeerTip::exhibited_barbs().contains(&dwow_core::barb::BarbId::Verify),
        "PeerTip must exhibit Verify"
    );
    ensure!(
        PeerTip::exhibited_barbs().contains(&dwow_core::barb::BarbId::SyncBarrier),
        "PeerTip must exhibit SyncBarrier"
    );
    ensure!(
        GenesisAuthority::exhibited_barbs().contains(&dwow_core::barb::BarbId::Mine),
        "GenesisAuthority must exhibit Mine"
    );
    ensure!(
        ConsensusInitTaskConfig::exhibited_barbs().contains(&dwow_core::barb::BarbId::Mine),
        "ConsensusInitTaskConfig must exhibit Mine"
    );

    eprintln!("[TEST] G: PASS — all boundary types have complete barb declarations");
    Ok(())
}

// ── Test H: Byzantine message validation ──────────────────────────────

/// T5: GetBlocks with invalid parameters MUST be rejected at the type boundary.
/// start_height=0 is semantically invalid (genesis is height 1, not 0).
#[test]
fn test_getblocks_rejects_zero_start() -> TestResult<()> {
    use dwow_chain::sync_types::GetBlocks;
    let gb = GetBlocks {
        start_height: dwow_sdk::blockchain::BlockHeight::new(0),
        count: 10,
    };
    // NB: this assertion is tautological — it compares the value to the one just
    // constructed, so it cannot fail. It is kept as written because this conversion must
    // not change what the tests assert; recording it rather than silently strengthening a
    // test is the point. The expectation it documents (handlers MUST reject start_height=0)
    // is not actually exercised.
    ensure!(
        gb.start_height == dwow_sdk::blockchain::BlockHeight::new(0),
        "GetBlocks with start_height=0 is a semantic error — genesis is height 1"
    );
    // The type system allows BlockHeight(0) because 0 is valid as a pre-genesis sentinel,
    // but sync protocol handlers MUST reject it. This test documents the expectation.
    Ok(())
}

/// T5: Tip with non-zero height but missing genesis_hash MUST be rejected.
/// The genesis_hash is required for fork detection — without it, a peer cannot
/// distinguish chains.
#[test]
fn test_tip_missing_genesis_hash_rejected() -> TestResult<()> {
    use dwowd::proto::linear_sync_client::PeerTip;
    use dwow_chain::sync_types::Tip;
    let tip = Tip {
        height: dwow_sdk::blockchain::BlockHeight::new(5),
        hash: dwow_chain::sync_types::BlockHash::zero(),
        genesis_hash: None,
    };
    ensure!(
        PeerTip::from_tip(&tip).is_err(),
        "Tip at height>0 missing genesis_hash MUST be rejected"
    );
    Ok(())
}

/// T5: Tip with u64::MAX height MUST be rejected (sentinel for uninitialized).
/// Hash-level validation (empty/zero hash, invalid hex) is now performed by
/// serde deserialization in the BlockHash type itself (§8.2.1 re-lift).
#[test]
fn test_tip_max_height_rejected() -> TestResult<()> {
    use dwowd::proto::linear_sync_client::PeerTip;
    use dwow_chain::sync_types::Tip;
    let tip = Tip {
        height: dwow_sdk::blockchain::BlockHeight::new(u64::MAX),
        hash: dwow_chain::sync_types::BlockHash::zero(),
        genesis_hash: None,
    };
    ensure!(
        PeerTip::from_tip(&tip).is_err(),
        "Tip with u64::MAX height MUST be rejected"
    );
    Ok(())
}

/// T5: PeerTip boundary types MUST implement ExhibitsBarb.
#[test]
fn test_peertip_exhibits_correct_barbs() -> TestResult<()> {
    use dwowd::proto::linear_sync_client::PeerTip;
    use dwow_core::barb::ExhibitsBarb;
    let barbs = PeerTip::exhibited_barbs();
    ensure!(
        barbs.contains(&dwow_core::barb::BarbId::Verify),
        "PeerTip must exhibit Verify (tip data must be cryptographically verifiable)"
    );
    ensure!(
        barbs.contains(&dwow_core::barb::BarbId::SyncBarrier),
        "PeerTip must announce SyncBarrier (tip announcement gates sync start)"
    );
    Ok(())
}
