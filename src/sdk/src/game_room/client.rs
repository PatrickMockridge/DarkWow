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

//! Game Room SDK Client
//!
//! High-level client for interacting with the Game Room contract.
//! This provides a convenient API for app developers to:
//! - Create and join game rooms
//! - Deposit/withdraw stake
//! - Place bets, raises, calls, folds
//! - Manage entropy contributions
//! - Claim winnings
//!
//! ## Usage
//!
//! ```ignore
//! use darkfi_sdk::game_room::{GameRoomClient, RoomConfig, EntropyMode};
//!
//! // Create a client
//! let client = GameRoomClient::new(
//!     "http://localhost:8080",
//!     contract_id,
//!     keypair,
//! );
//!
//! // Create a room
//! let config = RoomConfig::new(owner_dao, token_id, 100, 10000, EntropyMode::BlockHash);
//! let room_id = client.create_room(config).await?;
//!
//! // Join the room
//! client.deposit(room_id, 500).await?;
//!
//! // Place a bet
//! client.place_bet(room_id, 100, BetType::Ante, nonce).await?;
//! ```

use crate::{
    crypto::{ContractId, Keypair, PublicKey},
    pasta::pallas,
    tx::ContractCall,
};

use super::types::*;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Game Room contract function IDs
#[derive(Debug, Clone, Copy)]
pub enum GameRoomFunction {
    CreateRoomV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    PlaceBetV1 = 0x03,
    RaiseV1 = 0x04,
    CallV1 = 0x05,
    FoldV1 = 0x06,
    ClosePotV1 = 0x07,
    SettlePotV1 = 0x08,
    ContributeEntropyV1 = 0x09,
    ClaimV1 = 0x0A,
}

impl GameRoomFunction {
    fn as_u8(&self) -> u8 {
        match self {
            Self::CreateRoomV1 => 0x00,
            Self::DepositV1 => 0x01,
            Self::WithdrawV1 => 0x02,
            Self::PlaceBetV1 => 0x03,
            Self::RaiseV1 => 0x04,
            Self::CallV1 => 0x05,
            Self::FoldV1 => 0x06,
            Self::ClosePotV1 => 0x07,
            Self::SettlePotV1 => 0x08,
            Self::ContributeEntropyV1 => 0x09,
            Self::ClaimV1 => 0x0A,
        }
    }
}

// ============================================================================
// CLIENT
// ============================================================================

/// Game Room client for app developers
///
/// Provides a high-level interface for interacting with game rooms.
/// Construct transactions locally and broadcast via RPC.
#[derive(Debug, Clone)]
pub struct GameRoomClient {
    /// RPC endpoint URL
    rpc_url: String,
    /// Game Room contract ID
    contract_id: ContractId,
    /// User's keypair for signing
    keypair: Keypair,
}

impl GameRoomClient {
    // ============================================================================
    // INITIALIZATION
    // ============================================================================

    /// Create a new GameRoomClient
    pub fn new(rpc_url: &str, contract_id: ContractId, keypair: Keypair) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            contract_id,
            keypair,
        }
    }

    /// Get the RPC URL
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    /// Get the contract ID
    pub fn contract_id(&self) -> ContractId {
        self.contract_id
    }

    /// Get the user's public key
    pub fn pubkey(&self) -> PublicKey {
        self.keypair.public
    }

    // ============================================================================
    // TRANSACTION BUILDERS
    // ============================================================================

    /// Build a CreateRoomV1 transaction
    pub fn build_create_room_tx(&self, params: CreateRoomParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::CreateRoomV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a DepositV1 transaction
    pub fn build_deposit_tx(&self, params: DepositParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::DepositV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a WithdrawV1 transaction
    pub fn build_withdraw_tx(&self, params: WithdrawParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::WithdrawV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a PlaceBetV1 transaction
    pub fn build_place_bet_tx(&self, params: PlaceBetParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::PlaceBetV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a RaiseV1 transaction
    pub fn build_raise_tx(&self, params: RaiseParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::RaiseV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a CallV1 transaction
    pub fn build_call_tx(&self, params: CallParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::CallV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a FoldV1 transaction
    pub fn build_fold_tx(&self, params: FoldParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::FoldV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a ClosePotV1 transaction
    pub fn build_close_pot_tx(&self, params: ClosePotParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::ClosePotV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a SettlePotV1 transaction
    pub fn build_settle_pot_tx(&self, params: SettlePotParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::SettlePotV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a ContributeEntropyV1 transaction
    pub fn build_contribute_entropy_tx(&self, params: ContributeEntropyParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::ContributeEntropyV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    /// Build a ClaimV1 transaction
    pub fn build_claim_tx(&self, params: ClaimParams) -> ContractCall {
        let mut data = vec![GameRoomFunction::ClaimV1.as_u8()];
        data.extend_from_slice(&darkfi_serial::serialize(&params));
        ContractCall { contract_id: self.contract_id, data }
    }

    // ============================================================================
    // CONVENIENCE METHODS
    // ============================================================================

    /// Create a new room with default settings
    pub fn create_room(
        &self,
        owner: PublicKey,
        token_id: pallas::Base,
        min_stake: u64,
        max_stake: u64,
        entropy_mode: EntropyMode,
        nonce: pallas::Base,
    ) -> ContractCall {
        let params = CreateRoomParams {
            owner,
            token_id,
            min_stake,
            max_stake,
            entropy_config: EntropyConfig { mode: entropy_mode, ..Default::default() },
            max_players: 10,
            nonce,
        };
        self.build_create_room_tx(params)
    }

    /// Deposit stake into a room
    pub fn deposit(&self, room_id: RoomId, amount: u64) -> ContractCall {
        let params = DepositParams { room_id, player: self.pubkey(), amount };
        self.build_deposit_tx(params)
    }

    /// Withdraw from a room
    pub fn withdraw(&self, room_id: RoomId, amount: u64) -> ContractCall {
        let params = WithdrawParams { room_id, player: self.pubkey(), amount };
        self.build_withdraw_tx(params)
    }

    /// Place a bet
    pub fn place_bet(&self, room_id: RoomId, amount: u64, bet_type: BetType, nonce: pallas::Base) -> ContractCall {
        let params = PlaceBetParams {
            room_id,
            player: self.pubkey(),
            amount,
            bet_type,
            nonce,
        };
        self.build_place_bet_tx(params)
    }

    /// Raise the current bet
    pub fn raise(&self, room_id: RoomId, amount: u64, nonce: pallas::Base) -> ContractCall {
        let params = RaiseParams { room_id, player: self.pubkey(), amount, nonce };
        self.build_raise_tx(params)
    }

    /// Call the current bet
    pub fn call(&self, room_id: RoomId, nonce: pallas::Base) -> ContractCall {
        let params = CallParams { room_id, player: self.pubkey(), nonce };
        self.build_call_tx(params)
    }

    /// Fold (forfeit the hand)
    pub fn fold(&self, room_id: RoomId) -> ContractCall {
        let params = FoldParams { room_id, player: self.pubkey() };
        self.build_fold_tx(params)
    }

    /// Close the pot (room owner)
    pub fn close_pot(&self, room_id: RoomId, pot_id: PotId) -> ContractCall {
        let params = ClosePotParams { room_id, pot_id };
        self.build_close_pot_tx(params)
    }

    /// Settle pot to winners (room owner DAO)
    pub fn settle_pot(
        &self,
        room_id: RoomId,
        pot_id: PotId,
        winners: Vec<(PublicKey, u64)>,
        signature: Vec<u8>,
    ) -> ContractCall {
        let params = SettlePotParams {
            caller: self.pubkey(),
            room_id,
            pot_id,
            winners,
            signature,
        };
        self.build_settle_pot_tx(params)
    }

    /// Contribute entropy (commit or reveal for trusted setup)
    pub fn contribute_entropy(
        &self,
        room_id: RoomId,
        commitment: pallas::Base,
        reveal: Option<pallas::Base>,
    ) -> ContractCall {
        let params = ContributeEntropyParams {
            room_id,
            player: self.pubkey(),
            commitment,
            reveal,
        };
        self.build_contribute_entropy_tx(params)
    }

    /// Claim winnings from a pot
    pub fn claim(&self, room_id: RoomId, pot_id: PotId) -> ContractCall {
        let params = ClaimParams { room_id, pot_id, winner: self.pubkey() };
        self.build_claim_tx(params)
    }

    // ============================================================================
    // HELPER METHODS
    // ============================================================================

    /// Generate a nonce for bet commitment
    pub fn generate_nonce(&self) -> pallas::Base {
        use crate::crypto::poseidon_hash;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        poseidon_hash([pallas::Base::from(timestamp), self.pubkey().x()])
    }

    /// Derive a room ID from parameters
    pub fn derive_room_id(
        owner_dao: &ContractId,
        token_id: pallas::Base,
        block_height: u64,
        nonce: pallas::Base,
    ) -> RoomId {
        use crate::crypto::poseidon_hash;
        poseidon_hash([
            owner_dao.inner(),
            token_id,
            pallas::Base::from(block_height),
            nonce,
        ])
    }
}

// ============================================================================
// SERIALIZATION HELPERS
// ============================================================================

impl GameRoomClient {
    /// Serialize a contract call to bytes for transmission
    pub fn serialize_call(call: &ContractCall) -> Vec<u8> {
        darkfi_serial::serialize(call)
    }

    /// Deserialize a contract call from bytes
    pub fn deserialize_call(data: &[u8]) -> Result<ContractCall, crate::error::ContractError> {
        Ok(darkfi_serial::deserialize(data)?)
    }

    /// Serialize multiple contract calls into a batch
    pub fn serialize_calls(calls: &[ContractCall]) -> Vec<u8> {
        let mut result = Vec::new();
        for call in calls {
            result.extend_from_slice(&darkfi_serial::serialize(call));
        }
        result
    }
}