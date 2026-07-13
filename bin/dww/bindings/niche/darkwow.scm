;; DarkWow Wallet — Guile FFI binding
;;   (load "darkwow.scm")
;;   (define w (dw-open "keys.toml" "wallet-1" "testnet"))
;;   (format #t "balance=~a caps=~a~%" (dw-balance w) (dw-cap-count w))
;;   (dw-close w)

(use-modules (system foreign))

(define darkwow-lib (dynamic-link
  (or (getenv "DARKWOW_LIB") "libdwow_wallet.so")))

(define dw-version
  (pointer->procedure '* (dynamic-func "dwow_wallet_version" darkwow-lib) '()))

(define dw-open
  (pointer->procedure '* (dynamic-func "dwow_wallet_open" darkwow-lib)
    (list '* '* '*)))

(define dw-close
  (pointer->procedure void (dynamic-func "dwow_wallet_free" darkwow-lib)
    (list '*)))

(define dw-scan
  (pointer->procedure int (dynamic-func "dwow_wallet_scan_block_json" darkwow-lib)
    (list '* '*)))

(define dw-cap-count
  (pointer->procedure int (dynamic-func "dwow_wallet_cap_count" darkwow-lib)
    (list '*)))

(define dw-balance
  (pointer->procedure uint64 (dynamic-func "dwow_wallet_balance" darkwow-lib)
    (list '*)))

(define dw-chain-height
  (pointer->procedure uint64 (dynamic-func "dwow_wallet_chain_height" darkwow-lib)
    (list '*)))

(define (dw-open* keys-path section network)
  (let ((h (dw-open (string->pointer keys-path)
                     (string->pointer section)
                     (string->pointer network))))
    (if (eq? h %null-pointer) (error "dwow_wallet_open failed"))
    h))
