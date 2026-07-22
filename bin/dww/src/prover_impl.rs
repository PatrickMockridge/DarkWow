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

use dwow_core::zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::pasta::pallas;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::prover::{CapabilityProvider, ProverContext, WitnessSource};
use rand::SeedableRng;

/// Concrete capability provider — pre-resolved by the wallet before calling
/// the generic prover. Holds note fields (decoded from the manifest's
/// note_schema or NativeToken), the spending secret, merkle proof, and leaf
/// position. The wallet resolves all of these before proof creation.
pub struct ResolvedCapProvider {
    /// Note fields keyed by field name → raw pallas::Base (or u64 as Base).
    /// Populated by the caller from the manifest's note_schema decoder or
    /// the hardcoded NativeToken format.
    note_fields: Vec<(String, pallas::Base)>,
    secret: dwow_sdk::crypto::SecretKey,
    merkle_path: Vec<pallas::Base>,
    leaf_position: u32,
}

impl ResolvedCapProvider {
    /// Construct from pre-resolved data.
    pub fn new(
        note_fields: Vec<(String, pallas::Base)>,
        secret: dwow_sdk::crypto::SecretKey,
        merkle_path: Vec<pallas::Base>,
        leaf_position: u32,
    ) -> Self {
        Self { note_fields, secret, merkle_path, leaf_position }
    }
}

impl CapabilityProvider for ResolvedCapProvider {
    fn note_field(&self, name: &str) -> Option<pallas::Base> {
        self.note_fields.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    fn secret(&self) -> dwow_sdk::crypto::SecretKey {
        self.secret.clone()
    }

    fn merkle_path(&self) -> Vec<pallas::Base> {
        self.merkle_path.clone()
    }

    fn leaf_position(&self) -> u32 {
        self.leaf_position
    }
}

/// Convert a note field value (u64 or pallas::Base) into a pallas::Base for
/// the witness binder. Helper for populating `ResolvedCapProvider::note_fields`.
pub fn note_field_as_base(value: u64, blind: pallas::Base) -> pallas::Base {
    // Most note fields are u64 values stored as pallas::Base.
    // Callers with more complex note schemas should decode via the manifest.
    let _ = blind; // reserved for typed binding
    pallas::Base::from(value)
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
) -> Result<Vec<u8>, String> {
    // Step 4: decode the zkas binary → ordered witness list
    let zkbin = ZkBinary::decode(zkas_bytes, false)
        .map_err(|e| format!("ZkBinary::decode: {:?}", e))?;
    let witness_count = zkbin.witnesses.len();

    // Step 5: bind every witness slot per witness_map, in declared order
    let mut witnesses: Vec<Witness> = Vec::with_capacity(witness_count);
    for (idx, source) in ctx.witness_map.entries.iter().enumerate() {
        let val: Value<pallas::Base> = match source {
            WitnessSource::NoteField(field) => {
                let v = provider.note_field(field)
                    .ok_or_else(|| format!("witness[{}]: note field '{}' not found", idx, field))?;
                Value::known(v)
            }
            WitnessSource::ParamField(_field) => {
                // Parameter fields are bound by the caller into the call data;
                // they arrive at the entrypoint as inputs, not as circuit
                // witnesses. Return a named error rather than a silent default.
                return Err(format!(
                    "witness[{}]: param fields are not witness-bound — \
                     they are call-data arguments", idx,
                ));
            }
            WitnessSource::Secret => {
                Value::known(provider.secret().inner())
            }
            WitnessSource::MerklePath => {
                let _path = provider.merkle_path();
                // Merkle path is variable-length; bind each element as a
                // separate witness. The manifest's witness_map must declare
                // one `merkle_path` entry per path element, OR the circuit
                // has a fixed-depth path. For now, bind one element per
                // declaration and let the caller ensure the path length
                // matches the circuit's expected depth.
                //
                // TODO(Phase 6 full): the manifest should carry path depth
                // alongside witness_map so we can verify arity at bind time.
                return Err(format!(
                    "witness[{}]: merkle_path binding not yet implemented \
                     (path depth must match circuit's expected tree depth; \
                     manifest schema extension pending)", idx,
                ));
            }
            WitnessSource::LeafPosition => {
                Value::known(pallas::Base::from(provider.leaf_position() as u64))
            }
            WitnessSource::Blind => {
                // Fresh blind derived from Seed (§6.1): domain = witness index
                // ensures every blind is unique even within a single proof.
                let seed_base = pallas::Base::from_repr(ctx.seed).unwrap_or(pallas::Base::zero());
                let blind = dwow_sdk::crypto::poseidon_hash([
                    seed_base,
                    pallas::Base::from(idx as u64),
                    pallas::Base::from(0x00), // domain: blind
                ]);
                Value::known(blind)
            }
            WitnessSource::TxCommitment => {
                // tx_commitment = zero for single-call transactions
                Value::known(pallas::Base::zero())
            }
            WitnessSource::TxNonce => {
                // tx_nonce = zero for single-call transactions
                Value::known(pallas::Base::zero())
            }
        };
        // Convert to the VarType the slot expects. The zkbin witness list
        // tells us whether it's Base or Scalar; we always produce Base here
        // and rely on the caller to provide the correct zkas binary.
        witnesses.push(Witness::Base(val));
    }

    if witnesses.len() != witness_count {
        return Err(format!(
            "witness count mismatch: bound {} slots, circuit declares {}",
            witnesses.len(), witness_count,
        ));
    }

    // Step 6: build proving key (cacheable per circuit — not yet cached) →
    // create proof. Seed-derived RNG for determinism (FeeCollectV1 pattern,
    // §3.6 determinism proof).
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit)
        .map_err(|e| format!("ProvingKey::build: {:?}", e))?;
    let seed_bytes: [u8; 32] = ctx.seed;
    let mut rng = rand::rngs::StdRng::from_seed(seed_bytes);
    let proof = Proof::create(&pk, &[circuit], &[], &mut rng)
        .map_err(|e| format!("Proof::create: {:?}", e))?;

    let mut buf = Vec::new();
    dwow_serial::Encodable::encode(&proof, &mut buf)
        .map_err(|e| format!("proof encode: {:?}", e))?;
    Ok(buf)
}
