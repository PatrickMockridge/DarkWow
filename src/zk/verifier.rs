/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Pure ZK proof verification module.
//!
//! Design principles:
//! - Stateless: no sled, no WASM, no side effects
//! - Deterministic: same inputs always produce same output
//! - Separated: independent from sync, consensus, and block production

use std::collections::HashMap;
use std::sync::Mutex;

use dwow_sdk::pasta::pallas;

use crate::{zk::ZkCircuit, zk::empty_witnesses, zk::Proof, zk::VerifyingKey, zkas::ZkBinary};

/// ZK Verification result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZkVerifyResult {
    /// Proof is valid
    Ok,
    /// Proof verification failed
    InvalidProof,
    /// Could not derive VK from circuit bytes
    InvalidVk,
}

/// Process-global cache of VerifyingKeys, keyed by the ZKAS binary bytes.
///
/// VK derivation (`keygen_vk`) is O(k * 2^k) — several hundred milliseconds
/// for k=14+. The VK is purely deterministic (same circuit bytes → same VK),
/// so caching eliminates this cost on every proof verification after the first.
/// Most nodes see O(10) unique circuits, so a Vec-keyed HashMap is fine.
///
/// Capped at `VK_CACHE_MAX_ENTRIES` to prevent memory exhaustion from
/// attackers submitting transactions with unique circuit binaries.
/// Eviction: when full, the oldest half of entries are removed.
static VK_CACHE: Mutex<Option<HashMap<Vec<u8>, VerifyingKey>>> = Mutex::new(None);

/// Maximum number of unique circuits to cache before eviction.
/// 256 entries × ~few KB per VK = a few MB max — sufficient for
/// all genesis contracts plus post-genesis deployments.
const VK_CACHE_MAX_ENTRIES: usize = 256;

/// Verify a ZK proof given the circuit bytes and public instances.
///
/// This is a pure function - same inputs always produce same output.
/// No sled, no WASM, no side effects.
///
/// VK derivation is cached: the first call for a given circuit pays the full
/// `keygen_vk` cost; subsequent calls hit the cache (~0.1ms vs ~200ms).
pub fn verify_zkp(
    proof: &Proof,
    zkbin_bytes: &[u8],
    instances: &[pallas::Base],
) -> ZkVerifyResult {
    // 1. Check VK cache
    {
        let cache = VK_CACHE.lock().unwrap();
        if let Some(ref map) = *cache {
            if let Some(vk) = map.get(zkbin_bytes) {
                return match proof.verify(vk, instances) {
                    Ok(()) => ZkVerifyResult::Ok,
                    Err(_) => ZkVerifyResult::InvalidProof,
                };
            }
        }
    }

    // 2. Cache miss — decode circuit and derive VK
    let Ok(zkbin) = ZkBinary::decode(zkbin_bytes, false) else {
        return ZkVerifyResult::InvalidVk
    };

    let witnesses = match empty_witnesses(&zkbin) {
        Ok(w) => w,
        Err(_) => return ZkVerifyResult::InvalidVk,
    };
    let circuit = ZkCircuit::new(witnesses, &zkbin);

    let vk = match VerifyingKey::build(zkbin.k, &circuit) {
        Ok(vk) => vk,
        Err(_) => return ZkVerifyResult::InvalidVk,
    };

    // 3. Store in cache (with eviction cap) and verify
    {
        let mut cache = VK_CACHE.lock().unwrap();
        let map = cache.get_or_insert_with(HashMap::new);
        // Evict half the entries if the cache is full to bound memory usage.
        // A simple half-eviction avoids depending on an LRU crate; if a more
        // sophisticated policy is desired later, this is the insertion point.
        if map.len() >= VK_CACHE_MAX_ENTRIES {
            let keys_to_remove: Vec<Vec<u8>> =
                map.iter().take(map.len() / 2).map(|(k, _)| k.clone()).collect();
            for k in keys_to_remove {
                map.remove(&k);
            }
        }
        map.insert(zkbin_bytes.to_vec(), vk.clone());
    }

    match proof.verify(&vk, instances) {
        Ok(()) => ZkVerifyResult::Ok,
        Err(_) => ZkVerifyResult::InvalidProof,
    }
}