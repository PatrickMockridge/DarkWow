#lang racket/base

;; DarkWow Wallet — Racket FFI binding
;;   #lang racket
;;   (require "darkwow.rkt")
;;   (define w (dw-open "keys.toml" "wallet-1" "testnet"))
;;   (printf "balance=~a caps=~a~n" (dw-balance w) (dw-cap-count w))
;;   (dw-close w)

(require ffi/unsafe
         ffi/unsafe/define)

(define-ffi darkwow-lib
  (ffi-lib (or (getenv "DARKWOW_LIB") "libdwow_wallet")))

(define-ffi-fun (dw-version) darkwow-lib "dwow_wallet_version" (_fun -> _string))

(define-ffi-fun (dw-open kp sec net) darkwow-lib "dwow_wallet_open"
  (_fun _string _string _string -> _pointer))

(define-ffi-fun (dw-close h) darkwow-lib "dwow_wallet_free"
  (_fun _pointer -> _void))

(define-ffi-fun (dw-scan h json) darkwow-lib "dwow_wallet_scan_block_json"
  (_fun _pointer _string -> _int))

(define-ffi-fun (dw-cap-count h) darkwow-lib "dwow_wallet_cap_count"
  (_fun _pointer -> _int))

(define-ffi-fun (dw-balance h) darkwow-lib "dwow_wallet_balance"
  (_fun _pointer -> _uint64))

(define-ffi-fun (dw-chain-height h) darkwow-lib "dwow_wallet_chain_height"
  (_fun _pointer -> _uint64))

(provide dw-open dw-close dw-scan dw-cap-count dw-balance dw-chain-height dw-version)
