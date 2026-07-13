(* DarkWow Wallet — OCaml ctypes binding
   (* Compile: ocamlfind opt -package ctypes.foreign -linkpkg darkwow.ml *)
   open DarkWow
   let () =
     let w = open_wallet "keys.toml" "wallet-1" "testnet" in
     let n = scan_block w block_json in
     Printf.printf "scanned %d outputs, balance=%Ld\n" n (balance w);
     close_wallet w
*)

open Ctypes
open Foreign

type wallet_handle = unit ptr
let wallet_handle : wallet_handle typ = ptr void

let version_fn = foreign "dwow_wallet_version" (void @-> returning string)

let open_fn = foreign "dwow_wallet_open"
    (string @-> string @-> string @-> returning wallet_handle)

let free_fn = foreign "dwow_wallet_free" (wallet_handle @-> returning void)

let scan_fn = foreign "dwow_wallet_scan_block_json"
    (wallet_handle @-> string @-> returning int)

let cap_count_fn = foreign "dwow_wallet_cap_count"
    (wallet_handle @-> returning int)

let balance_fn = foreign "dwow_wallet_balance"
    (wallet_handle @-> returning int64_t)

let chain_height_fn = foreign "dwow_wallet_chain_height"
    (wallet_handle @-> returning int64_t)

let open_wallet kp sec net = open_fn kp sec net
let close_wallet = free_fn
let scan_block h json = scan_fn h json
let cap_count h = cap_count_fn h
let balance h = Int64.to_int (balance_fn h)
let chain_height h = Int64.to_int (chain_height_fn h)
let version () = version_fn ()
