// DarkWow Wallet — Odin FFI binding
//   import "darkwow"
//   w := darkwow.open("keys.toml", "wallet-1", "testnet")
//   defer darkwow.close(w)
//   n := darkwow.scan_block(w, block_json)
//   fmt.printf("scanned %d outputs, balance=%d\n", n, darkwow.balance(w))

package darkwow

import "core:c"

WalletHandle :: struct {}

foreign import darkwow "system:dwow_wallet"

@(default_calling_convention = "c")
foreign darkwow {
    dwow_wallet_version :: proc() -> cstring ---
    dwow_wallet_open :: proc(keys_path, section, network: cstring) -> ^WalletHandle ---
    dwow_wallet_free :: proc(handle: ^WalletHandle) ---
    dwow_wallet_scan_block_json :: proc(handle: ^WalletHandle, block_json: cstring) -> c.int ---
    dwow_wallet_cap_count :: proc(handle: ^WalletHandle) -> c.int ---
    dwow_wallet_balance :: proc(handle: ^WalletHandle) -> u64 ---
    dwow_wallet_chain_height :: proc(handle: ^WalletHandle) -> u64 ---
}

open :: proc(keys_path, section, network: cstring) -> ^WalletHandle {
    h := dwow_wallet_open(keys_path, section, network)
    if h == nil { panic("dwow_wallet_open failed") }
    return h
}

close :: dwow_wallet_free
scan_block :: dwow_wallet_scan_block_json
cap_count :: proc(h: ^WalletHandle) -> int { return int(dwow_wallet_cap_count(h)) }
balance :: dwow_wallet_balance
chain_height :: dwow_wallet_chain_height
version :: proc() -> string { return string(dwow_wallet_version()) }
