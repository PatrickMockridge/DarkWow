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

use tinyjson::JsonValue;

use dwow_core::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResponse, JsonResult};

/// Custom RPC errors available for dwowd.
/// Please sort them sensefully.
pub enum RpcError {
    // Contract-related errors
    ContractStateNotFound = -32201,

    // Miner configuration errors
    MinerInvalidRecipientPrefix = -32303,

    // Stratum errors
    MinerMissingLogin = -32306,
    MinerInvalidLogin = -32307,
    MinerMissingPassword = -32308,
    MinerInvalidPassword = -32309,
    MinerMissingAgent = -32310,
    MinerInvalidAgent = -32311,
    MinerMissingAlgo = -32312,
    MinerInvalidAlgo = -32313,
    MinerRandomXNotSupported = -32314,
    MinerMissingClientId = -32315,
    MinerInvalidClientId = -32316,
    MinerMissingJobId = -32318,
    MinerInvalidJobId = -32319,
    MinerMissingNonce = -32320,
    MinerInvalidNonce = -32321,

    // Merge mining errors (p2pool protocol)
    MinerMissingAddress = -32324,
    MinerInvalidAddress = -32325,
    MinerMissingAuxHash = -32326,
    MinerInvalidAuxHash = -32327,
    MinerMissingHeight = -32328,
    MinerInvalidHeight = -32329,
    MinerMissingPrevId = -32330,
    MinerInvalidPrevId = -32331,
    MinerMissingAuxBlob = -32332,
    MinerInvalidAuxBlob = -32333,
    MinerMissingBlob = -32334,
    MinerInvalidBlob = -32335,
    MinerMissingMerkleProof = -32336,
    MinerInvalidMerkleProof = -32337,
    MinerMissingPath = -32338,
    MinerInvalidPath = -32339,
    MinerMissingSeedHash = -32340,
    MinerInvalidSeedHash = -32341,
    MinerMerkleProofConstructionFailed = -32342,
    MinerMoneroPowDataConstructionFailed = -32343,

    // Node lifecycle errors
    NodeNotSynced = -32350,

}

fn to_tuple(e: RpcError) -> (i32, String) {
    let msg = match e {
        // Contract-related errors
        RpcError::ContractStateNotFound => "Records not found for given contract state",

        // Miner configuration errors
        RpcError::MinerInvalidRecipientPrefix => {
            "Request recipient wallet address prefix is invalid"
        }

        // Stratum errors
        RpcError::MinerMissingLogin => "Request is missing the login",
        RpcError::MinerInvalidLogin => "Request login is invalid",
        RpcError::MinerMissingPassword => "Request is missing the password",
        RpcError::MinerInvalidPassword => "Request password is invalid",
        RpcError::MinerMissingAgent => "Request is missing the agent",
        RpcError::MinerInvalidAgent => "Request agent is invalid",
        RpcError::MinerMissingAlgo => "Request is missing the algo",
        RpcError::MinerInvalidAlgo => "Request algo is invalid",
        RpcError::MinerRandomXNotSupported => "Request doesn't support rx/0",
        RpcError::MinerMissingClientId => "Request is missing the client ID",
        RpcError::MinerInvalidClientId => "Request client ID is invalid",
        RpcError::MinerMissingJobId => "Request is missing the job ID",
        RpcError::MinerInvalidJobId => "Request job ID is invalid",
        RpcError::MinerMissingNonce => "Request is missing the nonce",
        RpcError::MinerInvalidNonce => "Request nonce is invalid",

        // Merge mining errors
        RpcError::MinerMissingAddress => "Request is missing the address",
        RpcError::MinerInvalidAddress => "Request address is invalid",
        RpcError::MinerMissingAuxHash => "Request is missing the aux_hash",
        RpcError::MinerInvalidAuxHash => "Request aux_hash is invalid",
        RpcError::MinerMissingHeight => "Request is missing the height",
        RpcError::MinerInvalidHeight => "Request height is invalid",
        RpcError::MinerMissingPrevId => "Request is missing the prev_id",
        RpcError::MinerInvalidPrevId => "Request prev_id is invalid",
        RpcError::MinerMissingAuxBlob => "Request is missing the aux_blob",
        RpcError::MinerInvalidAuxBlob => "Request aux_blob is invalid",
        RpcError::MinerMissingBlob => "Request is missing the blob",
        RpcError::MinerInvalidBlob => "Request blob is invalid",
        RpcError::MinerMissingMerkleProof => "Request is missing the merkle_proof",
        RpcError::MinerInvalidMerkleProof => "Request merkle_proof is invalid",
        RpcError::MinerMissingPath => "Request is missing the path",
        RpcError::MinerInvalidPath => "Request path is invalid",
        RpcError::MinerMissingSeedHash => "Request is missing the seed_hash",
        RpcError::MinerInvalidSeedHash => "Request seed_hash is invalid",
        RpcError::MinerMerkleProofConstructionFailed => {
            "Merkle proof construction failed"
        }
        RpcError::MinerMoneroPowDataConstructionFailed => {
            "Monero PoW data construction failed"
        }
        RpcError::NodeNotSynced => "Node is still syncing — mining not yet available",
    };

    (e as i32, msg.to_string())
}

pub fn server_error(e: RpcError, id: u16, msg: Option<&str>) -> JsonResult {
    let (code, default_msg) = to_tuple(e);

    if let Some(message) = msg {
        return JsonError::new(ServerError(code), Some(message.to_string()), id).into()
    }

    JsonError::new(ServerError(code), Some(default_msg), id).into()
}

pub fn miner_status_response(id: u16, status: &str) -> JsonResult {
    JsonResponse::new(
        JsonValue::from(HashMap::from([(
            "status".to_string(),
            JsonValue::from(String::from(status)),
        )])),
        id,
    )
    .into()
}
