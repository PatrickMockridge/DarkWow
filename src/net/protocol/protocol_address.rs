/* This file is part of DarkFi (https://dark.fi)
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

use async_trait::async_trait;
use smol::{lock::RwLock as AsyncRwLock, Executor};
use std::{sync::Arc, time::UNIX_EPOCH};
use tracing::debug;
use url::Url;

use super::{
    super::{
        channel::ChannelPtr,
        hosts::{HostColor, HostsPtr},
        message::{AddrsMessage, GetAddrsMessage},
        message_publisher::MessageSubscription,
        p2p::P2pPtr,
        session::{SESSION_MANUAL, SESSION_OUTBOUND},
        settings::Settings,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
    protocol_jobs_manager::{ProtocolJobsManager, ProtocolJobsManagerPtr},
};
use crate::Result;

/// Defines address and get-address messages.
///
/// On receiving GetAddr, nodes reply an AddrMessage containing nodes from
/// their hostlist.  On receiving an AddrMessage, nodes enter the info into
/// their greylists.
///
/// The node selection logic for creating an AddrMessage is as follows:
///
/// 1. First select nodes matching the requested transports from the
///    anchorlist. These nodes have the highest guarantee of being reachable,
///    so we prioritize them first.
///
/// 2. Then select nodes matching the requested transports from the
///    whitelist.
///
/// 3. Next select whitelist nodes that don't match our transports. We do
///    this so that nodes share and propagate nodes of different transports,
///    even if they can't connect to them themselves.
///
/// 4. Finally, if there's still space available, fill the remaining vector
///    space with darklist entries. This is necessary to propagate transports
///    that neither this node nor the receiving node support.
pub struct ProtocolAddress {
    channel: ChannelPtr,
    addrs_sub: MessageSubscription<AddrsMessage>,
    get_addrs_sub: MessageSubscription<GetAddrsMessage>,
    hosts: HostsPtr,
    settings: Arc<AsyncRwLock<Settings>>,
    jobsman: ProtocolJobsManagerPtr,
}

const PROTO_NAME: &str = "ProtocolAddress";

/// A vector of all currently accepted transports and valid transport
/// combinations.  Should be updated if and when new transports are
/// added. Creates a upper bound on the number of transports a given peer
/// can request.
const TRANSPORT_COMBOS: [&str; 9] =
    ["tor", "tls", "tcp", "nym", "i2p", "tor+tls", "nym+tls", "tcp+tls", "i2p+tls"];

/// Strip query parameters from a URL before broadcasting.
///
/// This prevents leaking internal tracking identifiers (e.g., UPnP cookies)
/// that could be used for fingerprinting nodes on the P2P network.
fn strip_query_params(url: &Url) -> Url {
    let mut stripped = url.clone();
    stripped.set_query(None);
    stripped
}

impl ProtocolAddress {
    /// Creates a new address protocol. Makes an address, an external address
    /// and a get-address subscription and adds them to the address protocol
    /// instance.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        // Creates a subscription to address message
        #[expect(clippy::expect_used, reason = "message dispatcher subscription is always registered")]
        let addrs_sub =
            channel.subscribe_msg::<AddrsMessage>().await.expect("Missing addrs dispatcher!");

        // Creates a subscription to get-address message
        #[expect(clippy::expect_used, reason = "message dispatcher subscription is always registered")]
        let get_addrs_sub =
            channel.subscribe_msg::<GetAddrsMessage>().await.expect("Missing getaddrs dispatcher!");

        Arc::new(Self {
            channel: channel.clone(),
            addrs_sub,
            get_addrs_sub,
            hosts: p2p.hosts(),
            jobsman: ProtocolJobsManager::new(PROTO_NAME, channel),
            settings: p2p.settings(),
        })
    }

    /// Handles receiving the address message. Loops to continually receive
    /// address messages on the address subscription. Validates and adds the
    /// received addresses to the greylist.
    async fn handle_receive_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::handle_receive_addrs",
            "[START] address={}", self.channel.display_address(),
        );

        loop {
            let addrs_msg = self.addrs_sub.receive().await?;
            debug!(
                target: "net::protocol_address::handle_receive_addrs",
                "Received {} addrs from {}", addrs_msg.addrs.len(), self.channel.display_address(),
            );

            debug!(
                target: "net::protocol_address::handle_receive_addrs",
                "Appending to greylist...",
            );

            // Filter before inserting, to the size `ADDRS_MAX_BYTES` assumes per address.
            //
            // That bound is derived as `1 (vec_len) + (u8::MAX * 2) * 128` — i.e. it
            // assumes **128 bytes per address** — so an address longer than that silently
            // breaks the bound the reply is sized against. Nothing else checks it: this
            // node accepts whatever a peer sends, relays it in its own reply, the reply
            // exceeds `ADDRS_MAX_BYTES`, and under `BanPolicy::Strict` **every peer that
            // receives that reply rejects it and bans this node**. One small `addr`
            // frame, a persistent ban of an honest node, and the attacker is not the one
            // punished. Measured in the same encoding the bound is derived from.
            let usable: Vec<(Url, u64)> = addrs_msg
                .addrs
                .iter()
                .filter(|(addr, _)| dwow_serial::serialize(addr).len() <= MAX_ADDR_BYTES)
                .cloned()
                .collect();
            if usable.len() < addrs_msg.addrs.len() {
                debug!(
                    target: "net::protocol_address::handle_receive_addrs",
                    "discarded {} of {} received addresses: longer than the {} bytes \
                     `ADDRS_MAX_BYTES` assumes per address",
                    addrs_msg.addrs.len() - usable.len(),
                    addrs_msg.addrs.len(),
                    MAX_ADDR_BYTES
                );
            }
            self.hosts.insert(HostColor::Grey, &usable).await;
        }
    }

    /// Handles receiving the get-address message. Continually receives
    /// get-address messages on the get-address subscription. Then replies
    /// with an address message.
    async fn handle_receive_get_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::handle_receive_get_addrs",
            "[START] address={}", self.channel.display_address(),
        );

        loop {
            let get_addrs_msg = self.get_addrs_sub.receive().await?;

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs",
                "Received GetAddrs({}) message from {}", get_addrs_msg.max, self.channel.display_address(),
            );

            // Filter out transports not meant to be shared like Socks5 and Socks5+tls
            let requested_transports: Vec<String> = get_addrs_msg
                .transports
                .iter()
                .filter(|tp| TRANSPORT_COMBOS.contains(&tp.as_str()))
                .cloned()
                .collect();

            // First we grab address with the requested transports from the gold list
            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching gold entries with schemes");
            // Clamp `max` before it is used as a count, and give every fetch below one
            // shared budget.
            //
            // The field is a `u32`, but this protocol's own intent is `u8::MAX` — that is
            // what this tree's sender asks for (`:362`) — so clamping there cannot refuse
            // a legitimate request. And the response is documented as at most `2 * max`
            // addresses (`src/net/message.rs:217-219`), so the fetches share that budget
            // rather than each taking `max`.
            //
            // Without both, the first three fetches (gold/white/grey, with schemes) alone
            // reached `3 * max`, and the `2 * max - addrs.len()` subtractions that follow
            // **underflowed**. In release — `Cargo.toml` declares no `[profile]`, so
            // `overflow-checks = false` — they wrapped to ~`usize::MAX`; the remaining
            // fetches then drained the entire hostlist into the reply, the reply exceeded
            // `ADDRS_MAX_BYTES`, and **every peer that received it rejected it and banned
            // the victim**. One 57-byte request, a persistent ban of an honest node. Debug
            // builds panicked instead.
            let max = (get_addrs_msg.max as usize).min(u8::MAX as usize);
            let mut addrs = self.hosts.container.fetch_n_random_with_schemes(
                HostColor::Gold,
                &requested_transports,
                remaining_reply_budget(max, 0),
            );

            // Then we grab address with the requested transports from the whitelist
            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching whitelist entries with schemes");
            addrs.append(&mut self.hosts.container.fetch_n_random_with_schemes(
                HostColor::White,
                &requested_transports,
                remaining_reply_budget(max, addrs.len()),
            ));

            // Greylist (matching transports) — share recently-connected peers
            // immediately. With BanPolicy::Relaxed, nodes that completed the
            // version handshake are trusted enough to share.
            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching greylist entries with schemes");
            addrs.append(&mut self.hosts.container.fetch_n_random_with_schemes(
                HostColor::Grey,
                &requested_transports,
                remaining_reply_budget(max, addrs.len()),
            ));

            // Next we grab addresses without the requested transports
            // to fill a 2 * max length vector.

            // Then we grab address without the requested transports from the gold list
            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching gold entries without schemes");
            let remain = remaining_reply_budget(max, addrs.len());
            addrs.append(&mut self.hosts.container.fetch_n_random_excluding_schemes(
                HostColor::Gold,
                &requested_transports,
                remain,
            ));

            // Then we grab address without the requested transports from the white list
            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching white entries without schemes");
            let remain = remaining_reply_budget(max, addrs.len());
            addrs.append(&mut self.hosts.container.fetch_n_random_excluding_schemes(
                HostColor::White,
                &requested_transports,
                remain,
            ));

            // Greylist (excluding transports) — share for transport diversity
            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching greylist entries without schemes");
            let remain = remaining_reply_budget(max, addrs.len());
            addrs.append(&mut self.hosts.container.fetch_n_random_excluding_schemes(
                HostColor::Grey,
                &requested_transports,
                remain,
            ));

            // If there's still space available, take from the Dark list.

            /* NOTE: We share peers from our Dark list because to ensure
            that non-compatiable transports are shared with other nodes
            so that they propagate on the network even if they're not
            popular transports. */

            debug!(target: "net::protocol_address::handle_receive_get_addrs",
            "Fetching dark entries");
            let remain = remaining_reply_budget(max, addrs.len());
            addrs.append(&mut self.hosts.container.fetch_n_random(HostColor::Dark, remain));

            // Filter out transports not meant to be shared like Socks5 and Socks5+tls
            addrs.retain(|addr| TRANSPORT_COMBOS.contains(&addr.0.scheme()));

            debug!(
                target: "net::protocol_address::handle_receive_get_addrs",
                "Sending {} addresses to {}", addrs.len(), self.channel.display_address(),
            );

            let addrs_msg = AddrsMessage { addrs };
            self.channel.send(&addrs_msg).await?;
        }
    }

    /// Send our own external addresses over a channel. Set the
    /// last_seen field to now.
    async fn send_my_addrs(self: Arc<Self>) -> Result<()> {
        debug!(
            target: "net::protocol_address::send_my_addrs",
            "[START] channel address={}", self.channel.display_address(),
        );

        if self.channel.session_type_id() & (SESSION_OUTBOUND | SESSION_MANUAL) == 0 {
            debug!(
                target: "net::protocol_address::send_my_addrs",
                "Not an outbound session. Stopping",
            );
            return Ok(())
        }

        let external_addrs = self.channel.hosts().external_addrs().await;

        if external_addrs.is_empty() {
            debug!(
                target: "net::protocol_address::send_my_addrs",
                "External addr not configured. Stopping",
            );
            return Ok(())
        }

        let mut addrs = vec![];

        for addr in external_addrs {
            let stripped_addr = strip_query_params(&addr);
            #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
            addrs.push((stripped_addr, last_seen));
        }

        debug!(
            target: "net::protocol_address::send_my_addrs",
            "Broadcasting {} addresses", addrs.len(),
        );

        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;

        debug!(
            target: "net::protocol_address::send_my_addrs",
            "[END] channel address={}", self.channel.display_address(),
        );

        Ok(())
    }
}

#[async_trait]
impl ProtocolBase for ProtocolAddress {
    /// Start the address protocol. If it's an outbound session and has an
    /// external address, send our external address. Run receive address
    /// and get address protocols on the protocol task manager. Then send
    /// get-address msg.
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(
            target: "net::protocol_address::start",
            "START => address={}", self.channel.display_address(),
        );

        let settings = self.settings.read().await;
        let outbound_connections = settings.outbound_connections;
        let active_profiles = settings.active_profiles.clone();
        let getaddrs_max = settings.getaddrs_max;
        drop(settings);

        self.jobsman.clone().start(ex.clone());

        self.jobsman.clone().spawn(self.clone().send_my_addrs(), ex.clone()).await;

        self.jobsman.clone().spawn(self.clone().handle_receive_addrs(), ex.clone()).await;

        self.jobsman.spawn(self.clone().handle_receive_get_addrs(), ex).await;

        // Send get_address message.
        // We ask for a maximum of u8::MAX addresses from a single node
        let get_addrs = GetAddrsMessage {
            max: getaddrs_max.unwrap_or(outbound_connections.min(u32::MAX as usize) as u32),
            transports: active_profiles,
        };
        self.channel.send(&get_addrs).await?;

        debug!(
            target: "net::protocol_address::start",
            "END => address={}", self.channel.display_address(),
        );

        Ok(())
    }
    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}

/// The size `ADDRS_MAX_BYTES` assumes per address.
///
/// `src/net/message.rs` derives that bound as `1 (vec_len) + (u8::MAX * 2) * 128`, so this
/// is the tree's own figure rather than a new one. It is enforced when an address is
/// *received* (`handle_receive_addrs`), because an address longer than this makes the
/// node's own relayed reply exceed the bound it is sized against — and the peers that
/// receive that reply ban *this* node.
const MAX_ADDR_BYTES: usize = 128;

/// How many addresses a `GetAddrs` reply may still take, given what it already holds.
///
/// Extracted so the arithmetic can be witnessed directly, because the defect lived in it:
/// the handler used `2 * max - addrs.len()` unguarded, and the gold/white/grey fetches
/// come *before* it and can return up to `3 * max` between them — more than the `2 * max`
/// the response is documented to be (`src/net/message.rs:217-219`). When
/// `addrs.len() > 2 * max` the subtraction underflowed; in release — `Cargo.toml` declares
/// no `[profile]`, so `overflow-checks = false` — it wrapped to ~`usize::MAX`, the
/// remaining fetches drained the entire hostlist into the reply, the reply exceeded
/// `ADDRS_MAX_BYTES`, and every peer that received it rejected it and banned the victim.
/// Debug builds panicked instead.
fn remaining_reply_budget(max: usize, already: usize) -> usize {
    (2 * max).saturating_sub(already)
}

#[cfg(test)]
mod tests {
    use dwow_serial::serialize;

    use crate::net::message::GET_ADDRS_MAX_BYTES;

    use super::{GetAddrsMessage, TRANSPORT_COMBOS};

    // Helps to check if the MAX_BYTES for GetAddrs message is valid as new transports are added
    #[test]
    fn test_get_addrs_msg_size() {
        let message = GetAddrsMessage {
            max: u8::MAX as u32,
            transports: TRANSPORT_COMBOS.iter().map(|x| x.to_string()).collect(),
        };

        assert_eq!(serialize(&message).len() as u64, GET_ADDRS_MAX_BYTES);
    }

    /// The reply's remaining budget cannot underflow — the defect's mechanism.
    ///
    /// Before the fix the handler subtracted `addrs.len()` from `2 * max` unguarded, and
    /// the three "with schemes" fetches that run first can return up to `3 * max` between
    /// them. So `addrs.len() > 2 * max` was reachable with a single small request, the
    /// subtraction wrapped, and the rest of the reply drained the whole hostlist — which
    /// every receiving peer then rejected *and banned the sender for*. This asserts the
    /// two properties that matter: it never wraps, and it never exceeds the documented
    /// `2 * max` response.
    #[test]
    fn test_reply_budget_never_underflows_or_overruns() {
        for max in [0usize, 1, 2, 100, u8::MAX as usize] {
            assert_eq!(
                super::remaining_reply_budget(max, 0),
                2 * max,
                "an empty reply may take the whole documented budget"
            );
            assert_eq!(
                super::remaining_reply_budget(max, 2 * max),
                0,
                "an exact budget leaves nothing"
            );
            assert_eq!(
                super::remaining_reply_budget(max, 3 * max),
                0,
                "over budget must saturate at zero, never wrap — `3 * max` is reachable \
                 from the three with-schemes fetches alone"
            );
            assert_eq!(
                super::remaining_reply_budget(max, usize::MAX),
                0,
                "no input can make the budget wrap to a huge value"
            );
        }
    }

    /// The handshake derivations account for every field the messages carry.
    ///
    /// `app_name` was absent from both until 2026-09-25, and `node_id` was sized as if it
    /// were a `u64` although it is a `String`. So a configured application name longer than
    /// the missing allowance produced a `version`/`verack` frame **over** the bound — and
    /// under `BanPolicy::Strict` every peer that received it rejected the frame and banned
    /// the sender, over a purely local config value. A bound that does not account for a
    /// field it carries is not a bound.
    #[test]
    fn test_handshake_bounds_account_for_every_field() {
        use crate::net::message::{MAX_HANDSHAKE_STRING_LEN, VERACK_MAX_BYTES, VERSION_MAX_BYTES};
        let s = MAX_HANDSHAKE_STRING_LEN as u64;
        // VersionMessage: node_id + app_name + version + timestamp + connect_recv_addr
        //   + resolve_recv_addr + ext_send_addr (10) + features (10)
        assert_eq!(
            VERSION_MAX_BYTES,
            (1 + s) * 2 + 128 + 8 + 128 + (1 + 128) + (1 + 128 * 10) + (1 + 36 * 10),
            "every field the message carries must appear in the sum"
        );
        // VerackMessage: app_version (24 + 52 + 52) + app_name
        assert_eq!(VERACK_MAX_BYTES, (24 + 52 + 52) + (1 + s));
    }

    /// And the bound is enforced on the *value*, not only in the total.
    #[test]
    fn test_handshake_string_bound_is_enforced() {
        use crate::net::message::{handshake_string_fits, MAX_HANDSHAKE_STRING_LEN};
        assert!(handshake_string_fits(""));
        assert!(handshake_string_fits(&"a".repeat(MAX_HANDSHAKE_STRING_LEN)));
        assert!(
            !handshake_string_fits(&"a".repeat(MAX_HANDSHAKE_STRING_LEN + 1)),
            "one byte over must be refused — a bound stated only in a message total \
             cannot stop a node from building a message that exceeds it"
        );
    }
}
