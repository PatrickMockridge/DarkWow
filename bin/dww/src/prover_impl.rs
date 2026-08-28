/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Generic prover — wallet-side concrete implementation (wallet.md §6.4.1).
//!
//! The capability SDK (`dwow_sdk::prover`) defines the API; this module provides
//! the concrete proof-creation that needs `dwow_core::zk` types. The wallet
//! resolves capabilities, loads zkas binaries from the store (§3), and delegates
//! to this module to bind witnesses and create proofs.
//!
//! Witness binding is positional and manifest-declared (`witness_map`). Three
//! source categories (wallet.md §6.4.1): input (`note:`, `param:`, `secret`,
//! `merkle_path`, `leaf_position`, `tx_*`), named blind (`blind:<name>`), and
//! derived (`derived:<rule>:<slot>…`). Derived witnesses are computed with the
//! same SDK crypto primitives the native_token client uses — no per-contract Rust.

use dwow_core::zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit};
use dwow_core::zkas::{Opcode, VarType, ZkBinary};
use dwow_sdk::crypto::constants::{
    DRK_POSEIDON_DOMAIN_COMMITMENT, DRK_POSEIDON_DOMAIN_MERKLE_LEAF,
    DRK_POSEIDON_DOMAIN_NULLIFIER, DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET,
    DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING, MERKLE_DEPTH_ORCHARD,
};
use dwow_sdk::crypto::pasta_prelude::{Curve, CurveAffine, PrimeField};
use dwow_sdk::crypto::util::hash_to_base;
use dwow_sdk::crypto::{pedersen_commitment_u64, poseidon_hash, Blind, MerkleNode, SecretKey};
use dwow_sdk::manifest::NoteFieldValue;
use dwow_sdk::pasta::pallas;
use dwow_sdk::prover::{parse_public_input, CapabilityProvider, DerivedRule, ProverContext, PublicInputSource, WitnessSource};
use rand::SeedableRng;

/// The raw value of a bound witness slot, retained so derived rules (which
/// reference earlier slots by 0-based index) can read their operands.
#[derive(Debug, Clone)]
enum SlotValue {
    Base(pallas::Base),
    Scalar(pallas::Scalar),
    U64(u64),
    U32(u32),
    /// A bound MerklePath witness (leaf_pos/merkle_path travel on the wire as
    /// `leaf_pos`/`merkle_path` params, not as derived operands).
    MerklePath([MerkleNode; MERKLE_DEPTH_ORCHARD]),
}

impl SlotValue {
    fn as_base(&self) -> Result<pallas::Base, String> {
        match self {
            SlotValue::Base(b) => Ok(*b),
            SlotValue::U64(u) => Ok(pallas::Base::from(*u)),
            other => Err(format!("derived operand is not a base field element: {other:?}")),
        }
    }

    fn as_scalar(&self) -> Result<pallas::Scalar, String> {
        match self {
            SlotValue::Scalar(s) => Ok(*s),
            other => Err(format!("derived operand is not a scalar: {other:?}")),
        }
    }

    fn as_u64(&self) -> Result<u64, String> {
        match self {
            SlotValue::U64(u) => Ok(*u),
            SlotValue::Base(b) => Ok(base_to_u64(*b)),
            other => Err(format!("derived operand is not a u64: {other:?}")),
        }
    }

    fn to_witness(&self, vartype: &VarType) -> Result<Witness, String> {
        match vartype {
            VarType::Base => Ok(Witness::Base(Value::known(self.as_base()?))),
            VarType::Scalar => Ok(Witness::Scalar(Value::known(self.as_scalar()?))),
            VarType::Uint32 => Ok(Witness::Uint32(Value::known(match self {
                SlotValue::U32(u) => *u,
                other => return Err(format!("slot is not a u32: {other:?}")),
            }))),
            VarType::Uint64 => Ok(Witness::Uint64(Value::known(self.as_u64()?))),
            other => Err(format!("unsupported witness VarType {other:?}")),
        }
    }
}

/// Concrete capability provider — pre-resolved by the wallet before calling
/// the generic prover. Holds the decrypted note fields (typed, from the
/// manifest's `note_schema`), the spending secret, any named secrets, action
/// parameters, the Merkle proof, and the leaf position.
pub struct ResolvedCapProvider {
    note_fields: Vec<(String, NoteFieldValue)>,
    secret: SecretKey,
    named_secrets: Vec<(String, SecretKey)>,
    params: Vec<(String, NoteFieldValue)>,
    merkle_path: Vec<pallas::Base>,
    merkle_root: pallas::Base,
    leaf_position: u32,
}

impl ResolvedCapProvider {
    /// Construct from pre-resolved note fields, secret, Merkle proof, and leaf
    /// position. Named secrets, parameters, and Merkle root default empty/zero;
    /// use the builder methods for multi-secret / parameterised / anchored
    /// circuits.
    pub fn new(
        note_fields: Vec<(String, NoteFieldValue)>,
        secret: SecretKey,
        merkle_path: Vec<pallas::Base>,
        leaf_position: u32,
    ) -> Self {
        Self {
            note_fields,
            secret,
            named_secrets: Vec::new(),
            params: Vec::new(),
            merkle_path,
            merkle_root: pallas::Base::zero(),
            leaf_position,
        }
    }

    /// Attach named spending secrets for multi-secret circuits.
    pub fn with_named_secrets(mut self, named_secrets: Vec<(String, SecretKey)>) -> Self {
        self.named_secrets = named_secrets;
        self
    }

    /// Attach action `[[parameters]]` values (typed, from `encode_params_by_schema`).
    pub fn with_params(mut self, params: Vec<(String, NoteFieldValue)>) -> Self {
        self.params = params;
        self
    }

    /// Attach the Merkle root that anchored this capability (wallet's stored proof).
    pub fn with_merkle_root(mut self, merkle_root: pallas::Base) -> Self {
        self.merkle_root = merkle_root;
        self
    }
}

impl CapabilityProvider for ResolvedCapProvider {
    fn note_value(&self, name: &str) -> Option<NoteFieldValue> {
        self.note_fields.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    fn secret(&self) -> SecretKey {
        self.secret.clone()
    }

    fn named_secret(&self, name: &str) -> Option<SecretKey> {
        self.named_secrets.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    fn merkle_path(&self) -> Vec<pallas::Base> {
        self.merkle_path.clone()
    }

    fn merkle_root(&self) -> pallas::Base {
        self.merkle_root
    }

    fn leaf_position(&self) -> u32 {
        self.leaf_position
    }

    fn param_value(&self, name: &str) -> Option<NoteFieldValue> {
        self.params.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }
}

/// The generic proof-creation function — wallet.md §6.4.1 steps 4-6.
///
/// Given the prover context, a capability provider, and the zkas binary loaded
/// from the store, bind every witness slot per the manifest's `witness_map` and
/// create the ZK proof. All randomness is derived from `ctx.seed` (§6.1).
pub fn create_generic_proof(
    ctx: &ProverContext,
    provider: &dyn CapabilityProvider,
    zkas_bytes: &[u8],
) -> Result<(Vec<u8>, Vec<Option<NoteFieldValue>>), String> {
    // Step 4: decode the zkas binary → ordered witness list
    let zkbin = ZkBinary::decode(zkas_bytes, false)
        .map_err(|e| format!("ZkBinary::decode: {:?}", e))?;
    let witness_count = zkbin.witnesses.len();

    // Arity: the witness_map must cover every slot exactly (no unbound slot).
    ctx.witness_map
        .validate_count(witness_count)
        .map_err(|e| format!("witness_map arity: {e}"))?;

    // Step 5: two-pass binding. Derived rules may reference ANY input slot,
    // including ones that appear LATER in the witness order (e.g. box put's
    // nullifier reads owner_secret at slot 8, but nullifier is slot 5). Pass 1
    // binds every input source; pass 2 computes the derived slots in order.
    let mut witnesses: Vec<Option<Witness>> = vec![None; witness_count];
    let mut bound: Vec<Option<SlotValue>> = vec![None; witness_count];

    for (idx, source) in ctx.witness_map.entries.iter().enumerate() {
        if matches!(source, WitnessSource::Derived(_)) {
            continue;
        }
        let vartype = &zkbin.witnesses[idx];
        let (witness, slot) = bind_slot(idx, source, vartype, provider, &bound, ctx.seed)?;
        witnesses[idx] = Some(witness);
        bound[idx] = slot;
    }

    for (idx, source) in ctx.witness_map.entries.iter().enumerate() {
        if !matches!(source, WitnessSource::Derived(_)) {
            continue;
        }
        let vartype = &zkbin.witnesses[idx];
        let (witness, slot) = bind_slot(idx, source, vartype, provider, &bound, ctx.seed)?;
        witnesses[idx] = Some(witness);
        bound[idx] = slot;
    }

    let witnesses: Vec<Witness> = witnesses
        .into_iter()
        .map(|w| w.ok_or_else(|| "unbound witness slot after two-pass binding".to_string()))
        .collect::<Result<_, _>>()?;

    // Public inputs: the circuit's `constrain_instance` targets. If the manifest
    // declares `public_inputs` (intermediate targets), evaluate them; else derive
    // from the witness-slot `constrain_instance` opcodes (box/purse).
    let instances = evaluate_public_inputs(ctx, &zkbin, &bound, witness_count)?;

    // Step 6: build proving key (cacheable per circuit — not yet cached) →
    // create proof with Seed-derived RNG.
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit)
        .map_err(|e| format!("ProvingKey::build: {:?}", e))?;
    let mut rng = rand::rngs::StdRng::from_seed(ctx.seed);
    let proof = Proof::create(&pk, &[circuit], &instances, &mut rng)
        .map_err(|e| format!("Proof::create: {:?}", e))?;

    // Return the RAW proof bytes (the transcript), NOT the length-prefixed
    // `Encodable` encoding. Callers wrap these bytes with `Proof::new(bytes)`,
    // and the tx witness then length-prefixes them once more at serialization.
    // Returning the `Encodable` form here double-prefixes the proof, so the
    // verifier's transcript starts with a spurious VarInt and `verify_zkp`
    // fails with "invalid proof" (T1 wire congruence — proof bytes, not params).
    let proof_bytes = proof.as_ref().to_vec();

    // The circuit-computed (derived / merkle_root / merkle_path) values, keyed by
    // witness slot index — returned typed (Base/Scalar/U64/U32/MerklePath) so the
    // caller can inject them into the wire params and the produce-side note.
    let bound_values: Vec<Option<NoteFieldValue>> = bound.iter().map(|s| {
        s.as_ref().map(|v| match v {
            SlotValue::Base(b) => NoteFieldValue::Base(*b),
            SlotValue::Scalar(s) => NoteFieldValue::Scalar(*s),
            SlotValue::U64(u) => NoteFieldValue::U64(*u),
            SlotValue::U32(u) => NoteFieldValue::U32(*u),
            SlotValue::MerklePath(arr) => NoteFieldValue::MerklePath(
                arr.iter().flat_map(|n| n.to_bytes()).collect(),
            ),
        })
    }).collect();

    Ok((proof_bytes, bound_values))
}

/// Bind one witness slot to its source, producing the `Witness` (for the
/// circuit) and the raw `SlotValue` (for derived-rule operands and public-input
/// extraction). A Merkle-path slot carries no `SlotValue` (it is never a
/// derived operand or public input).
fn bind_slot(
    idx: usize,
    source: &WitnessSource,
    vartype: &VarType,
    provider: &dyn CapabilityProvider,
    bound: &[Option<SlotValue>],
    seed: [u8; 32],
) -> Result<(Witness, Option<SlotValue>), String> {
    match source {
        WitnessSource::NoteField(field) => {
            let nv = provider
                .note_value(field)
                .ok_or_else(|| format!("witness[{idx}]: note field '{field}' not found"))?;
            let slot = coerce_note(&nv, vartype, idx)?;
            Ok((slot.to_witness(vartype)?, Some(slot)))
        }
        WitnessSource::ParamField(field) => {
            let nv = provider
                .param_value(field)
                .ok_or_else(|| format!("witness[{idx}]: param field '{field}' not found"))?;
            let slot = coerce_note(&nv, vartype, idx)?;
            Ok((slot.to_witness(vartype)?, Some(slot)))
        }
        WitnessSource::Secret => {
            let b = *provider.secret().inner();
            Ok((Witness::Base(Value::known(b)), Some(SlotValue::Base(b))))
        }
        WitnessSource::SecretNamed(name) => {
            let sk = provider
                .named_secret(name)
                .ok_or_else(|| format!("witness[{idx}]: named secret '{name}' not found"))?;
            let b = *sk.inner();
            Ok((Witness::Base(Value::known(b)), Some(SlotValue::Base(b))))
        }
        WitnessSource::MerklePath
        | WitnessSource::MerklePathCurrent
        | WitnessSource::MerklePathCumulative => {
            let arr = merkle_path_array(provider)?;
            Ok((Witness::MerklePath(Value::known(arr)), Some(SlotValue::MerklePath(arr))))
        }
        WitnessSource::MerkleRoot => {
            let root = provider.merkle_root();
            Ok((Witness::Base(Value::known(root)), Some(SlotValue::Base(root))))
        }
        WitnessSource::LeafPosition => {
            let pos = provider.leaf_position();
            Ok((Witness::Uint32(Value::known(pos)), Some(SlotValue::U32(pos))))
        }
        WitnessSource::Blind(name) => {
            let b = derive_blind(seed, name);
            match vartype {
                VarType::Scalar => {
                    let s = base_to_scalar(b)?;
                    Ok((Witness::Scalar(Value::known(s)), Some(SlotValue::Scalar(s))))
                }
                VarType::Base => Ok((Witness::Base(Value::known(b)), Some(SlotValue::Base(b)))),
                other => Err(format!(
                    "witness[{idx}]: blind:<name> binds Base or Scalar, got {other:?}"
                )),
            }
        }
        WitnessSource::TxCommitment | WitnessSource::TxNonce => {
            // Single-call transaction binding is zero; multi-call binding is a
            // follow-on (the caller supplies the binding names).
            let z = pallas::Base::zero();
            Ok((Witness::Base(Value::known(z)), Some(SlotValue::Base(z))))
        }
        WitnessSource::Derived(rule) => {
            let slot = compute_derived(rule, bound, idx)?;
            Ok((slot.to_witness(vartype)?, Some(slot)))
        }
    }
}

/// Coerce a typed note/param value into the slot's declared `VarType`.
fn coerce_note(nv: &NoteFieldValue, vartype: &VarType, idx: usize) -> Result<SlotValue, String> {
    match (vartype, nv) {
        (VarType::Base, NoteFieldValue::Base(b)) => Ok(SlotValue::Base(*b)),
        (VarType::Base, NoteFieldValue::U64(u)) => Ok(SlotValue::Base(pallas::Base::from(*u))),
        (VarType::Scalar, NoteFieldValue::Scalar(s)) => Ok(SlotValue::Scalar(*s)),
        (VarType::Uint32, NoteFieldValue::U64(u)) => Ok(SlotValue::U32(*u as u32)),
        (VarType::Uint64, NoteFieldValue::U64(u)) => Ok(SlotValue::U64(*u)),
        _ => Err(format!(
            "witness[{idx}]: note/param type mismatch (slot {vartype:?} vs value {nv:?})"
        )),
    }
}

/// Apply a closed `derived:<rule>` using the already-bound operands.
fn compute_derived(
    rule: &DerivedRule,
    bound: &[Option<SlotValue>],
    idx: usize,
) -> Result<SlotValue, String> {
    let base = |i: usize| -> Result<pallas::Base, String> {
        operand(bound, i, idx)?.as_base()
    };
    let scalar = |i: usize| -> Result<pallas::Scalar, String> {
        operand(bound, i, idx)?.as_scalar()
    };
    let u64val = |i: usize| -> Result<u64, String> { operand(bound, i, idx)?.as_u64() };

    Ok(match rule {
        DerivedRule::Nullifier { secret, id, nonce } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_NULLIFIER,
            base(*secret)?,
            base(*id)?,
            base(*nonce)?,
        ])),
        DerivedRule::TxBinding { txc, txn } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            base(*txc)?,
            base(*txn)?,
        ])),
        DerivedRule::Leaf { id, contents, nonce } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_MERKLE_LEAF,
            base(*id)?,
            base(*contents)?,
            base(*nonce)?,
        ])),
        DerivedRule::MerkleRoot { .. } => {
            // expected_root/merkle_root is carried in the L1 note (§C.8.2), so it
            // is bound as `note:merkle_root` — never derived. Reject rather than
            // fabricate a root without the sibling path.
            return Err(format!(
                "witness[{idx}]: derived:merkle_root is unsupported — bind the root as \
                 note:merkle_root (the L1 note carries it, wallet.md §2.3 / contract-wasm-type-system.md §C.8.2)"
            ))
        }
        DerivedRule::OwnerPub { secret } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET,
            base(*secret)?,
        ])),
        DerivedRule::TokenCommit { asset_id, blind } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_TOKEN_COMMIT,
            base(*asset_id)?,
            base(*blind)?,
        ])),
        DerivedRule::PurseId { owner_pub, asset_id, purse_id } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_COMMITMENT,
            base(*owner_pub)?,
            base(*asset_id)?,
            base(*purse_id)?,
        ])),
        DerivedRule::Coin { coin_public, value, asset_id, spend_hook, user_data, blind } => {
            SlotValue::Base(poseidon_hash([
                DRK_POSEIDON_DOMAIN_COMMITMENT,
                base(*coin_public)?,
                base(*value)?,
                base(*asset_id)?,
                base(*spend_hook)?,
                base(*user_data)?,
                base(*blind)?,
            ]))
        }
        DerivedRule::PedersenX { value, blind } => {
            let pt = pedersen_commitment_u64(u64val(*value)?, Blind(scalar(*blind)?));
            SlotValue::Base(pedersen_coord(pt, true)?)
        }
        DerivedRule::PedersenY { value, blind } => {
            let pt = pedersen_commitment_u64(u64val(*value)?, Blind(scalar(*blind)?));
            SlotValue::Base(pedersen_coord(pt, false)?)
        }
        DerivedRule::BaseAdd { a, b } => SlotValue::Base(base(*a)? + base(*b)?),
        DerivedRule::BaseSub { a, b } => SlotValue::Base(base(*a)? - base(*b)?),
        DerivedRule::BlindSum { a, b } => SlotValue::Scalar(scalar(*a)? + scalar(*b)?),
        DerivedRule::BlindSub { a, b } => SlotValue::Scalar(scalar(*a)? - scalar(*b)?),
        DerivedRule::SignatureSecret { secret, nullifier } => SlotValue::Base(poseidon_hash([
            DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET,
            base(*secret)?,
            base(*nullifier)?,
        ])),
    })
}

/// Read a bound operand slot by index, with a clear error on forward/out-of-range.
fn operand<'a>(
    bound: &'a [Option<SlotValue>],
    i: usize,
    idx: usize,
) -> Result<&'a SlotValue, String> {
    bound.get(i).and_then(|s| s.as_ref()).ok_or_else(|| {
        format!("witness[{idx}]: derived operand slot {i} is unbound or out of range")
    })
}

/// Extract the public inputs (the `constrain_instance` targets) in opcode
/// order. Witness slots occupy heap indices 0..witness_count; a public input
/// that references an intermediate (heap index ≥ witness_count) is a circuit
/// whose constrain_instance targets are not witness slots — those need the
/// manifest `public_inputs` declaration (follow-on).
fn extract_instances(
    zkbin: &ZkBinary,
    bound: &[Option<SlotValue>],
    witness_count: usize,
) -> Result<Vec<pallas::Base>, String> {
    let mut instances = Vec::new();
    for (opcode, args) in &zkbin.opcodes {
        if !matches!(opcode, Opcode::ConstrainInstance) {
            continue
        }
        let heap_idx = args
            .first()
            .map(|(_, i)| *i)
            .ok_or_else(|| "ConstrainInstance opcode has no operand".to_string())?;
        if heap_idx >= witness_count {
            return Err(format!(
                "public input references intermediate heap slot {heap_idx} (>= {witness_count}); \
                 circuits whose constrain_instance targets are intermediates require the manifest \
                 public_inputs declaration (not yet implemented)"
            ))
        }
        let slot = bound[heap_idx]
            .as_ref()
            .ok_or_else(|| format!("public input heap slot {heap_idx} is unbound"))?;
        instances.push(slot.as_base()?);
    }
    Ok(instances)
}

/// Evaluate the circuit's public inputs (wallet.md §6.4.1 invariant 6): if the
/// manifest declares `public_inputs` (intermediate `constrain_instance` targets),
/// evaluate each `slot:<idx>` / `derived:<rule>:<slots>` entry in order; else
/// derive from the witness-slot `constrain_instance` opcodes (box/purse).
fn evaluate_public_inputs(
    ctx: &ProverContext,
    zkbin: &ZkBinary,
    bound: &[Option<SlotValue>],
    witness_count: usize,
) -> Result<Vec<pallas::Base>, String> {
    let declared = ctx.manifest.circuits.iter()
        .find(|c| c.name == ctx.witness_map.circuit_name)
        .map(|c| c.public_inputs.as_slice())
        .unwrap_or(&[]);
    if declared.is_empty() {
        return extract_instances(zkbin, bound, witness_count)
    }
    let mut instances = Vec::with_capacity(declared.len());
    for (i, entry) in declared.iter().enumerate() {
        let source = parse_public_input(entry)
            .map_err(|e| format!("public_input[{i}] '{entry}': {e}"))?;
        let base = match source {
            PublicInputSource::Slot(idx) => bound.get(idx).and_then(|s| s.as_ref())
                .ok_or_else(|| format!("public_input[{i}]: slot {idx} unbound"))?
                .as_base()?,
            PublicInputSource::Derived(rule) =>
                compute_derived(&rule, bound, i)?.as_base()?,
        };
        instances.push(base);
    }
    Ok(instances)
}

/// Build the fixed-depth `[MerkleNode; 32]` Merkle path, padding a short path
/// with the Sinsemilla `empty_root(altitude)` values (NOT zeros) — the VM
/// `merkle_root` opcode hashes against empty roots, so a zero pad would produce
/// a wrong root (T5 merkle-path congruence).
fn merkle_path_array(provider: &dyn CapabilityProvider) -> Result<[MerkleNode; MERKLE_DEPTH_ORCHARD], String> {
    use dwow_sdk::bridgetree::{Hashable, Level};
    let mut path = provider.merkle_path();
    if path.len() > MERKLE_DEPTH_ORCHARD {
        return Err(format!(
            "merkle path has {} elements, expected at most {MERKLE_DEPTH_ORCHARD}",
            path.len()
        ))
    }
    while path.len() < MERKLE_DEPTH_ORCHARD {
        let lvl = path.len();
        path.push(MerkleNode::empty_root(Level::from(lvl as u8)).inner());
    }
    let nodes: Vec<MerkleNode> = path.into_iter().map(MerkleNode::new).collect();
    nodes.try_into().map_err(|_| "merkle path conversion failed".to_string())
}

/// Derive a named blind from Seed with a distinct per-name domain (§6.1).
fn derive_blind(seed: [u8; 32], name: &str) -> pallas::Base {
    hash_to_base(b"darkwow-blind", &[name.as_bytes(), &seed])
}

fn base_to_scalar(b: pallas::Base) -> Result<pallas::Scalar, String> {
    Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(b.to_repr()))
        .ok_or_else(|| "blind: non-canonical scalar".to_string())
}

fn base_to_u64(b: pallas::Base) -> u64 {
    let repr = b.to_repr();
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&repr[..8]);
    u64::from_le_bytes(arr)
}

fn pedersen_coord(pt: pallas::Point, x: bool) -> Result<pallas::Base, String> {
    let coords = pt.to_affine().coordinates();
    if coords.is_none().into() {
        return Err("pedersen: identity point".to_string())
    }
    // CtOption::unwrap is not flagged by clippy::unwrap_used (targets std
    // Option/Result only, type-system.md §2.3.4); identity is checked above.
    let c = coords.unwrap();
    Ok(if x { *c.x() } else { *c.y() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_core::zk::{verify_zkp, ZkVerifyResult};
    use dwow_sdk::crypto::MerkleTree;
    use dwow_sdk::manifest::ContractManifest;
    use dwow_sdk::prover::CircuitWitnessMap;

    /// T5 merkle-triple congruence — the decisive unit test. Build the box `put`
    /// through the generic prover with a provider mirroring the wallet's exact
    /// state, then verify the emitted proof against independently-computed public
    /// inputs (the circuit's `constrain_instance` values). This proves the generic
    /// prover emits a valid proof for the merkle triple the wallet feeds it.
    #[test]
    fn box_put_generic_prover_emits_valid_proof() {
        let zkbin_bytes = include_bytes!("../../../src/contract/box/proof/put.zk.bin");
        let manifest = ContractManifest::from_toml(include_str!(
            "../../../src/contract/box/manifest.toml"
        ))
        .expect("parse box manifest");
        let circuit = manifest
            .circuits
            .iter()
            .find(|c| c.name == "Put")
            .expect("Put circuit");
        let witness_map = CircuitWitnessMap::from_manifest(
            circuit.name.clone(),
            circuit.namespace.clone(),
            &circuit.witness_map,
        )
        .expect("witness map");

        // Domain constants, matching put.zk `witness_base(1/3/5)`.
        let dnl = pallas::Base::from(1u64);
        let dtb = pallas::Base::from(3u64);
        let dml = pallas::Base::from(5u64);
        let secret = SecretKey::from_base(pallas::Base::from(42u64));
        let bid = pallas::Base::from(1u64);
        let osn = pallas::Base::from(1u64); // old_state_nonce
        let nsn = pallas::Base::from(2u64); // new_state_nonce
        let occ = poseidon_hash([pallas::Base::from(100u64)]); // old_contents_commit
        let ncc = poseidon_hash([pallas::Base::from(200u64)]); // new_contents_commit
        let tn = pallas::Base::zero(); // tx_nonce

        // old_leaf = poseidon(5, bid, occ, osn) — the seed put's new_leaf.
        let old_leaf = poseidon_hash([dml, bid, occ, osn]);

        // Reconstruct the contract tree [zero, old_leaf] exactly as
        // walletdb::reconstruct_contract_tree does.
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(old_leaf));
        let mk = tree.mark().expect("mark");
        let leaf_pos = u32::try_from(u64::from(mk)).expect("position");
        let path: Vec<pallas::Base> = tree
            .witness(mk, 0)
            .expect("witness")
            .iter()
            .map(|n| n.inner())
            .collect();
        let root = tree.root(0).expect("root").inner();

        let provider = ResolvedCapProvider::new(vec![], secret.clone(), path, leaf_pos)
            .with_merkle_root(root)
            .with_params(vec![
                ("box_id".to_string(), NoteFieldValue::Base(bid)),
                ("old_state_nonce".to_string(), NoteFieldValue::Base(osn)),
                ("new_state_nonce".to_string(), NoteFieldValue::Base(nsn)),
                ("old_contents_commit".to_string(), NoteFieldValue::Base(occ)),
                ("new_contents_commit".to_string(), NoteFieldValue::Base(ncc)),
                ("tx_nonce".to_string(), NoteFieldValue::Base(tn)),
            ]);

        let ctx = ProverContext::new(manifest, "Put".to_string(), witness_map, [7u8; 32]);
        let (proof_bytes, _bound) = create_generic_proof(&ctx, &provider, zkbin_bytes)
            .expect("create_generic_proof box put");
        // `Proof::new` wraps the RAW transcript bytes, exactly as the e2e test
        // and the tx witness do — this is what surfaces a length-prefix bug.
        let proof = Proof::new(proof_bytes);

        // Independently-computed public inputs, in constrain_instance order
        // [nullifier, expected_root, new_leaf, tx_binding, tx_nonce].
        let expected = vec![
            poseidon_hash([dnl, *secret.inner(), bid, osn]), // nullifier
            root,                                           // expected_root
            poseidon_hash([dml, bid, ncc, nsn]),            // new_leaf
            poseidon_hash([dtb, pallas::Base::zero(), tn]), // tx_binding
            tn,                                             // tx_nonce
        ];

        assert_eq!(
            verify_zkp(&proof, zkbin_bytes, &expected),
            ZkVerifyResult::Ok,
            "generic prover box put proof must verify against independently-computed public inputs"
        );
    }

    /// T5 — the wallet's `reconstruct_contract_tree` build sequence (append seed,
    /// `mark()`, append leaf, `mark()`, `current_position()`) MUST produce the
    /// same `(pos, path, root)` triple as the harness `build_root` (append seed,
    /// append leaf, single `mark()`). If these diverge, the wallet feeds the
    /// circuit a position/path/root from a different tree than it reconstructs.
    #[test]
    fn reconstruct_pattern_matches_harness_build_root() {
        let leaf = poseidon_hash([
            pallas::Base::from(5u64),
            pallas::Base::from(1u64),
            poseidon_hash([pallas::Base::from(100u64)]),
            pallas::Base::from(1u64),
        ]);

        // walletdb::reconstruct_contract_tree sequence.
        let mut w = MerkleTree::new(1);
        w.append(MerkleNode::from_base(pallas::Base::zero()));
        w.mark();
        w.append(MerkleNode::from_base(leaf));
        w.mark();
        let w_pos = w.current_position().expect("pos");
        let w_path: Vec<pallas::Base> = w
            .witness(w_pos, 0)
            .expect("witness")
            .iter()
            .map(|n| n.inner())
            .collect();
        let w_root = w.root(0).expect("root").inner();

        // harness build_root sequence.
        let mut h = MerkleTree::new(1);
        h.append(MerkleNode::from_base(pallas::Base::zero()));
        h.append(MerkleNode::from_base(leaf));
        let h_mk = h.mark().expect("mark");
        let h_path: Vec<pallas::Base> = h
            .witness(h_mk, 0)
            .expect("witness")
            .iter()
            .map(|n| n.inner())
            .collect();
        let h_root = h.root(0).expect("root").inner();

        assert_eq!(u64::from(w_pos), u64::from(h_mk), "position");
        assert_eq!(w_path, h_path, "merkle path");
        assert_eq!(w_root, h_root, "merkle root");
    }

    /// T5 padding — `merkle_path_array` SHALL pad a short path with Sinsemilla
    /// `empty_root(altitude)` values, not zeros (the VM `merkle_root` opcode
    /// hashes against empty roots, so a zero pad produces a wrong root).
    #[test]
    fn merkle_path_array_pads_with_sinsemilla_empty_roots() {
        // Build a tree with a single leaf at position 0; its witness path is the
        // canonical 32-element Sinsemilla path (leaf sibling + empty roots).
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::from(9u64)));
        tree.mark();
        let full: Vec<pallas::Base> = tree
            .witness(tree.current_position().expect("pos"), 0)
            .expect("witness")
            .iter()
            .map(|n| n.inner())
            .collect();
        assert_eq!(full.len(), MERKLE_DEPTH_ORCHARD);

        // A provider that carries only the FIRST sibling (a short path).
        let short = vec![full[0]];
        let provider = ResolvedCapProvider::new(
            vec![],
            SecretKey::from_base(pallas::Base::from(42u64)),
            short,
            0,
        );
        let padded = merkle_path_array(&provider).expect("pad");
        assert_eq!(padded.len(), MERKLE_DEPTH_ORCHARD);
        // The padded tail must equal the Sinsemilla empty roots, not zeros.
        for (i, node) in padded.iter().enumerate().skip(1) {
            assert_eq!(
                node.inner(),
                full[i],
                "merkle_path_array padded tail[{i}] must be the Sinsemilla empty root"
            );
        }
    }
}
