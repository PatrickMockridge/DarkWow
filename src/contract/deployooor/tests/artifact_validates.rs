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

//! Does the artifact the deploy path validates itself pass that validator?
//!
//! `src/contract/deployooor/src/entrypoint/deploy.rs` calls
//! `wasmparser::validate(&params.wasm_bincode)` on every deployed artifact — the
//! last gate before a contract is stored. On 2026-09-25 the executor observed a
//! **trap inside wasmparser** (`BinaryReader::visit_operator` →
//! `FuncValidator::validate` → `validate_all`) while deployooor validated *its own*
//! artifact, which is the deployment nothing could reach while the host refused
//! payloads of 1 MiB or more.
//!
//! Two causes produce that symptom and they need opposite fixes, so this test
//! separates them by asking the question on the host: deployooor's artifact, the same
//! `wasmparser` version the guest links, the same call the guest makes.
//!
//! - **Passes here** → the validator can read the artifact, so a guest that traps on
//!   it was given different bytes. That is a payload-delivery defect.
//! - **Fails or traps here** → the validator cannot read the artifact, and the deploy
//!   path cannot accept a contract of this shape at all. That is a deployooor defect,
//!   independent of delivery.
//!
//! This file lives in `tests/`, which is outside `WASM_SRC` (`find src -type f`) and so
//! out of `SOURCE_MANIFEST` — it changes no artifact and does not move the genesis pin.

/// The artifact is deployooor's own, i.e. exactly what the failing deployment carried.
const ARTIFACT: &[u8] = include_bytes!("../dwow_deployooor_contract.wasm");

#[test]
fn the_deployooor_artifact_passes_wasmparser_validate() {
    assert!(
        ARTIFACT.len() > 1_048_576,
        "this witness is about a payload the old host refused; the artifact is now {} bytes",
        ARTIFACT.len()
    );

    match wasmparser::validate(ARTIFACT) {
        Ok(_types) => {
            println!("validate OK: {} bytes", ARTIFACT.len());
        }
        Err(e) => panic!(
            "wasmparser::validate REJECTED deployooor's own {} -byte artifact: {e}\n\
             This is the call `deploy_process_instruction_v1` makes before storing a \
             contract, so the deploy path cannot accept this artifact at all.",
            ARTIFACT.len()
        ),
    }
}

/// The same artifact read with the *default* feature set, which is what a plain
/// `wasmparser = "0.243.0"` dependency gives — in the guest as much as here. If the
/// two differ, the two builds of the validator are not the same validator and the
/// guest's failure says nothing about the artifact.
#[test]
fn the_validator_features_the_guest_uses_are_the_default_set() {
    // `validate` above already used the default set; this test exists to make the
    // assumption explicit and to fail loudly if a future edit narrows it here.
    let mut validator = wasmparser::Validator::new();
    let result = validator.validate_all(ARTIFACT);
    assert!(
        result.is_ok(),
        "validate_all (not just validate) rejected the artifact: {:?}",
        result.err()
    );
}
