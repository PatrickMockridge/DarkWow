/* This file is part of DarkFi (https://dark.fi)
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

//! Data structures for bridge contract calls
//!
//! Security Model: Object Capability Security (No VSS)
//!
//! Unlike VSS-based bridges, this design uses deterministic address derivation:
//! - Bridge address = H(recipient_identity, nonce)
//! - No secret sharing between bridge nodes
//! - User alone controls withdrawal via their secret

use darkfi_serial::{SerialDecodable, SerialEncodable};
use darkfi_sdk::crypto::{IntentCommitment, IntentNullifier};

/// Namespace for bridge intents (used with generic intent primitives)
pub const BRIDGE_NAMESPACE: u64 = 0x0002;

/// External chain identifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum ExternalChain {
    Ethereum,
    Monero,
    Zcash,
    Aztec,
    Litecoin,
    // Future chains can be added here
    // Bitcoin,
}

/// Bridge deposit parameters
///
/// Security: Deposit creates a commitment H(secret, amount, bridge_address).
/// Only the depositor knows `secret`, so only they can later withdraw.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositParams {
    /// Commitment hash from user's secret (uses generic PrivateIntent commitment)
    /// commitment = poseidon_hash([9001, owner_x, owner_y, namespace, payload_hash, expiry, nonce, blind])
    pub commitment: IntentCommitment,

    /// Recipient public key for address derivation
    /// Used to compute: bridge_address = H(H(secret)*G, recipient_pub)
    pub recipient_pub_x: [u8; 32],
    pub recipient_pub_y: [u8; 32],

    /// Nonce ensures fresh address per deposit (temporal privacy)
    pub bridge_nonce: u64,

    /// The external chain where the deposit was made
    pub chain: ExternalChain,

    /// Hash of the external block containing the deposit
    pub external_block_hash: [u8; 32],

    /// Merkle proof of deposit inclusion in external chain (Ethereum)
    pub merkle_proof: Vec<[u8; 32]>,

    /// Merkle root of external chain state at block
    pub external_state_root: [u8; 32],

    /// Bridge fee paid by depositor
    pub fee: u64,

    /// ZK proof demonstrating:
    /// 1. Knowledge of secret
    /// 2. Deposit exists in external chain
    /// 3. Commitment is correctly computed
    pub proof: Vec<u8>,

    /// XMR-specific deposit proof data (used when chain is Monero)
    /// This contains DLEq proof, tx data, and confirmation proof
    pub xmr_proof: Option<XmrDepositProof>,

    /// ZEC-specific deposit proof data (used when chain is Zcash)
    /// This contains nullifier, commitment, merkle path, and spend proofs
    pub zec_proof: Option<ZcashDepositProof>,

    /// AZT-specific deposit proof data (used when chain is Aztec)
    /// This contains nullifier, commitment, rollup data, and proofs
    pub azt_proof: Option<AztecDepositProof>,

    /// LTC-specific deposit proof data (used when chain is Litecoin)
    /// This contains tx data, merkle proof, and optional MWEB data
    pub ltc_proof: Option<LitecoinDepositProof>,
}

/// Bridge withdrawal parameters
///
/// Security: Withdrawal is authorized by the depositor alone via their secret.
/// No VSS/threshold signing required.
///
/// Nullifier = H(secret) proves deposit ownership without revealing secret.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParams {
    /// Nullifier = H(secret) - proves deposit exists and hasn't been withdrawn
    /// Uses generic PrivateIntent nullifier: poseidon_hash([9002, owner_secret, namespace, nonce, commitment])
    pub nullifier: IntentNullifier,

    /// Recipient address hash on external chain
    /// Hash of actual address for privacy
    pub recipient_hash: [u8; 32],

    /// Amount to withdraw
    pub amount: u64,

    /// ZK proof demonstrating:
    /// 1. Knowledge of secret corresponding to a registered deposit
    /// 2. Deposit is in the bridge's Merkle tree
    /// 3. Amount is valid (<= deposited amount)
    /// 4. Recipient hash matches
    pub proof: Vec<u8>,

    /// Bridge fee paid by withdrawer
    pub fee: u64,
}

/// Bridge configuration update parameters
///
/// Security: Only callable by authorized governance (DAO).
/// This doesn't affect user funds - only operational parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParams {
    /// New deposit fee
    pub deposit_fee: u64,

    /// New withdrawal fee
    pub withdrawal_fee: u64,

    /// Minimum confirmations required on external chain
    pub min_confirmations: u32,

    /// Maximum deposit amount (anti-money laundering)
    pub max_deposit: u64,

    /// Maximum withdrawal amount
    pub max_withdrawal: u64,
}

/// Stored deposit record
///
/// This record tracks deposits registered in the bridge.
/// The actual proof of deposit ownership is via the commitment
/// which requires knowledge of secret to claim.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Deposit {
    /// Commitment hash (uses generic PrivateIntent commitment)
    pub commitment: IntentCommitment,

    /// Amount deposited
    pub amount: u64,

    /// External chain of origin
    pub chain: ExternalChain,

    /// Block height on external chain
    pub external_height: u64,

    /// Whether deposit has been claimed (withdrawn)
    pub claimed: bool,

    /// Timestamp of registration
    pub registered_at: u64,
}

/// Stored withdrawal record
///
/// Records successful withdrawals for audit trail.
/// Note: Withdrawal doesn't reveal which deposit was withdrawn,
/// only that some deposit was spent.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Withdrawal {
    /// Nullifier (proves deposit was spent) - uses generic PrivateIntent nullifier
    pub nullifier: IntentNullifier,

    /// Recipient on external chain (hashed)
    pub recipient_hash: [u8; 32],

    /// Amount withdrawn
    pub amount: u64,

    /// Whether withdrawal has been executed on external chain
    pub executed: bool,

    /// Transaction hash on external chain (if executed)
    pub external_tx_hash: Option<[u8; 32]>,

    /// Timestamp of withdrawal
    pub withdrawn_at: u64,
}

// ================================================================
// XMR (MONERO) BRIDGING SUPPORT
// ================================================================
//
// Monero uses Cryptonote protocol which differs from Ethereum's UTXO model:
// - One-time addresses instead of regular public keys
// - View keys for observation without spending authority
// - DLEq proofs for ownership verification instead of signatures
//
// XMR Deposit Flow:
//
// 1. User computes one-time address: derive_from(bridge_pub, view_key)
// 2. User sends XMR to this address on Monero chain
// 3. Relayer observes deposit via Monero RPC (view key)
// 4. Relayer constructs DLEq proof showing ownership
// 5. User submits DepositV1 with XmrDepositProof
// 6. Contract verifies DLEq + merkle proof + confirmations
// 7. Contract mints wXMR to user
//
// ================================================================

/// XMR deposit proof data for Monero bridging
///
/// This structure contains the cryptographic proof required to verify
/// an XMR deposit on the Monero chain without revealing the user's
/// spend key.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct XmrDepositProof {
    /// Monero transaction hash (cn_fast_hash / keccak256 of tx serialization)
    pub tx_hash: [u8; 32],

    /// Monero block height containing the deposit
    pub block_height: u64,

    /// Output index in the transaction (proves which output is the deposit)
    pub output_index: u64,

    /// Amount in piconero (smallest XMR unit, 1 XMR = 10^12 piconero)
    pub amount: u64,

    /// Ephemeral public key of the one-time address (receiving address)
    pub ephemeral_pub: [u8; 32],

    /// DLEq proof demonstrating ownership of the one-time address
    /// This proves the recipient owns the private key corresponding to ephemeral_pub
    pub dleq_proof: DleqProof,

    /// Merkle proof to coinbase hash (proves block is in main chain)
    pub coinbase_merkle_proof: Vec<[u8; 32]>,

    /// Number of block confirmations (must meet minimum threshold)
    pub confirmations: u64,
}

/// Discrete Logarithm Equality proof structure
///
/// DLEq proves that the prover knows x such that:
/// - Y1 = x * G1 (on curve 1)
/// - Y2 = x * G2 (on curve 2)
///
/// For Monero, this proves ownership of the one-time address private key
/// without revealing the key itself.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DleqProof {
    /// First challenge response
    pub challenge_response_1: [u8; 32],
    /// Second challenge response
    pub challenge_response_2: [u8; 32],
    /// Challenge value
    pub challenge: [u8; 32],
}

// ================================================================
// ZCASH (SAPLING) BRIDGING SUPPORT
// ================================================================
//
// Zcash Sapling provides fully shielded transactions with:
// - Pedersen commitments for value (jubjub curve)
// - Nullifiers derived from note private key + note plaintext
// - Spend proofs (Groth16 zk-SNARK proofs)
// - Merkle path authentication (Sapling note commitment tree)
//
// ZEC Deposit Flow:
//
// 1. User creates a Sapling shielded address (zaddr)
// 2. User sends ZEC to this address on Zcash chain
// 3. Relayer observes deposit via Zcash light walletd RPC (view key)
// 4. Relayer constructs proof showing:
//    - Note exists at given merkle root
//    - Prover knows note private key (nullifier derived)
// 5. User submits DepositV1 with ZcashDepositProof
// 6. Contract verifies anchor + merkle proof + spend proof
// 7. Contract mints wZEC to user
//
// ================================================================

/// Zcash Sapling deposit proof data
///
/// This structure contains the cryptographic proofs required to verify
/// a Zcash Sapling deposit on the Zcash chain without revealing the
/// user's spend key or transaction details.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ZcashDepositProof {
    /// Nullifier derived from note (proves note hasn't been spent)
    /// Computed as: note_nullifier = blake2s(labeled_communication, ...)
    pub nullifier: [u8; 32],

    /// Pedersen commitment to the deposit value (jubjub curve)
    /// Computed as: commitment = value * G_v + randomness * G_r
    pub commitment: [u8; 32],

    /// Merkle root of the Sapling note commitment tree at deposit height
    pub anchor: [u8; 32],

    /// Merkle path authenticating the note's position in the tree
    pub merkle_path: Vec<[u8; 32]>,

    /// Spend proof bytes (Groth16 proof for spend authorization)
    /// Proves:
    /// 1. Prover knows note private key corresponding to nullifier
    /// 2. Note commitment is correctly computed
    /// 3. Merkle path is valid
    pub spend_proof: Vec<u8>,

    /// Output proof bytes (Groth16 proof for output)
    /// Proves the output note commitment is well-formed
    pub output_proof: Vec<u8>,

    /// Randomized public key for the note (diversified payment address)
    pub randomized_pub_key: [u8; 32],

    /// Ephemeral randomness used in commitment (blinding factor)
    pub randomness: [u8; 32],

    /// Deposit amount in zatoshi (smallest ZEC unit, 1 ZEC = 10^8 zatoshi)
    pub amount: u64,

    /// Zcash block height containing the deposit
    pub block_height: u64,

    /// Number of block confirmations (must meet minimum threshold)
    pub confirmations: u64,
}

/// Zcash withdrawal parameters
///
/// For withdrawal, the user burns wZEC on DarkFi and specifies
/// a Zcash shielded destination via a hashed recipient address.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ZcashWithdrawParams {
    /// Nullifier proving the wZEC hasn't been spent
    pub nullifier: IntentNullifier,

    /// Hash of the Zcash destination address (privacy-preserving)
    /// Can be a transparent taddr or shielded zaddr
    pub recipient_hash: [u8; 32],

    /// Whether recipient is a shielded address (zaddr) or transparent
    pub is_shielded: bool,

    /// Amount to withdraw in zatoshi
    pub amount: u64,

    /// Block height timeout - if relayer doesn't execute by this height,
    /// the withdrawal can be cancelled
    pub timeout_height: u64,

    /// ZK proof demonstrating:
    /// - Prover knows secret corresponding to the nullifier
    /// - Recipient hash is correctly computed
    pub proof: Vec<u8>,
}

// ================================================================
// AZTEC (PRIVATE ROLLUP) BRIDGING SUPPORT
// ================================================================
//
// Aztec is a private rollup on Ethereum that enables fully private
// smart contract execution. Key features for bridging:
//
// - Private transactions for ETH and ERC-20 tokens (DAI, etc.)
// - Notes are encrypted and stored as Pedersen commitments
// - Nullifiers prevent double-spending
// - Data availability posted to Ethereum L1
//
// AZT Deposit Flow:
//
// 1. User deposits ETH/DAI into Aztec bridge contract on Ethereum
// 2. Aztec rollup processes deposit and creates note commitment
// 3. Relayer observes deposit via Aztec RPC (encrypted note data)
// 4. Relayer constructs proof showing note exists in rollup tree
// 5. User submits DepositV1 with AztecDepositProof
// 6. Contract verifies rollup inclusion + note proof
// 7. Contract mints wETH/wDAI to user
//
// The Sell: "Private DAI and ETH - Aztec's private DeFi made portable"
//
// ================================================================

/// Aztec deposit proof data
///
/// This structure contains the cryptographic proofs required to verify
/// an Aztec deposit on the Ethereum rollup without revealing the
/// user's transaction details or balance.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AztecDepositProof {
    /// Nullifier derived from note (proves note hasn't been spent)
    /// Computed as: nullifier = pedersen_hash(note_secret, asset_id)
    pub nullifier: [u8; 32],

    /// Pedersen commitment to the deposit value
    /// Computed as: commitment = value * G_v + secret * G_s
    pub commitment: [u8; 32],

    /// Merkle root of the Aztec note tree at deposit rollup
    pub anchor: [u8; 32],

    /// Merkle path authenticating the note's position in the tree
    pub merkle_path: Vec<[u8; 32]>,

    /// Proof bytes demonstrating:
    /// 1. Note exists and prover knows the secret
    /// 2. Commitment is correctly computed
    /// 3. Merkle path is valid
    pub proof_bytes: Vec<u8>,

    /// Public value being deposited (for consistency check)
    pub value: u64,

    /// Asset identifier (ETH = 0, DAI = 1, etc.)
    pub asset_id: u32,

    /// Aztec rollup block height containing the deposit
    pub rollup_height: u64,

    /// Ethereum block height of rollup commitment
    pub eth_block_height: u64,

    /// Number of rollup confirmations (must meet minimum threshold)
    pub confirmations: u64,

    /// The Ethereum transaction hash where the rollup was posted
    pub rollup_tx_hash: [u8; 32],
}

/// Aztec withdrawal parameters
///
/// For withdrawal, the user burns wETH/wDAI on DarkFi and specifies
/// an Aztec destination via a hashed recipient address.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AztecWithdrawParams {
    /// Nullifier proving the wrapped token hasn't been spent
    pub nullifier: IntentNullifier,

    /// Hash of the Aztec destination address (privacy-preserving)
    pub recipient_hash: [u8; 32],

    /// Amount to withdraw
    pub amount: u64,

    /// Asset ID (ETH = 0, DAI = 1)
    pub asset_id: u32,

    /// Block height timeout - if relayer doesn't execute by this height,
    /// the withdrawal can be cancelled
    pub timeout_height: u64,

    /// ZK proof demonstrating:
    /// - Prover knows secret corresponding to the nullifier
    /// - Recipient hash is correctly computed
    pub proof: Vec<u8>,
}

// ================================================================
// LITECOIN (TRANSPARENT + MIMBLEWIMBLE) BRIDGING SUPPORT
// ================================================================
//
// Litecoin is Bitcoin's silver - it's fundamentally similar to Bitcoin
// but with faster block times and active development. Key features:
//
// - 4x faster block time than Bitcoin (2.5 min vs 10 min)
// - Lower fees, same security model
// - MimbleWimble extension blocks for privacy (MWEB)
// - Native support for confidential transactions
// - Already widely used as XMR trade pair
//
// LTC Deposit Flow:
//
// 1. User deposits LTC to DarkFi bridge address on Litecoin
// 2. Relayer observes deposit via Litecoin RPC/MWEB
// 3. Relayer constructs proof showing:
//    - Deposit exists in Litecoin blockchain
//    - Amount is verified via confidential tx or standard tx
// 4. User submits DepositV1 with LitecoinDepositProof
// 5. Contract verifies merkle proof + amount
// 6. Contract mints wLTC to user
//
// The Sell: "The Monero trade pair - move in and out of privacy with LTC"
//
// ================================================================

/// Litecoin deposit proof data
///
/// This structure contains the cryptographic proof required to verify
/// a Litecoin deposit on the Litecoin chain. Supports both standard
/// transparent UTXO deposits and MimbleWimble MWEB confidential deposits.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LitecoinDepositProof {
    /// Transaction hash of the LTC deposit
    pub tx_hash: [u8; 32],

    /// Output index proving which output is the deposit
    pub output_index: u64,

    /// Deposit amount in satoshis (smallest LTC unit, 1 LTC = 10^8 satoshis)
    pub amount: u64,

    /// Merkle proof authenticating tx inclusion in Litecoin block
    pub merkle_proof: Vec<[u8; 32]>,

    /// Merkle root of Litecoin block header
    pub block_merkle_root: [u8; 32],

    /// Block height containing the deposit
    pub block_height: u64,

    /// Number of block confirmations (must meet minimum threshold)
    pub confirmations: u64,

    /// If using MWEB/MimbleWimble: commitment to the amount
    /// This is a Pedersen commitment when using confidential transactions
    pub confidential_commitment: Option<[u8; 32]>,

    /// If using MWEB: range proof bytes for amount verification
    /// Proves the amount is within valid range without revealing it
    pub range_proof: Option<Vec<u8>>,

    /// Whether this is a confidential (MWEB) or transparent deposit
    pub is_confidential: bool,
}

/// Litecoin withdrawal parameters
///
/// For withdrawal, the user burns wLTC on DarkFi and specifies
/// a Litecoin destination via a hashed recipient address.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LitecoinWithdrawParams {
    /// Nullifier proving the wLTC hasn't been spent
    pub nullifier: IntentNullifier,

    /// Hash of the Litecoin destination address (privacy-preserving)
    /// Can be MWEB address (ltc1...) or legacy (L...)
    pub recipient_hash: [u8; 32],

    /// Whether recipient is a MWEB address
    pub is_mweb: bool,

    /// Amount to withdraw in satoshis
    pub amount: u64,

    /// Block height timeout - if relayer doesn't execute by this height,
    /// the withdrawal can be cancelled
    pub timeout_height: u64,

    /// ZK proof demonstrating:
    /// - Prover knows secret corresponding to the nullifier
    /// - Recipient hash is correctly computed
    pub proof: Vec<u8>,
}

/// XMR withdrawal parameters
///
/// For withdrawal, the user burns wXMR on DarkFi and specifies
/// a Monero destination via a hashed recipient address.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct XmrWithdrawParams {
    /// Nullifier proving the wXMR hasn't been spent
    pub nullifier: IntentNullifier,

    /// Hash of the Monero destination address (privacy-preserving)
    pub recipient_hash: [u8; 32],

    /// Amount to withdraw in piconero
    pub amount: u64,

    /// Block height timeout - if relayer doesn't execute by this height,
    /// the withdrawal can be cancelled
    pub timeout_height: u64,

    /// ZK proof demonstrating:
    /// - Prover knows secret corresponding to the nullifier
    /// - Recipient hash is correctly computed
    pub proof: Vec<u8>,
}

/// Pending withdrawal record
///
/// Tracks withdrawals that have been submitted but not yet executed.
/// This allows the timeout mechanism to work - if relayer doesn't
/// execute within the timeout, user can cancel and reclaim funds.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PendingWithdrawal {
    /// Nullifier of the withdrawal
    pub nullifier: IntentNullifier,

    /// Recipient hash on external chain
    pub recipient_hash: [u8; 32],

    /// Amount in piconero
    pub amount: u64,

    /// Timeout height - if current block > timeout_height, withdrawal can be cancelled
    pub timeout_height: u64,

    /// Relayer address that picked up this withdrawal
    pub relayer: [u8; 32],

    /// When the withdrawal was submitted
    pub submitted_at: u64,

    /// Whether cancellation has been requested
    pub cancelled: bool,
}

/// Cancellation parameters for timed-out withdrawals
///
/// When a withdrawal times out (current block > timeout_height),
/// the user can submit a cancellation to reclaim their funds.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelWithdrawParams {
    /// Nullifier of the withdrawal to cancel
    pub nullifier: IntentNullifier,

    /// Original signature or proof that this withdrawal was valid
    /// This ensures only the original submitter can cancel
    pub proof: Vec<u8>,
}

/// Relayer slash record
///
/// Records relayer misbehavior for potential slashing.
/// If a relayer fails to execute a withdrawal within timeout,
/// they can be slashed as punishment.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RelayerSlash {
    /// Relayer address
    pub relayer: [u8; 32],

    /// Withdrawal nullifier that timed out
    pub withdrawal_nullifier: IntentNullifier,

    /// Block height when timeout occurred
    pub timeout_height: u64,

    /// Slash amount (penalty for misbehavior)
    pub slash_amount: u64,

    /// Whether slash has been applied
    pub executed: bool,
}

// ================================================================
// OBJECT CAPABILITY SECURITY MODEL
// ================================================================
//
// Capability Derivation (No VSS):
//
//   bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, bridge_nonce)
//   bridge_pub = bridge_secret * G
//   bridge_address = poseidon_hash(ec_get_x(bridge_pub), ec_get_y(bridge_pub))
//
// Deposit Authorization:
//
//   commitment = poseidon_hash(secret, amount, bridge_address)
//
// Withdrawal Authorization:
//
//   nullifier = poseidon_hash(secret)
//
// The bridge contract never sees bridge_secret. Only the user knows it.
// To withdraw, user proves knowledge of secret via ZK proof.
//
// Security Properties:
//
// 1. Bridge nodes cannot steal funds (no VSS shards)
// 2. User alone authorizes withdrawals (no threshold)
// 3. Fresh addresses per deposit (temporal privacy)
// 4. Double-spend prevention via nullifiers
//
// ================================================================
