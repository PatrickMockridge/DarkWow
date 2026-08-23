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
        use dwow_chain::{Block, BlockHeader, BlockTarget, BlockReward, PowSource, Transaction, ContractCall, CoinCommitment};
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
        // The note carries value, asset_id, spend_hook, user_data, coin_blind,
        // value_blind, token_blind, memo, and commitment (the CapCommitment).
        let value: u64 = 1_000_000;
        let asset_id = pallas::Base::from(777u64);
        let commitment = pallas::Base::from(999u64);

        let note = dwow_promissory_note_contract::client::PromissoryNote {
            value,
            asset_id,
            spend_hook: pallas::Base::from(0u64),
            user_data: pallas::Base::from(0u64),
            coin_blind: pallas::Base::from(11u64),
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
        assert_eq!(rec.commitment, CoinCommitment::from_base(commitment),
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
        use dwow_chain::{Block, BlockHeader, BlockTarget, BlockReward, PowSource, Transaction, ContractCall, CoinCommitment};
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
        assert_eq!(rec.commitment, CoinCommitment::from_base(nl),
            "commitment (box new leaf) read from the note");

        // Cleanup
        let _ = std::fs::remove_file(&keys_path);
        let _ = std::fs::remove_dir_all(&wallet_dir);
    });
}
