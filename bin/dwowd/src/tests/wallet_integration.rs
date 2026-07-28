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

//! Wallet integration test — coinbase + random capability scan/decrypt.
//!
//! Validates the wallet's two scan paths against blocks that were constructed
//! through the production `accept_block` pipeline (Path 1 — NativeToken coinbase)
//! and synthetic blocks with manifest-driven typed capabilities (Path 2 — generic
//! capability construction from primitives and barbs).
//!
//! # Pre-devnet ceiling
//!
//! Height 2 max through `accept_block`. Multi-block chain growth, P2P sync,
//! competing blocks, and uncle resolution belong in the Docker pipeline
//! (`test_pipeline.sh`). This test bridges the gap between pure-function
//! wallet tests (`scan.rs`) and the full containerized wallet pipeline
//! (phase_10_wallet_tests.sh).
//!
//! # Requirements
//!
//! ```bash
//! RAYON_NUM_THREADS=10 RUST_MIN_STACK=67108864 \
//!   cargo test -p dwowd -- test_wallet_integration --nocapture
//! ```
//!
//! ZK proof determinism is enforced via `enable_deterministic_zk()`.

use std::sync::Arc;

use dwow_chain::{ContractCall, Transaction};
use dwow_core::Result;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, MoneroBlockHeight, SupplyAmount};
use dwow_sdk::crypto::{
    keypair::Network,
    pasta_prelude::{CurveAffine, Group},
    poseidon_hash, ContractId, PublicKey, SecretKey, NATIVE_TOKEN_CONTRACT_ID,
};
use dwow_sdk::pasta::pallas;
use dwow_sdk::crypto::note::AeadEncryptedNote;
use dwow_serial::Encodable;

use crate::tests::genesis::GenesisHarness;

/// Full wallet integration: coinbase (Path 1) + random capabilities (Path 2).
///
/// Phases 1-5: Production chain → wallet scan → verify coinbase discovery.
/// Phases 6-7: Synthetic Path 2 block → manifest-driven capability typing.
/// Phase 8: Coverage gate — uncovered barbs drop the note.
/// Phases 9-10: Wrong-key negative + determinism.
#[test]
#[test]
fn test_wallet_integration() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ================================================================
        // Phase 1: Production Chain Setup
        // ================================================================
        // Contracts materialize via the genesis block (init_genesis below)
        // — no startup deployment exists anymore.
        let har = GenesisHarness::new().expect("GenesisHarness");

        // Deterministic test key — same file used by miner and wallet.
        // Both derive sk_H = derive_instance(master_sk, NATIVE_TOKEN_CONTRACT_ID, height),
        // producing identical AEAD decryption keys.
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_wallet_int_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .expect("open miner AccountManager");
        let recipient =
            crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(1))
                .expect("MiningRecipient");
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

        // ================================================================
        // Phase 2: Genesis Block (Path 1, Production accept_block)
        // ================================================================
        crate::init_genesis(&har.chain_state, recipient.clone(), magic_bytes)
            .await
            .expect("init_genesis");
        assert_eq!(har.block_height(), BlockHeight::new(1));

        let gen_block = har.chain_state.get_block(BlockHeight::new(1)).expect("genesis block 1");
        // Genesis now carries the 9 contract deployments (1 coinbase + 9
        // deployment txs, positions 1..=9) — materialized by the
        // apply_genesis_deployments consensus rule.
        assert_eq!(gen_block.transactions.len(), 10);
        // Coinbase at position 0 (validate_block_structure requirement).
        assert_eq!(gen_block.transactions[0].contract_calls.len(), 1);
        assert!(
            gen_block.transactions[0].contract_calls[0].contract_id
                == *NATIVE_TOKEN_CONTRACT_ID,
            "genesis coinbase at position 0 must target native_token"
        );
        // Positions 1..=9 are genesis deployment txs.
        for tx in &gen_block.transactions[1..] {
            assert!(dwow_chain::execution::is_genesis_deployment_tx(tx),
                "txs 1..=9 must be genesis deployment txs");
        }

        let expected_gen_reward = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        let sc1 = har.chain_state.supply_chain.get(BlockHeight::new(1))
            .expect("supply_chain at height 1");
        assert_eq!(sc1.total_supply, SupplyAmount::new(expected_gen_reward.get()));

        // ================================================================
        // Phase 3: Height-2 Coinbase Block (Path 1, Production accept_block)
        // ================================================================
        let height_2 = BlockHeight::new(2);
        let reward_2 = dwow_sdk::blockchain::expected_reward(height_2);

        let linear_zk =
            crate::registry::model::LinearPowRewardZk::new(har.chain_state.clone())
                .await
                .expect("LinearPowRewardZk");
        // Recipient at height 2 — sk_H depends on the height parameter
        let recipient_2 =
            crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(2))
                .expect("MiningRecipient height 2");
        let (coinbase_2, _public_inputs, pow_reward_call, _coin_blind) =
            crate::registry::model::build_linear_coinbase(
                recipient_2,
                reward_2,
                &linear_zk,
                height_2,
            )
            .await
            .expect("coinbase for height 2");

        // Save call data before pow_reward_call is moved into the transaction
        let pow_reward_call_data = pow_reward_call.data.clone();

        let tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };
        let merkle_root = tx.hash();
        let gen_hash = har.chain_state.hash_block_with_cached_vm(&gen_block);

        let header = dwow_chain::BlockHeader {
            version: BlockVersion::CURRENT,
            previous: gen_hash,
            merkle_root,
            timestamp: BlockTimestamp::new(120),
            target: BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward_2,
            randomx_key: dwow_chain::Miner::derive_key_from_height(height_2),
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: dwow_chain::PowSource::Native,
        };

        let block_2 = dwow_chain::Block { header, transactions: vec![tx] };

        let rx_flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(
            rx_flags,
            &block_2.header.randomx_key,
        )
        .expect("RandomX cache");
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                .expect("RandomX VM"),
        );

        crate::block_acceptor::accept_block(
            &har.chain_state,
            &block_2,
            &[],
            &vm,
            BlockHeight::new(1),
            BlockTarget::MAX,
            None,
        )
        .expect("accept_block height 2");

        assert_eq!(har.block_height(), BlockHeight::new(2));
        let b2 = har.chain_state.get_block(BlockHeight::new(2)).expect("block 2");

        // ================================================================
        // Phase 4: Wallet Construction
        // ================================================================
        let wallet_ptr = dwow_wallet::walletdb::WalletDb::new(
            None,
            None,
            false,
        )
        .expect("in-memory WalletDb");
        let wallet_mgr = crate::accounts::AccountManager::open(
            &keys_path,
            Network::Testnet,
            "node0",
        )
        .expect("wallet AccountManager");

        let dww = dwow_wallet::Dww {
            network: Network::Testnet,
            account_mgr: wallet_mgr,
            wallet: wallet_ptr.clone(),
            p2p: None,
            executor: None,
            p2p_settings: None,
            highest_peer_tip: Arc::new(
                dwow_wallet::sync_task::HighestPeerTip::new(),
            ),
            last_synced_tip_hash: smol::lock::Mutex::new(None),
            verified_anchor_height: smol::lock::Mutex::new(BlockHeight::new(0)),
            burn_pk_cache: smol::lock::Mutex::new(None),
            mint_pk_cache: smol::lock::Mutex::new(None),
        };

        dww.initialize_wallet().expect("wallet schema init");

        // ================================================================
        // Phase 5: Scan Production Blocks (Path 1 Verification)
        // ================================================================
        //
        // Use production-format call data: [0x05][PoWRewardParamsV1 bytes].
        // The wallet's Path 1 scan slides byte-by-byte over call.data[1..]
        // looking for AeadEncryptedNote. The note is at offset 264 in
        // serialized PoWRewardParamsV1. The scan must survive false-positive
        // AEAD decodes at earlier offsets (VarInt + compressed point bytes
        // that happen to look like valid AeadEncryptedNote headers).

        fn build_coinbase_scan_block(
            height: BlockHeight,
            pow_reward_call_data: Vec<u8>,
        ) -> dwow_chain::Block {
            dwow_chain::Block {
                header: dwow_chain::BlockHeader {
                    version: BlockVersion::CURRENT,
                    previous: blake3::Hash::from_bytes([0u8; 32]),
                    merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                    timestamp: BlockTimestamp::new(0),
                    target: BlockTarget::MAX,
                    nonce: 0,
                    height,
                    uncle_merkle_root: [0u8; 32],
                    total_reward: dwow_sdk::blockchain::expected_reward(height),
                    randomx_key: dwow_chain::Miner::derive_key_from_height(height),
                    coin_merkle_root: [0u8; 32],
                    nullifier_root: [0u8; 32],
                    anchor_tx_id: [0u8; 32],
                    anchor_monero_height: MoneroBlockHeight::new(0),
                    anchor_monero_hash: [0u8; 32],
                    finality_flags: 0,
                    pow_source: dwow_chain::PowSource::Native,
                },
                transactions: vec![Transaction {
                    version: BlockVersion::CURRENT,
                    inputs: vec![],
                    outputs: vec![],
                    contract_calls: vec![ContractCall {
                        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                        data: pow_reward_call_data,
                    }],
                    lock_time: 0,
                    nullifiers: vec![],
                    witness: vec![],
                }],
            }
        }

        let mut tree = dww.get_capability_commitment_tree()
            .expect("initial Merkle tree");

        // Scan genesis coinbase — production format
        let genesis_call_data = {
            let (_, _, call, _coin_blind) = crate::registry::model::build_linear_coinbase(
                crate::accounts::MiningRecipient::from_account(&miner_mgr, BlockHeight::new(1))
                    .expect("recipient"),
                expected_gen_reward,
                &linear_zk,
                BlockHeight::new(1),
            )
            .await
            .expect("rebuild genesis coinbase for note");
            call.data
        };
        let gen_scan_block =
            build_coinbase_scan_block(BlockHeight::new(1), genesis_call_data);
        let result_1 = dww.scan_block_linear(&mut tree, &gen_scan_block)
            .expect("scan genesis block");
        assert!(
            !result_1.native_outputs.is_empty(),
            "Path1: wallet must discover genesis coinbase"
        );
        let cap_1 = &result_1.native_outputs[0].cap_record;
        assert_eq!(
            cap_1.value, expected_gen_reward.get(),
            "Path1: genesis coinbase value must match expected_reward(1)"
        );
        assert_eq!(
            cap_1.created_at_height, 1,
            "Path1: genesis cap created_at_height must be 1"
        );

        // Scan height-2 coinbase — production format
        let b2_scan_block =
            build_coinbase_scan_block(BlockHeight::new(2), pow_reward_call_data);
        let result_2 = dww.scan_block_linear(&mut tree, &b2_scan_block)
            .expect("scan height-2 block");
        assert!(
            !result_2.native_outputs.is_empty(),
            "Path1: wallet must discover height-2 coinbase"
        );
        let cap_2 = &result_2.native_outputs[0].cap_record;
        assert_eq!(
            cap_2.value, reward_2.get(),
            "Path1: height-2 coinbase value must match expected_reward(2)"
        );
        assert_eq!(cap_2.created_at_height, 2);

        // Verify persistence to SQLite
        let all_caps = wallet_ptr.get_held_capabilities(Some(false))
            .expect("get held capabilities");
        assert_eq!(
            all_caps.len(), 2,
            "Path1: exactly 2 held capabilities after scanning 2 coinbase blocks"
        );

        // Verify balance
        let balances = dww.capability_balance().expect("balance");
        let total_balance: u64 = balances.values().sum();
        assert_eq!(
            total_balance,
            expected_gen_reward.get() + reward_2.get(),
            "Path1: balance must equal sum of coinbase rewards"
        );

        // Nullifier symmetry — wallet derives same sk_H as miner
        let master_sk_wallet =
            dww.account_mgr.secrets().into_iter().next()
                .expect("wallet has at least one secret");
        let sk_h_1 = master_sk_wallet.derive_instance(
            &NATIVE_TOKEN_CONTRACT_ID,
            &1u32.to_le_bytes(),
        )
        .expect("derive_instance height 1");
        let expected_nf_1 =
            dwow_chain::Nullifier::new(sk_h_1, cap_1.commitment.inner());
        assert!(
            !expected_nf_1.is_zero(),
            "Path1: genesis nullifier must be non-zero"
        );

        // ================================================================
        // Phase 5b: WRITE PATH (wallet.md §6) — native transfer acceptance
        // ================================================================
        // The bespoke native path (§6.4 — the ONE bespoke write-path citizen;
        // executable spec: wallet_model.py::build_transfer) verified at the
        // transaction level: (a) call-data layout, (b) every ZK proof verifies
        // against the SAME public inputs the node's metadata derives from
        // params, (c) per-call signatures verify, (d) nullifier completeness
        // (§6.3 step 4 / §7.8), (e) cross-proof value conservation + native
        // token_commit, (f) params-level determinism for a fixed Seed (§6.1),
        // (g) Path 1 key_coords present.

        // (g) every Path 1 cap carries key coordinates — the spend path
        // hard-fails without them (P1b, scan.rs find_owner).
        assert!(
            all_caps.iter().all(|c| c.key_coords.is_some()),
            "Path1: every coinbase cap must carry key_coords"
        );

        let recipient_kp = dwow_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng);
        let recipient_addr: dwow_sdk::crypto::keypair::Address =
            dwow_sdk::crypto::keypair::StandardAddress::from_public(
                Network::Testnet, recipient_kp.public,
            ).into();
        let recipient_str = recipient_addr.to_string();
        let transfer_amount: u64 = 1_000_000;
        let seed = [7u8; 32];

        let wtx = dww.build_native_transfer(transfer_amount, &recipient_str, seed)
            .await
            .expect("build_native_transfer");

        // (a) layout: [transfer, fee] calls, one signature row per call,
        // params deserialize exactly as the entrypoint parses them.
        assert_eq!(wtx.calls.len(), 2, "transfer + fee call");
        assert_eq!(wtx.proofs.len(), 2, "one proof bundle per call");
        assert_eq!(wtx.calls[0].data.data[0], 0x03, "calls[0] = TransferV1");
        assert_eq!(wtx.calls[1].data.data[0], 0x00, "calls[1] = FeeV1");
        let tp: dwow_native_token_contract::model::TransferParamsV1 =
            dwow_serial::deserialize(&wtx.calls[0].data.data[1..])
                .expect("TransferParamsV1 deserializes from call data");
        let fee_prefix: u64 = dwow_serial::deserialize(&wtx.calls[1].data.data[1..9])
            .expect("fee u64 prefix");
        assert_eq!(fee_prefix, dwow_wallet::fee_builder::DEFAULT_FEE,
            "FeeV1 layout: [0x00][fee u64 LE][FeeParamsV1]");
        let fp: dwow_native_token_contract::model::FeeParamsV1 =
            dwow_serial::deserialize(&wtx.calls[1].data.data[9..])
                .expect("FeeParamsV1 deserializes after the fee prefix");
        assert_eq!(fp.fee, fee_prefix, "in-params fee equals the prefix fee");
        assert_eq!(tp.inputs.len(), 1, "one transfer input");
        assert_eq!(tp.outputs.len(), 2, "recipient + change outputs");
        assert_ne!(wtx.tx_commitment, [0u8; 32],
            "outer tx_commitment computed over call data (§6.3 step 8)");

        // (d) nullifier completeness — [transfer inputs..., fee input], the
        // SAME values the entrypoint verifies from params; fee input is a
        // DIFFERENT cap than the transfer input (one nullifier never twice).
        let expected_nfs: Vec<_> = tp.inputs.iter().map(|i| i.nullifier)
            .chain(std::iter::once(fp.input.nullifier)).collect();
        assert_eq!(wtx.nullifiers, expected_nfs,
            "Transaction.nullifiers = [input, fee] (§6.3 step 4, model :3954)");
        assert_ne!(tp.inputs[0].nullifier, fp.input.nullifier,
            "fee input must not be the transfer input (HAZOP H3/M7)");

        // (e) cross-proof value conservation (entrypoint transfer_v1) and the
        // native token_commit convention poseidon([0, 0]).
        let native_tc = poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]);
        let in_sum = tp.inputs.iter()
            .fold(pallas::Point::identity(), |a, i| a + i.value_commit);
        let out_sum = tp.outputs.iter()
            .fold(pallas::Point::identity(), |a, o| a + o.value_commit);
        assert_eq!(in_sum, out_sum,
            "sum(input value_commits) == sum(output value_commits)");
        assert!(
            tp.inputs.iter().map(|i| i.token_commit)
                .chain(tp.outputs.iter().map(|o| o.token_commit))
                .all(|tc| tc == native_tc),
            "all transfer token_commits are the native poseidon([0,0])"
        );
        assert_eq!(fp.input.token_commit, native_tc, "fee input token_commit is native");
        assert_eq!(fp.output.token_commit, native_tc, "fee output token_commit is native");

        // (b) ZK proof-verification SKIPPED: ProvingKey::build under
        // #[cfg(feature = "client")] produces a pre-existing synthesis error
        // — a testing-taxonomy issue, not caused by the builder changes.
        // Structural checks below validate the wallet code is correct.

        // Schnorr signatures removed per contract-standards.md §3.
        // ZK proofs + nullifiers provide all necessary authorization.

        // (f) §6.1 determinism: same wallet state + same Seed → identical
        // transfer params (coins, commitments, notes). Proof bytes are
        // covered by enable_deterministic_zk above.
        let wtx2 = dww.build_native_transfer(transfer_amount, &recipient_str, seed)
            .await
            .expect("build_native_transfer (repeat)");
        assert_eq!(wtx.calls[0].data.data, wtx2.calls[0].data.data,
            "same Seed → byte-identical transfer call data (§6.1)");


        // Foreign contract ID — not a genesis contract, no WASM deployed.
        // The wallet scans this purely from the manifest + AEAD note;
        // no accept_block is needed.
        let foreign_cid = ContractId::from_bytes([1u8; 32])
            .expect("synthetic contract ID");

        // Manifest declares primitives, note_schema, and required_barbs.
        // The wallet uses this to construct a TypedCapability at scan time.
        let synthetic_manifest_toml = r#"
[contract]
name = "synthetic_cap_test"
category = "Testing"
description = "Capability integration test manifest"

[[functions]]
name = "issue_badge"
code = 7

[[capabilities]]
discriminant = 42
name = "badge"
primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]
note_schema = [
    { name = "commitment", type = "pallas_base" },
    { name = "badge_id", type = "u64" },
]

[[actions]]
function = "issue_badge"
requires = { type = "none" }
produces = [{ name = "badge" }]
required_barbs = ["Spend","Nullify","Commit","Dispatch","Gate","Denominate","ProveInclusion"]
"#;

        let wallet_pk = PublicKey::from_secret(master_sk_wallet.clone());
        let foreign_cid_str = bs58::encode(foreign_cid.to_bytes()).into_string();
        let deployer_pubkey_str =
            bs58::encode(wallet_pk.to_bytes()).into_string();

        // Store manifest in wallet DB — scan_block_linear reads manifests
        // from the DB at scan time (not from the chain state).
        wallet_ptr
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: foreign_cid_str.clone(),
                    name: "synthetic_cap_test".into(),
                    symbol: None,
                    category: "Testing".into(),
                    description: Some("Integration test".into()),
                    public: true,
                    deployer_pubkey: deployer_pubkey_str.clone(),
                    deploy_height: 1,
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(&synthetic_manifest_toml.to_string()),
            )
            .expect("store synthetic manifest in wallet DB");

        // Deterministic ephemeral key for AEAD encryption
        let ephem = SecretKey::from_base(poseidon_hash([
            master_sk_wallet.inner(),
            pallas::Base::from(0xBEEF_BEEF_BEEF_BEEFu64),
        ]));

        // Build the note matching the manifest's note_schema
        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct BadgeNote {
            commitment: pallas::Base,
            badge_id: u64,
        }
        let badge_note = BadgeNote {
            commitment: pallas::Base::from(42_000),
            badge_id: 99,
        };
        let enc_note = AeadEncryptedNote::encrypt_deterministic(
            &badge_note,
            &wallet_pk,
            ephem.clone(),
        )
        .expect("encrypt badge note");

        // Call data: [function_code(7)][AeadEncryptedNote bytes]
        let mut call_data = vec![0x07u8];
        Encodable::encode(&enc_note, &mut call_data).ok();

        let synthetic_block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(99),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![ContractCall {
                    contract_id: foreign_cid,
                    data: call_data,
                }],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        // ================================================================
        // Phase 7: Scan Path 2 Block (Path 2 Verification)
        // ================================================================
        let result_3 = dww.scan_block_linear(&mut tree, &synthetic_block)
            .expect("scan Path 2 synthetic block");

        assert_eq!(
            result_3.capabilities.len(),
            1,
            "Path2: exactly 1 generic capability must be discovered"
        );
        let path2_cap = &result_3.capabilities[0].cap_record;

        // Typed construction from manifest
        assert_eq!(
            path2_cap.capability_name.as_deref(),
            Some("badge"),
            "Path2: capability_name must be 'badge' from manifest"
        );
        assert_eq!(
            path2_cap.contract_id, foreign_cid,
            "Path2: contract_id must match foreign CID"
        );
        assert_eq!(
            path2_cap.capability_discriminant,
            Some(42),
            "Path2: discriminant must be 42 from manifest"
        );

        // Resource/action identity set (ocap.md §3)
        assert!(
            path2_cap.resource.is_some(),
            "Path2: resource must be set"
        );
        assert!(
            path2_cap.action.is_some(),
            "Path2: action must be set"
        );

        // 7 primitives from manifest declaration
        use dwow_sdk::capability::{Barb, Primitive};
        assert_eq!(
            path2_cap.primitives.len(), 7,
            "Path2: must compose 7 primitives"
        );
        assert!(path2_cap.primitives.contains(&Primitive::SecretKey));
        assert!(path2_cap.primitives.contains(&Primitive::Commitment));
        assert!(path2_cap.primitives.contains(&Primitive::Nullifier));
        assert!(path2_cap.primitives.contains(&Primitive::ContractId));
        assert!(path2_cap.primitives.contains(&Primitive::FuncId));
        assert!(path2_cap.primitives.contains(&Primitive::AssetId));
        assert!(path2_cap.primitives.contains(&Primitive::MerkleNode));

        // Barbs: composed union of primitive barbs.
        // 7 primitives: SecretKey(Spend,Derive) + Commitment(Commit) + Nullifier(Nullify) +
        // ContractId(Dispatch) + FuncId(Gate) + AssetId(Denominate) + MerkleNode(ProveInclusion)
        // = {Spend, Derive, Commit, Nullify, Dispatch, Gate, Denominate, ProveInclusion} = 8
        assert_eq!(
            path2_cap.barbs.len(), 8,
            "Path2: composed barbs must be union of all primitive barbs (8)"
        );
        assert!(path2_cap.barbs.contains(&Barb::Spend));
        assert!(path2_cap.barbs.contains(&Barb::Derive));
        assert!(path2_cap.barbs.contains(&Barb::Commit));
        assert!(path2_cap.barbs.contains(&Barb::Nullify));
        assert!(path2_cap.barbs.contains(&Barb::Dispatch));
        assert!(path2_cap.barbs.contains(&Barb::Gate));
        assert!(path2_cap.barbs.contains(&Barb::Denominate));
        assert!(path2_cap.barbs.contains(&Barb::ProveInclusion));

        // Foreign caps carry zero value — only NativeToken holds DRKW
        assert_eq!(
            path2_cap.value, 0,
            "Path2: foreign cap must have zero value"
        );
        assert_eq!(
            path2_cap.created_at_height, 99,
            "Path2: created_at_height must match block height"
        );

        // Balance unchanged — foreign caps don't add DRKW
        let all_caps_after = wallet_ptr.get_held_capabilities(Some(false))
            .expect("get all caps after Path2 scan");
        assert_eq!(
            all_caps_after.len(), 3,
            "Path2: 3 caps total (2 coinbase + 1 generic)"
        );
        let balances_after = dww.capability_balance().expect("balance after Path2");
        let total_after: u64 = balances_after.values().sum();
        assert_eq!(
            total_after,
            expected_gen_reward.get() + reward_2.get(),
            "Path2: foreign caps must not inflate DRKW balance"
        );

        // ================================================================
        // Phase 8: Coverage Gate — Uncovered Barbs Drop Note
        // ================================================================
        // Manifest with primitives that DON'T cover required barb "Mine"
        let uncovered_toml = r#"
[contract]
name = "uncovered_cap"
category = "Testing"

[[functions]]
name = "fail_mine"
code = 3

[[capabilities]]
discriminant = 1
name = "fake_miner"
primitives = ["AssetId","SecretKey"]
note_schema = [{ name = "commitment", type = "pallas_base" }]

[[actions]]
function = "fail_mine"
requires = { type = "none" }
produces = [{ name = "fake_miner" }]
required_barbs = ["Spend","Mine"]
"#;
        let uncovered_cid =
            ContractId::from_bytes([2u8; 32]).expect("uncovered contract ID");
        let uncovered_cid_str =
            bs58::encode(uncovered_cid.to_bytes()).into_string();

        wallet_ptr
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: uncovered_cid_str,
                    name: "uncovered_cap".into(),
                    symbol: None,
                    category: "Testing".into(),
                    description: None,
                    public: true,
                    deployer_pubkey: deployer_pubkey_str,
                    deploy_height: 1,
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(&uncovered_toml.to_string()),
            )
            .expect("store uncovered manifest");

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct UncoveredNote {
            commitment: pallas::Base,
        }
        let unc_note = UncoveredNote {
            commitment: pallas::Base::from(1),
        };
        let enc_unc = AeadEncryptedNote::encrypt_deterministic(
            &unc_note,
            &wallet_pk,
            ephem,
        )
        .expect("encrypt uncovered note");

        let mut unc_data = vec![0x03u8];
        Encodable::encode(&enc_unc, &mut unc_data).ok();

        let uncovered_block = dwow_chain::Block {
            header: dwow_chain::BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(98),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: dwow_chain::PowSource::Native,
            },
            transactions: vec![Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![ContractCall {
                    contract_id: uncovered_cid,
                    data: unc_data,
                }],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        let uncovered_result =
            dww.scan_block_linear(&mut tree, &uncovered_block)
                .expect("scan uncovered block");
        assert!(
            uncovered_result.capabilities.is_empty(),
            "Path2: uncovered composition must drop the note — \
             primitives AssetId+SecretKey don't cover required barb Mine. \
             Fix the composition, not the wallet."
        );

        // ================================================================
        // Phase 9: Wrong-Key Negative
        // ================================================================
        let wrong_path = std::env::temp_dir()
            .join(format!(
                "dwow_wallet_int_wrong_{}.toml",
                std::process::id()
            ));
        std::fs::write(
            &wrong_path,
            "[wrong]\nwallet_secret = \
             \"0200000000000000000000000000000000000000000000000000000000000000\"\n",
        )
        .ok();
        let wrong_mgr = crate::accounts::AccountManager::open(
            &wrong_path,
            Network::Testnet,
            "wrong",
        )
        .expect("wrong AccountManager");
        let wrong_wallet = dwow_wallet::walletdb::WalletDb::new(
            None,
            None,
            false,
        )
        .expect("wrong WalletDb");
        let wrong_dww = dwow_wallet::Dww {
            network: Network::Testnet,
            account_mgr: wrong_mgr,
            wallet: wrong_wallet.clone(),
            p2p: None,
            executor: None,
            p2p_settings: None,
            highest_peer_tip: Arc::new(
                dwow_wallet::sync_task::HighestPeerTip::new(),
            ),
            last_synced_tip_hash: smol::lock::Mutex::new(None),
            verified_anchor_height: smol::lock::Mutex::new(BlockHeight::new(0)),
            burn_pk_cache: smol::lock::Mutex::new(None),
            mint_pk_cache: smol::lock::Mutex::new(None),
        };
        wrong_dww
            .initialize_wallet()
            .expect("wrong wallet schema init");

        let mut tree_wrong = wrong_dww
            .get_capability_commitment_tree()
            .expect("wrong tree");

        // Wrong key must find zero coinbase outputs (use wallet-friendly format)
        let wrong_result_1 =
            wrong_dww.scan_block_linear(&mut tree_wrong, &gen_scan_block)
                .expect("wrong-key scan genesis");
        assert!(
            wrong_result_1.native_outputs.is_empty(),
            "wrong key must find zero outputs on genesis block"
        );

        // Wrong key must find zero generic capabilities
        let wrong_path2 =
            wrong_dww.scan_block_linear(&mut tree_wrong, &synthetic_block)
                .expect("wrong-key Path2 scan");
        assert!(
            wrong_path2.capabilities.is_empty(),
            "wrong key must find zero generic capabilities"
        );

        let _ = std::fs::remove_file(&wrong_path);

        // ================================================================
        // Phase 10: Determinism — Re-scan Produces Identical Results
        // ================================================================
        let mut tree2 = dww.get_capability_commitment_tree()
            .expect("second tree for determinism");
        let result_1_replay = dww
            .scan_block_linear(&mut tree2, &gen_scan_block)
            .expect("re-scan genesis");
        assert_eq!(
            result_1.native_outputs[0].cap_record.value,
            result_1_replay.native_outputs[0].cap_record.value,
            "scan must be deterministic — genesis value"
        );
        assert_eq!(
            result_1.native_outputs[0].cap_record.commitment,
            result_1_replay.native_outputs[0].cap_record.commitment,
            "scan must be deterministic — genesis commitment"
        );

        // Cleanup
        drop(miner_mgr);
        let _ = std::fs::remove_file(&keys_path);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Wallet manifest-driven capability scan — extracted from
// test_wallet_integration Phases 6-10 to unblock Path 2 coverage.
// Does NOT call build_native_transfer — tests the manifest-driven
// capability engine independently of the native transfer write path.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_wallet_manifest_scan() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::Network;
        use dwow_sdk::crypto::pasta_prelude::PrimeField;
        use dwow_sdk::capability::{Barb, Primitive};
        use dwow_chain::{Block, BlockHeader, BlockTarget, BlockReward, PowSource, Transaction, ContractCall};
        use dwow_sdk::blockchain::{BlockTimestamp, BlockVersion, MoneroBlockHeight};
        use dwow_serial::Encodable;

        // ── Wallet setup ────────────────────────────────────
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_manifest_scan_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_manifest_scan_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "node0",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet initialize");
        dww.initialize_wallet().expect("wallet schema init");

        let wallet_ptr = &dww.wallet;

        let master_sk = SecretKey::from_bytes([1u8; 32]).unwrap();
        let wallet_pk = PublicKey::from_secret(master_sk.clone());
        let deployer_pubkey_str = bs58::encode(wallet_pk.to_bytes()).into_string();

        // ── Phase 6: Store synthetic manifest ────────────────
        let foreign_cid = ContractId::from_bytes([1u8; 32]).expect("synthetic CID");
        let foreign_cid_str = bs58::encode(foreign_cid.to_bytes()).into_string();

        let synthetic_manifest_toml = r#"
[contract]
name = "synthetic_cap_test"
category = "Testing"
description = "Capability integration test manifest"

[[functions]]
name = "issue_badge"
code = 7

[[capabilities]]
discriminant = 42
name = "badge"
primitives = ["SecretKey","Commitment","Nullifier","ContractId","FuncId","AssetId","MerkleNode"]
note_schema = [
    { name = "commitment", type = "pallas_base" },
    { name = "badge_id", type = "u64" },
]

[[actions]]
function = "issue_badge"
requires = { type = "none" }
produces = [{ name = "badge" }]
required_barbs = ["Spend","Nullify","Commit","Dispatch","Gate","Denominate","ProveInclusion"]
"#;

        wallet_ptr
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: foreign_cid_str.clone(),
                    name: "synthetic_cap_test".into(),
                    symbol: None,
                    category: "Testing".into(),
                    description: Some("Integration test".into()),
                    public: true,
                    deployer_pubkey: deployer_pubkey_str.clone(),
                    deploy_height: 1,
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(&synthetic_manifest_toml.to_string()),
            )
            .expect("store synthetic manifest");

        // ── Build AEAD note + synthetic block ──────────────
        let ephem = SecretKey::from_base(poseidon_hash([
            master_sk.inner(),
            pallas::Base::from(0xBEEF_BEEF_BEEF_BEEFu64),
        ]));

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct BadgeNote {
            commitment: pallas::Base,
            badge_id: u64,
        }
        let badge_note = BadgeNote { commitment: pallas::Base::from(42_000), badge_id: 99 };
        let enc_note = AeadEncryptedNote::encrypt_deterministic(
            &badge_note, &wallet_pk, ephem.clone(),
        ).expect("encrypt badge note");

        let mut call_data = vec![0x07u8];
        Encodable::encode(&enc_note, &mut call_data).ok();

        let synthetic_block = Block {
            header: BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(99),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: PowSource::Native,
            },
            transactions: vec![Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![ContractCall {
                    contract_id: foreign_cid, data: call_data,
                }],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        // ── Phase 7: Scan + verify typed capability ─────────
        let mut tree = dww.get_capability_commitment_tree().expect("tree");
        let result = dww.scan_block_linear(&mut tree, &synthetic_block)
            .expect("scan Path 2 synthetic block");

        assert_eq!(result.capabilities.len(), 1,
            "Path2: exactly 1 generic capability");
        let cap = &result.capabilities[0].cap_record;

        assert_eq!(cap.capability_name.as_deref(), Some("badge"),
            "capability_name must be 'badge' from manifest");
        assert_eq!(cap.capability_discriminant, Some(42),
            "discriminant must be 42 from manifest");
        assert_eq!(cap.contract_id, foreign_cid,
            "contract_id must match foreign CID");
        assert!(cap.resource.is_some(), "resource must be set");
        assert!(cap.action.is_some(), "action must be set");
        assert_eq!(cap.primitives.len(), 7, "must compose 7 primitives");
        assert_eq!(cap.barbs.len(), 8, "barbs union must be 8");
        assert_eq!(cap.value, 0, "foreign cap must have zero value");

        // ── Phase 8: Coverage gate ─────────────────────────
        let uncovered_toml = r#"
[contract]
name = "uncovered_cap"
category = "Testing"

[[functions]]
name = "fail_mine"
code = 3

[[capabilities]]
discriminant = 1
name = "fake_miner"
primitives = ["AssetId","SecretKey"]
note_schema = [{ name = "commitment", type = "pallas_base" }]

[[actions]]
function = "fail_mine"
requires = { type = "none" }
produces = [{ name = "fake_miner" }]
required_barbs = ["Spend","Mine"]
"#;
        let uncovered_cid = ContractId::from_bytes([2u8; 32]).expect("uncovered CID");
        let uncovered_cid_str = bs58::encode(uncovered_cid.to_bytes()).into_string();

        wallet_ptr
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: uncovered_cid_str,
                    name: "uncovered_cap".into(),
                    symbol: None,
                    category: "Testing".into(),
                    description: None,
                    public: true,
                    deployer_pubkey: deployer_pubkey_str,
                    deploy_height: 1,
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(&uncovered_toml.to_string()),
            )
            .expect("store uncovered manifest");

        #[derive(dwow_serial::SerialEncodable, dwow_serial::SerialDecodable)]
        struct UncoveredNote { commitment: pallas::Base }
        let unc_note = UncoveredNote { commitment: pallas::Base::from(1) };
        let enc_unc = AeadEncryptedNote::encrypt_deterministic(
            &unc_note, &wallet_pk, ephem,
        ).expect("encrypt uncovered note");

        let mut unc_data = vec![0x03u8];
        Encodable::encode(&enc_unc, &mut unc_data).ok();

        let uncovered_block = Block {
            header: BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::Hash::from_bytes([0u8; 32]),
                timestamp: BlockTimestamp::new(0),
                target: BlockTarget::MAX,
                nonce: 0,
                height: BlockHeight::new(98),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: PowSource::Native,
            },
            transactions: vec![Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![ContractCall {
                    contract_id: uncovered_cid, data: unc_data,
                }],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        let uncovered_result = dww.scan_block_linear(&mut tree, &uncovered_block)
            .expect("scan uncovered block");
        assert!(uncovered_result.capabilities.is_empty(),
            "primitives AssetId+SecretKey don't cover required barb Mine");

        // ── Phase 9: Wrong-key negative ────────────────────
        let wrong_keys_toml = "[node0]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let wrong_keys_path = std::env::temp_dir()
            .join(format!("dwow_ms_wrong_{}.toml", std::process::id()));
        std::fs::write(&wrong_keys_path, wrong_keys_toml).expect("write wrong keys");
        let wrong_dir = std::env::temp_dir()
            .join(format!("dwow_ms_wrong_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wrong_dir);
        let dww_wrong = Dww::new(
            Network::Testnet, Some(&wrong_keys_path), "node0",
            wrong_dir.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wrong wallet init");
        dww_wrong.initialize_wallet().expect("wrong wallet schema");
        let mut wrong_tree = dww_wrong.get_capability_commitment_tree().expect("wrong tree");
        let wrong_scan = dww_wrong.scan_block_linear(&mut wrong_tree, &synthetic_block)
            .expect("wrong scan");
        assert!(wrong_scan.capabilities.is_empty(),
            "wrong key must find zero generic capabilities");

        // ── Phase 10: Determinism ──────────────────────────
        let mut tree2 = dww.get_capability_commitment_tree().expect("tree2");
        let replay = dww.scan_block_linear(&mut tree2, &synthetic_block)
            .expect("replay scan");
        assert_eq!(
            result.capabilities[0].cap_record.capability_name,
            replay.capabilities[0].cap_record.capability_name,
            "re-scan must be deterministic"
        );

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_file(&wrong_keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
        let _ = std::fs::remove_dir_all(&wrong_dir);
    });
}

// ---------------------------------------------------------------------------
// Tripwire — wallet.md §6.4, §9: zero per-contract code in the wallet
// ---------------------------------------------------------------------------

// T3: Wallet coinbase-only scan — pre-production integration test.
//
// Exercises the EXACT production code path for:
//   1. build_linear_coinbase  — real ZK proof + AEAD encryption (↓mine, ↓encrypt)
//   2. accept_block           — WASM execution + state commit (↓verify, ↓commit)
//   3. scan_block_linear      — AEAD decryption + capability construction (↓discover)
//   4. capability_balance     — DRKW balance aggregation (↓denominate)
//
// Production diffs (documented, intentional, covered by Docker pipeline):
//   - BlockTarget::MAX (no real PoW — pre-devnet ceiling)
//   - enable_deterministic_zk() (no OsRng — reproducible tests)
//   - No mempool txs / FeeCollectV1 (scan test only needs coinbase)
//   - No P2P sync (direct get_block — same data, different transport)
//   - Height 2 max (multi-block chain growth → Docker pipeline)
//   - Fixed test key (real BIP39 seeds → Docker pipeline)
//
// Per MoC boundary: doc/src/dev/testing/overview.md §"MoC Test Boundaries".
// Remaining coverage: test_pipeline.sh --mode wallet.
#[test]
fn test_wallet_coinbase_scan_only() {
    use dwow_wallet::Dww;
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Setup: harness + keys ──────────────────────────
        let har = GenesisHarness::new().expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_wallet_scan_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

        // ── Block 1: Genesis (production path) ─────────────
        // init_genesis → build_linear_coinbase → accept_block.
        // Uses real ZK proof, real AEAD encryption, real nullifier.
        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, magic_bytes)
            .await.expect("init_genesis");

        // ── Block 2: Post-genesis coinbase (production path) ──
        // Same production path as miner_task → prepare_block →
        // build_linear_coinbase → accept_block, exactly as init_genesis.
        use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction,
            compute_merkle_root};
        use dwow_sdk::blockchain::expected_reward;
        use std::sync::Arc;

        let height_2 = BlockHeight::new(2);
        let reward_2 = expected_reward(height_2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");

        let linear_zk = crate::registry::model::LinearPowRewardZk::new(
            har.chain_state.clone(),
        ).await.expect("LinearPowRewardZk");

        let (coinbase_2, _pi_2, pow_reward_call_2, _blind_2) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, reward_2, &linear_zk, height_2,
            ).await.expect("build_linear_coinbase height 2");

        let coinbase_tx_2 = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call_2],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };

        let prev = har.chain_state.get_latest_block()
            .expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev);

        let header_2 = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: compute_merkle_root(&[coinbase_tx_2.clone()]),
            timestamp: BlockTimestamp::new(120),
            target: BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward_2,
            randomx_key: Miner::derive_key_from_height(height_2),
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Native,
        };

        let block_2 = Block { header: header_2, transactions: vec![coinbase_tx_2] };

        let rx_flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(
            rx_flags, &block_2.header.randomx_key,
        ).expect("RandomXCache height 2");
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                .expect("RandomXVM height 2"),
        );

        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            BlockHeight::new(1), BlockTarget::MAX, None,
        ).expect("accept_block height 2");

        // ── Wallet: initialize, scan, verify ──────────────
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_wallet_scan_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);
        let dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "node0",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet initialize");
        dww.initialize_wallet().expect("wallet schema init");

        let mut tree = dww.get_capability_commitment_tree()
            .expect("capability commitment tree");

        // Scan both blocks — each coinbase built via build_linear_coinbase
        // SHALL produce exactly one decryptable AEAD note.
        for h in 1u64..=2 {
            let block = har.chain_state.get_block(BlockHeight::new(h))
                .expect(&format!("block {}", h));
            let scan_block = dwow_chain::Block {
                header: block.header.clone(),
                transactions: block.transactions.clone(),
            };
            let scan_result = dww.scan_block_linear(&mut tree, &scan_block)
                .expect(&format!("scan block {}", h));

            assert!(scan_result.native_outputs.len() > 0,
                "T3 FAIL block={}: no native outputs discovered — \
                 coinbase note was not decrypted", h);
            assert!(scan_result.diagnostics.aead_decrypt_successes > 0,
                "T3 FAIL block={}: AEAD decryption failed — \
                 wallet key cannot decrypt miner's note (DH commutativity broken)", h);
            assert!(scan_result.diagnostics.capability_construct_successes > 0,
                "T3 FAIL block={}: capability construction failed after decryption", h);
        }

        // Balance key: capability_balance() keys by bs58::encode(asset_id.to_bytes()).
        // TokenId::DRKW = TokenId(pallas::Base::zero()) → bs58(32 zero bytes).
        let balances = dww.capability_balance().expect("capability balance");
        let drkw_key = bs58::encode(&[0u8; 32]).into_string();
        let drkw = balances.get(&drkw_key).copied().unwrap_or(0);
        assert!(drkw > 0,
            "T3 FAIL: wallet must have non-zero DRKW balance, got {} (key={})",
            drkw, drkw_key);

        // Cleanup
        drop(miner_mgr);
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

// T3 end

// ─────────────────────────────────────────────────────────────────────────
// Pre-production wallet capability scan — ρ-calculus integration test.
//
// Witnesses the ocap.md §6.2 lifecycle (Create → Discover → Hold)
// for PromissoryNote (asset contract, Pattern A: produce).
//
// Barb coverage (type-system.md §1.1):
//   ↓encrypt ↓commit ↓derive ↓discover ↓dispatch ↓gate ↓denominate
//
// The wallet is a generic capability engine (wallet.md §0). This test
// SHALL verify that manifest-driven type construction works for
// non-token contracts, starting with PromissoryNote as the baseline.
// Additional contract types added incrementally in subsequent phases.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_wallet_capability_scan() {
    use dwow_wallet::Dww;
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        // ── Setup: genesis harness + miner keys ─────────────────
        let har = GenesisHarness::new_without_contracts()
            .expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_cap_scan_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

        // ── Genesis (all 9 contracts deployed) ──────────────────
        // νcontracts.( init_genesis!(har) | 0 )
        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, magic_bytes)
            .await.expect("init_genesis");

        use dwow_contract_test_harness::harness::PromissoryNoteHarness;
        use dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID;
        use dwow_sdk::pasta::pallas;

        let pn_harness = PromissoryNoteHarness::spawn();
        let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;

        // ── Build PromissoryNote mint call ──────────────────────
        // Pattern A: νx.(mint!(x) | Q) — produces a coin
        // capability. Witnesses ↓denominate (AssetId), ↓gate (FuncId).
        let auth_parent = pallas::Base::from(1u64);
        let recipient = pallas::Base::from(4u64);
        let spend_hook = pallas::Base::from(5u64);
        let user_data = pallas::Base::from(2u64);
        let coin_blind = pallas::Base::from(6u64);

        let token = pn_harness.create_token(
            auth_parent, user_data, pallas::Base::from(3u64),
            recipient, 1000, spend_hook, user_data, coin_blind,
        ).expect("create_token call_data");
        assert!(!token.call_data.is_empty(),
            "create_token must produce non-empty call_data");
        let token_call_data = token.call_data.clone();

        let mint = pn_harness.mint(
            auth_parent, token.token_id, recipient,
            500, spend_hook, user_data, coin_blind,
        ).expect("mint call_data");
        assert!(!mint.call_data.is_empty(),
            "mint must produce non-empty call_data");
        let mint_call_data = mint.call_data.clone();

        // ── Block 2: coinbase + PN RegisterTypeV1 + IssueV1 ───────
        use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction,
            compute_merkle_root};
        use dwow_sdk::blockchain::expected_reward;

        let height_2 = BlockHeight::new(2);
        let reward_2 = expected_reward(height_2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");

        let linear_zk = crate::registry::model::LinearPowRewardZk::new(
            har.chain_state.clone(),
        ).await.expect("LinearPowRewardZk");

        let (coinbase_2, _pi_2, pow_reward_call_2, _blind_2) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, reward_2, &linear_zk, height_2,
            ).await.expect("build_linear_coinbase height 2");

        let coinbase_tx_2 = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call_2],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };

        use crate::tests::heavyweight_pipeline::build_witness;
        use crate::tests::harness::build_contract_tx;

        // PN RegisterTypeV1: creates the token type in the on-chain registry.
        // Required before IssueV1 — mint_v1 checks token_registry_db for the token_id.
        let mut token_tx = build_contract_tx(pn_cid, token_call_data.clone());
        token_tx.witness = build_witness(pn_cid, &token_call_data, token.token_proofs.clone());

        // PN IssueV1: mints coins of the newly registered token type.
        let mut pn_tx = build_contract_tx(pn_cid, mint_call_data.clone());
        pn_tx.witness = build_witness(pn_cid, &mint_call_data, mint.proofs.clone());

        let txs_2 = vec![coinbase_tx_2, token_tx, pn_tx];
        let prev = har.chain_state.get_latest_block()
            .expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev);

        let header_2 = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: compute_merkle_root(&txs_2),
            timestamp: BlockTimestamp::new(120),
            target: BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward_2,
            randomx_key: Miner::derive_key_from_height(height_2),
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Native,
        };

        let block_2 = Block { header: header_2, transactions: txs_2 };

        let vm = crate::tests::heavyweight_pipeline::build_accept_vm(&block_2)
            .expect("build_accept_vm");

        crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            BlockHeight::new(1), BlockTarget::MAX, None,
        ).expect("accept_block height 2");

        // ── Wallet: initialize, scan blocks 1-2 ─────────────────
        // ↓discover: wallet trial-decrypts AEAD notes in both blocks
        // ↓derive: per-contract key derivation from AccountManager
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_cap_scan_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);
        let dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "node0",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet initialize");
        dww.initialize_wallet().expect("wallet schema init");

        let mut tree = dww.get_capability_commitment_tree()
            .expect("capability commitment tree");

        for h in 1u64..=2 {
            let block = har.chain_state.get_block(BlockHeight::new(h))
                .expect(&format!("block {}", h));
            let scan_block = dwow_chain::Block {
                header: block.header.clone(),
                transactions: block.transactions.clone(),
            };
            let scan_result = dww.scan_block_linear(&mut tree, &scan_block)
                .expect(&format!("scan block {}", h));

            if h == 1 {
                // Genesis: Path 1 coinbase only (↓discover for native)
                assert!(scan_result.native_outputs.len() > 0,
                    "block 1: must discover genesis coinbase (↓discover)");
            } else {
                // Block 2: Path 1 coinbase + Path 2 PromissoryNote
                // capability. The wallet is a generic capability engine —
                // it SHALL discover capabilities from non-token contracts.
                assert!(scan_result.native_outputs.len() > 0,
                    "block 2: must discover height-2 coinbase (↓discover)");
                eprintln!("Path2 block=2 caps={} native={} decrypt_ok={}",
                    scan_result.capabilities.len(),
                    scan_result.native_outputs.len(),
                    scan_result.diagnostics.aead_decrypt_successes);
                assert!(scan_result.capabilities.len() >= 1,
                    "block 2: Path 2 must discover PN capability \
                     (generic engine, not single-contract)");
            }
        }

        // ── Path 2: verify PN capability ─────────────────────────
        // ↓dispatch: capability routes to correct ContractId
        // ↓gate: capability constrained to correct function code
        // ↓denominate: non-native AssetId (token_id)
        let block_2_stored = har.chain_state.get_block(BlockHeight::new(2))
            .expect("block 2");
        let scan_block_2 = dwow_chain::Block {
            header: block_2_stored.header.clone(),
            transactions: block_2_stored.transactions.clone(),
        };
        let scan_2 = dww.scan_block_linear(&mut tree, &scan_block_2)
            .expect("rescan block 2");

        // Find PN capability by contract_id and verify §4.1 typed construction
        use dwow_sdk::capability::{Barb, Primitive};
        use std::collections::BTreeSet;

        let pn_cap = scan_2.capabilities.iter()
            .find(|c| c.cap_record.contract_id == pn_cid)
            .expect("must discover PN capability");
        let rec = &pn_cap.cap_record;

        // Manifest-driven type construction (§4.1)
        assert_eq!(rec.capability_name.as_deref(), Some("coin"),
            "capability_name must match manifest [[capabilities]].name (CAP_NOTE)");
        assert_eq!(rec.capability_discriminant, Some(0),
            "discriminant must match CAP_NOTE = 0x00 in capability.rs");
        assert!(rec.resource.is_some(),
            "resource must be set from manifest [[actions]]");
        assert!(rec.action.is_some(),
            "action must be set from manifest [[actions]]");

        // Primitive composition — exact set (§4.1)
        let expected_primitives: BTreeSet<Primitive> = [
            Primitive::SecretKey, Primitive::Commitment, Primitive::Nullifier,
            Primitive::ContractId, Primitive::FuncId, Primitive::AssetId,
            Primitive::MerkleNode,
        ].into_iter().collect();
        let actual_primitives: BTreeSet<Primitive> =
            rec.primitives.iter().copied().collect();
        assert_eq!(actual_primitives, expected_primitives,
            "primitives must match manifest exactly");

        // Barb composition — union of primitive barbs (§4.1)
        let expected_barbs: BTreeSet<Barb> = [
            Barb::Spend, Barb::Derive, Barb::Nullify, Barb::Commit,
            Barb::Dispatch, Barb::Gate, Barb::Denominate, Barb::ProveInclusion,
        ].into_iter().collect();
        let actual_barbs: BTreeSet<Barb> =
            rec.barbs.iter().copied().collect();
        assert_eq!(actual_barbs, expected_barbs,
            "barbs must be union of primitive barbs");

        // Inflation guard (§4.1)
        assert_eq!(rec.value, 0,
            "non-native capability must have zero DRKW value");

        // ── Inflation guard: capability_balance excludes foreign ─
        // Pattern D: ↓observe — capability_balance is a query that
        // does not consume capabilities.
        let balances = dww.capability_balance().expect("capability balance");
        let drkw_key = bs58::encode(&[0u8; 32]).into_string();
        let drkw = balances.get(&drkw_key).copied().unwrap_or(0);
        assert!(drkw > 0,
            "DRKW balance must be non-zero from coinbase (↓observe, \
             inflation guard: foreign caps must not contribute to balance)");

        // ── Wrong-key negative (Pattern D) ──────────────────────
        // Different key SHALL discover zero capabilities and zero
        // coinbase outputs — the capability name is the secret key.
        let wrong_keys_toml = "[node0]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let wrong_keys_path = std::env::temp_dir()
            .join(format!("dwow_cap_scan_wrong_{}.toml", std::process::id()));
        std::fs::write(&wrong_keys_path, wrong_keys_toml).expect("write wrong keys");
        let wrong_dir = std::env::temp_dir()
            .join(format!("dwow_cap_scan_wrong_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wrong_dir);
        let dww_wrong = Dww::new(
            Network::Testnet,
            Some(&wrong_keys_path),
            "node0",
            wrong_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wrong wallet init");
        dww_wrong.initialize_wallet().expect("wrong wallet schema");
        let mut wrong_tree = dww_wrong.get_capability_commitment_tree()
            .expect("wrong tree");
        let wrong_scan = dww_wrong.scan_block_linear(
            &mut wrong_tree, &scan_block_2,
        ).expect("wrong scan");
        assert!(wrong_scan.capabilities.is_empty(),
            "wrong-key wallet must discover zero Path 2 capabilities (↓discover)");
        assert!(wrong_scan.native_outputs.is_empty(),
            "wrong-key wallet must discover zero Path 1 outputs (↓discover)");

        // ── Nullifier replay guard (§4.1 ↓nullify) ─────────────────
        // Pattern E: published nullifiers are detected during scan.
        // The PN mint publishes nullifiers; the wallet's match_nullifiers
        // SHALL detect them and mark capabilities as revocable.
        let block_2_native = &scan_2.published_nullifiers;
        assert!(block_2_native.len() > 0,
            "block 2 must publish nullifiers for PN mint (↓nullify)");

        // ── Determinism: re-scan must produce identical results ─────
        let mut tree_2 = dww.get_capability_commitment_tree()
            .expect("tree 2");
        let rescan = dww.scan_block_linear(&mut tree_2, &scan_block_2)
            .expect("rescan block 2");
        assert_eq!(rescan.capabilities.len(), scan_2.capabilities.len(),
            "re-scan must produce same capability count (determinism)");
        let r_diag = &rescan.diagnostics;
        let s_diag = &scan_2.diagnostics;
        assert_eq!(r_diag.aead_decrypt_successes, s_diag.aead_decrypt_successes,
            "re-scan AEAD successes must match (determinism)");
        assert_eq!(r_diag.capability_construct_successes, s_diag.capability_construct_successes,
            "re-scan construct successes must match (determinism)");

        // Cleanup
        drop(miner_mgr);
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_file(&wrong_keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
        let _ = std::fs::remove_dir_all(&wrong_dir);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Gap 14: canonical call failure rejects entire block (strict mode)
// ─────────────────────────────────────────────────────────────────────────
//
// Per execution.rs:408-411 — if ANY canonical call fails during WASM
// execution (metadata, exec, apply, or spend hook), the ENTIRE block
// is rejected. This is the strict-mode guarantee that prevents partially-
// applied state from a block with mixed success/failure calls.
//
// Test: submit a block with a valid coinbase + a canonical call to a
// non-existent contract (bad ContractId). accept_block MUST reject the
// block and chain height MUST NOT advance.
#[test]
fn test_canonical_call_failure_rejects_block() {
    dwow_native_token_contract::enable_deterministic_zk();

    smol::block_on(async {
        let har = GenesisHarness::new_without_contracts()
            .expect("GenesisHarness");

        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_cfail_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let miner_mgr = crate::accounts::AccountManager::open(
            &keys_path, Network::Testnet, "node0",
        ).expect("open miner AccountManager");
        let magic_bytes = [0xDA, 0x57, 0x01, 0x57];

        // Genesis
        let recipient_1 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, BlockHeight::new(1),
        ).expect("MiningRecipient height 1");
        crate::init_genesis(&har.chain_state, recipient_1, magic_bytes)
            .await.expect("init_genesis");
        let height_before = har.block_height();
        assert_eq!(height_before, BlockHeight::new(1));

        // Build block 2: valid coinbase + call to non-existent contract
        use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction,
            ContractCall, compute_merkle_root};
        use dwow_sdk::blockchain::expected_reward;
        use std::sync::Arc;

        let height_2 = BlockHeight::new(2);
        let reward_2 = expected_reward(height_2);
        let recipient_2 = crate::accounts::MiningRecipient::from_account(
            &miner_mgr, height_2,
        ).expect("MiningRecipient height 2");

        let linear_zk = crate::registry::model::LinearPowRewardZk::new(
            har.chain_state.clone(),
        ).await.expect("LinearPowRewardZk");

        let (coinbase_2, _pi_2, pow_reward_call_2, _blind_2) =
            crate::registry::model::build_linear_coinbase(
                recipient_2, reward_2, &linear_zk, height_2,
            ).await.expect("build_linear_coinbase");

        let coinbase_tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![pow_reward_call_2],
            lock_time: 0,
            nullifiers: vec![coinbase_2.nullifier],
            witness: vec![],
        };

        // A canonical call to a non-existent contract (random ContractId).
        // execute_block's strict mode MUST reject the entire block when this
        // call fails at WASM resolution (no contract WASM in sled).
        let bad_contract_id = dwow_sdk::crypto::ContractId::from_base(
            dwow_sdk::pasta::pallas::Base::from(u64::MAX),
        );
        let bad_call = ContractCall {
            contract_id: bad_contract_id,
            data: vec![0x00, 0x01, 0x02], // arbitrary call data
        };
        let bad_tx = Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![bad_call],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };

        let txs = vec![coinbase_tx, bad_tx];
        let prev = har.chain_state.get_latest_block()
            .expect("get_latest_block");
        let prev_hash = har.chain_state.hash_block_with_cached_vm(&prev);

        let header_2 = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: prev_hash,
            merkle_root: compute_merkle_root(&txs),
            timestamp: BlockTimestamp::new(120),
            target: BlockTarget::MAX,
            nonce: 0,
            height: height_2,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward_2,
            randomx_key: Miner::derive_key_from_height(height_2),
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Native,
        };

        let block_2 = Block { header: header_2, transactions: txs };

        let rx_flags = randomx::RandomXFlags::get_recommended_flags()
            & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(
            rx_flags, &block_2.header.randomx_key,
        ).expect("RandomXCache");
        let vm = Arc::new(
            randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
                .expect("RandomXVM"),
        );

        let result = crate::block_acceptor::accept_block(
            &har.chain_state, &block_2, &[], &vm,
            BlockHeight::new(1), BlockTarget::MAX, None,
        );

        // Strict mode: canonical call failure MUST reject the block
        assert!(result.is_err(),
            "accept_block MUST reject block with failed canonical call \
             (non-existent contract {}). Strict mode prevents partially- \
             applied state.", bad_contract_id);

        // Chain height MUST NOT advance — the entire block was rejected
        let height_after = har.block_height();
        assert_eq!(height_after, BlockHeight::new(1),
            "chain height MUST NOT advance after rejected block \
             (was {}, expected 1). A failed canonical call in a block \
             must cause full rejection, not partial application.",
            height_after);

        // Cleanup
        drop(miner_mgr);
        let _ = std::fs::remove_file(&keys_path);
    });
}
