// Wallet Daemon RPC Server
//
// Minimal Unix socket JSON-RPC server for wallet daemon IPC.
// Hand-rolled like p2p_wallet.rs — no dependency on dwow_core::net
// or structopt-toml. Uses smol async I/O directly.
//
// Protocol: newline-delimited JSON-RPC 2.0 over Unix socket.
//   → {"jsonrpc":"2.0","method":"...","params":...,"id":N}\n
//   ← {"jsonrpc":"2.0","result":...,"id":N}\n

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use smol::net::unix::{UnixListener, UnixStream};
use smol::{io::BufReader, prelude::*};

use crate::wallet_error::{Error, Result};

// ── JSON-RPC 2.0 types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct JsonRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: u16,
}

#[derive(Debug, Serialize)]
pub struct JsonResponse {
    pub jsonrpc: String,
    pub id: u16,
    pub result: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonErrorBody {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct JsonError {
    pub jsonrpc: String,
    pub id: u16,
    pub error: JsonErrorBody,
}

pub type RpcResult = std::result::Result<serde_json::Value, JsonError>;

// ── Handler trait ───────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait RpcHandler: Send + Sync {
    async fn handle(&self, method: &str, id: u16, params: serde_json::Value) -> RpcResult;
}

// ── Server ──────────────────────────────────────────────────────────

/// Start listening on a Unix socket. Blocks until the listener fails.
pub async fn listen(handler: Arc<dyn RpcHandler>, socket_path: &str) -> Result<()> {
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path)
        .map_err(|e| Error::Custom(format!("RPC bind {}: {}", socket_path, e)))?;

    // Restrict to owner only (0600) — any local process could otherwise
    // call arbitrary RPC methods including tx.broadcast and wallet.scan.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600));
    }

    tracing::info!(target: "drk::wallet::rpc", "RPC server listening on {}", socket_path);

    loop {
        let (stream, _) = listener.accept().await
            .map_err(|e| Error::Custom(format!("RPC accept: {}", e)))?;

        let handler = handler.clone();
        smol::spawn(async move {
            let fut = std::panic::AssertUnwindSafe(
                handle_connection(handler, stream)
            );
            match futures::FutureExt::catch_unwind(fut).await {
                Ok(()) => {}
                Err(e) => {
                    let msg = e.downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown panic");
                    tracing::error!(target: "drk::wallet::rpc",
                        "RPC connection panicked: {}", msg);
                }
            }
        }).detach();
    }
}

async fn handle_connection(handler: Arc<dyn RpcHandler>, mut stream: UnixStream) {
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();

    const MAX_LINE: usize = 1024 * 1024; // 1MB
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(n) if n > 0 => {}
            _ => break,
        }

        if line.len() > MAX_LINE {
            tracing::warn!(target: "drk::wallet::rpc",
                "RPC request too large ({} bytes), rejecting", line.len());
            break;
        }

        let request: JsonRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                write_err(&mut reader, 0, -32700, &format!("Parse error: {}", e)).await;
                continue;
            }
        };

        match handler.handle(&request.method, request.id, request.params).await {
            Ok(result) => {
                write_ok(&mut reader, request.id, &result).await;
            }
            Err(mut err) => {
                err.id = request.id;
                write_raw(&mut reader, &err).await;
            }
        }
    }
}

fn method_not_found(id: u16) -> JsonError {
    JsonError {
        jsonrpc: "2.0".into(),
        id,
        error: JsonErrorBody { code: -32601, message: "Method not found".into() },
    }
}

async fn write_ok(reader: &mut BufReader<&mut UnixStream>, id: u16, result: &serde_json::Value) {
    let resp = JsonResponse { jsonrpc: "2.0".into(), id, result: result.clone() };
    write_raw(reader, &resp).await;
}

async fn write_err(reader: &mut BufReader<&mut UnixStream>, id: u16, code: i32, msg: &str) {
    let err = JsonError { jsonrpc: "2.0".into(), id, error: JsonErrorBody { code, message: msg.into() } };
    write_raw(reader, &err).await;
}

async fn write_raw(reader: &mut BufReader<&mut UnixStream>, body: &impl Serialize) {
    if let Ok(mut json) = serde_json::to_string(body) {
        json.push('\n');
        let _ = reader.get_mut().write_all(json.as_bytes()).await;
    }
}

// ── Handler wrapper ──────────────────────────────────────────────────

use crate::DwwPtr;

/// Wraps a DwwPtr (Arc<RwLock<Dww>>) so concurrent RPC requests acquire
/// their own read locks internally. Dww can't be cloned because sled::Db
/// doesn't implement Clone.
pub struct DwwRpcHandler {
    dww: DwwPtr,
}

impl DwwRpcHandler {
    pub fn new(dww: DwwPtr) -> Arc<dyn RpcHandler> {
        Arc::new(Self { dww })
    }
}

#[async_trait::async_trait]
impl RpcHandler for DwwRpcHandler {
    async fn handle(&self, method: &str, id: u16, params: serde_json::Value) -> RpcResult {
        let dww = self.dww.read().await;

        let err = |code, msg: &str| JsonError {
            jsonrpc: "2.0".into(),
            id,
            error: JsonErrorBody { code, message: msg.into() },
        };

        match method {
            "ping" => {
                // Health check: verify DB access and report sync state.
                let height = dww.chain.get_height().unwrap_or(0);
                let peers = dww.p2p.as_ref()
                    .map(|p| p.hosts().peers().len())
                    .unwrap_or(0);
                // Quick SQLite check — fatal for DB if this fails.
                let db_ok = dww.wallet.get_addresses().is_ok();
                Ok(serde_json::json!({
                    "status": "ok",
                    "height": height,
                    "peers": peers,
                    "db_ok": db_ok,
                }))
            }

            "wallet.balance" => {
                let balances = dww.capability_balance()
                    .map_err(|e| err(-32000, &format!("{}", e)))?;
                Ok(serde_json::to_value(balances)
                    .map_err(|e| err(-32000, &format!("{}", e)))?)
            }

            "wallet.sync_status" => {
                let height = dww.chain.get_height().unwrap_or(0);
                let peer_tip = dww.highest_peer_tip.get();
                let peers = dww.p2p.as_ref()
                    .map(|p| p.hosts().peers().len())
                    .unwrap_or(0);
                Ok(serde_json::json!({
                    "height": height,
                    "peer_tip": peer_tip,
                    "peers": peers,
                    "synced": dww.is_synced(),
                }))
            }

            "chain.get_height" => {
                let h = dww.chain.get_height()
                    .map_err(|e| err(-32000, &format!("{}", e)))?;
                Ok(serde_json::json!({"height": h}))
            }

            "wallet.scan" => {
                let mut output = vec![];
                dww.scan_blocks(&mut output, None, &true).await
                    .map_err(|e| err(-32000, &format!("{}", e)))?;
                Ok(serde_json::json!({"scanned": output}))
            }

            "wallet.secret_count" => {
                let count = dww.get_secrets()
                    .map(|s| s.len())
                    .map_err(|e| err(-32000, &format!("get_secrets failed: {e}")))?;
                Ok(serde_json::json!({"count": count}))
            }

            "tx.broadcast" => {
                let tx_hex = params.get("tx")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err(-32602, "missing 'tx' param (hex-encoded)"))?;
                // Cap at 1MB hex (512KB binary) — prevents OOM from
                // oversized input held under the Dww read lock.
                if tx_hex.len() > 1024 * 1024 {
                    return Err(err(-32602, "tx hex too large (max 1MB)"));
                }
                let tx_bytes = hex::decode(tx_hex)
                    .map_err(|e| err(-32602, &format!("invalid hex: {}", e)))?;
                let tx: dwow_core::tx::Transaction = dwow_serial::deserialize(&tx_bytes)
                    .map_err(|e| err(-32602, &format!("invalid tx: {}", e)))?;
                let mut output = vec![];
                let txid = dww.broadcast_tx(&tx, &mut output, false, None, None).await
                    .map_err(|e| err(-32000, &format!("broadcast failed: {}", e)))?;
                Ok(serde_json::json!({"txid": txid, "output": output}))
            }

            _ => Err(err(-32601, "Method not found")),
        }
    }
}

// ── Method registry (for help/docs) ──────────────────────────────────

pub fn rpc_methods() -> &'static [(&'static str, &'static str)] {
    &[
        ("ping",               "Health check — returns pong"),
        ("wallet.balance",      "Get token balances"),
        ("wallet.sync_status",  "Get sync status (height, peer_tip, peers)"),
        ("wallet.secret_count", "Get number of secret keys in wallet"),
        ("chain.get_height",    "Get local chain height"),
    ]
}
