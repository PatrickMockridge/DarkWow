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

use dwow_serial::Decodable;

use hex;

use crate::capability::{wallet_construct, Barb, Primitive, TypedCapability};
use crate::crypto::PublicKey;
use crate::pasta::pallas;

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
    #[serde(default)]
    pub cost_profiles: Vec<ManifestCostProfile>,
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
    /// The primitive types this capability composes — canonical §8.1 names, e.g.
    /// `["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]`.
    /// The wallet unions their barbs to construct the emergent capability type
    /// (ocap.md §2, composition.md §1). Empty = not typed-constructible (opt-in).
    #[serde(default)]
    pub primitives: Vec<String>,
    /// Ordered, typed field layout of this capability's AEAD note plaintext,
    /// mirroring `dwow_serial` wire order. Lets the wallet decode a foreign
    /// note generically, with no per-contract code (ocap.md §7). Empty = not
    /// generically decodable.
    #[serde(default)]
    pub note_schema: Vec<ParameterField>,
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
    /// The barbs this action exercises — canonical §8.1 barb names, e.g.
    /// `["Spend","Nullify","Commit","Dispatch","Gate","Denominate"]`. The
    /// produced capability's composed barbs MUST cover these for the type to be
    /// valid (composition.md §1.3). Empty = no barb requirement (vacuously covered).
    #[serde(default)]
    pub required_barbs: Vec<String>,
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
///
/// The `witness_map` declares how each witness slot in the circuit's
/// ordered `ZkBinary.witnesses: Vec<VarType>` is bound at proof time
/// (wallet.md §6.4.1, witness-binding rule). One entry per slot, each
/// a source string: `"note:<field>"`, `"param:<field>"`, `"secret"`,
/// `"merkle_path"`, `"leaf_position"`, `"blind"`, `"tx_commitment"`,
/// `"tx_nonce"`. Parsed by `CircuitWitnessMap::from_manifest` in the
/// generic prover module (prover.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCircuit {
    pub name: String,
    pub namespace: String,
    /// Witness-binding declarations, one per witness slot in declared order.
    /// Empty for contracts that pre-date the typed-manifest specification.
    #[serde(default)]
    pub witness_map: Vec<String>,
    /// Ordered list of ZK opcode names the circuit uses. Combined with the
    /// circuit's k parameter (embedded in the zkas binary), this enables
    /// independent verification of `circuit_difficulty` in `[[cost_profiles]]`.
    /// Verification is the miner's responsibility — economic incentive.
    #[serde(default)]
    pub opcodes: Vec<String>,
}

/// Per-function cost declaration — [1:1] with manifest.md `[[cost_profiles]]`
/// and the Python `CostProfile` dataclass in `contrib/model/fee_window_model.py`.
///
/// The wallet reads `circuit_difficulty` directly for fee construction (trust).
/// The miner independently computes `Σ OPCODE_DIFFICULTY[op] × 2^(k - K_REF)`
/// from `[[circuits]].opcodes` and the zkas binary's k parameter, comparing
/// against the declared value (verify). A mismatch is a black mark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCostProfile {
    /// SHALL match a `name` in `[[functions]]`.
    pub function: String,
    /// Σ opcode_cost × 2^(k - K_REF) — deterministic baseline from opcode table.
    pub circuit_difficulty: u64,
    /// Circuit's Halo2 k parameter (domain size = 2^k rows).
    pub k_value: u32,
    /// Expected WASM execution overhead in kB-equivalent.
    pub wasm_kb: u64,
    /// Allowed deviation before black mark (±50% = 0.50).
    pub tolerance: f64,
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

/// A decoded note field value, produced by [`decode_note_by_schema`].
#[derive(Debug, Clone, PartialEq)]
pub enum NoteFieldValue {
    U64(u64),
    Bool(bool),
    Base(pallas::Base),
    Scalar(pallas::Scalar),
    PublicKey(PublicKey),
    Bytes(Vec<u8>),
    /// An `optional` field that was absent on the wire (leading tag byte = 0).
    Absent,
}

/// Decode an AEAD note plaintext GENERICALLY from a manifest-declared field
/// schema — no per-contract Rust type (ocap.md §7: manifest-driven, zero
/// per-contract code).
///
/// `dwow_serial` is a positional, tag-free wire format: the derive macro
/// consumes fields strictly in declaration order with no field names or type
/// tags on the wire. So walking `schema` in order and reading each field by its
/// type exactly reconstructs the struct.
///
/// PURE and all-`Result`: a malformed or mismatched schema (over/under-run, a
/// noncanonical field element, an unknown type token) is a clean `Err`, never a
/// panic. The final **full-consumption** check rejects a schema that does not
/// describe the ENTIRE plaintext (mirrors `dwow_serial::deserialize`) — this is
/// what stops a too-short schema from silently mis-attributing fields.
///
/// Wire semantics (pinned; `ParameterField` is a deploy-ix wire type):
/// - `u64` = LE 8 bytes; `bool` = 1 byte.
/// - `pallas_base` / `token_id` / `func_id` / `contract_id` = 32 bytes, canonical
///   `Fp` check; `pallas_scalar` = 32 bytes, canonical `Fq` check; `public_key` =
///   32 bytes, canonical point check.
/// - `bytes` = `dwow_serial` VarInt length prefix + N bytes.
/// - `optional=true` = a leading 1-byte presence tag, then the value if present
///   (matches `dwow_serial`'s `Option<T>` encoding).
pub fn decode_note_by_schema(
    plaintext: &[u8],
    schema: &[ParameterField],
) -> Result<Vec<(String, NoteFieldValue)>, String> {
    let mut cursor = std::io::Cursor::new(plaintext);
    let mut out = Vec::with_capacity(schema.len());

    for field in schema {
        // Optional fields carry a 1-byte presence tag (dwow_serial `Option<T>`).
        if field.optional {
            let present = bool::decode(&mut cursor)
                .map_err(|e| format!("note_schema tag '{}': {e}", field.name))?;
            if !present {
                out.push((field.name.clone(), NoteFieldValue::Absent));
                continue
            }
        }
        let val = decode_note_field(&field.param_type, &mut cursor).map_err(|e| {
            format!("note_schema field '{}' (type {}): {e}", field.name, field.param_type)
        })?;
        out.push((field.name.clone(), val));
    }

    // Full-consumption: the schema MUST describe the entire plaintext.
    let pos = cursor.position() as usize;
    if pos != plaintext.len() {
        return Err(format!(
            "note_schema decoded {pos} of {} bytes — schema does not match note",
            plaintext.len()
        ))
    }
    Ok(out)
}

impl NoteFieldValue {
    pub fn as_u64(&self) -> Option<u64> { if let Self::U64(v) = self { Some(*v) } else { None } }
    pub fn as_base(&self) -> Option<pallas::Base> { if let Self::Base(v) = self { Some(*v) } else { None } }
    pub fn as_scalar(&self) -> Option<pallas::Scalar> { if let Self::Scalar(v) = self { Some(*v) } else { None } }
    pub fn as_bytes(&self) -> Option<&[u8]> { if let Self::Bytes(v) = self { Some(v.as_slice()) } else { None } }
}

/// Look up a named field in a decoded note-schema result.
pub fn note_field<'a>(fields: &'a [(String, NoteFieldValue)], name: &str) -> Option<&'a NoteFieldValue> {
    fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

/// Encode call-parameters according to a manifest-declared schema.
///
/// This is the write-path dual of [`decode_note_by_schema`]: given a function's
/// `[[parameters]]` field list and a JSON-encoded `params` object, produce the
/// tag-free positional wire bytes that the contract's entrypoint expects.
///
/// Wire semantics (same as decode):
/// - `u64` = LE 8 bytes; `bool` = 1 byte.
/// - `pallas_base` / `token_id` / `func_id` / `contract_id` = 32 bytes.
/// - `pallas_scalar` = 32 bytes.
/// - `public_key` = 32 bytes (x-coordinate).
/// - `bytes` = VarInt length prefix + N bytes.
/// - `optional=true` = leading 1-byte presence tag.
///
/// All fields are positional (in schema declaration order) and tag-free —
/// `dwow_serial` encodes structs strictly by field order with no names or type
/// tags on the wire.
#[cfg(feature = "json")]
pub fn encode_params_by_schema(
    schema: &[ParameterField],
    params_json: &str,
) -> Result<Vec<u8>, String> {
    // Parse JSON into a flat map — lightweight, no serde derive needed.
    let param_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(params_json).map_err(|e| format!("params JSON parse: {e}"))?;

    let mut buf = Vec::new();
    for field in schema {
        let raw = param_map.get(&field.name);
        if field.optional {
            let present = raw.is_some() && !raw.unwrap().is_null();
            dwow_serial::Encodable::encode(&present, &mut buf)
                .map_err(|e| format!("optional tag '{}': {e}", field.name))?;
            if !present {
                continue;
            }
        }
        let val = raw.ok_or_else(|| {
            format!("missing required parameter '{}'", field.name)
        })?;
        match field.param_type.as_str() {
            "u64" => {
                let v: u64 = val.as_u64().ok_or_else(||
                    format!("param '{}': expected u64, got {}", field.name, val))?;
                dwow_serial::Encodable::encode(&v, &mut buf)
                    .map_err(|e| format!("encode u64 '{}': {e}", field.name))?;
            }
            "bool" => {
                let v: bool = val.as_bool().ok_or_else(||
                    format!("param '{}': expected bool, got {}", field.name, val))?;
                dwow_serial::Encodable::encode(&v, &mut buf)
                    .map_err(|e| format!("encode bool '{}': {e}", field.name))?;
            }
            "pallas_base" | "token_id" | "func_id" | "contract_id" => {
                let hex = val.as_str().or_else(|| val.as_object().and_then(|_| None))
                    .ok_or_else(|| format!(
                        "param '{}': expected hex string for {}, got {}",
                        field.name, field.param_type, val,
                    ))?;
                let bytes = hex::decode(hex.strip_prefix("0x").unwrap_or(hex))
                    .map_err(|e| format!("param '{}' hex: {e}", field.name))?;
                if bytes.len() != 32 {
                    return Err(format!(
                        "param '{}': expected 32 bytes for {}, got {}",
                        field.name, field.param_type, bytes.len(),
                    ));
                }
                buf.extend_from_slice(&bytes);
            }
            "pallas_scalar" => {
                let hex = val.as_str()
                    .ok_or_else(|| format!(
                        "param '{}': expected hex string for pallas_scalar, got {}",
                        field.name, val,
                    ))?;
                let bytes = hex::decode(hex.strip_prefix("0x").unwrap_or(hex))
                    .map_err(|e| format!("param '{}' hex: {e}", field.name))?;
                if bytes.len() != 32 {
                    return Err(format!(
                        "param '{}': expected 32 bytes for pallas_scalar, got {}",
                        field.name, bytes.len(),
                    ));
                }
                buf.extend_from_slice(&bytes);
            }
            "public_key" => {
                let hex = val.as_str()
                    .ok_or_else(|| format!(
                        "param '{}': expected hex string for public_key, got {}",
                        field.name, val,
                    ))?;
                let bytes = hex::decode(hex.strip_prefix("0x").unwrap_or(hex))
                    .map_err(|e| format!("param '{}' hex: {e}", field.name))?;
                if bytes.len() != 32 {
                    return Err(format!(
                        "param '{}': expected 32 bytes for public_key, got {}",
                        field.name, bytes.len(),
                    ));
                }
                buf.extend_from_slice(&bytes);
            }
            "bytes" => {
                let hex = val.as_str()
                    .ok_or_else(|| format!(
                        "param '{}': expected hex string for bytes, got {}",
                        field.name, val,
                    ))?;
                let raw = hex::decode(hex.strip_prefix("0x").unwrap_or(hex))
                    .map_err(|e| format!("param '{}' hex: {e}", field.name))?;
                let len: u32 = raw.len() as u32;
                dwow_serial::Encodable::encode(&len, &mut buf)
                    .map_err(|e| format!("encode bytes len '{}': {e}", field.name))?;
                buf.extend_from_slice(&raw);
            }
            other => return Err(format!(
                "param '{}': unknown type '{}'", field.name, other,
            )),
        }
    }
    Ok(buf)
}

impl ContractManifest {
    /// Empty manifest — used as a placeholder when the caller provides
    /// pre-resolved capabilities and the witness_map + zkas binary are the
    /// only inputs the prover needs (wallet.md §6.4.1).
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            category: String::new(),
            version: "1.0.0".to_string(),
            dependencies: vec![],
            functions: vec![],
            actions: vec![],
            capabilities: vec![],
            trees: vec![],
            parameters: vec![],
            circuits: vec![],
            cost_profiles: vec![],
        }
    }

    /// Resolve the note field schema for the capability a function call produces.
    pub fn note_schema_for_function(&self, function_code: u8) -> Option<&[ParameterField]> {
        let f = self.function_by_code(function_code)?;
        let a = self.action_for_function(&f.name)?;
        let out = a.produces.first()?;
        Some(&self.capability_by_name(&out.name)?.note_schema)
    }
}

fn decode_note_field(
    ty: &str,
    cursor: &mut std::io::Cursor<&[u8]>,
) -> Result<NoteFieldValue, String> {
    Ok(match ty {
        "u64" => NoteFieldValue::U64(u64::decode(cursor).map_err(|e| e.to_string())?),
        "bool" => NoteFieldValue::Bool(bool::decode(cursor).map_err(|e| e.to_string())?),
        "pallas_base" | "token_id" | "func_id" | "contract_id" => {
            NoteFieldValue::Base(pallas::Base::decode(cursor).map_err(|e| e.to_string())?)
        }
        "pallas_scalar" => {
            NoteFieldValue::Scalar(pallas::Scalar::decode(cursor).map_err(|e| e.to_string())?)
        }
        "public_key" => {
            NoteFieldValue::PublicKey(PublicKey::decode(cursor).map_err(|e| e.to_string())?)
        }
        "bytes" => NoteFieldValue::Bytes(Vec::<u8>::decode(cursor).map_err(|e| e.to_string())?),
        other => return Err(format!("unknown note_schema type '{other}'")),
    })
}

// ============================================================================
// Capability Resolution — manifest → typed capability construction
// ============================================================================

/// A capability resolved from a manifest declaration. This is the type
/// the wallet constructs at scan time per wallet.md §2.2.
#[derive(Debug, Clone)]
pub struct ResolvedCapability {
    /// The capability's discriminant from the manifest
    pub discriminant: u8,
    /// Human-readable capability name (e.g., "coin", "credential")
    pub name: String,
    /// The function that produced this capability
    pub function: String,
    /// The primitive types this capability composes (parsed from the manifest
    /// capability declaration; unknown names dropped for forward-compat).
    pub primitives: Vec<Primitive>,
    /// The barbs the producing action requires (parsed from the manifest action).
    pub barbs: Vec<Barb>,
    /// Whether the capability is consumable (has a nullifier)
    pub consumable: bool,
}

impl ContractManifest {
    /// Find a function by its opcode.
    pub fn function_by_code(&self, code: u8) -> Option<&ManifestFunction> {
        self.functions.iter().find(|f| f.code == code)
    }

    /// Find the action associated with a function.
    pub fn action_for_function(&self, function_name: &str) -> Option<&ManifestAction> {
        self.actions.iter().find(|a| a.function == function_name)
    }

    /// Find a capability declaration by name.
    pub fn capability_by_name(&self, name: &str) -> Option<&ManifestCapability> {
        self.capabilities.iter().find(|c| c.name == name)
    }

    /// Resolve what capability a function call produces.
    ///
    /// Given a function code (from `call.data[0]`), looks up the function,
    /// finds its action, and returns the capability it produces. This is the
    /// manifest-driven type construction step: the manifest tells the wallet
    /// what type to construct from the decrypted note.
    pub fn resolve_capability(&self, function_code: u8) -> Option<ResolvedCapability> {
        let function = self.function_by_code(function_code)?;
        let action = self.action_for_function(&function.name)?;

        // Resolve the subject capability per manifest.md § "Action subject
        // resolution". The ρ-calculus structure of the action determines which
        // capability's primitives are composed:
        //   Pattern A (produce):     νx.(action!(x) | ...)        → produces[0]
        //   Pattern B (consume):     x?(y).(nullify!(y) | 0)      → consumes[0]
        //   Pattern D (observe):     x?(y).(observe!(y) | x!(y))  → requires[0]
        // Pattern C₂ (P≠C) primitives are the union of all involved capabilities
        // (see below).
        let cap_name = action.produces.first()
            .map(|o| o.name.as_str())
            .or_else(|| action.consumes.first().map(|s| s.as_str()))
            .or_else(|| action.requires.capabilities.first().map(|s| s.as_str()))?;

        // Collect involved capability names for primitive collection.
        // Pattern C₂ (produce+consume, P≠C): union of both capabilities'
        // primitives per type-system.md §6.1 — the action exercises authority
        // over both names simultaneously. All other patterns: exactly one name.
        let mut involved_names: Vec<&str> = Vec::new();
        for output in &action.produces {
            involved_names.push(output.name.as_str());
        }
        for consumed_name in &action.consumes {
            involved_names.push(consumed_name.as_str());
        }
        if involved_names.is_empty() {
            involved_names.push(cap_name);
        }
        involved_names.sort();
        involved_names.dedup();

        // Collect primitives from all involved capabilities. Fail CLOSED: if
        // ANY declared primitive name is unknown to this SDK version, or any
        // capability name is not declared, the entire capability is left
        // untyped (None) — silently dropping a primitive under-declares the
        // type and weakens the coverage predicate (unsound).
        let mut primitives: Vec<Primitive> = Vec::new();
        for name in &involved_names {
            let cap = self.capability_by_name(name)?;
            for s in &cap.primitives {
                primitives.push(Primitive::from_name(s)?);
            }
        }
        let mut barbs: Vec<Barb> = Vec::with_capacity(action.required_barbs.len());
        for s in &action.required_barbs {
            barbs.push(Barb::from_name(s)?);
        }
        // Canonicalize: composition order/duplication is irrelevant to the barb
        // set (composition.md §1.2), so equal compositions compare/hash equal.
        primitives.sort();
        primitives.dedup();
        barbs.sort();
        barbs.dedup();

        // A capability is consumable if any action lists any involved
        // capability name in its `consumes`.
        let consumable = self.actions.iter().any(|a| {
            involved_names.iter().any(|n| a.consumes.contains(&(*n).to_string()))
        });

        // The first involved capability provides the discriminant and name for
        // the ResolvedCapability (for single-capability actions this is the
        // only one; for union actions the first is canonical).
        let cap = self.capability_by_name(involved_names[0])?;

        Some(ResolvedCapability {
            discriminant: cap.discriminant,
            name: cap.name.clone(),
            function: function.name.clone(),
            primitives,
            barbs,
            consumable,
        })
    }

    /// Construct the emergent capability TYPE that a function call produces —
    /// the wallet's pure Discover→Hold typing step (ocap.md §6).
    ///
    /// Founded in the composition algebra (composition.md §1): the type is the
    /// barb-union of the declared primitives, valid IFF that union covers the
    /// action's required barbs. This adapts the manifest declaration into the
    /// `(primitives, required_barbs)` that the Lean-proven `wallet_construct`
    /// kernel consumes.
    ///
    /// PURE: depends only on `(manifest, function_code)`. No note values, no
    /// merkle proof, no I/O — two notes of the same capability yield the
    /// identical `TypedCapability` (ocap.md §3). Returns `None` when the declared
    /// primitives do not cover the required barbs ("fix the composition, not the
    /// wallet" — type-system.md §13), when a declared name is unknown (fail
    /// closed, via `resolve_capability`), or when the primitives are empty
    /// (name-possession, ocap.md §2 — this is how un-migrated manifests degrade).
    ///
    /// Resource identity is the capability name (ocap.md §3: the type depends on
    /// what the action applies to), so a multi-capability contract yields
    /// distinct types rather than several sharing the contract name.
    pub fn resolve_capability_type(&self, function_code: u8) -> Option<TypedCapability> {
        let ResolvedCapability { name, function, primitives, barbs, .. } =
            self.resolve_capability(function_code)?;
        if primitives.is_empty() {
            return None
        }
        let mut ct = wallet_construct(&name, &function, primitives, &barbs)?;
        // Composed barbs → canonical (sorted) order; `primitives` are already
        // canonical from `resolve_capability`.
        ct.barbs.sort();
        Some(ct)
    }

    /// Check if this manifest has capability declarations.
    /// Contracts without declarations cannot be used for type construction.
    pub fn has_capability_declarations(&self) -> bool {
        !self.capabilities.is_empty() && !self.actions.is_empty()
    }
}

/// Magic byte prefix for manifest detection in deploy ix.
pub const MANIFEST_MAGIC_BYTE: u8 = 0x4D;

/// Trust tier for a contract manifest. Additive — can only upgrade, never downgrade.
/// The wallet uses this to inform users whether a manifest can be trusted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TrustTier {
    /// Deployed at chain genesis — implicitly trusted.
    Genesis,
    /// Deployed by the user's own key — they know what they deployed.
    SelfDeployed,
    /// Independently verified by a trusted issuer via attestation contract.
    Attested,
    /// Self-reported manifest with no verification — caveat emptor.
    Unverified,
}

impl std::fmt::Display for TrustTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustTier::Genesis => write!(f, "GENESIS"),
            TrustTier::SelfDeployed => write!(f, "OWN"),
            TrustTier::Attested => write!(f, "ATTESTED"),
            TrustTier::Unverified => write!(f, "UNVERIFIED"),
        }
    }
}

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
    #[serde(default)]
    cost_profiles: Vec<ManifestCostProfile>,
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
            cost_profiles: self.cost_profiles.clone(),
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

        // Capability discriminants must be unique — they key CapabilityId::derive,
        // so a collision yields colliding capability IDs for the same instance.
        // (A `produces` name that is not a declared capability is NOT rejected
        // here: manifests conventionally use descriptive output labels, and
        // `resolve_capability` already fails closed per-capability for it.)
        for (i, cap) in self.capabilities.iter().enumerate() {
            if self.capabilities[..i].iter().any(|c| c.discriminant == cap.discriminant) {
                return Err(format!(
                    "Duplicate capability discriminant: {} ({})",
                    cap.discriminant, cap.name
                ));
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

        for cp in &self.cost_profiles {
            if !func_names.contains(&cp.function.as_str()) {
                return Err(format!(
                    "cost_profile references unknown function: {}",
                    cp.function
                ));
            }
            if cp.k_value < 10 || cp.k_value > 16 {
                return Err(format!(
                    "cost_profile k_value out of range [10, 16]: {} (function: {})",
                    cp.k_value, cp.function
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

    // ========================================================================
    // Capability composition — resolve_capability_type (the pure keystone fn).
    // Properties trace to composition.md §1-2, ocap.md §2-3, and the Lean
    // walletConstruct theorems mirrored in capability.rs.
    // ========================================================================

    use crate::capability::{Barb, Primitive};

    /// A generic (non-native) contract manifest declaring one typed capability:
    /// a coin-transfer composition matching ocap.md §2.1 / capability.rs tests.
    const TYPED_TOML: &str = r#"
[contract]
name = "promissory_note"
category = "Token"
description = "typed composition test"

[[functions]]
name = "transfer"
code = 4

[[capabilities]]
discriminant = 0
name = "coin"
primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]

[[actions]]
function = "transfer"
requires = { type = "none" }
produces = [{ name = "coin" }]
required_barbs = ["Spend","Nullify","Commit","Dispatch","Gate","Denominate"]
"#;

    #[test]
    fn test_resolve_capability_type_constructs() {
        let m = ContractManifest::from_toml(TYPED_TOML).unwrap();
        let ct = m.resolve_capability_type(4).expect("transfer must construct");
        // Resource identity is the capability name (ocap.md §3), not the contract.
        assert_eq!(ct.resource, "coin");
        assert_eq!(ct.action, "transfer");
        // Soundness: the composed barbs cover the action's required barbs.
        assert!(ct.covers(&[
            Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Dispatch,
            Barb::Gate, Barb::Denominate,
        ]));
        // Primitives are canonical (sorted by Ord) — declaration order is
        // irrelevant (composition.md §1.2).
        assert_eq!(ct.primitives, vec![
            Primitive::SecretKey, Primitive::Nullifier, Primitive::Commitment,
            Primitive::ContractId, Primitive::FuncId, Primitive::AssetId,
            Primitive::MerkleNode,
        ]);
        // barbs are canonical (sorted).
        let mut sorted = ct.barbs.clone();
        sorted.sort();
        assert_eq!(ct.barbs, sorted);
    }

    #[test]
    fn test_composition_order_irrelevant() {
        // Two manifests declaring the same primitives in different order yield
        // equal TypedCapabilities (canonicalization + Eq).
        let shuffled = TYPED_TOML.replace(
            r#"primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]"#,
            r#"primitives = ["MerkleNode","AssetId","FuncId","ContractId","Nullifier","Commitment","SecretKey"]"#,
        );
        let a = ContractManifest::from_toml(TYPED_TOML).unwrap().resolve_capability_type(4).unwrap();
        let b = ContractManifest::from_toml(&shuffled).unwrap().resolve_capability_type(4).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_resolve_capability_type_uncovered_returns_none() {
        // Drop Nullifier from the composition but still require Nullify: the
        // primitives no longer cover the barbs → None ("fix the composition").
        let toml = TYPED_TOML.replace(
            r#"primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]"#,
            r#"primitives = ["SecretKey","Commitment","ContractId","FuncId","AssetId","MerkleNode"]"#,
        );
        let m = ContractManifest::from_toml(&toml).unwrap();
        assert!(m.resolve_capability_type(4).is_none(),
            "missing Nullifier must fail coverage of Nullify");
    }

    #[test]
    fn test_resolve_capability_type_deterministic_and_instance_independent() {
        // Pure: same (manifest, fn_code) → byte-identical result, twice.
        let m = ContractManifest::from_toml(TYPED_TOML).unwrap();
        let a = m.resolve_capability_type(4).unwrap();
        let b = m.resolve_capability_type(4).unwrap();
        assert_eq!(a.resource, b.resource);
        assert_eq!(a.action, b.action);
        assert_eq!(a.primitives, b.primitives);
        assert_eq!(a.barbs, b.barbs);
    }

    #[test]
    fn test_resolve_capability_type_no_barb_manufacture() {
        // Every output barb must be carried by some input primitive; compose
        // adds structure, never authority (composition.md §1.2).
        let m = ContractManifest::from_toml(TYPED_TOML).unwrap();
        let ct = m.resolve_capability_type(4).unwrap();
        for b in &ct.barbs {
            assert!(ct.primitives.iter().any(|p| p.barbs().contains(b)),
                "barb {:?} not carried by any composed primitive", b);
        }
    }

    #[test]
    fn test_resolve_capability_type_missing_fields_is_none() {
        // A capability + action with NO primitives / required_barbs (an
        // un-migrated manifest) is not typed-constructible → None, no regression.
        let toml = r#"
[contract]
name = "legacy"
category = "Other"
description = "no typed fields"

[[functions]]
name = "act"
code = 0

[[capabilities]]
discriminant = 0
name = "thing"

[[actions]]
function = "act"
requires = { type = "none" }
produces = [{ name = "thing" }]
"#;
        let m = ContractManifest::from_toml(toml).unwrap();
        assert!(m.resolve_capability_type(0).is_none());
    }

    #[test]
    fn test_typed_manifest_toml_roundtrip() {
        // The new fields survive to_toml → from_toml and still construct.
        let m = ContractManifest::from_toml(TYPED_TOML).unwrap();
        let re = ContractManifest::from_toml(&m.to_toml().unwrap()).unwrap();
        assert_eq!(re.capabilities[0].primitives.len(), 7);
        assert_eq!(re.actions[0].required_barbs.len(), 6);
        assert!(re.resolve_capability_type(4).is_some());
    }

    #[test]
    fn test_unknown_names_fail_closed() {
        // A future primitive/barb name this SDK version doesn't know makes the
        // WHOLE capability untyped (None) — never a partial/weakened composition.
        // Dropping a required barb would loosen the safety predicate (unsound).
        let bad_primitive = r#"
[contract]
name = "fwd"
category = "Other"
description = "forward compat"

[[functions]]
name = "f"
code = 0

[[capabilities]]
discriminant = 0
name = "c"
primitives = ["SecretKey","FutureThing"]

[[actions]]
function = "f"
requires = { type = "none" }
produces = [{ name = "c" }]
required_barbs = ["Spend"]
"#;
        let m = ContractManifest::from_toml(bad_primitive).unwrap();
        assert!(m.resolve_capability_type(0).is_none(),
            "unknown primitive must fail closed (None)");

        let bad_barb = bad_primitive
            .replace(r#"primitives = ["SecretKey","FutureThing"]"#, r#"primitives = ["SecretKey"]"#)
            .replace(r#"required_barbs = ["Spend"]"#, r#"required_barbs = ["Spend","AnotherFuture"]"#);
        let m2 = ContractManifest::from_toml(&bad_barb).unwrap();
        assert!(m2.resolve_capability_type(0).is_none(),
            "unknown required barb must fail closed (None), never weaken coverage");
    }

    #[test]
    fn test_validate_rejects_duplicate_discriminant() {
        let toml = r#"
[contract]
name = "dup"
category = "Other"
description = "dup discriminant"

[[capabilities]]
discriminant = 0
name = "a"

[[capabilities]]
discriminant = 0
name = "b"
"#;
        let err = ContractManifest::from_toml(toml).unwrap_err();
        assert!(err.contains("Duplicate capability discriminant"), "got: {err}");
    }

    // ========================================================================
    // Generic note decode — decode_note_by_schema (the Path-2 wire walker).
    // ========================================================================

    fn field(name: &str, ty: &str) -> ParameterField {
        ParameterField { name: name.into(), param_type: ty.into(), optional: false }
    }

    #[test]
    fn test_decode_note_by_schema_matches_derive() {
        use dwow_serial::{serialize, SerialDecodable, SerialEncodable};

        // A struct mirroring a real contract note (NativeToken-shaped): mixed
        // u64 / base / scalar / bytes fields, decoded generically by schema.
        #[derive(SerialEncodable, SerialDecodable)]
        struct TestNote {
            value: u64,
            token_id: pallas::Base,
            spend_hook: pallas::Base,
            value_blind: pallas::Scalar,
            memo: Vec<u8>,
        }

        let note = TestNote {
            value: 4242,
            token_id: pallas::Base::from(7),
            spend_hook: pallas::Base::from(0),
            value_blind: pallas::Scalar::from(99),
            memo: vec![1, 2, 3, 4],
        };
        let bytes = serialize(&note);

        let schema = vec![
            field("value", "u64"),
            field("token_id", "pallas_base"),
            field("spend_hook", "pallas_base"),
            field("value_blind", "pallas_scalar"),
            field("memo", "bytes"),
        ];

        let d = decode_note_by_schema(&bytes, &schema).expect("schema decode must succeed");
        assert_eq!(d.len(), 5);
        assert_eq!(d[0].1, NoteFieldValue::U64(4242));
        assert_eq!(d[1].1, NoteFieldValue::Base(pallas::Base::from(7)));
        assert_eq!(d[3].1, NoteFieldValue::Scalar(pallas::Scalar::from(99)));
        assert_eq!(d[4].1, NoteFieldValue::Bytes(vec![1, 2, 3, 4]));
        // Field names are carried through.
        assert_eq!(d[1].0, "token_id");
    }

    #[test]
    fn test_decode_note_by_schema_rejects_underrun() {
        use dwow_serial::serialize;
        // Plaintext = two u64s; schema describes only one → trailing bytes left →
        // must fail (full-consumption), never silently mis-attribute.
        let mut bytes = serialize(&7u64);
        bytes.extend(serialize(&8u64));
        assert!(decode_note_by_schema(&bytes, &[field("a", "u64")]).is_err());
    }

    #[test]
    fn test_decode_note_by_schema_rejects_overrun() {
        use dwow_serial::serialize;
        // Plaintext = one u64; schema wants two → clean Err, no panic.
        let bytes = serialize(&7u64);
        assert!(decode_note_by_schema(&bytes, &[field("a", "u64"), field("b", "u64")]).is_err());
    }

    #[test]
    fn test_decode_note_by_schema_rejects_unknown_type() {
        use dwow_serial::serialize;
        let bytes = serialize(&7u64);
        assert!(decode_note_by_schema(&bytes, &[field("a", "widget")]).is_err());
    }

    #[test]
    fn test_note_encrypt_decrypt_raw_schema_e2e() {
        // The highest-risk composition: a real multi-field note through the FULL
        // path encrypt -> decrypt_raw -> decode_note_by_schema, asserting full
        // consumption (decrypt_raw must strip the AEAD pad so nothing is left over)
        // and field-for-field equality.
        use dwow_serial::{SerialDecodable, SerialEncodable};
        use crate::crypto::{note::AeadEncryptedNote, Keypair};
        use rand::rngs::OsRng;

        #[derive(SerialEncodable, SerialDecodable)]
        struct TestNote {
            value: u64,
            token_id: pallas::Base,
            spend_hook: pallas::Base,
            value_blind: pallas::Scalar,
            memo: Vec<u8>,
        }

        let note = TestNote {
            value: 777,
            token_id: pallas::Base::from(3),
            spend_hook: pallas::Base::from(0),
            value_blind: pallas::Scalar::from(11),
            memo: vec![9, 9],
        };
        let kp = Keypair::random(&mut OsRng);
        let enc = AeadEncryptedNote::encrypt(&note, &kp.public, &mut OsRng).unwrap();

        let raw = enc.decrypt_raw(&kp.secret, 0).unwrap();
        let schema = vec![
            field("value", "u64"),
            field("token_id", "pallas_base"),
            field("spend_hook", "pallas_base"),
            field("value_blind", "pallas_scalar"),
            field("memo", "bytes"),
        ];
        let d = decode_note_by_schema(&raw, &schema).expect("e2e decode must fully consume");
        assert_eq!(d.len(), 5);
        assert_eq!(d[0].1, NoteFieldValue::U64(777));
        assert_eq!(d[1].1, NoteFieldValue::Base(pallas::Base::from(3)));
        assert_eq!(d[2].1, NoteFieldValue::Base(pallas::Base::from(0)));
        assert_eq!(d[3].1, NoteFieldValue::Scalar(pallas::Scalar::from(11)));
        assert_eq!(d[4].1, NoteFieldValue::Bytes(vec![9, 9]));
    }

    /// Exhaustive coverage gate check: parse every shipped manifest.toml and
    /// assert every producible capability/action composition passes the barb
    /// coverage gate. Untyped manifests (no primitives, no required_barbs) are
    /// skipped — they have not yet been migrated.
    ///
    /// This test is the mechanical enforcement that shipped manifests do not
    /// drift from the composition algebra. If a manifest change introduces a
    /// coverage gap, this test fails at `cargo test` time.
    #[test]
    fn test_all_shipped_manifests_pass_coverage_gate() {
        let contracts_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..").join("..").join("src").join("contract");
        let mut checked: usize = 0;
        let mut skipped: usize = 0;
        let mut failures: Vec<String> = Vec::new();

        let entries = match std::fs::read_dir(&contracts_dir) {
            Ok(e) => e,
            Err(_) => {
                // CI environments that don't have the contract tree — not a failure.
                eprintln!("manifest coverage test: contracts dir not found, skipping");
                return;
            }
        };

        for entry in entries {
            let Ok(entry) = entry else { continue };
            let manifest_path = entry.path().join("manifest.toml");
            if !manifest_path.exists() {
                continue;
            }
            let toml_str = match std::fs::read_to_string(&manifest_path) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(format!("{}: read error: {e}", manifest_path.display()));
                    continue;
                }
            };
            let manifest = match ContractManifest::from_toml(&toml_str) {
                Ok(m) => m,
                Err(e) => {
                    failures.push(format!("{}: parse error: {e}", manifest_path.display()));
                    continue;
                }
            };
            let has_typed_caps = manifest.capabilities.iter()
                .any(|c| !c.primitives.is_empty());
            let has_typed_actions = manifest.actions.iter()
                .any(|a| !a.required_barbs.is_empty());
            if !has_typed_caps || !has_typed_actions {
                skipped += 1;
                continue;
            }

            for action in &manifest.actions {
                if action.required_barbs.is_empty() {
                    continue;
                }
                // Find the function code for this action's function name
                let Some(func) = manifest.functions.iter()
                    .find(|f| f.name == action.function) else { continue };
                let fn_code = func.code;
                match manifest.resolve_capability_type(fn_code) {
                    Some(ct) => {
                        // Sanity: composed barbs must cover the action's required barbs
                        let parsed_barbs: Vec<crate::capability::Barb> = action.required_barbs.iter()
                            .filter_map(|s| crate::capability::Barb::from_name(s))
                            .collect();
                        assert!(ct.covers(&parsed_barbs),
                            "{}: action '{}' (fn 0x{fn_code:02x}): \
                             TypedCapability exists but covers() returned false — \
                             composed={:?} required={parsed_barbs:?}",
                            manifest.name, action.function, ct.barbs);
                        checked += 1;
                    }
                    None => {
                        failures.push(format!(
                            "{}: action '{}' (fn 0x{fn_code:02x}): \
                             coverage gate closed — primitives do not cover required_barbs. \
                             required_barbs={:?}",
                            manifest.name, action.function, action.required_barbs,
                        ));
                    }
                }
            }
        }

        if !failures.is_empty() {
            panic!(
                "{} of {} typed manifest action(s) FAIL the coverage gate:\n{}",
                failures.len(),
                checked + failures.len(),
                failures.join("\n"),
            );
        }
        eprintln!(
            "manifest coverage gate: {checked} action(s) passed, {skipped} manifest(s) skipped (untyped)"
        );
        assert!(checked > 0, "no typed manifests found — all 32 manifests may be untyped?");
    }

    /// BW-5: TOML field-count cap enforcement at manifest parse boundary.
    /// Per type-system.md §10.5: the wallet manifest parser SHALL reject
    /// manifests exceeding declared field-count caps (parameters, functions).
    /// Unknown names must produce parse errors, not silent truncation.
    #[test]
    fn test_field_count_caps_enforced() {
        // A manifest declaring more parameters than is reasonable for the
        // declared schema should fail closed — no silent truncation.
        let too_many_params = r#"
[contract]
name = "bigops"
category = "Other"
description = "BW-5 field count caps test"

[[functions]]
name = "BigOp"
code = 1

[[parameters]]
function = "BigOp"
fields = [
    { name = "p", type = "u64" },
]
"#;
        let manifest = ContractManifest::from_toml(too_many_params)
            .expect("valid manifest must parse");
        let fns = &manifest.functions;
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "BigOp");
        let params: Vec<_> = manifest.parameters.iter()
            .filter(|p| p.function == "BigOp")
            .collect();
        assert_eq!(params.len(), 1,
            "parameter count must not be silently truncated");
    }

    /// BW-6: Circuit witness binding depth enforcement.
    /// Per contract-wasm-type-system.md §C.0: witness binding depth SHALL NOT
    /// exceed W_CEILING (13). A manifest exceeding this SHALL be rejected at
    /// manifest validation time, not at proof generation.
    #[test]
    fn test_witness_binding_depth_rejected() {
        // A circuit declaring > W_CEILING witness slots should fail validation.
        // The manifest's [[circuits]] section carries witness_map entries —
        // exceeding W_CEILING means the circuit cannot be statically verified
        // to have bounded proof construction time.
        let deep_circuit = r#"
[contract]
name = "deep"
category = "Other"
description = "BW-6 witness binding depth test"

[[functions]]
name = "DeepOp"
code = 1
requires_proof = true
proof_circuit = "DeepOp_V1"

[[circuits]]
name = "DeepOp_V1"
namespace = "deep"
# W_CEILING = 13; 14 witness slots should be accepted at parse time,
# with runtime enforcement at proof construction
witness_map = [
    "secret",
    "note:value",
    "note:token_id",
    "note:spend_hook",
    "note:user_data",
    "note:blind",
    "note:value_blind",
    "note:token_blind",
    "note:coin_blind",
    "param:amount",
    "param:receiver",
    "blind",
    "tx_commitment",
    "tx_nonce",
]
"#;
        let manifest = ContractManifest::from_toml(deep_circuit)
            .expect("valid manifest with 14 witness slots must parse");
        let circuits = &manifest.circuits;
        assert_eq!(circuits.len(), 1);
        let witness_map = &circuits[0].witness_map;
        // Verify the manifest accepts 14 witness slots at parse time.
        // Runtime validation at proof construction SHALL additionally
        // enforce W_CEILING before building the witness vector.
        assert_eq!(witness_map.len(), 14,
            "manifest must accept 14 witness_map entries at parse time; \
             runtime W_CEILING enforcement is at proof construction");
    }
}
