// Wallet Daemon RPC Client
//
// Connect-per-call Unix socket JSON-RPC client. Talks to the daemon's
// RPC server on /tmp/drk-{network}.sock.
//
// Each method opens a fresh connection, sends a JSON-RPC request,
// reads the response, and closes. Unix socket connect is cheap —
// no connection pool needed for CLI usage.

use std::collections::HashMap;
use std::sync::Arc;

use smol::net::unix::UnixStream;
use smol::prelude::*;

use crate::wallet_error::{Error, Result};

pub struct WalletRpcClient {
    socket_path: String,
}

#[derive(Debug, serde::Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    id: u16,
    #[serde(default)]
    result: serde_json::Value,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, serde::Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl WalletRpcClient {
    pub fn new(network: &str) -> Self {
        Self {
            socket_path: format!("/tmp/drk-{}.sock", network.to_lowercase()),
        }
    }

    /// Try to connect and ping the daemon. Returns Some(client) if the
    /// daemon is reachable, None otherwise.
    pub fn try_connect(network: &str) -> Option<Self> {
        let client = Self::new(network);
        match client.ping_sync() {
            Ok(_) => Some(client),
            Err(_) => None,
        }
    }

    fn ping_sync(&self) -> Result<String> {
        // Timeout after 3s — a stale socket (daemon crashed without cleanup)
        // would hang forever on connect+read. Unix sockets succeed connect even
        // if nobody is listening, then read_line blocks indefinitely.
        smol::block_on(async {
            smol::future::or(
                self.call("ping", serde_json::json!({})),
                async {
                    smol::Timer::after(std::time::Duration::from_secs(3)).await;
                    Err(Error::Custom("daemon ping timed out".into()))
                },
            ).await
        })
    }

    async fn call(&self, method: &str, params: serde_json::Value) -> Result<String> {
        let mut stream = UnixStream::connect(&self.socket_path).await
            .map_err(|e| Error::Custom(format!("RPC connect {}: {}", self.socket_path, e)))?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });
        let mut req_str = serde_json::to_string(&request)
            .map_err(|e| Error::Custom(format!("RPC serialize: {}", e)))?;
        req_str.push('\n');

        stream.write_all(req_str.as_bytes()).await
            .map_err(|e| Error::Custom(format!("RPC write: {}", e)))?;

        let mut reader = smol::io::BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).await
            .map_err(|e| Error::Custom(format!("RPC read: {}", e)))?;

        let resp: JsonRpcResponse = serde_json::from_str(&line)
            .map_err(|e| Error::Custom(format!("RPC parse: {} — raw: {}", e, line.trim())))?;

        if let Some(err) = resp.error {
            return Err(Error::Custom(format!("RPC error {}: {}", err.code, err.message)));
        }

        serde_json::to_string(&resp.result)
            .map_err(|e| Error::Custom(format!("RPC result serialize: {}", e)))
    }

    // ── Public methods ────────────────────────────────────────────

    pub fn ping(&self) -> Result<String> {
        smol::block_on(self.call("ping", serde_json::json!({})))
    }

    pub fn balance(&self) -> Result<HashMap<String, u64>> {
        let raw = smol::block_on(self.call("wallet.balance", serde_json::json!({})))?;
        serde_json::from_str(&raw)
            .map_err(|e| Error::Custom(format!("parse balance: {}", e)))
    }

    pub fn sync_status(&self) -> Result<serde_json::Value> {
        let raw = smol::block_on(self.call("wallet.sync_status", serde_json::json!({})))?;
        serde_json::from_str(&raw)
            .map_err(|e| Error::Custom(format!("parse sync_status: {}", e)))
    }

    pub fn chain_height(&self) -> Result<u64> {
        let raw = smol::block_on(self.call("chain.get_height", serde_json::json!({})))?;
        let v: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| Error::Custom(format!("parse height: {}", e)))?;
        v["height"].as_u64()
            .ok_or_else(|| Error::Custom("missing height field".into()))
    }

    /// Scan blocks via RPC. Returns scan progress messages from the daemon.
    pub fn scan(&self) -> Result<Vec<String>> {
        let raw = smol::block_on(self.call("wallet.scan", serde_json::json!({})))?;
        let v: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| Error::Custom(format!("parse scan result: {}", e)))?;
        let output: Vec<String> = v["scanned"].as_array()
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default();
        Ok(output)
    }

    pub fn broadcast_tx(&self, tx_hex: &str) -> Result<String> {
        let raw = smol::block_on(self.call("tx.broadcast", serde_json::json!({"tx": tx_hex})))?;
        let v: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| Error::Custom(format!("parse broadcast: {}", e)))?;
        v["txid"].as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Custom("missing txid field".into()))
    }
}
