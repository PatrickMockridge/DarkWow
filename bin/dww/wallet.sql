-- Wallet definitions for drk.
-- We store data that is needed for wallet operations.

PRAGMA foreign_keys = ON;

-- Scanned blocks information (for rollback support)
CREATE TABLE IF NOT EXISTS scanned_blocks (
    height INTEGER PRIMARY KEY NOT NULL,
    hash TEXT NOT NULL,
    signing_key TEXT NOT NULL DEFAULT '-'
);

-- Chain blocks: wallet's synced blockchain data.
-- Replaces the sled-backed dwow_chain::LinearStore.
CREATE TABLE IF NOT EXISTS chain_blocks (
    height INTEGER PRIMARY KEY NOT NULL,
    block_json TEXT NOT NULL
);

-- Addresses table REMOVED — the wallet no longer stores keys. Its identity is
-- declared in keys.toml and derived on boot via AccountManager (no key store).

-- tokens table REMOVED — never populated; token knowledge is from capabilities.

-- Held capabilities: retained capabilities with Merkle proof metadata
-- V.1 migration (2026-07): cryptographic fields changed from TEXT (bs58) to BLOB.
-- Migration adds new BLOB columns, copies data from old TEXT columns, then drops
-- old columns. See walletdb.rs::migrate_v1_caprecord().
CREATE TABLE IF NOT EXISTS held_capabilities (
    cap_id TEXT PRIMARY KEY NOT NULL,
    value INTEGER NOT NULL,
    token_id_blob BLOB,
    token_id TEXT,
    spend_hook_blob BLOB,
    spend_hook TEXT,
    user_data_blob BLOB,
    user_data TEXT,
    leaf_position INTEGER NOT NULL,
    commitment_blob BLOB,
    commitment TEXT,
    contract_id_blob BLOB,
    func_id_blob BLOB,
    capability_discriminant INTEGER,
    cap_blind_blob BLOB,
    cap_blind TEXT,
    value_blind_blob BLOB,
    value_blind TEXT,
    token_blind_blob BLOB,
    token_blind TEXT,
    revoked INTEGER NOT NULL DEFAULT 0,
    revoked_at_height INTEGER,
    created_at_height INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_held_capabilities_token_id ON held_capabilities(token_id);
CREATE INDEX IF NOT EXISTS idx_held_capabilities_revoked ON held_capabilities(revoked);

-- Coin Merkle proofs table: stores Merkle paths
CREATE TABLE IF NOT EXISTS capability_proofs (
    cap_id TEXT PRIMARY KEY NOT NULL,
    merkle_proof TEXT NOT NULL,
    merkle_root TEXT NOT NULL,
    FOREIGN KEY (cap_id) REFERENCES held_capabilities(cap_id)
);

-- NOTE: capability_secrets and addresses tables removed — no key store.
-- Identity is declared in keys.toml and derived on boot via AccountManager;
-- scan reads secrets from `Dww.account_mgr` (no SQLite mirror).

-- Cache state tables (formerly sled trees — consolidated into SQLite 2026-07-02)

-- Merkle tree checkpoints (replaces _merkle_trees sled tree)
CREATE TABLE IF NOT EXISTS merkle_trees (
    name TEXT PRIMARY KEY,
    tree_blob BLOB NOT NULL
);
-- Key lifecycle persistence: JSON blob holding encrypted lifecycle keys
-- (imported, generated, HD-derived) beyond the declared identity from keys.toml.
-- Single row, loaded on boot after AccountManager::open().
CREATE TABLE IF NOT EXISTS key_lifecycle (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    blob TEXT NOT NULL
);

-- account_manager table REMOVED — the wallet no longer persists key material.
-- Identity is declared in keys.toml and derived on boot via AccountManager.
-- deploy_authorities table REMOVED — never populated.
-- Contract registry table: maps contract names to their deployed ContractIds
-- Contract metadata table: on-chain metadata discovered during scan
CREATE TABLE IF NOT EXISTS contract_metadata (
    contract_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    symbol TEXT,
    category TEXT NOT NULL,
    description TEXT,
    public INTEGER NOT NULL DEFAULT 1,
    deployer_pubkey TEXT NOT NULL,
    deploy_height INTEGER NOT NULL,
    attestations_json TEXT DEFAULT '[]',
    manifest_json TEXT DEFAULT '',
    lock_status TEXT DEFAULT 'unlocked'
);

CREATE INDEX IF NOT EXISTS idx_contract_metadata_category ON contract_metadata(category);
CREATE INDEX IF NOT EXISTS idx_contract_metadata_public ON contract_metadata(public);

-- contract_interactions table REMOVED (fully dead — no writer, no reader).

-- aliases table REMOVED — never populated (no writer).

-- Capabilities table: generic storage for ALL discovered capabilities.
-- The AEAD authentication tag IS the discriminator. When the generic
-- scan path decrypts an output, the capability is stored here regardless
-- of whether we recognize the note type. Structured decoders (NativeToken,
-- PromissoryNote, etc.) also record here in addition to their typed tables.
-- generic capabilities table REMOVED — write-only dead (only held_capabilities lives).
