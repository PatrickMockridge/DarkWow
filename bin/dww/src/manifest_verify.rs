/// WASM manifest verification — mechanical, zero-trust.
///
/// This module answers one question: does the manifest match the binary?
/// It parses WASM exports and ZK circuit data sections, then cross-references
/// against the manifest's declarations. This is objective string comparison —
/// no trust required.
///
/// What it does NOT check (attestation concerns, social verification):
/// - Whether the WASM logic is correct
/// - Whether the ZK circuits are sound
/// - Whether the capability model makes sense
///
/// Separation of concerns:
///   Trust Tier  → Who deployed this? (social)
///   WASM Verify → Does the manifest match the binary? (mechanical)
///   Attestation → Does the binary do what it claims? (social)

use std::collections::HashSet;

use dwow_sdk::manifest::ContractManifest;
use wasmparser::{
    ExternalKind::{Func, Memory},
    Payload::{ExportSection, DataSection},
};

/// Extracted WASM export information.
#[derive(Debug)]
pub struct WasmExports {
    pub functions: Vec<String>,
    pub has_memory: bool,
    pub has_initialize: bool,
    pub has_entrypoint: bool,
    pub has_update: bool,
    pub has_metadata: bool,
}

/// Extracted ZK circuit metadata from WASM data sections.
#[derive(Debug)]
pub struct CircuitMeta {
    pub name: String,
    pub namespace: String,
}

/// Result of manifest-vs-WASM verification.
#[derive(Debug)]
pub struct VerificationResult {
    pub passed: bool,
    pub manifest_functions: usize,
    pub wasm_functions: usize,
    pub missing_exports: Vec<String>,
    pub extra_exports: Vec<String>,
    pub manifest_circuits: usize,
    pub wasm_circuits: usize,
    pub missing_circuits: Vec<String>,
    pub circuit_mismatches: Vec<String>,
}

impl VerificationResult {
    /// Human-readable summary for CLI output.
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        // Functions
        if self.missing_exports.is_empty() {
            lines.push(format!(
                "  Functions: PASSED ({} declared, {} in WASM)",
                self.manifest_functions, self.wasm_functions
            ));
        } else {
            lines.push(format!(
                "  Functions: FAILED ({} declared, {} in WASM)",
                self.manifest_functions, self.wasm_functions
            ));
            if !self.missing_exports.is_empty() {
                lines.push(format!(
                    "    Missing from WASM: {}",
                    self.missing_exports.join(", ")
                ));
            }
            if !self.extra_exports.is_empty() {
                lines.push(format!(
                    "    Extra in WASM: {}",
                    self.extra_exports.join(", ")
                ));
            }
        }

        // Circuits
        if self.missing_circuits.is_empty() && self.circuit_mismatches.is_empty() {
            lines.push(format!(
                "  Circuits: PASSED ({} declared, {} in WASM)",
                self.manifest_circuits, self.wasm_circuits
            ));
        } else {
            lines.push("  Circuits: FAILED".to_string());
            if !self.missing_circuits.is_empty() {
                lines.push(format!(
                    "    Missing from WASM: {}",
                    self.missing_circuits.join(", ")
                ));
            }
            for m in &self.circuit_mismatches {
                lines.push(format!("    Mismatch: {m}"));
            }
        }

        // Overall
        lines.push(format!(
            "  Summary: {} — manifest {} WASM",
            if self.passed { "PASSED" } else { "FAILED" },
            if self.passed { "matches" } else { "does not match" }
        ));

        lines.join("\n")
    }
}

/// Extract WASM exports from a binary.
pub fn extract_wasm_exports(wasm_bincode: &[u8]) -> Result<WasmExports, String> {
    let mut functions = Vec::new();
    let mut has_memory = false;
    let mut has_initialize = false;
    let mut has_entrypoint = false;
    let mut has_update = false;
    let mut has_metadata = false;

    let parser = wasmparser::Parser::new(0);
    for payload in parser.parse_all(wasm_bincode) {
        let payload = payload.map_err(|e| format!("WASM parse error: {e}"))?;
        if let ExportSection(reader) = payload {
            for export in reader.into_iter_with_offsets() {
                let (_, export) = export.map_err(|e| format!("Export read error: {e}"))?;
                match export.name {
                    "memory" if export.kind == Memory => has_memory = true,
                    "__initialize" if export.kind == Func => has_initialize = true,
                    "__entrypoint" if export.kind == Func => has_entrypoint = true,
                    "__update" if export.kind == Func => has_update = true,
                    "__metadata" if export.kind == Func => has_metadata = true,
                    name if export.kind == Func => functions.push(name.to_string()),
                    _ => {}
                }
            }
        }
    }

    Ok(WasmExports {
        functions,
        has_memory,
        has_initialize,
        has_entrypoint,
        has_update,
        has_metadata,
    })
}

/// Extract ZK circuit metadata from WASM data sections.
///
/// ZK circuits are embedded as WASM data segments. The `.zk.bin` format
/// has a detectable header. We scan all data sections for circuit metadata.
/// For now, this is a heuristic — it looks for data segments that contain
/// circuit namespace strings referenced by the contract's zkas_db_set calls.
pub fn extract_zk_circuits(wasm_bincode: &[u8]) -> Result<Vec<CircuitMeta>, String> {
    let mut circuits = Vec::new();

    let parser = wasmparser::Parser::new(0);
    for payload in parser.parse_all(wasm_bincode) {
        let payload = payload.map_err(|e| format!("WASM parse error: {e}"))?;
        if let DataSection(reader) = payload {
            for segment in reader.into_iter_with_offsets() {
                let (_offset, data) = match segment {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let bytes = data.data.iter().copied().collect::<Vec<u8>>();
                // Look for circuit namespace strings in data segments.
                // Circuit binaries have identifiable structure: magic + namespace + name.
                if let Some(circuit) = try_parse_circuit_data(&bytes) {
                    circuits.push(circuit);
                }
            }
        }
    }

    Ok(circuits)
}

/// Try to parse circuit metadata from a WASM data segment.
///
/// Circuit binaries (.zk.bin) contain a magic header followed by
/// namespace and circuit name strings. We do a best-effort extraction.
fn try_parse_circuit_data(data: &[u8]) -> Option<CircuitMeta> {
    // .zk.bin files start with a 4-byte magic: 0x7A 0x6B 0x62 0x69 ("zkbi")
    if data.len() < 8 || &data[0..4] != b"zkbi" {
        return None;
    }

    // After the magic, the format is:
    //   namespace_len: u16 LE
    //   namespace: UTF-8 bytes
    //   name_len: u16 LE
    //   name: UTF-8 bytes
    let ns_len = u16::from_le_bytes([data[4], data[5]]) as usize;
    if data.len() < 6 + ns_len + 2 {
        return None;
    }
    let namespace = std::str::from_utf8(&data[6..6 + ns_len]).ok()?.to_string();

    let name_offset = 6 + ns_len;
    let name_len = u16::from_le_bytes([data[name_offset], data[name_offset + 1]]) as usize;
    if data.len() < name_offset + 2 + name_len {
        return None;
    }
    let name = std::str::from_utf8(&data[name_offset + 2..name_offset + 2 + name_len])
        .ok()?
        .to_string();

    Some(CircuitMeta { name, namespace })
}

/// Verify that a manifest accurately describes the WASM binary.
///
/// Mechanical, objective comparison. Zero trust required.
pub fn verify_manifest_against_wasm(
    manifest: &ContractManifest,
    wasm_bincode: &[u8],
) -> VerificationResult {
    let exports = extract_wasm_exports(wasm_bincode).unwrap_or_else(|_| WasmExports {
        functions: vec![],
        has_memory: false,
        has_initialize: false,
        has_entrypoint: false,
        has_update: false,
        has_metadata: false,
    });

    let wasm_circuits = extract_zk_circuits(wasm_bincode).unwrap_or_default();

    // Function cross-reference
    let manifest_names: HashSet<&str> =
        manifest.functions.iter().map(|f| f.name.as_str()).collect();
    let wasm_names: HashSet<&str> = exports.functions.iter().map(|s| s.as_str()).collect();

    let mut missing: Vec<String> = manifest_names
        .difference(&wasm_names)
        .map(|s| s.to_string())
        .collect();
    missing.sort();

    let mut extra: Vec<String> = wasm_names
        .difference(&manifest_names)
        .map(|s| s.to_string())
        .collect();
    extra.sort();

    // Circuit cross-reference
    let wasm_circuit_names: HashSet<&str> =
        wasm_circuits.iter().map(|c| c.name.as_str()).collect();
    let manifest_circuit_names: HashSet<&str> =
        manifest.circuits.iter().map(|c| c.name.as_str()).collect();

    let mut missing_circuits: Vec<String> = manifest_circuit_names
        .difference(&wasm_circuit_names)
        .map(|s| s.to_string())
        .collect();
    missing_circuits.sort();

    let mut circuit_mismatches: Vec<String> = Vec::new();

    for mc in &manifest.circuits {
        if let Some(wc) = wasm_circuits.iter().find(|c| c.name == mc.name) {
            if wc.namespace != mc.namespace {
                circuit_mismatches.push(format!(
                    "{}: manifest namespace '{}', WASM namespace '{}'",
                    mc.name, mc.namespace, wc.namespace
                ));
            }
            // Check at least one function references this circuit
            let used = manifest
                .functions
                .iter()
                .any(|f| f.proof_circuit.as_deref() == Some(&mc.name));
            if !used {
                circuit_mismatches.push(format!(
                    "{}: declared in circuits but no function references it",
                    mc.name
                ));
            }
        }
    }

    // Circuits in WASM but not in manifest
    for wc in &wasm_circuits {
        if !manifest.circuits.iter().any(|mc| mc.name == wc.name) {
            circuit_mismatches.push(format!(
                "{}: exists in WASM but not declared in manifest",
                wc.name
            ));
        }
    }

    VerificationResult {
        passed: missing.is_empty()
            && missing_circuits.is_empty()
            && circuit_mismatches.is_empty(),
        manifest_functions: manifest_names.len(),
        wasm_functions: wasm_names.len(),
        missing_exports: missing,
        extra_exports: extra,
        manifest_circuits: manifest_circuit_names.len(),
        wasm_circuits: wasm_circuit_names.len(),
        missing_circuits,
        circuit_mismatches,
    }
}
