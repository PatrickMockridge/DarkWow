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

use std::sync::Arc;

use tracing::info;

use dwow_core::{
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use blake3::Hash as Blake3Hash;
use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN;
use dwow_sdk::crypto::{
    keypair::{Keypair, SecretKey},
    pasta_prelude::PrimeField,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;

use crate::error::RpcError;

/// Linear blockchain miner rewards recipient configuration.
/// Reward value is computed from `dwow_sdk::blockchain::expected_reward(height)`,
/// not configured statically.
#[derive(Debug, Clone)]
pub struct LinearMinerRewardsRecipientConfig {
    /// Validated recipient of mining rewards — always the node's own declared key.
    pub recipient: crate::accounts::MiningRecipient,
}

impl LinearMinerRewardsRecipientConfig {
    /// Build the recipient config from the node's own declared identity.
    /// Decision basis: one miner, one declared key — coinbase always goes to the
    /// node's own key; there is no external/forwarded recipient. Value moves
    /// elsewhere only via a later transfer.
    ///
    /// `height` is used for per-block address cycling:
    /// `derive_instance(NATIVE_TOKEN_CONTRACT_ID, height.to_le_bytes())` produces
    /// a fresh unlinkable recipient per block for privacy-preserving rewards.
    pub fn from_account(
        mgr: &crate::accounts::AccountManager,
        height: u32,
    ) -> std::result::Result<Self, RpcError> {
        let recipient = crate::accounts::MiningRecipient::from_account(mgr, height)
            .map_err(|_| RpcError::MinerMissingAddress)?;
        Ok(Self { recipient })
    }
}

/// Linear blockchain block template for mining
#[derive(Debug, Clone)]
pub struct LinearBlockTemplate {
    /// Previous block hash
    pub previous: [u8; 32],
    /// Block height
    pub height: u64,
    /// Difficulty target
    pub target: u32,
    /// Unix timestamp (seconds) — captured once and reused for mining blob + verification
    pub timestamp: u64,
    /// Coinbase reward value
    pub value: u64,
    /// ZK proof for the coinbase transaction
    pub zk_proof: Vec<u8>,
    /// ZK public inputs: [coin, value_commit.x, value_commit.y, token_commit]
    pub zk_public_inputs: [[u8; 32]; 4],
    /// Coin commitment (poseidon hash of coin attributes)
    pub coin: [u8; 32],
    /// Pedersen value commitment x-coordinate (32 bytes)
    pub value_commit_x: [u8; 32],
    /// Pedersen value commitment y-coordinate (32 bytes)
    pub value_commit_y: [u8; 32],
    /// Poseidon token commitment
    pub token_commit: [u8; 32],
    /// AEAD encrypted note (contains coin blinds, value, token_id for recipient)
    pub encrypted_note: Vec<u8>,
    /// Coin merkle root after including this block's coinbase coin
    pub coin_merkle_root: [u8; 32],
    /// Nullifier root (all spent nullifiers)
    pub nullifier_root: [u8; 32],
    /// Transactions included in this block template (drained from mempool at generation time)
    pub transactions: Vec<dwow_chain::Transaction>,
    /// Merkle root of the transactions (included in mining blob)
    pub merkle_root: Blake3Hash,
    /// Uncle blocks included in this block (competing blocks from previous height)
    pub uncles: Vec<dwow_chain::UncleBlock>,
    /// Merkle root of the uncle block headers (included in mining blob)
    pub uncle_merkle_root: [u8; 32],
    /// Merkle proofs for each uncle (for stateless verification)
    pub uncle_proofs: Vec<dwow_chain::UncleProof>,
}

/// Build a privacy-preserving coinbase transaction for the linear blockchain.
///
/// Uses the Mint_V1 ZK circuit to create:
/// 1. A ZK proof that the coin was correctly minted
/// 2. Pedersen value commitment (hidden value)
/// 3. Poseidon token commitment (hidden token)
/// 4. Poseidon coin commitment (hash of all attributes)
/// 5. AEAD encrypted note containing coin blinds and block signing secret
pub async fn build_linear_coinbase(
    recipient: crate::accounts::MiningRecipient,
    value: u64,
    linear_zk: &LinearPowRewardZk,
    height: u32,
) -> Result<(
    dwow_chain::CoinbaseTransaction,
    [[u8; 32]; 4],
    dwow_chain::ContractCall,  // pow_reward_v1 contract call data
)> {
    use dwow_native_token_contract::client::pow_reward_v1::PoWRewardCallBuilder;
    use dwow_sdk::crypto::pasta_prelude::{Curve, CurveAffine};

    // Cumulative supply state from the single authoritative source.
    // CumulativeSupplyChain owns its own sled tree — no manual key
    // construction, no dual read paths. Genesis (height=1) returns
    // identity state from get_latest() when no entries exist.
    use dwow_sdk::blockchain::expected_cumulative_supply;
    use dwow_sdk::pasta::pallas;
    let expected_cum_supply = expected_cumulative_supply(height);
    let prev_entry = linear_zk.chain_state.supply_chain.get_latest();
    let old_total_supply = prev_entry.total_supply;
    let old_cumulative_commit = prev_entry.value_commit;
    let old_cumulative_blind = prev_entry.blind;

    // Per-block signing key + ephemeral secret. These are EPHEMERAL (per-block,
    // not the owner identity) — random is correct. `build_linear_coinbase` is only
    // called for MINED blocks (height >= 2, via prepare_block / block-template);
    // genesis is built separately by init_genesis with `coinbase: None`, so there
    // is no height==1 special case here (removed — it was unreachable).
    let block_signing_keypair = Keypair::random(&mut OsRng);
    let ephemeral_secret = SecretKey::random(&mut OsRng);

    let debris = PoWRewardCallBuilder {
        secret: block_signing_keypair.secret,
        ephemeral_signature_secret: ephemeral_secret,
        block_height: height,
        fees: 0,
        recipient: Some(recipient.public()),
        spend_hook: None,
        user_data: None,
        expected_cumulative_supply: expected_cum_supply,
        old_total_supply,
        old_cumulative_commit,
        old_cumulative_blind,
        mint_zkbin: (*linear_zk.zkbin).clone(),
        mint_pk: (*linear_zk.provingkey).clone(),
        tx_nonce: pallas::Base::zero(),
        tx_commitment: pallas::Base::zero(),
    }
    .build_with_custom_reward(value)?;

    // Verify: the ZK proof's new_cumulative_commit matches the cumulative
    // supply chain module's computation. This is the single computation point
    // for the Pedersen chain invariant S_H = S_{H-1} + C_H.
    // Mirrors DualTreeSupplyChain.compute_coinbase() in the Python spec.
    use dwow_chain::CumulativeSupplyChain;
    let _computed_next = CumulativeSupplyChain::compute_next(
        &prev_entry,
        debris.params.output.value_commit,
        debris.params.input.value_blind.inner(),
        value,
    );
    debug_assert_eq!(
        _computed_next.value_commit, debris.params.new_cumulative_commit,
        "ZK proof new_cumulative_commit does not match compute_next()"
    );

    let params = &debris.params;
    let output = &params.output;

    let coin_bytes: [u8; 32] = output.coin.inner().to_repr();

    let valcom_coords = output.value_commit.to_affine().coordinates().unwrap();
    let mut value_commit_x = [0u8; 32];
    let mut value_commit_y = [0u8; 32];
    value_commit_x.copy_from_slice(&valcom_coords.x().to_repr());
    value_commit_y.copy_from_slice(&valcom_coords.y().to_repr());

    let token_commit_bytes: [u8; 32] = output.token_commit.to_repr();

    let public_inputs: [[u8; 32]; 4] = [
        coin_bytes,
        value_commit_x,
        value_commit_y,
        token_commit_bytes,
    ];

    let mut proof_bytes = vec![];
    for proof in &debris.proofs {
        proof.encode(&mut proof_bytes)
            .map_err(|e| Error::Custom(format!("Failed to encode ZK proof: {}", e)))?;
    }

    let mut note_bytes = vec![];
    output.note.encode(&mut note_bytes)
        .map_err(|e| Error::Custom(format!("Failed to encode encrypted note: {}", e)))?;

    let coinbase = dwow_chain::CoinbaseTransaction {
        proof: proof_bytes,
        public_inputs,
        coin: coin_bytes,
        value_commit_x,
        value_commit_y,
        token_commit: token_commit_bytes,
        encrypted_note: note_bytes,
    };

    // Build the pow_reward_v1 contract call that triggers WASM execution.
    // This call is added to the coinbase transaction's contract_calls so
    // execute_block() dispatches it to the NativeToken WASM entrypoint.
    // Selector byte 0x05 = NativeTokenFunction::PoWRewardV1.
    let pow_reward_selector: u8 = dwow_native_token_contract::NativeTokenFunction::PoWRewardV1 as u8;
    let mut pow_reward_call_data = vec![pow_reward_selector];
    pow_reward_call_data.extend(dwow_serial::serialize(&debris.params));
    let pow_reward_call = dwow_chain::ContractCall {
        contract_id: dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
        data: pow_reward_call_data,
    };

    Ok((coinbase, public_inputs, pow_reward_call))
}

/// Linear blockchain ZK mining data.
/// Loads the Mint_V1 ZK circuit and proving key for creating privacy-preserving
/// coinbase transactions.
///
/// zkbin and provingkey are Arc-wrapped: the proving key is ~5MB and is cloned
/// every block for coinbase construction. Arc makes clone a ref-count increment
/// instead of a deep copy (structural fix for Clone-amplification pattern).
#[derive(Clone)]
pub struct LinearPowRewardZk {
    pub zkbin: Arc<ZkBinary>,
    pub provingkey: Arc<ProvingKey>,
    pub chain_state: Arc<dwow_chain::CChainState>,
}

impl LinearPowRewardZk {
    pub async fn new(chain_state: Arc<dwow_chain::CChainState>) -> Result<Self> {
        info!(
            target: "dwowd::registry::model::LinearPowRewardZk::new",
            "Initializing linear ZK mining data...",
        );

        let zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode Mint_V1 ZK binary: {}", e)))?;

        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let provingkey = ProvingKey::build(zkbin.k, &circuit)
            .map_err(|e| Error::Custom(format!("ProvingKey::build mint: {:?}", e)))?;

        info!(
            target: "dwowd::registry::model::LinearPowRewardZk::new",
            "Mint_V1 ZK circuit loaded (k={})", zkbin.k,
        );

        Ok(Self {
            zkbin: Arc::new(zkbin),
            provingkey: Arc::new(provingkey),
            chain_state,
        })
    }
}

/// Generate next block template for linear blockchain.
/// When `linear_zk` is provided, creates a privacy-preserving ZK coinbase.
/// Otherwise falls back to a transparent coinbase (for development/testing).
/// `transactions` are drained from the mempool at template generation time
/// so the merkle root (included in the mining blob) remains fixed.
pub async fn generate_linear_block_template(
    chain_state: &dwow_chain::CChainState,
    recipient_config: &LinearMinerRewardsRecipientConfig,
    linear_zk: Option<&LinearPowRewardZk>,
    transactions: Vec<dwow_chain::Transaction>,
    uncles: Vec<dwow_chain::UncleBlock>,
) -> Result<LinearBlockTemplate> {
    // Cap transactions so the merkle root (included in the mining blob)
    // stays within the block gas budget. Each call is assumed to use its
    // full GAS_LIMIT budget (conservative — actual usage may be lower).
    // Remaining txs stay in the mempool for the next block.
    let gas_limit = dwow_core::runtime::vm_runtime::GAS_LIMIT;
    let block_gas_limit = dwow_chain::execution::BLOCK_GAS_LIMIT;
    let transactions: Vec<dwow_chain::Transaction> = {
        let mut capped = Vec::new();
        let mut estimated_gas: u64 = 0;
        for tx in transactions {
            let call_gas = tx.contract_calls.len() as u64 * gas_limit;
            if estimated_gas + call_gas > block_gas_limit {
                break; // stop here; remainder stays in mempool
            }
            estimated_gas += call_gas;
            capped.push(tx);
        }
        capped
    };

    let height = chain_state.get_height() + 1;

    let previous_hash: [u8; 32] = if height == 1 {
        [0u8; 32]
    } else {
        let latest_block = chain_state.get_latest_block()
            .map_err(|e| Error::Custom(format!("Failed to get latest block: {}", e)))?;
        let _prev_key = latest_block.header.randomx_key;
        *chain_state.hash_block_with_cached_vm(&latest_block).as_bytes()
    };

    let target = {
        let consensus = chain_state.consensus.lock().unwrap();
        consensus.target()
    };

    use dwow_sdk::blockchain::expected_reward;
    let reward = expected_reward(height as u32);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Compute transaction merkle root (included in mining blob).
    // Must be deterministic and match verify_merkle_root() in block.rs.
    let merkle_root = {
        let tx_hashes: Vec<Blake3Hash> = transactions.iter().map(|tx| tx.hash()).collect();
        if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if layer.len() % 2 != 0 {
                    layer.push(*layer.last().unwrap());
                }
                layer = layer
                    .chunks(2)
                    .map(|pair| {
                        let mut combined = pair[0].as_bytes().to_vec();
                        combined.extend_from_slice(pair[1].as_bytes());
                        blake3::hash(&combined)
                    })
                    .collect();
            }
            layer[0]
        }
    };

    // Compute uncle merkle root from collected competing blocks.
    // Must be done BEFORE mining — the root is included in the mining blob
    // and covered by the PoW hash. Uses the existing build_uncle_merkle
    // function from dwow_chain which is already tested.
    let (uncle_merkle_root, uncle_proofs) = if uncles.is_empty() {
        ([0u8; 32], Vec::new())
    } else {
        // Use a fresh VM for uncle hash computation (uncles have their
        // own randomx_keys — not the block's key).
        let uncle_vm = {
            let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
            let cache = randomx::RandomXCache::new(flags, &[0u8; 32])
                .map_err(|e| Error::Custom(format!("Uncle VM cache: {}", e)))?;
            randomx::RandomXVM::new(flags, Some(cache), None)
                .map_err(|e| Error::Custom(format!("Uncle VM: {}", e)))?
        };
        let (root, proofs) = dwow_chain::build_uncle_merkle(&uncles, &uncle_vm);
        (root, proofs)
    };

    if let Some(zk) = linear_zk {
        // Diagnostic: log exact recipient public key used for AEAD encryption.
        // Cross-reference with wallet's derived_pk from scan diagnostics.
        let recipient_bytes = recipient_config.recipient.public().to_bytes();
        tracing::info!(
            target: "dwowd::registry",
            "Coinbase encrypt: recipient_pk={} height={} reward={}",
            hex::encode(recipient_bytes), height, reward,
        );
        let (coinbase, public_inputs, _pow_reward_call) = build_linear_coinbase(
            recipient_config.recipient.clone(),
            reward,
            zk,
            height as u32,
        ).await?;

        let coin_merkle_root = chain_state.compute_root_including_coin(&coinbase.coin);
        let nullifier_root = chain_state.compute_nullifier_root();

        return Ok(LinearBlockTemplate {
            previous: previous_hash,
            height,
            target,
            timestamp,
            value: reward,
            zk_proof: coinbase.proof,
            zk_public_inputs: public_inputs,
            coin: coinbase.coin,
            value_commit_x: coinbase.value_commit_x,
            value_commit_y: coinbase.value_commit_y,
            token_commit: coinbase.token_commit,
            encrypted_note: coinbase.encrypted_note,
            coin_merkle_root,
            nullifier_root,
            transactions,
            merkle_root,
            uncles,
            uncle_merkle_root,
            uncle_proofs,
        });
    }

    // Fallback: transparent coinbase (no ZK proof)
    Ok(LinearBlockTemplate {
        previous: previous_hash,
        height,
        target,
        timestamp,
        value: reward,
        zk_proof: vec![],
        zk_public_inputs: [[0u8; 32]; 4],
        coin: [0u8; 32],
        value_commit_x: [0u8; 32],
        value_commit_y: [0u8; 32],
        token_commit: [0u8; 32],
        encrypted_note: vec![],
        coin_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        transactions,
        merkle_root,
        uncles,
        uncle_merkle_root,
        uncle_proofs,
    })
}
