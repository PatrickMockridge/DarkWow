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

//! Layer 2 of the contract trust model: does the deployed WASM match the manifest?
//!
//! `OBL-C237`. A manifest is self-reported by the deployer. The wallet reads it before
//! invoking a contract, so a manifest that claims circuits the binary does not carry — or a
//! binary that carries circuits the manifest does not declare — must be *detected*, not
//! trusted. This module performs the mechanical comparison at scan time, from the `DeployV1`
//! call's own WASM, and returns a verdict to store with the contract's metadata.
//!
//! **What is compared, and what cannot be.** The SDK exports four fixed symbols
//! (`__initialize`, `__entrypoint`, `__update`, `__metadata`, plus the optional
//! `__spend_hook`); contract *functions* are selector arms inside `__entrypoint`, not exports.
//! "Every function is an export" was never checkable and the trust-model page said it until
//! 2026-10-21. What is checkable is: the required exports exist; every circuit the manifest
//! declares has an embedded zkas binary of the same namespace and (when the manifest declares
//! them) the same witness and opcode counts; and no embedded zkas binary belongs to a
//! namespace the manifest does not declare. Namespaces are not unique per circuit — several
//! genesis manifests declare one namespace for all their circuits — so circuits are matched
//! as a multiset of `(namespace, witnesses, opcodes)`, never by position.
//!
//! **What this does not establish.** That the circuits are sound, that the WASM logic is
//! correct, or that the manifest's prose is true. That is Layer 3 (attestation). This is a
//! script-kiddie filter, and it is recorded as one.

use std::collections::BTreeMap;

use dwow_sdk::manifest::ContractManifest;
use wasmparser::{Parser, Payload};

/// The exports every DarkWow contract module provides.
const REQUIRED_EXPORTS: [&str; 4] = ["__initialize", "__entrypoint", "__update", "__metadata"];

/// What the scan found embedded in the deployed WASM, one entry per zkas binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedCircuit {
    pub namespace: String,
    pub witnesses: usize,
    pub opcodes: usize,
}

/// The Layer-2 verdict, stored with the contract's metadata and shown by `contract show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestWasmVerdict {
    /// True when no check failed. Warnings do not make a verdict false.
    pub ok: bool,
    pub checked: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl ManifestWasmVerdict {
    fn new() -> Self {
        Self { ok: true, checked: vec![], warnings: vec![], errors: vec![] }
    }

    fn error(&mut self, message: String) {
        self.ok = false;
        self.errors.push(message);
    }

    /// The compact form persisted in `contract_metadata.manifest_wasm_verdict`.
    pub fn to_stored(&self) -> String {
        if self.ok {
            if self.warnings.is_empty() {
                "ok".to_string()
            } else {
                format!("ok (warnings: {})", self.warnings.join("; "))
            }
        } else {
            format!("mismatch: {}", self.errors.join("; "))
        }
    }

    /// Recover the verdict flag from the stored form. `None` means never checked (a record
    /// written before this check existed, or a contract whose deploy carried no manifest).
    pub fn stored_is_ok(stored: &str) -> Option<bool> {
        if stored.is_empty() {
            None
        } else if stored == "ok" || stored.starts_with("ok (") {
            Some(true)
        } else {
            Some(false)
        }
    }
}

/// Compare a manifest against the WASM it claims to describe.
pub fn verify(
    manifest: &ContractManifest,
    wasm: &[u8],
    embedded: &[EmbeddedCircuit],
) -> ManifestWasmVerdict {
    let mut verdict = ManifestWasmVerdict::new();

    verify_exports(wasm, &mut verdict);
    verify_circuits(manifest, embedded, &mut verdict);
    verify_function_references(manifest, &mut verdict);

    verdict
}

/// The required exports are present and are functions.
fn verify_exports(wasm: &[u8], verdict: &mut ManifestWasmVerdict) {
    let mut functions = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        match payload {
            Ok(Payload::ExportSection(reader)) => {
                for export in reader.into_iter() {
                    let Ok(export) = export else { continue };
                    if export.kind == wasmparser::ExternalKind::Func {
                        functions.push(export.name.to_string());
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                verdict.error(format!("WASM does not parse: {e}"));
                return;
            }
        }
    }
    let missing: Vec<&str> = REQUIRED_EXPORTS
        .iter()
        .copied()
        .filter(|name| !functions.iter().any(|f| f == name))
        .collect();
    if missing.is_empty() {
        verdict.checked.push(format!(
            "exports: {} required function export(s) present",
            REQUIRED_EXPORTS.len()
        ));
    } else {
        verdict.error(format!("WASM is missing required export(s): {}", missing.join(", ")));
    }
}

/// Find every zkas binary embedded in a contract WASM, in byte order.
///
/// This is a magic-byte search plus a **length-aware** decode (`ZkBinary::decode_len`), not a
/// byte-slide that hands the decoder an open-ended slice: the latter fails on the bytes after the
/// first binary, so the scan's `embedded` list and `zkas_binaries` store were empty for every
/// deployed contract and the Layer-2 verdict never measured anything (`OBL-C249`). The returned
/// bytes are the exact binary, so the caller can store what the generic prover will load.
pub fn extract_embedded_circuits(wasm: &[u8]) -> Vec<(EmbeddedCircuit, Vec<u8>)> {
    let magic = dwow_core::zkas::MAGIC_BYTES;
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset + magic.len() <= wasm.len() {
        let Some(rel) = wasm[offset..].windows(magic.len()).position(|w| w == magic) else {
            break;
        };
        let start = offset + rel;
        match dwow_core::zkas::ZkBinary::decode_len(&wasm[start..], false) {
            Ok((zkbin, consumed)) if consumed > magic.len() => {
                let bytes = wasm[start..start + consumed].to_vec();
                out.push((
                    EmbeddedCircuit {
                        namespace: zkbin.namespace,
                        witnesses: zkbin.witnesses.len(),
                        opcodes: zkbin.opcodes.len(),
                    },
                    bytes,
                ));
                offset = start + consumed;
            }
            // A magic-byte lookalike: advance past it and keep searching.
            _ => offset = start + magic.len(),
        }
    }
    out
}

/// Every declared circuit is embedded; no undeclared circuit is embedded.
///
/// Circuits are compared as a multiset of `(namespace, witnesses, opcodes)` because the zkas
/// header carries no circuit name and namespaces repeat across a contract's circuits.
fn verify_circuits(
    manifest: &ContractManifest,
    embedded: &[EmbeddedCircuit],
    verdict: &mut ManifestWasmVerdict,
) {
    // Count embedded by (namespace, witnesses, opcodes) so duplicate namespaces stay distinct.
    // A manifest that does not declare witness_map/opcodes is saying "unknown", not "zero", so
    // those entries match any count.
    let mut remaining: BTreeMap<(&str, usize, usize), usize> = BTreeMap::new();
    for e in embedded {
        *remaining
            .entry((e.namespace.as_str(), e.witnesses, e.opcodes))
            .or_default() += 1;
    }
    let mut missing: Vec<&str> = Vec::new();
    for circuit in &manifest.circuits {
        let want_w = if circuit.witness_map.is_empty() { None } else { Some(circuit.witness_map.len()) };
        let want_o = if circuit.opcodes.is_empty() { None } else { Some(circuit.opcodes.len()) };
        let hit = remaining.iter_mut().find(|((ns, w, o), count)| {
            **count > 0
                && *ns == circuit.namespace.as_str()
                && want_w.map_or(true, |want| want == *w)
                && want_o.map_or(true, |want| want == *o)
        });
        match hit {
            Some((_, count)) => *count -= 1,
            None => {
                missing.push(circuit.name.as_str());
                verdict.error(format!(
                    "manifest circuit '{}' (namespace '{}') is not embedded in the WASM",
                    circuit.name, circuit.namespace
                ));
            }
        }
    }
    if missing.is_empty() {
        verdict.checked.push(format!(
            "circuits: {} declared circuit(s) embedded",
            manifest.circuits.len()
        ));
    }

    // The other direction: nothing undeclared rides along.
    let mut undeclared = Vec::new();
    for e in embedded {
        let declared_here = manifest.circuits.iter().any(|c| c.namespace == e.namespace);
        if !declared_here {
            undeclared.push(e.namespace.clone());
        }
    }
    undeclared.sort();
    undeclared.dedup();
    for ns in undeclared {
        verdict.error(format!("WASM embeds a circuit in undeclared namespace '{ns}'"));
    }
    if verdict.ok {
        verdict.checked.push(format!("circuits: {} embedded zkas binary(ies) declared", embedded.len()));
    }
}

/// A function that names a proof circuit must name a declared one.
fn verify_function_references(manifest: &ContractManifest, verdict: &mut ManifestWasmVerdict) {
    for function in &manifest.functions {
        for circuit in function.circuits() {
            if !manifest.circuits.iter().any(|c| c.name == circuit) {
                verdict.error(format!(
                    "function '{}' names proof circuit '{circuit}', which the manifest does not declare",
                    function.name
                ));
            }
        }
    }

    // The reverse is a warning, not an error: a manifest may declare a circuit used by a
    // function that composes it without naming it, and a circuit may be embedded for a
    // client-side helper.
    for circuit in &manifest.circuits {
        if !manifest.functions.iter().any(|f| f.circuits().contains(&circuit.name.as_str())) {
            verdict.warnings.push(format!(
                "circuit '{}' is not named by any function's proof circuits",
                circuit.name
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::manifest::ManifestCircuit;

    fn manifest(circuits: Vec<(&str, &str, Vec<&str>)>) -> ContractManifest {
        ContractManifest {
            name: "test".into(),
            category: "Test".into(),
            description: "test".into(),
            version: "1.0.0".into(),
            dependencies: vec![],
            functions: vec![],
            capabilities: vec![],
            actions: vec![],
            trees: vec![],
            circuits: circuits
                .into_iter()
                .map(|(name, ns, witness)| ManifestCircuit {
                    name: name.into(),
                    namespace: ns.into(),
                    witness_map: witness.into_iter().map(|s| s.to_string()).collect(),
                    opcodes: vec![],
                    public_inputs: vec![],
                })
                .collect(),
            parameters: vec![],
            cost_profiles: vec![],
        }
    }

    /// The deployed box artifact: a real module with the four exports.
    const BOX_WASM: &[u8] = include_bytes!("../../../src/contract/box/dwow_box_contract.wasm");

    #[test]
    fn a_matching_manifest_and_wasm_pass() {
        let m = manifest(vec![("Put", "Put", vec![]), ("Take", "Take", vec![])]);
        let embedded = vec![
            EmbeddedCircuit { namespace: "Put".into(), witnesses: 22, opcodes: 40 },
            EmbeddedCircuit { namespace: "Take".into(), witnesses: 22, opcodes: 40 },
        ];
        let v = verify(&m, BOX_WASM, &embedded);
        assert!(v.ok, "a consistent manifest and WASM must pass: {:?}", v.errors);
    }

    /// The tampered-manifest control: the binary does not carry the circuit the manifest claims.
    #[test]
    fn a_manifest_claiming_a_missing_circuit_is_a_mismatch() {
        let m = manifest(vec![
            ("Put", "Put", vec![]),
            ("Take", "Take", vec![]),
            ("Drain", "Drain", vec![]),
        ]);
        let embedded = vec![
            EmbeddedCircuit { namespace: "Put".into(), witnesses: 22, opcodes: 40 },
            EmbeddedCircuit { namespace: "Take".into(), witnesses: 22, opcodes: 40 },
        ];
        let v = verify(&m, BOX_WASM, &embedded);
        assert!(!v.ok, "a manifest naming a circuit the WASM lacks must not pass");
        assert!(
            v.errors.iter().any(|e| e.contains("Drain")),
            "the verdict must name the missing circuit: {:?}",
            v.errors
        );
    }

    /// The other direction: an embedded circuit the manifest does not declare is a mismatch.
    #[test]
    fn an_undeclared_embedded_circuit_is_a_mismatch() {
        let m = manifest(vec![("Put", "Put", vec![])]);
        let embedded = vec![
            EmbeddedCircuit { namespace: "Put".into(), witnesses: 22, opcodes: 40 },
            EmbeddedCircuit { namespace: "Backdoor".into(), witnesses: 22, opcodes: 40 },
        ];
        let v = verify(&m, BOX_WASM, &embedded);
        assert!(!v.ok, "an undeclared embedded circuit must not pass");
        assert!(
            v.errors.iter().any(|e| e.contains("Backdoor")),
            "the verdict must name the undeclared namespace: {:?}",
            v.errors
        );
    }

    /// A declared witness count that disagrees with the binary is a mismatch.
    #[test]
    fn a_witness_count_mismatch_is_a_mismatch() {
        let m = manifest(vec![("Put", "Put", vec!["a", "b", "c"])]);
        let embedded = vec![EmbeddedCircuit { namespace: "Put".into(), witnesses: 22, opcodes: 40 }];
        let v = verify(&m, BOX_WASM, &embedded);
        assert!(!v.ok, "a declared witness count that disagrees with the binary must not pass");
    }

    /// A WASM without the required exports is a mismatch even if the circuits agree.
    #[test]
    fn a_wasm_missing_the_required_exports_is_a_mismatch() {
        let m = manifest(vec![]);
        let v = verify(&m, &[0x00, 0x61, 0x73, 0x6d], &[]);
        assert!(!v.ok, "a module with no exports must not pass");
    }

    /// The eight manifest-bearing genesis `(name, wasm, manifest)` pairs, embedded at compile
    /// time. Deployooor is plaintext infrastructure and has no manifest.
    const GENESIS: &[(&str, &[u8], &str)] = &[
        ("native_token", include_bytes!("../../../src/contract/native_token/dwow_native_token_contract.wasm"),
         include_str!("../../../src/contract/native_token/manifest.toml")),
        ("promissory_note", include_bytes!("../../../src/contract/promissory_note/dwow_promissory_note_contract.wasm"),
         include_str!("../../../src/contract/promissory_note/manifest.toml")),
        ("purse", include_bytes!("../../../src/contract/purse/dwow_purse_contract.wasm"),
         include_str!("../../../src/contract/purse/manifest.toml")),
        ("box", include_bytes!("../../../src/contract/box/dwow_box_contract.wasm"),
         include_str!("../../../src/contract/box/manifest.toml")),
        ("identity", include_bytes!("../../../src/contract/identity/dwow_identity_contract.wasm"),
         include_str!("../../../src/contract/identity/manifest.toml")),
        ("attestation", include_bytes!("../../../src/contract/attestation/dwow_attestation_contract.wasm"),
         include_str!("../../../src/contract/attestation/manifest.toml")),
        ("multisig", include_bytes!("../../../src/contract/multisig/dwow_multisig_contract.wasm"),
         include_str!("../../../src/contract/multisig/manifest.toml")),
        ("oracle", include_bytes!("../../../src/contract/oracle/dwow_oracle_contract.wasm"),
         include_str!("../../../src/contract/oracle/manifest.toml")),
    ];

    /// `OBL-C249` exit (a): all eight embedded genesis manifests pass Layer 2 against their own WASM, and
    /// each manifest's namespace equals its circuit name — the property the wallet's
    /// `(namespace, circuit)` lookup depends on.
    #[test]
    fn every_genesis_manifest_names_its_embedded_circuits() {
        for (name, wasm, manifest_toml) in GENESIS {
            let m = ContractManifest::from_toml(manifest_toml)
                .unwrap_or_else(|e| panic!("{name}: manifest parse: {e}"));
            let extracted = extract_embedded_circuits(wasm);
            assert!(!extracted.is_empty(), "{name}: no embedded zkas binary found in the WASM");
            let embedded: Vec<EmbeddedCircuit> =
                extracted.iter().map(|(c, _)| c.clone()).collect();
            let v = verify(&m, wasm, &embedded);
            assert!(v.ok, "{name}: Layer-2 verdict failed: {:?}", v.errors);
            for circuit in &m.circuits {
                assert_eq!(
                    circuit.namespace, circuit.name,
                    "{name}: manifest namespace '{}' is not the circuit name '{}'",
                    circuit.namespace, circuit.name
                );
                assert!(
                    embedded.iter().any(|e| e.namespace == circuit.namespace),
                    "{name}: declared circuit '{}' is not embedded",
                    circuit.name
                );
            }
        }
    }

    /// `OBL-C249` exit (b): each manifest circuit's own `proof/*.zk.bin` decodes, and its
    /// namespace is the manifest's. This reads the source binaries, not the WASM copy.
    #[test]
    fn every_genesis_circuit_binary_carries_its_manifest_namespace() {
        for (name, _wasm, manifest_toml) in GENESIS {
            let m = ContractManifest::from_toml(manifest_toml)
                .unwrap_or_else(|e| panic!("{name}: manifest parse: {e}"));
            let proof_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../src/contract")
                .join(name)
                .join("proof");
            let mut found = std::collections::BTreeSet::new();
            for entry in std::fs::read_dir(&proof_dir)
                .unwrap_or_else(|e| panic!("{name}: reading {}: {e}", proof_dir.display()))
            {
                let path = entry.expect("dir entry").path();
                if path.extension().and_then(|e| e.to_str()) != Some("bin") {
                    continue;
                }
                let bytes = std::fs::read(&path).expect("read zk.bin");
                let zk = dwow_core::zkas::ZkBinary::decode(&bytes, false)
                    .unwrap_or_else(|e| panic!("{name}: decoding {}: {e}", path.display()));
                found.insert(zk.namespace);
            }
            assert!(!found.is_empty(), "{name}: no .zk.bin in {}", proof_dir.display());
            for circuit in &m.circuits {
                assert!(
                    found.contains(&circuit.namespace),
                    "{name}: {} carries no .zk.bin with namespace '{}'",
                    circuit.name, circuit.namespace
                );
            }
        }
    }

    /// `OBL-C249` exit (c): the generic prover's load path — the key is
    /// `(contract, manifest namespace, circuit name)`. Storing the embedded binaries under the
    /// namespace they carry and looking them up by the manifest must succeed for all eight.
    #[test]
    fn every_genesis_circuit_loads_by_its_manifest_namespace() {
        let db = std::env::temp_dir().join(format!("dwow-c249-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        let wallet = crate::walletdb::WalletDb::new(Some(db.clone()), None, false)
            .expect("open a wallet db");
        // `WalletDb::new` opens the connection; the schema is applied by the caller (as `Dww::init`
        // does). Without it the `zkas_binaries` table does not exist and the store fails.
        wallet
            .exec_batch_sql(include_str!("../wallet.sql"))
            .expect("apply the wallet schema");
        for (name, wasm, manifest_toml) in GENESIS {
            let cid = crate::contract_imports::get_contract_id(name)
                .unwrap_or_else(|| panic!("{name}: no well-known contract id"));
            let cid_str = bs58::encode(cid.to_bytes()).into_string();
            let m = ContractManifest::from_toml(manifest_toml)
                .unwrap_or_else(|e| panic!("{name}: manifest parse: {e}"));
            for (circuit, bytes) in extract_embedded_circuits(wasm) {
                wallet
                    .store_zkas_binary(&cid_str, &circuit.namespace, &circuit.namespace, &bytes)
                    .unwrap_or_else(|e| panic!("{name}: storing '{}': {e}", circuit.namespace));
            }
            for circuit in &m.circuits {
                assert!(
                    wallet
                        .load_zkas_binary(&cid_str, &circuit.namespace, &circuit.name)
                        .unwrap_or_else(|e| panic!("{name}: loading '{}': {e}", circuit.name))
                        .is_some(),
                    "{name}: prover lookup ({}, {}) misses",
                    circuit.namespace, circuit.name
                );
            }
        }
        drop(wallet);
        let _ = std::fs::remove_file(&db);
    }

    /// `OBL-C249` control: flipping one namespace makes the verdict fail and names both sides.
    #[test]
    fn a_flipped_namespace_is_refused_and_named() {
        let mut m = ContractManifest::from_toml(include_str!(
            "../../../src/contract/box/manifest.toml"
        ))
        .expect("box manifest");
        let wasm = include_bytes!("../../../src/contract/box/dwow_box_contract.wasm");
        let embedded: Vec<EmbeddedCircuit> = extract_embedded_circuits(wasm)
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        m.circuits[0].namespace = "WrongNamespace".into();
        let v = verify(&m, wasm, &embedded);
        assert!(!v.ok, "a flipped namespace must not pass");
        assert!(
            v.errors.iter().any(|e| e.contains("WrongNamespace")),
            "the verdict must name the claimed namespace: {:?}",
            v.errors
        );
        assert!(
            v.errors.iter().any(|e| e.contains("undeclared namespace") && e.contains("Put")),
            "the verdict must name the binary's real namespace: {:?}",
            v.errors
        );
    }
}
