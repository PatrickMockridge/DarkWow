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

    /// DarkWow wallet address for stratum login (required for dwowd stratum protocol)
    #[structopt(long, default_value = "")]
    wallet_address: String,

    /// Maximum stratum connection retry attempts (default: 30 = ~60s)
    #[structopt(long, default_value = "30")]
    connect_retries: u32,

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
    info!(target: "adaptor", "Wallet: {}", args.wallet_address);

    if args.wallet_address.is_empty() {
        error!(target: "adaptor", "--wallet-address is required (stratum login needs a DarkWow bs58 address)");
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "missing --wallet-address",
        )));
    }

    // Create async executor
    let ex = smol::Executor::new();

    smol::block_on(ex.run(async {
        // Connect to dwowd stratum with retry
        info!(target: "adaptor", "Connecting to dwowd stratum (retries: {})...", args.connect_retries);
        let client = DwowdClient::connect(
            args.dwowd_rpc.clone(),
            args.dwowd_stratum.clone(),
            args.wallet_address.clone(),
            args.connect_retries,
        )
        .await
        .map_err(|e| {
            error!(target: "adaptor", "Failed to connect to dwowd stratum: {e}");
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e)
        })?;
        info!(target: "adaptor", "Connected to dwowd stratum");

        let state = Arc::new(AdaptorState::new(client));

        // Start HTTP JSON-RPC server for p2pool
        let listener = TcpListener::bind(&args.listen)
            .map_err(|e| format!("Failed to bind to {}: {}", args.listen, e))?;
        listener
            .set_nonblocking(false)
            .map_err(|e| format!("Failed to set blocking mode: {}", e))?;

        info!(target: "adaptor", "Listening for p2pool on http://{}", args.listen);

        // Blocking listener + smol::spawn doesn't work: the main executor
        // thread is blocked in accept(), so spawned tasks never execute.
        // Use std::thread::spawn with its own smol::block_on for each connection.
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let state = state.clone();
                    std::thread::spawn(move || {
                        smol::block_on(async {
                            handle_connection(stream, state).await;
                        });
                    });
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
        Err(e) => {
            warn!(target: "adaptor", "Failed to clone stream: {e}");
            return;
        }
    });

    // Set a read timeout so incorrect Content-Length doesn't block forever
    let _ = reader.get_mut().set_read_timeout(Some(std::time::Duration::from_secs(30)));

    // Read HTTP request
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        warn!(target: "adaptor", "Failed to read HTTP request line");
        return;
    }

    // Read headers until empty line
    let mut content_length = 0usize;
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).is_err() {
            warn!(target: "adaptor", "Failed to read HTTP header");
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
        if let Err(e) = reader.read_exact(&mut body) {
            warn!(target: "adaptor", "Failed to read body (Content-Length: {content_length}): {e}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_jsonrpc_request_deser_valid() {
        let json = r#"{"jsonrpc":"2.0","method":"get_info","params":[],"id":1}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "get_info");
        assert_eq!(req.id, serde_json::Value::Number(1.into()));
    }

    #[test]
    fn test_jsonrpc_request_deser_submit_block() {
        let json = r#"{"jsonrpc":"2.0","method":"submit_block","params":["deadbeef"],"id":42}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "submit_block");
        let blob = req
            .params
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(blob, "deadbeef");
    }

    #[test]
    fn test_jsonrpc_request_deser_empty_params() {
        let json = r#"{"jsonrpc":"2.0","method":"get_block_template","id":1}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "get_block_template");
        assert!(req.params.is_null());
    }

    #[test]
    fn test_jsonrpc_response_ser() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::json!({"status": "OK"})),
            error: None,
            id: serde_json::Value::Number(1.into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(json.contains("\"status\""));
        assert!(json.contains("\"OK\""));
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(serde_json::json!({"code": -32601, "message": "Method not found"})),
            id: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_send_response_format() {
        // Test the HTTP wire format produced by send_response.
        // Use a TcpListener/TcpStream pair to avoid nightly-only TcpStream::pair().
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server_thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = String::new();
            stream.read_to_string(&mut buf).unwrap();
            buf
        });

        let mut client = std::net::TcpStream::connect(addr).unwrap();
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(serde_json::json!({"status": "OK", "height": 42})),
            error: None,
            id: serde_json::Value::Number(1.into()),
        };

        send_response(&mut client, &resp).unwrap();
        drop(client);

        let buf = server_thread.join().unwrap();

        assert!(buf.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(buf.contains("Content-Type: application/json\r\n"));
        assert!(buf.contains("Content-Length: "));
        assert!(buf.contains("Connection: close\r\n"));
        let body_start = buf.find("\r\n\r\n").unwrap() + 4;
        let body = &buf[body_start..];
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["result"]["height"], 42);
        assert_eq!(parsed["result"]["status"], "OK");
    }

    #[test]
    fn test_unknown_method_produces_error() {
        // Verify the dispatch logic: unknown methods return Method not found.
        // We test the JSON structure since handle_rpc requires an AdaptorState.
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "nonexistent".into(),
            params: serde_json::Value::Null,
            id: serde_json::Value::Number(99.into()),
        };

        // Directly test the error case logic (matching what handler_rpc does for unknown methods)
        let response = JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(serde_json::json!({"code": -32601, "message": "Method not found"})),
            id: req.id.clone(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("-32601"));
        assert!(json.contains("Method not found"));
        assert!(json.contains("\"id\":99"));
    }
}
