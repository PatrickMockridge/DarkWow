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

//! Transaction structures for linear blockchain

use blake3::Hash;
use dwow_sdk::{
    blockchain::BlockVersion,
    crypto::ContractId,
    deploy::DeployParamsV1,
    error::ContractError,
    pasta::pallas,
};
use dwow_sdk::pasta::group::ff::PrimeField;
use dwow_serial::Decodable;
use serde::{Deserialize, Serialize};

// ============================================================================
// Cryptographic newtypes — compile-time enforcement of mathematical spec.
// ============================================================================
// These types prevent the compiler from accepting semantically invalid code.
// Commitment and Nullifier are both 32 bytes but MUST NOT be swappable.
// TokenCommitment is also 32 bytes — distinct from both.
// PedersenCoordinate wraps a 32-byte value commitment coordinate.
// ============================================================================

/// Commitment: C = poseidon_hash([pk.x, pk.y, value, token_id, ...]).
/// MUST NOT be swapped with Nullifier or raw [u8; 32].
/// Backed by pallas::Base — field element, not raw bytes — per type system unification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Commitment(pallas::Base);

impl Commitment {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_base(x: pallas::Base) -> Self { Self(x) }
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError("non-canonical Commitment".into()))
        }
    }
}

// Manual serde — reads/writes [u8; 32] to preserve block serialization format.
impl Serialize for Commitment {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_bytes().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Commitment {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 32]>::deserialize(d)?;
        Commitment::from_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

// Canonical Nullifier type — re-exported from the native token contract.
// The contract defines the mathematical representation; chain code consumes it.
// Deleted the old chain-level Nullifier(pub [u8; 32]) — type fracture #1 resolved.
// See doc/src/arch/type-system.md §2 (Type Distinction Principle) and §9.4.
pub use dwow_native_token_contract::model::Nullifier;

/// Token commitment: poseidon_hash(token_id, token_blind).
/// MUST NOT be swapped with Commitment or Nullifier.
/// Backed by pallas::Base — field element, not raw bytes — per type system unification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenCommitment(pallas::Base);

impl TokenCommitment {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError("non-canonical TokenCommitment".into()))
        }
    }
}

// Manual serde — reads/writes [u8; 32] to preserve block serialization format.
impl Serialize for TokenCommitment {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_bytes().serialize(s)
    }
}

impl<'de> Deserialize<'de> for TokenCommitment {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 32]>::deserialize(d)?;
        TokenCommitment::from_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

/// Pedersen commitment coordinate — wraps a 32-byte value.
/// Distinct from Commitment, Nullifier, and TokenCommitment.
/// Backed by pallas::Base — field element, not raw bytes — per type system unification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PedersenCoordinate(pallas::Base);

impl PedersenCoordinate {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError("non-canonical PedersenCoordinate".into()))
        }
    }
}

// Manual serde — reads/writes [u8; 32] to preserve block serialization format.
impl Serialize for PedersenCoordinate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_bytes().serialize(s)
    }
}

impl<'de> Deserialize<'de> for PedersenCoordinate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 32]>::deserialize(d)?;
        PedersenCoordinate::from_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

/// Transaction input - reference to an unspent output.
/// Renamed from `Input` to avoid collision with contract-level `Input`
/// (ZK privacy-preserving input in native_token/promissory_note/bearer_bond).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    /// Reference to the previous transaction output
    pub previous_output: Hash,
    /// Signature script / proof
    pub script: Vec<u8>,
    /// Sequence number (for timelock)
    pub sequence: u32,
}

/// Transaction output - new value created in this transaction.
/// Renamed from `Output` to avoid collision with contract-level `Output`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    /// Value being transferred
    pub value: u64,
    /// Public key or script hash
    pub script: Vec<u8>,
}

/// A contract call embedded in a transaction input's script field.
/// Format: [1 byte call_idx][32 bytes contract_id][varbytes payload]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCall {
    /// ID of the contract to invoke — typed ContractId per Phase 2.1.
    /// Was raw [u8; 32]; now uses the canonical ContractId(pallas::Base).
    pub contract_id: ContractId,
    /// Call data passed to the contract (function selector + params)
    pub data: Vec<u8>,
}

impl ContractCall {
    /// Attempt to decode this call as FeeV3 call data.
    /// `[domain: mass_balance + fee_signalling]`
    /// Returns `None` if contract_id does not match or selector is not `0x08`.
    /// This is the SINGLE site where FeeV3 dispatch is determined per
    /// type-system.md §10.5 (absorber boundary re-lift).
    pub fn as_mass_balance_fee_v3(&self) -> Option<dwow_sdk::mass_balance_call_data::MassBalanceFeeV3CallData> {
        if self.contract_id != *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID {
            return None;
        }
        dwow_sdk::mass_balance_call_data::MassBalanceFeeV3CallData::from_bytes(&self.data)
    }

    /// Attempt to decode this call as PoWRewardV1 call data.
    /// `[domain: mass_balance]` — block-opening coinbase nullifier claim.
    pub fn as_mass_balance_coinbase_v1(&self) -> Option<dwow_sdk::mass_balance_call_data::MassBalanceCoinbaseV1CallData> {
        if self.contract_id != *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID {
            return None;
        }
        dwow_sdk::mass_balance_call_data::MassBalanceCoinbaseV1CallData::from_bytes(&self.data)
    }

    /// Attempt to decode this call as DeployV1 call data.
    /// `[domain: fee_signalling]` — WASM deploy size determines wasm_kB for threshold.
    ///
    /// Returns `Some(wasm_byte_length)` if this is a DeployV1 call
    /// (contract_id == DEPLOYOOOR_CONTRACT_ID, selector == 0x00).
    /// Returns `None` if not a deploy.
    ///
    /// The WASM byte length is estimated from the call data size (minus the
    /// selector byte). For production deploys the WASM bincode dominates the
    /// total DeployParamsV1 size — any overestimate is conservative (deploys
    /// will never be underpriced). FI-WASM-1 (fee-spec.md §14.8).
    pub fn as_deploy_v1(&self) -> Option<usize> {
        if self.contract_id != *dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID {
            return None;
        }
        // `first` covers both halves of the `is_empty() || data[0]` test without an index.
        if self.data.first() != Some(&0x00) {
            return None;
        }
        // Properly decode DeployParamsV1 using dwow_serial — same pattern as
        // execution.rs:559,910. Returns the EXACT wasm_bincode length, not an
        // estimate. FI-WASM-1: wasm_kB = max(1, ceil(wasm_bytes / 1024)).
        let payload = self.data.get(1..)?;
        let mut cursor = std::io::Cursor::new(payload);
        DeployParamsV1::decode(&mut cursor)
            .ok()
            .map(|params| params.wasm_bincode.len())
    }
}

/// The coinbase output the miner assembles.
/// Since b6bf44f79 the coinbase is a plaintext PoWRewardV1 contract call
/// (selector 0x05) — no ZK proof is attached to this struct, and it carries no
/// ZK public inputs. It holds the commitment, nullifier and encrypted note that
/// dwowd's `build_linear_coinbase` returns to the miner and genesis paths
/// (which read `commitment`/`nullifier`; the WASM entrypoint re-verifies the
/// supply-chain values from the call data itself).
/// Newtypes enforce the mathematical spec at compile time:
///   - Commitment ≠ Nullifier ≠ TokenCommitment (compiler rejects swaps)
///   - Nullifier::from_bytes rejects zero sentinel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinbaseTransaction {
    /// Poseidon hash of commitment attributes — C = poseidon_hash([pk.x, pk.y, value, ...])
    pub commitment: Commitment,
    /// Pedersen value commitment x-coordinate
    pub value_commit_x: PedersenCoordinate,
    /// Pedersen value commitment y-coordinate
    pub value_commit_y: PedersenCoordinate,
    /// Poseidon token commitment
    pub token_commit: TokenCommitment,
    /// Nullifier: nf = poseidon_hash(sk_H.inner(), C) — capability claim.
    /// The miner exercises the coinbase capability by publishing this nullifier.
    /// Validators verify it against the nullifier set.
    /// Constructed via Nullifier::from_bytes() — rejects [0u8; 32].
    pub nullifier: Nullifier,
    /// Cumulative supply commitment x-coordinate (S_H.x)
    pub new_cumulative_x: PedersenCoordinate,
    /// Cumulative supply commitment y-coordinate (S_H.y)
    pub new_cumulative_y: PedersenCoordinate,
    /// AEAD encrypted note (AeadEncryptedNote serialized)
    pub encrypted_note: Vec<u8>,
}

/// Transaction - a transfer of value in the blockchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction version
    pub version: BlockVersion,
    /// Inputs spent in this transaction
    pub inputs: Vec<TxInput>,
    /// Outputs created by this transaction
    pub outputs: Vec<TxOutput>,
    /// Contract calls embedded in inputs (optional extension).
    /// The coinbase transaction (block reward) places its PoWRewardV1 call here
    /// at transactions[0].contract_calls[0] — no separate coinbase field.
    pub contract_calls: Vec<ContractCall>,
    /// Lock time (can be block height or timestamp)
    pub lock_time: u64,
    /// Pre-computed nullifiers for mempool double-spend detection.
    /// When empty (most transactions), omitted from JSON to preserve
    /// hash determinism across code versions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nullifiers: Vec<Nullifier>,
    /// L1 authenticated-transaction carriage: the opaque, dwow_serial-encoded
    /// witness bundle — the ZK proofs, signatures, and tx_commitment of the core
    /// transaction. Carried and persisted so a verifier (L2) can check it;
    /// EXCLUDED from `hash()` — block identity commits to transaction semantics,
    /// never to interchangeable witness bytes (see `hash`). Empty for the
    /// coinbase and for not-yet-populated txs; omitted from JSON when empty, so
    /// the persisted and wire format stays byte-identical to the pre-witness
    /// format (no fork, no genesis regen).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub witness: Vec<u8>,
}

/// Default Transaction has version: 1 (not 0) — a version-0 transaction is
/// consensus-invalid. The Default derive was removed to prevent silent creation
/// of invalid state.
impl Default for Transaction {
    fn default() -> Self {
        Self {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![], contract_calls: vec![],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        }
    }
}

impl Transaction {
    /// True when this tx's first contract call targets the native token
    /// contract with selector 0x05 (PoWRewardV1) — the coinbase
    /// classification used by chain_state, validation, execution and
    /// proof_of_token_balance.
    ///
    /// Pinned cross-crate: 0x05 ↔
    /// dwow_native_token_contract::NativeTokenFunction::PoWRewardV1.
    /// contrib/ci/check_heavyweight_coverage.sh fails CI if that variant is
    /// renamed — keep this pin updated if it ever is.
    pub fn is_pow_reward_coinbase_tx(&self) -> bool {
        self.contract_calls.first().map_or(false, |c| {
            c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID &&
                c.data.first() == Some(&0x05)
        })
    }

    /// True when this tx's first contract call targets the native token contract with selector 0x06
    /// (FeeCollectV1) — the block-closing fee collection plate.
    ///
    /// Pinned cross-crate, exactly as the coinbase predicate above is: 0x06 ↔
    /// `dwow_native_token_contract::NativeTokenFunction::FeeCollectV1` (`native_token/src/lib.rs:70`).
    ///
    /// It exists for block-level classification rather than for validation: like the coinbase, a fee
    /// collection is **generated by the block** and belongs to it alone (it claims that block's fee pot and
    /// closes its commitment merkle tree), so a path that returns a block's transactions to the mempool
    /// must exclude both (`OBL-C44`) or it re-admits a claim that can only settle once.
    pub fn is_fee_collect_tx(&self) -> bool {
        self.contract_calls.first().map_or(false, |c| {
            c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID &&
                c.data.first() == Some(&0x06)
        })
    }

    /// True when this tx's first contract call carries selector 0x05
    /// (PoWRewardV1) regardless of contract id — the structural variant used
    /// where the block-structure rule (transactions[0] is the coinbase) is
    /// checked separately from contract identity.
    pub fn first_call_is_pow_reward(&self) -> bool {
        self.contract_calls.first().map_or(false, |c| c.data.first() == Some(&0x05))
    }

    /// Calculate the hash of this transaction.
    ///
    /// L1 barrier #1 — identity/witness decoupling. The hash commits ONLY to the
    /// transaction's semantics (version, inputs, outputs, contract_calls,
    /// lock_time, nullifiers) and NEVER to the `witness` (ZK proofs + signatures
    /// + tx_commitment).
    ///
    /// Deterministic by construction: each field is written directly to a
    /// blake3::Hasher in a fixed order with length-prefixed vectors. No
    /// serialization library — the format IS the function body below.
    /// Closes: M2 (Transaction hash uses non-canonical serde_json).
    /// Enforces: type-system.md §2.2 (deterministic at persistence boundaries).
    pub fn hash(&self) -> Hash {
        let mut h = blake3::Hasher::new();

        // Chain ID: 32 bytes — prevents cross-network transaction replay
        h.update(&crate::CHAIN_ID);

        // version: 1 byte
        h.update(&[self.version.get()]);

        // inputs: count (u32 LE) + each input
        h.update(&(self.inputs.len() as u32).to_le_bytes());
        for input in &self.inputs {
            h.update(input.previous_output.as_bytes());       // 32 bytes
            h.update(&(input.script.len() as u32).to_le_bytes());
            h.update(&input.script);
            h.update(&input.sequence.to_le_bytes());          // 4 bytes LE
        }

        // outputs: count (u32 LE) + each output
        h.update(&(self.outputs.len() as u32).to_le_bytes());
        for output in &self.outputs {
            h.update(&output.value.to_le_bytes());             // 8 bytes LE
            h.update(&(output.script.len() as u32).to_le_bytes());
            h.update(&output.script);
        }

        // contract_calls: count (u32 LE) + each call
        h.update(&(self.contract_calls.len() as u32).to_le_bytes());
        for call in &self.contract_calls {
            h.update(&call.contract_id.to_bytes());            // 32 bytes
            h.update(&(call.data.len() as u32).to_le_bytes());
            h.update(&call.data);
        }

        // lock_time: 8 bytes LE
        h.update(&self.lock_time.to_le_bytes());

        // nullifiers: count (u32 LE) + each nullifier
        h.update(&(self.nullifiers.len() as u32).to_le_bytes());
        for nf in &self.nullifiers {
            h.update(&nf.to_bytes());                          // 32 bytes
        }

        h.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OBL-C44` — the block-generated transactions are identified, so a path returning a displaced
    /// block's transactions to the mempool can exclude the two that belong to the block itself: the
    /// coinbase (which pays that block's reward) and the fee collection (which claims its fee pot and
    /// closes its commitment merkle tree).
    ///
    /// Asserted in both directions, because either half alone passes against a predicate that is simply
    /// always true or always false: each selector classifies its own call and not the other, the same
    /// selector on a **different** contract is not a native-token call, and an empty call list is neither.
    #[test]
    fn test_block_generated_tx_classification() {
        let native = *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
        let other = *dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID;

        let with = |contract_id: ContractId, selector: u8| Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall { contract_id, data: vec![selector] }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        let coinbase = with(native, 0x05);
        let fee_collect = with(native, 0x06);

        assert!(coinbase.is_pow_reward_coinbase_tx(), "0x05 on native_token is the coinbase");
        assert!(!coinbase.is_fee_collect_tx(), "and not the fee collection");
        assert!(fee_collect.is_fee_collect_tx(), "0x06 on native_token is the fee collection");
        assert!(!fee_collect.is_pow_reward_coinbase_tx(), "and not the coinbase");

        assert!(
            !with(other, 0x05).is_pow_reward_coinbase_tx(),
            "control: the same selector on another contract is not the coinbase — the predicate is a"
        );
        assert!(
            !with(other, 0x06).is_fee_collect_tx(),
            "control: nor is it the fee collection on another contract"
        );
        let empty = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        assert!(
            !empty.is_pow_reward_coinbase_tx() && !empty.is_fee_collect_tx(),
            "control: a transaction with no contract call is neither"
        );
    }

    /// The selector-only probe is **not** a coinbase classifier, and this is the input on which the
    /// two disagree — the one that made an accept-path exemption unsound until 2026-09-24.
    ///
    /// `first_call_is_pow_reward` matches `data[0] == 0x05` **against any contract**, which its own
    /// docstring states ("regardless of contract id … the structural variant used where the
    /// block-structure rule is checked separately from contract identity"). Around twenty contracts
    /// use 0x05 as a real function code — `IdentityFunction::IssueCapabilityV1`,
    /// `AttestationFunction::ConsumeClaimV1`, `dex`/`relayer_endowment`'s `UpdateConfigV1`,
    /// `CancelV1`, `RefundBidV1`, `RepayStableV1` and others — so for a transaction whose first call
    /// is one of those the probe answers "coinbase" and the shared classifier answers "not the
    /// coinbase".
    ///
    /// Every accept-path exemption needs the second answer, because the L2 witness loop is the only
    /// place a transaction's proofs are verified at block acceptance: with the probe, such a
    /// transaction was exempted from proof verification entirely and a fabricated proof rode in.
    /// `execution.rs`, `block_acceptor.rs`, `chain_state.rs::check_coinbase_maturity`, the miner's
    /// assembly filter and both RPC predicates are the sites that had to agree, and
    /// `scripts/check-coinbase-classifier.sh` is the gate that keeps them agreeing.
    #[test]
    fn test_selector_only_probe_is_not_a_coinbase_classifier() {
        let native = *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
        let other = *dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID;

        let with = |contract_id: ContractId, selector: u8| Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall { contract_id, data: vec![selector] }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        // The disagreement, asserted on both predicates so the test fails if either is widened or
        // narrowed: 0x05 on a non-native contract fires the probe and is not the coinbase.
        assert!(
            with(other, 0x05).first_call_is_pow_reward(),
            "the probe fires on 0x05 whatever the contract — this is the trap, not a bug in the probe"
        );
        assert!(
            !with(other, 0x05).is_pow_reward_coinbase_tx(),
            "the shared classifier requires the native token contract, and is what every exemption must use"
        );

        // And the two agree on the genuine coinbase, so the shared classifier is not simply narrower:
        // an exemption keyed on it still exempts the transaction it is for.
        assert!(with(native, 0x05).is_pow_reward_coinbase_tx());
        assert!(with(native, 0x05).first_call_is_pow_reward());

        // The probe is a *first-call* test, not a contract test: 0x05 anywhere but first does not fire
        // it, which is why it is safe for a site that position-gates it anyway.
        let second_call = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![
                ContractCall { contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID, data: vec![0x00] },
                ContractCall { contract_id: other, data: vec![0x05] },
            ],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        assert!(
            !second_call.first_call_is_pow_reward()
                && !second_call.is_pow_reward_coinbase_tx(),
            "a 0x05 call that is not first fires neither predicate"
        );
    }

    /// Transaction::hash() MUST be deterministic across serde round-trips.
    /// Serializing, deserializing, and re-serializing a transaction MUST
    /// produce the same hash — otherwise merkle roots diverge.
    #[test]
    fn test_transaction_hash_determinism() {
        let tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        let hash1 = tx.hash();
        let json = serde_json::to_vec(&tx).unwrap();
        let tx2: Transaction = serde_json::from_slice(&json).unwrap();
        let json2 = serde_json::to_vec(&tx2).unwrap();
        let hash2 = tx2.hash();

        assert_eq!(json, json2, "serde round-trip must be bit-identical");
        assert_eq!(hash1, hash2, "hash must be deterministic across round-trip");
    }

    /// Transactions with nullifiers MUST round-trip correctly.
    #[test]
    fn test_transaction_with_nullifiers_roundtrip() {
        let nf = Nullifier::from_bytes([1u8; 32]).unwrap();
        let tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![nf],
            witness: vec![],
        };

        let json = serde_json::to_vec(&tx).unwrap();
        let tx2: Transaction = serde_json::from_slice(&json).unwrap();
        assert_eq!(tx2.nullifiers.len(), 1);
        assert_eq!(tx2.nullifiers[0], nf);
    }

    /// Empty nullifiers MUST be absent from JSON (skip_serializing_if).
    #[test]
    fn test_transaction_empty_nullifiers_omitted_from_json() {
        let tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        let json_str = serde_json::to_string(&tx).unwrap();
        assert!(
            !json_str.contains("\"nullifiers\""),
            "empty nullifiers MUST be omitted from JSON output: {}",
            json_str
        );
    }

    /// L1 barrier #1 (operational): populating the `witness` (proofs +
    /// signatures + tx_commitment) MUST NOT change `tx.hash()`. Block identity
    /// commits to transaction semantics, not to interchangeable witness bytes —
    /// this is what preserves every existing block hash and the genesis hash
    /// once proofs are actually carried.
    #[test]
    fn test_witness_excluded_from_hash() {
        let mut tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![Nullifier::from_bytes([2u8; 32]).unwrap()],
            witness: vec![],
        };
        let h_empty = tx.hash();
        tx.witness = vec![9u8; 4096];
        assert_eq!(h_empty, tx.hash(), "populating the witness MUST NOT change tx.hash()");
        tx.witness = vec![0xAB; 100];
        assert_eq!(h_empty, tx.hash(), "any witness → identical hash");
    }

    /// L1: the `witness` rides the serde_json format used for block persistence,
    /// the block P2P wire, and mempool storage — round-tripping a proof-carrying
    /// tx MUST preserve the witness bytes and MUST NOT change the hash. An empty
    /// witness MUST be omitted from JSON (byte-identical to the pre-witness
    /// format — no fork, no genesis regen).
    #[test]
    fn test_witness_survives_serde_roundtrip() {
        let tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![1u8, 2, 3, 4, 5],
        };
        let json = serde_json::to_vec(&tx).unwrap();
        let tx2: Transaction = serde_json::from_slice(&json).unwrap();
        assert_eq!(tx2.witness, tx.witness, "witness must survive the serde round-trip");
        assert_eq!(tx.hash(), tx2.hash(), "hash stable across a witness round-trip");

        let empty = Transaction { witness: vec![], ..tx.clone() };
        assert!(
            !serde_json::to_string(&empty).unwrap().contains("witness"),
            "empty witness MUST be omitted from JSON (byte-identical to pre-witness format)"
        );
    }
}
    #[test]
    fn test_coin_commitment_roundtrip() {
        let cc = Commitment::from_bytes([1u8; 32]).unwrap();
        let bytes = cc.to_bytes();
        let cc2 = Commitment::from_bytes(bytes).unwrap();
        assert_eq!(cc, cc2);
    }

    #[test]
    fn test_coin_commitment_zero_valid() {
        // Zero IS valid for Commitment (unlike Nullifier)
        assert!(Commitment::from_bytes([0u8; 32]).is_ok());
    }

    #[test]
    fn test_pedersen_coordinate_roundtrip() {
        let pc = PedersenCoordinate::from_bytes([2u8; 32]).unwrap();
        let bytes = pc.to_bytes();
        let pc2 = PedersenCoordinate::from_bytes(bytes).unwrap();
        assert_eq!(pc, pc2);
    }
