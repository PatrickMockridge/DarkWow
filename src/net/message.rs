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

use std::net::Ipv6Addr;

use dwow_serial::{
    serialize_async, async_trait, AsyncDecodable, AsyncEncodable, SerialDecodable, SerialEncodable,
};
use url::{Host, Url};

use crate::{net::metering::MeteringConfiguration, util::time::NanoTimestamp};

// ═══════════════════════════════════════════════════════════════════════
// BoundaryCodec — DarkWow-native absorber boundary (type-system.md §10.5)
// ═══════════════════════════════════════════════════════════════════════
//
// Replaces upstream AsyncEncodable/AsyncDecodable for message serialization.
// Delegates to Encodable/Decodable for wire format (byte-identical). Adds
// quote/eval semantics and per-type defense constants.
//
// Phase 1 (this commit): Trait definition + pilot on PingMessage.
// Phase 2: All P2P message types. Phase 3: Message trait drops async bounds.

/// A type that crosses the P2P wire boundary via the ρ-calculus
/// quote/eval pattern (§10.5). Thin semantic layer over Encodable/Decodable
/// — same wire format, with boundary semantics and defense constants attached.
pub trait BoundaryCodec: dwow_serial::Encodable + dwow_serial::Decodable + Sized {
    /// §10.5 quote: typed value → bytes. Erases barbs — output has no
    /// behavioral constraints (§2.2). Default: encode to Vec<u8>.
    fn quote(&self) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode(&mut buf)?;
        Ok(buf)
    }

    /// §10.5 eval: bytes → typed value via validating constructor.
    /// SHALL reject invalid bytes. Default: deserialize from slice.
    fn eval(bytes: &[u8]) -> std::io::Result<Self> {
        dwow_serial::deserialize(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Maximum wire size in bytes (§8.6.2). Zero only when METERING_SCORE > 0.
    const MAX_BYTES: u64;

    /// Metering contribution for rate limiting (§8.6.1).
    const METERING_SCORE: u64;

    /// Barb set carried across this boundary (§10.5). Empty by default
    /// for types not yet audited.
    const BARBS: &'static [crate::barb::BarbId] = &[];
}

/// Shorthand for implementing BoundaryCodec on P2P message types.
/// Types must already derive SerialEncodable/SerialDecodable.
macro_rules! impl_boundary_codec {
    ($ty:ty, $max_bytes:expr, $metering_score:expr) => {
        impl_boundary_codec!($ty, $max_bytes, $metering_score, &[]);
    };
    ($ty:ty, $max_bytes:expr, $metering_score:expr, $barbs:expr) => {
        impl $crate::net::message::BoundaryCodec for $ty {
            const MAX_BYTES: u64 = $max_bytes;
            const METERING_SCORE: u64 = $metering_score;
            const BARBS: &'static [$crate::barb::BarbId] = $barbs;
        }
    };
}
pub(crate) use impl_boundary_codec;

/// Generic message template.
/// Phase 2: AsyncDecodable + AsyncEncodable bound shifted to net-full only.
pub trait Message: 'static + Send + Sync + AsyncDecodable + AsyncEncodable {
    const NAME: &'static str;
    /// Message bytes vector length limit.
    /// Set to 0 for no limit.
    const MAX_BYTES: u64;
    /// Message metering score value.
    /// Set to 0 for no impact in metering.
    const METERING_SCORE: u64;
    /// Message metering configuration for rate limit.
    /// Use `MeteringConfiguration::default()` for no limit.
    const METERING_CONFIGURATION: MeteringConfiguration;
    /// Barb set carried by this message (type-system.md §10.5).
    /// Each message type that crosses the wire boundary SHALL declare the
    /// barbs its handler exhibits — this is the per-channel declared set
    /// that makes the absorber measurable. Empty by default for messages
    /// that have not yet been audited; a cardinality snapshot test gates
    /// production deploy profiles on audit completeness.
    const BARBS: &'static [crate::barb::BarbId] = &[];
}

/// Generic serialized message template.
pub struct SerializedMessage {
    pub command: String,
    pub payload: Vec<u8>,
}

impl SerializedMessage {
    pub async fn new<M: Message>(message: &M) -> Self {
        Self { command: M::NAME.to_string(), payload: serialize_async(message).await }
    }
}

#[macro_export]
macro_rules! impl_p2p_message {
    // Classic 5-arg form (backward-compatible; BARBS defaults to &[]).
    ($st:ty, $nm:expr, $mb:expr, $ms:expr, $mc:expr) => {
        $crate::impl_p2p_message!($st, $nm, $mb, $ms, $mc, &[]);
    };
    // 6-arg form with explicit barb set.
    // $crate resolves to the defining crate (dwow_core), so the path
    // compiles correctly even when the macro is invoked from another
    // crate (e.g. dwowd's proto modules).
    ($st:ty, $nm:expr, $mb:expr, $ms:expr, $mc:expr, $barbs:expr) => {
        impl Message for $st {
            const NAME: &'static str = $nm;
            const MAX_BYTES: u64 = $mb;
            const METERING_SCORE: u64 = $ms;
            const METERING_CONFIGURATION: MeteringConfiguration = $mc;
            const BARBS: &'static [$crate::barb::BarbId] = $barbs;
        }
    };
}

/// Maximum command (message name) length in bytes.
pub const MAX_COMMAND_LENGTH: u8 = 255;

/// For each message configs a threshold was calculated by taking the
/// maximum number of messages in a 10 seconds window and multiply it
/// by 2 not to be strict.
pub const PING_PONG_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 4,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Ping-Pong messages fields size:
/// * nonce = 2
pub const PING_PONG_MAX_BYTES: u64 = 2;

/// Outbound keepalive message.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct PingMessage {
    pub nonce: u16,
}
impl_p2p_message!(PingMessage, "ping", PING_PONG_MAX_BYTES, 1, PING_PONG_METERING_CONFIGURATION);
// Phase D.1 pilot: BoundaryCodec for PingMessage — same constants as Message.
impl_boundary_codec!(PingMessage, PING_PONG_MAX_BYTES, 1);

/// Inbound keepalive message.
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct PongMessage {
    pub nonce: u16,
}
impl_p2p_message!(PongMessage, "pong", PING_PONG_MAX_BYTES, 1, PING_PONG_METERING_CONFIGURATION);
// Phase D.1 pilot: BoundaryCodec for PongMessage.
impl_boundary_codec!(PongMessage, PING_PONG_MAX_BYTES, 1);

/// Requests address of outbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GetAddrsMessage {
    /// Maximum number of addresses with preferred
    /// transports to receive. Response vector will
    /// also contain addresses without the preferred
    /// transports, so its size will be 2 * max.
    pub max: u32,
    /// Preferred addresses transports.
    pub transports: Vec<String>,
}
pub const GET_ADDRS_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 6,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// GetAddrs message fields size:
/// * max = 4
/// * transports = 1 (vec_len) + 4 + 4 + 4 + 4 + 4 + 8 + 8 + 8 + 8 = 53
///
/// Transports is list of all transports to be shared specified in protocol_address.
pub const GET_ADDRS_MAX_BYTES: u64 = 57;

impl_p2p_message!(
    GetAddrsMessage,
    "getaddr",
    GET_ADDRS_MAX_BYTES,
    1,
    GET_ADDRS_METERING_CONFIGURATION
);
impl_boundary_codec!(GetAddrsMessage, GET_ADDRS_MAX_BYTES, 1);

/// Sends address information to inbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddrsMessage {
    pub addrs: Vec<(Url, u64)>,
}
pub const ADDRS_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 6,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Addrs message fields size:
/// * addrs = 1 (vec_len) + (u8::MAX * 2) * 128
///
/// Url type is estimated to be max 128 bytes here and for other message below.
pub const ADDRS_MAX_BYTES: u64 = 65281;

impl_p2p_message!(AddrsMessage, "addr", ADDRS_MAX_BYTES, 1, ADDRS_METERING_CONFIGURATION);
impl_boundary_codec!(AddrsMessage, ADDRS_MAX_BYTES, 1);

/// Requests version information of outbound connection.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VersionMessage {
    /// Only used for debugging. Compromises privacy when set.
    pub node_id: String,
    /// App identifier
    pub app_name: String,
    /// Identifies protocol version being used by the node.
    pub version: semver::Version,
    /// UNIX timestamp of when the VersionMessage was created.
    pub timestamp: u64,
    /// Network address of the node receiving this message (before
    /// resolving).
    pub connect_recv_addr: Url,
    /// Network address of the node receiving this message (after
    /// resolving). Optional because only used by outbound connections.
    pub resolve_recv_addr: Option<Url>,
    /// External address of the sender node, if it exists (empty
    /// otherwise).
    pub ext_send_addr: Vec<Url>,
    /// List of features consisting of a tuple of (services, version)
    /// to be enabled for this connection.
    pub features: Vec<(String, u32)>,
}
pub const VERSION_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 4,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Version message fields size:
/// * node_id = 8  (this will be empty most of the time)
/// * version = 128 (look at VerackMessage for the reasoning)
/// * timestamp = 8
/// * connect_recv_addr = 128
/// * resolve_recv_addr = 1 (enum_len) + 128(url) = 129
/// * ext_send_addr = 1 (vec_len)  + 128 * 10 = 1281 (10 is a reasonable cap for number of external addresses)
/// * features = 1 (vec_len) + (32 (service_name) + 4 (service_version)) * 10 = 361 (10 features is an estimate)
pub const VERSION_MAX_BYTES: u64 = 2043;

impl_p2p_message!(VersionMessage, "version", VERSION_MAX_BYTES, 1, VERSION_METERING_CONFIGURATION);
impl_boundary_codec!(VersionMessage, VERSION_MAX_BYTES, 1);

impl VersionMessage {
    pub(in crate::net) fn get_ipv6_addr(&self) -> Option<Ipv6Addr> {
        let host = self.connect_recv_addr.host()?;
        // Check the reported address is Ipv6
        match host {
            Host::Ipv6(addr) => Some(addr),
            _ => None,
        }
    }
}

/// Sends version information to inbound connection.
/// Response to `VersionMessage`.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerackMessage {
    /// App version
    pub app_version: semver::Version,
    /// App identifier
    pub app_name: String,
}
pub const VERACK_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 4,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

/// Verack message fields size:
/// * app_version = 24 (major = 8, minor = 8, patch = 8) + 52 (prerelease =  1(str_len) + 51(str)) + 52 (build = 1(str_len) + 51(str))
///
/// Prerelease and build strings are variable length but shouldn't be larger than 102 bytes.
pub const VERACK_MAX_BYTES: u64 = 128;

impl_p2p_message!(VerackMessage, "verack", VERACK_MAX_BYTES, 1, VERACK_METERING_CONFIGURATION);
impl_boundary_codec!(VerackMessage, VERACK_MAX_BYTES, 1);

/// Maximum number of error responses per connection to prevent DoS
/// amplification. After this limit is reached, further errors are
/// silently dropped — the peer is expected to disconnect.
pub const MAX_SEED_ERRORS_PER_CONNECTION: u64 = 3;

// ============================================================================
// Seed Error Codes — HTTP-style categorization for P2P seed responses
// ============================================================================
//
// 4xx — Client Error: the requester did something wrong.
//       Do NOT retry without changing the request.
// 5xx — Server Error: the request is valid but the seed cannot fulfill.
//       MAY retry with backoff.
// 2xx — Success: NOT sent as SeedErrorMessage (implicit in success messages)

/// §4.3: Seed error vocabulary SHALL be a #[repr(u32)] enum, not raw u32
/// constants. Every error variant IS a barb (§4). Wire-identical encoding
/// to the prior raw u32 constants.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedErrorCode {
    BadRequest = 400,
    VersionMismatch = 401,
    Forbidden = 403,
    UnknownMessage = 404,
    NoMatchingTransports = 406,
    RateLimited = 429,
    Internal = 500,
    HostlistEmpty = 503,
    UpstreamTimeout = 504,
}

impl SeedErrorCode {
    /// Returns true for 4xx client errors — do NOT retry without changing request.
    pub fn is_client_error(self) -> bool {
        matches!(self, Self::BadRequest | Self::VersionMismatch | Self::Forbidden
            | Self::UnknownMessage | Self::NoMatchingTransports | Self::RateLimited)
    }

    /// Returns true for 5xx server errors — MAY retry with backoff.
    pub fn is_server_error(self) -> bool {
        matches!(self, Self::Internal | Self::HostlistEmpty | Self::UpstreamTimeout)
    }
}

impl std::fmt::Display for SeedErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", *self as u32)
    }
}

// Wire-identical: encode/decode as u32. Delegates to u32's Encodable/Decodable.
impl dwow_serial::Encodable for SeedErrorCode {
    fn encode<W: std::io::Write>(&self, e: &mut W) -> Result<usize, std::io::Error> {
        (*self as u32).encode(e)
    }
}
impl dwow_serial::Decodable for SeedErrorCode {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self, std::io::Error> {
        let code = u32::decode(d)?;
        Ok(match code {
            400 => Self::BadRequest,
            401 => Self::VersionMismatch,
            403 => Self::Forbidden,
            404 => Self::UnknownMessage,
            406 => Self::NoMatchingTransports,
            429 => Self::RateLimited,
            500 => Self::Internal,
            503 => Self::HostlistEmpty,
            504 => Self::UpstreamTimeout,
            n => return Err(std::io::Error::other(
                format!("unknown SeedErrorCode: {}", n),
            )),
        })
    }
}

// Async bridge — delegates to u32's async impls. Minimal upstream compatibility
// shim. Per Phase D plan: removed when BoundaryCodec replaces AsyncEncodable/
// AsyncDecodable on the Message trait (Phase D.3).
#[async_trait]
impl dwow_serial::AsyncEncodable for SeedErrorCode {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(
        &self, w: &mut W,
    ) -> std::io::Result<usize> {
        (*self as u32).encode_async(w).await
    }
}
#[async_trait]
impl dwow_serial::AsyncDecodable for SeedErrorCode {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(
        d: &mut D,
    ) -> std::io::Result<Self> {
        let code = u32::decode_async(d).await?;
        Ok(match code {
            400 => Self::BadRequest,
            401 => Self::VersionMismatch,
            403 => Self::Forbidden,
            404 => Self::UnknownMessage,
            406 => Self::NoMatchingTransports,
            429 => Self::RateLimited,
            500 => Self::Internal,
            503 => Self::HostlistEmpty,
            504 => Self::UpstreamTimeout,
            n => return Err(std::io::Error::other(
                format!("unknown SeedErrorCode: {}", n),
            )),
        })
    }
}

/// Error response sent by seed nodes when a request cannot be fulfilled.
/// Carries an HTTP-style numeric error code (4xx = client error, 5xx = server
/// error) and a human-readable reason string. Wire name: "seederr"
///
/// Metering: each connection may send at most [`MAX_SEED_ERRORS_PER_CONNECTION`]
/// error responses. Beyond that limit, errors are dropped silently to prevent
/// DoS amplification (cf. Bitcoin Core PR #15437 removing `reject` for the
/// same reason).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SeedErrorMessage {
    /// HTTP-style numeric error code (§4.3 SeedErrorCode)
    pub code: SeedErrorCode,
    /// Human-readable reason string
    pub reason: String,
}

/// SeedError message fields size:
/// * code = 4
/// * reason = 1 (str_len) + 255 (str_content) = 256
pub const SEED_ERROR_MAX_BYTES: u64 = 260;

pub const SEED_ERROR_METERING_CONFIGURATION: MeteringConfiguration = MeteringConfiguration {
    threshold: 3,
    sleep_step: 1000,
    expiry_time: NanoTimestamp::from_secs(10),
};

impl_p2p_message!(
    SeedErrorMessage,
    "seederr",
    SEED_ERROR_MAX_BYTES,
    1,
    SEED_ERROR_METERING_CONFIGURATION
);
impl_boundary_codec!(SeedErrorMessage, SEED_ERROR_MAX_BYTES, 1);
