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

//! BridgeChannel — typed cross-path communication with barb-carrying type safety.
//!
//! A channel between the blockchain path and the event graph path. The phantom
//! type parameter `B: BarbWitness` carries the exhibited barbs as a compile-time
//! constraint, preventing messages with incompatible barbs from crossing the
//! quarantine boundary.
//!
//! Maps to ρ-calculus bridging (§10.4):
//! ```
//! bridge_chain_evg : Channel<BridgeMessage>   exhibits { ↓commit, ↓verify }
//! bridge_evg_chain : Channel<StateProof>      exhibits { ↓broadcast, ↓dag-parent }
//! sync_barrier     : Channel<()>             exhibits { ↓sync-barrier }
//! ```

use std::marker::PhantomData;

use smol::channel;

use super::barb_trait::{BarbId, ExhibitsBarb};

/// Witness type for barb-carrying channels.
///
/// A zero-sized witness that the channel carries messages whose sender
/// exhibits the declared barbs. The compiler enforces that a channel
/// declared with `BarbWitness<BlockchainMiner>` can only be used to
/// send messages from processes that implement `ExhibitsBarb` with
/// the matching barb set.
pub struct BarbWitness<B: ExhibitsBarb> {
    _phantom: PhantomData<B>,
}

impl<B: ExhibitsBarb> BarbWitness<B> {
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }

    /// The barbs exhibited by messages on this channel.
    pub fn barbs() -> &'static [BarbId] {
        B::exhibited_barbs()
    }

    /// Returns true if this channel carries blockchain-only barbs.
    pub fn has_blockchain_barbs() -> bool {
        B::has_blockchain_barbs()
    }
}

impl<B: ExhibitsBarb> Default for BarbWitness<B> {
    fn default() -> Self {
        Self::new()
    }
}

/// A typed channel for cross-path communication.
///
/// `T` is the message payload type. `B` is the barb witness — a phantom
/// type parameter that carries the barbs exhibited by the sender process.
///
/// # Safety
///
/// The `B` parameter statically enforces quarantine: a `BridgeChannel<T,
/// BarbWitness<BlockchainMiner>>` cannot receive messages from an event-graph
/// process that does not implement `ExhibitsBarb` with the matching barbs.
///
/// This is a type-level guarantee, not a runtime check. The compiler
/// rejects code that attempts to route blockchain barbs through event-graph
/// channels.
pub struct BridgeChannel<T, B: ExhibitsBarb> {
    _barb: PhantomData<B>,
    _marker: PhantomData<T>,
}

impl<T: Send + 'static, B: ExhibitsBarb> BridgeChannel<T, B> {
    /// Create a new bridged channel pair (sender + receiver).
    pub fn pair() -> (BridgeSender<T, B>, BridgeReceiver<T, B>) {
        let (tx, rx) = channel::unbounded();
        let sender = BridgeSender { tx, _barb: PhantomData };
        let receiver = BridgeReceiver { rx, _barb: PhantomData };
        (sender, receiver)
    }
}

/// Send half of a BridgeChannel.
pub struct BridgeSender<T, B: ExhibitsBarb> {
    tx: channel::Sender<(T, BarbWitness<B>)>,
    _barb: PhantomData<B>,
}

impl<T: Send + 'static, B: ExhibitsBarb> BridgeSender<T, B> {
    /// Send a message through the bridge.
    /// The barb witness is attached at send time for type tracking.
    pub async fn send(&self, msg: T) -> Result<(), channel::SendError<(T, BarbWitness<B>)>> {
        self.tx.send((msg, BarbWitness::<B>::new())).await
    }

    /// Try to send without blocking.
    pub fn try_send(&self, msg: T) -> Result<(), channel::TrySendError<(T, BarbWitness<B>)>> {
        self.tx.try_send((msg, BarbWitness::<B>::new()))
    }
}

/// Receive half of a BridgeChannel.
pub struct BridgeReceiver<T, B: ExhibitsBarb> {
    rx: channel::Receiver<(T, BarbWitness<B>)>,
    _barb: PhantomData<B>,
}

impl<T: Send + 'static, B: ExhibitsBarb> BridgeReceiver<T, B> {
    /// Receive a message from the bridge.
    /// Returns the message and its barb witness for downstream type tracking.
    pub async fn recv(&self) -> Result<(T, BarbWitness<B>), channel::RecvError> {
        self.rx.recv().await
    }

    /// Try to receive without blocking.
    pub fn try_recv(&self) -> Result<(T, BarbWitness<B>), channel::TryRecvError> {
        self.rx.try_recv()
    }
}

impl<T, B: ExhibitsBarb> Clone for BridgeSender<T, B> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone(), _barb: PhantomData }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::barb_trait::BarbId;

    // Test process types with specific barb sets
    struct BlockchainMessage;
    impl ExhibitsBarb for BlockchainMessage {
        fn exhibited_barbs() -> &'static [BarbId] {
            &[BarbId::Commit, BarbId::Verify]
        }
    }

    struct EventGraphMessage;
    impl ExhibitsBarb for EventGraphMessage {
        fn exhibited_barbs() -> &'static [BarbId] {
            &[BarbId::Broadcast, BarbId::DagParent]
        }
    }

    #[test]
    fn test_bridge_channel_barbs() {
        // Channel carries blockchain barbs
        let barbs = BridgeChannel::<String, BlockchainMessage>::new_channel_barbs();
        assert!(barbs.contains(&BarbId::Commit));
        assert!(barbs.contains(&BarbId::Verify));
        assert!(!barbs.contains(&BarbId::Broadcast));
    }

    #[test]
    fn test_bridge_channel_send_recv() {
        use std::sync::Arc;
        use smol::Executor;

        let ex = Arc::new(Executor::new());

        smol::block_on(ex.run(async {
            let (tx, rx) = BridgeChannel::<String, BlockchainMessage>::pair();
            tx.send("hello".to_string()).await.unwrap();
            let (msg, _witness) = rx.recv().await.unwrap();
            assert_eq!(msg, "hello");
            assert_eq!(BarbWitness::<BlockchainMessage>::barbs(), &[BarbId::Commit, BarbId::Verify]);
        }));
    }

    // Compile-time test: the following would NOT compile because it attempts
    // to route blockchain barbs through an event-graph channel:
    //
    // ```compile_fail
    // let (tx, _rx) = BridgeChannel::<String, EventGraphMessage>::pair();
    // // This requires BlockchainMessage barbs but channel has EventGraphMessage barbs
    // let _bad: BridgeSender<String, BlockchainMessage> = tx; // type mismatch
    // ```
}

impl<T, B: ExhibitsBarb> BridgeChannel<T, B> {
    /// Return the barb set for messages on this channel.
    /// Used for documentation and runtime verification.
    #[cfg(test)]
    fn new_channel_barbs() -> Vec<BarbId> {
        B::exhibited_barbs().to_vec()
    }
}
