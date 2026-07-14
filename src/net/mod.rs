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

#[cfg(test)]
mod tests;

/// Wire protocol message types (VersionMessage, VerackMessage, PingMessage,
/// etc.). Part of `net-wire` — wallet needs this for the binary handshake.
pub mod message;
pub use message::Message;

/// Metering configuration and rate-limiting. Part of `net-wire` — needed by
/// `message` for MeteringConfiguration in the Message trait.
pub mod metering;

// ── net-full modules below — daemon P2P stack, NOT compiled for wallet ──

/// Generic publish/subscribe message dispatcher.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod message_publisher;
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub use message_publisher::MessageSubscription;

/// Network transports (TCP, TLS, Tor, SOCKS5, QUIC, Unix).
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod transport;

/// Port mapping protocols (UPnP, NAT-PMP, PCP).
#[cfg(feature = "upnp-igd")]
pub mod upnp;

/// Hostlist — peer addresses, coloring, persistence.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod hosts;

/// Async channel — framed message send/recv with magic bytes.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod channel;
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub use channel::ChannelPtr;

/// Compile-time barb declaration for protocol handlers (§1.1, §10.4).
pub mod barb_trait;

/// Typed cross-path channel with barb-carrying type safety (§10.4).
pub mod bridge_channel;

/// P2P orchestrator — all six sessions, channel store, broadcast.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod p2p;
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub use p2p::{P2p, P2pPtr};

/// Protocol handlers (version, ping, address, seed).
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod protocol;
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub use protocol::{
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};

/// Session management (inbound, outbound, manual, seedsync, direct, refine).
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod session;

/// Inbound connection acceptor.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod acceptor;

/// Outbound connection dialer.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod connector;

/// Network settings — profiles, timeouts, magic bytes.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub mod settings;
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
pub use settings::{BanPolicy, Settings};

/// Debug-notify event subsystem.
#[cfg(any(feature = "net-full", feature = "net-wallet"))]
#[macro_use]
pub mod dnet;
