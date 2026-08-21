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

use futures::{
    future::{join_all, select, Either},
    pin_mut,
};
use smol::{lock::RwLock as AsyncRwLock, Executor, Timer};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, UNIX_EPOCH},
};
use tracing::{debug, error, info};

use super::super::{
    channel::ChannelPtr,
    message::{VerackMessage, VersionMessage, SeedErrorCode},
    message_publisher::MessageSubscription,
    settings::Settings,
};
use crate::{
    net::{session::SESSION_OUTBOUND, BanPolicy},
    Error, Result,
};

/// Implements the protocol version handshake sent out by nodes at
/// the beginning of a connection.
pub struct ProtocolVersion {
    channel: ChannelPtr,
    version_sub: MessageSubscription<VersionMessage>,
    verack_sub: MessageSubscription<VerackMessage>,
    settings: Arc<AsyncRwLock<Settings>>,
    /// Set when the version exchange times out. Spawned send_version/recv_version
    /// tasks check this flag and abort early to avoid interacting with a
    /// half-stopped channel.
    cancel_flag: AtomicBool,
}

impl ProtocolVersion {
    /// Create a new version protocol. Makes a version and version ack
    /// subscription, then adds them to a version protocol instance.
    // TODO: This function takes settings as a param, however, it is also reachable through Channel.
    //       Maybe we want to navigate towards Settings through channel->session->p2p->settings
    pub async fn new(channel: ChannelPtr, settings: Arc<AsyncRwLock<Settings>>) -> Arc<Self> {
        // Creates a version subscription
        #[expect(clippy::expect_used, reason = "message dispatcher subscription is always registered")]
        let version_sub =
            channel.subscribe_msg::<VersionMessage>().await.expect("Missing version dispatcher!");

        // Creates a version acknowledgement subscription
        #[expect(clippy::expect_used, reason = "message dispatcher subscription is always registered")]
        let verack_sub =
            channel.subscribe_msg::<VerackMessage>().await.expect("Missing verack dispatcher!");

        Arc::new(Self {
            channel,
            version_sub,
            verack_sub,
            settings,
            cancel_flag: AtomicBool::new(false),
        })
    }

    /// Start version information exchange. Start the timer. Send version
    /// info and wait for version ack. Wait for version info and send
    /// version ack.
    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "net::protocol_version::run", "START => address={}", self.channel.display_address());
        let channel_handshake_timeout =
            self.settings.read().await.channel_handshake_timeout(self.channel.address().scheme());

        let timeout = Timer::after(Duration::from_secs(channel_handshake_timeout));
        let version = self.clone().exchange_versions(executor);

        pin_mut!(timeout);
        pin_mut!(version);

        // Run timer and version exchange at the same time. Either deal
        // with the success or failure of the version exchange or
        // time out.
        match select(version, timeout).await {
            Either::Left((Ok(_), _)) => {
                debug!(target: "net::protocol_version::run", "END => address={}",
                self.channel.display_address());

                Ok(())
            }
            Either::Left((Err(e), _)) => {
                error!(
                    target: "net::protocol_version::run",
                    "[P2P] Version Exchange failed [{}]: {e}",
                    self.channel.display_address()
                );

                self.channel.stop().await;
                Err(e)
            }

            Either::Right((_, _)) => {
                error!(
                    target: "net::protocol_version::run",
                    "[P2P] Version Exchange timed out [{}]",
                    self.channel.display_address(),
                );

                // Send structured error so the peer knows it was a timeout,
                // not a version mismatch or other rejection.
                self.channel.send_seed_error(
                    SeedErrorCode::UpstreamTimeout,
                    "version exchange timed out — seed may be overloaded or unreachable",
                ).await;

                // Signal spawned send_version/recv_version tasks to abort.
                // Without this, select() drops the exchange_versions future but
                // the spawned tasks continue running on the executor, holding
                // Arc<Channel> references and potentially interacting with a
                // half-stopped channel.
                self.cancel_flag.store(true, Ordering::SeqCst);

                self.channel.stop().await;
                Err(Error::ChannelTimeout)
            }
        }
    }

    /// Send and receive version information
    async fn exchange_versions(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        info!(
            target: "net::protocol_version::exchange_versions",
            "START => address={}", self.channel.display_address(),
        );

        let send = executor.spawn(self.clone().send_version());
        let recv = executor.spawn(self.clone().recv_version());

        let rets = join_all(vec![send, recv]).await;
        if let Err(e) = &rets[0] {
            error!(
                target: "net::protocol_version::exchange_versions",
                "send_version() FAILED: {e}"
            );
            return Err(e.clone())
        }

        if let Err(e) = &rets[1] {
            error!(
                target: "net::protocol_version::exchange_versions",
                "recv_version() FAILED: {e}"
            );
            return Err(e.clone())
        }

        info!(
            target: "net::protocol_version::exchange_versions",
            "END => address={}", self.channel.display_address(),
        );
        Ok(())
    }

    /// Send version info and wait for version acknowledgement.
    /// Ensures that the app version is the same.
    async fn send_version(self: Arc<Self>) -> Result<()> {
        // Abort early if the version exchange has timed out on the other path
        if self.cancel_flag.load(Ordering::SeqCst) {
            return Err(Error::ChannelStopped)
        }

        info!(
            target: "net::protocol_version::send_version",
            "START => address={}", self.channel.display_address(),
        );

        let settings = self.settings.read().await;
        let node_id = settings.node_id.clone();
        let app_version = settings.app_version.clone();
        let app_name = settings.app_name.clone();
        drop(settings);

        let external_addrs = self.channel.hosts().external_addrs().await;

        #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
        let timestamp = UNIX_EPOCH.elapsed().unwrap().as_secs();

        let version = VersionMessage {
            node_id,
            app_name: app_name.clone(),
            version: app_version.clone(),
            timestamp,
            connect_recv_addr: self.channel.connect_addr().clone(),
            resolve_recv_addr: self.channel.resolve_addr(),
            ext_send_addr: external_addrs,
            /* NOTE: `features` is a list of enabled features in the
            format Vec<(service, version)>. In the future, Protocols will
            add their own data to this field when they are attached.*/
            features: vec![],
        };
        self.channel.send(&version).await?;

        // Wait for verack
        let verack_msg = self.verack_sub.receive().await?;

        // Validate peer received version against our version.
        info!(
            target: "net::protocol_version::send_version",
            "Received VerackMessage: app_name={} app_version={}",
            verack_msg.app_name, verack_msg.app_version,
        );

        // Log app_name for diagnostics — purely informational (like Bitcoin's
        // user_agent). app_name is NEVER used to reject a connection. Only
        // major.minor version incompatibility triggers rejection.
        if app_name != verack_msg.app_name {
            info!(
                target: "net::protocol_version::send_version",
                "[P2P] Peer app_name differs: ours={} peer={} — informational only, not a rejection",
                app_name, verack_msg.app_name,
            );
        }

        // MAJOR and MINOR must be compatible for protocol interop
        if app_version.major != verack_msg.app_version.major ||
            app_version.minor != verack_msg.app_version.minor
        {
            let mut reasons = Vec::new();
            if app_version.major != verack_msg.app_version.major {
                reasons.push(format!(
                    "major version mismatch: ours={} peer={}",
                    app_version.major, verack_msg.app_version.major
                ));
            }
            if app_version.minor != verack_msg.app_version.minor {
                reasons.push(format!(
                    "minor version mismatch: ours={} peer={}",
                    app_version.minor, verack_msg.app_version.minor
                ));
            }
            let reason = reasons.join("; ");

            error!(
                target: "net::protocol_version::send_version",
                "[P2P] Version mismatch from {}: {}. Disconnecting...",
                self.channel.display_address(),
                reason,
            );

            // Send structured error to the peer before disconnecting
            self.channel.send_seed_error(SeedErrorCode::VersionMismatch, reason).await;

            // If it is outbound, ban the host so we don't share it with other nodes
            if self.channel.session_type_id() & SESSION_OUTBOUND != 0 {
                if let BanPolicy::Strict = self.channel.p2p().settings().read().await.ban_policy {
                    self.channel.ban().await;
                }
            }

            self.channel.stop().await;
            return Err(Error::ChannelStopped)
        }

        // Versions are compatible
        info!(
            target: "net::protocol_version::send_version",
            "Version handshake SUCCESSFUL for address={}", self.channel.display_address(),
        );
        Ok(())
    }

    /// Receive version info, validate it, and send verack with app version attached.
    ///
    /// Validation is now symmetric: both send_version() and recv_version() check
    /// app_name and version compatibility before completing their half of the
    /// handshake. The auto-addr insertion is deferred until AFTER validation
    /// succeeds so that mismatched peers cannot poison the hostlist.
    async fn recv_version(self: Arc<Self>) -> Result<()> {
        info!(
            target: "net::protocol_version::recv_version",
            "START => address={}", self.channel.display_address(),
        );

        // Receive version message
        let version = self.version_sub.receive().await?;
        info!(
            target: "net::protocol_version::recv_version",
            "Received VersionMessage: app_name={} version={}",
            version.app_name, version.version
        );

        // Log app_name for diagnostics — purely informational (like Bitcoin's
        // user_agent). app_name is NEVER used to reject a connection.
        let settings = self.settings.read().await;
        let our_version = settings.app_version.clone();
        let our_app_name = settings.app_name.clone();
        drop(settings);

        if our_app_name != version.app_name {
            info!(
                target: "net::protocol_version::recv_version",
                "[P2P] Peer app_name differs: ours={} peer={} — informational only, not a rejection",
                our_app_name, version.app_name,
            );
        }

        // Validate major.minor version compatibility BEFORE sending Verack.
        // Only version incompatibility triggers rejection — app_name is informational.
        if our_version.major != version.version.major ||
            our_version.minor != version.version.minor
        {
            let mut reasons = Vec::new();
            if our_version.major != version.version.major {
                reasons.push(format!(
                    "major version mismatch: ours={} peer={}",
                    our_version.major, version.version.major
                ));
            }
            if our_version.minor != version.version.minor {
                reasons.push(format!(
                    "minor version mismatch: ours={} peer={}",
                    our_version.minor, version.version.minor
                ));
            }
            let reason = reasons.join("; ");

            error!(
                target: "net::protocol_version::recv_version",
                "[P2P] Version mismatch from {}: {}. Rejecting...",
                self.channel.display_address(),
                reason,
            );

            self.channel.send_seed_error(SeedErrorCode::VersionMismatch, reason).await;
            self.channel.stop().await;
            return Err(Error::ChannelStopped)
        }

        // Validation succeeded — safe to insert peer address into hostlist
        if let Some(ipv6_addr) = version.get_ipv6_addr() {
            let hosts = self.channel.p2p().hosts();
            hosts.add_auto_addr(ipv6_addr);
        }

        // Abort early if the version exchange has timed out on the other path
        if self.cancel_flag.load(Ordering::SeqCst) {
            return Err(Error::ChannelStopped)
        }

        self.channel.set_version(version).await;

        // Send verack
        let verack = VerackMessage { app_version: our_version, app_name: our_app_name };
        info!(
            target: "net::protocol_version::recv_version",
            "Sending VerackMessage: app_name={}", verack.app_name
        );
        self.channel.send(&verack).await?;

        info!(
            target: "net::protocol_version::recv_version",
            "END => address={}", self.channel.display_address(),
        );
        Ok(())
    }
}
