//! Monero DLEq (Discrete Log Equality) proof verification.
//!
//! Proves that the depositor owns the Monero one-time address that received
//! the deposit. Uses a cross-curve DLEq proof: the same secret key derives
//! a Pallas public key (DarkWow side) and a Montgomery/Ed25519 public key
//! (Monero side).
//!
//! ## Two-Layer Verification
//!
//! Layer 1 (structural): amount minimum, confirmations threshold, non-zero fields.
//! Layer 2 (cryptographic): cross-curve DLEq Fiat-Shamir challenge verification.

use curve25519_dalek::{
    constants::X25519_BASEPOINT,
    edwards::EdwardsPoint,
    montgomery::MontgomeryPoint,
    scalar::Scalar as Curve25519Scalar,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::*,
        poseidon_hash,
    },
    error::ContractResult,
    pasta::{
        group::GroupEncoding,
        pallas,
    },
};
use crate::error::BridgeError;
use crate::model::XmrDepositProof;

/// Minimum Monero deposit in piconero (0.01 XMR).
const MIN_XMR_DEPOSIT: u64 = 10_000_000_000;
/// Required block confirmations for Monero deposits.
const XMR_CONFIRMATIONS: u64 = 10;

/// Verify a Monero DLEq proof of deposit address ownership.
///
/// # Protocol
///
/// The depositor knows a secret scalar `x` such that:
/// - `P_pallas = x * G_pallas` (DarkWow identity)
/// - `P_curve25519 = x * G_curve25519` (Monero one-time address)
///
/// The DLEq proof shows `log_Gp(P_p) == log_Gc(P_c)` without revealing `x`.
/// The prover generates random `r1, r2`, computes commitments, derives
/// challenge via Fiat-Shamir, then responds with `s1 = r1 + c*x` and
/// `s2 = r2 + c*x` (mod group orders).
///
/// The verifier reconstructs `R1 = s*G - c*P` on each curve and checks
/// the challenge matches.
pub fn verify_dleq_proof(proof: &XmrDepositProof) -> ContractResult {
    // ── Layer 1: Structural checks ──────────────────────────────────

    if proof.tx_hash.iter().all(|&b| b == 0) {
        return Err(BridgeError::InvalidDeposit(
            "Monero tx_hash is zero".into(),
        ).into());
    }
    if proof.block_height == 0 {
        return Err(BridgeError::InvalidDeposit(
            "Monero block_height is zero".into(),
        ).into());
    }
    if proof.amount < MIN_XMR_DEPOSIT {
        return Err(BridgeError::InvalidDeposit(
            format!(
                "Monero amount {} below minimum {} piconero",
                proof.amount, MIN_XMR_DEPOSIT,
            ).into(),
        ).into());
    }
    if proof.confirmations < XMR_CONFIRMATIONS {
        return Err(BridgeError::InsufficientConfirmations.into());
    }
    if proof.coinbase_merkle_proof.is_empty() {
        return Err(BridgeError::InvalidMerkleProof.into());
    }
    if proof.ephemeral_pub.iter().all(|&b| b == 0) {
        return Err(BridgeError::InvalidDeposit(
            "Monero ephemeral_pub is zero".into(),
        ).into());
    }

    // ── Layer 2: DLEq cryptographic verification ────────────────────

    let chal_r1 = &proof.dleq_proof.challenge_response_1;
    let chal_r2 = &proof.dleq_proof.challenge_response_2;
    let challenge = &proof.dleq_proof.challenge;

    // Convert response scalars to Pallas scalars.
    // Uses the `PrimeField` trait (re-exported via pasta_prelude).
    let r1_pallas = to_pallas_scalar(chal_r1, "challenge_response_1")?;
    let r2_pallas = to_pallas_scalar(chal_r2, "challenge_response_2")?;
    let c_pallas = to_pallas_scalar(challenge, "challenge")?;

    // Critical: verify scalars fit in Curve25519 field.
    // The Pallas scalar field (~2^254) is larger than the Curve25519
    // scalar field (~2^252). A scalar valid in Pallas may overflow
    // Curve25519, which would break the DLEq equality.
    let r2_curve = Curve25519Scalar::from_canonical_bytes(*chal_r2)
        .into_option()
        .ok_or_else(|| BridgeError::InvalidDeposit(
            "Monero DLEq challenge_response_2 exceeds Curve25519 scalar field".into(),
        ))?;
    let c_curve = Curve25519Scalar::from_canonical_bytes(*challenge)
        .into_option()
        .ok_or_else(|| BridgeError::InvalidDeposit(
            "Monero DLEq challenge exceeds Curve25519 scalar field".into(),
        ))?;

    // Parse ephemeral_pub on both curves.
    // Uses `GroupEncoding` trait for from_bytes.
    let p_pallas = Option::<pallas::Point>::from(
        pallas::Point::from_bytes(&proof.ephemeral_pub)
    ).ok_or_else(|| BridgeError::InvalidDeposit(
        "Monero ephemeral_pub is not a valid Pallas point".into(),
    ))?;
    let p_curve = MontgomeryPoint(proof.ephemeral_pub);

    // Generators.
    // Uses `Curve`/`Group` traits from pasta_prelude.
    let g_pallas = <pallas::Point as Group>::generator();
    // Secondary generator H_p = domain-separated point.
    // Domain: "DLEq-H-pallas" truncated to u64 since Field::from only
    // supports u64 for non-uniform sources.
    let h_pallas_domain = pallas::Base::from(0x444c4571_5f485f70u64);
    // Map to curve: multiply generator by domain scalar.
    // Field::from produces Base, but we need Scalar. Use from_repr/to_repr
    // round-trip: Base -> repr -> Scalar (this is fine for domain sep).
    let h_pallas_scalar = pallas::Scalar::from_repr(
        h_pallas_domain.to_repr()
    ).unwrap(); // domain scalar is always a valid Scalar repr
    let h_pallas = g_pallas * h_pallas_scalar;

    let g_curve = X25519_BASEPOINT;
    // Secondary generator H_c = domain-separated point.
    let h_curve_scalar = Curve25519Scalar::from_canonical_bytes({
        let mut b = [0u8; 32];
        b[..8].copy_from_slice(&0x444c45715f485f32u64.to_le_bytes());
        b
    }).into_option().unwrap_or(Curve25519Scalar::from(1u64));
    let h_curve_pt = h_curve_scalar * g_curve;

    // Reconstruct R1 commitments on each curve.
    // On Pallas: R1_p = response_1 * G_p - challenge * P_p
    // Group trait provides *, +, and Neg.
    let r1_pallas_pt = (g_pallas * r1_pallas) + (-(p_pallas * c_pallas));

    // On Curve25519: R1_c = response_2 * G_c - challenge * P_c
    // MontgomeryPoint has no Sub or Neg. Convert to EdwardsPoint for
    // subtraction, then back to Montgomery.
    let g_ed = g_curve
        .to_edwards(0)
        .ok_or_else(|| BridgeError::InvalidDeposit(
            "Monero DLEq: basepoint maps to infinity".into(),
        ))?;
    let p_ed = p_curve
        .to_edwards(0)
        .ok_or_else(|| BridgeError::InvalidDeposit(
            "Monero DLEq: ephemeral_pub maps to infinity".into(),
        ))?;
    let r1_ed = (g_ed * r2_curve) - (p_ed * c_curve);
    let r1_curve_pt = r1_ed.to_montgomery();

    // Fiat-Shamir transcript: hash(G_p || H_p || P_p || G_c || H_c || P_c || R1_p || R1_c)
    // Use Poseidon (DarkWow's native hash) for the Fiat-Shamir transform.
    let mut transcript = Vec::with_capacity(32 * 8);
    transcript.extend_from_slice(g_pallas.to_bytes().as_slice());
    transcript.extend_from_slice(h_pallas.to_bytes().as_slice());
    transcript.extend_from_slice(&proof.ephemeral_pub);
    transcript.extend_from_slice(g_curve.as_bytes());
    transcript.extend_from_slice(h_curve_pt.as_bytes());
    transcript.extend_from_slice(p_curve.as_bytes());
    transcript.extend_from_slice(r1_pallas_pt.to_bytes().as_slice());
    transcript.extend_from_slice(r1_curve_pt.as_bytes());

    let computed = compute_fiat_shamir_challenge(&transcript);
    if computed.to_repr() != *challenge {
        return Err(BridgeError::InvalidDeposit(
            "Monero DLEq challenge mismatch — proof is invalid".into(),
        ).into());
    }

    Ok(())
}

/// Convert `[u8; 32]` to a Pallas scalar, returning a descriptive error
/// if the bytes are not a valid field element (>= the modulus).
fn to_pallas_scalar(
    bytes: &[u8; 32],
    name: &str,
) -> Result<pallas::Scalar, BridgeError> {
    Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(*bytes))
        .ok_or_else(|| BridgeError::InvalidDeposit(
            format!("Monero DLEq {} is not a valid Pallas scalar", name).into(),
        ))
}

/// Compute a Fiat-Shamir challenge from a transcript using Poseidon.
///
/// Chunks the transcript into 32-byte segments, converts each to a
/// `pallas::Base` field element, and folds with Poseidon.
fn compute_fiat_shamir_challenge(transcript: &[u8]) -> pallas::Base {
    let mut elements = Vec::<pallas::Base>::new();

    for chunk in transcript.chunks(32) {
        let mut padded = [0u8; 32];
        let len = chunk.len().min(32);
        padded[..len].copy_from_slice(&chunk[..len]);
        if let Some(elem) = Option::<pallas::Base>::from(
            pallas::Base::from_repr(padded)
        ) {
            elements.push(elem);
        }
    }

    if elements.is_empty() {
        return pallas::Base::ZERO;
    }

    // poseidon_hash takes [pallas::Base; N] where N is compile-time
    // constant. For variable-length transcript, fold iteratively.
    let mut acc = elements[0];
    for elem in &elements[1..] {
        acc = poseidon_hash([acc, *elem]);
    }
    acc
}
