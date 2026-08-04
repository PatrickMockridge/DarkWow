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

//! Bootstrap sync session — connects to known peers for initial hostlist discovery.
//!
//! ── Terminology ──────────────────────────────────────────────────────────
//! "Seed" in this module means: blockchain bootstrap peer. DarkWow has a flat
//! P2P mesh — every node is a full peer. There is no seed/node hierarchy in
//! the blockchain network. A "seed" in this context is simply a known peer
//! you connect to first, to discover other peers via hostlist exchange.
//!
//! NOT to be confused with: lilith seed — an external P2P seed for the
//! tau/darkirc/dchat overlay networks. Lilith seeds ARE genuine seed nodes
//! in the traditional P2P sense. See bin/lilith/.
//! ─────────────────────────────────────────────────────────────────────────
//!
//! A new bootstrap sync session is created every time we call [`P2p::start()`].
//! The session loops through all the configured bootstrap peers and creates a
//! corresponding `Slot`. Slots are started, but sit in a suspended state until
//! they are activated by a call to notify (see: `p2p.seed()` — named for the
//! upstream convention; in DarkWow this is "bootstrap from known peers").
//!
//! When a `Slot` has been activated by a call to `notify()`, it will try to
//! connect to the given peer address using a [`Connector`]. This will either
//! connect successfully or fail with a warning. Results of each `Slot` are
//! gathered in an `AtomicBool` so that we can handle the error elsewhere.
//!
//! If a bootstrap peer connects successfully, it runs a version exchange
//! protocol, stores the channel in the p2p list of channels, and disconnects,
//! removing the channel from the channel list. The peer's hostlist populates
//! the outbound session's greylist, enabling connection to additional peers.
//!
//! The channel is registered using the [`Session::register_channel()`] trait
//! method. This invokes the Protocol Registry method `attach()`. Usually this
//! returns a list of protocols that we loop through and start. In this case,
//! `attach()` uses the bitflag selector to identify bootstrap sessions and
//! exclude them from the regular peer list (bootstrap connections are transient
//! — they exist only for hostlist exchange, not for ongoing sync).
//!
//! The version exchange occurs inside `register_channel()`. We create a handshake
//! task that runs the version exchange with the `perform_handshake_protocols()`
//! function. This runs the version exchange protocol, stores the channel in the
//! p2p list of channels, and subscribes to a stop signal.

use std::sync::{
    atomic::{AtomicBool, Ordering::SeqCst},
    Arc, Weak,
};
use std::time::Instant;

use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use smol::lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tracing::{debug, error, info, warn};
use url::Url;

use super::{
    super::{
        connector::Connector,
        hosts::HostColor,
        p2p::{P2p, P2pPtr},
        settings::Settings,
    },
    Session, SessionBitFlag, SESSION_SEED,
};
use crate::{
    net::hosts::HostState,
    concurrency::{CondVar, StoppableTask, StoppableTaskPtr},
    util::logger::verbose,
    Error, Result,
};

pub type SeedSyncSessionPtr = Arc<SeedSyncSession>;

/// Manages transient connections to known bootstrap peers for hostlist discovery.
///
/// ── Terminology ──────────────────────────────────────────────────────
/// Despite the name, this is NOT a "seed server" in the traditional P2P
/// sense. DarkWow's blockchain network is a flat P2P mesh — every node is
/// a full peer. This session connects to configured known peers, exchanges
/// hostlists, then disconnects. The outbound session then connects to the
/// discovered peers for ongoing block sync.
/// ─────────────────────────────────────────────────────────────────────
pub struct SeedSyncSession {
    pub(in crate::net) p2p: Weak<P2p>,
    slots: AsyncMutex<Vec<Arc<Slot>>>,
}

impl SeedSyncSession {
    /// Create a new seed sync session instance
    pub(crate) fn new(p2p: Weak<P2p>) -> SeedSyncSessionPtr {
        Arc::new(Self { p2p, slots: AsyncMutex::new(Vec::new()) })
    }

    /// Initialize the seedsync session. Each slot is suspended while it waits
    /// for a call to notify().
    pub(crate) async fn start(self: Arc<Self>) {
        // Activate mutex lock on connection slots.
        let mut slots = self.slots.lock().await;

        let mut futures = FuturesUnordered::new();

        let self_ = Arc::downgrade(&self);

        // Initialize a slot for each configured seed.
        // Connections will be started by not yet activated.
        for seed in &self.p2p().settings().read().await.seeds {
            let slot = Slot::new(self_.clone(), seed.clone(), self.p2p().settings());
            futures.push(slot.clone().start());
            slots.push(slot);
        }

        while (futures.next().await).is_some() {}
    }

    /// Activate the slots so they can continue with the seedsync process.
    /// Called in `p2p.seed()`.
    pub(crate) async fn notify(&self) {
        let slots = &*self.slots.lock().await;

        info!(target: "net::seedsync_session",
            "Activating {} seed slots", slots.len());

        for slot in slots {
            slot.notify();
        }
    }

    /// Stop the seedsync session.
    pub(crate) async fn stop(&self) {
        debug!(target: "net::seedsync_session", "Stopping seed sync session...");
        let slots = &*self.slots.lock().await;
        let mut futures = FuturesUnordered::new();

        for slot in slots {
            futures.push(slot.clone().stop());
        }

        while (futures.next().await).is_some() {}
        debug!(target: "net::seedsync_session", "Seed sync session stopped!");
    }

    /// Returns true if every seed attempt per slot has failed.
    pub async fn all_failed(&self) -> bool {
        let slots = &*self.slots.lock().await;
        slots.iter().all(|s| s._failed())
    }

    /// Returns Err(Error::SeedFailed) if all seed slots have failed.
    pub async fn check_seed_result(&self) -> Result<()> {
        if self.all_failed().await {
            return Err(Error::SeedFailed)
        }
        Ok(())
    }

    /// D5: Per-slot diagnostic state for wallet sync status.
    pub async fn slot_states(&self) -> Vec<SeedSlotInfo> {
        let slots = self.slots.lock().await;
        slots.iter().map(|s| SeedSlotInfo {
            addr: s.addr.to_string(),
            failed: s._failed(),
            completed: s.completed.load(SeqCst),
            inactive_secs: s.last_activity.lock().unwrap().elapsed().as_secs(),
        }).collect()
    }
}

/// D5: Public diagnostic info for a single seed slot.
#[derive(Debug, Clone)]
pub struct SeedSlotInfo {
    pub addr: String,
    pub failed: bool,
    pub completed: bool,
    pub inactive_secs: u64,
}

#[async_trait]
impl Session for SeedSyncSession {
    fn p2p(&self) -> P2pPtr {
        // §2.3.2: upgrade() failures SHALL produce errors, not panics.
// Full error propagation requires Session trait refactor (Phase D).
self.p2p.upgrade().expect("P2p dropped while SeedSyncSession active")
    }

    fn type_id(&self) -> SessionBitFlag {
        SESSION_SEED
    }

    async fn reload(self: Arc<Self>) {}
}

struct Slot {
    addr: Url,
    process: StoppableTaskPtr,
    wakeup_self: CondVar,
    session: Weak<SeedSyncSession>,
    connector: Connector,
    failed: AtomicBool,
    /// D5: Tracks last lifecycle event for liveness watchdog.
    last_activity: std::sync::Mutex<Instant>,
    /// D5: True when slot exited due to HostStateBlocked (outbound session
    /// already connected the same address — not a failure, job done).
    completed: AtomicBool,
}

impl Slot {
    fn new(
        session: Weak<SeedSyncSession>,
        addr: Url,
        settings: Arc<AsyncRwLock<Settings>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            addr,
            process: StoppableTask::new(),
            wakeup_self: CondVar::new(),
            session: session.clone(),
            connector: Connector::new(settings, session),
            failed: AtomicBool::new(false),
            last_activity: std::sync::Mutex::new(Instant::now()),
            completed: AtomicBool::new(false),
        })
    }

    fn touch_activity(&self) {
        *self.last_activity.lock().unwrap() = Instant::now();
    }

    async fn start(self: Arc<Self>) {
        let ex = self.p2p().executor();

        // D5: Watchdog task — checks slot liveness every 60s.
        // A slot stuck in wait() with no notify() signal for >60s
        // emits ERROR and appears in wallet diagnostic output.
        let slot_weak = Arc::downgrade(&self);
        ex.spawn(async move {
            loop {
                smol::Timer::after(std::time::Duration::from_secs(60)).await;
                let slot = match slot_weak.upgrade() {
                    Some(s) => s,
                    None => break,
                };
                if slot.completed.load(SeqCst) {
                    break;
                }
                let elapsed = slot.last_activity.lock().unwrap().elapsed();
                if elapsed > std::time::Duration::from_secs(60) {
                    error!(target: "net::seedsync_session",
                        "SEED SLOT LIVENESS: {} inactive for {}s. \
                         Slot is waiting for next notify() cycle and may be stuck.",
                        slot.addr, elapsed.as_secs());
                }
            }
        }).detach();

        self.process.clone().start(
            async move {
                self.run().await;
                // D5: Slot loop now exits on HostStateBlocked.
                Ok(())
            },
            // Ignore stop handler
            |_| async {},
            Error::NetworkServiceStopped,
            ex,
        );
    }

    /// Main seedsync connection process that is started on `p2p.start()` but does
    /// not proceed until it receives a call to `notify()` (called in `p2p.seed()`).
    /// Resets the CondVar after each run to re-suspend the connection process until
    /// `notify()` is called again.
    async fn run(self: Arc<Self>) {
        let ex = self.p2p().executor();
        let hosts = self.p2p().hosts();

        loop {
            // Wait for a signal from notify() before proceeding with the seedsync.
            self.wait().await;

            debug!(
                target: "net::seedsync_session", "SeedSyncSession::start_seed() [START]",
            );

            if let Err(e) = hosts.try_register(self.addr.clone(), HostState::Connect) {
                // D5: HostStateBlocked means the outbound session already connected
                // this address — the seed slot has done its job. Exit cleanly.
                if matches!(&e, crate::Error::HostStateBlocked(_, _)) {
                    info!(target: "net::seedsync_session",
                        "[P2P] Seed slot {}: host already connected via outbound session — job complete", &self.addr);
                    self.completed.store(true, SeqCst);
                    self.touch_activity();
                    break;
                }

                warn!(target: "net::seedsync_session",
                    "[P2P] Cannot connect to seed={}, err={e}", &self.addr);

                // Reset the CondVar for future use.
                self.touch_activity();
                self.reset();

                continue
            }
            self.touch_activity();

            match self.connector.connect(&self.addr).await {
                Ok((_, ch)) => {
                    info!(
                        target: "net::seedsync_session",
                        "[P2P] Connected seed [{}]",
                        ch.display_address()
                    );

                    match self.session().register_channel(ch.clone(), ex.clone()).await {
                        Ok(()) => {
                            self.failed.store(false, SeqCst);

                            info!(
                                target: "net::seedsync_session",
                                "[P2P] Disconnecting from seed [{}]",
                                ch.display_address()
                            );
                            ch.stop().await;

                            // Seed process complete
                            let grey_count = hosts.container.fetch_all(HostColor::Grey).len();
                            if grey_count == 0 {
                                warn!(target: "net::seedsync_session",
                                    "[P2P] Greylist empty after seeding — no peer addresses received");
                            } else {
                                info!(target: "net::seedsync_session",
                                    "[P2P] Seed hostlist: {} greylist entries after seed sync",
                                    grey_count);
                            }

                            // Reset the CondVar for future use.
                            self.reset();
                        }

                        Err(e) => {
                            warn!(
                                target: "net::seedsync_session",
                                "[P2P] Seed register_channel FAILED [{}]: {e}",
                                ch.display_address()
                            );
                            self.handle_failure(ch.address());

                            continue
                        }
                    }
                }

                Err(e) => {
                    warn!(
                        target: "net::seedsync_session",
                        "[P2P] Unable to connect to seed: {e}",
                    );
                    self.handle_failure(&self.addr);

                    continue
                }
            }
            debug!(
                target: "net::seedsync_session",
                "SeedSyncSession::start_seed() [END]",
            );
        }
    }

    fn handle_failure(&self, addr: &Url) {
        self.failed.store(true, SeqCst);

        // Free up this addr for future operations.
        if let Err(e) = self.p2p().hosts().unregister(addr) {
            verbose!(target: "net::seedsync_session", "[P2P] Error while unregistering addr={addr}, err={e}");
        }

        // Reset the CondVar for future use.
        self.reset();
    }

    fn _failed(&self) -> bool {
        self.failed.load(SeqCst)
    }

    fn session(&self) -> SeedSyncSessionPtr {
        self.session.upgrade().expect("SeedSyncSession dropped while Slot active")
    }

    fn p2p(&self) -> P2pPtr {
        self.session().p2p()
    }

    async fn wait(&self) {
        self.wakeup_self.wait().await;
    }

    fn reset(&self) {
        self.wakeup_self.reset()
    }

    fn notify(&self) {
        self.wakeup_self.notify()
    }

    async fn stop(self: Arc<Self>) {
        self.connector.stop();
        self.process.stop().await;
    }
}
