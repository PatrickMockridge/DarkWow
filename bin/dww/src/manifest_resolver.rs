/// Manifest resolver — composable wallet query layer.
///
/// Wraps a `ContractManifest` and provides lookup methods for the
/// wallet's CLI, capability resolver, and UX layer. Pure reads from
/// the already-parsed manifest — no I/O, no async.
///
/// Matches the Python `ManifestResolver` spec in wallet_model.py.

use dwow_sdk::manifest::{
    ContractManifest, ManifestAction, ManifestCapability, ManifestCostProfile,
    ManifestFunction, ManifestParameter,
};

/// Resolves contract interface queries from a manifest.
pub struct ManifestResolver<'a> {
    manifest: &'a ContractManifest,
}

impl<'a> ManifestResolver<'a> {
    pub fn new(manifest: &'a ContractManifest) -> Self {
        Self { manifest }
    }

    /// Look up a function by name.
    pub fn get_function(&self, name: &str) -> Option<&ManifestFunction> {
        self.manifest.functions.iter().find(|f| f.name == name)
    }

    /// Look up a function by opcode.
    pub fn get_function_by_code(&self, code: u8) -> Option<&ManifestFunction> {
        self.manifest.functions.iter().find(|f| f.code == code)
    }

    /// Look up a capability by name.
    pub fn get_capability(&self, name: &str) -> Option<&ManifestCapability> {
        self.manifest.capabilities.iter().find(|c| c.name == name)
    }

    /// Look up a capability by discriminant.
    pub fn get_capability_by_discriminant(
        &self,
        discriminant: u8,
    ) -> Option<&ManifestCapability> {
        self.manifest
            .capabilities
            .iter()
            .find(|c| c.discriminant == discriminant)
    }

    /// Get actions associated with a function.
    pub fn get_actions_for(&self, function: &str) -> Vec<&ManifestAction> {
        self.manifest
            .actions
            .iter()
            .filter(|a| a.function == function)
            .collect()
    }

    /// Get parameter schema for a function.
    pub fn get_params_for(&self, function: &str) -> Option<&ManifestParameter> {
        self.manifest
            .parameters
            .iter()
            .find(|p| p.function == function)
    }

    /// Get cost profile for a function. [1:1] manifest.md [[cost_profiles]].
    ///
    /// Returns the declared `circuit_difficulty`, `k_value`, `wasm_kb`, and
    /// `tolerance` for this function. The wallet reads this value for fee
    /// construction — it trusts the declaration. Verification is the miner's
    /// responsibility (economic incentive to detect misdeclared costs).
    pub fn get_cost_profile(&self, function: &str) -> Option<&ManifestCostProfile> {
        self.manifest
            .cost_profiles
            .iter()
            .find(|cp| cp.function == function)
    }

    /// List all function names — for CLI auto-completion.
    pub fn list_functions(&self) -> Vec<&str> {
        self.manifest
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect()
    }

    /// List all capability names.
    pub fn list_capabilities(&self) -> Vec<&str> {
        self.manifest
            .capabilities
            .iter()
            .map(|c| c.name.as_str())
            .collect()
    }

    /// Validate parameters against the manifest's schema.
    ///
    /// Returns `Ok(())` if all required params are present and types
    /// are valid, or an error message describing what's wrong.
    pub fn validate_params(
        &self,
        function: &str,
        params: &serde_json::Value,
    ) -> Result<(), String> {
        let schema = match self.get_params_for(function) {
            Some(s) => s,
            None => return Ok(()), // No schema = any params accepted
        };

        for field in &schema.fields {
            let value = params.get(&field.name);
            match value {
                None | Some(serde_json::Value::Null) => {
                    if !field.optional {
                        return Err(format!("Missing required parameter: {}", field.name));
                    }
                }
                Some(v) => {
                    if !Self::validate_field_type(&field.param_type, v) {
                        return Err(format!(
                            "Invalid type for {}: expected {}",
                            field.name, field.param_type
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_field_type(param_type: &str, value: &serde_json::Value) -> bool {
        match param_type {
            "u64" => value.is_number() && value.as_u64().is_some(),
            "pallas_base" | "pallas_scalar" | "public_key" | "contract_id" => {
                value.is_string() && value.as_str().unwrap().len() >= 32
            }
            "bool" => value.is_boolean(),
            "string" => value.is_string(),
            "bytes" => value.is_string() || value.is_array(),
            _ => true, // Unknown types pass through
        }
    }

    /// Human-readable description of the contract interface.
    pub fn describe(&self) -> String {
        self.describe_with_trust(None)
    }

    /// Human-readable description with optional trust tier.
    pub fn describe_with_trust(&self, trust: Option<&dwow_sdk::manifest::TrustTier>) -> String {
        let mut lines = Vec::new();

        let trust_str = match trust {
            Some(t) => match t {
                dwow_sdk::manifest::TrustTier::Unverified =>
                    format!(" [{} — manifest is self-reported, verify before use]", t),
                _ => format!(" [{}]", t),
            },
            None => String::new(),
        };

        lines.push(format!(
            "Contract: {} ({}){}",
            self.manifest.name, self.manifest.category, trust_str
        ));
        lines.push(format!("Version: {}", self.manifest.version));
        lines.push(format!("Description: {}", self.manifest.description));
        lines.push(String::new());

        if !self.manifest.functions.is_empty() {
            lines.push(format!("Functions ({}):", self.manifest.functions.len()));
            for f in &self.manifest.functions {
                let proof = if f.requires_proof {
                    format!(
                        " [proof: {}]",
                        f.proof_circuit.as_deref().unwrap_or("unknown")
                    )
                } else {
                    String::new()
                };
                lines.push(format!(
                    "  {} (0x{:02x}) — {}{}",
                    f.name, f.code, f.description, proof
                ));
            }
        }

        if !self.manifest.capabilities.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Capabilities ({}):",
                self.manifest.capabilities.len()
            ));
            for c in &self.manifest.capabilities {
                lines.push(format!(
                    "  {} (0x{:02x}) — {}",
                    c.name, c.discriminant, c.description
                ));
            }
        }

        if !self.manifest.actions.is_empty() {
            lines.push(String::new());
            lines.push(format!("Actions ({}):", self.manifest.actions.len()));
            for a in &self.manifest.actions {
                let requires_str = if a.requires.capabilities.is_empty() {
                    a.requires.expr_type.clone()
                } else {
                    format!(
                        "{} [{}]",
                        a.requires.expr_type,
                        a.requires.capabilities.join(", ")
                    )
                };
                let produces_str = if a.produces.is_empty() {
                    "nothing".to_string()
                } else {
                    a.produces
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                lines.push(format!(
                    "  {}: requires={}, produces={}",
                    a.function, requires_str, produces_str
                ));
            }
        }

        if !self.manifest.trees.is_empty() {
            lines.push(String::new());
            lines.push(format!("State Trees ({}):", self.manifest.trees.len()));
            for t in &self.manifest.trees {
                lines.push(format!("  {} — {}", t.name, t.description));
            }
        }

        if !self.manifest.dependencies.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Dependencies: {}",
                self.manifest.dependencies.join(", ")
            ));
        }

        if !self.manifest.parameters.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Parameter Schemas ({}):",
                self.manifest.parameters.len()
            ));
            for p in &self.manifest.parameters {
                let fields: Vec<String> = p
                    .fields
                    .iter()
                    .map(|f| {
                        let opt = if f.optional { "?" } else { "" };
                        format!("{}{}: {}", f.name, opt, f.param_type)
                    })
                    .collect();
                lines.push(format!("  {}: {}", p.function, fields.join(", ")));
            }
        }

        lines.join("\n")
    }
}
