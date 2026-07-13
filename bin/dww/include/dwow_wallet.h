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

/* ── Key derivation ───────────────────────────────────────────── */

/** Derive the per-block secret key sk_H for a given contract and height.
 *
 *  sk_H = derive_instance(sk_owner, contract_id, height.to_le_bytes())
 *  Same derivation as the mining node — deterministic, zero shared state.
 *
 *  @param account       Open AccountManager handle
 *  @param contract_id   32 bytes, the contract's ID
 *  @param height        Block height (u32, little-endian)
 *  @param out_secret    Output buffer, 32 bytes written on success
 *  @return              0 on success, -1 on error */
int32_t dwow_wallet_derive_key(
    const AccountManagerHandle* account,
    const uint8_t* contract_id,
    uint32_t height,
    uint8_t* out_secret);

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

/* ── Balance ───────────────────────────────────────────────────── */

/** Get the sum of all unspent native token values.
 *
 *  @return Balance in satoshi-equivalent base units, or 0 on error. */
uint64_t dwow_wallet_balance(const WalletHandle* handle);

#ifdef __cplusplus
}
#endif

#endif /* DWOW_WALLET_H */
