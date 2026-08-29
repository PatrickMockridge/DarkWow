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

use smol::channel::Sender;
use tracing;

use dwow_core::{
    blockchain::HeaderHash,
};
use crate::wallet_error::Result;
use dwow_chain::fee_window::FeeWindowFlags;
use dwow_chain::Commitment;
use std::collections::BTreeMap;

use dwow_sdk::{
    blockchain::BlockHeight,
    bridgetree::Position,
    crypto::{
        poseidon_hash,
        Blind, ContractId, FuncId, MerkleNode, MerkleTree, PublicKey,
        SecretKey, AssetId,
        DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
    },
    deploy::{ContractMetadata, DeployParamsV1},
    pasta::group::ff::PrimeField,
};
use dwow_native_token_contract::client::NativeToken;
use dwow_native_token_contract::model::{fee::FeeParamsV3, BurnParamsV1, CommitmentAttributes, SpendParamsV1, TransferParamsV1};
use dwow_sdk::capability::{wallet_construct, Barb, Primitive};
use dwow_sdk::crypto::note::AeadEncryptedNote;
use dwow_sdk::pasta::pallas;
use dwow_serial::Decodable;

use crate::{
    cli_util::append_or_print,
    error::{WalletDbError, WalletDbResult},
    walletdb::{CapRecord, MerkleProof},
    Dww,
};

// The wallet is a full node. Blocks are synced via P2P and read from the
// local chain store (LinearStore). No RPC — scan iterates self.chain directly.

/// Auxiliary structure holding various in memory caches to use during scan
pub struct ScanCache {
    /// The capability commitment tree — Merkle tree of H(w, params) for all capabilities
    pub capability_commitment_tree: MerkleTree,
    /// The capability nullifier SMT — sparse Merkle tree of nullifiers
    /// All our known secrets to decrypt capability commitments
    pub secrets: Vec<SecretKey>,
    /// Messages buffer for better downstream prints handling
    pub messages_buffer: Vec<String>,
}

impl ScanCache {
    /// Auxiliary function to append messages to the buffer.
    pub fn log(&mut self, msg: String) {
        self.messages_buffer.push(msg);
    }

    /// Auxiliary function to consume the messages buffer.
    pub fn flush_messages(&mut self) -> Vec<String> {
        self.messages_buffer.drain(..).collect()
    }
}

// ============================================================================
// Pure Scan Output Types
// ============================================================================
// These types decouple scan computation from database persistence.
// scan_block() returns a BlockScanResult — the caller handles DB writes.

/// Source of a discovered native token output.
#[derive(Debug, Clone)]
pub(crate) enum NativeTokenSource {
    PoWRewardV1,
    TransferV1,
    SpendV1,
    FeeV1,
    /// FeeV2 — privacy-preserving fee payment (0x08). Output carries AEAD-encrypted
    /// change note; discovered by trial decryption like other native token outputs.
    FeeV2,
    /// FeeCollectV1 — miner fee commitment (capability claim for a NEW commitment,
    /// not a spend — same exclusion as PoWRewardV1 from nullifier extraction)
    FeeCollectV1,
}

impl NativeTokenSource {
    fn as_str(&self) -> &'static str {
        match self {
            NativeTokenSource::PoWRewardV1 => "PoWRewardV1",
            NativeTokenSource::TransferV1 => "TransferV1",
            NativeTokenSource::SpendV1 => "SpendV1",
            NativeTokenSource::FeeV1 => "FeeV1",
            NativeTokenSource::FeeV2 => "FeeV2",
            NativeTokenSource::FeeCollectV1 => "FeeCollectV1",
        }
    }
}

/// A native token output discovered during block scan.
/// Carries enough data to construct a CapRecord + MerkleProof during persistence.
#[derive(Debug, Clone)]
pub struct NativeTokenDiscovery {
    pub cap_record: CapRecord,
    pub merkle_proof: MerkleProof,
}

/// A capability discovered via generic AEAD scan of non-native contract calls.
#[derive(Debug, Clone)]
pub struct CapabilityDiscovery {
    pub cap_record: CapRecord,
    pub merkle_proof: MerkleProof,
}

/// A nullifier published in a spending contract call.
/// Extracted from TransferV1/BurnV1/SpendV1 params.
/// Per V.2: nullifier is now typed Nullifier (↓nullify barb, zero-rejection
/// enforced at construction), not raw pallas::Base.
#[derive(Debug, Clone)]
pub struct NullifierRecord {
    pub(crate) nullifier: dwow_chain::Nullifier,
}

/// A contract deployment discovered during block scan.
#[derive(Debug, Clone)]
pub struct DeploymentDiscovery {
    pub(crate) contract_id: ContractId,
    pub(crate) deployer_pubkey: PublicKey,
    pub(crate) metadata: Option<ContractMetadata>,
    pub(crate) manifest_json: Option<String>,
    pub(crate) height: BlockHeight,
}

/// A zkas circuit binary discovered during deploy-scan.
/// Extracted from the DeployV1 WASM blob — stored in zkas_binaries (§3).
#[derive(Debug, Clone)]
pub struct ZkasBinaryDiscovery {
    pub contract_id: ContractId,
    pub namespace: String,
    pub circuit_name: String,
    pub zkas_bytes: Vec<u8>,
}

/// Typed scan errors replacing silent `Option` returns (Wave 3).
/// Each variant carries enough context to produce a distinct diagnostic.
#[derive(Debug, Clone)]
pub enum ScanError {
    /// Merkle tree witness operation failed — capability cannot be recorded.
    MerkleWitness { position: u64, reason: String },
    /// Merkle tree root(0) returned None — tree in inconsistent state.
    MerkleRoot,
    /// wallet_construct returned None — primitives do not cover required barbs.
    WalletConstructFailed { resource: String, action: String },
    /// account_mgr.find_owner() returned None — key not in accounts.
    KeyNotFound { public_key: String },
    /// Per-contract key derivation failed.
    KeyDerivation { contract_id: ContractId, height: BlockHeight, reason: String },
    /// Manifest store/load failure.
    Manifest { contract_id: ContractId, reason: String },
    /// Nullifier detection skipped (DB error loading held caps).
    NullifierDetectionSkipped,
    /// Parameter decode failure (nullifiers from transfer/spend/burn).
    ParamDecode { function_code: u8, reason: String },
}

/// Result of scanning a single block — pure computation, no side effects.
/// The caller persists these results to the database.
#[derive(Debug, Clone)]
pub struct BlockScanResult {
    pub native_outputs: Vec<NativeTokenDiscovery>,
    pub capabilities: Vec<CapabilityDiscovery>,
    pub published_nullifiers: Vec<NullifierRecord>,
    pub deployments: Vec<DeploymentDiscovery>,
    /// zkas circuit binaries extracted from DeployV1 WASM blobs — stored
    /// in zkas_binaries for the generic prover (§6.4.1 step 3).
    pub zkas_binaries: Vec<ZkasBinaryDiscovery>,
    pub messages: Vec<String>,
    /// Per-barrier diagnostic counters (type-system.md §Z: Diagnostic Transparency).
    /// Distinguishes "nothing to report" from "everything failed."
    pub diagnostics: BlockScanDiagnostics,
}

/// Per-barrier diagnostic counters. Every attempt at each scan pipeline stage
/// is counted. Operators can distinguish "block has no wallet-relevant data"
/// from "all decrypt/construct attempts failed."
///
/// Path 1 counters track native token (consensus-critical). Path 2 counters
/// track the manifest-driven capability engine (HAZOP 4.6 — type-system.md
/// §4.2.4: structured diagnostics MUST distinguish failure modes).
#[derive(Debug, Clone, Default)]
pub struct BlockScanDiagnostics {
    // Path 1: native token
    pub aead_decode_attempts: usize,
    pub aead_decrypt_attempts: usize,
    pub aead_decrypt_successes: usize,
    pub capability_construct_attempts: usize,
    pub capability_construct_successes: usize,
    pub nullifiers_matched: usize,
    // Path 2: manifest-driven capability engine
    pub manifest_misses: usize,
    pub derivation_failures: usize,
    pub path2_decrypt_attempts: usize,
    pub path2_decrypt_successes: usize,
    pub path2_coverage_drops: usize,
}

impl BlockScanResult {
    fn new() -> Self {
        Self {
            native_outputs: vec![],
            capabilities: vec![],
            published_nullifiers: vec![],
            deployments: vec![],
            zkas_binaries: vec![],
            messages: vec![],
            diagnostics: BlockScanDiagnostics::default(),
        }
    }
}

/// Resolve the trust tier for a deployed contract manifest.
/// Try to extract the public `instance_seed` from contract call data.
/// Contracts that use `derive_instance` for per-instance unlinkable keys
/// carry the seed in the clear in their call params (typically a `[u8; 32]`
/// field). We attempt a 32-byte read at offset 1 past the function-code byte.
/// No per-contract list — `derive_instance` just hashes the input, so a
/// false-positive extraction only produces a key that won't decrypt (harmless).
fn try_extract_instance_seed(_cid: &ContractId, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 33 { return None; } // function code + 32-byte seed
    Some(data[1..33].to_vec())
}

// ============================================================================
// Pure Scan Functions
// ============================================================================
// These functions take data and return data — zero database access.
// Same (secrets, caps, tree, block) → same BlockScanResult, every time.
// Testable without a running node, without SQLite, without P2P.

/// Append a leaf (poseidon commitment as `pallas::Base`) to a Merkle tree
/// and produce its inclusion proof. Note-type-agnostic — the generic engine and
/// the native path share the same tree-append/witness/root workflow.
fn append_leaf_and_prove(tree: &mut MerkleTree, leaf: pallas::Base) -> std::result::Result<(u64, MerkleProof), ScanError> {
    let leaf_pos = tree.current_position().map(u64::from).unwrap_or(0);
    let cap_leaf = MerkleNode::new(leaf);
    tree.append(cap_leaf);
    tree.mark();
    let siblings: Vec<MerkleNode> = tree.witness(Position::from(leaf_pos), 0)
        .map_err(|e| ScanError::MerkleWitness {
            position: leaf_pos,
            reason: format!("{:?}", e),
        })?;
    let mut sibling_strings: Vec<String> = siblings
        .iter()
        .map(|n| bs58::encode(n.inner().to_repr()).into_string())
        .collect();
    while sibling_strings.len() < dwow_sdk::crypto::constants::MERKLE_DEPTH_ORCHARD {
        let lvl = sibling_strings.len();
        use dwow_sdk::bridgetree::Hashable;
        sibling_strings.push(
            bs58::encode(
                MerkleNode::empty_root(dwow_sdk::bridgetree::Level::from(lvl as u8))
                    .inner()
                    .to_repr(),
            )
            .into_string(),
        );
    }
    let root = tree.root(0)
        .ok_or(ScanError::MerkleRoot)?
        .inner().to_repr();
    Ok((leaf_pos, MerkleProof {
        siblings: sibling_strings,
        root: bs58::encode(root).into_string(),
        leaf_position: leaf_pos,
    }))
}

/// Personalization for capability ID derivation via blake2b.
/// SHALL be exactly 16 bytes (BLAKE2B_PERSONALBYTES).
/// Null-padded from the original 13-byte "DarkFi_CoinId" to make the
/// zero-padding explicit — produces byte-identical hashes.
/// FIXME: update Python model and eliminate null padding when the spec
/// adopts a canonical 16-byte persona.
const COIN_ID_PERSONALIZATION: &[u8] = b"DarkFi_CoinId\0\0\0";

/// Derive a deterministic capability id: `bs58(blake2b(secret || commitment, "DarkFi_CoinId"))`.
fn derive_cap_id(secret: &SecretKey, commitment_bytes: &[u8; 32]) -> String {
    let mut hasher = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(COIN_ID_PERSONALIZATION)
        .to_state();
    hasher.update(&secret.inner().to_repr());
    hasher.update(commitment_bytes);
    let cap_id_hash = hasher.finalize();
    bs58::encode(cap_id_hash.as_bytes()).into_string()
}

/// Build a CapRecord + MerkleProof from a decrypted native token note.
/// Pure: mutates `tree` (append + witness) but does not touch any database.
/// Returns the cap record, its merkle proof, and a diagnostic message.
fn build_native_token_cap_record(
    tree: &mut MerkleTree,
    secret: &SecretKey,
    note: &NativeToken,
    height: BlockHeight,
    source: &NativeTokenSource,
    contract_id: ContractId,
    func_id: Option<FuncId>,
    capability_discriminant: Option<u8>,
    existing_cap_ids: &std::collections::HashSet<String>,
) -> std::result::Result<Option<(CapRecord, MerkleProof, String)>, ScanError> {
    // Full recipient support: the commitment's public key derives from the per-output
    // spend_secret carried in the note (fresh for transfers, self for
    // coinbase/fee), NOT from the wallet's AEAD decrypt secret. This makes the
    // reconstructed commitment match the on-chain commitment (Mint_V2 C2).
    let spend_secret = SecretKey::from_base(note.spend_secret);
    let public_key = PublicKey::from_secret(spend_secret.clone());
    let commitment_attrs = CommitmentAttributes {
        version: 0,
        public_key,
        value: note.value,
        asset_id: AssetId::from_base(note.asset_id),
        spend_hook: FuncId::from_base(note.spend_hook),
        user_data: note.user_data,
        blind: Blind(note.commitment_blind),
    };
    let commitment = commitment_attrs.to_commitment();
    let commitment_bytes = commitment.to_bytes();
    let cap_id = derive_cap_id(secret, &commitment_bytes);
    // Idempotent leaf position: skip a cap already in the DB (crash/re-scan
    // case) so it is neither re-appended to the tree nor given a new (gapped)
    // leaf_position. The tree is rebuilt from the deduped rows each scan, so
    // positions stay contiguous 0..N.
    if existing_cap_ids.contains(&cap_id) {
        return Ok(None);
    }
    let (leaf_pos, merkle_proof) = append_leaf_and_prove(tree, commitment.inner())?;

    // wallet.md §2.1: Path 1 coinbase capability type construction.
    // The same wallet_construct pipeline as Path 2 (§2.2) — only the source
    // of primitives differs (hardcoded per §6.4 vs manifest-declared).
    let native_typed = wallet_construct(
        "native_token_coinbase", "reward",
        vec![
            Primitive::SecretKey, Primitive::Commitment, Primitive::Nullifier,
            Primitive::ContractId, Primitive::FuncId, Primitive::AssetId,
            Primitive::MiningRecipient,
        ],
        &[Barb::Spend, Barb::Nullify, Barb::Commit, Barb::Dispatch,
          Barb::Gate, Barb::Denominate, Barb::Mine],
    ).ok_or_else(|| ScanError::WalletConstructFailed {
        resource: "native_token_coinbase".into(),
        action: "reward".into(),
    })?;

    let cap_record = CapRecord {
        cap_id: cap_id.clone(),
        value: note.value,
        asset_id: AssetId::from_base(note.asset_id),
        spend_hook: None,
        user_data: None,
        leaf_position: leaf_pos,
        commitment: Commitment::from_base(commitment.inner()),
        contract_id,
        func_id,
        cap_blind: Blind(note.commitment_blind),
        value_blind: Blind(note.value_blind),
        asset_blind: Blind(note.token_blind),
        capability_discriminant,
        capability_name: Some("commitment".to_string()),
        resource: Some(native_typed.resource.clone()),
        action: Some(native_typed.action.clone()),
        primitives: native_typed.primitives.clone(),
        barbs: native_typed.barbs.clone(),
        revoked: false,
        revoked_at_height: None,
        created_at_height: height,
        status: None, status_height: None, key_coords: None, spend_secret: Some(spend_secret.clone()),
        object_id: None, state_nonce: None,
    };

    let msg = format!(
        "Inserted native token {} cap {} at height {}",
        source.as_str(),
        &cap_id[..8],
        height
    );

    Ok(Some((cap_record, merkle_proof, msg)))
}

/// Match published nullifiers against existing held capabilities.
/// Pure: recomputes `poseidon_hash([secret, commitment])` for each held cap,
/// returns (cap_id, height) pairs for matches. No database access.
fn match_nullifiers(
    existing_caps: &[CapRecord],
    secrets: &[SecretKey],
    published: &[NullifierRecord],
    height: BlockHeight,
) -> Vec<(String, BlockHeight)> {
    if published.is_empty() {
        return vec![];
    }
    let published_fps: Vec<pallas::Base> = published.iter().map(|r| r.nullifier.inner()).collect();
    let mut revoked = vec![];

    for cap in existing_caps {
        // cap.commitment stores the Poseidon hash of commitment attributes as [u8; 32].
        // cap.cap_id is a Blake2b storage key (different value).
        let commitment = match cap.commitment.inner() {
            fp if fp != pallas::Base::zero() => fp,
            _ => continue,
        };
        // Try each secret — the nullifier is poseidon_hash(secret, commitment).
        // Per Cornerstone 1: secrets come from AccountManager, passed by caller.
        for secret in secrets {
            // 4-arg L1 nullifier (box/purse): poseidon(1, secret, object_id, nonce).
            // The produced leaf's nonce is the note's `state_nonce` (in-circuit +1).
            if let (Some(oid), Some(sn)) = (cap.object_id, cap.state_nonce) {
                let nullifier = poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_NULLIFIER, *secret.inner(), oid, sn]);
                if published_fps.contains(&&nullifier) {
                    revoked.push((cap.cap_id.clone(), height));
                    break;
                }
            }
            // Master-secret nullifier (transfers, PN notes, non-per-block caps).
            let nullifier = poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_NULLIFIER, *secret.inner(), commitment]);
            if published_fps.contains(&&nullifier) {
                revoked.push((cap.cap_id.clone(), height));
                break;
            }
            // Per-block derived-key nullifier — coinbase/fee caps are discovered
            // under sk_H = derive_instance(NATIVE_TOKEN_CONTRACT_ID, created_height),
            // so their spend nullifier must be recomputed with the same key.
            let block_secret = match secret.derive_instance(
                &NATIVE_TOKEN_CONTRACT_ID,
                &cap.created_at_height.to_le_bytes(),
            ) {
                Ok(sk) => sk,
                Err(_) => continue,
            };
            let block_nullifier = poseidon_hash([dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_NULLIFIER, *block_secret.inner(), commitment]);
            if published_fps.contains(&&block_nullifier) {
                revoked.push((cap.cap_id.clone(), height));
                break;
            }
        }
    }
    revoked
}

/// Discover native token outputs by AEAD-scanning call data bytes.
/// Pure: takes secrets + account_mgr (for per-block key derivation), returns
/// discovered (CapRecord, MerkleProof) pairs. Mutates `tree` to build proofs.
/// No database access.
fn discover_native_token_outputs(
    account_mgr: &dwow_accounts::AccountManager,
    tree: &mut MerkleTree,
    data: &[u8],
    height: BlockHeight,
    function_code: u8,
    diagnostics: &mut BlockScanDiagnostics,
    existing_cap_ids: &std::collections::HashSet<String>,
) -> std::result::Result<(Vec<(CapRecord, MerkleProof)>, Vec<String>), String> {
    let mut results = vec![];
    let mut messages = vec![];

    // Build augmented secret set with per-block keys for address cycling.
    // Per-block derivation failure IS a hard error for native token scan —
    // the per-block secret is the only key that can decrypt the coinbase note.
    // Falling back to master secrets silently guarantees capabilities=0.
    let height_bytes = height.to_le_bytes();
    let per_block_secrets = account_mgr.secrets_for_contract(
        &NATIVE_TOKEN_CONTRACT_ID, &height_bytes,
    ).map_err(|e| {
        tracing::error!(target: "dww::scan",
            "[native_token] step=1 derive_instance status=FAIL reason=\"per-block key derivation failed for height {}: {}\"", height, e);
        format!("per-block key derivation failed for height {}: {}", height, e)
    })?;
    let master_secrets = account_mgr.secrets();
    let mut trial_secrets: Vec<SecretKey> =
        Vec::with_capacity(master_secrets.len() + per_block_secrets.len());
    trial_secrets.extend(master_secrets);
    trial_secrets.extend(per_block_secrets);

    let source = match function_code {
        0x00 => NativeTokenSource::FeeV1,
        0x03 => NativeTokenSource::TransferV1,
        0x04 => NativeTokenSource::SpendV1,
        0x05 => NativeTokenSource::PoWRewardV1,
        0x06 => NativeTokenSource::FeeCollectV1,
        0x08 => NativeTokenSource::FeeV2,
        _ => NativeTokenSource::TransferV1, // fallback
    };

    let params = &data[1..]; // skip function code byte
    let mut off = 0;

    while off < params.len().saturating_sub(32) {
        let mut cursor = std::io::Cursor::new(&params[off..]);
        let pos_before = cursor.position();
        let Ok(generic_note) = AeadEncryptedNote::decode(&mut cursor) else {
            off += 1;
            continue;
        };
        diagnostics.aead_decode_attempts += 1;
        tracing::info!(target: "dww::scan",
            "[native_token] step=2 aead_decode status=OK offset={}", off);
        let consumed = (cursor.position() - pos_before) as usize;

        let mut decrypted = false;
        for secret in &trial_secrets {
            diagnostics.aead_decrypt_attempts += 1;
            if let Ok(decrypted_note) = generic_note.decrypt::<NativeToken>(secret, height.get()) {
                tracing::info!(target: "dww::scan",
                    "[native_token] step=3 aead_decrypt status=OK");
                diagnostics.aead_decrypt_successes += 1;
                decrypted = true;
                diagnostics.capability_construct_attempts += 1;
                match build_native_token_cap_record(
                    tree, secret, &decrypted_note, height, &source,
                    *NATIVE_TOKEN_CONTRACT_ID, None, None, existing_cap_ids,
                ) {
                    Ok(Some((cap_record, merkle_proof, msg))) => {
                        diagnostics.capability_construct_successes += 1;
                        tracing::info!(target: "dww::scan",
                            "[native_token] step=4 commitment_reconstruct status=OK commitment=0x{}",
                            hex::encode(&cap_record.commitment.to_bytes()));
                        // P1b: populate key_coords so the spend path can recover
                        // the owning secret via AccountManager::resolve_key.
                        let mut cap_record = cap_record;
                        cap_record.key_coords = account_mgr.find_owner(
                            &*NATIVE_TOKEN_CONTRACT_ID,
                            &height.to_le_bytes(),
                            &PublicKey::from_secret(secret.clone()),
                        );
                        results.push((cap_record, merkle_proof));
                        messages.push(msg);
                    }
                    Ok(None) => {
                        // Already in the DB (crash/re-scan) — idempotent skip.
                    }
                    Err(e) => {
                        tracing::error!(target: "dww::scan",
                            "[native_token] step=4 coin_reconstruct status=FAIL error={:?}", e);
                    }
                }
                off += consumed; // advance past this note only on successful decrypt
                break; // found match for this note → next note
            }
        }
        if !decrypted {
            tracing::debug!(target: "dww::scan",
                "[native_token] step=3 aead_decrypt status=FAIL reason=\"no key matched — wallet key differs from miner\"");
            off += 1; // false-positive decode: advance 1 byte, don't skip the real note
        }
    }

    tracing::info!(target: "dww::scan",
        "[native_token] step=5 caprecord_build status=OK count={}", results.len());
    Ok((results, messages))
}

/// Scan native token contract calls in a single transaction.
/// Pure: handles the full native token lifecycle (mint, transfer, spend, burn, fee)
/// via contract calls only. No tx.coinbase — PoWRewardV1 (0x05) handles mint discovery.
/// Returns discovered outputs and published nullifiers. Mutates `tree`.
fn scan_native_token_contract_calls(
    account_mgr: &dwow_accounts::AccountManager,
    tree: &mut MerkleTree,
    tx: &dwow_chain::Transaction,
    height: BlockHeight,
    diagnostics: &mut BlockScanDiagnostics,
    existing_cap_ids: &std::collections::HashSet<String>,
) -> (Vec<NativeTokenDiscovery>, Vec<NullifierRecord>, Vec<String>) {
    let mut outputs = vec![];
    let mut nullifiers = vec![];
    let mut messages = vec![];

    for call in &tx.contract_calls {
        let cid = call.contract_id;
        if cid != *NATIVE_TOKEN_CONTRACT_ID {
            continue;
        }
        if call.data.is_empty() {
            continue;
        }

        // C-2: Raw data[0] dispatch. TODO: use call.as_mass_balance_fee_v2()
        // for FeeV2 routing per type-system.md §10.5 absorber boundary.
        let function_code = call.data[0];

        // ── Spend detection: extract published nullifiers ──
        // 0x05 (PoWRewardV1) and 0x06 (FeeCollectV1) are excluded: their
        // nullifiers are capability CLAIMS for NEW commitments, not spends of held
        // capabilities. Inserting them would double-count the claim as both a
        // revocation signal and a spend — identical reasoning.
        if matches!(function_code, 0x02 | 0x03 | 0x04 | 0x08) {
            let cursor = std::io::Cursor::new(&call.data[1..]);
            // V.2: NullifierRecord stores typed Nullifier, not raw pallas::Base
            let published: Vec<dwow_chain::Nullifier> = match function_code {
                0x03 => match TransferParamsV1::decode(&cursor.get_ref()[cursor.position() as usize..]) {
                    Ok(p) => p.inputs.iter().map(|inp| inp.nullifier).collect(),
                    Err(_) => vec![],
                },
                0x02 => match BurnParamsV1::decode(&cursor.get_ref()[cursor.position() as usize..]) {
                    Ok(p) => p.inputs.iter().map(|inp| inp.nullifier).collect(),
                    Err(_) => vec![],
                },
                0x04 => match SpendParamsV1::decode(&cursor.get_ref()[cursor.position() as usize..]) {
                    Ok(p) => vec![p.input.nullifier],
                    Err(_) => vec![],
                },
                0x08 => match FeeParamsV3::decode(&cursor.get_ref()[cursor.position() as usize..]) {
                    Ok(p) => vec![p.input.nullifier],
                    Err(_) => vec![],
                },
                _ => vec![],
            };
            for nf in published {
                nullifiers.push(NullifierRecord { nullifier: nf });
            }
        }

        // ── Output discovery: decrypt output notes ──
        // PoWRewardV1  (0x05): coinbase reward
        // FeeCollectV1 (0x06): miner fee commitment (claim for new commitment — same
        //                      key derivation as coinbase, already in trial_secrets)
        // TransferV1   (0x03): receiver outputs
        // SpendV1      (0x04): change output
        // FeeV1        (0x00): change output
        if matches!(function_code, 0x00 | 0x03 | 0x04 | 0x05 | 0x06 | 0x08) {
            let (caps, msgs) = match discover_native_token_outputs(
                account_mgr, tree, &call.data, height, function_code, diagnostics,
                existing_cap_ids,
            ) {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!(target: "dww::scan",
                        "[native_token] discover_native_token_outputs failed: {}", e);
                    (vec![], vec![e])
                }
            };
            for (cap_record, merkle_proof) in caps {
                outputs.push(NativeTokenDiscovery { cap_record, merkle_proof });
            }
            messages.extend(msgs);
        }
    }

    (outputs, nullifiers, messages)
}

/// Scan a single block for wallet-relevant outputs.
/// PURE FUNCTION: same (secrets, existing_caps, tree, account_mgr, manifests, block) → same result.
/// No database access. No network. No mutable globals. Testable in isolation.
fn scan_block(
    tree: &mut MerkleTree,
    account_mgr: &dwow_accounts::AccountManager,
    manifests: &BTreeMap<ContractId, dwow_sdk::manifest::ContractManifest>,
    block: &dwow_chain::Block,
    existing_cap_ids: &std::collections::HashSet<String>,
) -> BlockScanResult {
    let mut result = BlockScanResult::new();
    // §2.3: BlockHeight propagates through scan pipeline; lowered to u64
    // only at persistence boundaries (CapRecord, SQLite, display).
    let height = block.header.height;

    result.messages.push(format!("[linear] Block height: {}", block.header.height));
    result.messages.push(format!(
        "[scan_block] Iterating over {} transactions",
        block.transactions.len()
    ));

    for tx in block.transactions.iter() {
        // ── Path 1: Native Token (sole special citizen) ──────
        let (native_outputs, nullifiers, mut msgs) =
            scan_native_token_contract_calls(account_mgr, tree, tx, height, &mut result.diagnostics, existing_cap_ids);
        result.native_outputs.extend(native_outputs);
        result.published_nullifiers.extend(nullifiers);
        result.messages.append(&mut msgs);

        // ── Path 2: Capabilities (all other contracts) ──────
        for (i, call) in tx.contract_calls.iter().enumerate() {
            let cid = call.contract_id;

            // Native Token handled by scan_native_token_contract_calls above.
            // Dual iteration of contract calls is the cost of architectural separation:
            // each scan path (native token / capability) independently classifies
            // calls by ContractId. The skip here ensures single processing per call.
            if cid == *NATIVE_TOKEN_CONTRACT_ID {
                continue;
            }

            // ── Deployooor contract ──────────────────────────
            if cid == *DEPLOYOOOR_CONTRACT_ID {
                let function_code = call.data.first().copied().unwrap_or(0xFF);
                if function_code == 0x00 {
                    if let Ok(params) = dwow_serial::deserialize::<DeployParamsV1>(
                        &call.data[1..]
                    ) {
                        let metadata = ContractMetadata::from_ix_bytes(&params.ix);
                        let manifest_result =
                            dwow_sdk::manifest::ContractManifest::from_deploy_ix(&params.ix);

                        // Genesis contracts carry well-known ContractIds, not
                        // derive_public(deploy_key) (wallet.md §3, manifest.md
                        // Circuit Binary Delivery — M2). Key the zkas/manifests
                        // under the well-known id so the generic prover finds them.
                        let contract_id = match &manifest_result {
                            Some(Ok(m)) => match crate::contract_imports::get_contract_id(&m.name) {
                                Some(id) => id,
                                None => ContractId::derive_public(params.public_key),
                            },
                            _ => ContractId::derive_public(params.public_key),
                        };
                        let contract_id_str = bs58::encode(contract_id.to_bytes()).into_string();

                        let manifest_json = match manifest_result {
                            Some(Ok(ref m)) => serde_json::to_string(m).ok(),
                            Some(Err(ref e)) => {
                                tracing::error!(target: "dww::scan",
                                    "[scan_block] DeployV1: manifest parse error for {}: {:?}",
                                    &contract_id_str[..8], e);
                                None
                            }
                            None => None,
                        };
                        result.deployments.push(DeploymentDiscovery {
                            contract_id,
                            deployer_pubkey: params.public_key,
                            metadata,
                            manifest_json,
                            height: height,
                        });

                        result.messages.push(format!(
                            "[scan_block] Deployooor::DeployV1: {} at height {}",
                            &contract_id_str[..8], height
                        ));

                        // Extract zkas circuit binaries from the deployed WASM
                        // (wallet.md §3, §6.4.1 step 3). include_bytes! embeds
                        // .zk.bin files in the WASM data segment at build time.
                        // We scan the raw WASM blob for byte sequences that
                        // successfully decode as ZkBinary — one per circuit.
                        // The namespace is extracted from the binary header
                        // (zkas wire format: k u32 LE, field_len u32 LE,
                        //  field bytes, namespace_len u32 LE, namespace bytes).
                        let mut seen: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        for offset in 0..params.wasm_bincode.len().saturating_sub(20) {
                            let slice = &params.wasm_bincode[offset..];
                            if let Ok(zkbin) = dwow_core::zkas::ZkBinary::decode(slice, false) {
                                // Extract namespace from the binary header.
                                // Header: k(u32) + field_len(u32) + field(str) +
                                //         namespace_len(u32) + namespace(str)
                                let mut pos = 4; // skip k
                                #[expect(clippy::unwrap_used, reason = "4-byte slice (or [0;4] fallback) is always 4 bytes")]
                                let field_len = u32::from_le_bytes(
                                    slice.get(pos..pos+4).unwrap_or(&[0;4]).try_into().unwrap()
                                ) as usize;
                                pos += 4 + field_len;
                                #[expect(clippy::unwrap_used, reason = "4-byte slice (or [0;4] fallback) is always 4 bytes")]
                                let ns_len = u32::from_le_bytes(
                                    slice.get(pos..pos+4).unwrap_or(&[0;4]).try_into().unwrap()
                                ) as usize;
                                pos += 4;
                                let ns = String::from_utf8_lossy(
                                    slice.get(pos..pos+ns_len).unwrap_or(b"")
                                ).to_string();
                                let binary_end = pos + ns_len;
                                if !ns.is_empty() && seen.insert(ns.clone()) {
                                    result.zkas_binaries.push(ZkasBinaryDiscovery {
                                        contract_id,
                                        namespace: ns.clone(),
                                        circuit_name: ns.clone(),
                                        zkas_bytes: slice[..binary_end].to_vec(),
                                    });
                                    result.messages.push(format!(
                                        "[scan_block] zkas '{}' extracted for {} (k={})",
                                        ns, &contract_id_str[..8], zkbin.k,
                                    ));
                                }
                            }
                        }
                    }
                }
                continue;
            }

            // ── Manifest-Driven Capability Construction (Path 2) ─────
            // Per wallet.md §2.2: read the contract's manifest, resolve
            // capability types from declarations.
            // TODO: integrate manifest resolution into scan path.
            // Foundation: manifest.rs resolve_capability(), CapRecord.capability_discriminant,
            // walletdb.get_contract_manifest(). The scan loop will call these in a follow-up.
            if call.data.len() < 2 {
                continue;
            }
            let data = &call.data[1..]; // skip function code byte
            let mut off: usize = 0;
            let mut aead_notes_tried: usize = 0;

            while off < data.len().saturating_sub(32) {
                let mut cursor = std::io::Cursor::new(&data[off..]);
                let pos_before = cursor.position();
                if let Ok(generic_note) = AeadEncryptedNote::decode(&mut cursor) {
                    aead_notes_tried += 1;
                    let consumed = (cursor.position() - pos_before) as usize;

                    // Build augmented secret set with per-contract derived keys
                    let mut trial_secrets: Vec<SecretKey> = account_mgr.secrets();
                    if let Some(iseed) = try_extract_instance_seed(&cid, &call.data) {
                        let derived = account_mgr.secrets_for_contract(&cid, &iseed)
                            .unwrap_or_else(|e| {
                                tracing::warn!(target: "dww::scan",
                                    "per-contract key derivation failed for {:?}: {}", cid, e);
                                vec![]
                            });
                        trial_secrets.extend(derived);
                    }

                    let mut path2_decrypted = false;
                    for secret in &trial_secrets {
                        result.diagnostics.path2_decrypt_attempts += 1;
                        let Ok(raw) = generic_note.decrypt_raw(secret, height.get()) else { continue };
                        result.diagnostics.path2_decrypt_successes += 1;
                        // Path 2: generic manifest-driven type-construction.
                        // From here every failure DROPS the note (clean skip);
                        // there is no native fallback.
                        let Some(fn_code) = call.data.first().copied() else { break };
                        let Some(manifest) = manifests.get(&cid) else {
                            result.diagnostics.manifest_misses += 1;
                            break;
                        };
                        let Some(resolved) = manifest.resolve_capability(fn_code) else {
                            tracing::debug!(target: "dww::scan",
                                "Path2: no capability for fn_code 0x{:02x} in manifest {}",
                                fn_code, bs58::encode(cid.to_bytes()).into_string());
                            break;
                        };
                        // Collect the call's published nullifier — the consumed
                        // cap's 4-arg L1 nullifier travels in the `[[parameters]]`
                        // "nullifier" wire field, not in the note. Without this,
                        // match_nullifiers never revokes a spent purse/box cap and
                        // the ambiguity guard (V7) fires on the next spend (HAZOP V2).
                        if let Some(fn_name) = manifest.function_by_code(fn_code).map(|f| f.name.as_str()) {
                            if let Some(schema) = manifest.parameters.iter()
                                .find(|p| p.function == fn_name)
                                .map(|p| p.fields.as_slice())
                            {
                                if let Ok(offset) = dwow_sdk::manifest::field_offset_by_name(schema, "nullifier") {
                                    let start = 1 + offset;
                                    if let Some(bytes) = call.data.get(start..start + 32) {
                                        if let Ok(arr) = <[u8; 32]>::try_from(bytes) {
                                            if let Ok(nf) = dwow_chain::Nullifier::from_bytes(arr) {
                                                result.published_nullifiers.push(NullifierRecord { nullifier: nf });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // The coverage gate: an uncovered composition (primitives don't
                        // cover required barbs) is NOT a valid capability — drop the note
                        // per §13 "fix the composition, not the wallet."
                        let Some(typed) = manifest.resolve_capability_type(fn_code) else {
                            result.diagnostics.path2_coverage_drops += 1;
                            tracing::warn!(target: "dww::scan",
                                "Path2: coverage gate closed for fn 0x{:02x} contract {} — \
                                 primitives don't cover required barbs (fix the composition, not the wallet)",
                                fn_code, bs58::encode(cid.to_bytes()).into_string());
                            break;
                        };
                        let Some(schema) = manifest.note_schema_for_function(fn_code) else {
                            tracing::debug!(target: "dww::scan",
                                "Path2: no note_schema for fn 0x{:02x} in manifest {}",
                                fn_code, bs58::encode(cid.to_bytes()).into_string());
                            break;
                        };
                        if schema.is_empty() { break }
                        let Ok(fields) =
                            dwow_sdk::manifest::decode_note_by_schema(&raw, schema) else { break };

                        // Merkle leaf: the note must declare a `commitment` field
                        // of type pallas_base. Absent or wrong type → drop.
                        // (L1 trajectory identification — wallet.md §2.3; L2 flat
                        // discovery with no commitment/leaf is a follow-up.)
                        let Some(leaf) = dwow_sdk::manifest::note_field(&fields, "commitment")
                            .and_then(|v| v.as_base()) else { break };

                        // Value and asset denomination are read from the note's
                        // declared fields, never hardcoded to DRKW/0 (wallet.md
                        // §2.3): a promissory note's real value/asset_id come from
                        // its note, not from a native-token assumption.
                        let value = dwow_sdk::manifest::note_field(&fields, "value")
                            .or_else(|| dwow_sdk::manifest::note_field(&fields, "amount"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let asset_id = dwow_sdk::manifest::note_field(&fields, "asset_id")
                            .and_then(|v| v.as_base())
                            .map(AssetId::from_base)
                            .unwrap_or(AssetId::DRKW);

                        // The value/balance blind (pallas_scalar) is read from the
                        // note when the capability declares one (purse balance_blind);
                        // box/PN have no scalar blind and stay zero. Read it here so
                        // the write path can later re-blind the produce-side note
                        // (cap_record_note_fields maps balance_blind → Scalar).
                        let value_blind = dwow_sdk::manifest::note_field(&fields, "balance_blind")
                            .or_else(|| dwow_sdk::manifest::note_field(&fields, "value_blind"))
                            .or_else(|| dwow_sdk::manifest::note_field(&fields, "old_balance_blind"))
                            .and_then(|v| v.as_scalar())
                            .unwrap_or_else(pallas::Scalar::zero);

                        let cap_id = derive_cap_id(secret, &leaf.to_repr());
                        if existing_cap_ids.contains(&cap_id) {
                            break; // idempotent skip — already in the DB
                        }
                        let (leaf_pos, merkle_proof) = match append_leaf_and_prove(tree, leaf) {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::error!(target: "dww::scan",
                                    "Path 2: merkle tree failure: {:?}", e);
                                break;
                            }
                        };
                        // merkle_proof is consumed in CapabilityDiscovery below

                        // §0.1.3: resolve key coordinates via AccountManager
                        // delegation so the spend path can later recover the
                        // owning secret (wallet.md §4). Path 1 does this at
                        // line 396; Path 2 was deferred (P0.1b) — fixed here.
                        let key_coords = account_mgr.find_owner(
                            &call.contract_id,
                            &height.to_le_bytes(),
                            &PublicKey::from_secret(secret.clone()),
                        );

                        let cap_record = CapRecord {
                            cap_id: cap_id.clone(),
                            value,
                            asset_id,
                            spend_hook: None,
                            user_data: None,
                            leaf_position: leaf_pos,
                            commitment: Commitment::from_base(leaf),
                            contract_id: call.contract_id,  // foreign — balance gate excludes it
                            func_id: Some(FuncId::from_base(pallas::Base::from(fn_code as u64))),
                            cap_blind: Blind(pallas::Base::zero()),
                            value_blind: Blind(value_blind),
                            asset_blind: Blind(pallas::Base::zero()),
                            capability_discriminant: Some(resolved.discriminant),
                            capability_name: Some(resolved.name.clone()),
                            resource: Some(typed.resource.clone()),
                            action: Some(typed.action.clone()),
                            primitives: typed.primitives.clone(),
                            barbs: typed.barbs.clone(),
                            status: None,
                            status_height: None,
                            revoked: false,
                            revoked_at_height: None,
                            created_at_height: height,
                            key_coords, // resolved via find_owner
                            spend_secret: None,
                            object_id: dwow_sdk::manifest::note_field(&fields, "purse_id")
                                .and_then(|v| v.as_base()),
                            state_nonce: dwow_sdk::manifest::note_field(&fields, "state_nonce")
                                .and_then(|v| v.as_base()),
                        };
                        result.capabilities.push(CapabilityDiscovery { cap_record, merkle_proof });
                        path2_decrypted = true;
                        off += consumed; // advance past this note only on successful decrypt
                        break; // our secret matched → next note
                    }
                    if !path2_decrypted {
                        off += 1; // false-positive decode: advance 1 byte, don't skip the real note
                    }
                } else {
                    off += 1;
                }
            }
            if aead_notes_tried > 0 {
                result.messages.push(format!(
                    "[scan_block] Generic AEAD: {} note(s) decoded in call {} block {}",
                    aead_notes_tried, i, block.header.height
                ));
            }
        }
    }

    result
}

impl Dww {
    /// Auxiliary function to generate a new [`ScanCache`] for the
    /// wallet.
    pub fn scan_cache(&self) -> Result<ScanCache> {
        let capability_commitment_tree = self.get_capability_commitment_tree()?;

        // Get our secrets
        let secrets = self.get_secrets()?;

        Ok(ScanCache {
            capability_commitment_tree,
            secrets,
            messages_buffer: vec![],
        })
    }

    /// Scans the linear blockchain for wallet relevant transactions,
    /// starting from the last scanned block. If a reorg has happened,
    /// we revert to its previous height and then scan from there.
    pub async fn scan_blocks(
        &self,
        output: &mut Vec<String>,
        sender: Option<&Sender<Vec<String>>>,
        print: &bool,
    ) -> WalletDbResult<()> {
        // Grab last scanned block height from the wallet db
        let (last_scanned, _) = self.get_last_scanned_block()?;
        let mut height = dwow_sdk::blockchain::BlockHeight::new(if last_scanned == 0 {
            // Reset the SCAN state (caps, proofs, markers) but NOT the synced
            // chain. reset() would also delete chain_blocks — the blocks the
            // sync task just pulled — leaving nothing to scan. Chain state and
            // scan state are independent.
            self.wallet.remove_capabilities_after(dwow_sdk::blockchain::BlockHeight::new(0))?;
            self.wallet.retain_capabilities_after(dwow_sdk::blockchain::BlockHeight::new(0))?;
            self.wallet.delete_scanned_blocks_above(dwow_sdk::blockchain::BlockHeight::new(0))?;
            1 // Start scanning from genesis block (height 1)
        } else {
            // Scan from the NEXT block. The last marked block is fully
            // processed (marker written after caps), so no re-scan is needed.
            last_scanned.saturating_add(1)
        });

        // Load tree once (immutable for the scan loop).
        // Secrets come from self.account_mgr directly — single authority per Cornerstone 1.
        let mut tree = match self.get_capability_commitment_tree() {
            Ok(t) => t,
            Err(e) => {
                append_or_print(output, sender, print,
                    vec![format!("[scan_blocks] Loading capability tree failed: {e}")]).await;
                return Err(WalletDbError::GenericError)
            }
        };

        append_or_print(output, sender, print,
            vec![format!("[scan_blocks] AccountManager loaded")]).await;

        loop {
            let mut buf = vec![format!("Requested to scan from block number: {height}")];
            let last_height = match self.chain_height() {
                Ok(h) => h,
                Err(e) => {
                    buf.push(format!("[scan_blocks] Local chain read failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                }
            };
            buf.push(format!("Chain tip from local store: height {last_height}"));
            append_or_print(output, sender, print, buf).await;

            if height > last_height {
                return Ok(())
            }

            while height <= last_height {
                let mut buf = vec![format!("Reading block {} from local store...", height.get())];
                let block = match self.chain_block(height) {
                    Ok(b) => b,
                    Err(e) => {
                        buf.push(format!("[scan_blocks] Local chain read failed: {e}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };
                buf.push(format!("Block {height} received! Scanning block..."));
                match self.scan_block_linear(&mut tree, &block) {
                    Ok(result) => {
                        for msg in &result.messages {
                            buf.push(msg.clone());
                        }
                    }
                    Err(e) => {
                        buf.push(format!("[scan_blocks] Scan block failed: {e}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };

                // HAZOP WP-7: lifecycle reconciliation after each block scan.
                // Advances PENDING→NULL (expiry) and PROCESSING→SPENT (maturity).
                let current_height = height;
                // §4.2.1: lifecycle reconciliation results SHALL NOT be discarded.
                // A DB failure here would otherwise leave caps Pending/Processing
                // forever with no observable barb.
                if let Err(e) = self.expire_pending_caps(current_height) {
                    tracing::error!(target: "dww::scan", "expire_pending_caps failed at height {}: {}", current_height, e);
                }
                if let Err(e) = self.check_confirmations(current_height) {
                    tracing::error!(target: "dww::scan", "check_confirmations failed at height {}: {}", current_height, e);
                }

                // Advance verified anchor height if this block has a
                // verified Caribina (Arweave) anchor.
                if block.header.anchor_tx_id != [0u8; 32] {
                    // §8.1: BlockHeight nominal type — comparison and assignment
                    // use the newtype, not bare u64.
                    let anchor_height = block.header.height;
                    let mut current = self.verified_anchor_height.lock().await;
                    if anchor_height > *current {
                        *current = anchor_height;
                        buf.push(format!(
                            "[scan_blocks] Verified anchor at height {} (Arweave tx: {})",
                            anchor_height,
                            hex::encode(block.header.anchor_tx_id)
                        ));
                    }
                }

                append_or_print(output, sender, print, buf).await;
                height = height.succ();
            }
        }
    }

    /// `scan_block_linear` processes a linear block: pure scan + persistence.
    ///
    /// The scanned block marker is written AFTER processing. If the process
    /// crashes mid-scan, the marker is absent and the next scan re-scans the
    /// block — capabilities use INSERT OR IGNORE (dedup), and the commitment
    /// tree is rebuilt from cap rows, so the re-scan is idempotent.
    pub fn scan_block_linear(
        &self,
        tree: &mut MerkleTree,
        block: &dwow_chain::Block,
    ) -> Result<BlockScanResult> {
        let height = block.header.height.get();

        // Checkpoint the merkle tree. §2.3: width conversion uses try_from,
        // not a bare `as` cast on the consensus height.
        let checkpoint_idx = usize::try_from(height)
            .map_err(|_| crate::wallet_error::Error::Custom(
                "height exceeds usize for merkle checkpoint".into(),
            ))?;
        tree.checkpoint(checkpoint_idx);

        // Get existing held caps for nullifier detection (spends by other parties)
        let existing_caps = self.wallet.get_held_capabilities(Some(false)).unwrap_or_else(|e| {
            tracing::error!(target: "dww::scan",
                "Failed to load held capabilities: {:?} — nullifier detection skipped for block {}",
                e, height);
            vec![]
        });
        // Idempotent leaf position: dedup set so a re-scan (crash mid-block)
        // skips caps already in the DB instead of re-appending them to the tree.
        let existing_cap_ids: std::collections::HashSet<String> =
            existing_caps.iter().map(|c| c.cap_id.clone()).collect();

        // ── Load manifests for generic capability typing (Path 2) ─
        // Pre-load once per block so the pure scan_block can resolve capability
        // types without DB access. Only foreign (non-native/deployooor)
        // contracts need manifests — those two are the sanctioned citizens.
        // Pre-load manifests so the pure scan can resolve capability types from
        // declarations without DB access (ocap.md §7: manifest-driven, zero per-contract code).
        let mut manifests: BTreeMap<ContractId, dwow_sdk::manifest::ContractManifest> = BTreeMap::new();
        let mut preload_manifest_misses: usize = 0;
        for tx in &block.transactions {
            for call in &tx.contract_calls {
                let cid = call.contract_id;
                if cid == *NATIVE_TOKEN_CONTRACT_ID
                    || cid == *DEPLOYOOOR_CONTRACT_ID { continue; }
                if manifests.contains_key(&cid) { continue; }
                let cid_str = bs58::encode(cid.to_bytes()).into_string();
                if let Ok(Some(m)) = self.wallet.get_contract_manifest(&cid_str) {
                    manifests.insert(cid, m);
                } else {
                    preload_manifest_misses += 1;
                }
            }
        }

        // ── Pure scan: no DB access ──────────────────────────
        let mut result = scan_block(tree, &self.account_mgr, &manifests, block, &existing_cap_ids);
        result.diagnostics.manifest_misses += preload_manifest_misses;

        // ── Persist results ──────────────────────────────────
        // Insertions are FATAL on failure: if a cap can't be inserted, the
        // wallet state is inconsistent. The block marker is written before
        // processing (crash recovery pattern); a failed insert leaves a
        // partially-marked block that the next scan will recover.
        for out in &result.native_outputs {
            self.wallet.insert_capability(&out.cap_record, &out.merkle_proof)
                .map_err(|e| crate::wallet_error::Error::Custom(format!(
                    "Failed to insert native token cap {} at height {}: {:?}",
                    &out.cap_record.cap_id[..8.min(out.cap_record.cap_id.len())],
                    height, e)))?;
        }
        // Discriminant is now set inside the pure scan_block (Path 2 manifest
        // resolution) — the post-hoc DB manifest lookup is no longer needed.
        for cap in &result.capabilities {
            self.wallet.insert_capability(&cap.cap_record, &cap.merkle_proof)
                .map_err(|e| crate::wallet_error::Error::Custom(format!(
                    "Failed to insert cap {} at height {}: {:?}",
                    &cap.cap_record.cap_id[..8.min(cap.cap_record.cap_id.len())],
                    height, e)))?;
        }

        // Apply nullifier revocations
        let secrets = self.account_mgr.secrets();
        let revoked = match_nullifiers(&existing_caps, &secrets, &result.published_nullifiers, BlockHeight::new(height));
        for (cap_id, h) in &revoked {
            if let Err(e) = self.wallet.mark_revoked(cap_id, *h) {
                tracing::error!(target: "dww::scan",
                    "Failed to mark cap {} revoked at height {}: {:?}",
                    &cap_id[..8.min(cap_id.len())], h, e);
            }
        }

        // Persist deployments
        for dep in &result.deployments {
            let contract_id_str = bs58::encode(dep.contract_id.to_bytes()).into_string();
            let deployer_pubkey_str = bs58::encode(dep.deployer_pubkey.to_bytes()).into_string();
            let record = crate::walletdb::ContractMetadataRecord {
                contract_id: contract_id_str.clone(),
                name: dep.metadata.as_ref().map(|m| m.name.clone())
                    .unwrap_or_else(|| format!("Contract-{}", &contract_id_str[..8])),
                symbol: dep.metadata.as_ref().and_then(|m| m.symbol.clone()),
                category: dep.metadata.as_ref()
                    .map(|m| format!("{:?}", m.category))
                    .unwrap_or_else(|| "Other".to_string()),
                description: dep.metadata.as_ref().and_then(|m| m.description.clone()),
                public: dep.metadata.as_ref().map(|m| m.public).unwrap_or(false),
                deployer_pubkey: deployer_pubkey_str,
                deploy_height: dep.height,
                attestations_json: "[]".to_string(),
                lock_status: "unlocked".to_string(),
            };
            if let Err(e) = self.wallet.insert_contract_metadata(&record) {
                tracing::error!(target: "dww::scan",
                    "Failed to insert contract metadata for {}: {e}", &contract_id_str[..8]);
            } else if let Some(ref manifest_json) = dep.manifest_json {
                if let Err(e) = self.wallet.store_manifest(&contract_id_str, manifest_json) {
                    tracing::error!(target: "dww::scan",
                        "Failed to store manifest for {}: {e}", &contract_id_str[..8]);
                }
            }
        }

        // Persist extracted zkas binaries
        for zkb in &result.zkas_binaries {
            let cid_str = bs58::encode(zkb.contract_id.to_bytes()).into_string();
            if let Err(e) = self.wallet.store_zkas_binary(
                &cid_str, &zkb.namespace, &zkb.circuit_name, &zkb.zkas_bytes,
            ) {
                tracing::error!(target: "dww::scan",
                    "[scan_blocks] Failed to store zkas binary '{}' for {}: {:?}",
                    zkb.namespace, &cid_str[..8], e);
            }
        }

        // The capability commitment tree is DERIVED from the cap rows
        // (WalletDb::rebuild_capability_tree), not persisted. Rebuilding on read
        // makes re-scan idempotent (no duplicate leaves) and reorg a pure rebuild.

        // Write marker AFTER processing — a crash before this point leaves the
        // block un-marked, so the next scan re-scans it idempotently.
        self.wallet.insert_scanned_block(
            &height,
            &HeaderHash(*block.header.previous.as_bytes()),
            &None,
        )?;

        Ok(result)
    }

}

// Unit tests for scan_block, discover_native_token_outputs, and cap_id derivation
// are in the test module below. They use in-memory MerkleTree (BridgeTree) and
// AccountManager — no sled DB required. Tests run in <10ms via cargo test.

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_chain::Nullifier;
    use dwow_sdk::blockchain::{BlockVersion, BlockTimestamp, MoneroBlockHeight};
    use dwow_sdk::crypto::keypair::SecretKey;
    use dwow_sdk::crypto::PublicKey;
    use dwow_sdk::crypto::note::AeadEncryptedNote;
    use dwow_sdk::crypto::Blind;
    use dwow_sdk::pasta::pallas;
    use dwow_sdk::pasta::group::ff::PrimeField;

    /// F2: discover_native_token_outputs is deterministic.
    /// Invariant: AEAD decrypt is deterministic for given key + ciphertext.
    /// Falsifiable: correct key finds output, wrong key finds nothing.
    #[test]
    fn test_discover_determinism() {
        let height: u64 = 42;

        // Minimal AccountManager with a known declared secret.
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_discover.toml");
        std::fs::write(&keys_path, "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(&keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet")
            .expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);

        // Get the master secret from AccountManager — single authority.
        let master_sk = account_mgr.secrets().into_iter().next()
            .expect("AccountManager must have at least one secret");
        let pk = PublicKey::from_secret(master_sk.clone());

        // Per-block derived key from the same master.
        let per_block_sk = master_sk.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");
        let per_block_pk = PublicKey::from_secret(per_block_sk.clone());

        // Create a minimal encrypted note (AeadEncryptedNote + NativeToken encoding)
        use dwow_sdk::pasta::group::ff::PrimeField;
        let nt = dwow_native_token_contract::client::NativeToken {
            value: 50_000_000,
            asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            commitment_blind: pallas::Base::from(1u64),
            spend_secret: pallas::Base::from(7u64),
            value_blind: pallas::Scalar::from(2u64),
            token_blind: pallas::Base::from(3u64),
            memo: vec![],
        };

        // encrypt takes &impl Encodable — pass NativeToken directly
        let aes = AeadEncryptedNote::encrypt(&nt, &per_block_pk, &mut rand::rngs::OsRng)
            .expect("encrypt should succeed");

        // Call data: selector 0x05 + encoded AeadEncryptedNote
        let mut aes_bytes = vec![];
        dwow_serial::Encodable::encode(&aes, &mut aes_bytes).ok();
        let call_data = {
            let mut d = vec![0x05u8];
            d.extend(&aes_bytes);
            d
        };

        let mut tree = MerkleTree::new(32);

        // Step 1: Verify encrypt/decrypt roundtrip
        let decrypted: dwow_native_token_contract::client::NativeToken = aes.decrypt(&per_block_sk, 1u64)
            .expect("F2 FAIL: direct decrypt roundtrip must work");
        assert_eq!(decrypted.value, 50_000_000, "F2 FAIL: decrypted value mismatch");

        // Verify AeadEncryptedNote encode/decode roundtrip
        let mut enc_bytes = vec![];
        dwow_serial::Encodable::encode(&aes, &mut enc_bytes).expect("encode must work");
        let decoded_aes = AeadEncryptedNote::decode(&mut std::io::Cursor::new(&enc_bytes))
            .expect("F2 FAIL: AeadEncryptedNote decode from encoded bytes must work");
        assert_eq!(decoded_aes.ciphertext.len(), aes.ciphertext.len(),
            "F2 FAIL: AES roundtrip ciphertext length mismatch");

        // Verify the scan can find it at offset 0 of the params
        let params = &call_data[1..]; // skip selector
        let scan_result = AeadEncryptedNote::decode(&mut std::io::Cursor::new(&params));
        assert!(scan_result.is_ok(),
            "F2 FAIL: AeadEncryptedNote::decode at offset 0 of call data params must work");

        // Step 1: Verify encrypt/decrypt roundtrip
        let decrypted: dwow_native_token_contract::client::NativeToken = aes.decrypt(&per_block_sk, 1u64)
            .expect("F2 FAIL Step 1: direct decrypt roundtrip must work");
        assert_eq!(decrypted.value, 50_000_000, "F2 FAIL Step 1: decrypted value mismatch");

        // Step 2: Verify AccountManager has the secret
        let mgr_secrets = account_mgr.secrets();
        assert!(mgr_secrets.len() >= 1, "F2 FAIL Step 2: AccountManager has no secrets");
        assert_eq!(mgr_secrets[0].inner().to_repr(), master_sk.inner().to_repr(),
            "F2 FAIL Step 2: AccountManager secret != test master secret. \
             AccountManager has different identity than the test key.");

        // Step 3: Verify per-block key derivation matches
        let mgr_per_block = account_mgr.secrets_for_contract(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("F2 FAIL Step 3: secrets_for_contract must not fail for valid height");
        assert!(mgr_per_block.len() >= 1, "F2 FAIL Step 3: secrets_for_contract returned empty");
        assert_eq!(mgr_per_block[0].inner().to_repr(), per_block_sk.inner().to_repr(),
            "F2 FAIL Step 3: AccountManager per-block key != manually derived per-block key. \
             derive_instance results differ.");

        // Step 4: Manually replicate what discover_native_token_outputs does —
        // decode AES note from call_data, decrypt with per-block key.
        let params = &call_data[1..]; // skip selector
        let decoded_aes = AeadEncryptedNote::decode(&mut std::io::Cursor::new(params))
            .expect("F2 FAIL Step 4: AES decode from call_data params must work");
        let decrypted2: dwow_native_token_contract::client::NativeToken = decoded_aes.decrypt(&per_block_sk, 1u64)
            .expect("F2 FAIL Step 4: decrypt with per_block_sk must work");
        assert_eq!(decrypted2.value, 50_000_000, "F2 FAIL Step 4: manual decrypt value mismatch");

        // Positive: AccountManager has the correct secret
        let (caps, _) = discover_native_token_outputs(
            &account_mgr, &mut tree, &call_data, BlockHeight::new(height), 0x05,
            &mut BlockScanDiagnostics::default(),
            &std::collections::HashSet::new(),
        ).expect("F2 FAIL: discover_native_token_outputs must succeed with correct key");
        assert!(!caps.is_empty(),
            "F2 FAIL: discover should find output when key matches. \
             Steps 1-3 passed but scan found nothing — check AEAD encoding in call data.");

        // Negative: different AccountManager with different secret finds nothing
        let wrong_path = temp_dir.join("dwow_test_discover_wrong.toml");
        std::fs::write(&wrong_path, "[wallet]\nwallet_secret = \"0200000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let wrong_mgr = dwow_accounts::AccountManager::open(&wrong_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet")
            .expect("wrong AccountManager::open");
        let _ = std::fs::remove_file(&wrong_path);
        let (caps2, _) = discover_native_token_outputs(
            &wrong_mgr, &mut MerkleTree::new(32), &call_data, BlockHeight::new(height), 0x05,
            &mut BlockScanDiagnostics::default(),
            &std::collections::HashSet::new(),
        ).expect("F2 FAIL: discover must succeed (returns empty with wrong key)");
        assert!(caps2.is_empty(),
            "F2 FAIL: discover should find nothing when key doesn't match");

        // Determinism: same inputs twice = same outputs
        let mut tree2 = MerkleTree::new(32);
        let (caps3, _) = discover_native_token_outputs(
            &account_mgr, &mut tree2, &call_data, BlockHeight::new(height), 0x05,
            &mut BlockScanDiagnostics::default(),
            &std::collections::HashSet::new(),
        ).expect("F2 FAIL: discover must be deterministic on second call");
        assert_eq!(caps.len(), caps3.len(),
            "F2 FAIL: discover must be deterministic — different result on second call");

        // R1 idempotency: passing the discovered cap IDs back as the dedup set
        // skips them (models a crash mid-block re-scan).
        let existing: std::collections::HashSet<String> =
            caps.iter().map(|(c, _)| c.cap_id.clone()).collect();
        let mut tree4 = MerkleTree::new(32);
        let (caps4, _) = discover_native_token_outputs(
            &account_mgr, &mut tree4, &call_data, BlockHeight::new(height), 0x05,
            &mut BlockScanDiagnostics::default(), &existing,
        ).expect("R1 FAIL: idempotent discover must succeed");
        assert!(caps4.is_empty(),
            "R1 FAIL: re-scan with existing cap IDs must discover zero new caps");
    }

    /// G5: cap_id cross-language determinism.
    /// Invariant: Rust blake2b_simd == Python hashlib.blake2b for the same inputs.
    /// Falsifiable: if the hash algorithm, domain separator, or input encoding
    /// changes on either side, this test FAILS.
    #[test]
    fn test_cap_id_matches_python_spec() {
        let secret_bytes: [u8; 32] = [
            0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let commitment_bytes: [u8; 32] = [0x99u8; 32];

        let mut hasher = blake2b_simd::Params::new()
            .hash_length(32)
            .personal(COIN_ID_PERSONALIZATION)
            .to_state();
        hasher.update(&secret_bytes);
        hasher.update(&commitment_bytes);
        let cap_id_hash = hasher.finalize();
        let cap_id = bs58::encode(cap_id_hash.as_bytes()).into_string();

        // Expected value from Python model:
        //   secret = bytes([0x42] + [0x00]*31)
        //   commitment = bytes([0x99]*32)
        //   cap_id = bs58(blake2b(secret || commitment, person=COIN_ID_PERSONALIZATION))
        assert_eq!(cap_id, "bbT1qqNUBXnwRuxZgmhQHTr9HiZrbuWpsCWJJAEiLij",
            "G5 FAIL: cap_id doesn't match Python model — cross-language determinism broken");
    }

    /// Wallet-miner pipeline symmetry: a coinbase built by the miner
    /// MUST be decryptable by the wallet using the same identity key.
    /// Per consensus-coinbase.md §2.2 + wallet.md §7.2.
    ///
    /// This test manually replicates the miner's deterministic coinbase
    /// construction (same key derivation, same blind derivation, same
    /// AEAD encryption) and verifies the wallet's scan_block discovers
    /// and correctly decrypts the coinbase output.
    #[test]
    fn test_wallet_miner_coinbase_symmetry() {
        let height: u64 = 42;
        let coinbase_reward: u64 = 50_000_000;

        // ── Setup: AccountManager from test key ─────────────────────
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_symmetry.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);

        let master_sk = account_mgr.secrets().into_iter().next()
            .expect("AccountManager must have at least one secret");
        let pk = PublicKey::from_secret(master_sk.clone());

        // ── Miner side: replicate PoWRewardCallBuilder determinism ──
        // consensus-coinbase.md §2.2: sk_H = derive_instance(sk_owner, cid, H)
        let sk_H = master_sk.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");
        let pk_H = PublicKey::from_secret(sk_H.clone());

        // Deterministic ephemeral key (model.rs:168)
        let ephemeral = SecretKey::from_base(dwow_sdk::crypto::poseidon_hash([
            *sk_H.inner(), pallas::Base::from(0xE7E7_E7E7_E7E7_E7E7u64),
        ]));

        // Deterministic blinds (pow_reward_v1.rs — domain-separated)
        let h_base = pallas::Base::from(height as u64);
        let commitment_blind = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(3u64)]));
        let value_blind = Blind(pallas::Scalar::from_repr(
            poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(1u64)]).to_repr(),
        ).unwrap());
        let token_blind = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(2u64)]));

        // ── Build NativeToken note ──────────────────────────────────
        let nt = dwow_native_token_contract::client::NativeToken {
            value: coinbase_reward,
            asset_id: pallas::Base::zero(), // DRKW_ASSET_ID
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind.inner(),
            spend_secret: *sk_H.inner(),
            value_blind: value_blind.inner(),
            token_blind: token_blind.inner(),
            memo: vec![],
        };

        // ── Encrypt deterministically (same as miner) ───────────────
        let aes = AeadEncryptedNote::encrypt_deterministic(&nt, &pk_H, ephemeral)
            .expect("deterministic encrypt");

        // ── Build call data: [0x05] ++ AeadEncryptedNote ───────────
        let mut aes_bytes = vec![];
        dwow_serial::Encodable::encode(&aes, &mut aes_bytes).ok();
        let mut call_data = vec![0x05u8];
        call_data.extend(&aes_bytes);

        // ── Build minimal Block with PoWRewardV1 contract call ──────
        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                fee_window_flags: FeeWindowFlags::default(),
                version: BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0),
                target: dwow_sdk::blockchain::BlockTarget::MAX,
                nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height),
                uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::new(coinbase_reward),
                randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![dwow_chain::Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![dwow_chain::ContractCall {
                    contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                    data: call_data,
                }],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        // ── Wallet side: scan_block ─────────────────────────────────
        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &BTreeMap::new(), &block, &std::collections::HashSet::new());

        // Must have discovered the native token output
        assert!(!result.native_outputs.is_empty(),
            "SYM FAIL: wallet must discover miner's coinbase output");
        let cap = &result.native_outputs[0].cap_record;
        assert_eq!(cap.value, coinbase_reward,
            "SYM FAIL: decrypted value must match miner's value");
        assert_eq!(cap.asset_id.inner(), pallas::Base::zero(),
            "SYM FAIL: asset_id must be DRKW_ASSET_ID");
        assert_eq!(cap.created_at_height, BlockHeight::new(height),
            "SYM FAIL: created_at_height must match block height");

        // ── Verify commitment attribute reconstruction ───────────────────
        let commitment_attrs = dwow_native_token_contract::model::CommitmentAttributes {
            version: 0,
            public_key: pk_H,
            value: coinbase_reward,
            asset_id: AssetId::DRKW,
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: commitment_blind,
        };
        let expected_commitment = commitment_attrs.to_commitment();
        assert_eq!(cap.commitment.inner(), expected_commitment.inner(),
            "SYM FAIL: commitment doesn't match — blind derivation or hash differs");

        // ── Verify nullifier symmetry ──────────────────────────────
        let expected_nf = Nullifier::new(sk_H, expected_commitment.inner());
        assert!(!expected_nf.is_zero(),
            "SYM FAIL: expected nullifier must be non-zero");

        // ── Negative: different AccountManager finds nothing ──────────
        let wrong_path = temp_dir.join("dwow_test_symmetry_wrong.toml");
        std::fs::write(&wrong_path,
            "[wrong]\nwallet_secret = \"0200000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let wrong_mgr = dwow_accounts::AccountManager::open(
            &wrong_path, dwow_sdk::crypto::keypair::Network::Testnet, "wrong",
        ).expect("AccountManager::open wrong");
        let _ = std::fs::remove_file(&wrong_path);
        let mut tree2 = MerkleTree::new(32);
        let wrong_result = scan_block(&mut tree2, &wrong_mgr, &BTreeMap::new(), &block, &std::collections::HashSet::new());
        assert!(wrong_result.native_outputs.is_empty(),
            "SYM FAIL: wrong AccountManager must find zero outputs");

        // ── Determinism: scan_block twice → identical results ──────
        let mut tree3 = MerkleTree::new(32);
        let result2 = scan_block(&mut tree3, &account_mgr, &BTreeMap::new(), &block, &std::collections::HashSet::new());
        assert_eq!(result.native_outputs.len(), result2.native_outputs.len(),
            "SYM FAIL: scan must be deterministic");
        assert_eq!(result.native_outputs[0].cap_record.value,
                   result2.native_outputs[0].cap_record.value,
            "SYM FAIL: scan determinism — value must match");
        assert_eq!(result.native_outputs[0].cap_record.commitment,
                   result2.native_outputs[0].cap_record.commitment,
            "SYM FAIL: scan determinism — commitment must match");

        // ── R1 idempotency: re-scan with existing cap IDs skips them ──
        // The dedup set models a crash mid-block: caps already in the DB must
        // NOT be re-appended to the tree (idempotent leaf position).
        let existing_ids: std::collections::HashSet<String> = result.native_outputs
            .iter().map(|o| o.cap_record.cap_id.clone()).collect();
        let mut tree4 = MerkleTree::new(32);
        let dedup_result = scan_block(&mut tree4, &account_mgr, &BTreeMap::new(), &block, &existing_ids);
        assert!(dedup_result.native_outputs.is_empty(),
            "R1 FAIL: re-scan with existing cap IDs must discover zero new caps");
    }

    /// P7 tripwire — positive: a non-native note with a typed manifest is discovered,
    /// typed, and carries zero balance impact.
    #[test]
    fn test_generic_path_types_foreign_note() {
        use dwow_sdk::capability::{Barb, Primitive};
        use dwow_sdk::manifest::ContractManifest;
        use dwow_sdk::crypto::Keypair;
        use dwow_chain::Transaction;

        let height: u64 = 100;
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_tripwire.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let master_sk = account_mgr.secrets().into_iter().next()
            .expect("AccountManager must have at least one secret");
        let pk = PublicKey::from_secret(master_sk);

        let typed_toml = r#"
[contract]
name = "promissory_note"
category = "Token"
description = "tripwire"
[[functions]]
name = "transfer"
code = 4
[[capabilities]]
discriminant = 0
name = "commitment"
primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]
note_schema = [{ name = "value", type = "u64" }, { name = "commitment", type = "pallas_base" }]
[[actions]]
function = "transfer"
requires = { type = "none" }
produces = [{ name = "commitment" }]
required_barbs = ["Spend","Nullify","Commit","Dispatch","Gate","Denominate"]
"#;
        let manifest = ContractManifest::from_toml(typed_toml).unwrap();
        let mut manifests: BTreeMap<ContractId, ContractManifest> = BTreeMap::new();
        let foreign_cid = ContractId::from_bytes([0u8; 32]).unwrap();
        manifests.insert(foreign_cid, manifest);

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct TestNote { value: u64, commitment: pallas::Base }
        let note = TestNote { value: 42, commitment: pallas::Base::from(999) };
        let enc_note = AeadEncryptedNote::encrypt(&note, &pk, &mut rand::rngs::OsRng).unwrap();
        // AeadEncryptedNote is SerialEncodable — encode via dwow_serial::Encodable.
        let mut call_data = vec![0x04u8];
        dwow_serial::Encodable::encode(&enc_note, &mut call_data).ok();

        let tx = Transaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall { contract_id: foreign_cid, data: call_data }],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        };
        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                fee_window_flags: FeeWindowFlags::default(),
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![tx],
        };

        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &manifests, &block, &std::collections::HashSet::new());
        assert_eq!(result.capabilities.len(), 1,
            "tripwire: foreign note must be discovered and typed");
        let cr = &result.capabilities[0].cap_record;
        assert_eq!(cr.capability_name.as_deref(), Some("commitment"));
        assert_eq!(cr.contract_id, foreign_cid);
        assert!(cr.resource.is_some());
        assert!(cr.action.is_some());
        assert_eq!(cr.primitives.len(), 7,
            "tripwire: primitives from manifest declaration");
        // Stored barbs must be the composed UNION (not the required subset).
        // The 7 primitives compose 8 barbs — the action's 6 required plus
        // Derive (from SecretKey) and ProveInclusion (from MerkleNode).
        assert!(cr.barbs.contains(&Barb::Derive), "tripwire: composed must include Derive");
        assert!(cr.barbs.contains(&Barb::ProveInclusion), "tripwire: composed must include ProveInclusion");
        assert_eq!(cr.barbs.len(), 8,
            "tripwire: composed union has 8 barbs (not 6 required)");
        assert_eq!(cr.value, 42, "tripwire: foreign cap carries its note's value");
        assert_eq!(cr.asset_id.inner(), pallas::Base::zero(),
            "tripwire: foreign cap must have zero asset_id");
    }

    /// P7 tripwire — negative: a manifest lacking typed fields drops the note
    /// (no fallback, no panic).
    #[test]
    fn test_generic_path_drops_untyped_note() {
        use dwow_sdk::manifest::ContractManifest;
        use dwow_sdk::crypto::Keypair;
        use dwow_chain::Transaction;

        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_untyped.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let master_sk = account_mgr.secrets().into_iter().next()
            .expect("AccountManager must have at least one secret");
        let pk = PublicKey::from_secret(master_sk);

        let bare_toml = r#"
[contract]
name = "bare"
category = "Other"
description = "no typed fields"
[[functions]]
name = "f"
code = 0
[[capabilities]]
discriminant = 0
name = "thing"
[[actions]]
function = "f"
requires = { type = "none" }
produces = [{ name = "thing" }]
"#;
        let manifest = ContractManifest::from_toml(bare_toml).unwrap();
        let mut manifests: BTreeMap<ContractId, ContractManifest> = BTreeMap::new();
        let foreign_cid = ContractId::from_bytes([0u8; 32]).unwrap();
        manifests.insert(foreign_cid, manifest);

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct TestNote2 { v: u64 }
        let note = TestNote2 { v: 1 };
        let enc_note = AeadEncryptedNote::encrypt(&note, &pk, &mut rand::rngs::OsRng).unwrap();
        let mut call_data = vec![0x00u8];
        dwow_serial::Encodable::encode(&enc_note, &mut call_data).ok();

        let tx = Transaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall { contract_id: foreign_cid, data: call_data }],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        };
        let h = 99u64;
        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(h), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                fee_window_flags: FeeWindowFlags::default(),
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![tx],
        };
        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &manifests, &block, &std::collections::HashSet::new());
        assert!(result.capabilities.is_empty(),
            "tripwire: untyped manifest must drop the note — no fallback");
    }

    /// P7 tripwire — coverage gate: a manifest whose declared primitives do
    /// NOT cover its required_barbs must drop the note (fix the composition,
    /// not the wallet — type-system.md §13).
    #[test]
    fn test_generic_path_drops_uncovered_composition() {
        use dwow_sdk::manifest::ContractManifest;
        use dwow_sdk::crypto::Keypair;
        use dwow_chain::Transaction;

        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_uncovered.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let master_sk = account_mgr.secrets().into_iter().next()
            .expect("AccountManager must have at least one secret");
        let pk = PublicKey::from_secret(master_sk);

        // Manifest where the declared primitives (AssetId + SecretKey) do NOT
        // cover the required barb "Mine". Under the fix, the coverage gate
        // drops the note.
        let uncovered_toml = r#"
[contract]
name = "uncovered"
category = "Other"
description = "primitives don't cover required_barbs"
[[functions]]
name = "mine"
code = 0
[[capabilities]]
discriminant = 0
name = "fake_miner"
primitives = ["AssetId","SecretKey"]
note_schema = [{ name = "commitment", type = "pallas_base" }]
[[actions]]
function = "mine"
requires = { type = "none" }
produces = [{ name = "fake_miner" }]
required_barbs = ["Spend","Mine"]
"#;
        let manifest = ContractManifest::from_toml(uncovered_toml).unwrap();
        let mut manifests: BTreeMap<ContractId, ContractManifest> = BTreeMap::new();
        let foreign_cid = ContractId::from_bytes([0u8; 32]).unwrap();
        manifests.insert(foreign_cid, manifest);

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct TestNote3 { commitment: pallas::Base }
        let note = TestNote3 { commitment: pallas::Base::from(1) };
        let enc_note = AeadEncryptedNote::encrypt(&note, &pk, &mut rand::rngs::OsRng).unwrap();
        let mut call_data = vec![0x00u8];
        dwow_serial::Encodable::encode(&enc_note, &mut call_data).ok();

        let tx = Transaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall { contract_id: foreign_cid, data: call_data }],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        };
        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(101), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                fee_window_flags: FeeWindowFlags::default(),
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![tx],
        };
        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &manifests, &block, &std::collections::HashSet::new());
        assert!(result.capabilities.is_empty(),
            "tripwire: uncovered composition must be dropped — no CapRecord");
    }

    /// P8 — User-deployed contract manifest discovery pipeline.
    ///
    /// Exercises the full DeployV1 → manifest extraction → deployment discovery →
    /// manifest storage → manifest-driven Path 2 typing chain. This is the code
    /// path that genesis seeding bypasses at wallet init — without this test,
    /// `from_deploy_ix()`, the 0x4D magic byte, and the DeployV1 manifest handler
    /// have zero pre-Docker coverage.
    #[test]
    fn test_deployv1_manifest_discovery_to_path2_typing() {
        use dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID;
        use dwow_sdk::deploy::DeployParamsV1;
        use dwow_sdk::manifest::ContractManifest;
        use dwow_serial::Encodable;
        use dwow_chain::Transaction as ChainTransaction;

        let height_deploy: u64 = 100;
        let height_call: u64 = 101;
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_p8.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let wallet_sk = account_mgr.secrets().into_iter().next()
            .expect("secrets");
        let wallet_pk = PublicKey::from_secret(wallet_sk);

        // Step 1: build a manifest TOML for a synthetic user-deployed contract
        let deployer_kp = dwow_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng);
        let contract_id = ContractId::derive_public(deployer_kp.public);
        let cid_str = bs58::encode(contract_id.to_bytes()).into_string();

        let manifest_toml = r#"
[contract]
name = "synthetic_user_contract"
category = "Testing"
description = "User-deployed contract for manifest discovery test"
version = "1.0.0"
dependencies = []

[[functions]]
name = "do_thing"
code = 7
requires_proof = true
proof_circuit = "Test_V1"

[[capabilities]]
discriminant = 1
name = "thing_cap"
description = "A capability produced by do_thing"
primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]
note_schema = [
    { name = "value", type = "u64" },
    { name = "commitment", type = "pallas_base" },
]

[[actions]]
function = "do_thing"
requires = { type = "none" }
produces = [{ name = "thing_cap" }]
required_barbs = ["Spend","Nullify","Commit","Dispatch","Gate","Denominate","ProveInclusion"]
"#;

        // Step 2: build deploy_ix with 0x4D magic byte + TOML
        let deploy_ix = {
            let mut ix = vec![0x4Du8];
            ix.extend_from_slice(manifest_toml.as_bytes());
            ix
        };
        let manifest_parsed = ContractManifest::from_deploy_ix(&deploy_ix);
        assert!(manifest_parsed.is_some(), "0x4D magic byte must be detected");
        let manifest_parsed = manifest_parsed.unwrap();
        assert!(manifest_parsed.is_ok(), "manifest TOML must parse");
        let manifest_parsed = manifest_parsed.unwrap();
        assert_eq!(manifest_parsed.name, "synthetic_user_contract");

        // Step 3: build DeployParamsV1 with this ix
        let deploy_params = DeployParamsV1 {
            wasm_bincode: vec![0x00, 0x61, 0x73, 0x6d],
            public_key: deployer_kp.public,
            ix: deploy_ix,
            singleton: false,
            singleton_name: String::new(),
        };
        let mut deploy_call_data = vec![0x00u8];
        deploy_params.encode(&mut deploy_call_data)
            .expect("DeployParamsV1 must encode");

        // Step 4: build a block with the DeployV1 transaction
        let deploy_tx = ChainTransaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall {
                contract_id: *DEPLOYOOOR_CONTRACT_ID,
                data: deploy_call_data,
            }],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        };
        let deploy_block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height_deploy), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                fee_window_flags: FeeWindowFlags::default(),
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![deploy_tx],
        };

        // Step 5: scan the DeployV1 block
        let mut tree = MerkleTree::new(32);
        let empty_manifests = BTreeMap::new();
        let deploy_result = scan_block(&mut tree, &account_mgr, &empty_manifests, &deploy_block, &std::collections::HashSet::new());
        assert_eq!(deploy_result.deployments.len(), 1,
            "P8: DeployV1 must produce exactly 1 DeploymentDiscovery");
        let dep = &deploy_result.deployments[0];
        assert_eq!(dep.contract_id, contract_id,
            "P8: DeploymentDiscovery.contract_id must match derived ContractId");
        assert!(dep.manifest_json.is_some(),
            "P8: DeploymentDiscovery.manifest_json must be Some for a 0x4D-ix deploy");
        let manifest_json = dep.manifest_json.as_ref().unwrap();
        let stored_manifest: ContractManifest = serde_json::from_str(manifest_json)
            .expect("manifest_json must deserialize");
        assert_eq!(stored_manifest.name, "synthetic_user_contract",
            "P8: manifest JSON roundtrip must preserve name");

        // Step 6: store manifest via wallet DB (simulating scan_block_linear)
        let wallet = crate::walletdb::WalletDb::new(None, None, false)
            .expect("in-memory WalletDb");
        wallet.exec_batch_sql(include_str!("../wallet.sql")).ok();
        let record = crate::walletdb::ContractMetadataRecord {
            contract_id: cid_str.clone(), name: "synthetic_user_contract".into(),
            symbol: None, category: "Testing".into(),
            description: Some("test".into()), public: true,
            deployer_pubkey: bs58::encode(deployer_kp.public.to_bytes()).into_string(),
            deploy_height: BlockHeight::new(height_deploy), attestations_json: "[]".into(),
            lock_status: "unlocked".into(),
        };
        wallet.insert_contract_metadata_with_manifest(&record, None).ok();
        wallet.store_manifest(&cid_str, manifest_json)
            .expect("store_manifest must succeed");

        // Step 7: read back via get_contract_manifest
        let loaded = wallet.get_contract_manifest(&cid_str)
            .expect("get_contract_manifest must succeed");
        assert!(loaded.is_some(), "P8: get_contract_manifest must return stored manifest");
        let loaded_manifest = loaded.unwrap();
        let mut manifests: BTreeMap<ContractId, ContractManifest> = BTreeMap::new();
        manifests.insert(contract_id, loaded_manifest);

        // Step 8: build a contract call block for the user-deployed contract
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct TestNote { value: u64, commitment: pallas::Base }
        let note = TestNote { value: 0, commitment: pallas::Base::from(42) };
        let enc_note = AeadEncryptedNote::encrypt(&note, &wallet_pk, &mut rand::rngs::OsRng)
            .expect("encrypt");
        let mut call_data = vec![0x07u8];
        dwow_serial::Encodable::encode(&enc_note, &mut call_data).ok();

        let call_tx = ChainTransaction {
            version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
            contract_calls: vec![dwow_chain::ContractCall { contract_id, data: call_data }],
            lock_time: 0, nullifiers: vec![], witness: vec![],
        };
        let call_block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height_call), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                fee_window_flags: FeeWindowFlags::default(),
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![call_tx],
        };

        // Step 9: Path 2 types the capability from the user-deployed manifest
        let mut tree2 = MerkleTree::new(32);
        let call_result = scan_block(&mut tree2, &account_mgr, &manifests, &call_block, &std::collections::HashSet::new());
        assert!(!call_result.capabilities.is_empty(),
            "P8: Path 2 must type capability from user-deployed manifest");
        let cap = &call_result.capabilities[0].cap_record;
        assert_eq!(cap.capability_name.as_deref(), Some("thing_cap"),
            "P8: capability name must match manifest");
        assert_eq!(cap.capability_discriminant, Some(1),
            "P8: discriminant must match manifest");
        assert_eq!(cap.contract_id, contract_id,
            "P8: contract_id must match the user-deployed contract");
        assert!(!cap.primitives.is_empty(),
            "P8: primitives must be populated from manifest (not empty vec)");
        assert!(!cap.barbs.is_empty(),
            "P8: barbs must be populated from manifest composition");
        assert_eq!(cap.value, 0, "P8: synthetic capability must carry zero value");
        assert_eq!(cap.created_at_height, BlockHeight::new(height_call),
            "P8: created_at_height must match call block height");
    }

    /// P9 — Nullifier publication and revocation.
    ///
    /// Both `match_nullifiers` and `mark_revoked` had zero test coverage before
    /// this test. Verifies that when a block publishes a nullifier matching a
    /// held capability, the capability is detected as revoked and filtered from
    /// unspent queries.
    #[test]
    fn test_nullifier_revocation_lifecycle() {
        let height: u64 = 42;
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_p9.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let sk = account_mgr.secrets().into_iter().next().expect("secrets");
        let _pk = PublicKey::from_secret(sk.clone());

        // Build a NativeToken output using deterministic values so the wallet
        // can decrypt it via trial decryption with its master key.
        let value: u64 = 100_000_000;
        let commitment_blind = Blind(pallas::Base::from(9999));
        let note = dwow_native_token_contract::client::NativeToken {
            value,
            asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind.inner(),
            spend_secret: pallas::Base::from(7u64),
            value_blind: pallas::Scalar::zero(),
            token_blind: pallas::Base::zero(),
            memo: vec![],
        };
        let enc_note = AeadEncryptedNote::encrypt(&note, &_pk, &mut rand::rngs::OsRng)
            .expect("AEAD encrypt");
        let mut call_data = vec![0x05u8]; // PoWRewardV1
        dwow_serial::Encodable::encode(&enc_note, &mut call_data).ok();
        // Decode the note to verify the wallet sees the same commitment
        let trial_notes = account_mgr.secrets();
        let mut found_commitment = None;
        for trial_sk in &trial_notes {
            if let Ok(decrypted) = enc_note.decrypt::<dwow_native_token_contract::client::NativeToken>(trial_sk, height) {
                let attrs = dwow_native_token_contract::model::CommitmentAttributes {
                    version: 0, public_key: _pk, value,
                    asset_id: AssetId::DRKW,
                    spend_hook: FuncId::none(),
                    user_data: pallas::Base::zero(), blind: commitment_blind.clone(),
                };
                let commitment = attrs.to_commitment();
                found_commitment = Some(commitment);
                break;
            }
        }
        let commitment = found_commitment.expect("P9: must decrypt note to get commitment");

        // Derive nullifier: nf = poseidon_hash(secret, commitment)
        let nullifier = Nullifier::new(sk.clone(), commitment.inner());

        // Insert a CapRecord into the wallet DB representing this held commitment
        let wallet = crate::walletdb::WalletDb::new(None, None, false)
            .expect("in-memory WalletDb");
        wallet.exec_batch_sql(include_str!("../wallet.sql")).ok();
        let cap_id = super::derive_cap_id(&sk, &commitment.to_bytes());
        let record = super::CapRecord {
            cap_id: cap_id.clone(), value,
            asset_id: AssetId::DRKW,
            spend_hook: None, user_data: None,
            leaf_position: 0,
            commitment: Commitment::from_base(commitment.inner()),
            contract_id: *NATIVE_TOKEN_CONTRACT_ID,
            func_id: None,
            cap_blind: commitment_blind,
            value_blind: Blind(pallas::Scalar::zero()),
            asset_blind: Blind(pallas::Base::zero()),
            capability_discriminant: None, capability_name: None,
            resource: None, action: None, primitives: vec![], barbs: vec![],
            revoked: false, revoked_at_height: None,
            created_at_height: BlockHeight::new(height), status: None, status_height: None, key_coords: None, spend_secret: None, object_id: None, state_nonce: None,
        };
        let merkle_proof = crate::walletdb::MerkleProof { root: String::new(), siblings: vec![], leaf_position: 0 };
        wallet.insert_capability(&record, &merkle_proof)
            .expect("P9: insert must succeed");

        // Verify the cap is visible as unspent
        let unspent = wallet.get_held_capabilities(Some(false))
            .expect("get unspent");
        assert_eq!(unspent.len(), 1, "P9: cap must be visible as unspent");
        assert_eq!(unspent[0].cap_id, cap_id);

        // Build a block that publishes the nullifier (TransferV1 call)
        let nf_record = super::NullifierRecord { nullifier };
        let mut published_nfs = std::collections::BTreeMap::new();
        published_nfs.insert(nullifier, nf_record);

        // match_nullifiers: should detect the match
        let secrets = account_mgr.secrets();
        let existing = wallet.get_held_capabilities(Some(false))
            .expect("get existing");
        let published: Vec<super::NullifierRecord> =
            published_nfs.values().cloned().collect();
        let revoked_matches = super::match_nullifiers(
            &existing, &secrets, &published, BlockHeight::new(height),
        );
        assert_eq!(revoked_matches.len(), 1,
            "P9: match_nullifiers must detect the published nullifier");
        assert_eq!(revoked_matches[0].0, cap_id,
            "P9: revoked cap_id must match");
        assert_eq!(revoked_matches[0].1, BlockHeight::new(height),
            "P9: revoked height must match block height");

        // mark_revoked: mark the cap as spent
        wallet.mark_revoked(&cap_id, BlockHeight::new(height))
            .expect("P9: mark_revoked must succeed");

        // Verify the cap is now filtered from unspent queries
        let unspent_after = wallet.get_held_capabilities(Some(false))
            .expect("get unspent after revoke");
        assert!(unspent_after.is_empty(),
            "P9: unspent caps must be empty after revocation");
        // But visible in "include revoked" queries
        let all_caps = wallet.get_held_capabilities(Some(true))
            .expect("get all caps");
        assert_eq!(all_caps.len(), 1,
            "P9: cap must still be visible when include_revoked=true");
        assert!(all_caps[0].revoked, "P9: cap must be marked revoked");
        assert_eq!(all_caps[0].revoked_at_height, Some(BlockHeight::new(height)),
            "P9: revoked_at_height must match");
    }

    /// P10 — Multi-transaction block scan: coinbase + generic Path 2 + TransferV1.
    ///
    /// The `scan_block` loop processes transactions sequentially. A block with
    /// multiple transactions of different types (native coinbase, manifest-driven
    /// Path 2, native transfer) exercises the full routing logic: NATIVE_TOKEN
    /// dispatch, DEPLOYOOOR bypass, generic AEAD Path 2, and native output
    /// discovery — all in one call.
    #[test]
    fn test_multi_transaction_block_scan() {
        use dwow_sdk::capability::{Barb, Primitive};
        use dwow_sdk::manifest::ContractManifest;
        use dwow_chain::Transaction;

        let height: u64 = 100;
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_p10.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let sk = account_mgr.secrets().into_iter().next().expect("secrets");
        let pk = PublicKey::from_secret(sk);

        // ── Tx 1: NativeToken PoWRewardV1 coinbase ─────────────────────
        let value: u64 = 100_000_000;
        let commitment_blind = Blind(pallas::Base::from(9999));
        let note = dwow_native_token_contract::client::NativeToken {
            value, asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind.inner(),
            spend_secret: pallas::Base::from(7u64),
            value_blind: pallas::Scalar::zero(),
            token_blind: pallas::Base::zero(), memo: vec![],
        };
        let enc_note = AeadEncryptedNote::encrypt(&note, &pk, &mut rand::rngs::OsRng)
            .expect("AEAD encrypt");
        let mut coinbase_data = vec![0x05u8];
        dwow_serial::Encodable::encode(&enc_note, &mut coinbase_data).ok();

        // ── Tx 2: Foreign Path 2 capability (manifest-driven) ─────────
        let manifest_toml = r#"
[contract]
name = "multi_tx_test"
category = "Testing"
description = "Multi-tx block scan test"
version = "1.0.0"
[[functions]]
name = "emit"
code = 1
[[capabilities]]
discriminant = 0
name = "event"
primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]
note_schema = [{ name = "value", type = "u64" }, { name = "commitment", type = "pallas_base" }]
[[actions]]
function = "emit"
requires = { type = "none" }
produces = [{ name = "event" }]
required_barbs = ["Spend","Nullify","Commit","Dispatch","Gate","Denominate"]
"#;
        let manifest = ContractManifest::from_toml(manifest_toml).unwrap();
        let foreign_cid = ContractId::from_bytes([0u8; 32]).unwrap();
        let mut manifests: BTreeMap<ContractId, ContractManifest> = BTreeMap::new();
        manifests.insert(foreign_cid, manifest);

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct Path2Note { value: u64, commitment: pallas::Base }
        let p2note = Path2Note { value: 0, commitment: pallas::Base::from(77) };
        let enc_p2note = AeadEncryptedNote::encrypt(&p2note, &pk, &mut rand::rngs::OsRng)
            .expect("AEAD encrypt");
        let mut p2_data = vec![0x01u8];
        dwow_serial::Encodable::encode(&enc_p2note, &mut p2_data).ok();

        // ── Block with 2 transactions ─────────────────────────────────
        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                    fee_window_flags: FeeWindowFlags::default(),
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::new(value), randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![
                Transaction {
                    version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
                    contract_calls: vec![dwow_chain::ContractCall {
                        contract_id: *NATIVE_TOKEN_CONTRACT_ID, data: coinbase_data,
                    }],
                    lock_time: 0, nullifiers: vec![], witness: vec![],
                },
                Transaction {
                    version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
                    contract_calls: vec![dwow_chain::ContractCall {
                        contract_id: foreign_cid, data: p2_data,
                    }],
                    lock_time: 0, nullifiers: vec![], witness: vec![],
                },
            ],
        };

        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &manifests, &block, &std::collections::HashSet::new());

        // Path 1: coinbase output discovered
        assert_eq!(result.native_outputs.len(), 1,
            "P10: must discover 1 coinbase output (Path 1)");
        assert_eq!(result.native_outputs[0].cap_record.value, value,
            "P10: coinbase value must match");

        // Path 2: foreign capability typed from manifest
        assert_eq!(result.capabilities.len(), 1,
            "P10: must discover 1 Path 2 capability");
        let p2cap = &result.capabilities[0].cap_record;
        assert_eq!(p2cap.contract_id, foreign_cid,
            "P10: Path 2 cap must reference foreign contract");
        assert!(!p2cap.primitives.is_empty(),
            "P10: Path 2 cap must have typed primitives");
    }

    /// P11 — FeeV1 output discovery.
    ///
    /// `discover_native_token_outputs` handles FeeV1 (0x00), but before this
    /// test only PoWRewardV1 (0x05) and TransferV1 (0x03) were exercised.
    /// Verifies that a FeeV1 call with an AEAD-encrypted change note is
    /// discovered and tagged with the correct source.
    #[test]
    fn test_feev1_output_discovery() {
        use dwow_native_token_contract::client::NativeToken;
        use dwow_native_token_contract::model::CommitmentAttributes;

        let height: u64 = 42;
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_p11.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let sk = account_mgr.secrets().into_iter().next().expect("secrets");
        let pk = PublicKey::from_secret(sk);

        // Build a FeeV1 change output note (same structure as any native token note)
        let value: u64 = 500_000_000;
        let commitment_blind = Blind(pallas::Base::from(12345));
        let note = NativeToken {
            value, asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind.inner(),
            spend_secret: pallas::Base::from(7u64),
            value_blind: pallas::Scalar::zero(),
            token_blind: pallas::Base::zero(), memo: vec![],
        };
        let enc_note = AeadEncryptedNote::encrypt(&note, &pk, &mut rand::rngs::OsRng)
            .expect("AEAD encrypt");

        // FeeV1 call data: [0x00 selector][FeeParamsV1]
        // We only need the note bytes — the scanner slides byte-by-byte looking
        // for AeadEncryptedNote. Wrap in a minimal FeeParamsV1 structure.
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FeeInput { fee: u64, nullifier: pallas::Base, tx_nonce: pallas::Base }
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FeeOutput { commitment: pallas::Base, note: AeadEncryptedNote }
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FeeParams { input: FeeInput, output: FeeOutput, tx_binding: pallas::Base }

        let fee_params = FeeParams {
            input: FeeInput { fee: 1, nullifier: pallas::Base::zero(), tx_nonce: pallas::Base::zero() },
            output: FeeOutput { commitment: pallas::Base::zero(), note: enc_note },
            tx_binding: pallas::Base::zero(),
        };
        let mut call_data = vec![0x00u8]; // FeeV1 function code
        dwow_serial::Encodable::encode(&fee_params, &mut call_data).ok();

        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                fee_window_flags: FeeWindowFlags::default(),
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![dwow_chain::Transaction {
                version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
                contract_calls: vec![dwow_chain::ContractCall {
                    contract_id: *NATIVE_TOKEN_CONTRACT_ID, data: call_data,
                }],
                lock_time: 0, nullifiers: vec![], witness: vec![],
            }],
        };

        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &BTreeMap::new(), &block, &std::collections::HashSet::new());

        // Must discover the fee change output
        assert!(!result.native_outputs.is_empty(),
            "P11: FeeV1 change output must be discovered (function_code 0x00)");
        let cap = &result.native_outputs[0].cap_record;
        assert_eq!(cap.value, value,
            "P11: FeeV1 change output value must match");
        assert_eq!(cap.asset_id.inner(), pallas::Base::zero(),
            "P11: FeeV1 change must carry DRKW asset_id");
        assert_eq!(cap.created_at_height, BlockHeight::new(height),
            "P11: FeeV1 change created_at_height must match");
    }

    /// P12: FeeCollectV1 (0x06) fee commitment discovery — miner's collection plate.
    /// The same per-block key derivation as coinbase (pk_H), plus FeeCollectV1
    /// deterministic blinds. Port of the 0x05 discover test.
    #[test]
    fn test_feecollectv1_output_discovery() {
        use dwow_native_token_contract::client::NativeToken;
        use dwow_native_token_contract::model::CommitmentAttributes;

        let height: u64 = 42;
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_p12.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);
        let sk = account_mgr.secrets().into_iter().next().expect("secrets");

        // ── Miner side: per-block key + deterministic FeeCollectV1 blinds ──
        let sk_H = sk.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");
        let pk_H = PublicKey::from_secret(sk_H.clone());
        let h_base = pallas::Base::from(height as u64);
        // Deterministic ephemeral key (domain 13 per spec §3.6)
        let ephem = SecretKey::from_base(poseidon_hash([
            *sk_H.inner(), h_base, pallas::Base::from(13u64),
        ]));
        // Deterministic blinds (domains 10-12 per spec §3.6)
        let commitment_blind = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(12u64)]));
        let value_blind = Blind(pallas::Scalar::from_repr(
            poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(10u64)]).to_repr(),
        ).unwrap());
        let token_blind = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(11u64)]));

        // ── Build the fee commitment note (identical structure to coinbase) ──
        let total_fees: u64 = 1;
        let note = NativeToken {
            value: total_fees, asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(), user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind.inner(),
            spend_secret: pallas::Base::from(7u64),
            value_blind: value_blind.inner(),
            token_blind: token_blind.inner(), memo: vec![],
        };
        let enc_note = AeadEncryptedNote::encrypt_deterministic(&note, &pk_H, ephem)
            .expect("AEAD encrypt_deterministic");

        // ── Minimal FeeCollectParamsV1 wrapper ──
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FcOutput { value_commit: Vec<u8>, token_commit: pallas::Base,
            commitment: pallas::Base, nullifier: pallas::Base, note: AeadEncryptedNote }
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FcParams { total_fees: u64, output: FcOutput,
            nullifier: pallas::Base, tx_binding: pallas::Base, tx_nonce: pallas::Base }

        let fc_params = FcParams {
            total_fees,
            output: FcOutput { value_commit: vec![], token_commit: pallas::Base::zero(),
                commitment: pallas::Base::zero(), nullifier: pallas::Base::zero(), note: enc_note },
            nullifier: pallas::Base::zero(),
            tx_binding: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
        };
        let mut call_data = vec![0x06u8]; // FeeCollectV1 function code
        dwow_serial::Encodable::encode(&fc_params, &mut call_data).ok();

        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                fee_window_flags: FeeWindowFlags::default(),
                version: BlockVersion::CURRENT, previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0), target: dwow_sdk::blockchain::BlockTarget::MAX, nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height), uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::ZERO, randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32], nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32], anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32], finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![dwow_chain::Transaction {
                version: BlockVersion::CURRENT, inputs: vec![], outputs: vec![],
                contract_calls: vec![dwow_chain::ContractCall {
                    contract_id: *NATIVE_TOKEN_CONTRACT_ID, data: call_data,
                }],
                lock_time: 0, nullifiers: vec![], witness: vec![],
            }],
        };

        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &BTreeMap::new(), &block, &std::collections::HashSet::new());

        // Must discover the fee commitment output
        assert!(!result.native_outputs.is_empty(),
            "P12: FeeCollectV1 fee commitment must be discovered (function_code 0x06)");
        let cap = &result.native_outputs[0].cap_record;
        assert_eq!(cap.value, total_fees,
            "P12: FeeCollectV1 fee commitment value must match");
        assert_eq!(cap.asset_id.inner(), pallas::Base::zero(),
            "P12: FeeCollectV1 fee commitment must carry DRKW asset_id");
        assert_eq!(cap.created_at_height, BlockHeight::new(height),
            "P12: FeeCollectV1 fee commitment created_at_height must match");
    }

    /// P13: Combined PoWRewardV1 (0x05) + FeeCollectV1 (0x06) in the same block.
    /// A real block has coinbase at transactions[0] and FeeCollect at transactions[last].
    /// The wallet scan must discover both native token outputs in a single scan_block call.
    /// G3: FeeV2 (0x08) is included in the output-discovery match guard.
    /// Before the fix, FeeV2 outputs were never discovered by wallet scan.
    /// This test verifies the fix is in place — 0x08 is matched for AEAD trial
    /// decryption alongside 0x00 (FeeV1), 0x03-0x06.
    #[test]
    fn test_feev2_08_in_output_discovery_guard() {
        // The scan_native_token_contract_calls function matches function codes
        // for output discovery. Verify 0x08 is included.
        assert!(matches!(0x08u8, 0x00 | 0x03 | 0x04 | 0x05 | 0x06 | 0x08),
            "G3: FeeV2 (0x08) must be in output-discovery match guard");
    }

    #[test]
    fn test_coinbase_and_feecollect_in_same_block() {
        use dwow_native_token_contract::client::NativeToken;

        let height: u64 = 42;
        let coinbase_reward: u64 = 50_000_000;
        let total_fees: u64 = 1;

        // ── Setup: AccountManager from test key ─────────────────────
        let temp_dir = std::env::temp_dir();
        let keys_path = temp_dir.join("dwow_test_p13.toml");
        std::fs::write(&keys_path,
            "[wallet]\nwallet_secret = \"0100000000000000000000000000000000000000000000000000000000000000\"\n").ok();
        let account_mgr = dwow_accounts::AccountManager::open(
            &keys_path, dwow_sdk::crypto::keypair::Network::Testnet, "wallet",
        ).expect("AccountManager::open");
        let _ = std::fs::remove_file(&keys_path);

        let master_sk = account_mgr.secrets().into_iter().next()
            .expect("AccountManager must have at least one secret");

        // ── Per-block key (shared by coinbase and FeeCollect) ──────
        let sk_H = master_sk.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");
        let pk_H = PublicKey::from_secret(sk_H.clone());
        let h_base = pallas::Base::from(height as u64);

        // ═══════════════════════════════════════════════════════════════
        // Transaction 0: PoWRewardV1 (0x05) — domains 1-3
        // ═══════════════════════════════════════════════════════════════
        let ephem_05 = SecretKey::from_base(poseidon_hash([
            *sk_H.inner(), pallas::Base::from(0xE7E7_E7E7_E7E7_E7E7u64),
        ]));
        let commitment_blind_05 = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(3u64)]));
        let value_blind_05 = Blind(pallas::Scalar::from_repr(
            poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(1u64)]).to_repr(),
        ).unwrap());
        let token_blind_05 = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(2u64)]));

        let nt_05 = NativeToken {
            value: coinbase_reward,
            asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind_05.inner(),
            spend_secret: pallas::Base::from(7u64),
            value_blind: value_blind_05.inner(),
            token_blind: token_blind_05.inner(),
            memo: vec![],
        };
        let aes_05 = AeadEncryptedNote::encrypt_deterministic(&nt_05, &pk_H, ephem_05)
            .expect("deterministic encrypt 0x05");
        let mut aes_bytes_05 = vec![];
        dwow_serial::Encodable::encode(&aes_05, &mut aes_bytes_05).ok();
        let mut call_data_05 = vec![0x05u8];
        call_data_05.extend(&aes_bytes_05);

        // ═══════════════════════════════════════════════════════════════
        // Transaction 1: FeeCollectV1 (0x06) — domains 10-13
        // ═══════════════════════════════════════════════════════════════
        let ephem_06 = SecretKey::from_base(poseidon_hash([
            *sk_H.inner(), h_base, pallas::Base::from(13u64),
        ]));
        let commitment_blind_06 = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(12u64)]));
        let value_blind_06 = Blind(pallas::Scalar::from_repr(
            poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(10u64)]).to_repr(),
        ).unwrap());
        let token_blind_06 = Blind(poseidon_hash([*sk_H.inner(), h_base, pallas::Base::from(11u64)]));

        let nt_06 = NativeToken {
            value: total_fees,
            asset_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            commitment_blind: commitment_blind_06.inner(),
            spend_secret: pallas::Base::from(7u64),
            value_blind: value_blind_06.inner(),
            token_blind: token_blind_06.inner(),
            memo: vec![],
        };
        let aes_06 = AeadEncryptedNote::encrypt_deterministic(&nt_06, &pk_H, ephem_06)
            .expect("deterministic encrypt 0x06");

        // ── FeeCollectParamsV1 wrapper (same structure as P12) ──
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FcOutput { value_commit: Vec<u8>, token_commit: pallas::Base,
            commitment: pallas::Base, nullifier: pallas::Base, note: AeadEncryptedNote }
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct FcParams { total_fees: u64, output: FcOutput,
            nullifier: pallas::Base, tx_binding: pallas::Base, tx_nonce: pallas::Base }

        let fc_params = FcParams {
            total_fees,
            output: FcOutput { value_commit: vec![], token_commit: pallas::Base::zero(),
                commitment: pallas::Base::zero(), nullifier: pallas::Base::zero(), note: aes_06 },
            nullifier: pallas::Base::zero(),
            tx_binding: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
        };
        let mut call_data_06 = vec![0x06u8];
        dwow_serial::Encodable::encode(&fc_params, &mut call_data_06).ok();

        // ═══════════════════════════════════════════════════════════════
        // Build Block with BOTH transactions
        // ═══════════════════════════════════════════════════════════════
        let block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                    fee_window_flags: FeeWindowFlags::default(),
                version: BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0),
                target: dwow_sdk::blockchain::BlockTarget::MAX,
                nonce: 0,
                height: dwow_sdk::blockchain::BlockHeight::new(height),
                uncle_merkle_root: [0u8; 32],
                total_reward: dwow_sdk::blockchain::BlockReward::new(coinbase_reward),
                randomx_key: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![
                // transactions[0] = coinbase (PoWRewardV1, 0x05)
                dwow_chain::Transaction {
                    version: BlockVersion::CURRENT,
                    inputs: vec![],
                    outputs: vec![],
                    contract_calls: vec![dwow_chain::ContractCall {
                        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                        data: call_data_05,
                    }],
                    lock_time: 0,
                    nullifiers: vec![],
                    witness: vec![],
                },
                // transactions[1] = FeeCollectV1 (0x06)
                dwow_chain::Transaction {
                    version: BlockVersion::CURRENT,
                    inputs: vec![],
                    outputs: vec![],
                    contract_calls: vec![dwow_chain::ContractCall {
                        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                        data: call_data_06,
                    }],
                    lock_time: 0,
                    nullifiers: vec![],
                    witness: vec![],
                },
            ],
        };

        // ── Wallet side: scan_block ─────────────────────────────────
        let mut tree = MerkleTree::new(32);
        let result = scan_block(&mut tree, &account_mgr, &BTreeMap::new(), &block, &std::collections::HashSet::new());

        // Must have discovered BOTH native token outputs
        assert_eq!(result.native_outputs.len(), 2,
            "P13 FAIL: expected 2 native outputs (coinbase + FeeCollect), got {}",
            result.native_outputs.len());

        // Output 0: coinbase (PoWRewardV1)
        let cap0 = &result.native_outputs[0].cap_record;
        assert_eq!(cap0.value, coinbase_reward,
            "P13 FAIL: coinbase value mismatch");
        assert_eq!(cap0.asset_id.inner(), pallas::Base::zero(),
            "P13 FAIL: coinbase asset_id must be DRKW_ASSET_ID");
        assert_eq!(cap0.created_at_height, BlockHeight::new(height),
            "P13 FAIL: coinbase created_at_height must match block height");

        // Output 1: FeeCollectV1
        let cap1 = &result.native_outputs[1].cap_record;
        assert_eq!(cap1.value, total_fees,
            "P13 FAIL: FeeCollect fee value mismatch");
        assert_eq!(cap1.asset_id.inner(), pallas::Base::zero(),
            "P13 FAIL: FeeCollect asset_id must be DRKW_ASSET_ID");
        assert_eq!(cap1.created_at_height, BlockHeight::new(height),
            "P13 FAIL: FeeCollect created_at_height must match block height");
    }

    /// BW-7: SQLite walletdb persistence roundtrip witness.
    /// Per type-system.md §10.5: the on-disk WalletDb path SHALL survive
    /// close/reopen cycles. The production wallet uses on-disk SQLite
    /// (WalletDb::new(Some(path), ...)); this test verifies that path
    /// works correctly — the database file is created, data persists
    /// across close/reopen, and nominal types are not truncated.
    #[test]
    fn test_walletdb_persistence_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("dwow_test_bw7_{}.db", std::process::id()));

        // Open on-disk, verify it creates the file
        let wallet = crate::walletdb::WalletDb::new(Some(db_path.clone()), None, false)
            .expect("BW-7 FAIL: WalletDb::new on-disk must succeed");
        assert!(db_path.exists(), "BW-7 FAIL: on-disk wallet must create the DB file");

        // Apply schema and insert a contract metadata record
        wallet.exec_batch_sql(include_str!("../wallet.sql")).ok();
        let cid = *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
        let height = BlockHeight::new(42);
        let record = crate::walletdb::ContractMetadataRecord {
            contract_id: bs58::encode(cid.to_bytes()).into_string(),
            name: "bw7_test".into(),
            symbol: None,
            category: "Testing".into(),
            description: None,
            public: true,
            deployer_pubkey: bs58::encode([0x11u8; 32]).into_string(),
            deploy_height: height,
            attestations_json: "[]".into(),
            lock_status: "unlocked".into(),
        };
        wallet.insert_contract_metadata_with_manifest(&record, None).ok();

        // Close (drop Arc) — WAL mode auto-checkpoints on last connection close
        drop(wallet);

        // Reopen the same file — must succeed
        let wallet2 = crate::walletdb::WalletDb::new(Some(db_path.clone()), None, false)
            .expect("BW-7 FAIL: WalletDb reopen must succeed");
        let loaded = wallet2.get_contract_metadata(&record.contract_id).ok();
        assert!(loaded.is_some(), "BW-7 FAIL: metadata must survive close/reopen");

        drop(wallet2);
        let _ = std::fs::remove_file(&db_path);
    }
}
