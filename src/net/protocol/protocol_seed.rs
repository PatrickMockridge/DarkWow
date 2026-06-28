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

use async_trait::async_trait;
use smol::{lock::RwLock as AsyncRwLock, Executor, Timer};
use std::{sync::Arc, time::UNIX_EPOCH};
use tracing::{debug, error, warn};
use super::{
    super::{
        channel::ChannelPtr,
        hosts::{HostColor, HostsPtr},
        message::{
            AddrsMessage, GetAddrsMessage, SeedErrorMessage, seed_error_is_client_error,
            seed_error_is_server_error,
        },
        message_publisher::MessageSubscription,
        p2p::P2pPtr,
        settings::Settings,
    },
    protocol_base::{ProtocolBase, ProtocolBasePtr},
};
use crate::{util::logger::verbose, Result};

/// Implements the seed protocol
pub struct ProtocolSeed {
    channel: ChannelPtr,
    hosts: HostsPtr,
    settings: Arc<AsyncRwLock<Settings>>,
    addr_sub: MessageSubscription<AddrsMessage>,
    seed_err_sub: MessageSubscription<SeedErrorMessage>,
}

const PROTO_NAME: &str = "ProtocolSeed";

impl ProtocolSeed {
    /// Create a new seed protocol.
    pub async fn init(channel: ChannelPtr, p2p: P2pPtr) -> ProtocolBasePtr {
        // Create a subscription to address message
        let addr_sub =
            channel.subscribe_msg::<AddrsMessage>().await.expect("Missing addr dispatcher!");

        // Create a subscription to seed error messages
        let seed_err_sub =
            channel.subscribe_msg::<SeedErrorMessage>().await.expect("Missing seederr dispatcher!");

        Arc::new(Self {
            channel,
            hosts: p2p.hosts(),
            settings: p2p.settings(),
            addr_sub,
            seed_err_sub,
        })
    }

    /// Send our own external addresses over a channel. Set the
    /// last_seen field to now.
    pub async fn send_my_addrs(&self) -> Result<()> {
        debug!(
            target: "net::protocol_seed::send_my_addrs",
            "[START] channel address={}", self.channel.display_address(),
        );

        let external_addrs = self.channel.hosts().external_addrs().await;

        if external_addrs.is_empty() {
            debug!(
                target: "net::protocol_seed::send_my_addrs",
                "External address is not configured. Stopping",
            );
            return Ok(())
        }

        let mut addrs = vec![];

        for addr in external_addrs {
            // Strip query parameters to prevent leaking internal tracking
            // identifiers (e.g., UPnP cookies) that could be used for
            // fingerprinting nodes on the P2P network.
            let mut stripped = addr.clone();
            stripped.set_query(None);
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
            addrs.push((stripped, last_seen));
        }

        debug!(
            target: "net::protocol_seed::send_my_addrs",
            "Broadcasting {} addresses", addrs.len(),
        );

        let ext_addr_msg = AddrsMessage { addrs };
        self.channel.send(&ext_addr_msg).await?;

        debug!(
            target: "net::protocol_seed::send_my_addrs",
            "[END] channel address={}", self.channel.display_address(),
        );

        Ok(())
    }
}

#[async_trait]
impl ProtocolBase for ProtocolSeed {
    /// Seed protocol: simple sequential address exchange
    /// 1. Send our addresses to seed
    /// 2. Request addresses from seed
    /// 3. Receive addresses (with timeout) or seed error, add to greylist
    /// 4. Return - channel closes naturally
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        verbose!(
            target: "net::protocol_seed",
            "[SEED] START address={}", self.channel.display_address()
        );

        // Spawn error handler: receives SeedErrorMessage from the seed and
        // logs it with retry guidance based on HTTP-style error code ranges.
        // 4xx = client error (don't retry without changing the request)
        // 5xx = server error (may retry with backoff)
        let self_err = self.clone();
        ex.spawn(async move {
            loop {
                match self_err.seed_err_sub.receive().await {
                    Ok(msg) => {
                        if seed_error_is_client_error(msg.code) {
                            error!(
                                target: "net::protocol_seed",
                                "[SEED] Client error (4xx) from {} — will NOT retry: code={} reason=\"{}\"",
                                self_err.channel.display_address(),
                                msg.code,
                                msg.reason,
                            );
                        } else if seed_error_is_server_error(msg.code) {
                            warn!(
                                target: "net::protocol_seed",
                                "[SEED] Server error (5xx) from {} — may retry with backoff: code={} reason=\"{}\"",
                                self_err.channel.display_address(),
                                msg.code,
                                msg.reason,
                            );
                        } else {
                            error!(
                                target: "net::protocol_seed",
                                "[SEED] Unknown error code from {}: code={} reason=\"{}\"",
                                self_err.channel.display_address(),
                                msg.code,
                                msg.reason,
                            );
                        }
                    }
                    Err(e) => {
                        debug!(
                            target: "net::protocol_seed",
                            "[SEED] Seed error subscription closed: {e}"
                        );
                        break;
                    }
                }
            }
        }).detach();

        // Step 1: Send our address to the seed
        self.send_my_addrs().await?;

        // Step 2: Build GetAddrsMessage
        let settings = self.settings.read().await;
        let outbound_connections = settings.outbound_connections;
        let getaddrs_max = settings.getaddrs_max;
        let active_profiles = settings.active_profiles.clone();
        drop(settings);

        let get_addr = GetAddrsMessage {
            max: getaddrs_max.unwrap_or(outbound_connections.min(u32::MAX as usize) as u32),
            transports: active_profiles,
        };

        verbose!(
            target: "net::protocol_seed",
            "[SEED] Sending GetAddrsMessage to {}", self.channel.display_address()
        );
        self.channel.send(&get_addr).await?;

        // Step 3: Wait for AddrsMessage with 30-second timeout.
        // Previously blocked forever on receive(), making it impossible
        // to distinguish "seed is slow" from "seed will never respond."
        verbose!(
            target: "net::protocol_seed",
            "[SEED] Waiting for AddrsMessage from {} (30s timeout)", self.channel.display_address()
        );
        let timeout = Timer::after(std::time::Duration::from_secs(30));
        let addrs_msg = {
            let recv = self.addr_sub.receive();
            smol::future::or(async { recv.await }, async { timeout.await; Err(crate::Error::ChannelTimeout) }).await?
        };

        // Step 4: Add received addresses to greylist
        if !addrs_msg.addrs.is_empty() {
            verbose!(
                target: "net::protocol_seed",
                "[SEED] Received {} addrs from {}, adding to greylist",
                addrs_msg.addrs.len(), self.channel.display_address()
            );
            self.hosts.insert(HostColor::Grey, &addrs_msg.addrs).await;
        } else {
            verbose!(
                target: "net::protocol_seed",
                "[SEED] Received empty AddrsMessage from {} — seed has no peers to share",
                self.channel.display_address()
            );
        }

        verbose!(
            target: "net::protocol_seed",
            "[SEED] END address={}", self.channel.display_address()
        );
        Ok(())
    }

    fn name(&self) -> &'static str {
        PROTO_NAME
    }
}
