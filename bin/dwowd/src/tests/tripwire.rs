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

/// The wallet core must contain no per-contract routing strings beyond the
/// two sanctioned citizens (native_token, deployooor) and the genesis
/// trust-tier ID table. Every other contract enters the wallet exclusively
/// through its stored manifest. This test is a grep-level guardrail —
/// adding a hardcoded contract-name routing string is the "ERC-20 hell"
/// failure mode the fired agent committed.
#[test]
fn tripwire_no_contract_names_in_wallet() {
    use std::path::Path;
    let dww_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("bin/dww/src");
    if !dww_src.exists() {
        return; // not running from workspace root; skip
    }
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
    for entry in std::fs::read_dir(&dww_src).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(true, |e| e != "rs") { continue; }
        if path.ends_with("contract_metadata.rs") { continue; } // legacy registry, comments only
        if allowed_files(&path) { continue; }
        let contents = std::fs::read_to_string(&path).unwrap();
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
                    panic!(
                        "CONTRACT ROUTING in wallet: {}:{} contains {} — \
                         the wallet must route NO contract beyond the two \
                         sanctioned citizens. Delete this hardcoded string.",
                        path.display(), lineno + 1, needle,
                    );
                }
            }
        }
    }
}
