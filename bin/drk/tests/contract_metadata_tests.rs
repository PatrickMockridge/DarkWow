use std::sync::Arc;

use dwow_sdk::deploy::{Category, ContractMetadata};
use dww::walletdb::{ContractMetadataRecord, WalletDb};

#[test]
fn test_contract_metadata_roundtrip_empty() {
    let meta = ContractMetadata {
        name: "TestContract".to_string(),
        symbol: None,
        category: Category::Other,
        description: None,
        public: true,
        attestations: vec![],
    };

    let bytes = meta.to_ix_bytes();
    assert!(!bytes.is_empty());

    let decoded = ContractMetadata::from_ix_bytes(&bytes).expect("roundtrip should succeed");
    assert_eq!(decoded.name, "TestContract");
    assert_eq!(decoded.symbol, None);
    assert_eq!(decoded.category, Category::Other);
    assert_eq!(decoded.description, None);
    assert!(decoded.public);
    assert!(decoded.attestations.is_empty());
}

#[test]
fn test_contract_metadata_roundtrip_full() {
    let meta = ContractMetadata {
        name: "DarkWow Stablecoin".to_string(),
        symbol: Some("DRWUSD".to_string()),
        category: Category::Stablecoin,
        description: Some("A stablecoin backed by real-world assets".to_string()),
        public: true,
        attestations: vec![],
    };

    let bytes = meta.to_ix_bytes();
    let decoded = ContractMetadata::from_ix_bytes(&bytes).expect("roundtrip should succeed");
    assert_eq!(decoded.name, "DarkWow Stablecoin");
    assert_eq!(decoded.symbol.as_deref(), Some("DRWUSD"));
    assert_eq!(decoded.category, Category::Stablecoin);
    assert_eq!(decoded.description.as_deref(), Some("A stablecoin backed by real-world assets"));
    assert!(decoded.public);
}

#[test]
fn test_from_ix_bytes_empty_returns_none() {
    assert!(ContractMetadata::from_ix_bytes(&[]).is_none());
}

#[test]
fn test_from_ix_bytes_garbage_returns_none() {
    assert!(ContractMetadata::from_ix_bytes(&[0xFF, 0xAB, 0x00]).is_none());
}

fn setup_wallet() -> Arc<WalletDb> {
    let wallet = WalletDb::new(None, None).unwrap();
    wallet.exec_batch_sql(
        "CREATE TABLE IF NOT EXISTS contract_metadata (
            contract_id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            symbol TEXT,
            category TEXT NOT NULL,
            description TEXT,
            public INTEGER NOT NULL DEFAULT 1,
            deployer_pubkey TEXT NOT NULL,
            deploy_height INTEGER NOT NULL,
            attestations_json TEXT DEFAULT '[]',
            lock_status TEXT DEFAULT 'unlocked'
        );
        CREATE INDEX IF NOT EXISTS idx_contract_metadata_category ON contract_metadata(category);
        CREATE INDEX IF NOT EXISTS idx_contract_metadata_public ON contract_metadata(public);",
    )
    .unwrap();
    wallet
}

#[test]
fn test_insert_and_get_metadata() {
    let wallet = setup_wallet();

    let record = ContractMetadataRecord {
        contract_id: "abc123".to_string(),
        name: "TestDAO".to_string(),
        symbol: Some("TDAO".to_string()),
        category: "DAO".to_string(),
        description: Some("A test DAO".to_string()),
        public: true,
        deployer_pubkey: "pubkey123".to_string(),
        deploy_height: 100,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    wallet.insert_contract_metadata(&record).unwrap();

    let fetched = wallet.get_contract_metadata("abc123").unwrap();
    assert_eq!(fetched.contract_id, "abc123");
    assert_eq!(fetched.name, "TestDAO");
    assert_eq!(fetched.symbol.as_deref(), Some("TDAO"));
    assert_eq!(fetched.category, "DAO");
    assert!(fetched.public);
    assert_eq!(fetched.deploy_height, 100);
    assert_eq!(fetched.lock_status, "unlocked");
}

#[test]
fn test_get_nonexistent_metadata_fails() {
    let wallet = setup_wallet();
    assert!(wallet.get_contract_metadata("nonexistent").is_err());
}

#[test]
fn test_public_only_filter() {
    let wallet = setup_wallet();

    let public_token = ContractMetadataRecord {
        contract_id: "tok1".to_string(),
        name: "PublicToken".to_string(),
        symbol: None,
        category: "Token".to_string(),
        description: None,
        public: true,
        deployer_pubkey: "pk1".to_string(),
        deploy_height: 10,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    let unlisted_dao = ContractMetadataRecord {
        contract_id: "dao1".to_string(),
        name: "SecretDAO".to_string(),
        symbol: None,
        category: "DAO".to_string(),
        description: None,
        public: false,
        deployer_pubkey: "pk2".to_string(),
        deploy_height: 20,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    let public_dex = ContractMetadataRecord {
        contract_id: "dex1".to_string(),
        name: "PublicDEX".to_string(),
        symbol: None,
        category: "DEX".to_string(),
        description: None,
        public: true,
        deployer_pubkey: "pk3".to_string(),
        deploy_height: 30,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    wallet.insert_contract_metadata(&public_token).unwrap();
    wallet.insert_contract_metadata(&unlisted_dao).unwrap();
    wallet.insert_contract_metadata(&public_dex).unwrap();

    let all = wallet.get_contract_metadata_list(false).unwrap();
    assert_eq!(all.len(), 3, "all records should be returned when public_only=false");

    let public_only = wallet.get_contract_metadata_list(true).unwrap();
    assert_eq!(public_only.len(), 2, "only 2 public records");
    for r in &public_only {
        assert!(r.public, "all returned records should be public");
    }
}

#[test]
fn test_category_filter() {
    let wallet = setup_wallet();

    let dao1 = ContractMetadataRecord {
        contract_id: "dao_a".to_string(),
        name: "DAO Alpha".to_string(),
        symbol: None,
        category: "DAO".to_string(),
        description: None,
        public: true,
        deployer_pubkey: "pk_a".to_string(),
        deploy_height: 5,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    let dao2 = ContractMetadataRecord {
        contract_id: "dao_b".to_string(),
        name: "DAO Beta".to_string(),
        symbol: None,
        category: "DAO".to_string(),
        description: None,
        public: false, // unlisted
        deployer_pubkey: "pk_b".to_string(),
        deploy_height: 15,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    let token1 = ContractMetadataRecord {
        contract_id: "tok_a".to_string(),
        name: "Token Alpha".to_string(),
        symbol: None,
        category: "Token".to_string(),
        description: None,
        public: true,
        deployer_pubkey: "pk_t".to_string(),
        deploy_height: 25,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    wallet.insert_contract_metadata(&dao1).unwrap();
    wallet.insert_contract_metadata(&dao2).unwrap();
    wallet.insert_contract_metadata(&token1).unwrap();

    let dao_list = wallet.get_contract_metadata_by_category("DAO").unwrap();
    assert_eq!(dao_list.len(), 1, "only public DAO should be returned");
    assert_eq!(dao_list[0].contract_id, "dao_a");
    assert!(dao_list[0].public);

    let token_list = wallet.get_contract_metadata_by_category("Token").unwrap();
    assert_eq!(token_list.len(), 1);
    assert_eq!(token_list[0].contract_id, "tok_a");
}

#[test]
fn test_update_existing_metadata() {
    let wallet = setup_wallet();

    let record = ContractMetadataRecord {
        contract_id: "upd1".to_string(),
        name: "Original".to_string(),
        symbol: None,
        category: "Other".to_string(),
        description: None,
        public: false,
        deployer_pubkey: "pk_upd".to_string(),
        deploy_height: 42,
        attestations_json: "[]".to_string(),
        lock_status: "unlocked".to_string(),
    };

    wallet.insert_contract_metadata(&record).unwrap();

    // Insert again with same contract_id (uses INSERT OR REPLACE)
    let updated = ContractMetadataRecord {
        contract_id: "upd1".to_string(),
        name: "Updated".to_string(),
        symbol: Some("UPD".to_string()),
        category: "Token".to_string(),
        description: Some("Updated desc".to_string()),
        public: true,
        deployer_pubkey: "pk_upd".to_string(),
        deploy_height: 42,
        attestations_json: "[]".to_string(),
        lock_status: "locked".to_string(),
    };

    wallet.insert_contract_metadata(&updated).unwrap();

    let fetched = wallet.get_contract_metadata("upd1").unwrap();
    assert_eq!(fetched.name, "Updated");
    assert_eq!(fetched.symbol.as_deref(), Some("UPD"));
    assert_eq!(fetched.category, "Token");
    assert_eq!(fetched.lock_status, "locked");
    assert!(fetched.public);
}
