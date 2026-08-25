# Key & Account Management

DarkWow keys are Pallas curve field elements. Mining nodes and wallets use the
same `AccountManager`, same derivation, same address format, same secret
management. The only difference: miners **receive** coinbase rewards (encrypted
to their public key), wallets **spend** them (scan for capabilities, build
transfers).

## Key Types

| Type | Size | Description |
|------|------|-------------|
| `SecretKey` | 32 bytes | Pallas Base field element. Canonical form via `from_uniform_bytes` reduction. |
| `PublicKey` | 32 bytes | Compressed Pallas point: `NullifierK.generator() * scalar(secret)`. |
| `Keypair` | — | Holds both `SecretKey` and derived `PublicKey`. |
| `Address` | ~50 chars | `[prefix_byte \| pubkey \| blake3_checksum[..4]]` as plain bs58. |
| `Network` | 1 byte | `Mainnet = 0x39`, `Testnet = 0xaf`. |

### Derivation Chain

```
SecretKey (32 bytes, canonical Pallas Base)
    → NullifierK.generator() * scalar_mod_q(secret)
    → PublicKey (32 bytes, compressed Pallas point)
    → StandardAddress::from_public(network, public)
    → [prefix | pubkey | blake3(prefix + pubkey)[..4]]
    → bs58 encode → Address string
```

## Account Manager

`AccountManager` (`crates/dwow-accounts/src/lib.rs`) is the unified key store. Both
`dwowd` (mining node) and `dwow_wallet` (wallet daemon) use it through the same
`AccountManager::open()` entry point. The crate is shared — both binaries depend on
`dwow-accounts` directly, not through copy-paste or re-export.

### Resolution Order

On startup, `AccountManager::open(path, network, section)` resolves the owner's
declared key deterministically:

1. **Read `keys.toml`** — the operator's declared `wallet_secret` in `[section]`.
2. **Derive the keypair** — via `SecretKey::from_bytes` → `Keypair::new`.
3. **Return a single-key manager.**
4. **Hard error if the file or section is missing** — keys are NEVER auto-generated.

```
keys.toml [section] → deterministic derive → single identity (hard error on missing)
```

- NO sled cache, NO `localnet` auto-generation, NO random/`Default` identity.
- The owner declares their key; the software only uses it.
- `section` is REQUIRED — no `NODE_NAME` default (dwowd requires `NODE_NAME`; wallet
  requires `WALLET_NAME`, both fail hard if unset).

### keys.toml Format

```toml
[node0]
wallet_secret = "0000000000000000000000000000000000000000000000000000000000000001"

[node1]
wallet_secret = "0000000000000000000000000000000000000000000000000000000000000002"

[wallet-1]
wallet_secret = "0000000000000000000000000000000000000000000000000000000000000001"
```

- Each section name matches a `NODE_NAME` or `WALLET_NAME` env var.
- `wallet_secret` is a 64-character hex string (32 bytes, no `0x` prefix).
- The mining node resolves its section from the `NODE_NAME` env var (REQUIRED).
- The wallet resolves its section from the `WALLET_NAME` env var (REQUIRED).
- Both binaries call the same `AccountManager::open(path, network, section)`.

### CRUD Operations

The declared identity is read-only at runtime — it comes from `keys.toml` and is
never mutated by the daemon. Key lifecycle operations (generate, import, HD derive,
persist) exist as **AccountManager module capabilities** (in the `dwow-accounts` crate),
NOT as wallet/miner CLI or RPC commands. The module API is the single source; both
binaries use it directly.

| Operation | Where |
|-----------|-------|
| Declare identity | `keys.toml` `[section].wallet_secret` (64-char hex) |
| Resolve identity | `AccountManager::open(path, network, section)` at boot |
| Show declared key | `accounts.show` RPC (dwowd, read-only) |
| Read secrets | `AccountManager::secrets()` |
| Generate key | `AccountManager::generate()` (module API; owner-initiated) |
| Import key | `AccountManager::import_hex()` / `import_base58()` (module API) |
| HD key derivation | `AccountManager::from_seed_phrase()` (module API) |

### Persistence

`AccountManager` serializes to JSON in sled under the `"accounts"` tree
(mining nodes) or SQLite `addresses` table (wallets). Keys are **encrypted
at rest** using ChaCha20Poly1305 with a key derived from the passphrase
(default: `DWOW_KEY_PASSPHRASE` env var, or devnet passphrase
`darkwow-devnet-key-encryption-v1`). Backward-compatible: `from_json()`
reads both encrypted (`encrypted_secret`) and old plaintext (`secret_hex`)
formats. Network is persisted alongside keys.

## Seed Phrases (BIP39 + BIP32)

DarkWow supports BIP39 mnemonic phrases and BIP32 hardened derivation.

### BIP39: Mnemonic → Seed

```
12/24 words → PBKDF2-HMAC-SHA512(password=mnemonic, salt="mnemonic"+passphrase, iterations=2048)
→ 64-byte seed
```

### BIP32: Seed → Master Key

```
HMAC-SHA512(key="DarkWow seed", data=seed)
→ (master_secret[..32], chain_code[32..])
```

DarkWow uses `"DarkWow seed"` as the HMAC key — **not** Bitcoin's `"Bitcoin seed"`.
This ensures the same mnemonic produces **different** keys on DarkWow vs Bitcoin,
preventing cross-chain key linkage.

### Derivation Path

```
m / 44' / 0' / 0' / 0 / 0
  │     │    │    │   └─ address index
  │     │    │    └───── external chain (0 = receiving)
  │     │    └────────── account
  │     └─────────────── coin_type (0 for DarkWow)
  └───────────────────── purpose (44 = BIP44)
```

Only hardened derivation (with `'` suffix) is currently implemented.

### Seed Retention

`from_seed_phrase()` **encrypts and retains** the mnemonic phrase. The encrypted
seed is stored alongside derived accounts and persists across restarts.
`derive_account(path)` can later derive additional HD accounts from the
stored seed without re-entering the phrase. This matches the production
baseline: all four reference chains (Bitcoin, Ethereum, Monero, ZCash)
retain the seed for multi-account derivation and recovery.

### SecretKey Conversion

BIP32 produces 32 arbitrary bytes. To convert to a valid Pallas field element:

```
derived_bytes → pad to 64 bytes → pallas::Base::from_uniform_bytes(&wide)
→ canonical to_repr() → SecretKey::from_bytes(canonical)
```

This is deterministic, always produces a valid key.

## Miner Key Flow

```
keys.toml → AccountManager::open(db, localnet, keys_toml, network, None)
  → default_public_key()
    → coinbase encryption (AEAD-encrypted NativeToken note)
      → mined block
```

**On startup:** `AccountManager::open()` resolves the mining key from `keys.toml`
(or auto-generates on localnet). The mining node passes `None` for `section_name`,
so `NODE_NAME` env var selects the section (default `"node0"`).

**Coinbase:** The miner builds a coinbase transaction with a `NativeToken` note
encrypted to `default_public_key()`. Only the holder of the corresponding
`SecretKey` can decrypt this note.

**Key rotation:** Call `accounts.generate` to create a new key (auto-set as
default), then `accounts.set_default` to switch back if needed. Old keys remain
in the account list for decrypting past coinbases. Persists across restarts.

## Wallet Key Flow

```
keys.toml → AccountManager::open(cached_json, localnet, keys_toml, network, Some("wallet-N"))
  → secrets() → import_secrets_batch() → SQLite addresses table
    → scan_block_linear() reads secrets via get_secrets() (same table as default_address())
      → AEAD decrypt coinbase + contract call notes
        → insert CapRecord into wallet DB
          → compute_balance()
            → select_commitments() → build_transfer() → broadcast
```

**On startup:** The wallet daemon runs `import-from-toml <name>` which calls
`AccountManager::open()` with `section_name: Some(name)`. This selects the
`[name]` section from `keys.toml` directly, independent of the `NODE_NAME`
env var. The resolved secrets are imported into the wallet's SQLite `addresses`
table — the single key store used by both `get_secrets()` (for scanning) and
`default_address()` (for display). No dual-store anti-pattern.

**Auto-scan:** A background task in the wallet daemon polls for new blocks and
calls `scan_blocks()` automatically. No manual `scan` command needed. The scan
engine loads secrets from the wallet's SQLite `addresses` table and attempts
AEAD decryption of every coinbase and contract call note.

**Scanning:** `scan_block_linear()` iterates every block. For coinbase and
contract call data, it attempts AEAD decryption with each wallet secret.
Successful AEAD tag verification proves capability ownership — no
contract-specific code needed.

**Spending:** `select_commitments()` picks unspent commitments (largest-first). `build_transfer()`
constructs ZK proofs, encrypts the output note to the recipient, pays the fee,
and broadcasts via P2P.

**Per-instance keys:** `SecretKey::derive_instance(secret, contract_id, instance_id)`
produces a unique key for each contract instance. This prevents cross-contract
identity linking while maintaining spend authority.

## Key Sharing (Testnet)

For deterministic testing, miners and wallets share keys from a single `keys.toml`:

```toml
[node0]
wallet_secret = "0000...0001"    # miner's key

[wallet-1]
wallet_secret = "0000...0001"    # same key → wallet can decrypt miner's coinbase
```

- `wallet-1` shares `node0`'s key → wallet can directly decrypt the miner's
  coinbase. This is the intended devnet pattern: a wallet needs coinbase
  funds for transaction fees, and sharing a miner's key is how it gets them.
  For production, miners and wallets use separate keys (matching the
  Bitcoin/Ethereum/Monero/ZCash baseline).

## Security

| Property | Mechanism |
|----------|-----------|
| **Production: no auto-generate** | `open(db, false, None, network, None)` without keys.toml returns hard error |
| **Localnet gate** | Auto-generation only on `localnet=true` |
| **Encrypted at rest** | `to_json()` emits ChaCha20Poly1305-encrypted secrets, not plaintext hex |
| **Seed retention** | `from_seed_phrase()` encrypts and stores mnemonic for HD re-derivation |
| **Single key store** | `get_secrets()` and `default_address()` read from same SQLite table |
| **No silent failures** | Empty wallet returns zero balance, not random key auto-generation |
| **Idempotent import** | `INSERT OR IGNORE` in SQLite, duplicate detection in AccountManager |
| **No auto-keygen** | `default_address()` returns error if no keys exist |
| **CRUD complete** | Import, generate, remove, export, set-default all available |
| **Cross-chain unlinkability** | BIP32 uses `"DarkWow seed"` not `"Bitcoin seed"` |
| **Double-spend prevention** | Nullifier dedup at mempool admission |
| **Network discrimination** | Address prefix differs by network (0x39 vs 0xaf) |

## Reference

- Rust: `crates/dwow-accounts/src/lib.rs` — AccountManager implementation (shared crate)
- Rust: `bin/dww/src/lib.rs` — Wallet `import_from_keys_toml()` → `AccountManager::open()`
- Rust: `bin/dwowd/src/lib.rs` — Mining node `AccountManager::open()` call
- Rust: `src/sdk/src/crypto/keypair.rs` — Key types and address encoding
- Python: `contrib/model/key_management.py` — Unified specification (24 tests)
- Python: `contrib/model/wallet_model.py` — Wallet model (AccountManager, scanning, AEAD)
- Docker: `contrib/docker/darkwow-testnet/keys.toml` — Testnet key configuration
- Docker: `contrib/docker/darkwow-testnet/entrypoint.sh` — Mining node startup
- Docker: `contrib/docker/darkwow-testnet/entrypoint-wallet.sh` — Wallet startup
