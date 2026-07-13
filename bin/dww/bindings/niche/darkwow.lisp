;; DarkWow Wallet — Common Lisp (SBCL) CFFI binding
;;   (ql:quickload :cffi)
;;   (load "darkwow.lisp")
;;   (let ((w (darkwow:open-wallet "keys.toml" "wallet-1" "testnet")))
;;     (format t "balance=~D caps=~D~%" (darkwow:balance w) (darkwow:cap-count w))
;;     (darkwow:close-wallet w))

(defpackage :darkwow
  (:use :cl)
  (:export :open-wallet :close-wallet :scan-block :cap-count :balance
           :chain-height :version))

(in-package :darkwow)

(cffi:define-foreign-library darkwow
  (:unix (or (uiop:getenv "DARKWOW_LIB") "libdwow_wallet.so"))
  (t (:default "libdwow_wallet")))

(cffi:use-foreign-library darkwow)

(cffi:defcfun ("dwow_wallet_version" version) :string)

(cffi:defcfun ("dwow_wallet_open" %open) :pointer
  (keys-path :string) (section :string) (network :string))

(cffi:defcfun ("dwow_wallet_free" %free) :void (handle :pointer))

(cffi:defcfun ("dwow_wallet_scan_block_json" %scan) :int
  (handle :pointer) (block-json :string))

(cffi:defcfun ("dwow_wallet_cap_count" %cap-count) :int (handle :pointer))

(cffi:defcfun ("dwow_wallet_balance" %balance) :uint64 (handle :pointer))

(cffi:defcfun ("dwow_wallet_chain_height" %chain-height) :uint64 (handle :pointer))

(defun open-wallet (keys-path section network)
  (let ((h (%open keys-path section network)))
    (if (cffi:null-pointer-p h)
        (error "dwow_wallet_open failed"))
    h))

(defun close-wallet (h) (%free h))
(defun scan-block (h json) (%scan h json))
(defun cap-count (h) (%cap-count h))
(defun balance (h) (%balance h))
(defun chain-height (h) (%chain-height h))
