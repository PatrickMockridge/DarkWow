//! Anchor a block to Arweave via ArDrive Turbo.
//!
//! POSTs a signed ANS-104 DataItem to `https://upload.ardrive.io/v1/tx/arweave`.
//! Small uploads (under ~100KB) are free from unfunded wallets.
//! Returns the data item's ID (SHA-256 of the raw signature) which serves
//! as the Arweave transaction ID for later verification.

use std::time::Duration;

use serde::Deserialize;

use super::data_item::DataItem;
use super::wallet::CaribinaWallet;
use crate::block::BlockHeader;

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

/// Build the signed ANS-104 DataItem that *is* a block's Caribina anchor proof.
///
/// **Pure** — no network. This is the half that consensus depends on: the bytes it returns go into
/// `header.caribina_anchor`, and `caribina::verify_anchor_proof` accepts or rejects them locally.
/// Publication is [`publish_anchor_proof`], and it is deliberately separate, because a block's
/// finality must not depend on whether an HTTP POST succeeded on the miner's machine.
///
/// The payload binds `anchor_commitment(header)` — a hash over the header with its post-mining
/// fields zeroed, so it can be computed before the proof it is about exists (see that function) —
/// plus the block's height and timestamp. `anchor_owner` must already be set on `header` and must be
/// `wallet`'s public key, or the proof will not verify: that equality is what makes the anchor the
/// miner's claim rather than anyone's.
pub fn build_anchor_proof(header: &BlockHeader, wallet: &CaribinaWallet) -> Vec<u8> {
    let commitment = super::verify::anchor_commitment(header);

    let mut payload = Vec::with_capacity(48);
    payload.extend_from_slice(&commitment);
    payload.extend_from_slice(&header.timestamp.get().to_le_bytes());
    payload.extend_from_slice(&header.height.to_le_bytes());

    let mut item = DataItem::new(&payload);
    item.sign(wallet);
    item.as_bytes().to_vec()
}

/// Publish an already-built anchor proof to ArDrive Turbo. Best-effort, and *only* that.
///
/// Returns the Arweave transaction id on success and `None` on any failure — network, Turbo
/// rejection, malformed response — so a miner can proceed with an unanchored block. That failure
/// costs the block its finality and nothing else: the block is still valid, and
/// `verify_anchor_proof` will return `false` for it, which is the honest outcome rather than a
/// silently-claimed anchor.
pub fn publish_anchor_proof(proof: &[u8]) -> Option<[u8; 32]> {
    let response = post_to_turbo(proof)?;
    base64url_to_bytes(&response.id)
}

/// POST raw DataItem bytes to ArDrive Turbo.
fn post_to_turbo(data: &[u8]) -> Option<TurboUploadResponse> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent();
    let response = agent.post(TURBO_UPLOAD_URL)
        .header("Content-Type", "application/octet-stream")
        .header("Connection", "close")
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
    // base64url → standard base64 with padding
    let mut std_base64 = s.replace('-', "+").replace('_', "/");
    let pad = (4 - (std_base64.len() % 4)) % 4;
    std_base64.push_str(&"=".repeat(pad));
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &std_base64,
    )
    .ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(arr)
}
