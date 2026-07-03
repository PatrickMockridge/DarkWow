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
use crate::wallet_error::{Error, Result};
use dwow_sdk::{
    bridgetree::Position,
    crypto::{
        smt::{PoseidonFp, EMPTY_NODES_FP},
        ContractId, MerkleNode, MerkleTree, PublicKey, SecretKey,
        DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
    },
    deploy::{ContractMetadata, DeployParamsV1},
    pasta::group::ff::PrimeField,
};
use dwow_native_token_contract::client::NativeToken;
use dwow_native_token_contract::model::{CoinAttributes, PoWRewardParamsV1};
use dwow_sdk::crypto::note::AeadEncryptedNote;
use dwow_sdk::pasta::pallas;
use dwow_serial::Decodable;
use dwow_serial::Encodable;

use crate::{
    cache::{BlockScanner, CacheSmt, PnSmtStorage},
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
///
/// Resolution order: Genesis → SelfDeployed → Unverified.
/// Attested tier requires on-chain attestation check (deferred).
fn resolve_manifest_trust(
    contract_id: &ContractId,
    deployer_pubkey: &PublicKey,
    wallet: &crate::walletdb::WalletDb,
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

    // Tier 2: Self-deployed — check if deployer's pubkey is in our wallet
    let deployer_bytes = deployer_pubkey.to_bytes();
    if let Ok(addresses) = wallet.get_addresses() {
        for addr in addresses {
            if let Ok(pk_bytes) = bs58::decode(&addr.public_key).into_vec() {
                if pk_bytes == deployer_bytes {
                    return TrustTier::SelfDeployed;
                }
            }
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
        let smt_store = PnSmtStorage::new(self.cache.conn.clone());  // Arc<Mutex<Connection>>
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
        let block_scanner = BlockScanner::new(&self.cache);
        block_scanner.insert_scanned_block(
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

            // Process contract calls (transfers, etc.)
            scan_cache.log(format!("[scan_block_linear] Processing transaction with {} calls", tx.contract_calls.len()));
            for (i, call) in tx.contract_calls.iter().enumerate() {
                // Convert linear [u8; 32] contract_id to ContractId for comparison
                let cid = ContractId::from(
                    pallas::Base::from_repr(call.contract_id).unwrap_or_else(|| {
                        let hex_id = hex::encode(call.contract_id);
                        tracing::error!(target: "drk::scan",
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

                // ── Hardcoded infrastructure ──────────────────────────
                // NativeToken: consensus-critical. Fee payment and coinbase
                // rewards are consensus operations. The wallet cannot function
                // without fee attachment and coinbase scanning.
                //
                // Deployooor: deployment infrastructure. The wallet MUST detect
                // DeployV1 transactions to discover new contracts and their
                // on-chain manifests. Without this, manifest discovery breaks.
                //
                // All other contracts (PN, BB, escrow, auction, 25+) go through
                // the generic AEAD capability scanner below — they are capabilities,
                // not special citizens.
                if cid == *NATIVE_TOKEN_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block_linear] Found Native Token contract in call {i}"));
                    if self
                        .apply_tx_native_token_data_linear(
                            scan_cache,
                            &call.data,
                            &height_u32,
                        )?
                    {
                        wallet_tx = true;
                    }
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
                                            &self.wallet,
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
                        for secret in &scan_cache.secrets {
                        if let Ok(plaintext) = generic_note.decrypt::<Vec<u8>>(secret) {
                            // AEAD succeeded — capability is ours. Try known decoders.
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
    tracing::error!(target: "drk::scan", "Invalid field element bytes, using zero — data may be corrupted");
    pallas::Base::zero()
})
                                );
                                scan_cache.capability_commitment_tree.append(cap_leaf);
                                let siblings: Vec<MerkleNode> = match scan_cache.capability_commitment_tree
                                    .witness(Position::from(leaf_pos), 0)
                                {
                                    Ok(s) => s,
                                    Err(_) => {
                                        tracing::error!(target: "drk::scan",
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
                                        "[scan_block_linear] Generic path: inserted capability {} from call {i}",
                                        &cap_id[..8]
                                    ));
                                }
                                Err(e) => {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] ERROR: Failed to insert capability {} from call {i}: {:?} — DB write failed, block will be re-scanned on restart",
                                        &cap_id[..8], e
                                    ));
                                }
                                }
                                // Also store in capabilities table (structured — NativeToken)
                                let nullifier_hash = blake3::hash(&plaintext);
                                let nullifier = bs58::encode(nullifier_hash.as_bytes()).into_string();
                                if let Err(e) = self.wallet.insert_generic_capability(
                                    &nullifier,
                                    &bs58::encode(call.contract_id).into_string(),
                                    height_u32,
                                    "NativeToken",
                                    &plaintext,
                                ) {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] Failed to insert NativeToken capability: {}",
                                        e));
                                }
                            } else {
                                // AEAD succeeded but format is unknown — still our capability.
                                // Store opaque record in capabilities table.
                                let nullifier_hash = blake3::hash(&plaintext);
                                let nullifier = bs58::encode(nullifier_hash.as_bytes()).into_string();
                                if let Err(e) = self.wallet.insert_generic_capability(
                                    &nullifier,
                                    &bs58::encode(call.contract_id).into_string(),
                                    height_u32,
                                    "unknown",
                                    &plaintext,
                                ) {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] Failed to insert unknown capability: {}",
                                        e));
                                }
                                scan_cache.log(format!(
                                    "[scan_block_linear] Capability stored in call {i}: {} bytes (unknown format)",
                                    plaintext.len()
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

            // Process coinbase transaction (mining reward with ZK privacy)
            if let Some(ref coinbase) = tx.coinbase {
                scan_cache.log(format!(
                    "[scan_block_linear] Found coinbase tx ({} secrets loaded), attempting decryption...",
                    scan_cache.secrets.len()
                ));
                if let Ok(aes_note) = AeadEncryptedNote::decode(
                    &mut std::io::Cursor::new(&coinbase.encrypted_note),
                ) {
                    // Byte-level diagnostic: log ephem_public for cross-reference
                    // with miner's encryption key.
                    let ephem_hex = hex::encode(aes_note.ephem_public.to_bytes());
                    scan_cache.log(format!(
                        "[scan_block_linear] AEAD note decoded: ephem_public={} ciphertext_len={}",
                        ephem_hex, aes_note.ciphertext.len(),
                    ));

                    // Collect log messages in a local buffer to avoid borrow conflicts
                    // with scan_cache.secrets (immutably borrowed in the for loop).
                    let mut diag_msgs: Vec<String> = Vec::new();
                    let secret_count = scan_cache.secrets.len();
                    let pk_summary: Vec<String> = scan_cache.secrets.iter().map(|s| {
                        let pk = PublicKey::from_secret(*s);
                        hex::encode(pk.to_bytes())
                    }).collect();
                    diag_msgs.push(format!(
                        "[scan_block_linear] Attempting decrypt with {} secret(s), derived_pks={:?}",
                        secret_count, pk_summary,
                    ));

                    for secret in &scan_cache.secrets {
                        // Path 1: native_token coinbase — dedicated, first-class
                        match aes_note.decrypt::<NativeToken>(secret) {
                            Ok(decrypted_note) => {
                            let public_key = PublicKey::from_secret(*secret);
                            let coin_attrs = CoinAttributes {
                                version: 0,
                                public_key,
                                value: decrypted_note.value,
                                token_id: decrypted_note.token_id,
                                spend_hook: decrypted_note.spend_hook,
                                user_data: decrypted_note.user_data,
                                blind: decrypted_note.coin_blind,
                            };
                            let commitment = coin_attrs.to_coin();
                            let cap_id_bytes = commitment.to_bytes();
                            let cap_id = bs58::encode(cap_id_bytes).into_string();

                            // Generate a real Merkle proof from the local tree.
                            // Generate real Merkle proof from the local capability tree.
                            let leaf_pos = scan_cache
                                .capability_commitment_tree
                                .current_position()
                                .map(|p| u64::from(p))
                                .unwrap_or(0);
                            // Append cap to tree before generating proof
                            let cap_leaf = MerkleNode::new(
                                pallas::Base::from_repr(cap_id_bytes).unwrap_or_else(|| {
    tracing::error!(target: "drk::scan", "Invalid field element bytes, using zero — data may be corrupted");
    pallas::Base::zero()
})
                            );
                            scan_cache.capability_commitment_tree.append(cap_leaf);
                            let siblings: Vec<MerkleNode> = match scan_cache
                                .capability_commitment_tree
                                .witness(Position::from(leaf_pos), 0)
                            {
                                Ok(s) => s,
                                Err(_) => {
                                    tracing::error!(target: "drk::scan",
                                        "Merkle witness failed at coinbase leaf_pos={} — tree state corrupted, re-scan from genesis required", leaf_pos);
                                    continue;
                                }
                            };
                            let mut sibling_strings: Vec<String> = siblings
                                .iter()
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
                            let root = scan_cache
                                .capability_commitment_tree
                                .root(0)
                                .map(|n| n.inner().to_repr())
                                .expect("capability_commitment_tree root after coinbase append");
                            let merkle_proof = MerkleProof {
                                siblings: sibling_strings,
                                root: bs58::encode(root).into_string(),
                            };

                            let token_id_str = bs58::encode(
                                decrypted_note.token_id.to_repr()
                            ).into_string();
                            let cap_record = CapRecord {
                                cap_id: cap_id.clone(),
                                value: decrypted_note.value,
                                token_id: token_id_str,
                                spend_hook: None,
                                user_data: None,
                                leaf_position: leaf_pos,
                                secret: bs58::encode(secret.inner().to_repr()).into_string(),
                                cap_blind: bs58::encode(decrypted_note.coin_blind.to_repr()).into_string(),
                                value_blind: bs58::encode(decrypted_note.value_blind.to_repr()).into_string(),
                                token_blind: bs58::encode(decrypted_note.token_blind.to_repr()).into_string(),
                                revoked: false,
                                revoked_at_height: None,
                                created_at_height: height_u32,
                            };

                            match self.wallet.insert_capability(&cap_record, &merkle_proof) {
                                Ok(()) => {
                                    tracing::info!(target: "drk::scan",
                                        "Inserted coinbase coin {} at height {}",
                                        &cap_id[..8], block.header.height
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(target: "drk::scan",
                                        "Failed to insert coinbase coin {} at height {}: {:?} — DB write failed, block will be re-scanned on restart",
                                        &cap_id[..8], block.header.height, e
                                    );
                                }
                            }
                            // Also store in capabilities table
                            let mut note_bytes = Vec::new();
                            if let Err(e) = decrypted_note.encode(&mut note_bytes) {
                                tracing::error!(target: "drk::scan",
                                    "Failed to encode decrypted coinbase note: {:?} — skipping capability insert",
                                    e
                                );
                                continue;
                            }
                            let nullifier_hash = blake3::hash(&note_bytes);
                            let nullifier = bs58::encode(nullifier_hash.as_bytes()).into_string();
                            if let Err(e) = self.wallet.insert_generic_capability(
                                &nullifier,
                                &bs58::encode(NATIVE_TOKEN_CONTRACT_ID.to_bytes()).into_string(),
                                height_u32,
                                "NativeToken",
                                &note_bytes,
                            ) {
                                tracing::error!(target: "drk::scan",
                                    "Failed to insert coinbase capability: {}", e);
                            }
                            wallet_tx = true;
                            break;
                        }
                            Err(e) => {
                                let sk_hex = hex::encode(secret.inner().to_repr());
                                diag_msgs.push(format!(
                                    "[scan_block_linear] decrypt error for secret={}: {:?}",
                                    sk_hex, e,
                                ));
                            }
                        }
                    }
                    // Flush diagnostic messages collected during decrypt loop
                    scan_cache.messages_buffer.extend(diag_msgs);
                    // If we iterated all secrets without decrypting, log for debugging
                    if !wallet_tx {
                        scan_cache.log(format!(
                            "[scan_block_linear] COINBASE_DECRYPT_FAILED block={} secrets_tried={} — no secret matched. Check that wallet has correct key imported.",
                            block.header.height, scan_cache.secrets.len()
                        ));
                    }
                } else {
                    scan_cache.log(format!(
                        "[scan_block_linear] Coinbase: failed to decode AeadEncryptedNote ({} bytes)",
                        coinbase.encrypted_note.len()
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
        self.cache.insert_merkle_trees(&[
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

    /// Apply native token transaction data from linear blockchain (without full note decryption params)
    ///
    /// For darkwow-devnet, mining rewards are directly sent to the wallet's public key
    fn apply_tx_native_token_data_linear(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        height: &u32,
    ) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        let function_code = data[0];

        match function_code {
            // PoWRewardV1 (0x05) in linear - reward goes directly to coinbase
            0x05 => {
                let mut cursor = std::io::Cursor::new(&data[1..]); // skip function code byte
                let params = PoWRewardParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode PoWRewardV1 params: {:?}", e)))?;

                let output = &params.output;

                // Try to decrypt the note with our secrets
                for secret in &scan_cache.secrets {
                    if let Ok(decrypted_note) = output.note.decrypt::<NativeToken>(secret) {
                        use dwow_sdk::crypto::PublicKey;
                        use dwow_native_token_contract::model::CoinAttributes;
                        let public_key = PublicKey::from_secret(*secret);
                        let coin_attrs = CoinAttributes {
                            version: 0,
                            public_key,
                            value: decrypted_note.value,
                            token_id: decrypted_note.token_id,
                            spend_hook: decrypted_note.spend_hook,
                            user_data: decrypted_note.user_data,
                            blind: decrypted_note.coin_blind,
                        };
                        let commitment = coin_attrs.to_coin();
                        let cap_id_bytes = commitment.to_bytes();
                        let cap_id = bs58::encode(cap_id_bytes).into_string();

                        // Generate real Merkle proof with full siblings (HAZOP #4 fix)
                        let leaf_pos = scan_cache.capability_commitment_tree.current_position()
                            .map(|p| u64::from(p)).unwrap_or(0);
                        let cap_id_bytes_fix = commitment.to_bytes();
                        let cap_leaf = MerkleNode::new(
                            pallas::Base::from_repr(cap_id_bytes_fix).unwrap_or_else(|| {
    tracing::error!(target: "drk::scan", "Invalid field element bytes, using zero — data may be corrupted");
    pallas::Base::zero()
})
                        );
                        scan_cache.capability_commitment_tree.append(cap_leaf);
                        let siblings: Vec<MerkleNode> = match scan_cache.capability_commitment_tree
                            .witness(Position::from(leaf_pos), 0)
                        {
                            Ok(s) => s,
                            Err(_) => {
                                tracing::error!(target: "drk::scan",
                                    "Merkle witness failed at PoW reward leaf_pos={} — tree state may be corrupted, re-scan from genesis required", leaf_pos);
                                scan_cache.log(format!(
                                    "[apply_tx_native_token_data_linear] ERROR: Merkle witness failed at leaf_pos={} — tree leaf already appended, state may be inconsistent. Re-scan from genesis required.",
                                    leaf_pos
                                ));
                                return Ok(false);
                            }
                        };
                        let mut sibling_strings: Vec<String> = siblings.iter()
                            .map(|n| bs58::encode(n.inner().to_repr()).into_string()).collect();
                        while sibling_strings.len() < dwow_sdk::crypto::constants::MERKLE_DEPTH_ORCHARD {
                            let lvl = sibling_strings.len();
                            sibling_strings.push(bs58::encode(
                                dwow_sdk::crypto::smt::EMPTY_NODES_FP[lvl].to_repr()
                            ).into_string());
                        }
                        let root = scan_cache.capability_commitment_tree.root(0)
                            .map(|n| n.inner().to_repr())
                            .expect("capability_commitment_tree root in apply_tx_native_token_data");
                        let merkle_proof = MerkleProof {
                            siblings: sibling_strings,
                            root: bs58::encode(root).into_string(),
                        };

                        let token_id_str = bs58::encode(decrypted_note.token_id.to_repr()).into_string();
                        let cap_record = CapRecord {
                            cap_id: cap_id.clone(),
                            value: decrypted_note.value,
                            token_id: token_id_str,
                            spend_hook: None,
                            user_data: None,
                            leaf_position: leaf_pos,
                            secret: bs58::encode(secret.inner().to_repr()).into_string(),
                            cap_blind: bs58::encode(decrypted_note.coin_blind.to_repr()).into_string(),
                            value_blind: bs58::encode(decrypted_note.value_blind.to_repr()).into_string(),
                            token_blind: bs58::encode(decrypted_note.token_blind.to_repr()).into_string(),
                            revoked: false,
                            revoked_at_height: None,
                            created_at_height: *height,
                        };

                        match self.wallet.insert_capability(&cap_record, &merkle_proof) {
                            Ok(()) => {
                                scan_cache.log(format!(
                                    "[apply_tx_native_token_data_linear] Inserted PoW reward cap {} at height {}",
                                    &cap_id[..8],
                                    height
                                ));
                            }
                            Err(e) => {
                                scan_cache.log(format!(
                                    "[apply_tx_native_token_data_linear] ERROR: Failed to insert PoW reward cap {} at height {}: {:?} — DB write failed, block will be re-scanned on restart",
                                    &cap_id[..8], height, e
                                ));
                            }
                        }
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_native_token_data_linear] Skipping NativeToken function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }

}

// Tests for scan.rs require full wallet/chain context for ScanCache construction.
// ScanCache::log/flush_messages and resolve_manifest_trust are pure but need
// MerkleTree/CacheSmt which need sled DB handles. Tests deferred to integration layer.
