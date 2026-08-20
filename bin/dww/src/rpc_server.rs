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

    // Generate random auth token at startup. Write to file next to socket
    // with mode 0600. Clients must include this token in every JSON-RPC
    // request as params.auth_token. Full token comparison (not substring match).
    let auth_token: [u8; 32] = rand::Rng::gen(&mut rand::rngs::OsRng);
    let auth_token_hex = hex::encode(auth_token);
    let token_path = format!("{}.token", socket_path);
    std::fs::write(&token_path, &auth_token_hex)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600));
    }

    tracing::info!(target: "dww::wallet::rpc", "RPC server listening on {} (auth token at {})", socket_path, token_path);

    loop {
        let (stream, _) = listener.accept().await
            .map_err(|e| Error::Custom(format!("RPC accept: {}", e)))?;

        let handler = handler.clone();
        let expected_token = auth_token_hex.clone();
        smol::spawn(async move {
            let fut = std::panic::AssertUnwindSafe(
                handle_connection(handler, stream, &expected_token)
            );
            match futures::FutureExt::catch_unwind(fut).await {
                Ok(()) => {}
                Err(e) => {
                    let msg = e.downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown panic");
                    tracing::error!(target: "dww::wallet::rpc",
                        "RPC connection panicked: {}", msg);
                }
            }
        }).detach();
    }
}

async fn handle_connection(handler: Arc<dyn RpcHandler>, mut stream: UnixStream, expected_token: &str) {
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
            tracing::warn!(target: "dww::wallet::rpc",
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

        // Verify auth token — constant-time full comparison.
        // Extract token from params.auth_token, rejecting if missing or wrong.
        let supplied = request.params.get("auth_token")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if supplied.len() != expected_token.len() {
            write_err(&mut reader, request.id, -32001, "Unauthorized: invalid auth token").await;
            continue;
        }
        // Constant-time byte comparison
        let mut mismatch: u8 = 0;
        for (a, b) in supplied.bytes().zip(expected_token.bytes()) {
            mismatch |= a ^ b;
        }
        if mismatch != 0 {
            write_err(&mut reader, request.id, -32001, "Unauthorized: invalid auth token").await;
            continue;
        }

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
        if let Err(e) = reader.get_mut().write_all(json.as_bytes()).await {
            tracing::error!("RPC write_all failed: {e} — client connection likely broken");
            return;
        }
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
                let height = match dww.wallet.chain_height() {
                    Ok(h) => h.get(),
                    Err(e) => {
                        tracing::error!("chain_height failed: {}", e);
                        0
                    }
                };
                let peers = dww.p2p.as_ref()
                    .map(|p| p.hosts().peers().len())
                    .unwrap_or(0);
                // Quick SQLite check — fatal for DB if this fails.
                let db_ok = dww.wallet.get_held_capabilities(Some(false)).is_ok();
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
                let height = match dww.wallet.chain_height() {
                    Ok(h) => h.get(),
                    Err(e) => {
                        tracing::error!("chain_height failed: {}", e);
                        0
                    }
                };
                let peer_tip = dww.highest_peer_tip.get();
                let peers = dww.p2p.as_ref()
                    .map(|p| p.hosts().peers().len())
                    .unwrap_or(0);
                Ok(serde_json::json!({
                    "height": height,
                    "peer_tip": peer_tip.get(),
                    "peers": peers,
                    "synced": dww.is_synced(),
                }))
            }

            "chain.get_height" => {
                let h = dww.wallet.chain_height()
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

            "wallet.capability_count" => {
                // Number of held (non-revoked) capabilities — the decrypt count the
                // pipeline asserts on via `scan --porcelain`. Mirrors secret_count.
                let count = dww.wallet.get_held_capabilities(Some(false))
                    .map(|c| c.len())
                    .map_err(|e| err(-32000, &format!("get_held_capabilities failed: {:?}", e)))?;
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
                // "wait_for_confirm": bool (optional, default false)
                //   When true, poll for chain-height advancement before returning.
                //
                //   CONFIRMATION MODEL: Best-effort height polling, NOT finality.
                //   Does not verify the specific transaction was included in a block.
                //   For finality guarantees, monitor wallet sync status and verify
                //   capability consumption via wallet.scan.
                let confirm = params.get("wait_for_confirm")
                    .and_then(|v| v.as_bool()).unwrap_or(false);
                let timeout = params.get("confirm_timeout_secs")
                    .and_then(|v| v.as_u64());
                let mut output = vec![];
                let txid = dww.broadcast_tx(&tx, &mut output, confirm, timeout, None).await
                    .map_err(|e| err(-32000, &format!("broadcast failed: {}", e)))?;
                Ok(serde_json::json!({"txid": txid, "output": output}))
            }

            "wallet.transfer" => {
                let amount = params.get("amount")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err(-32602, "missing 'amount' param"))?;
                let asset_id = params.get("asset_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err(-32602, "missing 'asset_id' param"))?;
                let recipient = params.get("recipient")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| err(-32602, "missing 'recipient' param"))?;
                let _spend_hook = params.get("spend_hook").and_then(|v| v.as_str());
                let _user_data = params.get("user_data").and_then(|v| v.as_str());

                // Two paths, by law (wallet.md §6.4, §9): DRKW = the native
                // token = the ONE bespoke write-path citizen, built by the
                // hardcoded NativeToken client via build_native_transfer —
                // never through invoke_contract or any manifest path. Non-DRKW
                // assets route through the generic path. (The promissory_note
                // hardcoding below is audit item A2 — replaced by generic
                // routing in the capability-side remediation phase.)
                let is_drkw = asset_id == "DRKW" || asset_id == "drkw";
                let tx = if is_drkw {
                    let amount_u64: u64 = amount.parse()
                        .map_err(|e| err(-32602, &format!("invalid 'amount': {}", e)))?;
                    // The shell draws the Seed (wallet.md §6.1).
                    let mut seed = [0u8; 32];
                    use rand::RngCore;
                    rand::rngs::OsRng.fill_bytes(&mut seed);
                    dww.build_native_transfer(amount_u64, recipient, seed).await
                        .map_err(|e| err(-32000, &format!("transfer build failed: {}", e)))?
                } else {
                    // Non-native capability transfer: manifest-driven path.
                    // Select held capability by asset_id → resolve contract →
                    // invoke_contract → manifest → CapabilityProvider → prover_impl.
                    let all_caps = dww.wallet.get_held_capabilities(Some(false))
                        .map_err(|e| err(-32000, &format!("{:?}", e)))?;
                    let asset_bytes = bs58::decode(asset_id).into_vec().unwrap_or_default();
                    let rec = all_caps.iter()
                        .find(|c| c.asset_id.to_bytes().to_vec() == asset_bytes)
                        .ok_or_else(|| err(-32602, &format!(
                            "no held capability with asset_id '{}'", asset_id)))?;
                    let (contract_id, function_name) = dww.resolve_transfer_contract(rec, "transfer")
                        .map_err(|e| err(-32000, &e))?;
                    let amount_u64: u64 = amount.parse()
                        .map_err(|e| err(-32602, &format!("invalid 'amount': {}", e)))?;
                    let params_json = serde_json::json!({
                        "amount": amount_u64,
                        "recipient": recipient,
                    }).to_string();
                    let cid_str = bs58::encode(contract_id.to_bytes()).into_string();
                    dww.invoke_contract(&cid_str, &function_name, Some(&params_json), vec![], vec![])
                        .await
                        .map_err(|e| err(-32000, &format!("invoke_contract: {}", e)))?
                };
                // "wait_for_confirm": bool (optional, default false)
                //   When true, poll for chain-height advancement before returning.
                //
                //   CONFIRMATION MODEL: Best-effort height polling, NOT finality.
                //   Does not verify the specific transaction was included in a block.
                let confirm = params.get("wait_for_confirm")
                    .and_then(|v| v.as_bool()).unwrap_or(false);
                let timeout = params.get("confirm_timeout_secs")
                    .and_then(|v| v.as_u64());
                let mut output = vec![];
                let txid = dww.broadcast_tx(&tx, &mut output, confirm, timeout, None).await
                    .map_err(|e| err(-32000, &format!("broadcast failed: {}", e)))?;
                if let Err(e) = dww.mark_tx_exercise(&tx, &mut output) {
                    tracing::error!("mark_tx_exercise failed for txid {}: {}", txid, e);
                    output.push(format!("WARNING: failed to mark tx as exercised: {}", e));
                }
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
        ("wallet.transfer",     "Build + broadcast a transfer transaction"),
        ("wallet.secret_count", "Get number of secret keys in wallet"),
        ("wallet.capability_count", "Get number of held capabilities"),
        ("chain.get_height",    "Get local chain height"),
    ]
}
