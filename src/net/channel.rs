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

use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
        Arc,
    },
    time::UNIX_EPOCH,
};

use dwow_serial::{
    AsyncDecodable, AsyncEncodable, SerialDecodable, SerialEncodable, VarInt,
};
use rand::{rngs::OsRng, Rng};
use smol::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    lock::{Mutex as AsyncMutex, OnceCell},
    Executor,
};
use tracing::{debug, error, info, trace, warn};
use url::Url;

use super::{
    dnet::{self, dnetev, DnetEvent},
    hosts::{HostColor, HostsPtr},
    message,
    message::{SerializedMessage, VersionMessage, MAX_COMMAND_LENGTH},
    message_publisher::{MessageSubscription, MessageSubsystem},
    metering::{MeteringConfiguration, MeteringQueue},
    p2p::P2pPtr,
    session::{
        Session, SessionBitFlag, SessionWeakPtr, SESSION_ALL, SESSION_INBOUND, SESSION_OUTBOUND,
        SESSION_REFINE,
    },
    transport::PtStream,
};
use crate::{
    net::BanPolicy,
    concurrency::{msleep, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
    util::{logger::verbose, time::NanoTimestamp},
    Error, Result,
};

/// Atomic pointer to async channel
pub type ChannelPtr = Arc<Channel>;

/// Channel debug info
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ChannelInfo {
    pub resolve_addr: Option<Url>,
    pub connect_addr: Url,
    pub start_time: u64,
    pub id: u32,
    pub transport_mixed: bool,
}

impl ChannelInfo {
    fn new(
        resolve_addr: Option<Url>,
        connect_addr: Url,
        start_time: u64,
        transport_mixed: bool,
    ) -> Self {
        Self { resolve_addr, connect_addr, start_time, id: OsRng.gen(), transport_mixed }
    }
}

/// Async channel for communication between nodes.
pub struct Channel {
    /// The reading half of the transport stream
    reader: AsyncMutex<ReadHalf<Box<dyn PtStream>>>,
    /// The writing half of the transport stream
    writer: AsyncMutex<WriteHalf<Box<dyn PtStream>>>,
    /// The message subsystem instance for this channel
    message_subsystem: MessageSubsystem,
    /// Publisher listening for stop signal for closing this channel
    stop_publisher: PublisherPtr<Error>,
    /// Task that is listening for the stop signal
    receive_task: StoppableTaskPtr,
    /// A boolean marking if this channel is stopped
    stopped: AtomicBool,
    /// Weak pointer to respective session
    pub(in crate::net) session: SessionWeakPtr,
    /// The version message of the node we are connected to.
    /// Some if the version exchange has already occurred, None
    /// otherwise.
    pub version: OnceCell<Arc<VersionMessage>>,
    /// Channel debug info
    pub info: ChannelInfo,
    /// Map holding a `MeteringQueue` for each [`crate::net::Message`]
    /// to perform rate limiting of propagation towards the stream.
    metering_map: AsyncMutex<HashMap<String, MeteringQueue>>,
    /// Counter of SeedErrorMessage responses sent on this channel.
    /// Enforces [`message::MAX_SEED_ERRORS_PER_CONNECTION`] to prevent
    /// DoS amplification.
    seed_error_count: AtomicU64,
}

impl Channel {
    /// Sets up a new channel. Creates a reader and writer [`PtStream`] and
    /// the message publisher subsystem. Performs a network handshake on the
    /// subsystem dispatchers.
    pub async fn new(
        stream: Box<dyn PtStream>,
        resolve_addr: Option<Url>,
        connect_addr: Url,
        session: SessionWeakPtr,
        transport_mixed: bool,
    ) -> Arc<Self> {
        let (reader, writer) = io::split(stream);
        let reader = AsyncMutex::new(reader);
        let writer = AsyncMutex::new(writer);

        let message_subsystem = MessageSubsystem::new();
        Self::setup_dispatchers(&message_subsystem).await;

        let start_time = UNIX_EPOCH.elapsed().unwrap().as_secs();
        let info =
            ChannelInfo::new(resolve_addr, connect_addr.clone(), start_time, transport_mixed);
        let metering_map = AsyncMutex::new(HashMap::new());

        Arc::new(Self {
            reader,
            writer,
            message_subsystem,
            stop_publisher: Publisher::new(),
            receive_task: StoppableTask::new(),
            stopped: AtomicBool::new(false),
            session,
            version: OnceCell::new(),
            info,
            metering_map,
            seed_error_count: AtomicU64::new(0),
        })
    }

    /// Perform network handshake for message subsystem dispatchers.
    async fn setup_dispatchers(subsystem: &MessageSubsystem) {
        subsystem.add_dispatch::<message::VersionMessage>().await;
        subsystem.add_dispatch::<message::VerackMessage>().await;
        subsystem.add_dispatch::<message::PingMessage>().await;
        subsystem.add_dispatch::<message::PongMessage>().await;
        subsystem.add_dispatch::<message::GetAddrsMessage>().await;
        subsystem.add_dispatch::<message::AddrsMessage>().await;
        subsystem.add_dispatch::<message::SeedErrorMessage>().await;
    }

    /// Starts the channel. Runs a receive loop to start receiving messages
    /// or handles a network failure.
    pub fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) {
        debug!(target: "net::channel::start", "START {self:?}");

        let self_ = self.clone();
        self.receive_task.clone().start(
            self.clone().main_receive_loop(),
            |result| self_.handle_stop(result),
            Error::ChannelStopped,
            executor,
        );

        debug!(target: "net::channel::start", "END {self:?}");
    }

    /// Stops the channel.
    /// Notifies all publishers that the channel has been closed in `handle_stop()`.
    pub async fn stop(&self) {
        info!(
            target: "net::channel::stop",
            "[CHANNEL] STOP called for channel {}, stopped={}",
            self.display_address(),
            self.stopped.load(SeqCst)
        );
        self.receive_task.stop().await;
        info!(target: "net::channel::stop", "[CHANNEL] STOP completed for {}", self.display_address());
    }

    /// Creates a subscription to a stopped signal.
    /// If the channel is stopped then this will return a ChannelStopped error.
    pub async fn subscribe_stop(&self) -> Result<Subscription<Error>> {
        debug!(target: "net::channel::subscribe_stop", "START {self:?}");

        if self.is_stopped() {
            return Err(Error::ChannelStopped)
        }

        let sub = self.stop_publisher.clone().subscribe().await;

        debug!(target: "net::channel::subscribe_stop", "END {self:?}");

        Ok(sub)
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(SeqCst)
    }

    /// Send a SeedErrorMessage with metering guard.
    ///
    /// Enforces [`message::MAX_SEED_ERRORS_PER_CONNECTION`] to prevent DoS
    /// amplification. If the per-connection error limit has been reached,
    /// the message is silently dropped (no error is returned to the caller).
    pub async fn send_seed_error(&self, code: u32, reason: impl Into<String>) {
        let count = self.seed_error_count.fetch_add(1, SeqCst);
        if count >= message::MAX_SEED_ERRORS_PER_CONNECTION {
            debug!(
                target: "net::channel::send_seed_error",
                "Seed error limit ({}) reached for {}, dropping error code={}",
                message::MAX_SEED_ERRORS_PER_CONNECTION,
                self.display_address(),
                code,
            );
            return;
        }
        let msg = message::SeedErrorMessage { code, reason: reason.into() };
        if let Err(e) = self.send(&msg).await {
            debug!(
                target: "net::channel::send_seed_error",
                "Failed to send SeedErrorMessage (code={}) to {}: {e}",
                code,
                self.display_address(),
            );
        }
    }

    /// Sends a message across a channel. First it converts the message
    /// into a `SerializedMessage` and then calls `send_serialized` to send it.
    /// Returns an error if something goes wrong.
    pub async fn send<M: message::Message>(&self, message: &M) -> Result<()> {
        self.send_serialized(
            &SerializedMessage::new(message).await,
            &M::METERING_SCORE,
            &M::METERING_CONFIGURATION,
        )
        .await
    }

    /// Sends the encoded payload of provided `SerializedMessage` across the channel.
    ///
    /// We first check if we should apply some throttling, based on the provided
    /// `Message` configuration. We always sleep 2x times more than the expected one,
    /// so we don't flood the peer.
    /// Then, calls `send_message` that creates a new payload and sends it over the
    /// network transport as a packet.
    /// Returns an error if something goes wrong.
    pub async fn send_serialized(
        &self,
        message: &SerializedMessage,
        metering_score: &u64,
        metering_config: &MeteringConfiguration,
    ) -> Result<()> {
        debug!(
             target: "net::channel::send", "[START] command={} {self:?}",
             message.command,
        );

        // Check if we need to initialize a `MeteringQueue`
        // for this specific `Message`.
        let mut lock = self.metering_map.lock().await;
        if !lock.contains_key(&message.command) {
            lock.insert(message.command.clone(), MeteringQueue::new(metering_config.clone()));
        }

        // Insert metering information and grab potential sleep time.
        // It's safe to unwrap here since we initialized the value
        // previously.
        let queue = lock.get_mut(&message.command).unwrap();
        queue.push(metering_score);
        let sleep_time = queue.sleep_time();
        drop(lock);

        // Check if we need to sleep
        if let Some(sleep_time) = sleep_time {
            let sleep_time = 2 * sleep_time;
            debug!(
                target: "net::channel::send",
                "[P2P] Channel rate limit is active, sleeping before sending for: {sleep_time} (ms)"
            );
            msleep(sleep_time).await;
        }

        // Check if the channel is stopped, so we can abort
        if self.is_stopped() {
            return Err(Error::ChannelStopped)
        }

        // Catch failure and stop channel, return the real error
        if let Err(e) = self.send_message(message).await {
            if self.session.upgrade()
                .map(|s| s.type_id() & (SESSION_ALL & !SESSION_REFINE) != 0)
                .unwrap_or(false)
            {
                error!(
                    target: "net::channel::send", "[P2P] Channel send error for [{self:?}]: {e}"
                );
            }
            self.stop().await;
            return Err(e)
        }

        debug!(
            target: "net::channel::send", "[END] command={} {self:?}",
            message.command
        );

        Ok(())
    }

    /// Sends the encoded payload of provided `SerializedMessage` by writing
    /// the data to the channel async stream.
    async fn send_message(&self, message: &SerializedMessage) -> Result<()> {
        // type-system.md §4, §10.5 obligation 2: malformed messages
        // SHALL return typed errors (↓bad-msg) — not crash the node.
        if message.command.is_empty() {
            return Err(Error::MessageInvalid);
        }

        let stream = &mut *self.writer.lock().await;
        let mut written: usize = 0;

        dnetev!(self, SendMessage, {
            chan: self.info.clone(),
            cmd: message.command.clone(),
            time: NanoTimestamp::current_time(),
        });

        trace!(target: "net::channel::send_message", "Sending magic...");
        let magic_bytes = self.p2p().settings().read().await.magic_bytes.0;
        written += magic_bytes.encode_async(stream).await?;
        trace!(target: "net::channel::send_message", "Sent magic");

        trace!(target: "net::channel::send_message", "Sending command...");
        written += message.command.encode_async(stream).await?;
        trace!(target: "net::channel::send_message", "Sent command: {}", message.command);

        trace!(target: "net::channel::send_message", "Sending payload...");
        // First extract the length of the payload as a VarInt and write it to the stream.
        written += VarInt(message.payload.len() as u64).encode_async(stream).await?;
        // Then write the encoded payload itself to the stream.
        stream.write_all(&message.payload).await?;
        written += message.payload.len();

        trace!(target: "net::channel::send_message", "Sent payload {} bytes, total bytes {written}",
            message.payload.len());

        stream.flush().await?;

        Ok(())
    }

    /// Returns a decoded Message command. We start by extracting the length
    /// from the stream, then allocate the precise buffer for this length
    /// using stream.take(). This manual deserialization provides a basic
    /// DDOS protection, since it prevents nodes from sending an arbitarily
    /// large payload.
    pub async fn read_command<R: AsyncRead + Unpin + Send + Sized>(
        &self,
        stream: &mut R,
    ) -> Result<String> {
        // Messages should have a 4 byte header of magic digits.
        // This is used for network debugging.
        let mut magic = [0u8; 4];
        trace!(target: "net::channel::read_command", "Reading magic...");
        stream.read_exact(&mut magic).await?;

        trace!(target: "net::channel::read_command", "Read magic {magic:?}");
        let magic_bytes = self.p2p().settings().read().await.magic_bytes.0;
        if magic != magic_bytes {
            error!(target: "net::channel::read_command", "Error: Magic bytes mismatch");

            // Send structured error so the peer knows WHY it was rejected.
            // Magic bytes identify the P2P network — mismatch means the peer
            // is on the wrong network (or probing).
            self.send_seed_error(
                message::SEED_ERR_FORBIDDEN,
                format!(
                    "magic bytes mismatch: expected {:?}, got {:?} — wrong network",
                    magic_bytes, magic,
                ),
            ).await;

            // If it is outbound, ban the host so we don't share it with other nodes
            if self.session_type_id() & SESSION_OUTBOUND != 0 {
                if let BanPolicy::Strict = self.p2p().settings().read().await.ban_policy {
                    self.ban().await;
                }
            }

            return Err(Error::MalformedPacket)
        }

        // First extract the length from the stream
        let cmd_len = VarInt::decode_async(stream).await?.0;
        if cmd_len > (MAX_COMMAND_LENGTH as u64) {
            error!(target: "net::channel::read_command",
                "Error: Command length ({cmd_len}) exceeds configured limit ({MAX_COMMAND_LENGTH}). Dropping...");
            return Err(Error::MessageInvalid);
        }

        // Then extract precisely `cmd_len` items from the stream.
        let mut take = stream.take(cmd_len);

        // Deserialize into a vector of `cmd_len` size.
        let mut bytes = vec![0; cmd_len.try_into().unwrap()];
        take.read_exact(&mut bytes).await?;

        let command = String::from_utf8(bytes)?;

        Ok(command)
    }

    /// Subscribe to a message on the message subsystem.
    /// Register a dispatcher for message type `M` on this channel.
    /// Must be called before `subscribe_msg::<M>()` if the dispatcher
    /// was not registered in `setup_dispatchers()`.
    pub async fn add_dispatch<M: message::Message>(&self) {
        self.message_subsystem.add_dispatch::<M>().await;
    }

    pub async fn subscribe_msg<M: message::Message>(&self) -> Result<MessageSubscription<M>> {
        debug!(
            target: "net::channel::subscribe_msg", "[START] command={} addr={}",
            M::NAME, self.display_address().as_str()
        );

        // Reject subscription on a stopped channel. Without this check,
        // subscribe_msg succeeds on a dead channel, the subscription's
        // receive() blocks forever (the main loop is dead, no messages
        // arrive), and the subscriber hangs. HAZOP C2/C4: half-open
        // channel → zombie subscription.
        if self.is_stopped() {
            warn!(
                target: "net::channel::subscribe_msg",
                "subscribe_msg::<{}>() on stopped channel {} — returning ChannelStopped",
                M::NAME, self.display_address().as_str()
            );
            return Err(Error::ChannelStopped)
        }

        debug!(
            target: "net::channel::subscribe_msg",
            "TRACE: about to call message_subsystem.subscribe::<{}>()", M::NAME
        );
        let sub = self.message_subsystem.subscribe::<M>().await;
        debug!(
            target: "net::channel::subscribe_msg",
            "TRACE: message_subsystem.subscribe::<{}>() returned", M::NAME
        );

        debug!(
            target: "net::channel::subscribe_msg", "[END] command={} addr={}",
            M::NAME, self.display_address().as_str()
        );

        sub
    }

    /// Handle network errors. Broadcast the stop event to all subscribers
    /// — both stop_publisher and message_subsystem — so blocked receive()
    /// calls wake up with ChannelStopped. HAZOP C3/C9: graceful disconnect
    /// MUST also call trigger_error() or subscribers hang forever.
    async fn handle_stop(self: Arc<Self>, result: Result<()>) {
        debug!(target: "net::channel::handle_stop", "[START] {self:?}");

        self.stopped.store(true, SeqCst);

        let err = match &result {
            Ok(()) => {
                info!(
                    target: "net::channel::handle_stop",
                    "[CHANNEL] Channel {} stopped normally", self.display_address()
                );
                Error::ChannelStopped
            }
            Err(e) => {
                info!(
                    target: "net::channel::handle_stop",
                    "[CHANNEL] Channel {} STOPPING with error: {}", self.display_address(), e
                );
                e.clone()
            }
        };

        self.stop_publisher.notify(Error::ChannelStopped).await;
        self.message_subsystem.trigger_error(err).await;

        debug!(target: "net::channel::handle_stop", "[END] {self:?}");
    }

    /// Run the receive loop. Start receiving messages or handle network failure.
    async fn main_receive_loop(self: Arc<Self>) -> Result<()> {
        info!(
            target: "net::channel::main_receive_loop",
            "[CHANNEL] main_receive_loop START for {}", self.display_address()
        );

        // Acquire reader lock
        let reader = &mut *self.reader.lock().await;

        // Run loop
        loop {
            let command = match self.read_command(reader).await {
                Ok(command) => command,
                Err(err) => {
                    if Self::is_eof_error(&err) {
                        info!(
                            target: "net::channel::main_receive_loop",
                            "[CHANNEL] Channel {} disconnected (EOF)",
                            self.display_address()
                        );
                    } else if let Error::MessageInvalid = err {
                        // The command name length has exceeded the limit, this is possibly a malicious attack so ban it
                        if let BanPolicy::Strict = self.p2p().settings().read().await.ban_policy {
                            self.ban().await;
                        }
                    } else if self.session.upgrade()
                        .map(|s| s.type_id() & (SESSION_ALL & !SESSION_REFINE) != 0)
                        .unwrap_or(false)
                    {
                        error!(
                            target: "net::channel::main_receive_loop",
                            "[P2P] Read error on channel {}: {err}",
                            self.display_address()
                        );
                    }

                    info!(
                        target: "net::channel::main_receive_loop",
                        "[CHANNEL] Stopping channel {} due to read error: {}",
                        self.display_address(),
                        err
                    );
                    return Err(err)
                }
            };

            dnetev!(self, RecvMessage, {
                chan: self.info.clone(),
                cmd: command.clone(),
                time: NanoTimestamp::current_time(),
            });

            // Send result to our publishers
            match self.message_subsystem.notify(&command, reader).await {
                Ok(()) => {}
                Err(Error::MissingDispatcher) => {
                    let Some(session) = self.session.upgrade() else {
                        return Err(Error::ChannelStopped);
                    };
                    if session.type_id() != SESSION_REFINE {
                        warn!(
                        target: "net::channel::main_receive_loop",
                        "MissingDispatcher for command={command}, channel={self:?}"
                        );
                        self.send_seed_error(
                            message::SEED_ERR_UNKNOWN_MESSAGE,
                            format!("unknown message type: {}", command),
                        ).await;
                        if let BanPolicy::Strict = self.p2p().settings().read().await.ban_policy {
                            self.ban().await;
                            return Err(Error::MissingDispatcher)
                        }
                        // Relaxed (or no ban-policy): log and continue
                    }
                }
                Err(Error::MessageInvalid) => {
                    let Some(session) = self.session.upgrade() else {
                        return Err(Error::ChannelStopped);
                    };
                    if session.type_id() != SESSION_REFINE {
                        warn!(
                        target: "net::channel::main_receive_loop",
                        "MessageInvalid for command={command}, channel={self:?} \
                         (payload exceeds MAX_BYTES or failed deserialization)"
                        );
                        self.send_seed_error(
                            message::SEED_ERR_BAD_REQUEST,
                            format!("invalid message: {} (payload exceeds limit or malformed)", command),
                        ).await;
                        if let BanPolicy::Strict = self.p2p().settings().read().await.ban_policy {
                            self.ban().await;
                            return Err(Error::MessageInvalid)
                        }
                    }
                }
                Err(Error::MeteringLimitExceeded) => {
                    let Some(session) = self.session.upgrade() else {
                        return Err(Error::ChannelStopped);
                    };
                    if session.type_id() != SESSION_REFINE {
                        warn!(
                        target: "net::channel::main_receive_loop",
                        "MeteringLimitExceeded for command={command}, channel={self:?}"
                        );
                        if let BanPolicy::Strict = self.p2p().settings().read().await.ban_policy {
                            self.ban().await;
                            return Err(Error::MeteringLimitExceeded)
                        }
                    }
                }
                Err(e) => {
                    error!(
                        target: "net::channel::main_receive_loop",
                        "Unexpected error from notify() for command={command}: {e}"
                    );
                    return Err(Error::ChannelStopped)
                }
            }
        }
    }

    /// Ban a malicious peer and stop the channel.
    pub async fn ban(&self) {
        debug!(target: "net::channel::ban", "START {self:?}");
        debug!(target: "net::channel::ban", "Peer: {:?}", self.display_address());

        // Just store the hostname if this is an inbound session.
        // This will block all ports from this peer by setting
        // `hosts.block_all_ports()` to true.
        let peer = {
            if self.session_type_id() & SESSION_INBOUND != 0 {
                if self.address().host().is_none() {
                    error!("[P2P] ban() caught Url without host: {:?}", self.display_address());
                    return
                }

                // An inbound Tor connection can't really be banned :)
                #[cfg(feature = "p2p-tor")]
                if (self.address().scheme() == "tor" || self.address().scheme() == "tor+tls") &&
                    self.p2p().hosts().is_local_host(self.address())
                {
                    return
                }

                if self.address().scheme() == "unix" {
                    return
                }

                // If we already have a successful connection with this host on another port,
                // this might indicate a misconfiguration or unintended overlap between separate P2P networks.
                // To prevent interference, we block only this specific port rather than the entire host.
                if self.hosts().has_existing_connection(self.address()) {
                    self.address().clone()
                } else {
                    let mut addr = self.address().clone();
                    addr.set_port(None).unwrap();
                    addr
                }
            } else {
                self.address().clone()
            }
        };

        let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
        verbose!(target: "net::channel::ban", "Blacklisting peer={peer}");
        match self.p2p().hosts().move_host(&peer, last_seen, HostColor::Black).await {
            Ok(()) => {
                verbose!(target: "net::channel::ban", "Peer={peer} blacklisted successfully");
            }
            Err(e) => {
                warn!(target: "net::channel::ban", "Could not blacklisted peer={peer}, err={e}");
            }
        }
        self.stop().await;
        debug!(target: "net::channel::ban", "STOP {self:?}");
    }

    /// Returns the relevant socket address for this connection. If this is
    /// an outbound connection, the transport-processed resolve_addr will
    /// be returned except for transport mixed connections, to make sure
    /// mixed hosts don't enter hostlist.
    /// Otherwise for inbound connections it will default
    /// to connect_addr.
    pub fn address(&self) -> &Url {
        if !self.info.transport_mixed {
            if let Some(resolve_addr) = &self.info.resolve_addr {
                return resolve_addr
            }
        }
        &self.info.connect_addr
    }

    /// Returns the address used for UI purposes like in logging or tools like dnet.
    /// For transport_mixed connection shows the mixed address.
    pub fn display_address(&self) -> &Url {
        self.info.resolve_addr.as_ref().unwrap_or(&self.info.connect_addr)
    }

    /// Returns the socket address that has undergone transport
    /// processing, if it exists. Returns None otherwise.
    pub fn resolve_addr(&self) -> Option<Url> {
        self.info.resolve_addr.clone()
    }

    /// Return the socket address without transport processing.
    pub fn connect_addr(&self) -> &Url {
        &self.info.connect_addr
    }

    /// Set the VersionMessage of the node this channel is connected
    /// to. Called on receiving a version message in `ProtocolVersion`.
    pub(crate) async fn set_version(&self, version: Arc<VersionMessage>) {
        self.version.set(version).await.unwrap();
    }
    /// Should only be called after the version exchange has been completed.
    pub fn get_version(&self) -> Arc<VersionMessage> {
        self.version.get().unwrap().clone()
    }

    /// Returns the inner [`MessageSubsystem`] reference
    pub fn message_subsystem(&self) -> &MessageSubsystem {
        &self.message_subsystem
    }

    fn session(&self) -> Arc<dyn Session> {
        self.session.upgrade().expect("Session dropped while Channel active")
    }

    pub fn session_type_id(&self) -> SessionBitFlag {
        let session = self.session();
        session.type_id()
    }

    #[inline]
    pub fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }
    #[inline]
    pub fn hosts(&self) -> HostsPtr {
        self.p2p().hosts()
    }

    fn is_eof_error(err: &Error) -> bool {
        match err {
            Error::Io(ioerr) => ioerr == &std::io::ErrorKind::UnexpectedEof,
            _ => false,
        }
    }
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<Channel addr='{}' id={}>", self.display_address(), self.info.id)
    }
}
