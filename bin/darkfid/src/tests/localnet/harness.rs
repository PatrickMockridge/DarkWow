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

//! 5-Node Local Testnet Harness
//!
//! Extends the 2-node harness pattern to 5 nodes (alice, bob, charlie,
//! david, eve), each with their own sled db and real P2P connections.
//! All nodes connect to alice as the seed peer.

use std::sync::Arc;

use dwow::{
    blockchain::{BlockInfo, BlockchainOverlay, Header},
    net::Settings,
    system::sleep,
    tx::{ContractCallLeaf, TransactionBuilder},
    validator::{
        consensus::{Fork, Proposal},
        utils::deploy_native_contracts,
        verification::{apply_producer_transaction, verify_block},
        ValidatorConfig,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_native_token_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, NativeTokenFunction,
    NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1, NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN,
};
use dwow_sdk::{
    crypto::{
        keypair::Keypair,
        pasta_prelude::{Curve, CurveAffine},
        MerkleTree, NATIVE_TOKEN_CONTRACT_ID,
    },
    ContractCall,
};
use dwow_serial::Encodable;
use url::Url;

use crate::{
    proto::ProposalMessage,
    tests::harness::generate_node,
    DarkfiNodePtr,
};

pub struct FiveNodeHarness {
    pub validator_config: ValidatorConfig,
    pub alice: DarkfiNodePtr,
    pub bob: DarkfiNodePtr,
    pub charlie: DarkfiNodePtr,
    pub david: DarkfiNodePtr,
    pub eve: DarkfiNodePtr,
}

impl FiveNodeHarness {
    pub async fn new(
        validator_config: ValidatorConfig,
        ex: &Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        let mut settings =
            Settings { localnet: true, inbound_connections: 8, ..Default::default() };

        // Alice is the seed peer; all others connect to her
        let alice_url = Url::parse("tcp+tls://127.0.0.1:18450")?;
        settings.inbound_addrs = vec![alice_url.clone()];
        settings.peers = vec![];
        let alice = generate_node(&validator_config, &settings, ex, true, None).await?;

        let peers = vec![alice_url.clone()];

        settings.inbound_addrs = vec![Url::parse("tcp+tls://127.0.0.1:18451")?];
        settings.peers = peers.clone();
        let bob = generate_node(&validator_config, &settings, ex, true, None).await?;

        settings.inbound_addrs = vec![Url::parse("tcp+tls://127.0.0.1:18452")?];
        settings.peers = peers.clone();
        let charlie = generate_node(&validator_config, &settings, ex, true, None).await?;

        settings.inbound_addrs = vec![Url::parse("tcp+tls://127.0.0.1:18453")?];
        settings.peers = peers.clone();
        let david = generate_node(&validator_config, &settings, ex, true, None).await?;

        settings.inbound_addrs = vec![Url::parse("tcp+tls://127.0.0.1:18454")?];
        settings.peers = peers;
        let eve = generate_node(&validator_config, &settings, ex, true, None).await?;

        Ok(Self { validator_config, alice, bob, charlie, david, eve })
    }

    pub fn all_nodes(&self) -> [&DarkfiNodePtr; 5] {
        [&self.alice, &self.bob, &self.charlie, &self.david, &self.eve]
    }

    /// Generate the next block on the given fork using the default keypair.
    pub async fn generate_next_block(&self, fork: &mut Fork) -> Result<BlockInfo> {
        let previous = fork.overlay.lock().unwrap().last_block()?;
        let block_height = previous.header.height + 1;
        let last_nonce = previous.header.nonce;

        let keypair = Keypair::default();
        let zkbin_bytes = NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN.to_vec();
        let zkbin = ZkBinary::decode(&zkbin_bytes, false)?;
        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let pk = ProvingKey::build(zkbin.k, &circuit);

        let debris = PoWRewardCallBuilder {
            signature_keypair: keypair,
            block_height,
            fees: 0,
            recipient: None,
            spend_hook: None,
            user_data: None,
            mint_zkbin: zkbin.clone(),
            mint_pk: pk.clone(),
        }
        .build()?;

        let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
        debris.params.encode(&mut data)?;
        let call = ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data };
        let mut tx_builder =
            TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
        let mut tx = tx_builder.build()?;
        let sigs = tx.create_sigs(&[keypair.secret])?;
        tx.signatures = vec![sigs];

        let timestamp = previous.header.timestamp.checked_add(1.into())?;
        let header = Header::new(previous.hash(), block_height, last_nonce, timestamp);
        let mut block = BlockInfo::new_empty(header);
        block.append_txs(vec![tx]);

        // Populate zkbin_data for stateless verification
        let value_coords = debris.params.output.value_commit.to_affine().coordinates().unwrap();
        let instances = vec![
            debris.params.output.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            debris.params.output.token_commit,
        ];
        block.zkbin_data = vec![(*NATIVE_TOKEN_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(), zkbin_bytes, instances)];

        // Apply directly to fork's overlay (not a clone)
        // This fixes the bug where we cloned, applied to clone, but fork's overlay was never updated
        let _ = apply_producer_transaction(
            &fork.overlay,
            block.header.height,
            fork.module.target,
            block.txs.last().unwrap(),
            &mut MerkleTree::new(1),
        )
        .await?;
        let diff = fork.overlay.lock().unwrap().overlay.lock().unwrap().diff(&fork.diffs)?;
        // Push the new diff so fork state is consistent with the applied changes
        fork.diffs.push(diff.clone());
        block.header.state_root = fork.overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

        block.sign(&keypair.secret);

        verify_block(
            &fork.overlay,
            &fork.diffs,
            &mut fork.module,
            &block,
            &previous,
            true,
            self.alice.validator.read().await.verify_fees,
            &block.zkbin_data,
        )
        .await?;
        fork.append_proposal(&Proposal::new(block.clone())).await?;

        Ok(block)
    }

    /// Add blocks to all nodes: append to alice then broadcast via P2P.
    pub async fn add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        for block in blocks {
            let proposal = Proposal::new(block.clone());
            self.alice.validator.write().await.append_proposal(&proposal).await?;
            let message = ProposalMessage(proposal);
            self.alice.p2p_handler.p2p.broadcast(&message).await;
        }

        sleep(10).await;

        for node in self.all_nodes() {
            node.validator.write().await.confirmation().await?;
        }

        Ok(())
    }

    /// Assert all 5 nodes have the same canonical chain length.
    pub async fn verify_consensus(&self, expected_blocks: usize) -> Result<()> {
        for node in self.all_nodes() {
            node.validator
                .read()
                .await
                .validate_blockchain(
                    self.validator_config.pow_target,
                    self.validator_config.pow_fixed_difficulty.clone(),
                )
                .await?;
        }

        let alice_len = self.alice.validator.read().await.blockchain.len();
        assert_eq!(alice_len, expected_blocks, "alice chain length mismatch");

        for (i, node) in self.all_nodes().iter().enumerate() {
            let len = node.validator.read().await.blockchain.len();
            assert_eq!(len, alice_len, "node {} chain length {} != alice {}", i, len, alice_len);
        }

        Ok(())
    }

    /// Assert all 5 nodes have the same canonical tip hash.
    pub async fn verify_tip_agreement(&self) -> Result<()> {
        let alice_tip = self.alice.validator.read().await.blockchain.last()?.1;
        for (i, node) in self.all_nodes().iter().enumerate().skip(1) {
            let tip = node.validator.read().await.blockchain.last()?.1;
            assert_eq!(tip, alice_tip, "node {} tip hash differs from alice", i);
        }
        Ok(())
    }
}
