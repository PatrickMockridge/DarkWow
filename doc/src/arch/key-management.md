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
export) are exposed through the **`darkwow account` CLI** (`bin/darkwow/src/account.rs`),
which drives the AccountManager module API directly.

| Operation | Where |
|-----------|-------|
| Declare identity | `keys.toml` `[section].wallet_secret` (64-char hex) |
| Resolve identity | `AccountManager::open(keys_toml, network, section)` at boot |
| Show declared key | `accounts.show` RPC (dwowd, read-only) |
| Read secrets | `AccountManager::secrets()` |
| Generate key | `darkwow account generate` |
| Import key | `darkwow account import-hex` / `import-base58` |
| HD key derivation | `darkwow account from-seed` |
| Export / list | `darkwow account export` / `darkwow account list` |

### Persistence

Key material is **not persisted by the daemon or the wallet** — both resolve
the declared identity from `keys.toml` on every boot. The only persisted key
store is the `darkwow account` vault: `AccountManager::to_json_string()`
writes encrypted JSON to `~/.dwow/lifecycle.json` (overridable with
`--output <path>`). Secrets are **encrypted at rest** with ChaCha20Poly1305
using a key derived from the `DWOW_KEY_PASSPHRASE` env var (REQUIRED —
`crates/dwow-accounts/src/lib.rs:550`). `from_json()` reads both encrypted
(`encrypted_secret`) and plaintext (`secret_hex`) formats. Network is
persisted alongside keys.

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

Both hardened and non-hardened child derivation are implemented
(`bip32_derive`, `crates/dwow-accounts/src/lib.rs:1108`) — the path above
mixes hardened (`44'`, `0'`, `0'`) and non-hardened (`0`, `0`) steps.

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
keys.toml → AccountManager::open(keys_toml, network, section)
  → default_public_key()
    → coinbase encryption (AEAD-encrypted NativeToken note)
      → mined block
```

**On startup:** `AccountManager::open(keys_toml, network, section)` resolves the
mining key from `keys.toml`. `dwowd` hard-fails if `NODE_NAME` is unset
(`bin/dwowd/src/lib.rs:710`) — the env var names the section.

**Coinbase:** The miner builds a coinbase transaction with a `NativeToken` note
encrypted to `default_public_key()`. Only the holder of the corresponding
`SecretKey` can decrypt this note.

**Key rotation:** `darkwow account generate` creates a new lifecycle key but
**never repoints the default** — `accounts[0]` (the declared `keys.toml`
identity) is always the default, and `set_default` rejects any index ≠ 0
(`crates/dwow-accounts/src/lib.rs:355`). Old keys remain in the vault for
decrypting past coinbases.

## Wallet Key Flow

```
keys.toml → AccountManager::open(keys_toml, network, section)   // section = WALLET_NAME
  → secrets() at boot (derived identity — never stored)
    → scan_block_linear() trial-decrypts with the declared secrets
      → AEAD decrypt coinbase + contract call notes
        → insert CapRecord into wallet DB
          → compute_balance()
```

**On startup:** The wallet resolves its identity from `keys.toml` via
`AccountManager::open(keys_toml, network, section)` where `section` is the
`WALLET_NAME` env var (hard fail if unset, `bin/dww/src/main.rs:150`). The
keys.toml path comes from `--keys` / `KEYS_FILE` (`bin/dww/src/config.rs`).
There is no import step and no `addresses` table — the identity is derived at
boot and nothing key-related is persisted (`config.rs`: "the wallet derives
its identity from these on boot; nothing is persisted").

**Auto-scan:** In `daemon` mode, a background task polls for new blocks and
scans automatically. The scan engine uses the declared secrets from
`AccountManager` and attempts AEAD decryption of every coinbase and contract
call note.

**Scanning:** `scan_block_linear()` iterates every block. For coinbase and
contract call data, it attempts AEAD decryption with each wallet secret.
Successful AEAD tag verification proves capability ownership — no
contract-specific code needed.

**Spending:** Native DRKW transfers go through `build_native_transfer()`
(`bin/dww/src/lib.rs`); every other contract action goes through the generic
manifest path (`contract invoke`). Output notes are encrypted to the recipient
and the transaction broadcasts via P2P.

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
| **Production: no auto-generate** | `AccountManager::open(keys_toml, network, section)` hard-errors on a missing file or section — keys are NEVER auto-generated |
| **Encrypted at rest** | `to_json_string()` emits ChaCha20Poly1305-encrypted secrets, not plaintext hex; `DWOW_KEY_PASSPHRASE` env var is REQUIRED |
| **Seed retention** | `from_seed_phrase()` encrypts and stores the mnemonic for HD re-derivation |
| **No stored wallet keys** | Identity is derived at boot from `keys.toml`; nothing key-related is persisted by the wallet |
| **No silent failures** | Empty wallet returns zero balance, not random key auto-generation |
| **Duplicate detection** | Duplicate detection in AccountManager vault operations |
| **No auto-keygen** | `default_address()` returns error if no keys exist |
| **CRUD complete** | Generate, import (hex/base58/seed), export, list via `darkwow account` |
| **Cross-chain unlinkability** | BIP32 uses `"DarkWow seed"` not `"Bitcoin seed"` |
| **Double-spend prevention** | Nullifier dedup at mempool admission |
| **Network discrimination** | Address prefix differs by network (0x39 vs 0xaf) |

## Reference

- Rust: `crates/dwow-accounts/src/lib.rs` — AccountManager implementation (shared crate)
- Rust: `bin/dww/src/lib.rs` — Wallet resolves identity via `AccountManager::open()`
- Rust: `bin/dwowd/src/lib.rs` — Mining node `AccountManager::open()` call
- Rust: `src/sdk/src/crypto/keypair.rs` — Key types and address encoding
- Python: `contrib/model/key_management.py` — Unified specification (24 tests)
- Python: `contrib/model/wallet_model.py` — Wallet model (AccountManager, scanning, AEAD)
- Docker: `contrib/docker/darkwow-testnet/keys.toml` — Testnet key configuration
- Docker: `contrib/docker/darkwow-testnet/entrypoint.sh` — Mining node startup
- Docker: `contrib/docker/darkwow-testnet/entrypoint-wallet.sh` — Wallet startup
