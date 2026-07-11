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
use dwow_chain::CoinCommitment;
use dwow_sdk::{
    bridgetree::Position,
    crypto::{
        poseidon_hash,
        BaseBlind, Blind, ContractId, FuncId, MerkleNode, MerkleTree, PublicKey,
        ScalarBlind, SecretKey, TokenId,
        DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
    },
    deploy::{ContractMetadata, DeployParamsV1},
    pasta::group::ff::PrimeField,
};
use dwow_native_token_contract::client::NativeToken;
use dwow_native_token_contract::model::{BurnParamsV1, CoinAttributes, SpendParamsV1, TransferParamsV1};
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
}

impl NativeTokenSource {
    fn as_str(&self) -> &'static str {
        match self {
            NativeTokenSource::PoWRewardV1 => "PoWRewardV1",
            NativeTokenSource::TransferV1 => "TransferV1",
            NativeTokenSource::SpendV1 => "SpendV1",
            NativeTokenSource::FeeV1 => "FeeV1",
        }
    }
}

/// A native token output discovered during block scan.
/// Carries enough data to construct a CapRecord + MerkleProof during persistence.
#[derive(Debug, Clone)]
pub(crate) struct NativeTokenDiscovery {
    pub(crate) cap_record: CapRecord,
    pub(crate) merkle_proof: MerkleProof,
}

/// A capability discovered via generic AEAD scan of non-native contract calls.
#[derive(Debug, Clone)]
pub(crate) struct CapabilityDiscovery {
    pub(crate) cap_record: CapRecord,
    pub(crate) merkle_proof: MerkleProof,
}

/// A nullifier published in a spending contract call.
/// Extracted from TransferV1/BurnV1/SpendV1 params.
/// Per V.2: nullifier is now typed Nullifier (↓nullify barb, zero-rejection
/// enforced at construction), not raw pallas::Base.
#[derive(Debug, Clone)]
pub(crate) struct NullifierRecord {
    pub(crate) nullifier: dwow_chain::Nullifier,
}

/// A contract deployment discovered during block scan.
#[derive(Debug, Clone)]
pub(crate) struct DeploymentDiscovery {
    pub(crate) contract_id: ContractId,
    pub(crate) deployer_pubkey: PublicKey,
    pub(crate) metadata: Option<ContractMetadata>,
    pub(crate) manifest_json: Option<String>,
    pub(crate) height: u32,
}

/// Result of scanning a single block — pure computation, no side effects.
/// The caller persists these results to the database.
#[derive(Debug, Clone)]
pub struct BlockScanResult {
    native_outputs: Vec<NativeTokenDiscovery>,
    capabilities: Vec<CapabilityDiscovery>,
    published_nullifiers: Vec<NullifierRecord>,
    deployments: Vec<DeploymentDiscovery>,
    messages: Vec<String>,
}

impl BlockScanResult {
    fn new() -> Self {
        Self {
            native_outputs: vec![],
            capabilities: vec![],
            published_nullifiers: vec![],
            deployments: vec![],
            messages: vec![],
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

/// Build a CapRecord + MerkleProof from a decrypted native token note.
/// Pure: mutates `tree` (append + witness) but does not touch any database.
/// Returns the cap record, its merkle proof, and a diagnostic message.
fn build_native_token_cap_record(
    tree: &mut MerkleTree,
    secret: &SecretKey,
    note: &NativeToken,
    height: u32,
    source: &NativeTokenSource,
    contract_id: ContractId,
    func_id: Option<FuncId>,
    capability_discriminant: Option<u8>,
) -> Option<(CapRecord, MerkleProof, String)> {
    let public_key = PublicKey::from_secret(*secret);
    let coin_attrs = CoinAttributes {
        version: 0,
        public_key,
        value: note.value,
        token_id: note.token_id,
        spend_hook: FuncId::from(note.spend_hook),
        user_data: note.user_data,
        blind: Blind(note.coin_blind),
    };
    let commitment = coin_attrs.to_coin();
    let commitment_bytes = commitment.to_bytes();

    // cap_id = bs58(blake2b(secret || commitment, person="DarkFi_CoinId"))
    let mut hasher = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"DarkFi_CoinId")
        .to_state();
    hasher.update(&secret.inner().to_repr());
    hasher.update(&commitment_bytes);
    let cap_id_hash = hasher.finalize();
    let cap_id = bs58::encode(cap_id_hash.as_bytes()).into_string();

    let leaf_pos = tree.current_position()
        .map(u64::from)
        .unwrap_or(0);
    let cap_leaf = MerkleNode::new(commitment.inner());
    tree.append(cap_leaf);
    // BridgeTree::witness requires the position to be registered via mark()
    // before it can return siblings. append() does not mark — we must mark
    // explicitly. Without this, witness() returns PositionNotMarked on empty
    // or freshly-checkpointed trees.
    tree.mark();

    let siblings: Vec<MerkleNode> = match tree.witness(Position::from(leaf_pos), 0) {
        Ok(s) => s,
        Err(_) => {
            tracing::error!(target: "dww::scan",
                "Merkle witness failed for leaf_pos={} — tree state corrupted", leaf_pos);
            return None;
        }
    };
    let mut sibling_strings: Vec<String> = siblings
        .iter()
        .map(|n| bs58::encode(n.inner().to_repr()).into_string())
        .collect();
    while sibling_strings.len() < dwow_sdk::crypto::constants::MERKLE_DEPTH_ORCHARD {
        let lvl = sibling_strings.len();
        sibling_strings.push(
            bs58::encode(dwow_sdk::crypto::smt::EMPTY_NODES_FP[lvl].to_repr()).into_string(),
        );
    }
    let root = match tree.root(0) {
        Some(n) => n.inner().to_repr(),
        None => {
            tracing::error!(target: "dww::scan",
                "Merkle root unavailable after append — tree state corrupted");
            return None;
        }
    };
    let merkle_proof = MerkleProof {
        siblings: sibling_strings,
        root: bs58::encode(root).into_string(),
    };

    let cap_record = CapRecord {
        cap_id: cap_id.clone(),
        value: note.value,
        token_id: TokenId(note.token_id),
        spend_hook: None,
        user_data: None,
        leaf_position: leaf_pos,
        commitment: CoinCommitment::from_base(commitment.inner()),
        contract_id,
        func_id,
        cap_blind: Blind(note.coin_blind),
        value_blind: Blind(note.value_blind),
        token_blind: Blind(note.token_blind),
        capability_discriminant,
        revoked: false,
        revoked_at_height: None,
        created_at_height: height,
    };

    let msg = format!(
        "Inserted native token {} cap {} at height {}",
        source.as_str(),
        &cap_id[..8],
        height
    );

    Some((cap_record, merkle_proof, msg))
}

/// Match published nullifiers against existing held capabilities.
/// Pure: recomputes `poseidon_hash([secret, commitment])` for each held cap,
/// returns (cap_id, height) pairs for matches. No database access.
fn match_nullifiers(
    existing_caps: &[CapRecord],
    secrets: &[SecretKey],
    published: &[NullifierRecord],
    height: u32,
) -> Vec<(String, u32)> {
    if published.is_empty() {
        return vec![];
    }
    let published_fps: Vec<pallas::Base> = published.iter().map(|r| r.nullifier.inner()).collect();
    let mut revoked = vec![];

    for cap in existing_caps {
        // cap.commitment stores the Poseidon hash of coin attributes as [u8; 32].
        // cap.cap_id is a Blake2b storage key (different value).
        // nf = poseidon_hash(secret, coin_commitment) requires the field element.
        let Some(commitment) = {
            let fp = cap.commitment.inner();
            if fp != pallas::Base::zero() { Some(fp) } else { None }
        }
        else {
            continue;
        };
        // Try each secret — the nullifier is poseidon_hash(secret, commitment).
        // Per Cornerstone 1: secrets come from AccountManager, passed by caller.
        for secret in secrets {
            let nullifier = poseidon_hash([secret.inner(), commitment]);
            if published_fps.contains(&&nullifier) {
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
    height: u32,
    function_code: u8,
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
            "[NATIVE_TOKEN] step=1 derive_instance status=FAIL reason=\"per-block key derivation failed for height {}: {}\"", height, e);
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
        tracing::info!(target: "dww::scan",
            "[NATIVE_TOKEN] step=2 aead_decode status=OK offset={}", off);
        let consumed = (cursor.position() - pos_before) as usize;
        off += consumed;

        let mut decrypted = false;
        for secret in &trial_secrets {
            if let Ok(decrypted_note) = generic_note.decrypt::<NativeToken>(secret) {
                tracing::info!(target: "dww::scan",
                    "[NATIVE_TOKEN] step=3 aead_decrypt status=OK");
                decrypted = true;
                if let Some((cap_record, merkle_proof, msg)) =
                    build_native_token_cap_record(
                        tree, secret, &decrypted_note, height, &source,
                        *NATIVE_TOKEN_CONTRACT_ID, None, None,
                    )
                {
                    tracing::info!(target: "dww::scan",
                        "[NATIVE_TOKEN] step=4 coin_reconstruct status=OK coin=0x{}",
                        hex::encode(&cap_record.commitment.to_bytes()));
                    results.push((cap_record, merkle_proof));
                    messages.push(msg);
                }
                break; // found match for this note → next note
            }
        }
        if !decrypted {
            tracing::debug!(target: "dww::scan",
                "[NATIVE_TOKEN] step=3 aead_decrypt status=FAIL reason=\"no key matched — wallet key differs from miner\"");
        }
    }

    tracing::info!(target: "dww::scan",
        "[NATIVE_TOKEN] step=5 caprecord_build status=OK count={}", results.len());
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
    height: u32,
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

        let function_code = call.data[0];

        // ── Spend detection: extract published nullifiers ──
        if matches!(function_code, 0x00 | 0x02 | 0x03 | 0x04) {
            let mut cursor = std::io::Cursor::new(&call.data[1..]);
            // V.2: NullifierRecord stores typed Nullifier, not raw pallas::Base
            let published: Vec<dwow_chain::Nullifier> = match function_code {
                0x03 => match TransferParamsV1::decode(&mut cursor) {
                    Ok(p) => p.inputs.iter().map(|inp| inp.nullifier).collect(),
                    Err(_) => vec![],
                },
                0x02 => match BurnParamsV1::decode(&mut cursor) {
                    Ok(p) => p.inputs.iter().map(|inp| inp.nullifier).collect(),
                    Err(_) => vec![],
                },
                0x04 => match SpendParamsV1::decode(&mut cursor) {
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
        // PoWRewardV1 (0x05): mint output
        // TransferV1  (0x03): receiver outputs
        // SpendV1     (0x04): change output
        // FeeV1       (0x00): change output
        if matches!(function_code, 0x00 | 0x03 | 0x04 | 0x05) {
            let (caps, msgs) = match discover_native_token_outputs(
                account_mgr, tree, &call.data, height, function_code,
            ) {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!(target: "dww::scan",
                        "[NATIVE_TOKEN] discover_native_token_outputs failed: {}", e);
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
/// PURE FUNCTION: same (secrets, existing_caps, tree, account_mgr, block) → same result.
/// No database access. No network. No mutable globals. Testable in isolation.
fn scan_block(
    tree: &mut MerkleTree,
    account_mgr: &dwow_accounts::AccountManager,
    block: &dwow_chain::Block,
) -> BlockScanResult {
    let mut result = BlockScanResult::new();
    let height_u32 = block.header.height as u32;

    result.messages.push(format!("[linear] Block height: {}", block.header.height));
    result.messages.push(format!(
        "[scan_block] Iterating over {} transactions",
        block.transactions.len()
    ));

    for tx in block.transactions.iter() {
        // ── Path 1: Native Token (sole special citizen) ──────
        let (native_outputs, nullifiers, mut msgs) =
            scan_native_token_contract_calls(account_mgr, tree, tx, height_u32);
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
                    if let Ok(params) = DeployParamsV1::decode(
                        &mut std::io::Cursor::new(&call.data[1..])
                    ) {
                        let contract_id = ContractId::derive_public(params.public_key);
                        let contract_id_str = bs58::encode(contract_id.to_bytes()).into_string();

                        let metadata = ContractMetadata::from_ix_bytes(&params.ix);
                        let manifest_result =
                            dwow_sdk::manifest::ContractManifest::from_deploy_ix(&params.ix);
                        let manifest_json = match manifest_result {
                            Some(Ok(ref m)) => serde_json::to_string(m).ok(),
                            _ => None,
                        };
                        result.deployments.push(DeploymentDiscovery {
                            contract_id,
                            deployer_pubkey: params.public_key,
                            metadata,
                            manifest_json,
                            height: height_u32,
                        });

                        result.messages.push(format!(
                            "[scan_block] Deployooor::DeployV1: {} at height {}",
                            &contract_id_str[..8], height_u32
                        ));
                    }
                }
                continue;
            }

            // ── Identity contract — O-Cap opcode detection ───
            if cid == *dwow_sdk::crypto::IDENTITY_CONTRACT_ID {
                if let Some(&fn_code) = call.data.first() {
                    let label = match fn_code {
                        0x09 => "RegisterCapabilityV1",
                        0x0a => "IssueCapabilityV1",
                        0x0b => "VerifyCapabilityV1",
                        0x0c => "RevokeCapabilityV1",
                        _ => "",
                    };
                    if !label.is_empty() {
                        result.messages.push(format!(
                            "[scan_block] O-Cap: {} detected at height {}",
                            label, height_u32
                        ));
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
                    off += consumed;

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

                    for secret in &trial_secrets {
                        if let Ok(plaintext) = generic_note.decrypt::<Vec<u8>>(secret) {
                            if let Ok(native_note) =
                                NativeToken::decode(&mut std::io::Cursor::new(&plaintext))
                            {
                                // Build cap record + merkle proof from tree
                                let source = NativeTokenSource::TransferV1; // generic path
                                let fn_code = call.data.first().copied().unwrap_or(0);
                                let func_id = Some(FuncId::from(pallas::Base::from(fn_code as u64)));
                                if let Some((cap_record, merkle_proof, msg)) =
                                    build_native_token_cap_record(
                                        tree, secret, &native_note, height_u32, &source,
                                        call.contract_id, func_id, None,
                                    )
                                {
                                    result.capabilities.push(CapabilityDiscovery {
                                        cap_record,
                                        merkle_proof,
                                    });
                                    result.messages.push(msg);
                                }
                            } else {
                                // AEAD succeeded, format unknown — log for diagnostics
                                result.messages.push(format!(
                                    "[CAPABILITY] unknown-format cap {} bytes cid={} height={}",
                                    plaintext.len(),
                                    &bs58::encode(call.contract_id.to_bytes()).into_string()[..8],
                                    block.header.height,
                                ));
                            }
                            break; // found matching secret → next note
                        }
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
        // Grab last scanned block height (stored as u32 in wallet db, convert to u64)
        let (last_scanned_u32, _) = self.get_last_scanned_block()?;
        let mut height: u64 = if last_scanned_u32 == 0 {
            let mut buf = vec![];
            self.reset(&mut buf)?;
            append_or_print(output, sender, print, buf).await;
            1 // Start scanning from genesis block (height 1)
        } else {
            // Defense-in-depth: always re-scan the last marked block.
            last_scanned_u32 as u64
        };

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
                let mut buf = vec![format!("Reading block {height} from local store...")];
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

                // Advance verified anchor height if this block has a
                // verified Caribina (Arweave) anchor.
                if block.header.anchor_tx_id != [0u8; 32] {
                    let anchor_height = block.header.height as u32;
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
                height += 1;
            }
        }
    }

    /// `scan_block_linear` processes a linear block: pure scan + persistence.
    ///
    /// Defense-in-depth: the scanned block marker is written BEFORE processing
    /// transactions. If the process crashes mid-scan, the marker exists but
    /// the Merkle tree checkpoint doesn't. On restart, scan_blocks() detects
    /// this and re-scans the block (capabilities use INSERT OR IGNORE).
    pub fn scan_block_linear(
        &self,
        tree: &mut MerkleTree,
        block: &dwow_chain::Block,
    ) -> Result<BlockScanResult> {
        let height_u32 = block.header.height as u32;

        // Write marker BEFORE processing — enables crash recovery.
        self.wallet.insert_scanned_block(
            &height_u32,
            &HeaderHash(*block.header.previous.as_bytes()),
            &None,
        )?;

        // Checkpoint the merkle tree
        tree.checkpoint(block.header.height as usize);

        // Get existing held caps for nullifier detection (spends by other parties)
        let existing_caps = self.wallet.get_held_capabilities(Some(false)).unwrap_or_else(|e| {
            tracing::error!(target: "dww::scan",
                "Failed to load held capabilities: {:?} — nullifier detection skipped for block {}",
                e, height_u32);
            vec![]
        });

        // ── Pure scan: no DB access ──────────────────────────
        let mut result = scan_block(tree, &self.account_mgr, block);

        // ── Persist results ──────────────────────────────────
        for out in &result.native_outputs {
            if let Err(e) = self.wallet.insert_capability(&out.cap_record, &out.merkle_proof) {
                tracing::error!(target: "dww::scan",
                    "Failed to insert native token cap {}: {:?}",
                    &out.cap_record.cap_id[..8.min(out.cap_record.cap_id.len())], e);
            }
        }
        // ── Manifest-driven capability discrimination (Path 2) ──
        // wallet.md §2.2: resolve capability type from the contract manifest.
        // For each capability with a func_id, look up the manifest and resolve
        // the discriminant. Must happen before insert_capability so the column
        // is populated on first write.
        for cap in &mut result.capabilities {
            if let Some(fid) = cap.cap_record.func_id {
                let fn_code = fid.inner().to_repr()[0];
                if fn_code != 0 && fn_code != 0x05 {
                    let cid_str = bs58::encode(&cap.cap_record.contract_id.to_bytes()).into_string();
                    if let Ok(Some(manifest)) = self.wallet.get_contract_manifest(&cid_str) {
                        if let Some(resolved) = manifest.resolve_capability(fn_code) {
                            cap.cap_record.capability_discriminant = Some(resolved.discriminant);
                        }
                    }
                }
            }
        }
        for cap in &result.capabilities {
            if let Err(e) = self.wallet.insert_capability(&cap.cap_record, &cap.merkle_proof) {
                tracing::error!(target: "dww::scan",
                    "Failed to insert capability cap {}: {:?}",
                    &cap.cap_record.cap_id[..8.min(cap.cap_record.cap_id.len())], e);
            }
        }

        // Apply nullifier revocations
        let secrets = self.account_mgr.secrets();
        let revoked = match_nullifiers(&existing_caps, &secrets, &result.published_nullifiers, height_u32);
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
            if self.wallet.insert_contract_metadata(&record).is_ok() {
                if let Some(ref manifest_json) = dep.manifest_json {
                    let _ = self.wallet.store_manifest(&contract_id_str, manifest_json);
                }
            }
        }

        // Update the merkle trees (must happen after all transaction processing)
        self.wallet.insert_merkle_trees(&[
            (b"capability_commitment_tree", tree),
        ])?;

        Ok(result)
    }

}

// Unit tests for scan_block, discover_native_token_outputs, and cap_id derivation
// are in the test module below. They use in-memory MerkleTree (BridgeTree) and
// AccountManager — no sled DB required. Tests run in <10ms via cargo test.

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::crypto::keypair::SecretKey;
    use dwow_sdk::crypto::PublicKey;
    use dwow_sdk::crypto::note::AeadEncryptedNote;

    /// F2: discover_native_token_outputs is deterministic.
    /// Invariant: AEAD decrypt is deterministic for given key + ciphertext.
    /// Falsifiable: correct key finds output, wrong key finds nothing.
    #[test]
    fn test_discover_determinism() {
        let height: u32 = 42;

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

        // Per-block derived key from the same master.
        let per_block_sk = master_sk.derive_instance(&NATIVE_TOKEN_CONTRACT_ID, &height.to_le_bytes())
            .expect("valid test derive_instance");
        let per_block_pk = PublicKey::from_secret(per_block_sk);

        // Create a minimal encrypted note (AeadEncryptedNote + NativeToken encoding)
        use dwow_sdk::pasta::group::ff::PrimeField;
        let nt = dwow_native_token_contract::client::NativeToken {
            value: 50_000_000,
            token_id: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: pallas::Base::from(1u64),
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
        let decrypted: dwow_native_token_contract::client::NativeToken = aes.decrypt(&per_block_sk)
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
        let decrypted: dwow_native_token_contract::client::NativeToken = aes.decrypt(&per_block_sk)
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
        let decrypted2: dwow_native_token_contract::client::NativeToken = decoded_aes.decrypt(&per_block_sk)
            .expect("F2 FAIL Step 4: decrypt with per_block_sk must work");
        assert_eq!(decrypted2.value, 50_000_000, "F2 FAIL Step 4: manual decrypt value mismatch");

        // Positive: AccountManager has the correct secret
        let (caps, _) = discover_native_token_outputs(
            &account_mgr, &mut tree, &call_data, height, 0x05,
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
            &wrong_mgr, &mut MerkleTree::new(32), &call_data, height, 0x05,
        ).expect("F2 FAIL: discover must succeed (returns empty with wrong key)");
        assert!(caps2.is_empty(),
            "F2 FAIL: discover should find nothing when key doesn't match");

        // Determinism: same inputs twice = same outputs
        let mut tree2 = MerkleTree::new(32);
        let (caps3, _) = discover_native_token_outputs(
            &account_mgr, &mut tree2, &call_data, height, 0x05,
        ).expect("F2 FAIL: discover must be deterministic on second call");
        assert_eq!(caps.len(), caps3.len(),
            "F2 FAIL: discover must be deterministic — different result on second call");
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
            .personal(b"DarkFi_CoinId")
            .to_state();
        hasher.update(&secret_bytes);
        hasher.update(&commitment_bytes);
        let cap_id_hash = hasher.finalize();
        let cap_id = bs58::encode(cap_id_hash.as_bytes()).into_string();

        // Expected value from Python model:
        //   secret = bytes([0x42] + [0x00]*31)
        //   commitment = bytes([0x99]*32)
        //   cap_id = bs58(blake2b(secret || commitment, person=b"DarkFi_CoinId"))
        assert_eq!(cap_id, "bbT1qqNUBXnwRuxZgmhQHTr9HiZrbuWpsCWJJAEiLij",
            "G5 FAIL: cap_id doesn't match Python model — cross-language determinism broken");
    }
}
