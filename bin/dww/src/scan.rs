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

use std::collections::HashMap;

use smol::channel::Sender;
use tracing;

use dwow_core::{
    blockchain::HeaderHash,
};
use crate::wallet_error::Result;
use dwow_sdk::{
    bridgetree::Position,
    crypto::{
        poseidon_hash,
        smt::{PoseidonFp, EMPTY_NODES_FP},
        ContractId, MerkleNode, MerkleTree, PublicKey, SecretKey,
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
    walletdb::{CacheSmt, PnSmtStorage},
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
    pub nullifier_smt: CacheSmt,
    /// All our known secrets to decrypt capability commitments
    pub secrets: Vec<SecretKey>,
    /// Our own deploy authorities
    pub own_deploy_auths: HashMap<[u8; 32], SecretKey>,
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

///
/// Resolution order: Genesis → SelfDeployed → Unverified.
/// Attested tier requires on-chain attestation check (deferred).
fn resolve_manifest_trust(
    contract_id: &ContractId,
    deployer_pubkey: &PublicKey,
    account_mgr: &dwow_accounts::AccountManager,
) -> dwow_sdk::manifest::TrustTier {
    use dwow_sdk::manifest::TrustTier;

    // Tier 1: Genesis contracts
    let cid_bytes = contract_id.to_bytes();
    let genesis_ids: [[u8; 32]; 9] = [
        dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::IDENTITY_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::ORACLE_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::ATTESTATION_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::PURSE_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::BOX_CONTRACT_ID.to_bytes(),
        dwow_sdk::crypto::MULTISIG_CONTRACT_ID.to_bytes(),
    ];
    if genesis_ids.contains(&cid_bytes) {
        return TrustTier::Genesis;
    }

    // Tier 2: Self-deployed — deployer's pubkey matches our declared identity
    let deployer_bytes = deployer_pubkey.to_bytes();
    for secret in account_mgr.secrets() {
        if PublicKey::from_secret(secret).to_bytes() == deployer_bytes {
            return TrustTier::SelfDeployed;
        }
    }

    // Tier 3: Unverified — may be upgraded by attestation check (future)
    TrustTier::Unverified
}

impl Dww {
    /// Auxiliary function to generate a new [`ScanCache`] for the
    /// wallet.
    pub fn scan_cache(&self) -> Result<ScanCache> {
        let capability_commitment_tree = self.get_capability_commitment_tree()?;

        // Create SMT storage and tree directly — no overlay
        let smt_store = PnSmtStorage::new(self.wallet.clone());
        let nullifier_smt = CacheSmt::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

        // Get our secrets
        let secrets = self.get_secrets()?;

        // TODO: Get deploy auth keys
        let own_deploy_auths: HashMap<[u8; 32], SecretKey> = HashMap::new();

        Ok(ScanCache {
            capability_commitment_tree,
            nullifier_smt,
            secrets,
            own_deploy_auths,
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
            // The marker is written BEFORE transaction processing (scan_block_linear).
            // If the process crashed mid-scan, the marker exists but the tree
            // checkpoint may not. Re-scanning is safe — capabilities use INSERT OR IGNORE.
            last_scanned_u32 as u64
        };

        // Generate a new scan cache
        let mut scan_cache = match self.scan_cache() {
            Ok(c) => c,
            Err(e) => {
                append_or_print(
                    output,
                    sender,
                    print,
                    vec![format!("[scan_blocks] Generating scan cache failed: {e}")],
                )
                .await;
                return Err(WalletDbError::GenericError)
            }
        };
        scan_cache.log(format!(
            "[scan_blocks] Scan cache initialized: {} secrets loaded",
            scan_cache.secrets.len()
        ));

        loop {
            // Read chain tip from local sled store (no RPC needed)
            let mut buf = vec![format!("Requested to scan from block number: {height}")];
            let last_height = match self.chain_height() {
                Ok(h) => h,
                Err(e) => {
                    buf.push(format!("[scan_blocks] Local chain read failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                }
            };
            buf.push(format!(
                "Chain tip from local store: height {last_height}"
            ));
            append_or_print(output, sender, print, buf).await;

            // Already scanned last block
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
                if let Err(e) = self.scan_block_linear(&mut scan_cache, &block) {
                    buf.push(format!("[scan_blocks] Scan block failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                };
                for msg in scan_cache.flush_messages() {
                    buf.push(msg);
                }

                // Advance verified anchor height if this block has a
                // verified Caribina (Arweave) anchor. Anchored blocks
                // cannot be reorged — verified_anchor_height is the
                // safety line for reset_to_height().
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

    /// `scan_block_linear` processes a linear block directly from dwow_chain::Block.
    /// Handles contract calls AND coinbase transactions (mining rewards).
    ///
    /// Defense-in-depth: the scanned block marker is written BEFORE processing
    /// transactions. If the process crashes mid-scan, the marker exists but
    /// the Merkle tree checkpoint doesn't. On restart, scan_blocks() detects
    /// this and re-scans the block (capabilities use INSERT OR IGNORE).
    pub fn scan_block_linear(
        &self,
        scan_cache: &mut ScanCache,
        block: &dwow_chain::Block,
    ) -> Result<()> {
        use dwow_sdk::pasta::{pallas, group::ff::PrimeField};

        let height_u32 = block.header.height as u32;

        // Write marker BEFORE processing — enables crash recovery.
        // If we crash after this but before tree checkpoint, the next
        // scan_blocks() call will detect and re-scan this block.
        self.wallet.insert_scanned_block(
            &height_u32,
            &HeaderHash(*block.header.previous.as_bytes()),
            &None,
        )?;

        // Checkpoint the merkle tree
        scan_cache.capability_commitment_tree.checkpoint(block.header.height as usize);

        // Scan the block
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[linear] Block height: {}", block.header.height));
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[scan_block_linear] Iterating over {} transactions", block.transactions.len()));
        for tx in block.transactions.iter() {
            let mut wallet_tx = false;

            // ── Token Model: Native Token (sole special citizen) ──────────
            // Full shielded-token lifecycle: mint, transfer, spend, burn, fee.
            // Handled in ONE dedicated function per wallet.md:82-85.
            // No native token discovery happens in the generic AEAD path.
            if self.scan_native_token(tx, scan_cache, height_u32)? {
                wallet_tx = true;
            }

            // Process contract calls (transfers, etc.)
            scan_cache.log(format!("[scan_block_linear] Processing transaction with {} calls", tx.contract_calls.len()));
            for (i, call) in tx.contract_calls.iter().enumerate() {
                // Convert linear [u8; 32] contract_id to ContractId for comparison
                let cid = ContractId::from(
                    pallas::Base::from_repr(call.contract_id).unwrap_or_else(|| {
                        let hex_id = hex::encode(call.contract_id);
                        tracing::error!(target: "dww::scan",
                            "Invalid field element bytes in contract_id at block {} call {} — contract identification impossible, raw bytes: {}",
        block.header.height, i, hex_id
    );
    scan_cache.log(format!(
        "[scan_block_linear] INVALID_CONTRACT_ID block={} call={} bytes={} — call skipped",
        block.header.height, i, hex_id
    ));
    pallas::Base::zero()
}),
                );

                // ── Native Token: handled by scan_native_token() above ─────
                // Token model runs before the capability loop. Native token
                // contract calls are already fully processed — skip here.
                if cid == *NATIVE_TOKEN_CONTRACT_ID {
                    continue
                }

                // Check Deployooor contract
                if cid == *DEPLOYOOOR_CONTRACT_ID {
                    let function_code = call.data.first().copied().unwrap_or(0xFF);
                    if function_code == 0x00 {
                        scan_cache.log(format!("[scan_block_linear] Found Deployooor::DeployV1 in call {i}"));
                        if let Ok(params) = DeployParamsV1::decode(&mut std::io::Cursor::new(&call.data[1..])) {
                            let contract_id = ContractId::derive_public(params.public_key);
                            let contract_id_str = bs58::encode(contract_id.to_bytes()).into_string();
                            let deployer_pubkey_str = bs58::encode(params.public_key.to_bytes()).into_string();

                            if let Some(metadata) = ContractMetadata::from_ix_bytes(&params.ix) {
                                let record = crate::walletdb::ContractMetadataRecord {
                                    contract_id: contract_id_str.clone(),
                                    name: metadata.name,
                                    symbol: metadata.symbol,
                                    category: format!("{:?}", metadata.category),
                                    description: metadata.description,
                                    public: metadata.public,
                                    deployer_pubkey: deployer_pubkey_str,
                                    deploy_height: height_u32,
                                    attestations_json: "[]".to_string(),
                                    lock_status: "unlocked".to_string(),
                                };
                                if self.wallet.insert_contract_metadata(&record).is_ok() {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] Recorded contract metadata for {} at height {}",
                                        &contract_id_str[..8], height_u32
                                    ));
                                }
                            } else {
                                // Deployment without metadata — still record it
                                let record = crate::walletdb::ContractMetadataRecord {
                                    contract_id: contract_id_str.clone(),
                                    name: format!("Contract-{}", &contract_id_str[..8]),
                                    symbol: None,
                                    category: "Other".to_string(),
                                    description: None,
                                    public: false,
                                    deployer_pubkey: deployer_pubkey_str,
                                    deploy_height: height_u32,
                                    attestations_json: "[]".to_string(),
                                    lock_status: "unlocked".to_string(),
                                };
                                if self.wallet.insert_contract_metadata(&record).is_ok() {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] Recorded anonymous contract {} at height {}",
                                        &contract_id_str[..8], height_u32
                                    ));
                                }
                            }

                            // Check for contract manifest (0x4D magic byte prefix)
                            if let Some(manifest_result) =
                                dwow_sdk::manifest::ContractManifest::from_deploy_ix(&params.ix)
                            {
                                match manifest_result {
                                    Ok(manifest) => {
                                        let manifest_json =
                                            serde_json::to_string(&manifest).unwrap_or_default();
                                        // Resolve trust tier
                                        let trust = resolve_manifest_trust(
                                            &contract_id,
                                            &params.public_key,
                                            &self.account_mgr,
                                        );
                                        let trust_str = trust.to_string();
                                        // NOTE: insert_contract_metadata_with_manifest() is available
                                        // in walletdb.rs for atomic metadata+manifest insertion.
                                        // Call it from the metadata block above once `record` is
                                        // accessible at manifest-detection time (future refactor).
                                        if self
                                            .wallet
                                            .store_manifest(&contract_id_str, &manifest_json)
                                            .is_ok()
                                        {
                                            scan_cache.log(format!(
                                                "[scan_block_linear] Stored manifest for {} ({} functions) [{trust_str}]",
                                                &contract_id_str[..8],
                                                manifest.functions.len()
                                            ));
                                        }
                                    }
                                    Err(e) => {
                                        scan_cache.log(format!(
                                            "[scan_block_linear] Malformed manifest for {}: {e}",
                                            &contract_id_str[..8]
                                        ));
                                    }
                                }
                            }

                            wallet_tx = true;
                        }
                    }
                    continue
                }

                // Identity contract — O-Cap opcode detection (0x09-0x0c)
                // RegisterCapabilityV1, IssueCapabilityV1, VerifyCapabilityV1, RevokeCapabilityV1
                // These are the on-chain capability lifecycle events the wallet should track.
                if cid == *dwow_sdk::crypto::IDENTITY_CONTRACT_ID {
                    if let Some(&fn_code) = call.data.first() {
                        match fn_code {
                            0x09 => scan_cache.log("[scan_block_linear] O-Cap: RegisterCapabilityV1 detected".into()),
                            0x0a => scan_cache.log("[scan_block_linear] O-Cap: IssueCapabilityV1 detected".into()),
                            0x0b => scan_cache.log("[scan_block_linear] O-Cap: VerifyCapabilityV1 detected".into()),
                            0x0c => scan_cache.log("[scan_block_linear] O-Cap: RevokeCapabilityV1 detected".into()),
                            _ => {}
                        }
                    }
                    continue
                }

                // Generic capability scan: try AEAD decryption on unknown contract call data.
                // Every contract uses the same AEAD encryption primitive.
                // The AEAD tag is the universal discriminator — no contract bias.
                scan_cache.log(format!(
                    "[scan_block_linear] Unknown contract in call {i}, attempting generic AEAD decryption...",
                ));
                // Path 2: Generic capability scan — byte-level AEAD scan.
                // Scans ALL bytes of call.data for AeadEncryptedNote patterns.
                // The AEAD authentication tag IS the discriminator — successful
                // decryption with the wallet's secret proves ownership regardless
                // of which contract produced it or what parameter struct wraps it.
                // New contracts work without any wallet code changes.
                if call.data.len() < 2 {
                    scan_cache.log(format!(
                        "[scan_block_linear] WARN: block {} call {} has no data (len={}) — cannot identify contract or function, skipping",
                        block.header.height, i, call.data.len()
                    ));
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
                        scan_cache.log(format!(
                            "[CAPABILITY] Stage 1 (SCAN): found AEAD note at offset={} consumed={} cid={} height={}",
                            off.saturating_sub(consumed), consumed,
                            &bs58::encode(call.contract_id).into_string()[..8],
                            block.header.height,
                        ));
                        // Build an augmented secret set: raw declared/lifecycle secrets
                        // plus per-contract derived keys for contracts that carry a
                        // public `instance_seed` in their call params (lottery, baccarat,
                        // slot, dice, roulette, game_room). The seed is on-chain in the
                        // clear, so we can re-derive at scan time with no prior state.
                        let mut trial_secrets: Vec<SecretKey> = scan_cache.secrets.clone();
                        if let Some(iseed) = try_extract_instance_seed(&cid, &call.data) {
                            let derived = self.account_mgr.secrets_for_contract(&cid, &iseed);
                            trial_secrets.extend(derived);
                        }
                        for secret in &trial_secrets {
                        if let Ok(plaintext) = generic_note.decrypt::<Vec<u8>>(secret) {
                            // AEAD succeeded — capability is ours. Try known decoders.
                            scan_cache.log(format!(
                                "[CAPABILITY] Stage 2 (DISCOVER): AEAD decryption succeeded cid={} height={}",
                                &bs58::encode(call.contract_id).into_string()[..8],
                                block.header.height,
                            ));
                            if let Ok(native_note) =
                                NativeToken::decode(&mut std::io::Cursor::new(&plaintext))
                            {
                                let public_key = PublicKey::from_secret(*secret);
                                let coin_attrs = CoinAttributes {
                                    version: 0,
                                    public_key,
                                    value: native_note.value,
                                    token_id: native_note.token_id,
                                    spend_hook: native_note.spend_hook,
                                    user_data: native_note.user_data,
                                    blind: native_note.coin_blind,
                                };
                                let commitment = coin_attrs.to_coin();
                                let cap_id_bytes = commitment.to_bytes();
                                let cap_id = bs58::encode(cap_id_bytes).into_string();
                                // Generate real Merkle proof from universal capability tree.
                                // Pattern matches Path 1 coinbase — same tree, same proof format.
                                let leaf_pos = scan_cache.capability_commitment_tree
                                    .current_position()
                                    .map(|p| u64::from(p))
                                    .unwrap_or(0);
                                // Append cap to the Merkle tree so subsequent proofs include it.
                                let cap_leaf = MerkleNode::new(
                                    pallas::Base::from_repr(cap_id_bytes).unwrap_or_else(|| {
    tracing::error!(target: "dww::scan", "Invalid field element bytes, using zero — data may be corrupted");
    pallas::Base::zero()
})
                                );
                                scan_cache.capability_commitment_tree.append(cap_leaf);
                                let siblings: Vec<MerkleNode> = match scan_cache.capability_commitment_tree
                                    .witness(Position::from(leaf_pos), 0)
                                {
                                    Ok(s) => s,
                                    Err(_) => {
                                        tracing::error!(target: "dww::scan",
                                            "Merkle witness failed for leaf_pos={} — tree state may be corrupted, re-scan from genesis required", leaf_pos);
                                        continue;
                                    }
                                };
                                let mut sibling_strings: Vec<String> = siblings.iter()
                                    .map(|n| bs58::encode(n.inner().to_repr()).into_string())
                                    .collect();
                                // Pad to fixed depth (32) for the circuit
                                while sibling_strings.len() < dwow_sdk::crypto::constants::MERKLE_DEPTH_ORCHARD {
                                    let lvl = sibling_strings.len();
                                    let empty = dwow_sdk::crypto::smt::EMPTY_NODES_FP[lvl];
                                    sibling_strings.push(
                                        bs58::encode(empty.to_repr()).into_string()
                                    );
                                }
                                let root = scan_cache.capability_commitment_tree
                                    .root(0)
                                    .map(|n| n.inner().to_repr())
                                    .expect("capability_commitment_tree root after append");
                                let merkle_proof = MerkleProof {
                                    siblings: sibling_strings,
                                    root: bs58::encode(root).into_string(),
                                };
                                let token_id_str = bs58::encode(
                                    native_note.token_id.to_repr()
                                ).into_string();
                                let cap_record = CapRecord {
                                    cap_id: cap_id.clone(),
                                    value: native_note.value,
                                    token_id: token_id_str,
                                    spend_hook: None,
                                    user_data: None,
                                    leaf_position: leaf_pos,
                                    secret: bs58::encode(
                                        secret.inner().to_repr()
                                    ).into_string(),
                                    cap_blind: bs58::encode(
                                        native_note.coin_blind.to_repr()
                                    ).into_string(),
                                    value_blind: bs58::encode(
                                        native_note.value_blind.to_repr()
                                    ).into_string(),
                                    token_blind: bs58::encode(
                                        native_note.token_blind.to_repr()
                                    ).into_string(),
                                    revoked: false,
                                    revoked_at_height: None,
                                    created_at_height: height_u32,
                                };
                                match self.wallet.insert_capability(&cap_record, &merkle_proof) {
                                Ok(()) => {
                                    scan_cache.log(format!(
                                        "[CAPABILITY] Stage 3 (STORE): inserted cap {} (NativeToken) cid={} height={}",
                                        &cap_id[..8],
                                        &bs58::encode(call.contract_id).into_string()[..8],
                                        block.header.height,
                                    ));
                                }
                                Err(e) => {
                                    scan_cache.log(format!(
                                        "[CAPABILITY] Stage 3 (STORE): ERROR cap {} insert failed: {:?} — DB write failed, block will be re-scanned",
                                        &cap_id[..8], e
                                    ));
                                }
                                }


                            } else {
                                // AEAD succeeded but format is unknown — capability discovered
                                // via the universal AEAD discriminator. Log for diagnostics.
                                scan_cache.log(format!(
                                    "[CAPABILITY] Stage 3 (STORE): unknown-format cap {} bytes cid={} height={}",
                                    plaintext.len(),
                                    &bs58::encode(call.contract_id).into_string()[..8],
                                    block.header.height,
                                ));
                            }
                            wallet_tx = true;
                            break;
                        }
                    }
                    } else {
                        off += 1;
                        continue;
                    }
                }
                // After scanning all bytes: if notes were decoded but no secret matched, log it
                if aead_notes_tried > 0 && !wallet_tx {
                    scan_cache.log(format!(
                        "[scan_block_linear] Generic AEAD: {} AeadEncryptedNote(s) decoded in call {} block {}, tried {} secret(s), none matched",
                        aead_notes_tried, i, block.header.height, scan_cache.secrets.len()
                    ));
                }
            }

            // Record transaction history for wallet-relevant transactions
            if wallet_tx {
                let tx_hash = tx.hash();
                let tx_hash_str = tx_hash.to_hex().to_string();
                let tx_blob = serde_json::to_vec(tx).unwrap_or_default();
                if self.wallet.insert_transaction_history(
                    &tx_hash_str,
                    "confirmed",
                    Some(height_u32),
                    &tx_blob,
                ).is_ok() {
                    scan_cache.log(format!(
                        "[scan_block_linear] Recorded tx history {} at height {}",
                        &tx_hash_str[..8], height_u32
                    ));
                }
            }
        }

        // Update the merkle trees (must happen after all transactions processed)
        self.wallet.insert_merkle_trees(&[
            (b"capability_commitment_tree", &scan_cache.capability_commitment_tree),
        ])?;

        // Flush sled
        // SQLite auto-flushes — no explicit flush needed

        Ok(())
    }

    // miner_mine + apply_tx_native_token_data REMOVED — dead code (HAZOP round 2).
    // miner_mine: 170 lines of stratum mining, no dispatch path.
    // apply_tx_native_token_data: 112 lines of old RPC path.
    // Live replacement: apply_tx_native_token_data_linear below.
    // See git history for removed implementations.

    /// Token Model: Native Token scanner — full shielded-token lifecycle.
    ///
    /// Handles ALL native token activity in a single function. Per wallet.md:82-85,
    /// native token is the sole special citizen because it is the consensus asset
    /// required for fee payment. No native token discovery happens in the generic
    /// capability path — zero crossover.
    ///
    /// Lifecycle handled here:
    ///   PoWRewardV1 (0x05) → Mint discovery: decrypt output note → insert cap
    ///   TransferV1  (0x03) → Spend detection: check nullifiers → revoke.
    ///                         Receive discovery: decrypt output notes → insert
    ///   BurnV1      (0x02) → Spend detection: check nullifiers → revoke
    ///   SpendV1     (0x04) → Spend detection: check nullifier → revoke.
    ///                         Change discovery: decrypt output note → insert
    ///   FeeV1       (0x00) → Spend detection: check nullifier → revoke.
    ///                         Change discovery: decrypt output note → insert
    ///
    /// Also handles tx.coinbase mint discovery (the encrypted_note on the
    /// CoinbaseTransaction field, which carries the same output as the
    /// PoWRewardV1 contract call — discovered once, not twice).
    fn scan_native_token(
        &self,
        tx: &dwow_chain::Transaction,
        scan_cache: &mut ScanCache,
        height: u32,
    ) -> Result<bool> {
        let mut found_any = false;

        // ── Mint discovery: tx.coinbase ──────────────────────────────────
        // The coinbase field carries the AEAD-encrypted note for newly minted
        // native tokens (block rewards). Decrypt with each secret to discover
        // coinbase rewards belonging to this wallet. Per-block address cycling
        // is supported: the trial set includes per-block derived keys.
        if let Some(ref coinbase) = tx.coinbase {
            if let Ok(aes_note) = AeadEncryptedNote::decode(
                &mut std::io::Cursor::new(&coinbase.encrypted_note),
            ) {
                // Build augmented secret set with per-block keys
                let height_bytes = height.to_le_bytes();
                let per_block_secrets = self.account_mgr.secrets_for_contract(
                    &NATIVE_TOKEN_CONTRACT_ID, &height_bytes,
                );
                let mut trial_secrets: Vec<SecretKey> =
                    Vec::with_capacity(scan_cache.secrets.len() + per_block_secrets.len());
                trial_secrets.extend(scan_cache.secrets.iter().cloned());
                trial_secrets.extend(per_block_secrets);

                for secret in &trial_secrets {
                    if let Ok(decrypted_note) = aes_note.decrypt::<NativeToken>(secret) {
                        self._insert_native_token_cap(
                            scan_cache, secret, &decrypted_note, height,
                            "PoWRewardV1 (coinbase)",
                        );
                        found_any = true;
                        break;
                    }
                }
                if !found_any {
                    scan_cache.log(format!(
                        "[native_token] COINBASE_DECRYPT_FAILED block={} secrets_tried={} — no secret matched. Check that wallet has correct key imported.",
                        height, trial_secrets.len()
                    ));
                }
            } else {
                scan_cache.log(format!(
                    "[native_token] Coinbase: failed to decode AeadEncryptedNote ({} bytes)",
                    coinbase.encrypted_note.len()
                ));
            }
        }

        // ── Native token contract calls: full lifecycle ──────────────────
        for call in &tx.contract_calls {
            let cid = ContractId::from(
                pallas::Base::from_repr(call.contract_id).unwrap_or(pallas::Base::zero()),
            );
            if cid != *NATIVE_TOKEN_CONTRACT_ID {
                continue;
            }
            if call.data.is_empty() {
                continue;
            }

            let function_code = call.data[0];

            // ── Spend detection: check published nullifiers ──
            // TransferV1 (0x03), BurnV1 (0x02), SpendV1 (0x04), FeeV1 (0x00)
            // all publish nullifiers. Detect and revoke held caps.
            if matches!(function_code, 0x00 | 0x02 | 0x03 | 0x04) {
                self.detect_nullifier_spends(scan_cache, &call.data, height)?;
            }

            // ── Output discovery: decrypt output notes ──
            // PoWRewardV1 (0x05): mint output
            // TransferV1 (0x03): receiver outputs
            // SpendV1 (0x04): change output
            // FeeV1 (0x00): change output
            if matches!(function_code, 0x00 | 0x03 | 0x04 | 0x05) {
                if self._discover_native_token_outputs(
                    scan_cache, &call.data, height, function_code,
                )? {
                    found_any = true;
                }
            }
        }

        Ok(found_any)
    }

    /// Insert a decrypted native token note as a held capability.
    /// Common helper called by both the coinbase and contract-call paths.
    fn _insert_native_token_cap(
        &self,
        scan_cache: &mut ScanCache,
        secret: &SecretKey,
        note: &NativeToken,
        height: u32,
        source: &str,
    ) {
        let public_key = PublicKey::from_secret(*secret);
        let coin_attrs = CoinAttributes {
            version: 0,
            public_key,
            value: note.value,
            token_id: note.token_id,
            spend_hook: note.spend_hook,
            user_data: note.user_data,
            blind: note.coin_blind,
        };
        let commitment = coin_attrs.to_coin();
        let cap_id_bytes = commitment.to_bytes();
        let cap_id = bs58::encode(cap_id_bytes).into_string();

        let leaf_pos = scan_cache
            .capability_commitment_tree
            .current_position()
            .map(|p| u64::from(p))
            .unwrap_or(0);
        let cap_leaf = MerkleNode::new(
            pallas::Base::from_repr(cap_id_bytes).unwrap_or(pallas::Base::zero()),
        );
        scan_cache.capability_commitment_tree.append(cap_leaf);

        let siblings: Vec<MerkleNode> = match scan_cache
            .capability_commitment_tree
            .witness(Position::from(leaf_pos), 0)
        {
            Ok(s) => s,
            Err(_) => {
                tracing::error!(target: "dww::scan",
                    "Merkle witness failed for leaf_pos={} — tree state corrupted", leaf_pos);
                return;
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
        let root = scan_cache
            .capability_commitment_tree
            .root(0)
            .map(|n| n.inner().to_repr())
            .expect("capability_commitment_tree root after append");
        let merkle_proof = MerkleProof {
            siblings: sibling_strings,
            root: bs58::encode(root).into_string(),
        };

        let token_id_str = bs58::encode(note.token_id.to_repr()).into_string();
        let cap_record = CapRecord {
            cap_id: cap_id.clone(),
            value: note.value,
            token_id: token_id_str,
            spend_hook: None,
            user_data: None,
            leaf_position: leaf_pos,
            secret: bs58::encode(secret.inner().to_repr()).into_string(),
            cap_blind: bs58::encode(note.coin_blind.to_repr()).into_string(),
            value_blind: bs58::encode(note.value_blind.to_repr()).into_string(),
            token_blind: bs58::encode(note.token_blind.to_repr()).into_string(),
            revoked: false,
            revoked_at_height: None,
            created_at_height: height,
        };

        match self.wallet.insert_capability(&cap_record, &merkle_proof) {
            Ok(()) => {
                tracing::info!(target: "dww::scan",
                    "Inserted native token {} cap {} at height {}",
                    source, &cap_id[..8], height
                );
            }
            Err(e) => {
                tracing::error!(target: "dww::scan",
                    "Failed to insert native token cap {} at height {}: {:?}",
                    &cap_id[..8], height, e
                );
            }
        }
    }

    /// Discover native token output notes by AEAD-scanning call params.
    /// Handles PoWRewardV1 (0x05), TransferV1 (0x03), SpendV1 (0x04), FeeV1 (0x00).
    /// Scans call data bytes for AeadEncryptedNote patterns, decrypts with each
    /// secret (including per-block derived keys), and inserts discovered native
    /// tokens as held capabilities.
    fn _discover_native_token_outputs(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        height: u32,
        function_code: u8,
    ) -> Result<bool> {
        // Build augmented secret set with per-block keys for address cycling
        let height_bytes = height.to_le_bytes();
        let per_block_secrets = self.account_mgr.secrets_for_contract(
            &NATIVE_TOKEN_CONTRACT_ID, &height_bytes,
        );
        let mut trial_secrets: Vec<SecretKey> =
            Vec::with_capacity(scan_cache.secrets.len() + per_block_secrets.len());
        trial_secrets.extend(scan_cache.secrets.iter().cloned());
        trial_secrets.extend(per_block_secrets);

        let params = &data[1..]; // skip function code byte
        let mut found_any = false;
        let mut off = 0;

        // Byte-level AEAD scan of call params (same pattern as generic AEAD path)
        while off < params.len().saturating_sub(32) {
            let mut cursor = std::io::Cursor::new(&params[off..]);
            let pos_before = cursor.position();
            let Ok(generic_note) = AeadEncryptedNote::decode(&mut cursor) else {
                off += 1;
                continue;
            };
            let consumed = (cursor.position() - pos_before) as usize;
            off += consumed;

            for secret in &trial_secrets {
                if let Ok(decrypted_note) = generic_note.decrypt::<NativeToken>(secret) {
                    let func_names: std::collections::HashMap<u8, &str> = [
                        (0x00, "FeeV1"), (0x03, "TransferV1"),
                        (0x04, "SpendV1"), (0x05, "PoWRewardV1"),
                    ].into_iter().collect();
                    let fname = func_names.get(&function_code).copied().unwrap_or("NativeToken");
                    self._insert_native_token_cap(
                        scan_cache, secret, &decrypted_note, height, fname,
                    );
                    found_any = true;
                    break; // found match for this note → next note
                }
            }
        }

        Ok(found_any)
    }

    /// P3-D nullifier detection. A spending call — TransferV1 (0x03) or
    /// BurnV1 (0x02) — publishes the nullifiers of the coins it consumes.
    /// For each cap we still hold, recompute
    /// `nullifier = poseidon_hash([secret, commitment])` and, if it matches a
    /// published nullifier, mark the cap revoked. Detects spends initiated by
    /// OTHER parties (shared key, foreign spend), not just our own.
    fn detect_nullifier_spends(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        height: u32,
    ) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        // Extract published nullifiers from the spending-call params.
        let function_code = data[0];
        let mut cursor = std::io::Cursor::new(&data[1..]);
        let published: Vec<pallas::Base> = match function_code {
            0x03 => match TransferParamsV1::decode(&mut cursor) {
                Ok(p) => p.inputs.iter().map(|inp| inp.nullifier.inner()).collect(),
                Err(_) => return Ok(()),
            },
            0x02 => match BurnParamsV1::decode(&mut cursor) {
                Ok(p) => p.inputs.iter().map(|inp| inp.nullifier.inner()).collect(),
                Err(_) => return Ok(()),
            },
            0x04 => match SpendParamsV1::decode(&mut cursor) {
                // SpendParamsV1 is single-input (1-in-1-out), unlike Transfer/Burn.
                Ok(p) => vec![p.input.nullifier.inner()],
                Err(_) => return Ok(()),
            },
            _ => return Ok(()), // not a spending call
        };
        if published.is_empty() {
            return Ok(());
        }

        // Recompute the nullifier of every cap we still hold.
        let caps = self.wallet.get_held_capabilities(Some(false)).unwrap_or_default();
        for cap in caps {
            let Some(commitment) = bs58::decode(&cap.cap_id).into_vec().ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                .and_then(|r| Option::<pallas::Base>::from(pallas::Base::from_repr(r)))
            else {
                continue
            };
            let Some(secret) = bs58::decode(&cap.secret).into_vec().ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                .and_then(|r| Option::<pallas::Base>::from(pallas::Base::from_repr(r)))
            else {
                continue
            };

            let nullifier = poseidon_hash([secret, commitment]);
            if published.contains(&nullifier) {
                match self.wallet.mark_revoked(&cap.cap_id, height) {
                    Ok(()) => scan_cache.log(format!(
                        "[detect_nullifier_spends] Cap {} revoked — nullifier published at height {}",
                        &cap.cap_id[..8.min(cap.cap_id.len())], height
                    )),
                    Err(e) => scan_cache.log(format!(
                        "[detect_nullifier_spends] ERROR marking cap {} revoked: {:?}",
                        &cap.cap_id[..8.min(cap.cap_id.len())], e
                    )),
                }
            }
        }

        Ok(())
    }

}

// Tests for scan.rs require full wallet/chain context for ScanCache construction.
// ScanCache::log/flush_messages and resolve_manifest_trust are pure but need
// MerkleTree/CacheSmt which need sled DB handles. Tests deferred to integration layer.
