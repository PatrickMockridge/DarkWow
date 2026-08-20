# Wallet FFI — Language Bindings

The DarkWow wallet ships as a C shared library (`libdwow_wallet.so`). Every
language with an FFI gets the full wallet engine — key derivation, block
scanning, AEAD decryption, capability construction, and balance computation.

**Design principle**: write the protocol binding once in C, and every language
gets it through FFI. This pattern follows [seatuya](https://github.com/moebiusV/seatuya)
— opaque handles, caller-provided buffers, NULL-safe, no Rust types crossing
the ABI boundary.

## Quick Start

```c
#include "dwow_wallet.h"

int main(void) {
    WalletHandle *w = dwow_wallet_open("keys.toml", "wallet-1", "testnet");
    int n = dwow_wallet_scan_block_json(w, block_json);
    printf("scanned %d outputs, balance=%lu, caps=%d\n",
           n, dwow_wallet_balance(w), dwow_wallet_cap_count(w));
    dwow_wallet_free(w);
}
```

```bash
cc -o wallet example.c -L target/release -ldwow_wallet
```

## API Reference

All functions are declared in [`bin/dww/include/dwow_wallet.h`](../../../bin/dww/include/dwow_wallet.h).
Rust implementation in [`bin/dww/src/ffi.rs`](../../../bin/dww/src/ffi.rs).

### Lifecycle

| Function | Description |
|----------|-------------|
| `dwow_wallet_open(keys_path, section, network)` | Open wallet from keys.toml. Returns handle or NULL. |
| `dwow_wallet_free(handle)` | Close wallet and free all resources. NULL-safe. |
| `dwow_wallet_open_account(keys_path, section, network)` | Open AccountManager only (no DB). |
| `dwow_wallet_free_account(handle)` | Free AccountManager handle. |
| `dwow_wallet_version()` | Library version string (static, do not free). |

### Key Derivation

| Function | Description |
|----------|-------------|
| `dwow_wallet_derive_key(account, contract_id, height, out_secret)` | Derive per-block sk_H. Returns 0 on success. |

### Scan

| Function | Description |
|----------|-------------|
| `dwow_wallet_scan_block_json(handle, block_json)` | Scan a block (JSON). Returns output count. |

### Capabilities

| Function | Description |
|----------|-------------|
| `dwow_wallet_cap_count(handle)` | Total active held capabilities. |
| `dwow_wallet_get_cap(handle, index)` | Get capability by index. Free with `dwow_wallet_free_cap`. |
| `dwow_wallet_cap_value(handle)` | Value in base units. |
| `dwow_wallet_cap_height(handle)` | Block height at creation. |
| `dwow_wallet_cap_id(handle, buf, len)` | Capability ID (bs58 string). |
| `dwow_wallet_cap_contract_id(handle, buf, len)` | Contract ID (32 raw bytes). |
| `dwow_wallet_cap_commitment(handle, buf, len)` | Poseidon commitment (32 raw bytes). |
| `dwow_wallet_cap_asset_id(handle, buf, len)` | Token ID (32 raw bytes). |
| `dwow_wallet_cap_leaf_position(handle)` | Merkle tree position. |
| `dwow_wallet_cap_revoked(handle)` | 1 if spent, 0 if active. |
| `dwow_wallet_free_cap(handle)` | Free capability handle. |

### Balance and Diagnostics

| Function | Description |
|----------|-------------|
| `dwow_wallet_balance(handle)` | Sum of all unspent native token values. |
| `dwow_wallet_chain_height(handle)` | Local chain tip height. |
| `dwow_wallet_default_address(handle, buf, len)` | Wallet default address string. |
| `dwow_wallet_aead_self_test(handle)` | AEAD encrypt/decrypt roundtrip. 0 = pass. |
| `dwow_wallet_last_error(handle, buf, len)` | Last error message. Clears after read. |

### Read Path

| Function | Description |
|----------|-------------|
| `dwow_wallet_balance_by_asset(handle, buf, len)` | Per-asset balances as a JSON map. |
| `dwow_wallet_is_synced(handle)` | 1 if chain is caught up, 0 if syncing. |
| `dwow_wallet_manifest(handle, cid_str, buf, len)` | Contract manifest TOML by bs58 contract ID. |
| `dwow_wallet_cap_merkle_root(handle, buf)` | Native token commitment tree root (32 bytes). |
| `dwow_wallet_addresses(handle, buf, len)` | All derived addresses as a JSON array. |
| `dwow_wallet_aliases_by_asset(handle, buf, len)` | Asset alias map as a JSON object. |
| `dwow_wallet_get_cap_batch(handle, filter, buf, len)` | Batch capability IDs (fixes O(n²) per-index). |

### Write Path

| Function | Description |
|----------|-------------|
| `dwow_wallet_insert_block(handle, block_json)` | Feed synced block to chain store. |
| `dwow_wallet_get_block(handle, height, buf, len)` | Get block by height as JSON. |
| `dwow_wallet_mark_exercise(handle, nullifier_json)` | Mark capabilities exercised by nullifier. |
| `dwow_wallet_diagnostic(handle, buf, len)` | Wallet diagnostic report. |

### Type System

| Function | Description |
|----------|-------------|
| `dwow_primitive_count()` | How many primitive types exist (currently 10). |
| `dwow_primitive_name(index)` | Static name string of primitive at index. |
| `dwow_primitive_barbs(index, buf, len)` | Comma-separated barbs for this primitive. |
| `dwow_barb_count()` | How many barbs exist (currently 14). |
| `dwow_barb_name(index)` | Static name string of barb at index. |
| `dwow_wallet_construct(resource, action, p_csv, b_csv, buf, len)` | wallet_construct soundness gate. Returns 1 if primitives cover required barbs, 0 if not. |

The type system functions enable C/C++ developers to enumerate all capability
primitives and barbs, understand primitive→barb mappings, and validate
capability compositions through `wallet_construct` without writing Rust.

### ZK / Halo2

Planned (Phase 5). The Python FFI at `src/sdk/python/src/zkas.rs` provides the
proven pattern. Future C FFI functions will expose proof verification, circuit
discovery, and canonical crypto helpers (Pedersen, Poseidon, nullifier, Merkle
root, key derivation).

## Language Bindings

Bindings for 18 languages are maintained alongside the C header. Each is a
thin shim — ~50 lines importing the C functions and wrapping them in idiomatic
language conventions.

### Tier 1 — Immediate Ecosystem Impact

| Language | File | Mechanism | Use Case |
|----------|------|-----------|----------|
| **Python** | [`darkwow.py`](../../../bin/dww/bindings/darkwow.py) | `ctypes` (stdlib) | Trading bots, scripting, data pipelines |
| **Node.js** | [`darkwow.js`](../../../bin/dww/bindings/darkwow.js) | `ffi-napi` / `bun:ffi` | Web backends, Electron desktop wallets |
| **Kotlin** | [`DarkWow.kt`](../../../bin/dww/bindings/DarkWow.kt) | JNA | Android mobile wallets |

### Tier 2 — Strong Use Cases

| Language | File | Mechanism | Use Case |
|----------|------|-----------|----------|
| **Swift** | [`DarkWow.swift`](../../../bin/dww/bindings/DarkWow.swift) | `@_silgen_name` | iOS/macOS wallets |
| **Go** | [`darkwow.go`](../../../bin/dww/bindings/darkwow.go) | `cgo` | Infrastructure services, CLI tools |
| **Dart** | [`darkwow.dart`](../../../bin/dww/bindings/darkwow.dart) | `dart:ffi` | Flutter cross-platform mobile |

### Niche — Ecosystem Diversity

These bindings are included because a diverse ecosystem is a healthy one.
They are not headlined — just present, and they work.

| Language | File | Mechanism |
|----------|------|-----------|
| **C** (native) | [`example.c`](../../../bin/dww/bindings/niche/example.c) | Direct C ABI |
| **Zig** | [`darkwow.zig`](../../../bin/dww/bindings/niche/darkwow.zig) | `extern "c"` |
| **Odin** | [`darkwow.odin`](../../../bin/dww/bindings/niche/darkwow.odin) | `foreign import` |
| **Haskell** | [`darkwow.hs`](../../../bin/dww/bindings/niche/darkwow.hs) | `ForeignFunctionInterface` |
| **OCaml** | [`darkwow.ml`](../../../bin/dww/bindings/niche/darkwow.ml) | `ctypes.foreign` |
| **Common Lisp** | [`darkwow.lisp`](../../../bin/dww/bindings/niche/darkwow.lisp) | CFFI |
| **Racket** | [`darkwow.rkt`](../../../bin/dww/bindings/niche/darkwow.rkt) | `ffi/unsafe` |
| **Guile** | [`darkwow.scm`](../../../bin/dww/bindings/niche/darkwow.scm) | `system foreign` |
| **Janet** | [`darkwow.janet`](../../../bin/dww/bindings/niche/darkwow.janet) | `ffi` module |
| **Lua** | [`darkwow.lua`](../../../bin/dww/bindings/niche/darkwow.lua) | LuaJIT FFI |
| **NewLisp** | [`darkwow.lsp`](../../../bin/dww/bindings/niche/darkwow.lsp) | `import` |
| **Tcl** | [`darkwow.tcl`](../../../bin/dww/bindings/niche/darkwow.tcl) | FFI extension (scaffold) |

## Build

```bash
cargo build --release -p dwow_wallet
# produces target/release/libdwow_wallet.so
```

Set `DARKWOW_LIB` to override the library path in any binding:
```bash
export DARKWOW_LIB=/path/to/libdwow_wallet.so
```

## Architecture

The wallet is a pure function of `(AccountManager, ChainBlocks)`. The C FFI
wraps the existing Rust public API — no new Rust code beyond the FFI layer.

```
┌──────────────────────────────────────────────────┐
│  Python  │  Node  │  Kotlin  │  Swift  │  Go  │ ... │
└────┬──────┴───┬────┴────┬─────┴────┬────┴───┬──┘
     │          │         │          │        │
     └──────────┴─────────┴──────────┴────────┘
                       │
              libdwow_wallet.so
              (C ABI — 55 symbols)
                       │
              bin/dww/src/ffi.rs
                       │
         ┌─────────────┼─────────────┐
         │             │             │
    AccountManager  WalletDb   scan_block_linear
         │             │             │
         └─────────────┴─────────────┘
                  Dww struct
          (wallet pure function)
```

## Related Documentation

- [Wallet Architecture](../arch/wallet.md) — Type construction engine, scan path discipline
- [Type System](../arch/type-system.md) — Barbs, primitives, capability composition
- [Genesis](../arch/genesis.md) — Contract list, coinbase format
- [Testing Overview](testing/overview.md) — Pre-devnet ceiling, MoC boundaries
