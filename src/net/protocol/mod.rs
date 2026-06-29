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

use super::{
    p2p::P2pPtr,
    session::SESSION_DEFAULT,
};
#[cfg(any(feature = "session-seed", feature = "protocol-seed"))]
use super::session::SESSION_SEED;

/// Manages the tasks for the network protocol.
///
/// Used by other connection protocols to handle asynchronous task execution
/// across the network. Runs all tasks that are handed to it on an executor
/// that has stopping functionality.
pub mod protocol_jobs_manager;

/// Protocol for version information handshake between nodes at the start
/// of a connection. This is the first step when establishing a p2p conn.
///
/// The version protocol starts by instantiating the protocol and creating
/// a new subscription to version and version acknowledgement messages.
/// Then we run the protocol. Nodes send a version message and wait for a
/// version acknowledgement, while asynchronously waiting for version info
/// from the other node and sending the version acknowledgement.
pub mod protocol_version;
pub use protocol_version::ProtocolVersion;

/// Protocol for ping-pong keepalive messages.
///
/// Implements ping message and pong response. These messages are like the
/// network heartbeat - they are sent continually between nodes, to ensure
/// each node is still alive and active. Ping-pong messages ensure that the
/// network doesn't time out.
pub mod protocol_ping;
pub use protocol_ping::ProtocolPing;

/// Protocol for address and get-address messages.
///
/// Implements how nodes exchange connection information about other nodes
/// on the network. Address and get-address messages are exchanged continually
/// alongside ping-pong messages as part of a network connection.
///
/// Protocol starts by creating a subscription to address and get-address
/// messages. Then the protocol sends out a get-address message and waits
/// for an address message. Upon receiving address messages, nodes validate
/// and add the address information to their local store.
pub mod protocol_address;
pub use protocol_address::ProtocolAddress;

/// Seed server protocol. Seed server is used when connecting to the network
/// for the first time. Returns a list of peers that nodes can connect to.
///
/// Gated behind `protocol-seed` feature. net-wallet and net-node use
/// ProtocolAddress for all address exchange instead.
#[cfg(feature = "protocol-seed")]
pub mod protocol_seed;
#[cfg(feature = "protocol-seed")]
pub use protocol_seed::ProtocolSeed;

/// NAT Hole Punching Protocol (QUIC-only)
///
/// This protocol enables direct connections between peers behind NATs by
/// using existing connected peers as signaling relays. It is restricted to
/// QUIC because QUIC's single-socket design allows the dialer and listener
/// to share the same port, which is essential for NAT traversal.
pub mod protocol_holepunch;
pub use protocol_holepunch::ProtocolHolepunch;

/// Generic protocol to receive specified structure messages.
///
/// Acts as a simple message queue, where we listen for the specified
/// structure message, and when one is received, we send it to the provided
/// smol channel. Afterwards, we wait for an action signal, specifying whether
/// or not we should propagate the message to rest nodes or skip it.
pub mod protocol_generic;

/// Base trait for implementing P2P protocols
pub mod protocol_base;
/// Interface for registering arbitrary P2P protocols
pub mod protocol_registry;

/// Register the default network protocols for a p2p instance.
pub async fn register_default_protocols(p2p: P2pPtr) {
    let registry = p2p.protocol_registry();
    #[cfg(feature = "session-seed")]
    registry.register(SESSION_DEFAULT | SESSION_SEED, ProtocolPing::init).await;
    #[cfg(not(feature = "session-seed"))]
    registry.register(SESSION_DEFAULT, ProtocolPing::init).await;
    registry.register(SESSION_DEFAULT, ProtocolAddress::init).await;
    #[cfg(feature = "protocol-seed")]
    registry.register(SESSION_SEED, ProtocolSeed::init).await;
}
