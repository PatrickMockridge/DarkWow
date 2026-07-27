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

//! Data structures for bridge contract calls
//!
//! Security Model: Object Capability Security (No VSS)
//!
//! Unlike VSS-based bridges, this design uses deterministic address derivation:
//! - Bridge address = H(recipient_identity, nonce)
//! - No secret sharing between bridge nodes
//! - User alone controls withdrawal via their secret

use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::{group::GroupEncoding, pallas};
use dwow_serial::{SerialDecodable, SerialEncodable};
use dwow_sdk::crypto::{IntentCommitment, IntentNullifier, PublicKey};

/// Deterministic bridge address: poseidon_hash(recipient_pub.xy(), nonce)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct BridgeAddress(pallas::Base);
impl BridgeAddress {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(b: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(b).into_option().map(Self)
    }
}

/// Per-chain balance sheet entry for a governance report
#[derive(Debug, Clone)]
pub struct ChainBalanceEntry {
    /// External chain
    pub chain: ExternalChain,
    /// Total deposited (wrapped tokens minted)
    pub total_deposited: u64,
    /// Total withdrawn (wrapped tokens burned)
    pub total_withdrawn: u64,
    /// Outstanding = total_deposited - total_withdrawn
    pub outstanding: u64,
}

/// Namespace for bridge intents (used with generic intent primitives)
pub const BRIDGE_NAMESPACE: u64 = 0x0002;

/// External chain identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChain {
    Ethereum,
    Monero,
    Zcash,
    Aztec,
    Litecoin,
    // Future chains can be added here
    // Bitcoin,
}

impl TryFrom<u8> for ExternalChain {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Ethereum),
            1 => Ok(Self::Monero),
            2 => Ok(Self::Zcash),
            3 => Ok(Self::Aztec),
            4 => Ok(Self::Litecoin),
            _ => Err(ContractError::IoError(
                format!("ExternalChain: unknown discriminant {}", b),
            )),
        }
    }
}

impl dwow_serial::Encodable for ExternalChain {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let b = *self as u8;
        w.write_all(&[b])?;
        Ok(1)
    }
}

impl dwow_serial::Decodable for ExternalChain {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 1];
        d.read_exact(&mut buf)?;
        Self::try_from(buf[0]).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncEncodable for ExternalChain {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let b = *self as u8;
        use dwow_serial::AsyncWriteExt;
        w.write_slice_async(&[b]).await?;
        Ok(1)
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncDecodable for ExternalChain {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 1];
        use dwow_serial::AsyncReadExt;
        d.read_slice_async(&mut buf).await?;
        Self::try_from(buf[0]).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

/// Chain-specific deposit proof data.
///
/// Each variant carries the proof data needed to verify a deposit on that
/// chain.  Replaces the four `Option<T>` fields that previously existed on
/// `DepositParams` — the enum guarantees exactly one proof is present and
/// makes the chain type self-evident from the variant.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum ExternalChainProof {
    Monero(XmrDepositProof),
    Zcash(ZcashDepositProof),
    Aztec(AztecDepositProof),
    Litecoin(LitecoinDepositProof),
    /// Ethereum deposits use the merkle_proof field on DepositParams
    /// instead of a chain-specific proof structure.
    Ethereum,
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
    pub recipient_pub: PublicKey,

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

    /// Chain-specific deposit proof (Monero, Zcash, Aztec, or Litecoin).
    /// The enum variant identifies the external chain, so `ExternalChain` on
    /// `DepositParams` only needs to be `Ethereum` when there is no variant.
    pub chain_proof: ExternalChainProof,
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

    /// Deposit leaf = poseidon_hash(secret, amount) — public input for ZK proof
    pub deposit_leaf: pallas::Base,

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

    /// Block height after which the withdrawal can be cancelled if not executed
    pub timeout_height: u64,

    /// Feed mode: 0 = standard (fee only), 1 = guaranteed (fee + premium)
    pub feed_mode: u8,

    /// Optional user-specified max fee in basis points (0 = use contract default)
    pub max_fee_bp: Option<u64>,
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

    /// Governance public key X (ZK-verified)
    pub gov_pub_x: pallas::Base,
    /// Governance public key Y (ZK-verified)
    pub gov_pub_y: pallas::Base,
    /// Nullifier = H(gov_pub_x, gov_pub_y, gov_secret) for ZK replay protection
    pub config_nullifier: pallas::Base,
}

/// Stored deposit record
///
/// This record tracks deposits registered in the bridge.
/// The actual proof of deposit ownership is via the commitment
/// which requires knowledge of secret to claim.
#[derive(Debug, Clone)]
pub struct Deposit {
    pub version: u8,
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
#[derive(Debug, Clone)]
pub struct Withdrawal {
    pub version: u8,
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
/// For withdrawal, the user burns wZEC on DarkWow and specifies
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
/// For withdrawal, the user burns wETH/wDAI on DarkWow and specifies
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
// 1. User deposits LTC to DarkWow bridge address on Litecoin
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
/// For withdrawal, the user burns wLTC on DarkWow and specifies
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
/// For withdrawal, the user burns wXMR on DarkWow and specifies
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
#[derive(Debug, Clone)]
pub struct PendingWithdrawal {
    pub version: u8,
    /// Nullifier of the withdrawal
    pub nullifier: IntentNullifier,

    /// Recipient hash on external chain
    pub recipient_hash: [u8; 32],

    /// Amount in piconero
    pub amount: u64,

    /// Timeout height - if current block > timeout_height, withdrawal can be cancelled
    pub timeout_height: u64,

    /// Relayer address that picked up this withdrawal
    pub relayer: Option<PublicKey>,

    /// When the withdrawal was submitted
    pub submitted_at: u64,

    /// Whether cancellation has been requested
    pub cancelled: bool,

    /// Feed mode: 0=standard (fee only), 1=guaranteed (fee + premium)
    pub feed_mode: u8,

    /// Guarantee premium paid upfront (refunded on successful execution)
    pub guarantee_premium: u64,

    /// Pool stake coverage allocation ID (for guaranteed withdrawals)
    pub stake_lock_id: Option<[u8; 32]>,

    /// Block height after which another relayer can reassign this withdrawal
    pub reassignable_after: Option<u64>,

    /// Last heartbeat block from assigned relayer
    pub heartbeat_at: Option<u64>,
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

    /// Current block height (for timeout verification)
    pub current_block: u64,
}

/// Parameters for executing a guaranteed withdrawal with pool stake coverage
///
/// For guaranteed withdrawals, the relayer must prove they have allocated
/// coverage from the pool_stake contract before execution is allowed.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteGuaranteedWithdrawParams {
    /// Nullifier of the withdrawal to execute
    pub nullifier: IntentNullifier,

    /// ZK proof demonstrating:
    /// 1. Knowledge of secret for the nullifier
    /// 2. Pool stake coverage was allocated for this withdrawal
    pub pool_stake_proof: Vec<u8>,

    /// Relayer signature authorizing this execution
    pub relayer_sig: Vec<u8>,

    /// External chain-specific execution data (tx hash, etc)
    pub execution_data: Vec<u8>,
}

/// Parameters for reassigning a withdrawal to a new relayer
///
/// When a relayer has been offline past `reassignable_after`,
/// another relayer can claim the withdrawal. The original relayer's
/// stake is partially slashed for abandonment.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ReassignWithdrawalParamsV1 {
    /// Nullifier of the withdrawal to reassign
    pub nullifier: IntentNullifier,

    /// New relayer address taking over
    pub new_relayer: PublicKey,

    /// Current block height for timeout verification
    pub current_block: u64,
}

/// Update for reassigning a withdrawal to a new relayer
#[derive(Debug, Clone)]
pub struct ReassignWithdrawalUpdateV1 {
    /// Nullifier of the withdrawal
    pub nullifier: IntentNullifier,

    /// New relayer taking over
    pub new_relayer: PublicKey,
}

// ============================================================================
// HTLC TYPES (for Cross-Chain Atomic Swaps)
// ============================================================================

/// Parameters for creating an HTLC that coordinates with atomic swap
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateHtlcParams {
    /// Swap ID (matches atomic_swap SwapId)
    pub swap_id: [u8; 32],
    /// Hash that locks the HTLC (poseidon_hash(secret))
    pub hash: dwow_sdk::pasta::pallas::Base,
    /// Timelock block height (after which refund is allowed)
    pub timelock: u64,
    /// Amount locked in HTLC
    pub amount: u64,
    /// Recipient's address on external chain
    pub external_recipient: Vec<u8>,
    /// External chain to create HTLC on
    pub chain: ExternalChain,
    /// Deposit proof from external chain (merkle proof, etc)
    pub deposit_proof: Vec<u8>,
}

/// Parameters for claiming an HTLC (when secret is revealed)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimHtlcParams {
    /// Swap ID of the HTLC
    pub swap_id: [u8; 32],
    /// The secret that unlocks the HTLC
    pub secret: dwow_sdk::pasta::pallas::Base,
}

/// Parameters for refunding an HTLC (after timelock)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundHtlcParams {
    /// Swap ID of the HTLC
    pub swap_id: [u8; 32],
    /// Current block height (for timelock verification)
    pub current_block: u64,
}

/// Update data for HTLC creation
#[derive(Debug, Clone)]
pub struct CreateHtlcUpdateV1 {
    pub swap_id: [u8; 32],
    pub hash: dwow_sdk::pasta::pallas::Base,
    pub timelock: u64,
    pub amount: u64,
    pub external_sender: Vec<u8>,
    pub external_recipient: Vec<u8>,
    pub chain: ExternalChain,
}

/// Update data for HTLC claim
#[derive(Debug, Clone)]
pub struct ClaimHtlcUpdateV1 {
    pub swap_id: [u8; 32],
    pub secret: dwow_sdk::pasta::pallas::Base,
}

/// Update data for HTLC refund
#[derive(Debug, Clone)]
pub struct RefundHtlcUpdateV1 {
    pub swap_id: [u8; 32],
}

/// Update data for withdrawal cancellation
#[derive(Debug, Clone)]
pub struct CancelWithdrawUpdateV1 {
    pub nullifier: IntentNullifier,
}

/// Update data for guaranteed withdrawal execution
#[derive(Debug, Clone)]
pub struct ExecuteGuaranteedWithdrawUpdateV1 {
    pub nullifier: IntentNullifier,
}

/// HTLC state enum for database storage
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum HtlcSwapState {
    Pending = 0,
    Claimable = 1,
    Claimed = 2,
    Refunded = 3,
}

/// HTLC info stored in database
#[derive(Debug, Clone)]
pub struct HtlcSwapInfo {
    pub version: u8,
    pub swap_id: [u8; 32],
    pub hash: dwow_sdk::pasta::pallas::Base,
    pub timelock: u64,
    pub amount: u64,
    pub external_sender: Vec<u8>,
    pub external_recipient: Vec<u8>,
    pub state: u8,  // HtlcSwapState as u8
    pub created_at: u64,
    pub claimed_at: Option<u64>,
    pub refunded_at: Option<u64>,
}

/// Relayer slash record
///
/// Records relayer misbehavior for potential slashing.
/// If a relayer fails to execute a withdrawal within timeout,
/// they can be slashed as punishment.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RelayerSlash {
    /// Relayer address
    pub relayer: Option<PublicKey>,

    /// Withdrawal nullifier that timed out
    pub withdrawal_nullifier: IntentNullifier,

    /// Block height when timeout occurred
    pub timeout_height: u64,

    /// Slash amount (penalty for misbehavior)
    pub slash_amount: u64,

    /// Whether slash has been applied
    pub executed: bool,
}

// ============================================================================
// RELAYER REGISTRY (Phase 2d hardening)
// ============================================================================

/// Parameters for registering a relayer with the bridge
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterRelayerParams {
    /// Relayer's public key
    pub relayer_pub: PublicKey,
}

/// Stored relayer info
#[derive(Debug, Clone)]
pub struct RelayerInfo {
    pub version: u8,
    pub pubkey: PublicKey,
    pub registered_at: u64,
    pub total_slashed: u64,
    pub total_withdrawals: u64,
    pub total_successful: u64,
    pub is_active: bool,
    pub fee_schedule_id: Option<[u8; 32]>,
}

/// Register relayer update
#[derive(Debug, Clone)]
pub struct RegisterRelayerUpdateV1 {
    pub relayer_pub: PublicKey,
    pub registered_at: u64,
}

// ============================================================================
// WITHDRAWAL ACCEPTANCE (Phase 2d hardening)
// ============================================================================

/// Parameters for a relayer accepting a pending withdrawal
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AcceptWithdrawalParams {
    /// Nullifier of the withdrawal to accept
    pub nullifier: IntentNullifier,
    /// Relayer's public key
    pub relayer_pub: PublicKey,
    /// Committed max fee in basis points (binding — exceeding = slashable)
    pub max_fee_bp: u64,
}

/// Accept withdrawal update
#[derive(Debug, Clone)]
pub struct AcceptWithdrawalUpdateV1 {
    pub nullifier: IntentNullifier,
    pub relayer_pub: PublicKey,
    pub max_fee_bp: u64,
    pub accepted_at: u64,
}

// ============================================================================
// REPUTATION VERIFICATION (Phase 2d hardening)
// ============================================================================

/// Parameters for verifying a relayer's reputation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyRelayerReputationParams {
    /// Relayer's public key to check
    pub relayer_pub: PublicKey,
}

/// Reputation info returned to caller
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ReputationInfo {
    pub slash_count: u64,
    pub success_count: u64,
    pub total_volume: u64,
    pub settlement_frequency: u64,
    pub is_registered: bool,
}

// ============================================================================
// FEE SCHEDULE (Phase 3 hardening)
// ============================================================================

/// Parameters for registering a fee schedule
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterFeeScheduleParams {
    /// Relayer's public key
    pub relayer_pub: PublicKey,
    /// Fee schedule attestation ID (from attestation contract)
    pub fee_schedule_id: [u8; 32],
}

/// Register fee schedule update
#[derive(Debug, Clone)]
pub struct RegisterFeeScheduleUpdateV1 {
    pub relayer_pub: PublicKey,
    pub fee_schedule_id: [u8; 32],
}

// ============================================================================
// GOVERNANCE REPORT (Cold/Precise — BaseDiv)
// ============================================================================

/// Governance report parameters for the bridge contract.
///
/// The reporter provides balance sheet data and a ZK proof that verifies
/// `outstanding = total_deposited - total_withdrawn` against the on-chain
/// config DB counters.
///
/// Unlike the stablecoin, the bridge cannot prove on-chain that external
/// chain deposits exist — those live on BTC/XMR/ZEC/etc. The governance
/// report proves **internal accounting consistency**: the bridge is not
/// minting unbacked wrapped tokens out of thin air.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GovernanceReportParams {
    /// External chain being reported on
    pub chain: ExternalChain,

    /// Reported total deposited (wrapped tokens minted, must match on-chain config)
    pub total_deposited: u64,

    /// Reported total withdrawn (wrapped tokens burned, must match on-chain config)
    pub total_withdrawn: u64,

    /// Outstanding = total_deposited - total_withdrawn
    pub outstanding: u64,

    /// Reporter's public key
    pub reporter_pub: PublicKey,

    /// ZK proof verifying outstanding = total_deposited - total_withdrawn
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Update data for governance report — persisted on-chain for public audit
#[derive(Debug, Clone)]
pub struct GovernanceReportUpdateV1 {
    /// External chain reported on
    pub chain: ExternalChain,

    /// Total deposited verified on-chain
    pub total_deposited: u64,

    /// Total withdrawn verified on-chain
    pub total_withdrawn: u64,

    /// Outstanding = total_deposited - total_withdrawn
    pub outstanding: u64,

    /// Block height when report was created
    pub report_block: u64,

    /// Reporter's public key
    pub reporter_pub: PublicKey,
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

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

impl ChainBalanceEntry {
    pub const ENCODED_SIZE: usize = 25;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(25); b.push(self.chain as u8); b.extend_from_slice(&self.total_deposited.to_le_bytes()); b.extend_from_slice(&self.total_withdrawn.to_le_bytes()); b.extend_from_slice(&self.outstanding.to_le_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 25 { return Err(ContractError::IoError(format!("ChainBalanceEntry: expected 25 bytes, got {}", data.len()))); }
        Ok(ChainBalanceEntry { chain: ExternalChain::try_from(data[0])?, total_deposited: u64::from_le_bytes(data[1..9].try_into().unwrap()), total_withdrawn: u64::from_le_bytes(data[9..17].try_into().unwrap()), outstanding: u64::from_le_bytes(data[17..25].try_into().unwrap()) })
    }
}

impl Deposit {
    pub const ENCODED_SIZE: usize = 59;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(59);
        b.push(self.version);
        b.extend_from_slice(&self.commitment.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.chain as u8);
        b.extend_from_slice(&self.external_height.to_le_bytes());
        b.push(self.claimed as u8);
        b.extend_from_slice(&self.registered_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 59 { return Err(ContractError::IoError(format!("Deposit: expected 59 bytes, got {}", data.len()))); }
        Ok(Deposit { version: data[0], commitment: IntentCommitment::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Deposit: invalid commitment: {}", e)))?, amount: u64::from_le_bytes(data[33..41].try_into().unwrap()), chain: ExternalChain::try_from(data[41])?, external_height: u64::from_le_bytes(data[42..50].try_into().unwrap()), claimed: data[50] != 0, registered_at: u64::from_le_bytes(data[51..59].try_into().unwrap()) })
    }
}

impl Withdrawal {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 82 + if self.external_tx_hash.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.recipient_hash);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.executed as u8);
        b.push(self.external_tx_hash.is_some() as u8);
        if let Some(ref h) = self.external_tx_hash { b.extend_from_slice(h); }
        b.extend_from_slice(&self.withdrawn_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 82 { return Err(ContractError::IoError(format!("Withdrawal: expected at least 82 bytes, got {}", data.len()))); }
        let version = data[0];
        let nullifier = IntentNullifier::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Withdrawal: invalid nullifier: {}", e)))?;
        let recipient_hash: [u8; 32] = data[33..65].try_into().unwrap();
        let amount = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let executed = data[73] != 0;
        let has_tx = data[74] != 0;
        let (external_tx_hash, pos) = if has_tx {
            (Some(data[75..107].try_into().unwrap()), 107usize)
        } else { (None, 75usize) };
        let withdrawn_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        Ok(Withdrawal { version, nullifier, recipient_hash, amount, executed, external_tx_hash, withdrawn_at })
    }
}

impl PendingWithdrawal {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 103 + if self.relayer.is_some() { 32 } else { 0 }
            + if self.stake_lock_id.is_some() { 32 } else { 0 }
            + if self.reassignable_after.is_some() { 8 } else { 0 }
            + if self.heartbeat_at.is_some() { 8 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.recipient_hash);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.timeout_height.to_le_bytes());
        b.push(self.relayer.is_some() as u8);
        if let Some(ref r) = self.relayer { b.extend_from_slice(&r.to_bytes()); }
        b.extend_from_slice(&self.submitted_at.to_le_bytes());
        b.push(self.cancelled as u8);
        b.push(self.feed_mode);
        b.extend_from_slice(&self.guarantee_premium.to_le_bytes());
        b.push(self.stake_lock_id.is_some() as u8);
        if let Some(ref id) = self.stake_lock_id { b.extend_from_slice(id); }
        b.push(self.reassignable_after.is_some() as u8);
        if let Some(ra) = self.reassignable_after { b.extend_from_slice(&ra.to_le_bytes()); }
        b.push(self.heartbeat_at.is_some() as u8);
        if let Some(hb) = self.heartbeat_at { b.extend_from_slice(&hb.to_le_bytes()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 103 { return Err(ContractError::IoError(format!("PendingWithdrawal: expected at least 103 bytes, got {}", data.len()))); }
        let version = data[0];
        let nullifier = IntentNullifier::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PendingWithdrawal: invalid nullifier: {}", e)))?;
        let recipient_hash: [u8; 32] = data[33..65].try_into().unwrap();
        let amount = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let timeout_height = u64::from_le_bytes(data[73..81].try_into().unwrap());
        let has_relayer = data[81] != 0;
        let (relayer, mut pos) = if has_relayer { (Some(PublicKey::from_bytes(data[82..114].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PendingWithdrawal: invalid relayer: {}", e)))?), 114usize) } else { (None, 82usize) };
        let submitted_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let cancelled = data[pos] != 0; pos += 1;
        let feed_mode = data[pos]; pos += 1;
        let guarantee_premium = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let has_stake = data[pos] != 0; pos += 1;
        let (stake_lock_id, mut pos) = if has_stake { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        let has_reassign = data[pos] != 0; pos += 1;
        let (reassignable_after, mut pos) = if has_reassign { (Some(u64::from_le_bytes(data[pos..pos+8].try_into().unwrap())), pos + 8) } else { (None, pos) };
        let has_hb = data[pos] != 0; pos += 1;
        let (heartbeat_at, _) = if has_hb { (Some(u64::from_le_bytes(data[pos..pos+8].try_into().unwrap())), pos + 8) } else { (None, pos) };
        Ok(PendingWithdrawal { version, nullifier, recipient_hash, amount, timeout_height, relayer, submitted_at, cancelled, feed_mode, guarantee_premium, stake_lock_id, reassignable_after, heartbeat_at })
    }
}

impl HtlcSwapInfo {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 82 + self.external_sender.len() + self.external_recipient.len()
            + if self.claimed_at.is_some() { 8 } else { 0 }
            + if self.refunded_at.is_some() { 8 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.hash.to_repr());
        b.extend_from_slice(&self.timelock.to_le_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.external_sender.len() as u8);
        b.extend_from_slice(&self.external_sender);
        b.push(self.external_recipient.len() as u8);
        b.extend_from_slice(&self.external_recipient);
        b.push(self.state);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.push(self.claimed_at.is_some() as u8);
        if let Some(c) = self.claimed_at { b.extend_from_slice(&c.to_le_bytes()); }
        b.push(self.refunded_at.is_some() as u8);
        if let Some(r) = self.refunded_at { b.extend_from_slice(&r.to_le_bytes()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 82 { return Err(ContractError::IoError(format!("HtlcSwapInfo: expected at least 82 bytes, got {}", data.len()))); }
        let version = data[0];
        let swap_id: [u8; 32] = data[1..33].try_into().unwrap();
        let hash = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("HtlcSwapInfo: invalid hash".into()))?;
        let timelock = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let amount = u64::from_le_bytes(data[73..81].try_into().unwrap());
        let sender_len = data[81] as usize;
        if data.len() < 82 + sender_len + 1 { return Err(ContractError::IoError("HtlcSwapInfo: data too short".into())); }
        let external_sender = data[82..82+sender_len].to_vec();
        let recip_len = data[82+sender_len] as usize;
        let mut pos = 83 + sender_len + recip_len;
        if data.len() < pos { return Err(ContractError::IoError("HtlcSwapInfo: data too short for recipient".into())); }
        let external_recipient = data[83+sender_len..pos].to_vec();
        let state = data[pos]; pos += 1;
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let has_claimed = data[pos] != 0; pos += 1;
        let (claimed_at, mut pos) = if has_claimed { (Some(u64::from_le_bytes(data[pos..pos+8].try_into().unwrap())), pos + 8) } else { (None, pos) };
        let has_refunded = data[pos] != 0; pos += 1;
        let (refunded_at, _) = if has_refunded { (Some(u64::from_le_bytes(data[pos..pos+8].try_into().unwrap())), pos + 8) } else { (None, pos) };
        Ok(HtlcSwapInfo { version, swap_id, hash, timelock, amount, external_sender, external_recipient, state, created_at, claimed_at, refunded_at })
    }
}

impl RelayerInfo {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 66 + if self.fee_schedule_id.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.pubkey.to_bytes());
        b.extend_from_slice(&self.registered_at.to_le_bytes());
        b.extend_from_slice(&self.total_slashed.to_le_bytes());
        b.extend_from_slice(&self.total_withdrawals.to_le_bytes());
        b.extend_from_slice(&self.total_successful.to_le_bytes());
        b.push(self.is_active as u8);
        b.push(self.fee_schedule_id.is_some() as u8);
        if let Some(ref id) = self.fee_schedule_id { b.extend_from_slice(id); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 66 { return Err(ContractError::IoError(format!("RelayerInfo: expected at least 66 bytes, got {}", data.len()))); }
        Ok(RelayerInfo { version: data[0], pubkey: PublicKey::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RelayerInfo: invalid pubkey: {}", e)))?, registered_at: u64::from_le_bytes(data[33..41].try_into().unwrap()), total_slashed: u64::from_le_bytes(data[41..49].try_into().unwrap()), total_withdrawals: u64::from_le_bytes(data[49..57].try_into().unwrap()), total_successful: u64::from_le_bytes(data[57..65].try_into().unwrap()), is_active: data[65] != 0, fee_schedule_id: if data[66] != 0 { Some(data[67..99].try_into().unwrap()) } else { None } })
    }
}

// --- Bridge update structs ---

impl ReassignWithdrawalUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.new_relayer.to_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 { return Err(ContractError::IoError(format!("ReassignWithdrawalUpdateV1: expected 64 bytes, got {}", data.len()))); }
        Ok(ReassignWithdrawalUpdateV1 { nullifier: IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ReassignWithdrawalUpdateV1: invalid nullifier: {}", e)))?, new_relayer: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ReassignWithdrawalUpdateV1: invalid new_relayer: {}", e)))? })
    }
}

impl RefundHtlcUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { self.swap_id.to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("RefundHtlcUpdateV1: expected 32 bytes, got {}", data.len()))); }
        Ok(RefundHtlcUpdateV1 { swap_id: data[0..32].try_into().unwrap() })
    }
}

impl CancelWithdrawUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { self.nullifier.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("CancelWithdrawUpdateV1: expected 32 bytes, got {}", data.len()))); }
        Ok(CancelWithdrawUpdateV1 { nullifier: IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CancelWithdrawUpdateV1: invalid nullifier: {}", e)))? })
    }
}

impl ExecuteGuaranteedWithdrawUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { self.nullifier.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("ExecuteGuaranteedWithdrawUpdateV1: expected 32 bytes, got {}", data.len()))); }
        Ok(ExecuteGuaranteedWithdrawUpdateV1 { nullifier: IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ExecuteGuaranteedWithdrawUpdateV1: invalid nullifier: {}", e)))? })
    }
}

impl RegisterRelayerUpdateV1 {
    pub const ENCODED_SIZE: usize = 40;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(40); b.extend_from_slice(&self.relayer_pub.to_bytes()); b.extend_from_slice(&self.registered_at.to_le_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 40 { return Err(ContractError::IoError(format!("RegisterRelayerUpdateV1: expected 40 bytes, got {}", data.len()))); }
        Ok(RegisterRelayerUpdateV1 { relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RegisterRelayerUpdateV1: invalid relayer_pub: {}", e)))?, registered_at: u64::from_le_bytes(data[32..40].try_into().unwrap()) })
    }
}

impl ClaimHtlcUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.swap_id); b.extend_from_slice(&self.secret.to_repr()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 { return Err(ContractError::IoError(format!("ClaimHtlcUpdateV1: expected 64 bytes, got {}", data.len()))); }
        Ok(ClaimHtlcUpdateV1 { swap_id: data[0..32].try_into().unwrap(), secret: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ClaimHtlcUpdateV1: invalid secret".into()))? })
    }
}

impl CreateHtlcUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 73 + self.external_sender.len() + self.external_recipient.len();
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.hash.to_repr());
        b.extend_from_slice(&self.timelock.to_le_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.chain as u8);
        b.push(self.external_sender.len() as u8);
        b.extend_from_slice(&self.external_sender);
        b.push(self.external_recipient.len() as u8);
        b.extend_from_slice(&self.external_recipient);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 73 { return Err(ContractError::IoError(format!("CreateHtlcUpdateV1: expected at least 73 bytes, got {}", data.len()))); }
        let swap_id: [u8; 32] = data[0..32].try_into().unwrap();
        let hash = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateHtlcUpdateV1: invalid hash".into()))?;
        let timelock = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let amount = u64::from_le_bytes(data[72..80].try_into().unwrap());
        let chain = ExternalChain::try_from(data[80])?;
        let sender_len = data[81] as usize;
        if data.len() < 82 + sender_len + 1 { return Err(ContractError::IoError("CreateHtlcUpdateV1: data too short".into())); }
        let external_sender = data[82..82+sender_len].to_vec();
        let recip_len = data[82+sender_len] as usize;
        let expected = 83 + sender_len + recip_len;
        if data.len() != expected { return Err(ContractError::IoError(format!("CreateHtlcUpdateV1: expected {} bytes, got {}", expected, data.len()))); }
        let external_recipient = data[83+sender_len..expected].to_vec();
        Ok(CreateHtlcUpdateV1 { swap_id, hash, timelock, amount, external_sender, external_recipient, chain })
    }
}

impl AcceptWithdrawalUpdateV1 {
    pub const ENCODED_SIZE: usize = 80;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(80);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.max_fee_bp.to_le_bytes());
        b.extend_from_slice(&self.accepted_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 80 { return Err(ContractError::IoError(format!("AcceptWithdrawalUpdateV1: expected 80 bytes, got {}", data.len()))); }
        Ok(AcceptWithdrawalUpdateV1 {
            nullifier: IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("AcceptWithdrawalUpdateV1: invalid nullifier: {}", e)))?,
            relayer_pub: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("AcceptWithdrawalUpdateV1: invalid relayer_pub: {}", e)))?,
            max_fee_bp: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            accepted_at: u64::from_le_bytes(data[72..80].try_into().unwrap()),
        })
    }
}

impl RegisterFeeScheduleUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.fee_schedule_id);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 64 { return Err(ContractError::IoError(format!("RegisterFeeScheduleUpdateV1: expected 64 bytes, got {}", data.len()))); }
        Ok(RegisterFeeScheduleUpdateV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RegisterFeeScheduleUpdateV1: invalid relayer_pub: {}", e)))?,
            fee_schedule_id: data[32..64].try_into().unwrap(),
        })
    }
}

impl GovernanceReportUpdateV1 {
    pub const ENCODED_SIZE: usize = 65;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(65);
        b.push(self.chain as u8);
        b.extend_from_slice(&self.total_deposited.to_le_bytes());
        b.extend_from_slice(&self.total_withdrawn.to_le_bytes());
        b.extend_from_slice(&self.outstanding.to_le_bytes());
        b.extend_from_slice(&self.report_block.to_le_bytes());
        b.extend_from_slice(&self.reporter_pub.to_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 65 { return Err(ContractError::IoError(format!("GovernanceReportUpdateV1: expected 65 bytes, got {}", data.len()))); }
        Ok(GovernanceReportUpdateV1 {
            chain: ExternalChain::try_from(data[0])?,
            total_deposited: u64::from_le_bytes(data[1..9].try_into().unwrap()),
            total_withdrawn: u64::from_le_bytes(data[9..17].try_into().unwrap()),
            outstanding: u64::from_le_bytes(data[17..25].try_into().unwrap()),
            report_block: u64::from_le_bytes(data[25..33].try_into().unwrap()),
            reporter_pub: PublicKey::from_bytes(data[33..65].try_into().unwrap())?,
        })
    }
}
