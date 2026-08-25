/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Blockchain Bridge — route blockchain events through the event graph DAG.
//!
//! Wraps blockchain messages (blocks, transactions) in event graph event
//! content for structured gossip dissemination through the DAG sync protocol.
//! Replaces flood broadcast (O(N²)) with DAG-based relay (O(k·N)).
//!
//! The quarantine boundary is maintained: blockchain event content is routed
//! through the event graph's DAG structure, but the event graph sled tree
//! (`dag`) is distinct from blockchain sled trees (`contracts`, `blocks`,
//! `commitment_set`, `nullifiers`). No blockchain capability semantics leak across.
//!
//! Maps to ρ-calculus bridging (§10.4):
//! ```
//! BlockchainEvent =
//!   νblock_data.(
//!     serialize!(block, block_data)
//!     | wrap!(BLOCKCHAIN_EVENT_MARKER, block_data, event_content)
//!     | dag_insert!(event)
//!   )
//! ```
//!
//! See: doc/src/arch/type-system.md §10.4 (Bridging)

/// Marker byte prepended to event content to identify blockchain events.
/// Events with this marker are routed through the event graph DAG but
/// contain blockchain data that is deserialized at the receive side.
pub const BLOCKCHAIN_EVENT_MARKER: u8 = 0x42; // 'B' for blockchain

/// Wraps blockchain data in event graph event content.
///
/// Prepends the blockchain marker byte so event graph protocol handlers
/// can filter and route blockchain events without deserializing the
/// payload. The marker is followed by the serialized blockchain message.
pub fn wrap_blockchain_event(data: &[u8]) -> Vec<u8> {
    let mut content = Vec::with_capacity(1 + data.len());
    content.push(BLOCKCHAIN_EVENT_MARKER);
    content.extend_from_slice(data);
    content
}

/// Tests whether an event contains blockchain data.
///
/// Returns true if the event content starts with the blockchain marker byte.
/// This check is fast (single byte comparison) and does not deserialize.
pub fn is_blockchain_event(event_content: &[u8]) -> bool {
    event_content.first() == Some(&BLOCKCHAIN_EVENT_MARKER)
}

/// Extracts blockchain data from event content.
///
/// Returns the payload bytes after the marker byte, or None if the
/// content is not a blockchain event.
pub fn unwrap_blockchain_event(event_content: &[u8]) -> Option<&[u8]> {
    if is_blockchain_event(event_content) {
        Some(&event_content[1..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_and_unwrap_roundtrip() {
        let data = b"blockchain message payload";
        let wrapped = wrap_blockchain_event(data);
        assert!(is_blockchain_event(&wrapped));
        assert_eq!(unwrap_blockchain_event(&wrapped), Some(&data[..]));
    }

    #[test]
    fn test_marker_byte() {
        assert_eq!(BLOCKCHAIN_EVENT_MARKER, 0x42);
    }

    #[test]
    fn test_non_blockchain_event() {
        let content = vec![0x00, 1, 2, 3]; // no marker
        assert!(!is_blockchain_event(&content));
        assert_eq!(unwrap_blockchain_event(&content), None);
    }

    #[test]
    fn test_empty_content() {
        let content: Vec<u8> = vec![];
        assert!(!is_blockchain_event(&content));
        assert_eq!(unwrap_blockchain_event(&content), None);
    }
}
