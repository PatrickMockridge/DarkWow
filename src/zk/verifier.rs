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
///
/// HAZOP M-4 fix: insertion-order Vec provides FIFO eviction — the oldest
/// entries are evicted first, which approximates LRU for a read-heavy cache
/// without adding a dependency on an LRU crate.
///
/// Capped at `VK_CACHE_MAX_ENTRIES` to prevent memory exhaustion.
/// Eviction: when full, the oldest half (by insertion order) is removed.
struct VkCache {
    /// Lookup: zkbin bytes → VerifyingKey
    map: HashMap<Vec<u8>, VerifyingKey>,
    /// Insertion order (FIFO): oldest entries are at the front
    order: Vec<Vec<u8>>,
}

static VK_CACHE: Mutex<Option<VkCache>> = Mutex::new(None);

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
        #[expect(clippy::unwrap_used, reason = "mutex is never poisoned")]
        let cache = VK_CACHE.lock().unwrap();
        if let Some(ref vk_cache) = *cache {
            if let Some(vk) = vk_cache.map.get(zkbin_bytes) {
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

    // 3. Store in cache (with FIFO eviction cap) and verify
    {
        #[expect(clippy::unwrap_used, reason = "mutex is never poisoned")]
        let mut cache = VK_CACHE.lock().unwrap();
        let vk_cache = cache.get_or_insert_with(|| VkCache {
            map: HashMap::new(),
            order: Vec::new(),
        });
        // HAZOP M-4 fix: FIFO eviction — remove oldest half by insertion order.
        // Previously used HashMap::iter().take() which is insertion order on
        // the default hasher but non-deterministic and non-LRU under churn.
        if vk_cache.map.len() >= VK_CACHE_MAX_ENTRIES {
            let evict_count = vk_cache.order.len() / 2;
            for _ in 0..evict_count {
                if let Some(old_key) = vk_cache.order.first().cloned() {
                    vk_cache.map.remove(&old_key);
                    vk_cache.order.remove(0);
                }
            }
        }
        let key = zkbin_bytes.to_vec();
        vk_cache.map.insert(key.clone(), vk.clone());
        vk_cache.order.push(key);
    }

    match proof.verify(&vk, instances) {
        Ok(()) => ZkVerifyResult::Ok,
        Err(_) => ZkVerifyResult::InvalidProof,
    }
}