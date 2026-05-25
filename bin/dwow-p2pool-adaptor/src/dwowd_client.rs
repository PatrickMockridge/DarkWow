/// Client for communicating with the co-located dwowd node.
///
/// Maintains a stratum connection for block templates and solution submission.
/// A background reader task continuously reads from the stratum stream to
/// handle job push notifications without corrupting submit response parsing.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use smol::channel;
use smol::lock::Mutex;
use tracing::{debug, info, warn};

/// A stratum job from dwowd, cached for translation to p2pool.
#[derive(Clone, Debug)]
pub struct StratumJob {
    pub job_id: String,
    pub blob: String,
    pub target: String,
    pub height: u64,
    pub seed_hash: String,
}

/// Pending submit — the submit response will arrive asynchronously.
struct PendingSubmit {
    tx: channel::Sender<String>,
}

/// A dwowd client with a shared stratum connection and background reader.
pub struct DwowdClient {
    /// Latest job from dwowd stratum (updated by background reader on push).
    current_job: Arc<Mutex<Option<StratumJob>>>,
    /// Submit request id counter.
    next_id: AtomicU64,
    /// Pending submit responses, keyed by request id.
    pending: Arc<Mutex<HashMap<u64, PendingSubmit>>>,
    /// Channel to send lines to the background writer task.
    write_tx: channel::Sender<String>,
    /// dwowd RPC URL for chain state queries.
    rpc_url: String,
}

impl DwowdClient {
    /// Connect to dwowd stratum and spawn the background reader.
    /// Retries up to `max_retries` times with 2-second delays.
    pub async fn connect(
        rpc_url: String,
        stratum_url: String,
        wallet_address: String,
        max_retries: u32,
    ) -> Result<Self, String> {
        let mut last_err = String::new();

        for attempt in 1..=max_retries {
            if attempt > 1 {
                info!(target: "dwowd_client", "Retrying stratum connection (attempt {attempt}/{max_retries})...");
                smol::Timer::after(Duration::from_secs(2)).await;
            }

            match Self::connect_once(rpc_url.clone(), &stratum_url, wallet_address.clone()).await {
                Ok(client) => {
                    info!(target: "dwowd_client", "Stratum connected on attempt {attempt}");
                    return Ok(client);
                }
                Err(e) => {
                    warn!(target: "dwowd_client", "Stratum connection attempt {attempt} failed: {e}");
                    last_err = e;
                }
            }
        }

        Err(format!(
            "Failed to connect to dwowd stratum after {max_retries} attempts: {last_err}"
        ))
    }

    /// Single connection attempt.
    async fn connect_once(
        rpc_url: String,
        stratum_url: &str,
        wallet_address: String,
    ) -> Result<Self, String> {
        let stream = TcpStream::connect(stratum_url)
            .map_err(|e| format!("TCP connect to {stratum_url}: {e}"))?;
        stream
            .set_nonblocking(false)
            .map_err(|e| format!("set_nonblocking: {e}"))?;

        // Send login using the wallet address
        let login = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "login",
            "params": {
                "login": wallet_address,
                "pass": "x",
                "agent": "dwow-p2pool-adaptor",
                "algo": ["rx/0"]
            },
            "id": 0
        });
        let login_str = serde_json::to_string(&login).unwrap() + "\n";
        let mut writer = stream
            .try_clone()
            .map_err(|e| format!("try_clone for login: {e}"))?;
        writer
            .write_all(login_str.as_bytes())
            .map_err(|e| format!("login write: {e}"))?;

        let mut reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("try_clone for reader: {e}"))?,
        );
        let mut response = String::new();
        reader
            .read_line(&mut response)
            .map_err(|e| format!("login read: {e}"))?;

        let parsed: serde_json::Value =
            serde_json::from_str(&response).map_err(|e| format!("login parse: {e}"))?;

        // The stratum response always includes "error":null. Only reject if
        // there's a non-null error object (an actual protocol-level failure).
        if let Some(err) = parsed.get("error") {
            if !err.is_null() {
                return Err(format!("Stratum login rejected: {err}"));
            }
        }

        let result = parsed.get("result").ok_or("login: no result field")?;
        let job = result.get("job").ok_or("login: no job in result")?;

        let initial_job = StratumJob {
            job_id: job["job_id"].as_str().unwrap_or("").to_string(),
            blob: job["blob"].as_str().unwrap_or("").to_string(),
            target: job["target"].as_str().unwrap_or("").to_string(),
            height: job["height"].as_u64().unwrap_or(0),
            seed_hash: job["seed_hash"].as_str().unwrap_or("").to_string(),
        };

        info!(
            target: "dwowd_client",
            "Stratum connected, job_id={}, height={}",
            initial_job.job_id, initial_job.height,
        );

        let current_job = Arc::new(Mutex::new(Some(initial_job)));
        let pending: Arc<Mutex<HashMap<u64, PendingSubmit>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (write_tx, write_rx) = channel::unbounded::<String>();

        // Spawn background reader
        let reader_job = current_job.clone();
        let reader_pending = pending.clone();
        std::thread::spawn(move || {
            Self::reader_loop(reader, writer, write_rx, reader_job, reader_pending);
        });

        Ok(Self {
            current_job,
            next_id: AtomicU64::new(1000),
            pending,
            write_tx,
            rpc_url,
        })
    }

    /// Background reader: reads lines from stratum, dispatches notifications
    /// and routes responses to pending submit handlers.
    fn reader_loop(
        mut reader: BufReader<TcpStream>,
        mut writer: TcpStream,
        write_rx: channel::Receiver<String>,
        current_job: Arc<Mutex<Option<StratumJob>>>,
        pending: Arc<Mutex<HashMap<u64, PendingSubmit>>>,
    ) {
        loop {
            // Check for outgoing writes first
            while let Ok(line) = write_rx.try_recv() {
                if let Err(e) = writer.write_all(line.as_bytes()) {
                    warn!(target: "dwowd_client", "Stratum write error: {e}");
                    return;
                }
                if let Err(e) = writer.flush() {
                    warn!(target: "dwowd_client", "Stratum flush error: {e}");
                    return;
                }
            }

            // Read one line from stratum
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    warn!(target: "dwowd_client", "Stratum connection closed by peer");
                    return;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(target: "dwowd_client", "Stratum read error: {e}");
                    return;
                }
            }

            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            // Parse to determine if this is a notification or a response
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&line);
            let Ok(msg) = parsed else {
                debug!(target: "dwowd_client", "Unparseable stratum line: {line}");
                continue;
            };

            // Job push notification (has "method":"job", no "id")
            if msg.get("method").and_then(|m| m.as_str()) == Some("job") {
                if let Some(params) = msg.get("params") {
                    let job = StratumJob {
                        job_id: params["job_id"].as_str().unwrap_or("").to_string(),
                        blob: params["blob"].as_str().unwrap_or("").to_string(),
                        target: params["target"].as_str().unwrap_or("").to_string(),
                        height: params["height"].as_u64().unwrap_or(0),
                        seed_hash: params["seed_hash"].as_str().unwrap_or("").to_string(),
                    };
                    info!(
                        target: "dwowd_client",
                        "New job push: height={}, job_id={}",
                        job.height, job.job_id,
                    );
                    smol::block_on(async {
                        *current_job.lock().await = Some(job);
                    });
                }
                continue;
            }

            // Response to a previous request (has "id")
            if let Some(id) = msg.get("id").and_then(|i| i.as_u64()) {
                smol::block_on(async {
                    let mut guard = pending.lock().await;
                    if let Some(submit) = guard.remove(&id) {
                        let _ = submit.tx.send(line).await;
                    }
                });
                continue;
            }

            debug!(target: "dwowd_client", "Unhandled stratum message: {line}");
        }
    }

    /// Get the latest cached stratum job.
    pub async fn current_job(&self) -> Option<StratumJob> {
        self.current_job.lock().await.clone()
    }

    /// Submit a solved block to dwowd stratum.
    /// Uses the background reader to avoid job-notification corruption.
    pub async fn submit_solution(
        &self,
        job_id: &str,
        nonce_hex: &str,
        hash_hex: &str,
    ) -> Result<String, String> {
        let req_id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let submit = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "submit",
            "params": {
                "id": "adaptor",
                "job_id": job_id,
                "nonce": nonce_hex,
                "result": hash_hex,
            },
            "id": req_id
        });
        let submit_str = serde_json::to_string(&submit).unwrap() + "\n";

        let (tx, rx) = channel::bounded::<String>(1);
        {
            let mut guard = self.pending.lock().await;
            guard.insert(req_id, PendingSubmit { tx });
        }

        // Send via background writer
        self.write_tx
            .send(submit_str)
            .await
            .map_err(|e| format!("write channel closed: {e}"))?;

        // Wait for response with timeout
        let response = smol::future::or(
            async {
                rx.recv()
                    .await
                    .map_err(|e| format!("response channel closed: {e}"))
            },
            async {
                smol::Timer::after(Duration::from_secs(10)).await;
                Err("submit timeout".to_string())
            },
        )
        .await?;

        debug!(target: "dwowd_client", "Submit response: {}", response);

        let parsed: serde_json::Value = serde_json::from_str(&response)
            .map_err(|e| format!("submit response parse: {e}"))?;

        if let Some(err) = parsed.get("error") {
            return Err(format!("submit error: {err}"));
        }

        let result = parsed.get("result").ok_or("submit: no result")?;
        let status = result["status"].as_str().unwrap_or("rejected").to_string();

        Ok(status)
    }

    /// Query dwowd JSON-RPC for chain state (raw TCP newline-delimited).
    async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        let body_str = serde_json::to_string(&body).unwrap() + "\n";

        let mut stream = smol::net::TcpStream::connect(&self.rpc_url)
            .await
            .map_err(|e| format!("RPC connect failed: {e}"))?;
        stream
            .set_nodelay(true)
            .map_err(|_| "set_nodelay failed")?;

        use smol::io::AsyncWriteExt;
        stream
            .write_all(body_str.as_bytes())
            .await
            .map_err(|e| format!("RPC write failed: {e}"))?;

        use smol::io::AsyncBufReadExt;
        let mut reader = smol::io::BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("RPC read failed: {e}"))?;

        let parsed: serde_json::Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("RPC parse error: {e}"))?;

        Ok(parsed)
    }

    /// Query a block header by height from dwowd.
    ///
    /// Returns a Monero-compatible `block_header` JSON object for p2pool's
    /// sidechain initialization. p2pool calls this to download historical
    /// block headers starting from seed height 0.
    pub async fn get_block_header_by_height(
        &self,
        height: u64,
    ) -> Result<serde_json::Value, String> {
        let resp = self
            .rpc_call(
                "blockchain.get_block_linear",
                serde_json::json!([height]),
            )
            .await?;

        let block_json_str = resp
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or("No result in get_block_linear response")?;

        let block: serde_json::Value = serde_json::from_str(block_json_str)
            .map_err(|e| format!("Failed to parse block JSON: {e}"))?;

        // Compute deterministic block hash from the block JSON
        let hash = blake3::hash(block_json_str.as_bytes())
            .to_hex()
            .to_string();

        let header = block.get("header").ok_or("No header in block")?;

        let prev_hash = header
            .get("previous")
            .and_then(|v| v.as_str())
            .unwrap_or("0000000000000000000000000000000000000000000000000000000000000000")
            .to_string();

        let block_height = header.get("height").and_then(|v| v.as_u64()).unwrap_or(height);
        let timestamp = header.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
        let nonce = header.get("nonce").and_then(|v| v.as_u64()).unwrap_or(0);
        let target = header.get("target").and_then(|v| v.as_u64()).unwrap_or(0);

        // Derive difficulty from target (Monero-style: max_u64 / target)
        let difficulty = if target > 0 {
            u64::MAX / target
        } else {
            1u64
        };

        Ok(serde_json::json!({
            "block_size": 0,
            "depth": 0,
            "difficulty": difficulty,
            "hash": hash,
            "height": block_height,
            "major_version": 16,
            "minor_version": 16,
            "nonce": nonce,
            "num_txes": 0,
            "orphan_status": false,
            "prev_hash": prev_hash,
            "reward": 0,
            "timestamp": timestamp,
        }))
    }

    /// Query the latest confirmed block info.
    ///
    /// Uses `blockchain.last_confirmed_block` first; falls back to the linear
    /// chain RPC (`get_block_linear`) for DarkWow linear nodes where the
    /// standard `last()` method operates on a shadow blockchain.
    pub async fn get_last_block_info(&self) -> Result<(u64, String, u64), String> {
        // Try standard last_confirmed_block first
        if let Ok(resp) = self
            .rpc_call("blockchain.last_confirmed_block", serde_json::json!([]))
            .await
        {
            if let Some(result) = resp.get("result") {
                let height = result[0].as_u64().unwrap_or(0);
                let hash = result[1].as_str().unwrap_or("").to_string();
                let timestamp = result[2].as_u64().unwrap_or(0);
                return Ok((height, hash, timestamp));
            }
        }

        // Fallback: use stratum job height and query get_block_linear
        let job = self.current_job().await;
        let height = job.map(|j| j.height).unwrap_or(1);

        // Query the latest block via get_block_linear
        let resp = self
            .rpc_call(
                "blockchain.get_block_linear",
                serde_json::json!([height.saturating_sub(1)]),
            )
            .await?;

        let block_json = resp
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or("No result in get_block_linear response")?;

        let block: serde_json::Value = serde_json::from_str(block_json)
            .map_err(|e| format!("Failed to parse block JSON: {e}"))?;

        // Hash the block JSON to produce a deterministic block hash
        let hash = blake3::hash(block_json.as_bytes()).to_hex().to_string();

        Ok((height.saturating_sub(1), hash, 0))
    }
}
