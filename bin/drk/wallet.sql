-- Wallet definitions for drk.
-- We store data that is needed for wallet operations.

PRAGMA foreign_keys = ON;

-- Scanned blocks information (for rollback support)
CREATE TABLE IF NOT EXISTS scanned_blocks (
    height INTEGER PRIMARY KEY NOT NULL,
    hash TEXT NOT NULL,
    rollback_query TEXT NOT NULL
);

-- Addresses table: stores wallet addresses
CREATE TABLE IF NOT EXISTS addresses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    public_key TEXT NOT NULL UNIQUE,
    secret TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    created_at_height INTEGER NOT NULL DEFAULT 0
);

-- Transactions history
CREATE TABLE IF NOT EXISTS transactions_history (
    transaction_hash TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    block_height INTEGER,
	tx BLOB NOT NULL
);

-- Tokens table: stores token metadata
CREATE TABLE IF NOT EXISTS tokens (
    token_id TEXT PRIMARY KEY NOT NULL,           -- Token ID (bs58 encoded pallas::Base)
    name TEXT,                                    -- Token name/alias
    symbol TEXT,                                  -- Token symbol
    decimals INTEGER DEFAULT 8,                    -- Decimal places
    mint_authority TEXT,                          -- Mint authority secret (bs58 encoded)
    token_blind TEXT NOT NULL,                    -- Token blind (bs58 encoded)
    is_frozen INTEGER NOT NULL DEFAULT 0,          -- 0=not frozen, 1=frozen
    freeze_height INTEGER,                        -- Height when token was frozen
    created_at_height INTEGER NOT NULL            -- Block height when created
);

CREATE INDEX IF NOT EXISTS idx_tokens_name ON tokens(name);
CREATE INDEX IF NOT EXISTS idx_tokens_frozen ON tokens(is_frozen);

-- Coins table: tracks unspent held_capabilities with Merkle proof metadata
CREATE TABLE IF NOT EXISTS held_capabilities (
    cap_id TEXT PRIMARY KEY NOT NULL,
    value INTEGER NOT NULL,
    token_id TEXT NOT NULL,
    spend_hook TEXT,
    user_data TEXT,
    leaf_position INTEGER NOT NULL,
    secret TEXT NOT NULL,
    cap_blind TEXT NOT NULL,
    value_blind TEXT NOT NULL,
    token_blind TEXT NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0,
    revoked_at_height INTEGER,
    externally_revoked INTEGER NOT NULL DEFAULT 0,
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

-- NOTE: capability_secrets table removed (2026-07-02).
-- Secrets are now stored exclusively in the addresses table.
-- AccountManager is the single key authority; scan reads from AccountManager,
-- not from a separate SQLite mirror. This eliminates the dual-store anti-pattern.

-- Cache state tables (formerly sled trees — consolidated into SQLite 2026-07-02)

-- Merkle tree checkpoints (replaces _merkle_trees sled tree)
CREATE TABLE IF NOT EXISTS merkle_trees (
    name TEXT PRIMARY KEY,
    tree_blob BLOB NOT NULL
);

-- Nullifier Sparse Merkle Tree (replaces _nullifier_smt sled tree)
-- WITHOUT ROWID for fast key-value lookups (O(1) B-tree vs O(2) with rowid)
CREATE TABLE IF NOT EXISTS nullifier_smt (
    key BLOB PRIMARY KEY,
    value BLOB NOT NULL
) WITHOUT ROWID;

-- AccountManager persistence (replaces sled "accounts" tree)
-- Single-row table: id=1 is the only row
CREATE TABLE IF NOT EXISTS account_manager (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    accounts_json TEXT NOT NULL
);

-- Deploy authorities table: stores deploy authority keypairs
CREATE TABLE IF NOT EXISTS deploy_authorities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contract_id TEXT NOT NULL,
    secret TEXT NOT NULL,
    is_locked INTEGER NOT NULL DEFAULT 0,
    created_at_height INTEGER,
    created_at INTEGER NOT NULL
);

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

-- Contract interactions table: records wallet-initiated contract calls
CREATE TABLE IF NOT EXISTS contract_interactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contract_id TEXT NOT NULL,
    function_name TEXT NOT NULL,
    tx_hash TEXT NOT NULL,
    block_height INTEGER,
    timestamp INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_contract_interactions_cid ON contract_interactions(contract_id);

-- Aliases table: human-readable token aliases (e.g. "DRK" → token_id).
-- Used by wallet balance to display familiar names instead of raw token IDs.
CREATE TABLE IF NOT EXISTS aliases (
    alias TEXT PRIMARY KEY NOT NULL,
    token_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT 0
);

-- Capabilities table: generic storage for ALL discovered capabilities.
-- The AEAD authentication tag IS the discriminator. When the generic
-- scan path decrypts an output, the capability is stored here regardless
-- of whether we recognize the note type. Structured decoders (NativeToken,
-- PromissoryNote, etc.) also record here in addition to their typed tables.
--
-- This is the foundational table of the capability OS kernel — it makes
-- the wallet discover capabilities from ANY contract without code changes.
CREATE TABLE IF NOT EXISTS capabilities (
    nullifier TEXT PRIMARY KEY NOT NULL,
    contract_id TEXT NOT NULL,
    block_height INTEGER NOT NULL,
    note_type TEXT NOT NULL DEFAULT 'unknown',
    raw_data BLOB
);
