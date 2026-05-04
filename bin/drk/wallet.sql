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
    public_key TEXT NOT NULL,
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

-- Coins table: tracks unspent coins with Merkle proof metadata
CREATE TABLE IF NOT EXISTS coins (
    coin_id TEXT PRIMARY KEY NOT NULL,
    value INTEGER NOT NULL,
    token_id TEXT NOT NULL,
    spend_hook TEXT,
    user_data TEXT,
    leaf_position INTEGER NOT NULL,
    secret TEXT NOT NULL,
    coin_blind TEXT NOT NULL,
    value_blind TEXT NOT NULL,
    token_blind TEXT NOT NULL,
    spent INTEGER NOT NULL DEFAULT 0,
    spent_at_height INTEGER,
    created_at_height INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_coins_token_id ON coins(token_id);
CREATE INDEX IF NOT EXISTS idx_coins_spent ON coins(spent);

-- Coin Merkle proofs table: stores Merkle paths
CREATE TABLE IF NOT EXISTS coin_merkle_proofs (
    coin_id TEXT PRIMARY KEY NOT NULL,
    merkle_proof TEXT NOT NULL,
    merkle_root TEXT NOT NULL,
    FOREIGN KEY (coin_id) REFERENCES coins(coin_id)
);

-- Secrets table: stores coin secrets (decrypted note data)
CREATE TABLE IF NOT EXISTS coin_secrets (
    secret TEXT PRIMARY KEY NOT NULL,
    coin_id TEXT NOT NULL,
    value INTEGER NOT NULL,
    token_id TEXT NOT NULL,
    coin_blind TEXT NOT NULL,
    value_blind TEXT NOT NULL,
    token_blind TEXT NOT NULL,
    memo BLOB,
    FOREIGN KEY (coin_id) REFERENCES coins(coin_id)
);

CREATE INDEX IF NOT EXISTS idx_coin_secrets_token_id ON coin_secrets(token_id);

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
CREATE TABLE IF NOT EXISTS contract_registry (
    contract_name TEXT PRIMARY KEY NOT NULL,
    contract_id TEXT NOT NULL
);
