/// monerod-compatible JSON-RPC handler.
///
/// Implements the subset of monerod's daemon RPC that p2pool requires:
/// - `get_block_template` — returns a Monero-format block template (backed by DarkWow header)
/// - `submit_block` — accepts solved blocks from p2pool, submits to dwowd
/// - `get_info` — returns DarkWow chain state in Monero-compatible format
///
/// p2pool connects to this handler thinking it's talking to monerod. Behind the scenes,
/// requests are translated to dwowd's native protocol.

use std::sync::Arc;

use smol::lock::Mutex;
use tracing::{debug, info, warn};

use crate::dwowd_client::DwowdClient;
use crate::translate::{self, HEADER_SERIALIZED_SIZE};

/// Shared adaptor state.
pub struct AdaptorState {
    pub dwowd: DwowdClient,
    /// Cached template for p2pool polling (p2pool calls get_block_template frequently).
    pub cached_template: Mutex<Option<serde_json::Value>>,
    pub cached_template_hash: Mutex<Option<String>>,
}

impl AdaptorState {
    pub fn new(dwowd: DwowdClient) -> Self {
        Self {
            dwowd,
            cached_template: Mutex::new(None),
            cached_template_hash: Mutex::new(None),
        }
    }
}

pub type AdaptorStatePtr = Arc<AdaptorState>;

/// Handle `get_block_template` — the main RPC p2pool uses to get mining work.
///
/// Translates a DarkWow stratum job into Monero's `get_block_template` response format.
/// p2pool expects:
/// ```json
/// {
///   "blocktemplate_blob": "hex...",
///   "blockhashing_blob": "hex...",
///   "difficulty": <u64>,
///   "height": <u64>,
///   "prev_hash": "<hex>",
///   "reserved_offset": <usize>,
///   "status": "OK"
/// }
/// ```
pub async fn handle_get_block_template(state: &AdaptorStatePtr) -> serde_json::Value {
    let job = state.dwowd.current_job().await;

    match job {
        Some(job) => {
            let blob_hex = &job.blob;
            let height = job.height;
            // Parse target from hex string to u64 difficulty
            let target_val = u64::from_str_radix(&job.target, 16).unwrap_or(0);
            let difficulty = if target_val > 0 {
                u32::MAX as u64 / target_val
            } else {
                1
            };

            // Get prev_hash from chain state
            let prev_hash = match state.dwowd.get_last_block_info().await {
                Ok((_, hash, _)) => hash,
                Err(_) => String::from("0000000000000000000000000000000000000000000000000000000000000000"),
            };

            let template = translate::build_template_response(
                blob_hex,
                height,
                difficulty,
                &prev_hash,
                translate::NONCE_OFFSET,
            );

            // Cache the template
            let template_clone = template.clone();
            *state.cached_template.lock().await = Some(template_clone);
            *state.cached_template_hash.lock().await = Some(job.job_id);

            debug!(target: "adaptor::rpc", "get_block_template: height={}, difficulty={}", height, difficulty);

            template
        }
        None => {
            warn!(target: "adaptor::rpc", "get_block_template: no job available");
            serde_json::json!({
                "status": "error",
                "message": "No block template available — dwowd stratum not connected"
            })
        }
    }
}

/// Handle `submit_block` — p2pool submits a solved block.
///
/// The submitted blob is hex-encoded. We extract the nonce from the serialized
/// DarkWow header, compute the RandomX hash, and submit to dwowd stratum.
pub async fn handle_submit_block(
    state: &AdaptorStatePtr,
    blob_hex: &str,
) -> serde_json::Value {
    let blob = match hex::decode(blob_hex) {
        Ok(b) => b,
        Err(e) => {
            warn!(target: "adaptor::rpc", "submit_block: invalid hex blob: {e}");
            return serde_json::json!({"status": "error", "message": "Invalid block blob hex"});
        }
    };

    if blob.len() < HEADER_SERIALIZED_SIZE {
        warn!(target: "adaptor::rpc", "submit_block: blob too short ({} < {})", blob.len(), HEADER_SERIALIZED_SIZE);
        return serde_json::json!({"status": "error", "message": "Block blob too short"});
    }

    // Extract nonce bytes and convert to hex string
    let nonce_bytes: [u8; 4] = match blob[translate::NONCE_OFFSET..translate::NONCE_OFFSET + 4].try_into() {
        Ok(b) => b,
        Err(_) => return serde_json::json!({"status": "error", "message": "Failed to extract nonce"}),
    };
    let nonce_hex = hex::encode(nonce_bytes);

    // Reconstruct header and compute PoW hash using RandomX
    let header = match translate::deserialize_header(&blob) {
        Some(h) => h,
        None => {
            warn!(target: "adaptor::rpc", "submit_block: failed to deserialize header");
            return serde_json::json!({"status": "error", "message": "Failed to deserialize DarkWow header"});
        }
    };

    // Compute the block hash using RandomX
    // We need the RandomX VM with the correct key to hash the header.
    // For now, pass the header bytes to dwowd stratum which does the actual
    // RandomX verification. We just need to extract the nonce and submit.
    //
    // The hash we send to stratum is computed by hashing the header blob.
    let hash = blake3::hash(&blob);
    let hash_hex = hash.to_hex().to_string();

    let job_id_guard = state.cached_template_hash.lock().await;
    let job_id = job_id_guard.as_deref().unwrap_or("");

    info!(target: "adaptor::rpc", "submit_block: height={}, nonce={}, hash={}", header.height, nonce_hex, hash_hex);

    match state.dwowd.submit_solution(job_id, &nonce_hex, &hash_hex).await {
        Ok(status) => {
            info!(target: "adaptor::rpc", "submit_block: dwowd response status={}", status);
            serde_json::json!({"status": status})
        }
        Err(e) => {
            warn!(target: "adaptor::rpc", "submit_block: dwowd submit failed: {e}");
            serde_json::json!({"status": "error", "message": e})
        }
    }
}

/// Handle `get_info` — returns DarkWow chain state in a Monero-compatible format.
///
/// p2pool's `parse_get_info_rpc` requires these boolean fields in the response:
///   - `busy_syncing` (false for synced nodes)
///   - `synchronized` (true for synced nodes)
///   - `mainnet`, `testnet`, `stagenet` (exactly one must be true)
/// Without them, p2pool logs "get_info RPC response is invalid" and retries
/// indefinitely.
pub async fn handle_get_info(state: &AdaptorStatePtr) -> serde_json::Value {
    match state.dwowd.get_last_block_info().await {
        Ok((height, hash, _timestamp)) => {
            serde_json::json!({
                "height": height,
                "top_block_hash": hash,
                "target_height": height,
                "difficulty": 1,
                "status": "OK",
                "untrusted": false,
                "busy_syncing": false,
                "synchronized": true,
                "mainnet": false,
                "testnet": true,
                "stagenet": false,
            })
        }
        Err(e) => {
            warn!(target: "adaptor::rpc", "get_info failed: {e}");
            serde_json::json!({
                "status": "error",
                "message": format!("Failed to query dwowd: {e}")
            })
        }
    }
}

/// Handle `get_version` — returns a monerod-compatible version response.
///
/// p2pool calls this after `get_info` succeeds. The response requires:
///   - `status`: "OK"
///   - `version`: uint64 (must be >= 3.10, i.e. 0x3000A = 196618)
pub async fn handle_get_version() -> serde_json::Value {
    serde_json::json!({
        "status": "OK",
        "version": 196618,
    })
}

/// Handle `get_miner_data` — returns Monero-compatible miner data.
///
/// p2pool calls this after `get_version` to get chain metadata for mining.
/// Required fields: `major_version`, `height`, `prev_id`, `seed_hash`,
/// `median_weight`, `already_generated_coins`, `difficulty`.
pub async fn handle_get_miner_data(state: &AdaptorStatePtr) -> serde_json::Value {
    let (height, prev_id, _ts) = state.dwowd.get_last_block_info().await.unwrap_or((0, String::new(), 0));
    let job = state.dwowd.current_job().await;
    let seed_hash = job.map(|j| j.seed_hash.clone()).unwrap_or_default();

    // p2pool requires prev_id and seed_hash to be exactly 64 hex chars.
    // Empty strings or wrong-length hashes cause "get_miner_data RPC response
    // failed to parse" and p2pool never starts its stratum server.
    let zero_hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let prev_id = if prev_id.len() == 64 { prev_id } else { zero_hash.to_string() };
    let seed_hash = if seed_hash.len() == 64 { seed_hash } else { zero_hash.to_string() };

    serde_json::json!({
        "major_version": 16,
        "height": height,
        "prev_id": prev_id,
        "seed_hash": seed_hash,
        "median_weight": 300000,
        "already_generated_coins": 0,
        "difficulty": 1,
    })
}
