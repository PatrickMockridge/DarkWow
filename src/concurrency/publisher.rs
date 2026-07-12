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

use std::{collections::HashMap, sync::Arc};

use rand::{rngs::OsRng, Rng};
use smol::lock::Mutex;
use tracing::warn;

pub type PublisherPtr<T> = Arc<Publisher<T>>;
pub type SubscriptionId = usize;

#[derive(Debug)]
/// Subscription to the Publisher. Created using `publisher.subscribe().await`.
pub struct Subscription<T> {
    id: SubscriptionId,
    recv_queue: smol::channel::Receiver<T>,
    parent: Arc<Publisher<T>>,
}

impl<T: Clone> Subscription<T> {
    pub fn get_id(&self) -> SubscriptionId {
        self.id
    }

    /// Receive message.
    pub async fn receive(&self) -> T {
        let message_result = self.recv_queue.recv().await;

        match message_result {
            Ok(message_result) => message_result,
            Err(err) => {
                panic!("Subscription::receive() recv_queue failed! {err}");
            }
        }
    }

    /// Must be called manually since async Drop is not possible in Rust
    pub async fn unsubscribe(&self) {
        self.parent.clone().unsubscribe(self.id).await
    }
}

/// Simple broadcast (publish-subscribe) class.
#[derive(Debug)]
pub struct Publisher<T> {
    subs: Mutex<HashMap<SubscriptionId, smol::channel::Sender<T>>>,
}

impl<T: Clone> Publisher<T> {
    /// Construct a new publisher.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { subs: Mutex::new(HashMap::new()) })
    }

    fn random_id() -> SubscriptionId {
        OsRng.gen()
    }

    /// Make sure you call this method early in your setup. That way the subscription
    /// will begin accumulating messages from notify.
    /// Then when your main loop begins calling `sub.receive().await`, the messages will
    /// already be queued.
    /// Maximum number of messages buffered per subscriber before dropping.
    const SUBSCRIBER_BUFFER: usize = 1024;

    pub async fn subscribe(self: Arc<Self>) -> Subscription<T> {
        let (sender, recvr) = smol::channel::bounded(Self::SUBSCRIBER_BUFFER);

        // Poor-man's do/while
        let mut subs = self.subs.lock().await;
        let mut sub_id = Self::random_id();
        while subs.contains_key(&sub_id) {
            sub_id = Self::random_id();
        }

        subs.insert(sub_id, sender);

        Subscription { id: sub_id, recv_queue: recvr, parent: self.clone() }
    }

    async fn unsubscribe(self: Arc<Self>, sub_id: SubscriptionId) {
        self.subs.lock().await.remove(&sub_id);
    }

    /// Publish a message to all listening subscriptions.
    pub async fn notify(&self, message_result: T) {
        self.notify_with_exclude(message_result, &[]).await
    }

    /// Publish a message to all listening subscriptions but exclude some subset.
    pub async fn notify_with_exclude(&self, message_result: T, exclude_list: &[SubscriptionId]) {
        for (id, sub) in (*self.subs.lock().await).iter() {
            if exclude_list.contains(id) {
                continue
            }

            match sub.try_send(message_result.clone()) {
                Ok(()) => {}
                Err(smol::channel::TrySendError::Full(_)) => {
                    warn!(
                        target: "concurrency::publisher",
                        "Subscriber {id} channel full, dropping message"
                    );
                }
                Err(smol::channel::TrySendError::Closed(_)) => {
                    warn!(
                        target: "concurrency::publisher",
                        "Subscriber {id} channel closed"
                    );
                }
            }
        }
    }

    /// Clear inactive subscribtions.
    /// Returns a flag indicating if we have active subscriptions
    /// after cleanup.
    pub async fn clear_inactive(&self) -> bool {
        // Grab a lock over current jobs
        let mut subs = self.subs.lock().await;

        // Find inactive subscriptions
        let mut dropped = vec![];
        for (sub, channel) in subs.iter() {
            if channel.receiver_count() == 0 {
                dropped.push(*sub);
            }
        }

        // Drop inactive subscriptions
        for sub in dropped {
            subs.remove(&sub);
        }

        // Return flag indicating if we still have subscriptions
        subs.is_empty()
    }
}
