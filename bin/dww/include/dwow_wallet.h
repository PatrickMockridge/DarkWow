/* DarkWow Wallet — C FFI header
 *
 * Link with -ldwow_wallet. Every wallet implementation — GUI, CLI,
 * mobile, embedded — calls these same functions. The type system
 * guarantees all wallets see identical scan results for identical
 * chain data.
 *
 * Build:
 *   cargo build --release -p dwow_wallet
 *   # produces target/release/libdwow_wallet.so
 *
 * Usage:
 *   #include "dwow_wallet.h"
 *   // link: -L target/release -ldwow_wallet
 */

#ifndef DWOW_WALLET_H
#define DWOW_WALLET_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Opaque handles ──────────────────────────────────────────── */

typedef struct AccountManagerHandle AccountManagerHandle;
typedef struct CapRecordHandle      CapRecordHandle;
typedef struct WalletHandle         WalletHandle;

/* ── Lifecycle ────────────────────────────────────────────────── */

/** Open a wallet identity from a keys.toml file.
 *
 *  @param keys_path  Path to keys.toml (e.g. "/run/config/keys.toml")
 *  @param section    TOML section name (e.g. "wallet-1")
 *  @param network    "testnet" or "mainnet"
 *  @return           Opaque handle, or NULL on error.
 *                    Free with dwow_wallet_free_account(). */
AccountManagerHandle* dwow_wallet_open_account(
    const char* keys_path,
    const char* section,
    const char* network);

/** Free an AccountManager handle. */
void dwow_wallet_free_account(AccountManagerHandle* handle);

/** Open a full wallet instance (AccountManager + WalletDb + scanner).
 *
 *  Initializes SQLite schema, loads lifecycle keys.
 *  No P2P, no network — pure scan engine.
 *
 *  @param keys_path  Path to keys.toml
 *  @param section    TOML section name
 *  @param network    "testnet" or "mainnet"
 *  @return           Opaque handle, or NULL on error.
 *                    Free with dwow_wallet_free(). */
WalletHandle* dwow_wallet_open(
    const char* keys_path,
    const char* section,
    const char* network);

/** Free a full wallet handle. */
void dwow_wallet_free(WalletHandle* handle);

/** Open a persistent wallet instance (on-disk DB with optional encryption).
 *
 *  @param keys_path   Path to keys.toml
 *  @param section     TOML section name
 *  @param network     "testnet" or "mainnet"
 *  @param db_path     Path to wallet.db (SQLite file)
 *  @param password    Password for encrypted DB ("" for none)
 *  @param production  Non-zero for production mode
 *  @return            Opaque handle, or NULL on error. */
WalletHandle* dwow_wallet_open_persistent(
    const char* keys_path,
    const char* section,
    const char* network,
    const char* db_path,
    const char* password,
    int32_t production);

/* ── Lifecycle (extended) ─────────────────────────────────────── */

/** Get the wallet version string.
 *
 *  @return Static C string (e.g. "0.5.0"). Never returns NULL. */
const char* dwow_wallet_version(void);

/** Get the last error message from a wallet handle.
 *
 *  @param handle  WalletHandle to query
 *  @param out_buf Caller-allocated buffer
 *  @param buf_len Buffer size in bytes
 *  @return        Error length (0 if no error), or -1 on invalid args. */
int32_t dwow_wallet_last_error(
    const WalletHandle* handle,
    char* out_buf,
    int32_t buf_len);

/* ── Key derivation ───────────────────────────────────────────── */

/** Derive the per-block address for a given contract and height.
 *
 *  Uses AccountManager::per_block_address — the sanctioned delegation
 *  path. NEVER exports raw secret bytes.
 *
 *  @param account       Open AccountManager handle
 *  @param contract_id   32 bytes, the contract's ID
 *  @param height        Block height (u32)
 *  @param out_address   Output buffer for the Testnet address C string
 *  @param out_len       Buffer size (must be >= 64)
 *  @return              Bytes written (including NUL), or 0 on error. */
int32_t dwow_wallet_derive_address(
    const AccountManagerHandle* account,
    const uint8_t* contract_id,
    uint32_t height,
    char* out_address,
    int32_t out_len);

/* ── Scan ──────────────────────────────────────────────────────── */

/** Scan a block through the full wallet pipeline (pure scan + persistence).
 *
 *  Uses Dww::scan_block_linear — Merkle tree checkpoint, manifest
 *  pre-loading, AEAD decryption, and capability persistence to the
 *  wallet DB. Every wallet implementation gets identical results.
 *
 *  @param handle     Open WalletHandle
 *  @param block_json NUL-terminated block JSON string
 *  @return           Number of native token outputs discovered (>= 0),
 *                    or -1 on error. */
int32_t dwow_wallet_scan_block_json(
    WalletHandle* handle,
    const char* block_json);

/* ── Capabilities ──────────────────────────────────────────────── */

/** Free a CapRecord handle. */
void dwow_wallet_free_cap(CapRecordHandle* handle);

/** Get the total held capability count from the wallet database.
 *
 *  @return Count (>= 0), or -1 on error. */
int32_t dwow_wallet_cap_count(const WalletHandle* handle);

/** Get a capability by index from the wallet database.
 *
 *  @return Opaque handle, or NULL if index out of bounds.
 *          Free with dwow_wallet_free_cap(). */
CapRecordHandle* dwow_wallet_get_cap(
    const WalletHandle* handle,
    int32_t index);

/** Get the value (in base units) of a capability. */
uint64_t dwow_wallet_cap_value(const CapRecordHandle* handle);

/** Get the block height at which this capability was created. */
uint32_t dwow_wallet_cap_height(const CapRecordHandle* handle);

/** Get the capability ID as a bs58 string.
 *
 *  @param out_buf  Caller-allocated buffer
 *  @param buf_len  Buffer size in bytes
 *  @return         Bytes written (excluding NUL), or -1 on error. */
int32_t dwow_wallet_cap_id(
    const CapRecordHandle* handle,
    char* out_buf,
    int32_t buf_len);

/** Get the contract ID for a capability (always 32 bytes).
 *
 *  @param out_buf  Caller-allocated buffer, must be >= 32 bytes
 *  @param buf_len  Buffer size in bytes
 *  @return         Bytes written (always 32), or -1 on error. */
int32_t dwow_wallet_cap_contract_id(
    const CapRecordHandle* handle,
    uint8_t* out_buf,
    int32_t buf_len);

/** Get the Poseidon commitment for a capability (always 32 bytes).
 *
 *  @param out_buf  Caller-allocated buffer, must be >= 32 bytes
 *  @param buf_len  Buffer size in bytes
 *  @return         Bytes written (always 32), or -1 on error. */
int32_t dwow_wallet_cap_commitment(
    const CapRecordHandle* handle,
    uint8_t* out_buf,
    int32_t buf_len);

/** Check if a capability has been revoked (spent).
 *
 *  @return 1 if revoked, 0 if active, -1 on error. */
int32_t dwow_wallet_cap_revoked(const CapRecordHandle* handle);

/** Get the block height at which this capability was revoked (0 if active). */
uint32_t dwow_wallet_cap_revoked_at_height(const CapRecordHandle* handle);

/** Get the manifest capability name (e.g. "coin", "credential").
 *  Returns bytes written (excluding NUL), or -1 on error. */
int32_t dwow_wallet_cap_name(
    const CapRecordHandle* handle, char* out_buf, int32_t buf_len);

/** Get the capability resource identity (ocap.md §3). */
int32_t dwow_wallet_cap_resource(
    const CapRecordHandle* handle, char* out_buf, int32_t buf_len);

/** Get the capability action identity (ocap.md §3). */
int32_t dwow_wallet_cap_action(
    const CapRecordHandle* handle, char* out_buf, int32_t buf_len);

/** Get the manifest capability discriminant (u8). */
uint8_t dwow_wallet_cap_discriminant(const CapRecordHandle* handle);

/** Get the composed primitives as a comma-separated string. */
int32_t dwow_wallet_cap_primitives(
    const CapRecordHandle* handle, char* out_buf, int32_t buf_len);

/** Get the covered barbs as a comma-separated string. */
int32_t dwow_wallet_cap_barbs(
    const CapRecordHandle* handle, char* out_buf, int32_t buf_len);

/** Get the spend hook FuncId (32 bytes, zeros if none). */
int32_t dwow_wallet_cap_spend_hook(
    const CapRecordHandle* handle, uint8_t* out_buf, int32_t buf_len);

/** Get the FuncId (32 bytes, zeros if none). */
int32_t dwow_wallet_cap_func_id(
    const CapRecordHandle* handle, uint8_t* out_buf, int32_t buf_len);

/** Get the Merkle inclusion proof as a JSON array of bs58 sibling strings. */
int32_t dwow_wallet_cap_merkle_proof(
    const CapRecordHandle* handle, char* out_buf, int32_t buf_len);

/** Get the asset ID (TokenId) for a capability (always 32 bytes).
 *
 *  @param out_buf  Caller-allocated buffer, must be >= 32 bytes
 *  @param buf_len  Buffer size in bytes
 *  @return         Bytes written (always 32), or -1 on error. */
int32_t dwow_wallet_cap_token_id(
    const CapRecordHandle* handle,
    uint8_t* out_buf,
    int32_t buf_len);

/** Get the leaf position for a capability in its Merkle tree.
 *
 *  @return Leaf position (u64), or 0 on error. */
uint64_t dwow_wallet_cap_leaf_position(const CapRecordHandle* handle);

/** Get the default address for this wallet.
 *
 *  @param handle   Open WalletHandle
 *  @param out_buf  Caller-allocated buffer
 *  @param buf_len  Buffer size in bytes
 *  @return         Bytes written (excluding NUL), or -1 on error. */
int32_t dwow_wallet_default_address(
    const WalletHandle* handle,
    char* out_buf,
    int32_t buf_len);

/** Get the current local chain height.
 *
 *  @return Block height, or 0 if uninitialized/error. */
uint64_t dwow_wallet_chain_height(const WalletHandle* handle);

/** Run a self-test of the AEAD encrypt/decrypt pipeline.
 *
 *  @return 1 if the pipeline is working, 0 on failure. */
int32_t dwow_wallet_aead_self_test(const WalletHandle* handle);

/* ── Balance ───────────────────────────────────────────────────── */

/** Get the sum of all unspent native token values.
 *
 *  @return Balance in satoshi-equivalent base units, or 0 on error. */
uint64_t dwow_wallet_balance(const WalletHandle* handle);

#ifdef __cplusplus
}
#endif

#endif /* DWOW_WALLET_H */
