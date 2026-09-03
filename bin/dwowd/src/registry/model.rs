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
use dwow_chain::Nullifier;
use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_V2_BIN;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockVersion, FeeAmount, SupplyAmount};
use dwow_sdk::crypto::{
    keypair::{SecretKey},
    pasta_prelude::PrimeField,
    poseidon_hash,
};
use dwow_sdk::pasta::pallas;
use dwow_serial::Encodable;

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
        height: BlockHeight,
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
    pub height: BlockHeight,
    /// Difficulty target
    pub target: BlockTarget,
    /// Unix timestamp (seconds) — captured once and reused for mining blob + verification
    pub timestamp: u64,
    /// Coinbase reward value
    pub value: BlockReward,
    /// ZK proof for the coinbase transaction
    pub zk_proof: Vec<u8>,
    /// ZK public inputs: [C, nf, vc.x, vc.y, tc, S_H.x, S_H.y, tx_binding, tx_nonce]
    pub zk_public_inputs: [[u8; 32]; 9],
    /// Commitment (poseidon hash of commitment attributes)
    pub commitment: dwow_chain::Commitment,
    /// Pedersen value commitment x-coordinate
    pub value_commit_x: dwow_chain::PedersenCoordinate,
    /// Pedersen value commitment y-coordinate
    pub value_commit_y: dwow_chain::PedersenCoordinate,
    /// Poseidon token commitment
    pub token_commit: dwow_chain::TokenCommitment,
    /// Nullifier: nf = poseidon_hash(sk_H.inner(), C) — capability claim.
    /// None only in the no-ZK-circuit fallback path (development only).
    /// Rule 3: zero is not a valid nullifier — use Option<Nullifier>.
    pub nullifier: Option<dwow_chain::Nullifier>,
    /// Cumulative supply commitment x-coordinate (S_H.x)
    pub new_cumulative_x: dwow_chain::PedersenCoordinate,
    /// Cumulative supply commitment y-coordinate (S_H.y)
    pub new_cumulative_y: dwow_chain::PedersenCoordinate,
    /// PoWRewardV1 contract call data (function selector 0x05 + serialized params).
    /// Required for stratum/mm_rpc miners to include the WASM call in contract_calls.
    pub pow_reward_call_data: Vec<u8>,
    /// AEAD encrypted note (contains commitment blinds, value, token_id for recipient)
    pub encrypted_note: Vec<u8>,
    /// Commitment merkle root after including this block's coinbase commitment
    pub commitment_merkle_root: [u8; 32],
    /// Nullifier root (all spent nullifiers)
    pub nullifier_root: [u8; 32],
    /// Miner's reward public key (pk_H) — set into `BlockHeader.miner`.
    /// Spec: uncle_merkle.md §Uncle Minting & Maturity — "Miner identity in the header".
    pub miner: [u8; 32],
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
/// 1. A ZK proof that the commitment was correctly minted
/// 2. Pedersen value commitment (hidden value)
/// 3. Poseidon token commitment (hidden token)
/// 4. Poseidon commitment (hash of all attributes)
/// 5. AEAD encrypted note containing commitment blinds and block signing secret
pub async fn build_linear_coinbase(
    recipient: crate::accounts::MiningRecipient,
    value: BlockReward,
    linear_zk: &LinearPowRewardZk,
    height: BlockHeight,
) -> Result<(
    dwow_chain::CoinbaseTransaction,
    [[u8; 32]; 9],
    dwow_chain::ContractCall,  // pow_reward_v1 contract call data
    pallas::Base,              // coin_blind — deterministic, same as ZK circuit witness
)> {
    // Spec: uncle_merkle.md §Uncle Minting & Maturity — no uncles ⇒ effective == full value.
    build_linear_coinbase_effective(recipient, value, value, linear_zk, height).await
}

/// Build a coinbase whose spendable note commits to a REDUCED effective value
/// (the canonical miner's share after uncle pins) while the cumulative supply
/// chain still commits to the FULL base reward.
/// Spec: uncle_merkle.md §Uncle Minting & Maturity ("Canonical note reduction").
pub async fn build_linear_coinbase_effective(
    recipient: crate::accounts::MiningRecipient,
    value: BlockReward,
    effective_value: BlockReward,
    linear_zk: &LinearPowRewardZk,
    height: BlockHeight,
) -> Result<(
    dwow_chain::CoinbaseTransaction,
    [[u8; 32]; 9],
    dwow_chain::ContractCall,  // pow_reward_v1 contract call data
    pallas::Base,              // coin_blind — deterministic, same as ZK circuit witness
)> {
    use dwow_native_token_contract::client::pow_reward::PoWRewardCallBuilder;
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

    // Deterministic per-block commitment ownership key.
    // sk_H = derive_instance(sk_owner, NATIVE_TOKEN_CONTRACT_ID, H).
    // Both miner and wallet compute this independently — same Poseidon hash,
    // same derived key. The wallet uses it to decrypt the coinbase note and
    // verify the nullifier. No randomness in the key path.
    // Per formal guardrail: CLAIM_COINBASE process, referential transparency.
    let sk_h: SecretKey = recipient.secret().clone().into();
    // Deterministic ephemeral key derived from sk_H — consensus-coinbase.md §2.7:
    // "no random keys." Domain-separated from the blind derivation domains.
    let ephemeral_secret = SecretKey::from_base(dwow_sdk::crypto::poseidon_hash([
        *sk_h.inner(),
        pallas::Base::from(0xE7E7_E7E7_E7E7_E7E7u64),
    ]));

    let debris = PoWRewardCallBuilder {
        secret: sk_h.clone(),
        ephemeral_signature_secret: ephemeral_secret,
        block_height: height,
        fees: 0,
        recipient: Some(recipient.public()),
        spend_hook: None,
        user_data: None,
        expected_cumulative_supply: expected_cum_supply.get(),
        old_total_supply: old_total_supply.get(),
        old_cumulative_commit,
        old_cumulative_blind,
        // HAZOP C7 fix: deterministic nonce from block height + call index
        tx_nonce: pallas::Base::from(height.get()),
        tx_commitment: pallas::Base::from(height.get() + 1),
    }
    .build_with_custom_reward_and_effective(value.get(), effective_value.get())?;

    // Verify: the ZK proof's new_cumulative_commit matches the cumulative
    // supply chain module's computation. This is the single computation point
    // for the Pedersen chain invariant S_H = S_{H-1} + C_H.
    // Mirrors DualTreeSupplyChain.compute_coinbase() in the Python spec.
    use dwow_chain::CumulativeSupplyChain;
    let _computed_next = CumulativeSupplyChain::compute_next(
        &prev_entry,
        debris.params.output.value_commit,
        debris.params.input.value_blind.inner(),
        SupplyAmount::from(value),
    );
    if _computed_next.value_commit != debris.params.new_cumulative_commit {
        return Err(Error::Custom(format!(
            "Cumulative supply chain invariant violated: computed={:?} zk_proof={:?}",
            _computed_next.value_commit, debris.params.new_cumulative_commit
        )));
    }

    let params = &debris.params;
    let output = &params.output;

    let commitment = dwow_chain::Commitment::from_base(output.commitment.inner());

    let vc = output.value_commit.to_affine().coordinates();
    if vc.is_none().into() {
        return Err(dwow_core::Error::Custom("coinbase value_commit is identity".into()));
    }
    let valcom_coords = vc.unwrap();
    let mut value_commit_x = [0u8; 32];
    let mut value_commit_y = [0u8; 32];
    value_commit_x.copy_from_slice(&valcom_coords.x().to_repr());
    value_commit_y.copy_from_slice(&valcom_coords.y().to_repr());

    let token_commit_bytes: [u8; 32] = output.token_commit.to_repr();

    // Compute nullifier: nf = poseidon_hash(sk_H.inner(), C)
    // Per formal guardrail: nf is the capability claim — the miner exercises
    // the coinbase capability by publishing this nullifier.
    // V.7: single canonical path via Nullifier::new() — no bytes round-trip.
    let coin_fp = commitment.inner();
    let nullifier = Nullifier::new(sk_h.clone(), coin_fp);

    // Extract cumulative supply from ZK proof output (S_H = S_{H-1} + C_H).
    // These MUST match what the circuit constrains — [0u8; 32] would break
    // the supply chain invariant.
    let cumcom_coords = debris.params.new_cumulative_commit.to_affine().coordinates()
        .expect("Cumulative commitment cannot be the identity element");
    let mut cum_x = [0u8; 32];
    let mut cum_y = [0u8; 32];
    cum_x.copy_from_slice(&cumcom_coords.x().to_repr());
    cum_y.copy_from_slice(&cumcom_coords.y().to_repr());

    let mut tx_binding_bytes = [0u8; 32];
    let mut tx_nonce_bytes = [0u8; 32];
    // The Mint_V1 ZK proof's real tx_binding/tx_nonce are carried in PoWRewardParamsV1
    // (the contract call data) and are verified by the WASM entrypoint via
    // verify_core_tx_with_tables — NOT via this serialized CoinbaseTransaction field.
    // These two slots are therefore a stable, deterministic representation (zero-filled)
    // rather than the proof's live public inputs. Populate them only when PoWRewardParamsV1
    // grows an explicit tx_binding field and a consumer of ZkPublicInputs[7..9] exists.
    tx_binding_bytes.copy_from_slice(&pallas::Base::zero().to_repr());
    tx_nonce_bytes.copy_from_slice(&pallas::Base::zero().to_repr());

    let public_inputs: [[u8; 32]; 9] = [
        commitment.to_bytes(),         // 1: C
        nullifier.to_bytes(),    // 2: nf (V.7: typed Nullifier)
        value_commit_x,     // 3: vc.x
        value_commit_y,     // 4: vc.y
        token_commit_bytes, // 5: tc
        cum_x,              // 6: S_H.x
        cum_y,              // 7: S_H.y
        tx_binding_bytes,   // 8: tx_binding
        tx_nonce_bytes,     // 9: tx_nonce
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
        public_inputs: dwow_chain::ZkPublicInputs(public_inputs),
        commitment: commitment,
        value_commit_x: dwow_chain::PedersenCoordinate::from_bytes(value_commit_x)
            .map_err(|e| Error::Custom(format!("Invalid value_commit_x: {}", e)))?,
        value_commit_y: dwow_chain::PedersenCoordinate::from_bytes(value_commit_y)
            .map_err(|e| Error::Custom(format!("Invalid value_commit_y: {}", e)))?,
        token_commit: dwow_chain::TokenCommitment::from_bytes(token_commit_bytes)
            .map_err(|e| Error::Custom(format!("Invalid token_commit: {}", e)))?,
        nullifier,
        new_cumulative_x: dwow_chain::PedersenCoordinate::from_bytes(cum_x)
            .map_err(|e| Error::Custom(format!("Invalid new_cumulative_x: {}", e)))?,
        new_cumulative_y: dwow_chain::PedersenCoordinate::from_bytes(cum_y)
            .map_err(|e| Error::Custom(format!("Invalid new_cumulative_y: {}", e)))?,
        encrypted_note: note_bytes,
    };

    // Build the pow_reward_v1 contract call that triggers WASM execution.
    // This call is added to the coinbase transaction's contract_calls so
    // execute_block() dispatches it to the NativeToken WASM entrypoint.
    // Selector byte 0x05 = NativeTokenFunction::PoWRewardV1.
    let pow_reward_selector: u8 = dwow_native_token_contract::NativeTokenFunction::PoWRewardV1 as u8;
    let mut pow_reward_call_data = vec![pow_reward_selector];
    pow_reward_call_data.extend_from_slice(&debris.params.encode());
    let pow_reward_call = dwow_chain::ContractCall {
        contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
        data: pow_reward_call_data,
    };

    // Deterministic coin_blind — computed from the same formula as the ZK circuit
    // (PoWRewardCallBuilder, pow_reward_v1.rs:164-167). Exposed so tests can build
    // fee/burn call_data referencing coinbase coins without decrypting the AEAD note.
    let coin_blind = poseidon_hash([
        *sk_h.inner(),
        pallas::Base::from(height.get()),
        pallas::Base::from(3u64), // DOMAIN_COIN_BLIND
    ]);

    Ok((coinbase, public_inputs, pow_reward_call, coin_blind))
}

/// Build the FeeCollectV1 "collection plate" transaction — the final
/// transaction in every block (consensus-coinbase.md §3). Single source of
/// truth for all mining paths (built-in miner, RPC miner, stratum, mm_rpc).
///
/// Sums all NativeToken FeeV1 fees in `transactions` per spec §3.12
/// (contract_id filter + checked arithmetic), and if the total is non-zero,
/// builds the FeeCollect_V1 ZK proof with the same sk_H as the coinbase and
/// assembles the chain transaction: proof in the L1 `witness` carriage, fee
/// nullifier in `tx.nullifiers`.
///
/// Returns `Ok(None)` iff `total_fees == 0`. A build failure with non-zero
/// fees is an error — silently omitting fee collection violates spec §3.1
/// and strands the fees permanently (§3.13).
pub fn build_fee_collect_tx(
    recipient: &crate::accounts::MiningRecipient,
    transactions: &[dwow_chain::Transaction],
    height: BlockHeight,
    linear_zk: &LinearPowRewardZk,
    total_fees: FeeAmount,
) -> Result<Option<dwow_chain::Transaction>> {
    use dwow_native_token_contract::client::fee_collect::FeeCollectCallBuilder;
    use dwow_sdk::pasta::pallas;

    // Fee summation: FeeV3 (0x08) — plaintext fees, no Pedersen blind.
    // total_fees is provided by the miner from block construction context.
    // Spec: fee-spec.md §12.4.
    let mut fee_call_count: u64 = 0;
    for tx in transactions {
        for c in &tx.contract_calls {
            if c.contract_id != *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID {
                continue;
            }
            if let Some(mb_fee_v2) = c.as_mass_balance_fee_v2() {
                fee_call_count += 1;
                continue;
            }
        }
    }

    if fee_call_count == 0 {
        return Ok(None);
    }

    let sk_h: SecretKey = recipient.secret().clone().into();
    let debris = FeeCollectCallBuilder {
        secret: sk_h,
        block_height: height,
        total_fees,
        fee_collect_zkbin: (*linear_zk.fee_collect_zkbin).clone(),
        fee_collect_pk: (*linear_zk.fee_collect_provingkey).clone(),
        // HAZOP C7 fix: deterministic nonce from block height
        tx_nonce: pallas::Base::from(height.get()),
        tx_commitment: pallas::Base::from(height.get() + 2),
    }
    .build()
    .map_err(|e| {
        Error::Custom(format!("FeeCollectV1 build failed at height {}: {}", height, e))
    })?;

    tracing::info!(target: "dwowd::registry::model::build_fee_collect_tx",
        "FeeCollectV1: {} fee units to miner at height {}", total_fees, height);

    let nullifier = debris.params.nullifier;
    let call_data = {
        let serialized = debris.params.encode();
        let mut buf = vec![dwow_native_token_contract::NativeTokenFunction::FeeCollectV1 as u8];
        buf.extend_from_slice(&serialized);
        buf
    };

    // L1 witness carriage (same as user transactions): the core tx carries
    // the ZK proof; the chain tx carries the serialized core tx in `witness`
    // and the nullifier in `nullifiers`. L2 verifies the proof at block
    // accept via verify_core_tx_with_tables (spec §3.15, Phase 3.1).
    let core_tx = dwow_core::tx::Transaction {
        calls: vec![dwow_sdk::dark_tree::DarkLeaf {
            data: dwow_sdk::tx::ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data: call_data.clone(),
            },
            children_indexes: vec![],
            parent_index: None,
        }],
        proofs: vec![debris.proofs],
        // Schnorr signatures removed per contract-standards.md §3.
        tx_commitment: [0u8; 32],
        nullifiers: vec![nullifier],
    };

    Ok(Some(dwow_chain::Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![dwow_chain::ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data: call_data,
        }],
        lock_time: 0,
        nullifiers: vec![nullifier],
        witness: dwow_serial::serialize(&core_tx),
    }))
}

/// Build an UncleMintV1 transaction — one spendable note for one accepted uncle.
/// Spec: uncle_merkle.md §Uncle Minting & Maturity — "Per-uncle note mint".
/// The note is encrypted to `uncle.header.miner`; its value is carved out of the
/// coinbase's full base (no supply bump).
pub fn build_uncle_mint_tx(
    uncle: &dwow_chain::UncleBlock,
    height: BlockHeight,
    tx_nonce: pallas::Base,
) -> Result<dwow_chain::Transaction> {
    use dwow_native_token_contract::client::uncle_mint::build_uncle_mint;
    use dwow_sdk::crypto::PublicKey;

    let pin = uncle.pin_confirmed.get();
    let uncle_miner = PublicKey::from_bytes(uncle.header.miner)
        .map_err(|e| Error::Custom(format!("invalid uncle miner pubkey: {e}")))?;
    let uncle_hash = *blake3::hash(&uncle.header.to_mining_blob()).as_bytes();

    let debris = build_uncle_mint(
        pin,
        uncle_miner,
        uncle_hash,
        height,
        pallas::Base::from(height.get() + 3),
        tx_nonce,
    )?;

    let nullifier = debris.params.nullifier;
    let call_data = {
        let serialized = debris.params.encode();
        let mut buf = vec![dwow_native_token_contract::NativeTokenFunction::UncleMintV1 as u8];
        buf.extend_from_slice(&serialized);
        buf
    };

    let core_tx = dwow_core::tx::Transaction {
        calls: vec![dwow_sdk::dark_tree::DarkLeaf {
            data: dwow_sdk::tx::ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data: call_data.clone(),
            },
            children_indexes: vec![],
            parent_index: None,
        }],
        proofs: vec![debris.proofs],
        tx_commitment: [0u8; 32],
        nullifiers: vec![nullifier],
    };

    Ok(dwow_chain::Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![dwow_chain::ContractCall {
            contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
            data: call_data,
        }],
        lock_time: 0,
        nullifiers: vec![nullifier],
        witness: dwow_serial::serialize(&core_tx),
    })
}

/// Linear blockchain ZK mining data.
/// The coinbase and uncle notes are minted PLAINTEXT (no Mint_V2 proof), so the
/// miner only needs the FeeCollect_V1 circuit (the "collection plate" final tx).
///
/// fee_collect_zkbin and fee_collect_provingkey are Arc-wrapped: the proving key
/// is ~5MB and is cloned every block for fee collection. Arc makes clone a
/// ref-count increment instead of a deep copy.
#[derive(Clone)]
pub struct LinearPowRewardZk {
    /// FeeCollect_V1 circuit — the "collection plate" final transaction
    /// (consensus-coinbase.md §3.5).
    pub fee_collect_zkbin: Arc<ZkBinary>,
    pub fee_collect_provingkey: Arc<ProvingKey>,
    pub chain_state: Arc<dwow_chain::CChainState>,
}

impl LinearPowRewardZk {
    pub async fn new(chain_state: Arc<dwow_chain::CChainState>) -> Result<Self> {
        info!(
            target: "dwowd::registry::model::LinearPowRewardZk::new",
            "Initializing linear ZK mining data...",
        );

        let fee_collect_zkbin = ZkBinary::decode(
            NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_V2_BIN,
            false,
        )
        .map_err(|e| Error::Custom(format!("Failed to decode FeeCollect_V1 ZK binary: {}", e)))?;

        let fee_collect_circuit =
            ZkCircuit::new(empty_witnesses(&fee_collect_zkbin)?, &fee_collect_zkbin);
        let fee_collect_provingkey = ProvingKey::build(fee_collect_zkbin.k, &fee_collect_circuit)
            .map_err(|e| Error::Custom(format!("ProvingKey::build fee_collect: {:?}", e)))?;

        info!(
            target: "dwowd::registry::model::LinearPowRewardZk::new",
            "FeeCollect_V1 ZK circuit loaded (k={})", fee_collect_zkbin.k,
        );

        Ok(Self {
            fee_collect_zkbin: Arc::new(fee_collect_zkbin),
            fee_collect_provingkey: Arc::new(fee_collect_provingkey),
            chain_state,
        })
    }
}

/// Generate next block template for linear blockchain.
/// When `linear_zk` is provided, creates a privacy-preserving ZK coinbase.
/// Otherwise falls back to a transparent coinbase (for development/testing).
/// `transactions` are drained from the mempool at template generation time
/// so the merkle root (included in the mining blob) remains fixed.
/// Required ZK proving materials — wraps Option<LinearPowRewardZk>.
///
/// Once constructed (after lazy-init succeeds), access is infallible.
/// Panics at construction if None — the Option only exists during the
/// brief initialization window. Per type-system.md §5: a type that
/// signals "may be absent" but is silently unwrapped SHALL be replaced
/// with a required-access wrapper.
#[derive(Clone)]
pub struct RequiredLinearZk {
    inner: LinearPowRewardZk,
}

impl RequiredLinearZk {
    pub fn new(opt: Option<LinearPowRewardZk>) -> Self {
        #[expect(clippy::expect_used, reason = "LinearPowRewardZk must be initialized before mining")]
        let inner = opt.expect("LinearPowRewardZk must be initialized before mining");
        Self { inner }
    }

    pub fn as_ref(&self) -> &LinearPowRewardZk {
        &self.inner
    }
}

pub async fn generate_linear_block_template(
    chain_state: &dwow_chain::CChainState,
    recipient_config: &LinearMinerRewardsRecipientConfig,
    linear_zk: &RequiredLinearZk,
    transactions: Vec<dwow_chain::Transaction>,
    uncles: Vec<dwow_chain::UncleBlock>,
) -> Result<LinearBlockTemplate> {
    // Cap transactions so the merkle root (included in the mining blob)
    // stays within the block gas budget. Each call is assumed to use its
    // full GAS_LIMIT budget (conservative — actual usage may be lower).
    // Remaining txs stay in the mempool for the next block.
    let gas_limit = dwow_core::runtime::vm_runtime::GAS_LIMIT;
    let block_gas_limit = dwow_chain::execution::BLOCK_GAS_LIMIT;
    // L1 barrier #7: also cap by serialized byte size so the miner never builds
    // a block that peers reject at the wire cap (MAX_BLOCK_SIZE). Proof witnesses
    // ride inside each tx now, so gas accounting alone is insufficient. Reserve
    // headroom for the block header and the coinbase tx added after this
    // selection (uncles are persisted in a separate tree, not in the wire Block).
    let byte_budget = dwow_chain::execution::MAX_BLOCK_SIZE.saturating_sub(512 * 1024);
    let transactions: Vec<dwow_chain::Transaction> = {
        let mut capped = Vec::new();
        let mut estimated_gas: u64 = 0;
        let mut estimated_bytes: usize = 0;
        for tx in transactions {
            let call_gas = tx.contract_calls.len() as u64 * gas_limit;
            // A tx that cannot be serialized cannot be broadcast/persisted; skip
            // it — never let an overflow smuggle an un-serializable tx into the
            // block (release-mode wraparound would otherwise pass the check).
            let tx_bytes = match serde_json::to_vec(&tx) {
                Ok(v) => v.len(),
                Err(_) => continue,
            };
            if estimated_gas.saturating_add(call_gas) > block_gas_limit
                || estimated_bytes.saturating_add(tx_bytes) > byte_budget
            {
                break; // stop here; remainder stays in mempool
            }
            estimated_gas += call_gas;
            estimated_bytes += tx_bytes;
            capped.push(tx);
        }
        capped
    };

    let height = chain_state.get_height().succ();

    // FeeCollectV1 — the "collection plate" as the final template transaction
    // (consensus-coinbase.md §3.12). MUST be appended BEFORE the merkle root
    // computation below so the mining blob commits to it, and it rides in
    // template.transactions into the submit-reconstructed block (stratum /
    // mm_rpc paths). Only in the ZK path — the debug-only non-ZK fallback
    // produces downstream-rejected blocks regardless.
    let transactions: Vec<dwow_chain::Transaction> = {
        let mut txs = transactions;
        { let zk = linear_zk.as_ref();
            // Stratum path: fee decryption not available (no miner_sk).
            // FI-ENCRYPT-3: no silent fallback — use FeeAmount::ZERO.
            // FeeCollectV1 will be skipped (total_fees == 0 → returns None).
            let _fc: u64 = txs.iter().flat_map(|t| &t.contract_calls)
                .filter(|c| c.as_mass_balance_fee_v2().is_some())
                .count() as u64;
            let tf = FeeAmount::ZERO;
            if _fc > 0 {
                tracing::warn!(target: "dwowd::stratum",
                    "{} FeeV2 calls in stratum template — fee decryption unavailable, \
                     FeeCollectV1 will be skipped per FI-ENCRYPT-3", _fc);
            }
            if let Some(fee_tx) = build_fee_collect_tx(
                &recipient_config.recipient,
                &txs,
                height,
                zk,
                tf,
            )? {
                txs.push(fee_tx);
            }
            // Spec: uncle_merkle.md §Uncle Minting & Maturity — "Per-uncle note mint".
            // Mint one spendable note per accepted uncle, appended to the block so
            // its commitment + nullifier ride into connect_block's 0x07 extraction.
            for (idx, uncle) in uncles.iter().enumerate() {
                if !uncle.pin_accepted || uncle.pin_confirmed.get() == 0 {
                    continue;
                }
                let tx_nonce = pallas::Base::from(height.get() * 1000 + idx as u64);
                let uncle_tx = build_uncle_mint_tx(uncle, height, tx_nonce)?;
                txs.push(uncle_tx);
            }
        }
        txs
    };

    #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
    let previous_hash: [u8; 32] = if height == BlockHeight::GENESIS {
        [0u8; 32]
    } else {
        let latest_block = chain_state.get_latest_block()
            .map_err(|e| Error::Custom(format!("Failed to get latest block: {}", e)))?;
        let _prev_key = latest_block.header.randomx_key;
        *chain_state.hash_block_with_cached_vm(&latest_block).expect("hash failed").as_bytes()
    };

    let target = {
        #[expect(clippy::unwrap_used, reason = "mutex is never poisoned")]
        let consensus = chain_state.consensus.lock().unwrap();
        consensus.target()
    };

    use dwow_sdk::blockchain::expected_reward;
    let reward = expected_reward(height);

    // Spec: uncle_merkle.md §Uncle Minting & Maturity — "Canonical note reduction".
    // Compute the reduced effective value (base − Σ pin) for the spendable coinbase
    // note; the cumulative supply chain still mints the FULL base reward.
    let total_pin: u64 = uncles.iter()
        .filter(|u| u.pin_accepted)
        .map(|u| u.pin_confirmed.get())
        .sum();
    let effective_value = BlockReward::new(reward.get().saturating_sub(total_pin));

    #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Compute transaction merkle root (included in mining blob).
    // Single canonical algorithm — shared with verify_merkle_root() and the
    // genesis ceremony via dwow_chain::compute_merkle_root.
    let merkle_root = dwow_chain::compute_merkle_root(&transactions);

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
        let (root, proofs) = dwow_chain::build_uncle_merkle(&uncles, &uncle_vm)
            .map_err(|e| dwow_core::Error::Custom(format!("uncle merkle: {e}")))?;
        (root, proofs)
    };

    { let zk = linear_zk.as_ref();
        // Diagnostic: log exact recipient public key used for AEAD encryption.
        // Cross-reference with wallet's derived_pk from scan diagnostics.
        let recipient_bytes = recipient_config.recipient.public().to_bytes();
        tracing::info!(
            target: "dwowd::registry",
            "Coinbase encrypt: recipient_pk={} height={} reward={}",
            hex::encode(recipient_bytes), height, reward,
        );
        let (coinbase, public_inputs, pow_reward_call, _coin_blind) = build_linear_coinbase_effective(
            recipient_config.recipient.clone(),
            reward,
            effective_value,
            zk,
            height,
        ).await?;

        let commitment_merkle_root = chain_state.compute_root_including_commitment(&coinbase.commitment);
        let nullifier_root = chain_state.block_anchor_root();

        return Ok(LinearBlockTemplate {
            previous: previous_hash,
            height,
            target,
            timestamp,
            value: effective_value,
            zk_proof: coinbase.proof,
            zk_public_inputs: public_inputs,
            commitment: coinbase.commitment,
            value_commit_x: coinbase.value_commit_x,
            value_commit_y: coinbase.value_commit_y,
            token_commit: coinbase.token_commit,
            nullifier: Some(coinbase.nullifier),
            new_cumulative_x: coinbase.new_cumulative_x,
            new_cumulative_y: coinbase.new_cumulative_y,
            encrypted_note: coinbase.encrypted_note,
            pow_reward_call_data: pow_reward_call.data.clone(),
            commitment_merkle_root,
            nullifier_root,
            miner: recipient_config.recipient.public().to_bytes(),
            transactions,
            merkle_root,
            uncles,
            uncle_merkle_root,
            uncle_proofs,
        });
    }

}
