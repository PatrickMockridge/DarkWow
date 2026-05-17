//! Anchor a block to Arweave via ArDrive Turbo.
//!
//! POSTs a signed ANS-104 DataItem to `https://upload.ardrive.io/v1/tx/arweave`.
//! Small uploads (under ~100KB) are free from unfunded wallets.
//! Returns the data item's ID (SHA-256 of the raw signature) which serves
//! as the Arweave transaction ID for later verification.

use serde::Deserialize;

use super::data_item::DataItem;
use super::wallet::CaribinaWallet;

/// ArDrive Turbo upload endpoint
pub const TURBO_UPLOAD_URL: &str = "https://upload.ardrive.io/v1/tx/arweave";

/// Response from ArDrive Turbo's upload endpoint.
#[derive(Debug, Deserialize)]
struct TurboUploadResponse {
    /// The data item's ID (base64url-encoded SHA-256 of raw signature)
    id: String,
    /// The signer's Arweave public address (base64url-encoded public key)
    #[serde(default)]
    #[allow(dead_code)]
    owner: String,
    /// Cost in winc (Turbo Credits), "0" for free uploads
    #[serde(default)]
    #[allow(dead_code)]
    winc: String,
}

/// Error type for anchoring operations.
#[derive(Debug, thiserror::Error)]
pub enum AnchorError {
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("Turbo upload rejected: {0}")]
    Rejected(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Anchor block data to Arweave and return the transaction ID.
///
/// Creates a new Ed25519 wallet, builds and signs a DataItem containing
/// `block_hash || timestamp || height`, and POSTs it to ArDrive Turbo.
/// Returns the data item ID (32-byte SHA-256 hash) which serves as the
/// permanent Arweave reference.
///
/// The wallet is cycled on every call — no address reuse, no tracking.
///
/// Returns `None` if anchoring fails (network error, Turbo rejection)
/// so that mining can proceed without the anchor.
pub fn anchor_block(
    block_hash: &[u8; 32],
    timestamp: u64,
    height: u64,
) -> Option<[u8; 32]> {
    // Build anchor payload: block_hash || timestamp || height
    let mut payload = Vec::with_capacity(48);
    payload.extend_from_slice(block_hash);
    payload.extend_from_slice(&timestamp.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());

    // Build and sign DataItem
    let wallet = CaribinaWallet::generate();
    let mut item = DataItem::new(&payload);
    item.sign(&wallet);
    let binary = item.as_bytes();

    // POST to ArDrive Turbo (blocking for now; async later)
    let response = post_to_turbo(binary)?;

    // Parse the ID from base64url to raw bytes
    let id = base64url_to_bytes(&response.id)?;

    Some(id)
}

/// POST raw DataItem bytes to ArDrive Turbo.
fn post_to_turbo(data: &[u8]) -> Option<TurboUploadResponse> {
    let response = ureq::post(TURBO_UPLOAD_URL)
        .header("Content-Type", "application/octet-stream")
        .send(data)
        .ok()?;

    if response.status() != 200 {
        let status = response.status().as_u16();
        let body = response.into_body().read_to_string().unwrap_or_default();
        tracing::warn!(
            "Turbo upload rejected: HTTP {} — {}",
            status,
            body
        );
        return None;
    }

    let mut body = response.into_body();
    let json_str = body.read_to_string().unwrap_or_default();
    let turbo_response: TurboUploadResponse = serde_json::from_str(&json_str).ok()?;
    Some(turbo_response)
}

/// Decode a base64url string to a 32-byte array.
fn base64url_to_bytes(s: &str) -> Option<[u8; 32]> {
    // base64url → standard base64
    let std_base64 = s.replace('-', "+").replace('_', "/");
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        std_base64,
    )
    .ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(arr)
}
