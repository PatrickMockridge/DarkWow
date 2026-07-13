# DarkWow Wallet — Tcl FFI binding
#   source darkwow.tcl
#   set w [darkwow::open "keys.toml" "wallet-1" "testnet"]
#   puts "balance=[darkwow::balance $w]"
#   darkwow::close $w

namespace eval darkwow {
    variable lib [expr {$::env(DARKWOW_LIB) ne {} ? $::env(DARKWOW_LIB) : "libdwow_wallet.so"}]

    if {[catch {load $lib} err]} {
        # Tcl 8.x FFI via critcl/cffi; fallback: exec-based interface
        # For direct C FFI, use ffidl extension or Tcl 9's native FFI
    }

    proc open {keysPath section {network testnet}} {
        # Requires FFI extension (ffidl/critcl)
        # return [dwow_wallet_open $keysPath $section $network]
    }

    proc close {handle} {
        # dwow_wallet_free $handle
    }

    proc balance {handle} {
        # return [dwow_wallet_balance $handle]
    }
}
