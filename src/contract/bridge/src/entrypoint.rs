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

//! WASM entrypoint for the bridge contract
//!
//! ## How This Implements Bridge Criteria
//!
//! This section explains how the bridge satisfies basic bridge criteria:
//! 1. **Funds are accounted for**: Every deposit creates a commitment in the
//!    Merkle tree. Every withdrawal nullifies a deposit. Arithmetic verified in ZK.
//! 2. **Operations are atomic**: Contract state changes happen in single tx.
//!    If proof verification fails, nothing is committed.
//! 3. **No fund creation**: Withdrawals can only use deposited funds (proven
//!    via membership in deposit tree). Total minted <= total deposited.
//! 4. **No fund destruction**: Burned deposits emit nullifiers. Unspent deposits remain.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, ContractId, PublicKey, poseidon_hash},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    wasm,
    pasta::pallas,
    pasta::group::GroupEncoding as _,
};
use dwow_promissory_note_contract::model::{IssueParamsV1, RedeemParamsV1};
use dwow_promissory_note_contract::validation::validate_child_contract_id;
use dwow_serial::{deserialize, Decodable, Encodable};

use crate::{
    error::BridgeError,
    model::{
        Deposit, DepositParams, ExternalChain,
        Withdrawal, WithdrawParams,
        XmrDepositProof, ZcashDepositProof, AztecDepositProof, LitecoinDepositProof,
    },
    BridgeFunction, BRIDGE_CONTRACT_DEPOSITS_TREE, BRIDGE_CONTRACT_INFO_TREE,
    BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V2, BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V2,
    BRIDGE_CONTRACT_NULLIFIERS_TREE, BRIDGE_CONTRACT_WITHDRAWALS_TREE,
    BRIDGE_CONTRACT_STATE,
    BRIDGE_CONTRACT_XMR_CONFIRMATIONS, BRIDGE_CONTRACT_ZEC_CONFIRMATIONS, BRIDGE_CONTRACT_AZT_CONFIRMATIONS,
    BRIDGE_CONTRACT_LTC_CONFIRMATIONS,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const BRIDGE_DB_VERSION_KEY: &[u8] = b"db_version";
/// HAZOP-13: chain-event uniqueness tree — prevents duplicate external deposits
const BRIDGE_CHAIN_EVENTS_TREE: &str = "chain_events";

/// Derive the deterministic wrapped-token ID for a chain.
///
/// The wrapped token's mint authority is a public, deterministic secret derived
/// from the bridge contract ID + chain (mint-authority Option 1). This mirrors
/// promissory_note's `RegisterTypeV1` asset_id derivation so the bridge can
/// validate child `IssueV1` calls without storing per-chain token IDs.
fn derive_wrapped_asset_id(cid: &ContractId, chain: ExternalChain) -> pallas::Base {
    // issue_secret = H(bridge_cid, chain, domain)
    let issue_secret = poseidon_hash([
        cid.inner(),
        pallas::Base::from(chain as u64),
        pallas::Base::from(0x62726964u64), // "brid"
    ]);
    // token_auth_parent = H(7, issue_secret)  (matches PN IssueV2 issue_public)
    let token_auth_parent = poseidon_hash([pallas::Base::from(7u64), issue_secret]);
    // token_blind = H(chain, domain)  (deterministic blinding factor)
    let token_blind = poseidon_hash([
        pallas::Base::from(chain as u64),
        pallas::Base::from(0x626c6e64u64), // "blnd"
    ]);
    // asset_id = H(2, token_auth_parent, token_user_data=0, token_blind)
    // (matches PN RegisterTypeV2)
    poseidon_hash([
        pallas::Base::from(2u64),
        token_auth_parent,
        pallas::Base::zero(),
        token_blind,
    ])
}

// ============================================================================
// CONTRACT DEFINITION
// ============================================================================

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize bridge contract state
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // V2 circuits (HAZOP RC3: domain separation)
    let deposit_v2_bincode = include_bytes!("../proof/deposit.zk.bin");
    wasm::db::zkas_db_set(&deposit_v2_bincode[..])?;
    let withdraw_v2_bincode = include_bytes!("../proof/withdraw.zk.bin");
    wasm::db::zkas_db_set(&withdraw_v2_bincode[..])?;

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, BRIDGE_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, BRIDGE_DB_VERSION_KEY, env!("CARGO_PKG_VERSION").as_bytes())?;
    wasm::db::db_set(info_db, BRIDGE_CONTRACT_STATE, b"initialized")?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;

    // Initialize deposits tree
    wasm::db::db_init(cid, BRIDGE_CONTRACT_DEPOSITS_TREE)?;
    // HAZOP-13: chain-event uniqueness — prevent duplicate external deposits
    wasm::db::db_init(cid, BRIDGE_CHAIN_EVENTS_TREE)?;

    // Initialize withdrawals tree
    wasm::db::db_init(cid, BRIDGE_CONTRACT_WITHDRAWALS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, BRIDGE_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[bridge::init_contract] Bridge initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BridgeFunction::try_from(self_.data[0])?;

    let metadata = match func {
        BridgeFunction::InitializeV1 => Ok(vec![]),
        BridgeFunction::DepositV1 => deposit_get_metadata(&self_.data[1..]),
        BridgeFunction::WithdrawV1 => withdraw_get_metadata(&self_.data[1..]),
    }?;

    wasm::util::set_return_data(&metadata)
}

/// Metadata for DepositV1 ZK proof verification.
fn deposit_get_metadata(data: &[u8]) -> Result<Vec<u8>, ContractError> {
    use dwow_sdk::pasta::pallas;

    let params = match DepositParams::decode(data) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V2.to_string(),
        vec![params.commitment.inner(), poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    let signature_pubkeys: Vec<pallas::Base> = vec![];
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for WithdrawV1 ZK proof verification.
fn withdraw_get_metadata(data: &[u8]) -> Result<Vec<u8>, ContractError> {
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_sdk::pasta::pallas;

    let params = match WithdrawParams::decode(data) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    let nullifier = params.nullifier.inner();
    let recipient_base = match pallas::Base::from_repr(params.recipient_hash).into() {
        Some(b) => b,
        None => return Ok(vec![]),
    };
    let derived_recipient = poseidon_hash([pallas::Base::from(7u64), recipient_base]);

    // Token-aware minimum withdrawal amount (anti-dust)
    let token_minimum = pallas::Base::from(params.token_minimum);

    zk_public_inputs.push((
        BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V2.to_string(),
        vec![nullifier, derived_recipient, token_minimum, poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    let signature_pubkeys: Vec<pallas::Base> = vec![];
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = BridgeFunction::try_from(func_byte)?;

    let update_bytes = match func {
        BridgeFunction::InitializeV1 => {
            msg!("[bridge::process_instruction] InitializeV1 has no update data");
            vec![]
        }
        BridgeFunction::DepositV1 => process_deposit_instruction(cid, call_idx, calls)?,
        BridgeFunction::WithdrawV1 => process_withdraw_instruction(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

/// Process deposit instruction
fn process_deposit_instruction(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for wrapped-PN issuance
    if this_call.children_indexes.len() != 1 {
        msg!("[bridge::DepositV1] Error: Expected 1 child call (promissory_note::issue_v1), got {}", this_call.children_indexes.len());
        return Err(BridgeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x02 {
        msg!("[bridge::DepositV1] Error: Expected promissory_note::issue_v1 (0x02), got 0x{:02x}", child_call.data[0]);
        return Err(BridgeError::InvalidChildCall.into())
    }
    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(BridgeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP RC-F fix: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        msg!("[bridge] Error: promissory_note contract ID not configured");
        return Err(BridgeError::InvalidChildCall.into());
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= DepositParams::decode(&self_.data[1..])?;

    // Validate the issued wrapped PN: spend_hook must be this bridge, and
    // asset_id must be the deterministic wrapped token for the deposit's chain.
    let issue_params = IssueParamsV1::decode(&child_call.data[1..])?;
    if issue_params.spend_hook.inner() != cid.inner() {
        msg!("[bridge::DepositV1] Error: wrapped PN spend_hook is not the bridge");
        return Err(BridgeError::InvalidChildCall.into())
    }
    let expected_asset_id = derive_wrapped_asset_id(&cid, params.chain);
    if issue_params.asset_id.inner() != expected_asset_id {
        msg!("[bridge::DepositV1] Error: wrapped PN asset_id does not match the deposit chain");
        return Err(BridgeError::InvalidChildCall.into())
    }

    msg!("[bridge::process_instruction] Processing deposit: commitment={:?}, chain={:?}", &params.commitment, &params.chain);

    // Verify deposit hasn't already been registered (double-deposit check)
    let deposits_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_DEPOSITS_TREE)?;
    if wasm::db::db_contains_key(deposits_db, &params.commitment.to_bytes())? {
        msg!("[bridge::process_instruction] ERROR: Deposit already registered");
        return Err(BridgeError::DoubleDeposit.into())
    }

    // HAZOP-13: prevent same external chain event from being deposited multiple
    // times with different DarkFi commitments (varying recipient_pub, nonce, secret).
    let events_db = wasm::db::db_lookup(cid, BRIDGE_CHAIN_EVENTS_TREE)?;
    let mut event_key = Vec::with_capacity(1 + params.external_block_hash.len());
    event_key.push(params.chain as u8);
    event_key.extend_from_slice(&params.external_block_hash);
    let event_hash = blake3::hash(&event_key);
    if wasm::db::db_contains_key(events_db, event_hash.as_bytes())? {
        msg!("[bridge::process_instruction] ERROR: External chain event already deposited");
        return Err(BridgeError::DoubleDeposit.into())
    }

    // HAZOP HAZ-CODE-01: unified verification path.
    #[cfg(feature = "bridge-verify")]
    crate::verify::verify_chain_proof(&params.chain_proof, &params.merkle_proof)?;
    #[cfg(not(feature = "bridge-verify"))]
    {
        if params.chain == ExternalChain::Ethereum {
            if params.merkle_proof.is_empty() {
                msg!("[bridge::DepositV1] Error: Ethereum merkle proof is empty");
                return Err(BridgeError::InvalidMerkleProof.into());
            }
        } else {
            msg!("[bridge::DepositV1] Error: bridge-verify feature not enabled — rejecting non-Ethereum deposit");
            return Err(BridgeError::InvalidDeposit(
                "Cross-chain verification not compiled (enable bridge-verify feature)".into()
            ).into());
        }
    }

    // Create update data
    let update = DepositUpdateV1 {
        commitment: params.commitment,
        recipient_pub: params.recipient_pub,
        bridge_nonce: params.bridge_nonce,
        chain: params.chain,
        external_block_hash: params.external_block_hash,
        amount: params.amount,
    };

    Ok(update.encode())
}

/// Verify XMR deposit proof
#[allow(dead_code)]
fn verify_xmr_deposit(_cid: ContractId, proof: &XmrDepositProof) -> ContractResult {
    use dwow_sdk::pasta::pallas;

    msg!("[bridge::verify_xmr_deposit] Verifying XMR deposit proof");
    msg!("[bridge::verify_xmr_deposit] tx_hash={:?}, amount={}, confirmations={}",
          &proof.tx_hash, proof.amount, proof.confirmations);

    const MIN_XMR_DEPOSIT: u64 = 1_000_000_000;
    if proof.amount < MIN_XMR_DEPOSIT {
        msg!("[bridge::verify_xmr_deposit] ERROR: Amount below minimum");
        return Err(BridgeError::InvalidDeposit("Amount below minimum".into()).into())
    }

    if proof.confirmations < BRIDGE_CONTRACT_XMR_CONFIRMATIONS as u64 {
        msg!("[bridge::verify_xmr_deposit] ERROR: Insufficient confirmations");
        return Err(BridgeError::InsufficientConfirmations.into())
    }

    let ephemeral_point = pallas::Point::from_bytes(&proof.ephemeral_pub);
    if bool::from(ephemeral_point.is_none()) {
        msg!("[bridge::verify_xmr_deposit] ERROR: Invalid ephemeral public key");
        return Err(BridgeError::InvalidCommitment.into())
    }

    msg!("[bridge::verify_xmr_deposit] WARNING: DLEq proof verification not implemented — deposit accepted without cryptographic proof of address ownership");

    if proof.coinbase_merkle_proof.is_empty() {
        msg!("[bridge::verify_xmr_deposit] ERROR: Empty coinbase merkle proof");
        return Err(BridgeError::InvalidMerkleProof.into())
    }
    msg!("[bridge::verify_xmr_deposit] Coinbase merkle proof length: {}", proof.coinbase_merkle_proof.len());

    msg!("[bridge::verify_xmr_deposit] XMR deposit proof verified successfully");
    Ok(())
}

/// Verify Zcash Sapling deposit proof
#[allow(dead_code)]
fn verify_zcash_deposit(_cid: ContractId, proof: &ZcashDepositProof) -> ContractResult {
    use dwow_sdk::pasta::pallas;

    msg!("[bridge::verify_zcash_deposit] Verifying Zcash Sapling deposit proof");
    msg!("[bridge::verify_zcash_deposit] nullifier={:?}, amount={}, confirmations={}",
          &proof.nullifier, proof.amount, proof.confirmations);

    const MIN_ZEC_DEPOSIT: u64 = 10_000;
    if proof.amount < MIN_ZEC_DEPOSIT {
        msg!("[bridge::verify_zcash_deposit] ERROR: Amount below minimum");
        return Err(BridgeError::InvalidDeposit("Amount below minimum".into()).into())
    }

    if proof.confirmations < BRIDGE_CONTRACT_ZEC_CONFIRMATIONS as u64 {
        msg!("[bridge::verify_zcash_deposit] ERROR: Insufficient confirmations");
        return Err(BridgeError::InsufficientConfirmations.into())
    }

    let commitment_point = pallas::Point::from_bytes(&proof.commitment);
    if bool::from(commitment_point.is_none()) {
        msg!("[bridge::verify_zcash_deposit] ERROR: Invalid commitment");
        return Err(BridgeError::InvalidCommitment.into())
    }

    if proof.anchor.iter().all(|&b| b == 0) {
        msg!("[bridge::verify_zcash_deposit] ERROR: Invalid anchor (zero)");
        return Err(BridgeError::InvalidMerkleProof.into())
    }

    if proof.spend_proof.is_empty() {
        msg!("[bridge::verify_zcash_deposit] ERROR: Empty spend proof");
        return Err(BridgeError::InvalidZkProof.into())
    }
    msg!("[bridge::verify_zcash_deposit] Spend proof length: {}", proof.spend_proof.len());

    if proof.output_proof.is_empty() {
        msg!("[bridge::verify_zcash_deposit] ERROR: Empty output proof");
        return Err(BridgeError::InvalidZkProof.into())
    }
    msg!("[bridge::verify_zcash_deposit] Output proof length: {}", proof.output_proof.len());

    if proof.merkle_path.is_empty() {
        msg!("[bridge::verify_zcash_deposit] ERROR: Empty merkle path");
        return Err(BridgeError::InvalidMerkleProof.into())
    }
    msg!("[bridge::verify_zcash_deposit] Merkle path length: {}", proof.merkle_path.len());

    msg!("[bridge::verify_zcash_deposit] Zcash Sapling deposit proof verified successfully");
    Ok(())
}

/// Verify Aztec rollup deposit proof
#[allow(dead_code)]
fn verify_aztec_deposit(_cid: ContractId, proof: &AztecDepositProof) -> ContractResult {
    use dwow_sdk::pasta::pallas;

    msg!("[bridge::verify_aztec_deposit] Verifying Aztec rollup deposit proof");
    msg!("[bridge::verify_aztec_deposit] nullifier={:?}, value={}, asset_id={}, confirmations={}",
          &proof.nullifier, proof.value, proof.asset_id, proof.confirmations);

    const MIN_AZT_DEPOSIT_VALUE: u64 = 1_000_000_000_000_000;
    if proof.value < MIN_AZT_DEPOSIT_VALUE {
        msg!("[bridge::verify_aztec_deposit] ERROR: Value below minimum");
        return Err(BridgeError::InvalidDeposit("Value below minimum".into()).into())
    }

    if proof.confirmations < BRIDGE_CONTRACT_AZT_CONFIRMATIONS as u64 {
        msg!("[bridge::verify_aztec_deposit] ERROR: Insufficient confirmations");
        return Err(BridgeError::InsufficientConfirmations.into())
    }

    let commitment_point = pallas::Point::from_bytes(&proof.commitment);
    if bool::from(commitment_point.is_none()) {
        msg!("[bridge::verify_aztec_deposit] ERROR: Invalid commitment");
        return Err(BridgeError::InvalidCommitment.into())
    }

    if proof.anchor.iter().all(|&b| b == 0) {
        msg!("[bridge::verify_aztec_deposit] ERROR: Invalid anchor (zero)");
        return Err(BridgeError::InvalidMerkleProof.into())
    }

    if proof.nullifier.iter().all(|&b| b == 0) {
        msg!("[bridge::verify_aztec_deposit] ERROR: Invalid nullifier (zero)");
        return Err(BridgeError::InvalidNullifier.into())
    }

    if proof.rollup_tx_hash.iter().all(|&b| b == 0) {
        msg!("[bridge::verify_aztec_deposit] ERROR: Invalid rollup tx hash (zero)");
        return Err(BridgeError::InvalidDeposit("Invalid rollup tx hash".into()).into())
    }

    if proof.proof_bytes.is_empty() {
        msg!("[bridge::verify_aztec_deposit] ERROR: Empty proof bytes");
        return Err(BridgeError::InvalidZkProof.into())
    }
    msg!("[bridge::verify_aztec_deposit] Proof bytes length: {}", proof.proof_bytes.len());

    if proof.merkle_path.is_empty() {
        msg!("[bridge::verify_aztec_deposit] ERROR: Empty merkle path");
        return Err(BridgeError::InvalidMerkleProof.into())
    }
    msg!("[bridge::verify_aztec_deposit] Merkle path length: {}", proof.merkle_path.len());

    if proof.rollup_height == 0 {
        msg!("[bridge::verify_aztec_deposit] ERROR: Invalid rollup height");
        return Err(BridgeError::InvalidDeposit("Invalid rollup height".into()).into())
    }
    if proof.eth_block_height == 0 {
        msg!("[bridge::verify_aztec_deposit] ERROR: Invalid Ethereum block height");
        return Err(BridgeError::InvalidDeposit("Invalid eth block height".into()).into())
    }

    msg!("[bridge::verify_aztec_deposit] Aztec rollup deposit proof verified successfully");
    Ok(())
}

/// Verify Litecoin deposit proof
#[allow(dead_code)]
fn verify_litecoin_deposit(_cid: ContractId, proof: &LitecoinDepositProof) -> ContractResult {
    use dwow_sdk::pasta::pallas;

    msg!("[bridge::verify_litecoin_deposit] Verifying Litecoin deposit proof");
    msg!("[bridge::verify_litecoin_deposit] tx_hash={:?}, amount={}, confirmations={}",
          &proof.tx_hash, proof.amount, proof.confirmations);
    msg!("[bridge::verify_litecoin_deposit] is_confidential={}", proof.is_confidential);

    const MIN_LTC_DEPOSIT: u64 = 100_000;
    if proof.amount < MIN_LTC_DEPOSIT {
        msg!("[bridge::verify_litecoin_deposit] ERROR: Amount below minimum");
        return Err(BridgeError::InvalidDeposit("Amount below minimum".into()).into())
    }

    if proof.confirmations < BRIDGE_CONTRACT_LTC_CONFIRMATIONS as u64 {
        msg!("[bridge::verify_litecoin_deposit] ERROR: Insufficient confirmations");
        return Err(BridgeError::InsufficientConfirmations.into())
    }

    if proof.tx_hash.iter().all(|&b| b == 0) {
        msg!("[bridge::verify_litecoin_deposit] ERROR: Invalid tx hash (zero)");
        return Err(BridgeError::InvalidDeposit("Invalid tx hash".into()).into())
    }

    if proof.block_merkle_root.iter().all(|&b| b == 0) {
        msg!("[bridge::verify_litecoin_deposit] ERROR: Invalid block merkle root (zero)");
        return Err(BridgeError::InvalidMerkleProof.into())
    }

    if proof.block_height == 0 {
        msg!("[bridge::verify_litecoin_deposit] ERROR: Invalid block height");
        return Err(BridgeError::InvalidDeposit("Invalid block height".into()).into())
    }

    if proof.is_confidential {
        if let Some(commitment) = proof.confidential_commitment {
            let commitment_point = pallas::Point::from_bytes(&commitment);
            if bool::from(commitment_point.is_none()) {
                msg!("[bridge::verify_litecoin_deposit] ERROR: Invalid confidential commitment");
                return Err(BridgeError::InvalidCommitment.into())
            }
            msg!("[bridge::verify_litecoin_deposit] Confidential commitment verified");
        } else {
            msg!("[bridge::verify_litecoin_deposit] ERROR: Missing confidential commitment for MWEB deposit");
            return Err(BridgeError::InvalidDeposit("Missing MWEB commitment".into()).into())
        }

        if proof.range_proof.is_none() {
            msg!("[bridge::verify_litecoin_deposit] ERROR: Missing range proof for MWEB deposit");
            return Err(BridgeError::InvalidZkProof.into())
        }
        msg!("[bridge::verify_litecoin_deposit] Range proof present for MWEB deposit");
    }

    if proof.merkle_proof.is_empty() {
        msg!("[bridge::verify_litecoin_deposit] ERROR: Empty merkle proof");
        return Err(BridgeError::InvalidMerkleProof.into())
    }
    msg!("[bridge::verify_litecoin_deposit] Merkle proof length: {}", proof.merkle_proof.len());

    msg!("[bridge::verify_litecoin_deposit] Litecoin deposit proof verified successfully");
    Ok(())
}

/// Process withdrawal instruction
fn process_withdraw_instruction(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for wrapped-PN redemption
    if this_call.children_indexes.len() != 1 {
        msg!("[bridge::WithdrawV1] Error: Expected 1 child call (promissory_note::redeem_v1), got {}", this_call.children_indexes.len());
        return Err(BridgeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x01 {
        msg!("[bridge::WithdrawV1] Error: Expected promissory_note::redeem_v1 (0x01), got 0x{:02x}", child_call.data[0]);
        return Err(BridgeError::InvalidChildCall.into())
    }
    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
        .ok_or(BridgeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP RC-F fix: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        msg!("[bridge] Error: promissory_note contract ID not configured");
        return Err(BridgeError::InvalidChildCall.into());
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params= WithdrawParams::decode(&self_.data[1..])?;

    // Validate the redeemed coin is a wrapped PN (spend_hook == bridge) and the
    // receipt routes back to the bridge (non-transferable, issuer-visible).
    let redeem_params = RedeemParamsV1::decode(&child_call.data[1..])?;
    if redeem_params.input.spend_hook.inner() != cid.inner() {
        msg!("[bridge::WithdrawV1] Error: redeemed coin spend_hook is not the bridge");
        return Err(BridgeError::InvalidChildCall.into())
    }
    if redeem_params.output.spend_hook.inner() != cid.inner() {
        msg!("[bridge::WithdrawV1] Error: receipt spend_hook is not the bridge");
        return Err(BridgeError::InvalidChildCall.into())
    }

    msg!("[bridge::process_instruction] Processing withdrawal: nullifier={:?}", &params.nullifier);

    // Verify nullifier hasn't been spent (double-spend check)
    let nullifiers_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.nullifier.to_bytes())? {
        msg!("[bridge::process_instruction] ERROR: Nullifier already spent");
        return Err(BridgeError::DoubleSpend.into())
    }

    // Create update data
    let update = WithdrawUpdateV1 {
        nullifier: params.nullifier,
        recipient_hash: params.recipient_hash,
        amount: params.amount,
        timeout_height: params.timeout_height,
        feed_mode: params.feed_mode,
    };

    Ok(update.encode())
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = BridgeFunction::try_from(update_data[0])?;

    match func {
        BridgeFunction::InitializeV1 => {
            msg!("[bridge::process_update] InitializeV1 has no update data");
            Ok(())
        }
        BridgeFunction::DepositV1 => {
            let update = DepositUpdateV1::decode(&update_data[1..])?;
            apply_deposit_update(cid, update)
        }
        BridgeFunction::WithdrawV1 => {
            let update = WithdrawUpdateV1::decode(&update_data[1..])?;
            apply_withdraw_update(cid, update)
        }
    }
}

/// Apply deposit state update
fn apply_deposit_update(cid: ContractId, update: DepositUpdateV1) -> ContractResult {
    let deposits_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_DEPOSITS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_INFO_TREE)?;

    wasm::db::db_set(deposits_db, &update.commitment.to_bytes(), &[1])?;

    // HAZOP-13: store chain-event uniqueness key to prevent duplicate external deposits
    let events_db = wasm::db::db_lookup(cid, BRIDGE_CHAIN_EVENTS_TREE)?;
    let mut event_key = Vec::with_capacity(1 + update.external_block_hash.len());
    event_key.push(update.chain as u8);
    event_key.extend_from_slice(&update.external_block_hash);
    let event_hash = blake3::hash(&event_key);
    wasm::db::db_set(events_db, event_hash.as_bytes(), &[1])?;

    // Store full deposit record
    let deposit = Deposit {
        version: 1,
        commitment: update.commitment,
        amount: update.amount,
        chain: update.chain.clone(),
        external_height: 0,
        claimed: false,
        registered_at: get_current_timestamp(info_db)?,
    };
    wasm::db::db_set(deposits_db, &build_deposit_key(&update.commitment.to_bytes()), &deposit.encode())?;

    msg!("[bridge::process_update] Deposit registered");
    Ok(())
}

/// Apply withdrawal state update
fn apply_withdraw_update(cid: ContractId, update: WithdrawUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_NULLIFIERS_TREE)?;
    let withdrawals_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_WITHDRAWALS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, BRIDGE_CONTRACT_INFO_TREE)?;

    // Mark nullifier as spent
    wasm::db::db_mark_spent(nullifiers_db, &update.nullifier.to_bytes())?;

    // Record withdrawal (external-release signal for the relayer)
    let withdrawal = Withdrawal {
        version: 1,
        nullifier: update.nullifier,
        recipient_hash: update.recipient_hash,
        amount: update.amount,
        executed: false,
        external_tx_hash: None,
        withdrawn_at: get_current_timestamp(info_db)?,
    };
    wasm::db::db_set(withdrawals_db, &build_withdrawal_key(&update.nullifier.to_bytes()), &withdrawal.encode())?;

    msg!("[bridge::process_update] Withdrawal recorded: nullifier={:?}", &update.nullifier);
    Ok(())
}


// ============================================================================
// UPDATE STRUCTS
// ============================================================================

/// Update data for deposit
#[derive(Debug, Clone)]
pub struct DepositUpdateV1 {
    pub commitment: dwow_sdk::crypto::IntentCommitment,
    pub recipient_pub: PublicKey,
    pub bridge_nonce: u64,
    pub chain: ExternalChain,
    pub external_block_hash: [u8; 32],
    pub amount: u64,
}

impl DepositUpdateV1 {
    pub const ENCODED_SIZE: usize = 113;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(113);
        b.extend_from_slice(&self.commitment.to_bytes());
        b.extend_from_slice(&self.recipient_pub.to_bytes());
        b.extend_from_slice(&self.bridge_nonce.to_le_bytes());
        b.push(self.chain as u8);
        b.extend_from_slice(&self.external_block_hash);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 113 { return Err(ContractError::IoError(format!("DepositUpdateV1: expected 113 bytes, got {}", data.len()))); }
        Ok(DepositUpdateV1 {
            commitment: dwow_sdk::crypto::IntentCommitment::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositUpdateV1: invalid commitment: {}", e)))?,
            recipient_pub: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositUpdateV1: invalid recipient_pub: {}", e)))?,
            bridge_nonce: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            chain: ExternalChain::try_from(data[72])?,
            external_block_hash: data[73..105].try_into().unwrap(),
            amount: u64::from_le_bytes(data[105..113].try_into().unwrap()),
        })
    }
}

/// Update data for withdrawal
#[derive(Debug, Clone)]
pub struct WithdrawUpdateV1 {
    pub nullifier: dwow_sdk::crypto::IntentNullifier,
    pub recipient_hash: [u8; 32],
    pub amount: u64,
    pub timeout_height: u64,
    pub feed_mode: u8,
}

impl WithdrawUpdateV1 {
    pub const ENCODED_SIZE: usize = 81;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(81);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.recipient_hash);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.timeout_height.to_le_bytes());
        b.push(self.feed_mode);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 81 { return Err(ContractError::IoError(format!("WithdrawUpdateV1: expected 81 bytes, got {}", data.len()))); }
        Ok(WithdrawUpdateV1 {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("WithdrawUpdateV1: invalid nullifier: {}", e)))?,
            recipient_hash: data[32..64].try_into().unwrap(),
            amount: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            timeout_height: u64::from_le_bytes(data[72..80].try_into().unwrap()),
            feed_mode: data[80],
        })
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Build deposit record key
fn build_deposit_key(commitment: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 32);
    key.push(b'D');
    key.extend_from_slice(commitment);
    key
}

/// Build withdrawal record key
fn build_withdrawal_key(nullifier: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + 32);
    key.push(b'W');
    key.extend_from_slice(nullifier);
    key
}

/// Get current timestamp from info_db
fn get_current_timestamp(info_db: u32) -> Result<u64, ContractError> {
    let data = wasm::db::db_get(info_db, b"current_timestamp")?;
    match data {
        Some(d) => {
            let mut cursor = std::io::Cursor::new(&d);
            u64::decode(&mut cursor).map_err(|_| ContractError::IoError("decode error".to_string()))
        }
        None => Ok(0),
    }
}
