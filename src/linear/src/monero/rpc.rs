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

#[cfg(test)]
pub(crate) mod test_helpers {
    //! Mock monerod HTTP server helpers for unit tests.
    //! Spawn a TCP listener on a random port, serve canned JSON-RPC responses,
    //! and return the URL for the client to connect to.

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    /// Serve a single canned JSON-RPC response, then exit.
    /// Returns the URL the server is listening on.
    pub fn serve_once(json_body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}/json_rpc", port);
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let body = format!(
                    "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    json_body.len(),
                    json_body,
                );
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
            }
        });
        url
    }

    /// Serve a sequence of canned JSON-RPC responses, one per request, then exit.
    /// Returns the URL the server is listening on.
    pub fn serve_sequence(responses: Vec<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}/json_rpc", port);
        thread::spawn(move || {
            for json_body in responses {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = [0u8; 8192];
                    let _ = stream.read(&mut buf);
                    let body = format!(
                        "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        json_body.len(),
                        json_body,
                    );
                    let _ = stream.write_all(body.as_bytes());
                    let _ = stream.flush();
                }
            }
        });
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_block_by_height_success() {
        let response = r#"{"result":{"block_header":{"hash":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789","height":3000000}}}"#;
        let url = test_helpers::serve_once(response);
        let (height, hash) = get_block_by_height(&url, 3000000).unwrap();
        assert_eq!(height, 3000000);
        let expected =
            hex::decode("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                .unwrap();
        assert_eq!(hash, expected.as_slice());
    }

    #[test]
    fn test_get_block_by_height_empty_hash() {
        let response = r#"{"result":{"block_header":{"hash":"","height":3000000}}}"#;
        let url = test_helpers::serve_once(response);
        let err = get_block_by_height(&url, 3000000).unwrap_err();
        assert!(matches!(err, MonerodError::BlockNotFound(3000000)));
    }

    #[test]
    fn test_get_block_by_height_malformed_json() {
        let response = "not json at all";
        let url = test_helpers::serve_once(response);
        let err = get_block_by_height(&url, 3000000).unwrap_err();
        assert!(matches!(err, MonerodError::JsonRpc(_)));
    }

    #[test]
    fn test_get_block_by_height_connection_refused() {
        let url = "http://127.0.0.1:19999/json_rpc";
        let err = get_block_by_height(url, 3000000).unwrap_err();
        assert!(matches!(err, MonerodError::Http(_)));
    }

    #[test]
    fn test_get_block_count_success() {
        let response = r#"{"result":{"count":3500000}}"#;
        let url = test_helpers::serve_once(response);
        let count = get_block_count(&url).unwrap();
        assert_eq!(count, 3500000);
    }

    #[test]
    fn test_get_block_count_jsonrpc_error() {
        let response = r#"{"error":{"code":-32601,"message":"Method not found"}}"#;
        let url = test_helpers::serve_once(response);
        let err = get_block_count(&url).unwrap_err();
        assert!(matches!(err, MonerodError::JsonRpc(_)));
    }
}
