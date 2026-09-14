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

//! Tripwire tests — grep-level guardrails that catch regressions
//! mechanically. These are NOT unit or integration tests in the
//! traditional sense; they are invariant checks that assert the
//! codebase itself conforms to architectural rules.
//!
//! Layer: Gating (run before Layer 4 integration tests)
//! Gate: `cargo test -p dwowd -- tripwire_`
//!
//! Both tripwires were silently vacuous until this conversion. Each derived its subject
//! from `env!("CARGO_MANIFEST_DIR")` — which is `bin/dwowd` — and then joined a path
//! written relative to the *workspace* root, so both directories were absent, both tests
//! took the "not running from workspace root" early return, and both reported `ok`
//! without opening a single file. The paths now resolve against the workspace root, and
//! a subject that cannot be found is a failure rather than a skip: a guardrail that
//! cannot see what it guards must not report success.

use std::path::Path;

use dwow_sdk::test_support::{TestError, TestResult};

/// This module's name in an INFRA-FAIL attribution.
const MODULE: &str = "tests::tripwire";

/// INFRA-FAIL: a tripwire could not run its scan, or found a violation.
///
/// A violation is not "the contract under test failed" — it is a shared architectural
/// invariant of the source tree being broken, which the taxonomy classes as INFRA-FAIL
/// ("a shared integrity check failed"). Attributing it here makes the failure name the
/// module and the scan rather than a line inside a loop.
fn infra(stage: &'static str, cause: impl Into<Box<dyn std::error::Error>>) -> TestError {
    TestError::infra(MODULE, stage, cause)
}

/// The workspace root, resolved from this crate's manifest directory (`bin/dwowd`).
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A tripwire that cannot find its subject must fail, not skip. The old
/// `if !dir.exists() { return; }` reported `ok` for a path that never existed.
fn subject_or_fail(stage: &'static str, dir: &Path) -> TestResult<()> {
    if dir.exists() {
        return Ok(());
    }
    Err(infra(
        stage,
        format!(
            "{} does not exist — the tripwire cannot scan, and must not report ok",
            dir.display()
        ),
    ))
}

/// The wallet core must contain no per-contract routing strings beyond the
/// two sanctioned citizens (native_token, deployooor) and the genesis
/// trust-tier ID table. Every other contract enters the wallet exclusively
/// through its stored manifest. This test is a grep-level guardrail —
/// adding a hardcoded contract-name routing string is the "ERC-20 hell"
/// failure mode the fired agent committed.
#[test]
fn tripwire_no_contract_names_in_wallet() -> TestResult<()> {
    let dww_src = workspace_root().join("bin/dww/src");
    subject_or_fail("locating the wallet source tree", &dww_src)?;
    // The two sanctioned citizens (wallet.md §0.1, §6.4) that the wallet
    // MAY name directly. All other contracts enter via stored manifests.
    // The genesis trust-tier ID table (contract_imports.rs) enumerates
    // all 9 genesis contracts' ContractIds — those strings are allowed
    // only there and in the genesis seeding array (lib.rs).
    let route_violation_contracts = [
        "promissory_note",
    ];
    // Allowed files: genesis ID lookup table, genesis seeding array
    let allowed_files = |p: &Path| -> bool {
        p.ends_with("contract_imports.rs") || p.ends_with("lib.rs")
    };
    let entries = std::fs::read_dir(&dww_src)
        .map_err(|e| infra("listing the wallet source tree", e))?;
    for entry in entries {
        let path = entry
            .map_err(|e| infra("reading an entry of the wallet source tree", e))?
            .path();
        if path.extension().map_or(true, |e| e != "rs") { continue; }
        if path.ends_with("contract_metadata.rs") { continue; } // legacy registry, comments only
        if allowed_files(&path) { continue; }
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| infra("reading a wallet source file", e))?;
        // Only scan non-comment, non-test-fixture lines for routing strings.
        for (lineno, line) in contents.lines().enumerate() {
            let stripped = line.trim();
            // Skip comments and TOML fixtures
            if stripped.starts_with("//") || stripped.starts_with("/*")
                || stripped.starts_with('*') || stripped.starts_with("name = ")
            {
                continue;
            }
            for name in &route_violation_contracts {
                let needle = format!("\"{}\"", name);
                if stripped.contains(&needle) {
                    return Err(infra(
                        "scanning the wallet for contract-routing strings",
                        format!(
                            "CONTRACT ROUTING in wallet: {}:{} contains {} — \
                             the wallet must route NO contract beyond the two \
                             sanctioned citizens. Delete this hardcoded string.",
                            path.display(), lineno + 1, needle,
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Every ZK contract metadata function SHALL return empty signature pubkeys.
/// Schnorr signatures are prohibited per contract-standards.md §3.
/// This tripwire catches any re-addition of non-empty signature_pubkeys before
/// it reaches the integration test layer.
#[test]
fn tripwire_no_schnorr_signature_pubkeys() -> TestResult<()> {
    let contracts_dir = workspace_root().join("src/contract");
    subject_or_fail("locating the contract source tree", &contracts_dir)?;
    // Patterns that indicate a non-empty signature_pubkeys initialization
    let violation_patterns = [
        "signature_pubkeys.push(",
        "signature_pubkeys = vec![",
        "sigs = vec![params.",
        "sigs = vec![input.",
        "sigs = vec![pr.",
        "sigs = vec![sp.",
        "sigs = vec![fee_",
        "empty_sigs = vec![",
    ];
    // Allowed: empty vec initialization
    let allowed = "vec![]";

    let entries = std::fs::read_dir(&contracts_dir)
        .map_err(|e| infra("listing src/contract", e))?;
    for entry in entries {
        let contract_dir = entry
            .map_err(|e| infra("reading an entry of src/contract", e))?
            .path();
        if !contract_dir.is_dir() { continue; }
        let entrypoint = contract_dir.join("src/entrypoint/mod.rs");
        if !entrypoint.exists() {
            let alt = contract_dir.join("src/entrypoint.rs");
            if !alt.exists() { continue; }
            let contents = std::fs::read_to_string(&alt)
                .map_err(|e| infra("reading an entrypoint file", e))?;
            check_schnorr_free(&alt, &contents, &violation_patterns, allowed)?;
            continue;
        }
        // Also check sub-entrypoint files (deployooor has deploy_v1.rs, lock_v1.rs)
        let parent = entrypoint.parent().unwrap_or(contract_dir.as_path());
        if let Ok(sub_entries) = std::fs::read_dir(parent) {
            for sub in sub_entries {
                let sub_path = sub
                    .map_err(|e| infra("reading an entrypoint directory entry", e))?
                    .path();
                if sub_path.extension().map_or(true, |e| e != "rs") { continue; }
                let contents = std::fs::read_to_string(&sub_path)
                    .map_err(|e| infra("reading an entrypoint source file", e))?;
                check_schnorr_free(&sub_path, &contents, &violation_patterns, allowed)?;
            }
        }
    }
    Ok(())
}

fn check_schnorr_free(
    path: &Path,
    contents: &str,
    violations: &[&str],
    allowed: &str,
) -> TestResult<()> {
    for (lineno, line) in contents.lines().enumerate() {
        let stripped = line.trim();
        if stripped.starts_with("//") || stripped.starts_with("/*") || stripped.starts_with('*') {
            continue;
        }
        for pattern in violations {
            if stripped.contains(pattern) && !stripped.contains(allowed) {
                return Err(infra(
                    "scanning entrypoints for schnorr signature pubkeys",
                    format!(
                        "SCHNORR LEAK in {}:{} — '{}' found but signature_pubkeys must be \
                         vec![]. See contract-standards.md §3.",
                        path.display(), lineno + 1, pattern.trim(),
                    ),
                ));
            }
        }
    }
    Ok(())
}
