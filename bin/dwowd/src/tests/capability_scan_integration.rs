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

//! Wallet capability-scan integration test — real L1 o-cap contract.
//!
//! Validates the Path 2 (manifest-driven) scan end-to-end against the REAL
//! Promissory Note contract: the real on-chain manifest (`manifest.toml`) and
//! the real `PromissoryNote` note type. This is the "no coins" L1 redemption
//! capability path — the note carries `value`, `asset_id`, and `commitment`
//! (the CapCommitment Merkle leaf), and the scan SHALL read all three from the
//! note_schema, NOT hardcode value=0 / asset_id=DRKW (wallet.md §2.3).
//!
//! Box/Purse note emission is a follow-on (their client was removed as phantom
//! wallet-grammar; the harness uses hardcoded field elements, not wallet keys).

use dwow_sdk::pasta::pallas;

/// The real Promissory Note note type and manifest, plus a real AEAD note
/// encrypted to the wallet's key, are discovered with their real value,
/// asset_id, and commitment — not collapsed to a DRKW zero.
#[test]
fn test_promissory_note_capability_scan() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::{Network, PublicKey, SecretKey};
        use dwow_sdk::crypto::{poseidon_hash, PROMISSORY_NOTE_CONTRACT_ID};
        use dwow_sdk::crypto::note::AeadEncryptedNote;
        use dwow_sdk::blockchain::{BlockHeight, BlockTimestamp, BlockVersion, MoneroBlockHeight};
        use dwow_chain::{Block, BlockHeader, BlockTarget, BlockReward, PowSource, Transaction, ContractCall, Commitment};
        use dwow_serial::Encodable;
        use dwow_sdk::crypto::AssetId;

        // ── Wallet identity: field element 1 (hex 0100…00) ────────────────
        let keys_toml = "[node0]\nwallet_secret = \
            \"0100000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_pn_scan_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_pn_scan_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "node0",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet init");
        dww.initialize_wallet().expect("wallet schema init");

        let wallet_ptr = &dww.wallet;

        let master_sk = SecretKey::from_bytes({
            let mut b = [0u8; 32];
            b[0] = 0x01;
            b
        }).unwrap();
        let wallet_pk = PublicKey::from_secret(master_sk.clone());
        let deployer_key = bs58::encode(wallet_pk.to_bytes()).into_string();

        // ── Store the REAL PN manifest (as Deployooor scan would) ────────
        let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;
        let pn_cid_str = bs58::encode(pn_cid.to_bytes()).into_string();
        let manifest_toml = include_str!("../../../../src/contract/promissory_note/manifest.toml");

        wallet_ptr
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: pn_cid_str.clone(),
                    name: "promissory_note".into(),
                    symbol: None,
                    category: "Token".into(),
                    description: Some("Real PN manifest".into()),
                    public: true,
                    deployer_pubkey: deployer_key.clone(),
                    deploy_height: BlockHeight::new(1),
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(manifest_toml),
            )
            .expect("store PN manifest");

        // ── Build a real PromissoryNote (transfer 0x04 output) ────────────
        // The note carries value, asset_id, spend_hook, user_data, commitment_blind,
        // value_blind, token_blind, memo, and commitment (the CapCommitment).
        let value: u64 = 1_000_000;
        let asset_id = pallas::Base::from(777u64);
        let commitment = pallas::Base::from(999u64);

        let note = dwow_promissory_note_contract::client::PromissoryNote {
            value,
            asset_id,
            spend_hook: pallas::Base::from(0u64),
            user_data: pallas::Base::from(0u64),
            commitment_blind: pallas::Base::from(11u64),
            value_blind: pallas::Scalar::from(0u64),
            token_blind: pallas::Base::from(12u64),
            memo: vec![],
            commitment,
        };

        let ephem = SecretKey::from_base(poseidon_hash([
            *master_sk.inner(),
            pallas::Base::from(0xCAFE_CAFE_CAFE_CAFEu64),
        ]));
        let encrypted = AeadEncryptedNote::encrypt_deterministic(&note, &wallet_pk, ephem)
            .expect("encrypt PN note");

        // fn_code 0x04 = transfer; manifest [[actions]].transfer produces the
        // "note" capability whose note_schema carries the commitment leaf.
        let mut call_data = vec![0x04u8];
        Encodable::encode(&encrypted, &mut call_data).ok();

        // ── Synthetic block with the PN transfer call ─────────────────────
        let block = Block {
            header: BlockHeader {
                fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
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
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
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
                contract_calls: vec![
                    ContractCall { contract_id: pn_cid, data: call_data },
                ],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        // ── Scan + assert non-DRKW discovery ──────────────────────────────
        let mut tree = dww.get_capability_commitment_tree()
            .expect("capability commitment tree");
        let result = dww.scan_block_linear(&mut tree, &block)
            .expect("scan PN block");

        assert_eq!(result.capabilities.len(), 1,
            "exactly 1 promissory note capability discovered");
        let cap = &result.capabilities[0];
        let rec = &cap.cap_record;

        assert_eq!(rec.contract_id, pn_cid, "discovered from the PN contract");
        assert_eq!(rec.capability_name.as_deref(), Some("note"),
            "capability renamed from 'coin' to 'note' (no-coins model)");

        // The core regression: value and asset denomination come from the note,
        // NOT hardcoded DRKW/0 (wallet.md §2.3, scan.rs Path 2).
        assert_eq!(rec.value, value,
            "promissory note value read from the note (not hardcoded 0)");
        assert_eq!(rec.asset_id, AssetId::from_base(asset_id),
            "asset_id read from the note (not hardcoded DRKW)");
        assert_eq!(rec.commitment, Commitment::from_base(commitment),
            "commitment (CapCommitment leaf) read from the note");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// Box capability send + receive: a sender puts a `box_capability` (a
/// produce-side note `{ commitment, state_nonce }`) encrypted to the recipient's
/// default address, and the recipient wallet discovers it via the
/// manifest-driven Path 2 scan — Box has no per-contract wallet client
/// (removed as phantom code), so the "send" is the produce-side note emitted by
/// the sender.
#[test]
fn test_box_send_receive() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::{Network, PublicKey};
        use dwow_sdk::crypto::{poseidon_hash, BOX_CONTRACT_ID};
        use dwow_sdk::crypto::note::AeadEncryptedNote;
        use dwow_sdk::blockchain::{BlockHeight, BlockTimestamp, BlockVersion, MoneroBlockHeight};
        use dwow_chain::{Block, BlockHeader, BlockTarget, BlockReward, PowSource, Transaction, ContractCall, Commitment};
        use dwow_serial::Encodable;
        use dwow_sdk::pasta::{group::ff::PrimeField, pallas};

        // ── Recipient wallet: field element 2 (hex 0200…00) ───────────────
        let keys_toml = "[wallet2]\nwallet_secret = \
            \"0200000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_box_sendrecv_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");

        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_box_sendrecv_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "wallet2",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet init");
        dww.initialize_wallet().expect("wallet schema init");

        let wallet_ptr = &dww.wallet;

        // The recipient's default address — the sender encrypts the note to its
        // public key (mirrors build_native_transfer's Address::public_key()).
        let addr2 = dww.default_address().expect("recipient address");
        let recipient_pk = *addr2.public_key();
        let deployer_key = bs58::encode(recipient_pk.to_bytes()).into_string();

        // ── Store the REAL Box manifest ────────────────────────────────────
        let box_cid = *BOX_CONTRACT_ID;
        let box_cid_str = bs58::encode(box_cid.to_bytes()).into_string();
        let manifest_toml = include_str!("../../../../src/contract/box/manifest.toml");

        wallet_ptr
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: box_cid_str.clone(),
                    name: "box".into(),
                    symbol: None,
                    category: "Infrastructure".into(),
                    description: Some("Real Box manifest".into()),
                    public: true,
                    deployer_pubkey: deployer_key.clone(),
                    deploy_height: BlockHeight::new(1),
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(manifest_toml),
            )
            .expect("store Box manifest");

        // ── Sender builds the produce-side box_capability note (put) ───────
        // Same field derivation as test-harness/src/harness/box.rs::put: the note
        // carries { commitment = poseidon(dml, bid, ncc, nsn), state_nonce = nsn }.
        let dml = pallas::Base::from(5u64);
        let bid = pallas::Base::from(1u64);
        let ncc = poseidon_hash([pallas::Base::from(100u64)]);
        let nsn = pallas::Base::from(1u64);
        let nl = poseidon_hash([dml, bid, ncc, nsn]);

        #[derive(dwow_serial::SerialEncodable)]
        struct BoxNote { commitment: pallas::Base, state_nonce: pallas::Base }
        let note = BoxNote { commitment: nl, state_nonce: nsn };
        let encrypted = AeadEncryptedNote::encrypt(&note, &recipient_pk, &mut rand::rngs::OsRng)
            .expect("encrypt Box note to recipient");

        // fn_code 0x01 = put; the scan byte-slides over call.data for the note.
        let mut call_data = vec![0x01u8];
        Encodable::encode(&encrypted, &mut call_data).ok();

        // ── Synthetic block with the put call ──────────────────────────────
        let block = Block {
            header: BlockHeader {
                fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
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
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
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
                contract_calls: vec![
                    ContractCall { contract_id: box_cid, data: call_data },
                ],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        // ── Recipient scans + asserts box_capability receipt ───────────────
        let mut tree = dww.get_capability_commitment_tree()
            .expect("capability commitment tree");
        let result = dww.scan_block_linear(&mut tree, &block)
            .expect("scan Box block");

        assert_eq!(result.capabilities.len(), 1,
            "recipient must discover exactly 1 box_capability");
        let rec = &result.capabilities[0].cap_record;

        assert_eq!(rec.contract_id, box_cid, "discovered from the Box contract");
        assert_eq!(rec.capability_name.as_deref(), Some("box_capability"),
            "capability name from the Box manifest");
        assert_eq!(rec.capability_discriminant, Some(0),
            "box_capability discriminant from the manifest");
        assert_eq!(rec.commitment, Commitment::from_base(nl),
            "commitment (box new leaf) read from the note");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// A real Box `put` is submitted through `accept_block` (the write-path
/// validation gate `box_roots` check), and the recipient wallet discovers the
/// `box_capability` from the accepted block — combining the on-chain acceptance
/// that `box_spec.rs` covers with the wallet scan that `test_box_send_receive`
/// covers only against a synthetic block (l1-capability-write-path-spec.md §5 test (2)).
#[test]
fn test_box_put_accepts_through_accept_block() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::{Network};
        use dwow_sdk::crypto::{poseidon_hash, BOX_CONTRACT_ID, MerkleNode, MerkleTree};
        use dwow_sdk::blockchain::BlockHeight;
        use dwow_sdk::pasta::pallas;
        use dwow_chain::Commitment;
        use dwow_contract_test_harness::harness::{BoxHarness, ContractHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // ── Real chain: genesis + submit a Box put through accept_block ────
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = BoxHarness::spawn();
        let put = harness.put().expect("box put");
        let put_height = chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &put.call_data, vec![put.proof])
            .expect("with_call")
            .submit().await
            .expect("submit box put");

        // ── On-chain acceptance gate: the new leaf root is in box_roots ───
        // Mirrors box_spec.rs verify_state: nl = poseidon_hash([dml=5, bid=1,
        // ncc, nsn=1]) with ncc = poseidon_hash([100]), tree = [ZERO, nl].
        let ncc = poseidon_hash([pallas::Base::from(100u64)]);
        let nl = poseidon_hash([
            pallas::Base::from(5u64), pallas::Base::from(1u64), ncc, pallas::Base::from(1u64),
        ]);
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(nl));
        let expected_root = tree.root(0).expect("tree.root").to_bytes().to_vec();
        let in_roots = chain.query_contract_state(*BOX_CONTRACT_ID, "box_roots", &expected_root)
            .expect("query box_roots");
        assert!(in_roots.is_some(),
            "box put must append new_leaf to box_roots (on-chain acceptance)");

        // ── Recipient wallet (owner secret 42 = BoxHarness os) discovers ──
        // The BoxHarness put() encrypts the produce-side note to
        // PublicKey::from_secret(SecretKey::from_base(42)), so the recipient
        // wallet is keyed to secret 42 to trial-decrypt it.
        let keys_toml = "[boxowner]\nwallet_secret = \
            \"2a00000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_box_accept_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_box_accept_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet,
            Some(&keys_path),
            "boxowner",
            wallet_dir.to_string_lossy().to_string(),
            "".to_string(),
            false,
            None,
        ).expect("wallet init");
        dww.initialize_wallet().expect("wallet schema init");

        let addr = dww.default_address().expect("recipient address");
        let deployer_key = bs58::encode(addr.public_key().to_bytes()).into_string();

        // Store the REAL Box manifest (Path 2 scan), as test_box_send_receive.
        let box_cid = *BOX_CONTRACT_ID;
        let box_cid_str = bs58::encode(box_cid.to_bytes()).into_string();
        let manifest_toml = include_str!("../../../../src/contract/box/manifest.toml");
        dww.wallet
            .insert_contract_metadata_with_manifest(
                &dwow_wallet::walletdb::ContractMetadataRecord {
                    contract_id: box_cid_str.clone(),
                    name: "box".into(),
                    symbol: None,
                    category: "Infrastructure".into(),
                    description: Some("Real Box manifest".into()),
                    public: true,
                    deployer_pubkey: deployer_key.clone(),
                    deploy_height: BlockHeight::new(1),
                    attestations_json: "[]".into(),
                    lock_status: "unlocked".into(),
                },
                Some(manifest_toml),
            )
            .expect("store Box manifest");

        // Scan the ACCEPTED block (not a synthetic block).
        let block = chain.chain_state.get_block(put_height).expect("accepted block");
        let scan_block = dwow_chain::Block {
            header: block.header.clone(),
            transactions: block.transactions.clone(),
        };
        let mut cap_tree = dww.get_capability_commitment_tree()
            .expect("capability commitment tree");
        let result = dww.scan_block_linear(&mut cap_tree, &scan_block)
            .expect("scan box block");

        assert_eq!(result.capabilities.len(), 1,
            "recipient must discover exactly 1 box_capability from the accepted block");
        let rec = &result.capabilities[0].cap_record;
        assert_eq!(rec.contract_id, box_cid, "discovered from the Box contract");
        assert_eq!(rec.commitment, Commitment::from_base(nl),
            "box new leaf (commitment) read from the accepted block's note");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// Wallet-driven Box `put` through the generic prover — the E2E proof that
/// `invoke_contract` → manifest → generic prover builds a valid L1 proof.
///
/// The wallet discovers a seeded box state via scan (trajectory identification),
/// then builds its OWN put via `invoke_contract`. The circuit-computed values
/// (nullifier, expected_root, new_leaf, tx_binding) are derived by the prover
/// and injected into the wire params (params assembly) — no per-contract Rust.
#[test]
fn test_box_put_wallet_driven_generic_prover() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::Network;
        use dwow_sdk::crypto::{poseidon_hash, BOX_CONTRACT_ID};
        use dwow_sdk::crypto::pasta_prelude::{Field, PrimeField};
        use dwow_sdk::blockchain::BlockHeight;
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{BoxHarness, ContractHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // Seed: harness put() creates the box state the wallet will later spend.
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = BoxHarness::spawn();
        let put = harness.put().expect("seed box put");
        let put_height = chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &put.call_data, vec![put.proof])
            .expect("with_call")
            .submit().await
            .expect("submit seed box put");

        // Wallet keyed to the harness owner secret (42) so it discovers the note.
        let keys_toml = "[boxowner]\nwallet_secret = \
            \"2a00000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_box_wd_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_box_wd_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet, Some(&keys_path), "boxowner",
            wallet_dir.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet init");
        dww.initialize_wallet().expect("wallet schema init");

        let box_cid_str = bs58::encode(BOX_CONTRACT_ID.to_bytes()).into_string();
        // Real manifest + zkas (M2: normally extracted from the genesis DeployV1
        // payload; the test seeds them directly — genesis-scan delivery is separate).
        let manifest_toml = include_str!("../../../../src/contract/box/manifest.toml");
        dww.wallet.insert_contract_metadata_with_manifest(
            &dwow_wallet::walletdb::ContractMetadataRecord {
                contract_id: box_cid_str.clone(), name: "box".into(), symbol: None,
                category: "Infrastructure".into(), description: Some("Box".into()),
                public: true, deployer_pubkey: "".into(), deploy_height: BlockHeight::new(1),
                attestations_json: "[]".into(), lock_status: "unlocked".into(),
            }, Some(manifest_toml),
        ).expect("store Box manifest");
        dww.wallet.store_zkas_binary(&box_cid_str, "Put", "Put",
            include_bytes!("../../../../src/contract/box/proof/put.zk.bin"))
            .expect("store put zkas");

        // Scan blocks 1..=put_height in order (insert_synced_block requires
        // contiguity) → discover the seeded box capability from the put block.
        let mut cap_tree = dww.get_capability_commitment_tree().expect("cap tree");
        let mut total_caps = 0usize;
        for h in 1u64..=put_height.get() {
            let block = chain.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            dww.insert_synced_block(&scan_block).expect("insert block");
            let result = dww.scan_block_linear(&mut cap_tree, &scan_block).expect("scan");
            total_caps += result.capabilities.len();
        }
        assert_eq!(total_caps, 1, "discover exactly 1 box capability (Path 2)");

        // Build the put via the manifest client directly (the generic prover +
        // produce-side note). We bypass the fee-finalizing invoke_contract path —
        // the single-account test wallet holds no DRKW (the coinbase goes to the
        // miner node0, not boxowner), so it cannot pay the fee here; the devnet
        // wallets are funded and exercise the full fee path.
        use dwow_sdk::contract_client::{ManifestContractClient, ContractClient};
        let ncc = poseidon_hash([pallas::Base::from(100u64)]);
        let new_cc = poseidon_hash([pallas::Base::from(200u64)]);
        let base_hex = |b: &pallas::Base| format!("0x{}", hex::encode(b.to_repr()));
        let params_json = format!(
            r#"{{"box_id":"{}","old_state_nonce":"{}","new_state_nonce":"{}","old_contents_commit":"{}","new_contents_commit":"{}","tx_nonce":"{}"}}"#,
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::from(2u64)),
            base_hex(&ncc),
            base_hex(&new_cc),
            base_hex(&pallas::Base::zero()),
        );
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(
            include_str!("../../../../src/contract/box/manifest.toml")
        ).expect("parse Box manifest");
        let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
        let self_pk = *dww.default_address().expect("self address").public_key();
        let client = ManifestContractClient::new(
            "box", manifest, box_cid_str.clone(), seed, self_pk,
        );
        let (call_body, proof_bytes) = client
            .build("put", &params_json, &dww)
            .expect("manifest client build box put");
        let proofs: Vec<dwow_core::zk::Proof> =
            proof_bytes.into_iter().map(dwow_core::zk::Proof::new).collect();
        // call_body = encoded_params ++ note; prepend the fn code byte (0x01 = put).
        let mut call_data = vec![0x01u8];
        call_data.extend_from_slice(&call_body);

        // Submit the wallet-built proof through accept_block — the proof that the
        // generic-prover box put is on-chain valid (not merely built).
        let submit_height = chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &call_data, proofs)
            .expect("with_call wallet-built put")
            .submit().await
            .expect("submit wallet-built box put");
        assert!(submit_height.get() > put_height.get(),
            "wallet-built box put must be accepted on-chain");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// Wallet-driven Box `take` through the generic prover — the E2E proof that
/// the wallet's generic prover builds a valid on-chain `take` (terminal
/// consumption via nullifier), not merely the harness. Mirrors
/// `test_box_put_wallet_driven_generic_prover` for the take circuit.
#[test]
fn test_box_take_wallet_driven_generic_prover() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::Network;
        use dwow_sdk::crypto::{poseidon_hash, BOX_CONTRACT_ID};
        use dwow_sdk::crypto::pasta_prelude::{Field, PrimeField};
        use dwow_sdk::blockchain::BlockHeight;
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{BoxHarness, ContractHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // Seed: harness put() creates the box state the wallet will later take.
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = BoxHarness::spawn();
        let put = harness.put().expect("seed box put");
        let put_height = chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &put.call_data, vec![put.proof])
            .expect("with_call")
            .submit().await
            .expect("submit seed box put");

        // Wallet keyed to the harness owner secret (42) so it discovers the note.
        let keys_toml = "[boxowner]\nwallet_secret = \
            \"2a00000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_box_take_wd_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_box_take_wd_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet, Some(&keys_path), "boxowner",
            wallet_dir.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet init");
        dww.initialize_wallet().expect("wallet schema init");

        let box_cid_str = bs58::encode(BOX_CONTRACT_ID.to_bytes()).into_string();
        // Real manifest + zkas (M2: normally extracted from the genesis DeployV1
        // payload; the test seeds them directly — genesis-scan delivery is separate).
        let manifest_toml = include_str!("../../../../src/contract/box/manifest.toml");
        dww.wallet.insert_contract_metadata_with_manifest(
            &dwow_wallet::walletdb::ContractMetadataRecord {
                contract_id: box_cid_str.clone(), name: "box".into(), symbol: None,
                category: "Infrastructure".into(), description: Some("Box".into()),
                public: true, deployer_pubkey: "".into(), deploy_height: BlockHeight::new(1),
                attestations_json: "[]".into(), lock_status: "unlocked".into(),
            }, Some(manifest_toml),
        ).expect("store Box manifest");
        dww.wallet.store_zkas_binary(&box_cid_str, "Take", "Take",
            include_bytes!("../../../../src/contract/box/proof/take.zk.bin"))
            .expect("store take zkas");

        // Scan blocks 1..=put_height in order → discover the seeded box capability.
        let mut cap_tree = dww.get_capability_commitment_tree().expect("cap tree");
        let mut total_caps = 0usize;
        for h in 1u64..=put_height.get() {
            let block = chain.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            dww.insert_synced_block(&scan_block).expect("insert block");
            let result = dww.scan_block_linear(&mut cap_tree, &scan_block).expect("scan");
            total_caps += result.capabilities.len();
        }
        assert_eq!(total_caps, 1, "discover exactly 1 box capability (Path 2)");

        // Build the take via the manifest client directly (the generic prover).
        // The take consumes the seeded state: box_id=1, contents_commit=poseidon([100]),
        // state_nonce=1. The nullifier/expected_root/leaf_pos/merkle_path/tx_binding
        // are derived by the prover and injected into the wire params.
        use dwow_sdk::contract_client::{ManifestContractClient, ContractClient};
        let cc = poseidon_hash([pallas::Base::from(100u64)]);
        let base_hex = |b: &pallas::Base| format!("0x{}", hex::encode(b.to_repr()));
        let params_json = format!(
            r#"{{"box_id":"{}","contents_commit":"{}","state_nonce":"{}","tx_nonce":"{}"}}"#,
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&cc),
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::zero()),
        );
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(
            include_str!("../../../../src/contract/box/manifest.toml")
        ).expect("parse Box manifest");
        let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
        let self_pk = *dww.default_address().expect("self address").public_key();
        let client = ManifestContractClient::new(
            "box", manifest, box_cid_str.clone(), seed, self_pk,
        );
        let (call_body, proof_bytes) = client
            .build("take", &params_json, &dww)
            .expect("manifest client build box take");
        let proofs: Vec<dwow_core::zk::Proof> =
            proof_bytes.into_iter().map(dwow_core::zk::Proof::new).collect();
        // call_body = encoded_params; prepend the fn code byte (0x02 = take).
        let mut call_data = vec![0x02u8];
        call_data.extend_from_slice(&call_body);

        // Submit the wallet-built take through accept_block — the proof that the
        // generic-prover box take is on-chain valid (not merely built).
        let submit_height = chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &call_data, proofs)
            .expect("with_call wallet-built take")
            .submit().await
            .expect("submit wallet-built box take");
        assert!(submit_height.get() > put_height.get(),
            "wallet-built box take must be accepted on-chain");

        // On-chain acceptance gate: the take nullifier is marked spent.
        // take nf = poseidon_hash([dnl=1, os=42, bid=1, sn=1]).
        let nf = poseidon_hash([
            pallas::Base::from(1u64), pallas::Base::from(42u64),
            pallas::Base::from(1u64), pallas::Base::from(1u64),
        ]);
        let in_nf = chain.query_contract_state(*BOX_CONTRACT_ID, "nullifiers", &nf.to_repr().to_vec())
            .expect("query nullifiers");
        assert!(in_nf.is_some(),
            "box take must mark its nullifier spent (on-chain acceptance)");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// Wallet-driven Purse `deposit` + `withdraw` through the generic prover — the
/// E2E proof that the wallet builds valid on-chain fungible-capability writes
/// with a single owner secret (the state_nonce increment keeps nullifiers unique).
#[test]
fn test_purse_deposit_withdraw_wallet_driven_generic_prover() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::Network;
        use dwow_sdk::crypto::{poseidon_hash, PURSE_CONTRACT_ID};
        use dwow_sdk::crypto::pasta_prelude::{Field, PrimeField};
        use dwow_sdk::blockchain::BlockHeight;
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{PurseHarness, ContractHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // Seed: harness deposit(100) creates the first on-chain purse state
        // (nonce 0 → 1). The wallet later spends it with the same owner secret.
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = PurseHarness::spawn();
        let deposit_seed = harness.deposit(100).expect("seed purse deposit");
        let seed_height = chain.block()
            .expect("block")
            .with_call(*PURSE_CONTRACT_ID, &harness, &deposit_seed.call_data, vec![deposit_seed.proof])
            .expect("with_call")
            .submit().await
            .expect("submit seed purse deposit");

        // Wallet keyed to the harness owner secret (42).
        let keys_toml = "[purseowner]\nwallet_secret = \
            \"2a00000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_purse_wd_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write test keys");
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_purse_wd_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww = Dww::new(
            Network::Testnet, Some(&keys_path), "purseowner",
            wallet_dir.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet init");
        dww.initialize_wallet().expect("wallet schema init");

        let purse_cid_str = bs58::encode(PURSE_CONTRACT_ID.to_bytes()).into_string();
        let manifest_toml = include_str!("../../../../src/contract/purse/manifest.toml");
        dww.wallet.insert_contract_metadata_with_manifest(
            &dwow_wallet::walletdb::ContractMetadataRecord {
                contract_id: purse_cid_str.clone(), name: "purse".into(), symbol: None,
                category: "Infrastructure".into(), description: Some("Purse".into()),
                public: true, deployer_pubkey: "".into(), deploy_height: BlockHeight::new(1),
                attestations_json: "[]".into(), lock_status: "unlocked".into(),
            }, Some(manifest_toml),
        ).expect("store Purse manifest");
        dww.wallet.store_zkas_binary(&purse_cid_str, "Deposit", "Deposit",
            include_bytes!("../../../../src/contract/purse/proof/deposit.zk.bin"))
            .expect("store deposit zkas");
        dww.wallet.store_zkas_binary(&purse_cid_str, "Withdraw", "Withdraw",
            include_bytes!("../../../../src/contract/purse/proof/withdraw.zk.bin"))
            .expect("store withdraw zkas");

        // Scan blocks 1..=seed_height → discover the seeded purse capability.
        let mut cap_tree = dww.get_capability_commitment_tree().expect("cap tree");
        let mut total_caps = 0usize;
        for h in 1u64..=seed_height.get() {
            let block = chain.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            dww.insert_synced_block(&scan_block).expect("insert block");
            let result = dww.scan_block_linear(&mut cap_tree, &scan_block).expect("scan");
            total_caps += result.capabilities.len();
        }
        assert_eq!(total_caps, 1, "discover exactly 1 purse capability (Path 2)");

        use dwow_sdk::contract_client::{ManifestContractClient, ContractClient};
        let base_hex = |b: &pallas::Base| format!("0x{}", hex::encode(b.to_repr()));
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(
            include_str!("../../../../src/contract/purse/manifest.toml")
        ).expect("parse Purse manifest");
        let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
        let self_pk = *dww.default_address().expect("self address").public_key();
        let client = ManifestContractClient::new(
            "purse", manifest, purse_cid_str.clone(), seed, self_pk,
        );

        // Deposit: consume nonce 1 (seed's output, balance 100), produce nonce 2 (150).
        let deposit_params_json = format!(
            r#"{{"purse_id":"{}","old_balance":100,"deposit_amount":50,"new_balance":150,"state_nonce":"{}","tx_nonce":"{}","asset_id":"{}"}}"#,
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::zero()),
            base_hex(&pallas::Base::from(1u64)),
        );
        let (call_body, proof_bytes) = client
            .build("deposit", &deposit_params_json, &dww)
            .expect("manifest client build purse deposit");
        let proofs: Vec<dwow_core::zk::Proof> =
            proof_bytes.into_iter().map(dwow_core::zk::Proof::new).collect();
        let mut call_data = vec![0x01u8]; // 0x01 = deposit
        call_data.extend_from_slice(&call_body);
        let deposit_height = chain.block()
            .expect("block")
            .with_call(*PURSE_CONTRACT_ID, &harness, &call_data, proofs)
            .expect("with_call wallet-built deposit")
            .submit().await
            .expect("submit wallet-built purse deposit");
        assert!(deposit_height.get() > seed_height.get(), "wallet-built deposit accepted");

        // Scan the deposit block → discover the produced purse capability.
        let dep_block = chain.chain_state.get_block(deposit_height).expect("block");
        let dep_scan_block = dwow_chain::Block {
            header: dep_block.header.clone(), transactions: dep_block.transactions.clone(),
        };
        dww.insert_synced_block(&dep_scan_block).expect("insert deposit block");
        let dep_result = dww.scan_block_linear(&mut cap_tree, &dep_scan_block).expect("scan deposit");
        assert_eq!(dep_result.capabilities.len(), 1, "discover the deposit's purse capability");

        // Withdraw: consume nonce 2 (balance 150), produce nonce 3 (100).
        let withdraw_params_json = format!(
            r#"{{"purse_id":"{}","old_balance":150,"withdraw_amount":50,"new_balance":100,"state_nonce":"{}","tx_nonce":"{}","asset_id":"{}"}}"#,
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::from(2u64)),
            base_hex(&pallas::Base::zero()),
            base_hex(&pallas::Base::from(1u64)),
        );
        let (wcall_body, wproof_bytes) = client
            .build("withdraw", &withdraw_params_json, &dww)
            .expect("manifest client build purse withdraw");
        let wproofs: Vec<dwow_core::zk::Proof> =
            wproof_bytes.into_iter().map(dwow_core::zk::Proof::new).collect();
        let mut wcall_data = vec![0x02u8]; // 0x02 = withdraw
        wcall_data.extend_from_slice(&wcall_body);
        let withdraw_height = chain.block()
            .expect("block")
            .with_call(*PURSE_CONTRACT_ID, &harness, &wcall_data, wproofs)
            .expect("with_call wallet-built withdraw")
            .submit().await
            .expect("submit wallet-built purse withdraw");
        assert!(withdraw_height.get() > deposit_height.get(), "wallet-built withdraw accepted");

        // On-chain acceptance: both nullifiers marked spent.
        // deposit nf = poseidon([1, 42, 1, 1]); withdraw nf = poseidon([1, 42, 1, 2]).
        let nf_deposit = poseidon_hash([
            pallas::Base::from(1u64), pallas::Base::from(42u64),
            pallas::Base::from(1u64), pallas::Base::from(1u64),
        ]);
        let in_nf_deposit = chain.query_contract_state(*PURSE_CONTRACT_ID, "nullifiers", &nf_deposit.to_repr().to_vec())
            .expect("query deposit nullifier");
        assert!(in_nf_deposit.is_some(), "deposit nullifier spent on-chain");
        let nf_withdraw = poseidon_hash([
            pallas::Base::from(1u64), pallas::Base::from(42u64),
            pallas::Base::from(1u64), pallas::Base::from(2u64),
        ]);
        let in_nf_withdraw = chain.query_contract_state(*PURSE_CONTRACT_ID, "nullifiers", &nf_withdraw.to_repr().to_vec())
            .expect("query withdraw nullifier");
        assert!(in_nf_withdraw.is_some(), "withdraw nullifier spent on-chain");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}

/// Wallet-driven Box `put` that transfers the box to a NEW owner (RC-C): the
/// wallet builds its own put via `invoke_contract(recipient=B)` and the
/// produce-side note is encrypted to B's key — never self — so the recipient
/// wallet (keyed to B's secret) discovers the transferred `box_capability`.
///
/// This closes the write-path invariant 3 (note production) end-to-end: seed →
/// scan → invoke(recipient=B) → note → new owner's wallet discovers. The note
/// discovery is asserted against a synthetic block carrying the wallet-built
/// call data (the wallet's proof is not submitted through accept_block here —
/// the on-chain tx-binding gate is RC-D, deferred).
#[test]
fn test_box_transfer_to_new_owner_wallet_driven() {
    smol::block_on(async {
        use dwow_wallet::Dww;
        use dwow_sdk::crypto::keypair::{Network, PublicKey, SecretKey};
        use dwow_sdk::crypto::{poseidon_hash, BOX_CONTRACT_ID};
        use dwow_sdk::crypto::pasta_prelude::{Field, PrimeField};
        use dwow_sdk::blockchain::{BlockHeight, BlockTimestamp, BlockVersion, MoneroBlockHeight};
        use dwow_chain::{Block, BlockHeader, BlockTarget, BlockReward, PowSource, Transaction, ContractCall, Commitment};
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{BoxHarness, ContractHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // ── Seed: harness put() creates the box state the wallet will spend ──
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = BoxHarness::spawn();
        let put = harness.put().expect("seed box put");
        let put_height = chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &put.call_data, vec![put.proof])
            .expect("with_call")
            .submit().await
            .expect("submit seed box put");

        // ── Wallet A: owner secret 42 (discovers the seeded box state) ───────
        let keys_toml = "[boxowner]\nwallet_secret = \
            \"2a00000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_path = std::env::temp_dir()
            .join(format!("dwow_box_xfer_a_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_path, keys_toml).expect("write A keys");
        let wallet_dir = std::env::temp_dir()
            .join(format!("dwow_box_xfer_a_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_dir);

        let dww_a = Dww::new(
            Network::Testnet, Some(&keys_path), "boxowner",
            wallet_dir.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet A init");
        dww_a.initialize_wallet().expect("wallet A schema init");

        let box_cid_str = bs58::encode(BOX_CONTRACT_ID.to_bytes()).into_string();
        let manifest_toml = include_str!("../../../../src/contract/box/manifest.toml");
        dww_a.wallet.insert_contract_metadata_with_manifest(
            &dwow_wallet::walletdb::ContractMetadataRecord {
                contract_id: box_cid_str.clone(), name: "box".into(), symbol: None,
                category: "Infrastructure".into(), description: Some("Box".into()),
                public: true, deployer_pubkey: "".into(), deploy_height: BlockHeight::new(1),
                attestations_json: "[]".into(), lock_status: "unlocked".into(),
            }, Some(manifest_toml),
        ).expect("store Box manifest");
        dww_a.wallet.store_zkas_binary(&box_cid_str, "Put", "Put",
            include_bytes!("../../../../src/contract/box/proof/put.zk.bin"))
            .expect("store put zkas");

        // Scan blocks 1..=put_height in order (insert_synced_block requires
        // contiguity) → discover the seeded box capability AND persist the chain
        // blocks so get_merkle_proof can replay the contract tree.
        let mut cap_tree = dww_a.get_capability_commitment_tree().expect("cap tree");
        let mut total_caps = 0usize;
        for h in 1u64..=put_height.get() {
            let block = chain.chain_state.get_block(BlockHeight::new(h)).expect("block");
            let scan_block = dwow_chain::Block {
                header: block.header.clone(), transactions: block.transactions.clone(),
            };
            dww_a.insert_synced_block(&scan_block).expect("insert block");
            let result = dww_a.scan_block_linear(&mut cap_tree, &scan_block).expect("scan");
            total_caps += result.capabilities.len();
        }
        assert_eq!(total_caps, 1, "A discovers exactly 1 seeded box capability");

        // ── New owner B: field element 3 (hex 0300…00) ──────────────────────
        let b_secret = SecretKey::from_bytes({
            let mut b = [0u8; 32];
            b[0] = 0x03;
            b
        }).expect("B secret");
        let b_pk = PublicKey::from_secret(b_secret.clone());

        // Wallet A builds its own put, transferring to B (note encrypted to B).
        let ncc = poseidon_hash([pallas::Base::from(100u64)]);
        let new_cc = poseidon_hash([pallas::Base::from(200u64)]);
        let base_hex = |b: &pallas::Base| format!("0x{}", hex::encode(b.to_repr()));
        let params_json = format!(
            r#"{{"box_id":"{}","old_state_nonce":"{}","new_state_nonce":"{}","old_contents_commit":"{}","new_contents_commit":"{}","tx_nonce":"{}"}}"#,
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::from(1u64)),
            base_hex(&pallas::Base::from(2u64)),
            base_hex(&ncc),
            base_hex(&new_cc),
            base_hex(&pallas::Base::zero()),
        );

        // Wallet A builds its own put via the manifest client directly (the
        // generic prover + produce-side note), transferring to B. We bypass the
        // fee-finalizing invoke_contract path — the wallet holds no DRKW to pay
        // the fee, and on-chain tx-binding (RC-D) is deferred — but the RC-C
        // note (encrypted to B) is exactly what this test asserts.
        use dwow_sdk::contract_client::{ManifestContractClient, ContractClient};
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(
            include_str!("../../../../src/contract/box/manifest.toml")
        ).expect("parse Box manifest");
        let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
        let client = ManifestContractClient::new(
            "box", manifest, box_cid_str.clone(), seed, b_pk,
        );
        let (call_body, _proofs) = client
            .build("put", &params_json, &dww_a)
            .expect("manifest client build box put → note to B");
        // call_body = encoded_params ++ note; prepend the fn code byte (0x01 = put).
        let mut call_data = vec![0x01u8];
        call_data.extend_from_slice(&call_body);

        // ── Synthetic block carrying the wallet-built call (note → B) ────────
        let synthetic = Block {
            header: BlockHeader {
                fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
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
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
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
                contract_calls: vec![
                    ContractCall { contract_id: *BOX_CONTRACT_ID, data: call_data },
                ],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }],
        };

        // ── Wallet B scans the synthetic block → discovers the transfer ──────
        let keys_b_toml = "[boxowner_b]\nwallet_secret = \
            \"0300000000000000000000000000000000000000000000000000000000000000\"\n";
        let keys_b_path = std::env::temp_dir()
            .join(format!("dwow_box_xfer_b_keys_{}.toml", std::process::id()));
        std::fs::write(&keys_b_path, keys_b_toml).expect("write B keys");
        let wallet_b_dir = std::env::temp_dir()
            .join(format!("dwow_box_xfer_b_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wallet_b_dir);

        let dww_b = Dww::new(
            Network::Testnet, Some(&keys_b_path), "boxowner_b",
            wallet_b_dir.to_string_lossy().to_string(), "".to_string(), false, None,
        ).expect("wallet B init");
        dww_b.initialize_wallet().expect("wallet B schema init");
        dww_b.wallet.insert_contract_metadata_with_manifest(
            &dwow_wallet::walletdb::ContractMetadataRecord {
                contract_id: box_cid_str.clone(), name: "box".into(), symbol: None,
                category: "Infrastructure".into(), description: Some("Box".into()),
                public: true, deployer_pubkey: "".into(), deploy_height: BlockHeight::new(1),
                attestations_json: "[]".into(), lock_status: "unlocked".into(),
            }, Some(include_str!("../../../../src/contract/box/manifest.toml")),
        ).expect("store Box manifest for B");

        let mut cap_tree_b = dww_b.get_capability_commitment_tree().expect("B cap tree");
        let result_b = dww_b.scan_block_linear(&mut cap_tree_b, &synthetic).expect("B scan");
        assert_eq!(result_b.capabilities.len(), 1,
            "B must discover exactly 1 box_capability (transferred, not self)");
        let rec = &result_b.capabilities[0].cap_record;

        assert_eq!(rec.contract_id, *BOX_CONTRACT_ID, "discovered from the Box contract");
        assert_eq!(rec.capability_name.as_deref(), Some("box_capability"),
            "capability name from the Box manifest");

        // The produce-side note's commitment is the new leaf (derived:leaf slot 7):
        // poseidon([DRK_POSEIDON_DOMAIN_MERKLE_LEAF=5, box_id=1, new_cc, new_sn=2]).
        let new_leaf = poseidon_hash([
            pallas::Base::from(5u64), pallas::Base::from(1u64),
            new_cc, pallas::Base::from(2u64),
        ]);
        assert_eq!(rec.commitment, Commitment::from_base(new_leaf),
            "B's discovered commitment is the transferred box new leaf");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
        let _ = std::fs::remove_file(&keys_b_path);
        let _ = std::fs::remove_dir_all(&wallet_b_dir);
    });
}

/// A real Box `take` (terminal consumption) is submitted through `accept_block`
/// after a `put` — the write-path validation gate `box_roots` check on the take
/// path (`box/src/entrypoint/mod.rs` take) — and the take nullifier lands
/// on-chain.
#[test]
fn test_box_take_accepts_through_accept_block() {
    smol::block_on(async {
        use dwow_sdk::crypto::{BOX_CONTRACT_ID, pasta_prelude::PrimeField, poseidon_hash};
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{BoxHarness, ContractHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // ── Real chain: genesis + submit a Box put through accept_block ────
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = BoxHarness::spawn();

        // put (first op): appends new_leaf, advancing box_roots past EMPTY so the
        // take's root check is NOT skipped.
        let put = harness.put().expect("box put");
        chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &put.call_data, vec![put.proof])
            .expect("with_call put")
            .submit().await
            .expect("submit box put");

        // take (second op): expected_root must be a box_roots key (the put root).
        let take = harness.take().expect("box take");
        chain.block()
            .expect("block")
            .with_call(*BOX_CONTRACT_ID, &harness, &take.call_data, vec![take.proof])
            .expect("with_call take")
            .submit().await
            .expect("submit box take");

        // ── On-chain acceptance gate: the take nullifier is marked spent ───
        // take nf = poseidon_hash([dnl=1, os=42, bid=1, sn=1]).
        let nf = poseidon_hash([
            pallas::Base::from(1u64), pallas::Base::from(42u64),
            pallas::Base::from(1u64), pallas::Base::from(1u64),
        ]);
        let in_nf = chain.query_contract_state(*BOX_CONTRACT_ID, "nullifiers", &nf.to_repr().to_vec())
            .expect("query nullifiers");
        assert!(in_nf.is_some(),
            "box take must mark its nullifier spent (on-chain acceptance)");
    });
}

/// A real Purse `deposit` + `withdraw` are submitted through `accept_block` —
/// the deposit appends a new leaf (skipping the `purse_roots` gate on the first
/// op, when the latest root is still the EMPTY genesis root), and the withdraw's
/// `expected_root` must be a `purse_roots` key (the deposit's new root), reaching
/// the `purse_roots` gate at `purse/src/entrypoint/mod.rs` withdraw path.
#[test]
fn test_purse_deposit_withdraw_accepts_through_accept_block() {
    smol::block_on(async {
        use dwow_sdk::crypto::{MerkleNode, MerkleTree, PURSE_CONTRACT_ID, poseidon_hash};
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{ContractHarness, PurseHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // ── Real chain: genesis + submit a Purse deposit through accept_block ─
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = PurseHarness::spawn();

        // deposit(100) — the FIRST op skips the purse_roots gate (latest root is
        // still EMPTY_PURSE_TREE_ROOT) and appends nl = poseidon_hash([5,1,100,0]).
        let deposit = harness.deposit(100).expect("purse deposit");
        chain.block()
            .expect("block")
            .with_call(*PURSE_CONTRACT_ID, &harness, &deposit.call_data, vec![deposit.proof])
            .expect("with_call deposit")
            .submit().await
            .expect("submit purse deposit");

        // withdraw(50) — the SECOND op's expected_root = root([zero, nl_100]) must
        // be a purse_roots key (the deposit's new root), reaching the purse_roots
        // gate (skip_root_check is now false).
        let withdraw = harness.withdraw(50).expect("purse withdraw");
        chain.block()
            .expect("block")
            .with_call(*PURSE_CONTRACT_ID, &harness, &withdraw.call_data, vec![withdraw.proof])
            .expect("with_call withdraw")
            .submit().await
            .expect("submit purse withdraw");

        // ── On-chain acceptance gate: the withdraw's expected_root (the deposit's
        // new root) is a purse_roots key. Mirrors purse_spec.rs verify_state:
        // nl = poseidon_hash([dml=5, pid=1, nb=100, sn=1]), tree = [ZERO, nl].
        let nl = poseidon_hash([
            pallas::Base::from(5u64), pallas::Base::from(1u64),
            pallas::Base::from(100u64), pallas::Base::from(1u64),
        ]);
        let mut tree = MerkleTree::new(1);
        tree.append(MerkleNode::from_base(pallas::Base::zero()));
        tree.append(MerkleNode::from_base(nl));
        let expected_root = tree.root(0).expect("tree.root").to_bytes().to_vec();
        let in_roots = chain.query_contract_state(*PURSE_CONTRACT_ID, "purse_roots", &expected_root)
            .expect("query purse_roots");
        assert!(in_roots.is_some(),
            "purse deposit must append new_leaf to purse_roots (on-chain acceptance)");
    });
}

/// A real PromissoryNote `transfer` is submitted through `accept_block` after a
/// `register_type` + `issue` seed a spendable note — the transfer input's
/// `merkle_root` must be a `commitment_roots` key (the gate at
/// `promissory_note/src/entrypoint/mod.rs` transfer path).
#[test]
fn test_promissory_note_transfer_accepts_through_accept_block() {
    smol::block_on(async {
        use dwow_sdk::crypto::keypair::{PublicKey, SecretKey};
        use dwow_sdk::crypto::{MerkleNode, MerkleTree, PROMISSORY_NOTE_CONTRACT_ID, poseidon_hash};
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness};
        use dwow_promissory_note_contract::client::transfer::{TransferCallInput, TransferCallOutput};
        use crate::tests::blockchain::HeavyweightPipeline;

        // ── Real chain: genesis + seed a spendable note through accept_block ─
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = PromissoryNoteHarness::spawn();

        // Deterministic seed values (mirror promissory_note_spec.rs).
        let auth_parent = pallas::Base::from(1u64);
        let user_data = pallas::Base::from(2u64);
        let blind = pallas::Base::from(3u64);
        let recipient = poseidon_hash([pallas::Base::from(7u64), auth_parent]);
        let spend_hook = pallas::Base::zero();
        let commitment_blind = pallas::Base::from(6u64);
        let token_auth_parent = poseidon_hash([pallas::Base::from(7u64), auth_parent]);
        let asset_id = poseidon_hash([pallas::Base::from(2u64), token_auth_parent, user_data, blind]);

        // register_type (token type) then issue (note at pos 2).
        let reg = harness.register_type(auth_parent, user_data, blind, recipient,
            1000, spend_hook, user_data, commitment_blind).expect("register_type");
        chain.block()
            .expect("block")
            .with_call(*PROMISSORY_NOTE_CONTRACT_ID, &harness, &reg.call_data, reg.token_proofs)
            .expect("with_call register_type")
            .submit().await
            .expect("submit register_type");

        let issue = harness.issue(auth_parent, asset_id, recipient,
            500, spend_hook, user_data, commitment_blind).expect("issue");
        chain.block()
            .expect("block")
            .with_call(*PROMISSORY_NOTE_CONTRACT_ID, &harness, &issue.call_data, issue.proofs)
            .expect("with_call issue")
            .submit().await
            .expect("submit issue");

        // Build the transfer's merkle witness over [guard, coin_a, coin_b].
        let coin_a = reg.commitment.inner();
        let coin_b = issue.commitment.inner();
        let mut tree_3 = MerkleTree::new(1);
        tree_3.append(MerkleNode::from_base(pallas::Base::zero()));
        tree_3.append(MerkleNode::from_base(coin_a));
        tree_3.append(MerkleNode::from_base(coin_b));
        let mark_b = tree_3.mark().expect("tree.mark b");
        let path_b: Vec<MerkleNode> = tree_3.witness(mark_b, 0).expect("witness b");
        let pos_b = u64::from(mark_b);
        let merkle_root = tree_3.root(0).expect("tree.root").to_bytes().to_vec();

        // transfer (spends coin_b): input.merkle_root must be a commitment_roots key.
        let recipient_pub = PublicKey::from_secret(SecretKey::from_base(recipient));
        let input = TransferCallInput {
            value: 500, asset_id, spend_hook, user_data, commitment_blind,
            leaf_position: pos_b, merkle_path: path_b,
            secret: auth_parent,
            ephemeral_signature_secret: pallas::Base::from(9u64),
            tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero(),
        };
        let output = TransferCallOutput {
            recipient, recipient_pub, value: 500, asset_id, spend_hook, user_data,
            commitment_blind: pallas::Base::from(7u64),
        };
        let transfer = harness.transfer(vec![input], vec![output]).expect("transfer");
        chain.block()
            .expect("block")
            .with_call(*PROMISSORY_NOTE_CONTRACT_ID, &harness, &transfer.call_data, transfer.proofs)
            .expect("with_call transfer")
            .submit().await
            .expect("submit transfer");

        // ── On-chain acceptance gate: the transfer input's merkle_root is a
        // commitment_roots key (transfer_v1 gate).
        let in_roots = chain.query_contract_state(*PROMISSORY_NOTE_CONTRACT_ID, "commitment_roots", &merkle_root)
            .expect("query commitment_roots");
        assert!(in_roots.is_some(),
            "PN transfer input merkle_root must be a commitment_roots key (on-chain acceptance)");
    });
}

/// A real PromissoryNote `redeem` is submitted through `accept_block` after a
/// `register_type` + `issue` seed a spendable note — the redeem input's
/// `merkle_root` must be a `commitment_roots` key (the gate at
/// `promissory_note/src/entrypoint/mod.rs` redeem path).
#[test]
fn test_promissory_note_redeem_accepts_through_accept_block() {
    smol::block_on(async {
        use dwow_sdk::crypto::{MerkleNode, MerkleTree, PROMISSORY_NOTE_CONTRACT_ID, poseidon_hash};
        use dwow_sdk::pasta::pallas;
        use dwow_contract_test_harness::harness::{ContractHarness, PromissoryNoteHarness};
        use crate::tests::blockchain::HeavyweightPipeline;

        // ── Real chain: genesis + seed a spendable note through accept_block ─
        let chain = HeavyweightPipeline::new().await.expect("HeavyweightPipeline");
        chain.init_genesis().await.expect("init_genesis");
        let harness = PromissoryNoteHarness::spawn();

        // Deterministic seed values (mirror promissory_note_spec.rs).
        let auth_parent = pallas::Base::from(1u64);
        let user_data = pallas::Base::from(2u64);
        let blind = pallas::Base::from(3u64);
        let recipient = poseidon_hash([pallas::Base::from(7u64), auth_parent]);
        let spend_hook = pallas::Base::zero();
        let commitment_blind = pallas::Base::from(6u64);
        let token_auth_parent = poseidon_hash([pallas::Base::from(7u64), auth_parent]);
        let asset_id = poseidon_hash([pallas::Base::from(2u64), token_auth_parent, user_data, blind]);

        // register_type (token type) then issue (note at pos 2).
        let reg = harness.register_type(auth_parent, user_data, blind, recipient,
            1000, spend_hook, user_data, commitment_blind).expect("register_type");
        chain.block()
            .expect("block")
            .with_call(*PROMISSORY_NOTE_CONTRACT_ID, &harness, &reg.call_data, reg.token_proofs)
            .expect("with_call register_type")
            .submit().await
            .expect("submit register_type");

        let issue = harness.issue(auth_parent, asset_id, recipient,
            500, spend_hook, user_data, commitment_blind).expect("issue");
        chain.block()
            .expect("block")
            .with_call(*PROMISSORY_NOTE_CONTRACT_ID, &harness, &issue.call_data, issue.proofs)
            .expect("with_call issue")
            .submit().await
            .expect("submit issue");

        // Build the redeem's merkle witness over [guard, coin_a, coin_b].
        let coin_a = reg.commitment.inner();
        let coin_b = issue.commitment.inner();
        let mut tree_3 = MerkleTree::new(1);
        tree_3.append(MerkleNode::from_base(pallas::Base::zero()));
        tree_3.append(MerkleNode::from_base(coin_a));
        tree_3.append(MerkleNode::from_base(coin_b));
        let mark_b = tree_3.mark().expect("tree.mark b");
        let path_b: Vec<MerkleNode> = tree_3.witness(mark_b, 0).expect("witness b");
        let pos_b = u64::from(mark_b);
        let merkle_root = tree_3.root(0).expect("tree.root").to_bytes().to_vec();

        // redeem (spends coin_b): input.merkle_root must be a commitment_roots key.
        let redeem = harness.redeem(500, asset_id, spend_hook, user_data,
            commitment_blind, auth_parent, recipient, pos_b, path_b)
            .expect("redeem");
        chain.block()
            .expect("block")
            .with_call(*PROMISSORY_NOTE_CONTRACT_ID, &harness, &redeem.call_data, redeem.proofs)
            .expect("with_call redeem")
            .submit().await
            .expect("submit redeem");

        // ── On-chain acceptance gate: the redeem input's merkle_root is a
        // commitment_roots key (redeem_v1 gate).
        let in_roots = chain.query_contract_state(*PROMISSORY_NOTE_CONTRACT_ID, "commitment_roots", &merkle_root)
            .expect("query commitment_roots");
        assert!(in_roots.is_some(),
            "PN redeem input merkle_root must be a commitment_roots key (on-chain acceptance)");
    });
}
