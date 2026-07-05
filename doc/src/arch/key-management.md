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

On startup, `AccountManager::open()` resolves keys in this order:

1. **Sled cache** — accounts previously persisted (restart path)
2. **`keys.toml` declaration** — operator-specified keys (single source of truth)
3. **Auto-generate** — random key for dev/testing (localnet only)
4. **Hard error** — non-localnet, no keys declared

```
sled cache → keys.toml → auto-generate (localnet) → error (production)
```

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
- The mining node selects its section via `NODE_NAME` env var (default `"node0"`).
- The wallet passes its section name directly as the `section_name` parameter to
  `AccountManager::open()`, bypassing env vars — `Some("wallet-N")` selects `[wallet-N]`.

### CRUD Operations

| Operation | RPC Method | CLI Command |
|-----------|-----------|-------------|
| List accounts | `accounts.list` | — |
| Import key (hex) | `accounts.import` | `wallet import-from-toml` |
| Generate random key | `accounts.generate` | `wallet keygen` |
| Set active mining key | `accounts.set_default` | — |
| Remove account | `accounts.remove` | — |
| Export secret (hex) | `accounts.export` | `wallet secrets` |
| Show balance | — | `wallet balance` |

- `accounts.generate` auto-sets the new key as default.
- `accounts.import` rejects duplicate keys with a clear error.
- `accounts.export` requires explicit confirmation.
- `accounts.remove` cannot remove the last remaining account.

### Persistence

`AccountManager` serializes to JSON in sled under the `"accounts"` tree,
key `"accounts_json"`. JSON is chosen for inspectability over binary formats.
Secrets are stored as hex in the JSON blob. Network is persisted alongside keys.

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

**Forwarding:** Set `FORWARD_DESTINATION` to redirect coinbase rewards to a
different address. The miner encrypts coinbase to the forwarding address's
public key. The wallet at that address must import the corresponding secret.

**Key rotation:** Call `accounts.generate` to create a new key (auto-set as
default), then `accounts.set_default` to switch back if needed. Old keys remain
in the account list for decrypting past coinbases. Persists across restarts.

## Wallet Key Flow

```
keys.toml → AccountManager::open(db, localnet, keys_toml, network, Some("wallet-N"))
  → secrets() → import_secrets() → SQLite capability_secrets table
    → scan_block_linear() reads secrets via get_secrets()
      → AEAD decrypt coinbase + contract call notes
        → insert CapRecord into wallet DB
          → compute_balance()
            → select_coins() → build_transfer() → broadcast
```

**On startup:** The wallet daemon runs `import-from-toml <name>` which calls
`AccountManager::open()` with `section_name: Some(name)`. This selects the
`[name]` section from `keys.toml` directly, independent of the `NODE_NAME`
env var. The resolved secrets are imported into the wallet's SQLite store
for scanning.

**Auto-scan:** A background task in the wallet daemon polls for new blocks and
calls `scan_blocks()` automatically. No manual `scan` command needed. The scan
engine loads secrets from the wallet's SQLite `capability_secrets` table and
attempts AEAD decryption of every coinbase and contract call note.

**Scanning:** `scan_block_linear()` iterates every block. For coinbase and
contract call data, it attempts AEAD decryption with each wallet secret.
Successful AEAD tag verification proves capability ownership — no
contract-specific code needed.

**Spending:** `select_coins()` picks unspent coins (largest-first). `build_transfer()`
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
  coinbase without forwarding.
- `FORWARD_DESTINATION` provides an alternative: miner encrypts to wallet's
  public key without sharing the secret.

## Security

| Property | Mechanism |
|----------|-----------|
| **Production: no auto-generate** | `open(db, false, None, network, None)` without keys.toml returns hard error |
| **Localnet gate** | Auto-generation only on `localnet=true` |
| **Idempotent import** | `INSERT OR IGNORE` in SQLite, duplicate detection in AccountManager |
| **No auto-keygen** | `default_address()` returns error if no keys exist |
| **CRUD complete** | Import, generate, remove, export, set-default all available |
| **Secrets confirmation** | `wallet secrets` requires explicit confirmation |
| **Cross-chain unlinkability** | BIP32 uses `"DarkWow seed"` not `"Bitcoin seed"` |
| **Double-spend prevention** | Nullifier dedup at mempool admission |
| **Network discrimination** | Address prefix differs by network (0x39 vs 0xaf) |

## Reference

- Rust: `crates/dwow-accounts/src/lib.rs` — AccountManager implementation (shared crate)
- Rust: `bin/dww/src/lib.rs` — Wallet `import_from_keys_toml()` → `AccountManager::open()`
- Rust: `bin/dwowd/src/lib.rs` — Mining node `AccountManager::open()` call
- Rust: `src/sdk/src/crypto/keypair.rs` — Key types and address encoding
- Python: `contrib/model/key_management.py` — Unified specification (13 tests)
- Python: `contrib/model/wallet_model.py` — Wallet model (AccountManager, scanning)
- Docker: `contrib/docker/darkwow-testnet/keys.toml` — Testnet key configuration
- Docker: `contrib/docker/darkwow-testnet/entrypoint.sh` — Mining node startup
- Docker: `contrib/docker/darkwow-testnet/entrypoint-wallet.sh` — Wallet startup
