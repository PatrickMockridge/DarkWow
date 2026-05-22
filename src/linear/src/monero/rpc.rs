//! Monerod JSON-RPC client for block verification.
//!
//! Queries a monerod instance via its JSON-RPC endpoint to verify
//! that a claimed Monero block hash and height are valid, and to
//! check confirmation depth.

use serde::Deserialize;
use serde_json::json;

/// Error type for monerod RPC operations.
#[derive(Debug, thiserror::Error)]
pub enum MonerodError {
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),
    #[error("Block not found at height {0}")]
    BlockNotFound(u64),
}

/// Minimal Monero block header returned by the `get_block` RPC method.
#[derive(Debug, Deserialize)]
struct MoneroBlockHeader {
    hash: String,
    height: u64,
}

/// JSON-RPC response wrapper for `get_block`.
#[derive(Debug, Deserialize)]
struct GetBlockResponse {
    block_header: MoneroBlockHeader,
}

/// JSON-RPC response wrapper for `get_block_count`.
#[derive(Debug, Deserialize)]
struct GetBlockCountResponse {
    count: u64,
}

/// Outer JSON-RPC envelope.
#[derive(Debug, Deserialize)]
struct JsonRpcResult<T> {
    result: T,
}

/// Fetch a Monero block header by height from monerod.
pub fn get_block_by_height(url: &str, height: u64) -> Result<(u64, [u8; 32]), MonerodError> {
    let request_body = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": "get_block",
        "params": { "height": height },
    }))
    .map_err(|e| MonerodError::JsonRpc(e.to_string()))?;

    let response = ureq::post(url)
        .header("Content-Type", "application/json")
        .send(&request_body)
        .map_err(|e| MonerodError::Http(e.to_string()))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| MonerodError::Http(e.to_string()))?;

    let envelope: JsonRpcResult<GetBlockResponse> =
        serde_json::from_str(&body).map_err(|e| MonerodError::JsonRpc(e.to_string()))?;

    let block = envelope.result.block_header;

    if block.hash.is_empty() {
        return Err(MonerodError::BlockNotFound(height));
    }

    let hash_bytes = hex::decode(&block.hash)
        .map_err(|e| MonerodError::JsonRpc(format!("invalid hash hex: {e}")))?;
    let hash: [u8; 32] = hash_bytes
        .try_into()
        .map_err(|_| MonerodError::JsonRpc("hash not 32 bytes".into()))?;

    Ok((block.height, hash))
}

/// Fetch the current Monero chain tip height from monerod.
pub fn get_block_count(url: &str) -> Result<u64, MonerodError> {
    let request_body = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": "0",
        "method": "get_block_count",
    }))
    .map_err(|e| MonerodError::JsonRpc(e.to_string()))?;

    let response = ureq::post(url)
        .header("Content-Type", "application/json")
        .send(&request_body)
        .map_err(|e| MonerodError::Http(e.to_string()))?;

    let body = response
        .into_body()
        .read_to_string()
        .map_err(|e| MonerodError::Http(e.to_string()))?;

    let envelope: JsonRpcResult<GetBlockCountResponse> =
        serde_json::from_str(&body).map_err(|e| MonerodError::JsonRpc(e.to_string()))?;

    Ok(envelope.result.count)
}
