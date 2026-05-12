/// dwow-p2pool-adaptor — Protocol adaptor that presents a dwowd node as a
/// monerod-compatible daemon to p2pool.
///
/// ## Architecture
///
/// ```text
/// xmrig --stratum--> p2pool --[monerod RPC]--> adaptor --[stratum]--> dwowd
///                                                              --> lilith P2P
/// ```
///
/// The adaptor speaks the monerod JSON-RPC protocol on one side (for p2pool) and
/// the dwowd stratum protocol on the other side (for block templates and submission).
/// p2pool thinks it's talking to monerod; the adaptor translates all requests to
/// DarkWow's native interface.
///
/// ## ZMQ
///
/// ZMQ PUB for `chain-main` notifications is not yet implemented. p2pool polls
/// `get_block_template` on its own interval regardless, so the adaptor works
/// without ZMQ. ZMQ support is a future enhancement for instant template refresh.
///
/// ## RPC Methods
///
/// | Method | Purpose |
/// |---|---|
/// | `get_block_template` | Returns DarkWow header as Monero-format block template |
/// | `submit_block` | Accepts solved block, submits to dwowd stratum |
/// | `get_info` | Returns DarkWow chain state |

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;

use structopt::StructOpt;
use tracing::{debug, error, info, warn};
use tracing_subscriber::filter::LevelFilter;

mod dwowd_client;
mod rpc;
mod translate;

use dwowd_client::DwowdClient;
use rpc::AdaptorState;

#[derive(Debug, structopt::StructOpt)]
#[structopt(name = "dwow-p2pool-adaptor", about = "Protocol adaptor: dwowd <-> monerod RPC for p2pool")]
struct Args {
    /// dwowd JSON-RPC URL for chain state queries
    #[structopt(long, default_value = "127.0.0.1:31345")]
    dwowd_rpc: String,

    /// dwowd stratum URL for block template and submission
    #[structopt(long, default_value = "127.0.0.1:31347")]
    dwowd_stratum: String,

    /// Address where the adaptor listens for p2pool connections (monerod-compatible RPC)
    #[structopt(long, default_value = "127.0.0.1:28081")]
    listen: String,

    /// Enable verbose logging
    #[structopt(short, parse(from_occurrences))]
    verbose: u8,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    id: serde_json::Value,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<serde_json::Value>,
    id: serde_json::Value,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::from_args();

    // Setup logging
    let log_level = match args.verbose {
        0 => LevelFilter::INFO,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };
    tracing_subscriber::fmt().with_max_level(log_level).init();

    info!(target: "adaptor", "dwow-p2pool-adaptor starting...");
    info!(target: "adaptor", "dwowd RPC: {}", args.dwowd_rpc);
    info!(target: "adaptor", "dwowd stratum: {}", args.dwowd_stratum);
    info!(target: "adaptor", "Listen: {}", args.listen);

    // Create async executor
    let ex = smol::Executor::new();

    smol::block_on(ex.run(async {
        // Initialize dwowd client
        let client = DwowdClient::new(args.dwowd_rpc.clone(), args.dwowd_stratum.clone());

        // Connect to dwowd stratum
        info!(target: "adaptor", "Connecting to dwowd stratum...");
        if let Err(e) = client.connect().await {
            error!(target: "adaptor", "Failed to connect to dwowd stratum: {e}");
            error!(target: "adaptor", "Is dwowd running and stratum enabled?");
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))
                as Box<dyn std::error::Error>);
        }
        info!(target: "adaptor", "Connected to dwowd stratum");

        let state = Arc::new(AdaptorState::new(client));

        // Start HTTP JSON-RPC server for p2pool
        let listener = TcpListener::bind(&args.listen)
            .map_err(|e| format!("Failed to bind to {}: {}", args.listen, e))?;
        listener
            .set_nonblocking(false)
            .map_err(|e| format!("Failed to set blocking mode: {}", e))?;

        info!(target: "adaptor", "Listening for p2pool on http://{}", args.listen);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let state = state.clone();
                    smol::spawn(async move {
                        handle_connection(stream, state).await;
                    })
                    .detach();
                }
                Err(e) => {
                    warn!(target: "adaptor", "Connection error: {e}");
                }
            }
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    }))?;

    Ok(())
}

/// Handle a single HTTP connection from p2pool.
async fn handle_connection(
    mut stream: std::net::TcpStream,
    state: Arc<AdaptorState>,
) {
    let addr = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    debug!(target: "adaptor", "New connection from {}", addr);

    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    // Read HTTP request
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    // Read headers until empty line
    let mut content_length = 0usize;
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).is_err() {
            return;
        }
        if header_line.trim().is_empty() {
            break;
        }
        if header_line.to_lowercase().starts_with("content-length:") {
            content_length = header_line
                .split(':')
                .nth(1)
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }

    // Read JSON-RPC body
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        use std::io::Read;
        if reader.read_exact(&mut body).is_err() {
            return;
        }
    }

    let body_str = String::from_utf8_lossy(&body);
    debug!(target: "adaptor", "Request: {}", body_str);

    let request: JsonRpcRequest = match serde_json::from_str(&body_str) {
        Ok(r) => r,
        Err(e) => {
            warn!(target: "adaptor", "Invalid JSON-RPC request: {e}");
            let err_resp = JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(serde_json::json!({"code": -32700, "message": "Parse error"})),
                id: serde_json::Value::Null,
            };
            let _ = send_response(&mut stream, &err_resp);
            return;
        }
    };

    let response = handle_rpc(&state, &request).await;

    if send_response(&mut stream, &response).is_err() {
        debug!(target: "adaptor", "Failed to send response to {}", addr);
    }
}

/// Dispatch JSON-RPC method calls.
async fn handle_rpc(state: &Arc<AdaptorState>, req: &JsonRpcRequest) -> JsonRpcResponse {
    let result = match req.method.as_str() {
        "get_block_template" => {
            let template = rpc::handle_get_block_template(state).await;
            Some(template)
        }
        "submit_block" => {
            // p2pool sends params as an array: [blob_hex]
            let blob = req
                .params
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let result = rpc::handle_submit_block(state, blob).await;
            Some(result)
        }
        "get_info" => {
            let info = rpc::handle_get_info(state).await;
            Some(info)
        }
        _ => {
            warn!(target: "adaptor", "Unknown method: {}", req.method);
            None
        }
    };

    match result {
        Some(r) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(r),
            error: None,
            id: req.id.clone(),
        },
        None => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(serde_json::json!({"code": -32601, "message": "Method not found"})),
            id: req.id.clone(),
        },
    }
}

fn send_response(
    stream: &mut std::net::TcpStream,
    response: &JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_string(response)?;
    let http_response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(http_response.as_bytes())?;
    Ok(())
}
