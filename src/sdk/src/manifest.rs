/// On-chain contract manifest — capability-native ABI.
///
/// A TOML document embedded in `DeployParamsV1::ix` with a `0x4D` magic byte
/// prefix. Describes a contract's functions, capabilities, actions, state
/// trees, ZK circuits, dependencies, and parameter schemas — enabling any
/// wallet to interact with any contract without hardcoded Rust knowledge.
///
/// This is an opt-in system. Contracts deployed without a manifest continue
/// to work via the existing hardcoded capability descriptors.

use serde::{Deserialize, Serialize};

/// The manifest — complete contract interface description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractManifest {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub functions: Vec<ManifestFunction>,
    #[serde(default)]
    pub capabilities: Vec<ManifestCapability>,
    #[serde(default)]
    pub actions: Vec<ManifestAction>,
    #[serde(default)]
    pub trees: Vec<ManifestTree>,
    #[serde(default)]
    pub circuits: Vec<ManifestCircuit>,
    #[serde(default)]
    pub parameters: Vec<ManifestParameter>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// A contract function — maps to a WASM export and ZK circuit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFunction {
    pub name: String,
    pub code: u8,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub requires_proof: bool,
    pub proof_circuit: Option<String>,
}

/// A capability type this contract defines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCapability {
    pub discriminant: u8,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// An action that exercises capabilities — requires/consumes/produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestAction {
    pub function: String,
    #[serde(default = "default_requires")]
    pub requires: CapabilityExpression,
    #[serde(default)]
    pub consumes: Vec<String>,
    #[serde(default)]
    pub produces: Vec<CapabilityOutput>,
}

fn default_requires() -> CapabilityExpression {
    CapabilityExpression {
        expr_type: "none".to_string(),
        capabilities: vec![],
        capability: None,
        count: None,
        total: None,
    }
}

/// Serializable capability requirement expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityExpression {
    #[serde(rename = "type")]
    pub expr_type: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub capability: Option<String>,
    pub count: Option<u32>,
    pub total: Option<u32>,
}

/// A capability produced by an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityOutput {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// A named sled tree the contract writes to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestTree {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// A ZK proof circuit referenced by the contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCircuit {
    pub name: String,
    pub namespace: String,
}

/// Parameter schema for a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestParameter {
    pub function: String,
    #[serde(default)]
    pub fields: Vec<ParameterField>,
}

/// A single parameter in a function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterField {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub optional: bool,
}

/// Magic byte prefix for manifest detection in deploy ix.
pub const MANIFEST_MAGIC_BYTE: u8 = 0x4D;

/// Wrapper for TOML deserialization — the TOML has a `[contract]` section.
#[derive(Debug, Serialize, Deserialize)]
struct ContractManifestToml {
    contract: ContractManifest,
    #[serde(default)]
    functions: Vec<ManifestFunction>,
    #[serde(default)]
    capabilities: Vec<ManifestCapability>,
    #[serde(default)]
    actions: Vec<ManifestAction>,
    #[serde(default)]
    trees: Vec<ManifestTree>,
    #[serde(default)]
    circuits: Vec<ManifestCircuit>,
    #[serde(default)]
    parameters: Vec<ManifestParameter>,
}

impl ContractManifest {
    /// Parse from TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        let wrapper: ContractManifestToml =
            toml::from_str(toml_str).map_err(|e| format!("Invalid TOML: {e}"))?;
        let mut manifest = wrapper.contract;
        manifest.functions = wrapper.functions;
        manifest.capabilities = wrapper.capabilities;
        manifest.actions = wrapper.actions;
        manifest.trees = wrapper.trees;
        manifest.circuits = wrapper.circuits;
        manifest.parameters = wrapper.parameters;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serialize to TOML string.
    pub fn to_toml(&self) -> Result<String, String> {
        let wrapper = ContractManifestToml {
            contract: self.clone(),
            functions: self.functions.clone(),
            capabilities: self.capabilities.clone(),
            actions: self.actions.clone(),
            trees: self.trees.clone(),
            circuits: self.circuits.clone(),
            parameters: self.parameters.clone(),
        };
        toml::to_string_pretty(&wrapper).map_err(|e| format!("TOML serialization failed: {e}"))
    }

    /// Encode for deploy ix — magic byte + TOML.
    pub fn to_deploy_ix(&self) -> Result<Vec<u8>, String> {
        let toml_str = self.to_toml()?;
        let mut bytes = vec![MANIFEST_MAGIC_BYTE];
        bytes.extend_from_slice(toml_str.as_bytes());
        Ok(bytes)
    }

    /// Validate cross-references between sections.
    pub fn validate(&self) -> Result<(), String> {
        let func_names: Vec<&str> = self.functions.iter().map(|f| f.name.as_str()).collect();
        let cap_names: Vec<&str> = self.capabilities.iter().map(|c| c.name.as_str()).collect();

        for action in &self.actions {
            if !func_names.contains(&action.function.as_str()) {
                return Err(format!(
                    "Action references unknown function: {}",
                    action.function
                ));
            }
            for cap_name in &action.requires.capabilities {
                if !cap_names.contains(&cap_name.as_str()) {
                    return Err(format!(
                        "Action requires unknown capability: {}",
                        cap_name
                    ));
                }
            }
            for cap_name in &action.consumes {
                if !cap_names.contains(&cap_name.as_str()) {
                    return Err(format!(
                        "Action consumes unknown capability: {}",
                        cap_name
                    ));
                }
            }
        }

        for param in &self.parameters {
            if !func_names.contains(&param.function.as_str()) {
                return Err(format!(
                    "Parameters reference unknown function: {}",
                    param.function
                ));
            }
        }

        Ok(())
    }

    /// Check if deploy ix contains a manifest (starts with magic byte).
    pub fn is_manifest(ix: &[u8]) -> bool {
        ix.first() == Some(&MANIFEST_MAGIC_BYTE)
    }

    /// Parse manifest from deploy ix bytes. Returns None if no manifest.
    pub fn from_deploy_ix(ix: &[u8]) -> Option<Result<Self, String>> {
        if !Self::is_manifest(ix) {
            return None;
        }
        let toml_bytes = &ix[1..];
        let toml_str = std::str::from_utf8(toml_bytes).ok()?;
        Some(Self::from_toml(toml_str))
    }

    /// Create deploy ix from manifest. None means legacy (empty ix).
    pub fn create_deploy_ix(manifest: Option<&Self>) -> Result<Vec<u8>, String> {
        match manifest {
            Some(m) => m.to_deploy_ix(),
            None => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAO_ESCROW_TOML: &str = r#"
[contract]
name = "dao_escrow"
category = "DAO"
description = "DAO-governed endowment with DrainProtection"
version = "1.0.0"
dependencies = ["native_token_v1"]

[[functions]]
name = "initialize"
code = 0
description = "Create a new DAO endowment"
requires_proof = true
proof_circuit = "init_v1"

[[functions]]
name = "pay_premium"
code = 1
description = "Pay premium to a drain-protected pool"
requires_proof = true
proof_circuit = "pay_premium_v1"

[[capabilities]]
discriminant = 0
name = "creator"
description = "The DAO endowment creator"

[[capabilities]]
discriminant = 1
name = "treasury_governor"
description = "Can propose and vote on fund allocation"

[[actions]]
function = "initialize"
requires = { type = "none" }
produces = [{ name = "creator", description = "Endowment creator capability" }]

[[actions]]
function = "pay_premium"
requires = { type = "any", capabilities = ["creator", "treasury_governor"] }
produces = [{ name = "receipt", description = "Premium payment confirmation" }]

[[trees]]
name = "daos"
description = "Active DAO endowments"

[[circuits]]
name = "init_v1"
namespace = "dao_escrow"

[[circuits]]
name = "pay_premium_v1"
namespace = "dao_escrow"

[[parameters]]
function = "initialize"
fields = [
    { name = "dao_bulla", type = "pallas_base" },
    { name = "endowment_token_id", type = "pallas_base" },
    { name = "enable_drain_protection", type = "bool", optional = true },
]
"#;

    #[test]
    fn test_parse_manifest() {
        let m = ContractManifest::from_toml(DAO_ESCROW_TOML).unwrap();
        assert_eq!(m.name, "dao_escrow");
        assert_eq!(m.category, "DAO");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.dependencies, vec!["native_token_v1"]);
        assert_eq!(m.functions.len(), 2);
        assert_eq!(m.functions[0].name, "initialize");
        assert_eq!(m.functions[0].code, 0);
        assert!(m.functions[0].requires_proof);
        assert_eq!(m.capabilities.len(), 2);
        assert_eq!(m.capabilities[0].discriminant, 0);
        assert_eq!(m.actions.len(), 2);
        assert_eq!(m.actions[1].requires.expr_type, "any");
        assert_eq!(m.trees.len(), 1);
        assert_eq!(m.circuits.len(), 2);
        assert_eq!(m.parameters.len(), 1);
        assert_eq!(m.parameters[0].fields.len(), 3);
    }

    #[test]
    fn test_minimal_manifest() {
        let toml = r#"
[contract]
name = "minimal"
category = "Other"
description = "Minimal"
"#;
        let m = ContractManifest::from_toml(toml).unwrap();
        assert_eq!(m.name, "minimal");
        assert!(m.functions.is_empty());
        assert!(m.capabilities.is_empty());
    }

    #[test]
    fn test_validate_bad_action_ref() {
        let toml = r#"
[contract]
name = "bad"
category = "Other"
description = "Bad refs"

[[actions]]
function = "nonexistent"
requires = { type = "none" }
"#;
        let result = ContractManifest::from_toml(toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("nonexistent"));
    }

    #[test]
    fn test_magic_byte_detection() {
        let manifest_bytes = vec![0x4D, b'[', b'c'];  // simplified
        assert!(ContractManifest::is_manifest(&manifest_bytes));

        let legacy_bytes = vec![0x00, 0x01];
        assert!(!ContractManifest::is_manifest(&legacy_bytes));

        let empty: Vec<u8> = vec![];
        assert!(!ContractManifest::is_manifest(&empty));
    }

    #[test]
    fn test_deploy_ix_roundtrip() {
        let m = ContractManifest::from_toml(DAO_ESCROW_TOML).unwrap();
        let ix = m.to_deploy_ix().unwrap();
        assert_eq!(ix[0], MANIFEST_MAGIC_BYTE);

        let parsed = ContractManifest::from_deploy_ix(&ix).unwrap().unwrap();
        assert_eq!(parsed.name, "dao_escrow");
        assert_eq!(parsed.functions.len(), 2);
    }

    #[test]
    fn test_opt_out() {
        // No manifest — legacy ix
        let ix = ContractManifest::create_deploy_ix(None).unwrap();
        assert!(ix.is_empty());
        assert!(!ContractManifest::is_manifest(&ix));
        assert!(ContractManifest::from_deploy_ix(&ix).is_none());
    }
}
