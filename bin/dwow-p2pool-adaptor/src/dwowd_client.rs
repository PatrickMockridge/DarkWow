/// Client for communicating with the co-located dwowd node.
///
/// The adaptor connects to dwowd's stratum server to get block templates and submit
/// solutions, and to dwowd's JSON-RPC for chain state queries. The stratum protocol
/// is reused because it already handles miner registration, job distribution, and
/// solution submission — the adaptor is just another stratum client from dwowd's
/// perspective.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::Arc;

use smol::lock::Mutex;
use tracing::{debug, info};

/// A stratum job received from dwowd, cached for translation to p2pool.
#[derive(Clone, Debug)]
pub struct StratumJob {
    pub job_id: String,
    /// Hex-encoded block header with zeroed nonce (the mining blob).
    pub blob: String,
    /// Target difficulty for this job, as a hex string.
    pub target: String,
    /// Block height.
    pub height: u64,
    /// RandomX seed hash.
    pub seed_hash: String,
}

/// Connection state for the stratum client.
struct StratumConnection {
    stream: TcpStream,
    client_id: String,
}

/// A dwowd client that maintains a stratum connection and provides chain state.
pub struct DwowdClient {
    /// Cached stratum connection (reconnected on failure).
    conn: Arc<Mutex<Option<StratumConnection>>>,
    /// Latest job from dwowd stratum.
    current_job: Arc<Mutex<Option<StratumJob>>>,
    /// dwowd RPC URL for chain state queries.
    rpc_url: String,
    /// dwowd stratum URL.
    stratum_url: String,
}

impl DwowdClient {
    pub fn new(rpc_url: String, stratum_url: String) -> Self {
        Self {
            conn: Arc::new(Mutex::new(None)),
            current_job: Arc::new(Mutex::new(None)),
            rpc_url,
            stratum_url,
        }
    }

    /// Connect to dwowd's stratum server and log in.
    pub async fn connect(&self) -> Result<(), String> {
        let stratum_url = self.stratum_url.clone();
        let stream = TcpStream::connect(&stratum_url)
            .map_err(|e| format!("Failed to connect to dwowd stratum at {stratum_url}: {e}"))?;
        stream
            .set_nonblocking(false)
            .map_err(|e| format!("Failed to set blocking mode: {e}"))?;

        let mut conn = StratumConnection {
            stream,
            client_id: String::new(),
        };

        // Send login request
        let login = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "login",
            "params": {
                "login": "p2pool-adaptor",
                "pass": "x",
                "agent": "dwow-p2pool-adaptor",
                "algo": ["rx/0"]
            },
            "id": 1
        });
        let login_str = serde_json::to_string(&login).unwrap() + "\n";
        conn.stream
            .write_all(login_str.as_bytes())
            .map_err(|e| format!("Stratum login write failed: {e}"))?;

        let mut reader = BufReader::new(conn.stream.try_clone().unwrap());
        let mut response = String::new();
        reader
            .read_line(&mut response)
            .map_err(|e| format!("Stratum login read failed: {e}"))?;

        let parsed: serde_json::Value =
            serde_json::from_str(&response).map_err(|e| format!("Stratum login parse: {e}"))?;

        if let Some(err) = parsed.get("error") {
            return Err(format!("Stratum login error: {err}"));
        }

        let result = parsed
            .get("result")
            .ok_or("Stratum login: no result")?;
        let job = result
            .get("job")
            .ok_or("Stratum login: no job")?;

        let client_id = result
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("adaptor")
            .to_string();
        conn.client_id = client_id;

        // Cache initial job
        let stratum_job = StratumJob {
            job_id: job["job_id"].as_str().unwrap_or("").to_string(),
            blob: job["blob"].as_str().unwrap_or("").to_string(),
            target: job["target"].as_str().unwrap_or("").to_string(),
            height: job["height"].as_u64().unwrap_or(0),
            seed_hash: job["seed_hash"].as_str().unwrap_or("").to_string(),
        };

        info!(
            target: "dwowd_client",
            "Stratum connected, job_id={}, height={}",
            stratum_job.job_id, stratum_job.height,
        );

        *self.conn.lock().await = Some(conn);
        *self.current_job.lock().await = Some(stratum_job);

        Ok(())
    }

    /// Get the latest cached stratum job.
    pub async fn current_job(&self) -> Option<StratumJob> {
        self.current_job.lock().await.clone()
    }

    /// Submit a solved block to dwowd stratum.
    pub async fn submit_solution(
        &self,
        job_id: &str,
        nonce_hex: &str,
        hash_hex: &str,
    ) -> Result<String, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or("Not connected to dwowd stratum")?;

        let submit = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "submit",
            "params": {
                "id": &conn.client_id,
                "job_id": job_id,
                "nonce": nonce_hex,
                "result": hash_hex,
            },
            "id": 2
        });
        let submit_str = serde_json::to_string(&submit).unwrap() + "\n";
        conn.stream
            .write_all(submit_str.as_bytes())
            .map_err(|e| format!("Stratum submit write failed: {e}"))?;

        let mut reader = BufReader::new(conn.stream.try_clone().unwrap());
        let mut response = String::new();
        reader
            .read_line(&mut response)
            .map_err(|e| format!("Stratum submit read failed: {e}"))?;

        debug!(target: "dwowd_client", "Submit response: {}", response);

        let parsed: serde_json::Value = serde_json::from_str(&response)
            .map_err(|e| format!("Stratum submit parse: {e}"))?;

        if let Some(err) = parsed.get("error") {
            return Err(format!("Stratum submit error: {err}"));
        }

        let result = parsed
            .get("result")
            .ok_or("Stratum submit: no result")?;
        let status = result["status"]
            .as_str()
            .unwrap_or("rejected")
            .to_string();

        Ok(status)
    }

    /// Query dwowd JSON-RPC for chain state.
    async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = &self.rpc_url;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });
        let body_str = serde_json::to_string(&body).unwrap();

        let response = smol::net::TcpStream::connect(url)
            .await
            .map_err(|e| format!("RPC connect failed: {e}"))?;
        response
            .set_nodelay(true)
            .map_err(|_| "set_nodelay failed")?;

        let mut writer = response.clone();
        let request = format!(
            "POST / HTTP/1.1\r\nHost: {url}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
            body_str.len(),
        );
        use smol::io::AsyncWriteExt;
        writer
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("RPC write failed: {e}"))?;

        use smol::io::AsyncReadExt;
        let mut reader = response;
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("RPC read failed: {e}"))?;

        let response_str =
            String::from_utf8(buf).map_err(|e| format!("RPC invalid UTF-8: {e}"))?;

        // Extract JSON body from HTTP response
        let body_start = response_str
            .find("\r\n\r\n")
            .map(|i| i + 4)
            .unwrap_or(0);
        let json_body = &response_str[body_start..];

        let parsed: serde_json::Value = serde_json::from_str(json_body)
            .map_err(|e| format!("RPC parse error: {e}"))?;

        Ok(parsed)
    }

    /// Query the latest confirmed block info.
    pub async fn get_last_block_info(&self) -> Result<(u64, String, u64), String> {
        let resp = self
            .rpc_call("blockchain.last_confirmed_block", serde_json::json!([]))
            .await?;

        let result = resp.get("result").ok_or("No result in RPC response")?;
        let height = result[0].as_u64().unwrap_or(0);
        let hash = result[1].as_str().unwrap_or("").to_string();
        let timestamp = result[2].as_u64().unwrap_or(0);

        Ok((height, hash, timestamp))
    }
}
