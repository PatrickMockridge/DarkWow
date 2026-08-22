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

use std::sync::Arc;

use tracing::{debug, error, info};

use dwow_chain::Transaction as ChainTransaction;
use dwow_sdk::blockchain::BlockVersion;
use dwow_core::{
    net::{
        protocol::protocol_generic::{
            ProtocolGenericHandler, ProtocolGenericHandlerPtr,
        },
        session::SESSION_DEFAULT,
        P2pPtr,
    },
    concurrency::ExecutorPtr,
    tx::Transaction as CoreTransaction,
    Error, Result,
};

use dwow_mempool::MempoolPtr;

/// Atomic pointer to the `ProtocolTx` handler.
pub type ProtocolTxHandlerPtr = Arc<ProtocolTxHandler>;

/// Handler managing P2P transaction messages.
/// Receives transactions from full-node peers (including wallet nodes),
/// validates them, adds to mempool, and relays to other peers.
/// Miners profit from relay: broader propagation = more inbound txs = more fees.
pub struct ProtocolTxHandler {
    /// The generic handler for incoming transaction messages.
    handler: ProtocolGenericHandlerPtr<CoreTransaction, CoreTransaction>,
    /// Mempool for accepted transactions (None in non-mining modes).
    mempool: Option<MempoolPtr>,
    /// P2P network for relay forwarding.
    p2p: P2pPtr,
}

impl dwow_core::barb::ExhibitsBarb for ProtocolTxHandler {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        // Transaction relay — validates txs at mempool admission, broadcasts
        // to peers, propagates network state.
        &[
            dwow_core::barb::BarbId::Verify,
            dwow_core::barb::BarbId::Broadcast,
            dwow_core::barb::BarbId::GossipForward,
        ]
    }
}

impl ProtocolTxHandler {
    /// Initialize a generic protocol handler for transaction messages
    /// and register it with the P2P network.
    pub async fn init(p2p: &P2pPtr, mempool: Option<MempoolPtr>) -> ProtocolTxHandlerPtr {
        debug!(
            target: "dwowd::proto::protocol_tx::init",
            "Adding ProtocolTx to the protocol registry"
        );

        let handler = ProtocolGenericHandler::new(p2p, "ProtocolTx", SESSION_DEFAULT).await;

        Arc::new(Self { handler, mempool, p2p: p2p.clone() })
    }

    /// Start the `ProtocolTx` background task.
    /// Receives transactions from P2P peers, validates, adds to mempool,
    /// and relays to other peers. Miners profit from relay: more propagation
    /// means more inbound txs from wallets that aren't directly connected.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
    ) -> Result<()> {
        let has_mempool = self.mempool.is_some();
        info!(
            target: "dwowd::proto::protocol_tx::start",
            "ProtocolTx handler starting (mempool={}, relay=enabled)",
            if has_mempool { "enabled" } else { "disabled" }
        );

        let handler = self.handler.clone();
        let mempool = self.mempool.clone();
        let p2p = self.p2p.clone();
        self.handler.task.clone().start(
            async move {
                loop {
                    match handler.receiver.recv().await {
                        Ok((_, core_tx)) => {
                            if let Some(ref mp) = mempool {
                                if let Err(e) = admit_tx_to_mempool(core_tx, mp, &p2p).await {
                                    error!(target: "dwowd::proto::protocol_tx",
                                        "Failed admitting tx to mempool: {e}");
                                }
                            }
                        }
                        Err(_) => return Ok(()),
                    }
                }
            },
            |res| async move {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "dwowd::proto::protocol_tx::start", "Failed starting ProtocolTx handler task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        debug!(
            target: "dwowd::proto::protocol_tx::start",
            "ProtocolTx handler task started!"
        );

        Ok(())
    }

    /// Stop the `ProtocolTx` background task.
    pub async fn stop(&self) {
        debug!(target: "dwowd::proto::protocol_tx::stop", "Terminating ProtocolTx handler task...");
        self.handler.task.stop().await;
        debug!(target: "dwowd::proto::protocol_tx::stop", "ProtocolTx handler task terminated!");
    }
}

/// Admit a core transaction into the mempool: convert to the chain
/// representation, run the L2 mempool admission gate, add, and relay.
/// Shared by the P2P `ProtocolTxHandler` loop and the unified-sync `BroadcastTx`
/// path so both rails admit txs identically (harmonization).
pub async fn admit_tx_to_mempool(
    core_tx: CoreTransaction,
    mempool: &MempoolPtr,
    p2p: &P2pPtr,
) -> Result<()> {
    let chain_tx = ChainTransaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: core_tx.calls.iter()
            .map(|leaf| dwow_chain::ContractCall {
                contract_id: leaf.data.contract_id,
                data: leaf.data.data.clone(),
            })
            .collect(),
        lock_time: 0,
        nullifiers: core_tx.nullifiers.clone(),
        witness: dwow_serial::serialize(&core_tx),
    };
    if chain_tx.contract_calls.is_empty() {
        return Ok(());
    }
    let is_coinbase = chain_tx.contract_calls
        .first()
        .map_or(false, |c| c.data.first() == Some(&0x05)
            && c.contract_id == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID);
    if !is_coinbase {
        dwow_chain::zk_verifier::verify_single_tx(&chain_tx)
            .map_err(|e| Error::Custom(format!("L2 mempool verify failed: {e}")))?;
    }
    mempool.add(chain_tx).await?;
    p2p.broadcast(&core_tx).await;
    Ok(())
}
