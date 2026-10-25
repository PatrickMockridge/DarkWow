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

//! OBL-C260's compensating control: the generic manifest client refuses a multi-proof arm by
//! name instead of returning a call built with one proof. The multi-proof engine is a feature
//! (residual R-08); the failure mode this prevents is a silently wrong single-proof call.

use dwow_sdk::contract_client::{ManifestContractClient, WalletStateProvider};
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::manifest::ContractManifest;
use dwow_sdk::pasta::pallas;

struct DummyWallet;

impl WalletStateProvider for DummyWallet {
    fn default_address(&self) -> Result<String, String> {
        Ok("dummy".to_string())
    }
}

#[test]
fn multi_proof_arm_is_refused_by_name() {
    let toml = r#"
[contract]
name = "multi"
category = "Test"
description = "multi-proof refusal fixture"
version = "1.0.0"

[[functions]]
name = "multi"
code = 1
requires_proof = true
proof_circuits = ["CircuitA", "CircuitB"]
"#;
    let manifest = ContractManifest::from_toml(toml).expect("the fixture manifest parses");
    let recipient = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(7u64)));
    let client = ManifestContractClient::new(
        "multi",
        manifest,
        "contract-id".to_string(),
        [0u8; 32],
        recipient,
    );
    let err = match client.prepare("multi", "{}", &DummyWallet) {
        Ok(_) => panic!("a multi-proof arm must be refused, not built with one proof"),
        Err(err) => err,
    };
    assert!(
        err.contains("multi-proof") && err.contains("OBL-C260"),
        "the refusal must name the class and the row: {err}"
    );
}
